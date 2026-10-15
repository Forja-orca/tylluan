// Minimal OAuth 2.0 + PKCE (RFC 7636) server for local desktop MCP clients.
// Desktop apps (local MCP clients) require OAuth as a protocol ritual even
// when connecting to localhost.
//
// ADR-019/OAuth Fase 1 (2026-10-15, docs/internal/DESIGN_oauth_operator_2026-10.md):
// the authorization endpoint auto-approves ONLY when dev_mode=true (local
// development). With dev_mode=false it refuses (403) until the operator-consent
// flow of Fase 2 exists — this closes the LAN "JWT dispenser" hole where any
// LAN host could self-issue an admin-role token. redirect_uri is restricted to
// loopback. PKCE + JWT validation always run.

use axum::{
    Form, Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tracing::info;

type HmacSha256 = Hmac<Sha256>;

// ─── State ────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct OAuthState {
    inner: Arc<OAuthInner>,
    pub base_url: String,
    /// Fase 1-a: gate for the auto-approving /oauth/authorize. Fail-closed
    /// default (`false`): callers that want dev behavior opt in explicitly
    /// via `with_dev_mode(true)`.
    dev_mode: bool,
}

struct OAuthInner {
    secret: Vec<u8>,
    codes: Mutex<HashMap<String, PendingCode>>,
    revoked: Mutex<HashSet<String>>,
}

struct PendingCode {
    code_challenge: String,
    redirect_uri: String,
    expires_at: i64,
}

impl OAuthState {
    pub fn new(base_url: String) -> Self {
        let secret: Vec<u8> = rand::random::<[u8; 32]>().to_vec();
        Self {
            base_url,
            inner: Arc::new(OAuthInner {
                secret,
                codes: Mutex::new(HashMap::new()),
                revoked: Mutex::new(HashSet::new()),
            }),
            dev_mode: false,
        }
    }

    /// Fase 1-a: opt in to the auto-approving authorize flow (dev_mode=true).
    pub fn with_dev_mode(mut self, dev_mode: bool) -> Self {
        self.dev_mode = dev_mode;
        self
    }

    pub fn dev_mode(&self) -> bool {
        self.dev_mode
    }

    pub fn sign_jwt(&self, jti: &str, ttl_secs: i64) -> String {
        let now = chrono::Utc::now().timestamp();
        let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"HS256","typ":"JWT"}"#);
        let claims = format!(
            r#"{{"sub":"mcp","iat":{},"exp":{},"jti":"{}"}}"#,
            now,
            now + ttl_secs,
            jti
        );
        let payload = URL_SAFE_NO_PAD.encode(claims.as_bytes());
        let signing_input = format!("{header}.{payload}");
        let mut mac = HmacSha256::new_from_slice(&self.inner.secret).expect("valid HMAC key");
        mac.update(signing_input.as_bytes());
        let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        format!("{signing_input}.{sig}")
    }

    pub fn validate_bearer(&self, token: &str) -> bool {
        let parts: Vec<&str> = token.splitn(3, '.').collect();
        if parts.len() != 3 {
            return false;
        }
        let signing_input = format!("{}.{}", parts[0], parts[1]);
        let mut mac = HmacSha256::new_from_slice(&self.inner.secret).expect("valid HMAC key");
        mac.update(signing_input.as_bytes());
        let expected_sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        if expected_sig != parts[2] {
            return false;
        }
        if let Ok(payload_bytes) = URL_SAFE_NO_PAD.decode(parts[1])
            && let Ok(s) = std::str::from_utf8(&payload_bytes)
                && let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
                    let exp = v.get("exp").and_then(|e| e.as_i64()).unwrap_or(0);
                    if chrono::Utc::now().timestamp() > exp {
                        return false;
                    }
                    let jti = v.get("jti").and_then(|j| j.as_str()).unwrap_or("");
                    if let Ok(revoked) = self.inner.revoked.lock()
                        && revoked.contains(jti) {
                            return false;
                        }
                    return true;
                }
        false
    }
}

// ─── Request / Response types ─────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct AuthorizeQuery {
    pub redirect_uri: String,
    pub code_challenge: String,
    pub code_challenge_method: Option<String>,
    pub state: Option<String>,
    // response_type, client_id accepted but not validated (public client)
}

#[derive(Deserialize)]
pub struct TokenForm {
    pub grant_type: String,
    pub code: String,
    pub redirect_uri: String,
    pub code_verifier: String,
}

#[derive(Deserialize)]
pub struct RevokeForm {
    pub token: String,
}

#[derive(Serialize)]
struct TokenResponse {
    access_token: String,
    token_type: &'static str,
    expires_in: i64,
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// GET /.well-known/oauth-authorization-server
pub async fn metadata_handler(State(oauth): State<Arc<OAuthState>>) -> impl IntoResponse {
    let base = &oauth.base_url;
    Json(serde_json::json!({
        "issuer": base,
        "authorization_endpoint": format!("{}/oauth/authorize", base),
        "token_endpoint": format!("{}/oauth/token", base),
        "revocation_endpoint": format!("{}/oauth/revoke", base),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"]
    }))
}

/// Fase 1-c: loopback-only redirect_uri (RFC 8252). Deliberately a strict
/// host parse — `http://127.0.0.1.evil.com` must NOT pass.
fn is_loopback_redirect_uri(uri: &str) -> bool {
    let rest = match uri.strip_prefix("http://").or_else(|| uri.strip_prefix("https://")) {
        Some(r) => r,
        None => return false,
    };
    let host_port = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = if let Some(after_bracket) = host_port.strip_prefix('[') {
        after_bracket.split(']').next().unwrap_or("")
    } else {
        host_port.split(':').next().unwrap_or("")
    };
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

/// GET /oauth/authorize  — auto-approves (dev_mode only), redirects with auth code
pub async fn authorize_handler(
    State(oauth): State<Arc<OAuthState>>,
    Query(params): Query<AuthorizeQuery>,
) -> Response {
    // Fase 1-a: auto-approval is dev_mode-only. With dev_mode=false this is
    // refused until the operator-consent flow (Fase 2) exists.
    if !oauth.dev_mode {
        info!("[OAuth] Authorize refused: dev_mode=false and operator consent (Fase 2) is not implemented");
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "access_denied",
                "error_description": "OAuth auto-approval is disabled outside dev_mode. \
                    Operator consent lands in Fase 2; meanwhile use a static Bearer token \
                    (Authorization header)."
            })),
        )
            .into_response();
    }

    // Fase 1-c: loopback-only redirect_uri (defense in depth).
    if !is_loopback_redirect_uri(&params.redirect_uri) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_request",
                "error_description": "redirect_uri must use a loopback host (127.0.0.1, localhost or ::1)"
            })),
        )
            .into_response();
    }

    // Only S256 supported
    if params.code_challenge_method.as_deref().unwrap_or("S256") != "S256" {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"invalid_request","error_description":"only S256 supported"})),
        )
            .into_response();
    }

    info!("[OAuth] Authorize request: client_id={}, redirect_uri={}, state={:?}", 
        params.redirect_uri, // temporary placeholder for logging
        params.redirect_uri, 
        params.state
    );

    let code = URL_SAFE_NO_PAD.encode(rand::random::<[u8; 16]>());

    {
        let mut codes = oauth.inner.codes.lock().unwrap_or_else(|e| e.into_inner());
        codes.insert(
            code.clone(),
            PendingCode {
                code_challenge: params.code_challenge.clone(),
                redirect_uri: params.redirect_uri.clone(),
                expires_at: chrono::Utc::now().timestamp() + 60,
            },
        );
    }

    let state_part = params
        .state
        .as_deref()
        .map(|s| format!("&state={}", urlencoding::encode(s)))
        .unwrap_or_default();

    let separator = if params.redirect_uri.contains('?') { "&" } else { "?" };
    let location = format!(
        "{}{}code={}{}",
        params.redirect_uri,
        separator,
        urlencoding::encode(&code),
        state_part
    );

    info!("[OAuth] Authorize OK -> {}", params.redirect_uri);
    Redirect::temporary(&location).into_response()
}

/// POST /oauth/token  — validates PKCE, returns signed JWT
pub async fn token_handler(
    State(oauth): State<Arc<OAuthState>>,
    Form(form): Form<TokenForm>,
) -> Response {
    if form.grant_type != "authorization_code" {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"unsupported_grant_type"})),
        )
            .into_response();
    }

    let pending = {
        let mut codes = oauth.inner.codes.lock().unwrap_or_else(|e| e.into_inner());
        codes.remove(&form.code)
    };

    let Some(pending) = pending else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"invalid_grant","error_description":"unknown or expired code"})),
        )
            .into_response();
    };

    if chrono::Utc::now().timestamp() > pending.expires_at {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"invalid_grant","error_description":"code expired"})),
        )
            .into_response();
    }

    if pending.redirect_uri != form.redirect_uri {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"invalid_grant","error_description":"redirect_uri mismatch"})),
        )
            .into_response();
    }

    // Verify PKCE S256: SHA256(verifier) must equal stored challenge
    let mut hasher = Sha256::new();
    hasher.update(form.code_verifier.as_bytes());
    let computed = URL_SAFE_NO_PAD.encode(hasher.finalize());
    if computed != pending.code_challenge {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"invalid_grant","error_description":"PKCE verification failed"})),
        )
            .into_response();
    }

    let jti = URL_SAFE_NO_PAD.encode(rand::random::<[u8; 16]>());
    let ttl = 3600_i64;
    let access_token = oauth.sign_jwt(&jti, ttl);

    info!("[OAuth] Token issued (jti prefix: {})", &jti[..8]);
    Json(TokenResponse {
        access_token,
        token_type: "bearer",
        expires_in: ttl,
    })
    .into_response()
}

/// POST /oauth/revoke  — adds jti to revoked set
pub async fn revoke_handler(
    State(oauth): State<Arc<OAuthState>>,
    Form(form): Form<RevokeForm>,
) -> impl IntoResponse {
    let parts: Vec<&str> = form.token.splitn(3, '.').collect();
    if parts.len() == 3
        && let Ok(bytes) = URL_SAFE_NO_PAD.decode(parts[1])
            && let Ok(s) = std::str::from_utf8(&bytes)
                && let Ok(v) = serde_json::from_str::<serde_json::Value>(s)
                    && let Some(jti) = v.get("jti").and_then(|j| j.as_str())
                        && let Ok(mut revoked) = oauth.inner.revoked.lock() {
                            revoked.insert(jti.to_string());
                            info!("[OAuth] Token revoked (jti prefix: {})", &jti[..8.min(jti.len())]);
                        }
    StatusCode::OK
}

#[cfg(test)]
mod fase1_tests {
    use super::*;
    use axum::extract::{Query, State};

    fn q(redirect: &str) -> AuthorizeQuery {
        AuthorizeQuery {
            redirect_uri: redirect.to_string(),
            code_challenge: "abc".to_string(),
            code_challenge_method: Some("S256".to_string()),
            state: None,
        }
    }

    /// R13: with dev_mode=false the authorize endpoint must REFUSE (Fase 1-a).
    #[tokio::test]
    async fn r13_authorize_refuses_when_dev_mode_false() {
        let st = Arc::new(OAuthState::new("http://localhost:3030".to_string()));
        assert!(!st.dev_mode(), "fail-closed default");
        let resp = authorize_handler(State(st), Query(q("http://127.0.0.1:9999/cb"))).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    /// R15: non-loopback redirect_uri is rejected even with dev_mode=true.
    #[tokio::test]
    async fn r15_redirect_must_be_loopback() {
        let st = Arc::new(OAuthState::new("http://localhost:3030".to_string()).with_dev_mode(true));
        let bad = authorize_handler(State(st.clone()), Query(q("http://evil.example/cb"))).await;
        assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
        let sneaky = authorize_handler(State(st), Query(q("http://127.0.0.1.evil.example/cb"))).await;
        assert_eq!(sneaky.status(), StatusCode::BAD_REQUEST, "prefix lookalike must not pass");
    }

    /// R12: dev_mode=true keeps the local auto-approval flow intact (302-ish redirect with code).
    #[tokio::test]
    async fn r12_dev_mode_approves_loopback_and_redirects() {
        let st = Arc::new(OAuthState::new("http://localhost:3030".to_string()).with_dev_mode(true));
        let resp = authorize_handler(State(st), Query(q("http://127.0.0.1:9999/cb"))).await;
        assert!(resp.status().is_redirection(), "got {}", resp.status());
        let loc = resp
            .headers()
            .get(axum::http::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(loc.contains("code="), "location: {loc}");
    }

    #[test]
    fn loopback_helper_boundaries() {
        assert!(is_loopback_redirect_uri("http://127.0.0.1:8080/cb"));
        assert!(is_loopback_redirect_uri("http://localhost/cb"));
        assert!(is_loopback_redirect_uri("http://[::1]:4711/cb"));
        assert!(!is_loopback_redirect_uri("http://127.0.0.1.evil.com/cb"));
        assert!(!is_loopback_redirect_uri("https://example.com/cb"));
        assert!(!is_loopback_redirect_uri("ftp://127.0.0.1/cb"));
    }
}
