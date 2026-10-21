// Minimal OAuth 2.0 + PKCE (RFC 7636) server for local desktop MCP clients.
// Desktop apps (local MCP clients) require OAuth as a protocol ritual even
// when connecting to localhost.
//
// ADR-019/OAuth Fase 1 (2026-10-15) + Fase 2 (D-15, 2026-10-21),
// docs/internal/DESIGN_oauth_operator_2026-10.md y
// docs/internal/PLAN_oauth_f2_consentimiento_deep_2026-10.md:
//   * dev_mode=true  -> auto-aprobacion (R12, desarrollo local, sin cambios).
//   * dev_mode=false -> consentimiento del operador: P1 (pagina HTML
//     autocontenida, `oauth_consent.rs`) + credencial tecleada por el humano
//     + `consents.json` (30 dias deslizantes, revocable editando el fichero).
//     Sin credencial correcta no se emite ningun code (invariante T1).
// redirect_uri is restricted to loopback. PKCE + JWT validation always run.

use axum::{
    Form, Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header::HOST},
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

/// Fase 2 (D-3): scopes mínimos (DESIGN §68). Un JWT de cliente MCP lleva
/// `scope="mcp"`; `operator` es la clase que solo podrá conceder el
/// consentimiento del operador (pendiente: `PLAN_oauth_f2_consentimiento_*.md`).
pub const SCOPE_MCP: &str = "mcp";
pub const SCOPE_OPERATOR: &str = "operator";

// ─── State ────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct OAuthState {
    inner: Arc<OAuthInner>,
    pub base_url: String,
    /// Fase 1-a: gate for the auto-approving /oauth/authorize. Fail-closed
    /// default (`false`): callers that want dev behavior opt in explicitly
    /// via `with_dev_mode(true)`.
    dev_mode: bool,
    /// Fase 2 (D-15): credencial del operador (el token maestro). `None` = el
    /// kernel no tiene credencial configurada, así que el consentimiento no
    /// puede aprobarse (fail-closed) y la página no se muestra aprobable.
    operator_credential: Option<String>,
    /// Fase 2 (D-15): consentimientos recordados + peticiones vivas.
    consents: Arc<super::oauth_consent::ConsentStore>,
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
    /// Fase 2 (D-2, RFC 8707): recurso declarado por el cliente en
    /// `/oauth/authorize`, si lo hubo. Se vuelve a validar en el canje.
    resource: Option<String>,
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
            operator_credential: None,
            consents: Arc::new(super::oauth_consent::ConsentStore::in_memory()),
        }
    }

    /// Fase 1-a: opt in to the auto-approving authorize flow (dev_mode=true).
    pub fn with_dev_mode(mut self, dev_mode: bool) -> Self {
        self.dev_mode = dev_mode;
        self
    }

    /// Fase 2 (D-15): credencial que el humano teclea en la página de
    /// consentimiento. Es el token maestro; NUNCA se guarda (ni aquí ni en
    /// `consents.json`), solo se compara en memoria.
    pub fn with_operator_credential(mut self, credential: Option<String>) -> Self {
        self.operator_credential = credential.filter(|c| !c.is_empty());
        self
    }

    /// Fase 2 (D-15): persiste los consentimientos en `dir/consents.json`
    /// (30 días deslizantes, revocables editando el fichero).
    pub fn with_consent_dir(mut self, dir: &std::path::Path) -> Self {
        self.consents = Arc::new(super::oauth_consent::ConsentStore::with_file(dir));
        self
    }

    /// Fase 2 (D-15): store de consentimientos (lo usan los handlers y los
    /// tests de la suite adversarial para preparar estado).
    pub fn consents(&self) -> &super::oauth_consent::ConsentStore {
        &self.consents
    }

    /// Usa un secreto de firma de JWT que sobrevive a los reinicios.
    ///
    /// Sin esto el secreto es aleatorio en cada arranque y cada reinicio
    /// invalida los tokens OAuth que los clientes tienen guardados (se ven
    /// obligados a reautenticarse; algunos no lo consiguen). Lee o crea
    /// `oauth_jwt_secret` en `dir`. Nunca hace fallar el arranque: si el
    /// fichero no se puede leer ni escribir se queda con el secreto aleatorio
    /// del proceso. No afecta al token maestro (credencial aparte), así que
    /// un fichero corrupto o borrado no deja a nadie sin acceso.
    pub fn with_persisted_secret(mut self, dir: &std::path::Path) -> Self {
        match load_or_create_jwt_secret(dir) {
            Ok(secret) => {
                self.inner = Arc::new(OAuthInner {
                    secret,
                    codes: Mutex::new(HashMap::new()),
                    revoked: Mutex::new(HashSet::new()),
                });
            }
            Err(e) => tracing::warn!(
                "oauth: no se pudo persistir el secreto JWT en {} ({e}); se usa uno efímero y los tokens OAuth no sobrevivirán a un reinicio",
                dir.display()
            ),
        }
        self
    }

    pub fn dev_mode(&self) -> bool {
        self.dev_mode
    }

    /// URI canónica del servidor MCP: el identificador con el que este servidor
    /// se declara como recurso protegido (RFC 9728, `/.well-known/oauth-protected-resource`)
    /// y la audiencia (`aud`) de sus propios tokens (RFC 8707).
    pub fn canonical_resource(&self) -> String {
        self.base_url.trim_end_matches('/').to_string()
    }

    pub fn sign_jwt(&self, jti: &str, ttl_secs: i64) -> String {
        let now = chrono::Utc::now().timestamp();
        // Fase 2 (D-2, RFC 8707): el token va ligado a ESTE recurso. El
        // servidor MCP "MUST only accept tokens specifically intended for
        // itself"; sin `aud`, cualquier token de otro servicio valdría.
        self.sign_jwt_claims(serde_json::json!({
            "sub": "mcp",
            "iat": now,
            "exp": now + ttl_secs,
            "jti": jti,
            "aud": self.canonical_resource(),
            // Fase 2 (D-3): cualquier token emitido por este flujo es de clase
            // MCP. `operator` no se emite todavía: no hay consentimiento.
            "scope": SCOPE_MCP,
        }))
    }

    /// Firma un payload de claims ya construido. Separado de `sign_jwt` para
    /// poder ejercitar en tests tokens legados (sin `aud`) y con audiencia
    /// ajena sin duplicar la lógica HMAC.
    fn sign_jwt_claims(&self, claims: serde_json::Value) -> String {
        let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"HS256","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(claims.to_string().as_bytes());
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
                    // Fase 2 (D-2, RFC 8707): audiencia. Un token cuyo `aud` no
                    // es el nuestro se rechaza SIEMPRE (fue emitido para otro
                    // servicio). Un token SIN `aud` es anterior a D-2 o de un
                    // cliente que no manda `resource`: con dev_mode=false se
                    // rechaza — no hay prueba de que fuera emitido para este
                    // servidor; con dev_mode=true se acepta para no romper el
                    // flujo local ya existente.
                    let audience_ok = match v.get("aud") {
                        Some(aud) => audience_matches(aud, &self.canonical_resource()),
                        None => self.dev_mode,
                    };
                    if !audience_ok {
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
    /// Fase 2 (D-2, RFC 8707): recurso al que el cliente quiere acceder. Se
    /// valida contra la URI canónica de este servidor.
    pub resource: Option<String>,
    /// Fase 2 (D-15): obligatorio para el consentimiento (RFC 6749 §4.1.1).
    /// Sin él no hay a quién mostrar en P1 ni a quién recordar 30 días, así
    /// que la petición muere en 403 `access_denied` sin emitir nada.
    pub client_id: Option<String>,
    /// Fase 2 (D-15): espacio de scopes pedido. Solo `mcp` es concebible;
    /// `operator` se rechaza SIEMPRE con `invalid_scope` (R11).
    pub scope: Option<String>,
    // response_type accepted but not validated (public client)
}

#[derive(Deserialize)]
pub struct TokenForm {
    pub grant_type: String,
    pub code: String,
    pub redirect_uri: String,
    pub code_verifier: String,
    /// Fase 2 (D-2, RFC 8707): si la autorización declaró un `resource`, el
    /// canje puede repetirlo; si lo declara, tiene que ser el mismo recurso.
    pub resource: Option<String>,
}

#[derive(Deserialize)]
pub struct RevokeForm {
    pub token: String,
}

/// Fase 2 (D-15): formulario de P1. `decision` viaja como valor del botón
/// (`approve` / `deny`); la credencial es lo único que el humano teclea.
#[derive(Deserialize)]
pub struct ConsentDecisionForm {
    pub consent_id: String,
    pub csrf: String,
    pub decision: String,
    #[serde(default)]
    pub credential: String,
}

#[derive(Serialize)]
struct TokenResponse {
    access_token: String,
    token_type: &'static str,
    expires_in: i64,
}

const JWT_SECRET_FILE: &str = "oauth_jwt_secret";

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode_32(text: &str) -> Option<Vec<u8>> {
    let t = text.trim();
    if t.len() != 64 || !t.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..32)
        .map(|i| u8::from_str_radix(&t[i * 2..i * 2 + 2], 16).ok())
        .collect()
}

/// Lee `oauth_jwt_secret` (64 caracteres hexadecimales = 32 bytes) o lo crea.
/// Un fichero ilegible o con otro formato se reemplaza por uno nuevo: solo
/// invalida los JWT, nunca el token maestro.
fn load_or_create_jwt_secret(dir: &std::path::Path) -> std::io::Result<Vec<u8>> {
    let path = dir.join(JWT_SECRET_FILE);
    if let Ok(text) = std::fs::read_to_string(&path)
        && let Some(secret) = hex_decode_32(&text)
    {
        return Ok(secret);
    }
    std::fs::create_dir_all(dir)?;
    let secret: Vec<u8> = rand::random::<[u8; 32]>().to_vec();
    std::fs::write(&path, hex_encode(&secret))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    tracing::info!("oauth: secreto JWT persistente creado en {}", path.display());
    Ok(secret)
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// GET /.well-known/oauth-authorization-server
pub async fn metadata_handler(
    State(oauth): State<Arc<OAuthState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let base = &request_origin(&oauth, &headers);
    Json(serde_json::json!({
        "issuer": base,
        "authorization_endpoint": format!("{}/oauth/authorize", base),
        "token_endpoint": format!("{}/oauth/token", base),
        "revocation_endpoint": format!("{}/oauth/revoke", base),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
        // Fase 2 (D-2, RFC 9207): el authorize incluye `iss` en la respuesta.
        "authorization_response_iss_parameter_supported": true
    }))
}

/// GET /.well-known/oauth-protected-resource (RFC 9728). El servidor MCP
/// "MUST implement OAuth 2.0 Protected Resource Metadata": es por aquí por
/// donde un cliente que cumple la spec descubre de quién es el recurso y qué
/// servidor de autorización lo emite (aquí, este mismo proceso).
///
/// `scopes_supported` se omite a propósito: todavía no hay scopes declarados
/// (llegan con los desafíos de scope de la fase B).
pub async fn protected_resource_handler(
    State(oauth): State<Arc<OAuthState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let canonical = request_origin(&oauth, &headers);
    Json(serde_json::json!({
        "resource": canonical,
        "authorization_servers": [canonical],
        // F1 ya rechaza tokens por query (el JWT por query nunca autentica);
        // el único método de portador válido es la cabecera.
        "bearer_methods_supported": ["header"]
    }))
}

/// D-2b: ¿es `host` un alias de loopback? `localhost`, cualquier 127.0.0.0/8 y
/// `::1` son la misma máquina; el kernel normaliza su URI canónica a
/// `localhost` y los clientes escriben `127.0.0.1` (CLAUDE.md) o `[::1]`.
fn is_loopback_host(host: &str) -> bool {
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    bare.eq_ignore_ascii_case("localhost")
        || bare.parse::<std::net::IpAddr>().map(|ip| ip.is_loopback()).unwrap_or(false)
}

/// D-2b: ¿`a` y `b` designan el mismo origen (esquema + host + puerto)? Los
/// alias de loopback cuentan como el mismo host; cualquier otro host se
/// compara estrictamente (sin sufijos: `localhost.evil.com` no es loopback).
fn same_origin_with_loopback_alias(a: &str, b: &str) -> bool {
    let (Ok(a), Ok(b)) = (reqwest::Url::parse(a), reqwest::Url::parse(b)) else {
        return false;
    };
    if a.scheme() != b.scheme() || a.port_or_known_default() != b.port_or_known_default() {
        return false;
    }
    match (a.host_str(), b.host_str()) {
        (Some(ha), Some(hb)) => {
            ha.eq_ignore_ascii_case(hb) || (is_loopback_host(ha) && is_loopback_host(hb))
        }
        _ => false,
    }
}

/// Fase 2 (D-2, RFC 8707): ¿el `resource` declarado por el cliente apunta a
/// este servidor? Mismo origen que la URI canónica (alias de loopback
/// incluidos, D-2b) y cualquier path/query (`/mcp`, ...). Un host o puerto
/// distinto, o un sufijo de host (`localhost.evil.com`), no coincide.
fn resource_matches_canonical(resource: &str, canonical: &str) -> bool {
    same_origin_with_loopback_alias(resource, canonical)
}

/// Fase 2 (D-2): `aud` puede ser una cadena (lo que emitimos) o un array
/// (JWT lo permite); en ambos casos tiene que contener la URI canónica
/// (o su alias de loopback, D-2b).
fn audience_matches(aud: &serde_json::Value, canonical: &str) -> bool {
    match aud {
        serde_json::Value::String(s) => same_origin_with_loopback_alias(s, canonical),
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(|i| i.as_str())
            .any(|i| same_origin_with_loopback_alias(i, canonical)),
        _ => false,
    }
}

/// D-2b: origen con el que mostrar los metadatos y el `iss`. Si la petición
/// llegó por un alias de loopback del mismo puerto (`Host: 127.0.0.1:<puerto>`)
/// se devuelve ese origen, para que el cliente vea un `resource`/`issuer`
/// idéntico a la URL que usó (RFC 9728/8414 lo exigen). Cualquier otro `Host`
/// (no loopback, otro puerto, ausente) se ignora y vale la URI canónica: un
/// `Host` hostil no puede hacer que el servidor se anuncie como otro.
pub(crate) fn request_origin(oauth: &OAuthState, headers: &HeaderMap) -> String {
    let canonical = oauth.canonical_resource();
    let Some(host) = headers.get(HOST).and_then(|v| v.to_str().ok()) else {
        return canonical;
    };
    let scheme = reqwest::Url::parse(&canonical)
        .map(|u| u.scheme().to_string())
        .unwrap_or_else(|_| "http".to_string());
    let candidate = format!("{scheme}://{host}");
    if same_origin_with_loopback_alias(&candidate, &canonical) {
        candidate
    } else {
        canonical
    }
}

/// Fase 1-c: loopback-only redirect_uri (RFC 8252). Deliberately a strict
/// host parse — `http://127.0.0.1.evil.com` must NOT pass.
fn is_loopback_redirect_uri(uri: &str) -> bool {
    let rest = match uri.strip_prefix("http://").or_else(|| uri.strip_prefix("https://")) {
        Some(r) => r,
        None => return false,
    };
    let host_port = rest.split(['/', '?', '#']).next().unwrap_or("");
    // RFC 3986: anything before '@' is userinfo, so `http://127.0.0.1:9@evil/` has host evil.
    // No legitimate loopback redirect carries userinfo; reject it (and backslash tricks).
    if host_port.contains('@') || host_port.contains('\\') {
        return false;
    }
    let host = if let Some(after_bracket) = host_port.strip_prefix('[') {
        after_bracket.split(']').next().unwrap_or("")
    } else {
        host_port.split(':').next().unwrap_or("")
    };
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

// ── Fase 2 (D-15): petición de autorización normalizada ─────────────────────

/// Resultado de validar `/oauth/authorize` (F1-c + D-2 + D-15). Se construye
/// una sola vez para que el camino `dev_mode` y el de consentimiento no
/// diverjan en sus comprobaciones.
struct PreparedAuth {
    redirect_uri: String,
    code_challenge: String,
    state: Option<String>,
    resource: Option<String>,
    client_id: Option<String>,
    scopes: Vec<String>,
}

fn oauth_error(status: StatusCode, error: &str, description: impl Into<String>) -> Response {
    (
        status,
        Json(serde_json::json!({
            "error": error,
            "error_description": description.into()
        })),
    )
        .into_response()
}

/// Fase 2 (D-15, R11): scopes. `mcp` es el único concedible; `operator` se
/// rechaza SIEMPRE (la clase operador solo existe por el token maestro).
fn parse_scopes(raw: Option<&str>) -> Result<Vec<String>, Box<Response>> {
    let Some(raw) = raw else {
        return Ok(vec![SCOPE_MCP.to_string()]);
    };
    let mut out: Vec<String> = Vec::new();
    for scope in raw.split_whitespace() {
        match scope {
            SCOPE_MCP => {
                if !out.iter().any(|s| s == SCOPE_MCP) {
                    out.push(SCOPE_MCP.to_string());
                }
            }
            SCOPE_OPERATOR => {
                return Err(Box::new(oauth_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_scope",
                    "scope=operator is never granted: the operator class requires the master token, \
                     not an OAuth client",
                )));
            }
            other => {
                return Err(Box::new(oauth_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_scope",
                    format!("unknown scope '{other}'; this server grants only '{SCOPE_MCP}'"),
                )));
            }
        }
    }
    if out.is_empty() {
        out.push(SCOPE_MCP.to_string());
    }
    Ok(out)
}

fn validate_authorization(
    oauth: &OAuthState,
    params: &AuthorizeQuery,
) -> Result<PreparedAuth, Box<Response>> {
    // Fase 1-c: loopback-only redirect_uri (defense in depth).
    if !is_loopback_redirect_uri(&params.redirect_uri) {
        return Err(Box::new(oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "redirect_uri must use a loopback host (127.0.0.1, localhost or ::1)",
        )));
    }

    // Only S256 supported
    if params.code_challenge_method.as_deref().unwrap_or("S256") != "S256" {
        return Err(Box::new(oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "only S256 supported",
        )));
    }

    // Fase 2 (D-2, RFC 8707): el cliente declara para qué recurso pide el
    // token. Este servidor solo emite para sí mismo; un `resource` ajeno se
    // rechaza con `invalid_target` en vez de emitir un token que después
    // rechazaríamos nosotros mismos al validarlo (audiencia).
    if let Some(resource) = params.resource.as_deref()
        && !resource_matches_canonical(resource, &oauth.canonical_resource())
    {
        return Err(Box::new(oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_target",
            format!("resource must be this server ({})", oauth.canonical_resource()),
        )));
    }

    let scopes = parse_scopes(params.scope.as_deref())?;

    Ok(PreparedAuth {
        redirect_uri: params.redirect_uri.clone(),
        code_challenge: params.code_challenge.clone(),
        state: params.state.clone(),
        resource: params.resource.clone(),
        client_id: params.client_id.clone().filter(|c| !c.is_empty()),
        scopes,
    })
}

/// Emite un code (60 s, un solo uso) y redirige con `state` + `iss`.
fn issue_code(oauth: &OAuthState, headers: &HeaderMap, prepared: &PreparedAuth) -> Response {
    let code = URL_SAFE_NO_PAD.encode(rand::random::<[u8; 16]>());

    {
        let mut codes = oauth.inner.codes.lock().unwrap_or_else(|e| e.into_inner());
        codes.insert(
            code.clone(),
            PendingCode {
                code_challenge: prepared.code_challenge.clone(),
                redirect_uri: prepared.redirect_uri.clone(),
                expires_at: chrono::Utc::now().timestamp() + 60,
                resource: prepared.resource.clone(),
            },
        );
    }

    let state_part = prepared
        .state
        .as_deref()
        .map(|s| format!("&state={}", urlencoding::encode(s)))
        .unwrap_or_default();

    // Fase 2 (D-2, RFC 9207): `iss` identifica al emisor en la propia
    // respuesta de autorización (defensa contra mix-up).
    let iss_part = format!(
        "&iss={}",
        urlencoding::encode(&request_origin(oauth, headers))
    );

    let separator = if prepared.redirect_uri.contains('?') { "&" } else { "?" };
    let location = format!(
        "{}{}code={}{}{}",
        prepared.redirect_uri,
        separator,
        urlencoding::encode(&code),
        state_part,
        iss_part
    );

    info!("[OAuth] Authorize OK -> {}", prepared.redirect_uri);
    let mut response = Redirect::temporary(&location).into_response();
    // El consent_id ya se consumió: la cookie no sirve para nada más.
    if let Ok(v) = axum::http::HeaderValue::from_str(
        "tylluan_oauth_consent=; HttpOnly; SameSite=Lax; Path=/oauth; Max-Age=0",
    ) {
        response
            .headers_mut()
            .insert(axum::http::header::SET_COOKIE, v);
    }
    response
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    use subtle::ConstantTimeEq;
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

fn consent_cookie(headers: &HeaderMap) -> Option<String> {
    let raw = headers
        .get(axum::http::header::COOKIE)?
        .to_str()
        .ok()?;
    raw.split(';').find_map(|part| {
        let (k, v) = part.trim().split_once('=')?;
        (k == "tylluan_oauth_consent").then(|| v.to_string())
    })
}

/// P1: página de consentimiento autocontenida (sin JS, sin build del
/// dashboard, sin recursos externos). `error` re-renderiza tras una credencial
/// incorrecta.
fn consent_page_response(
    oauth: &OAuthState,
    headers: &HeaderMap,
    pending: &super::oauth_consent::PendingConsent,
    error: Option<&str>,
    status: StatusCode,
) -> Response {
    use super::oauth_consent::{ConsentPage, PENDING_TTL_SECS, render_consent_page};

    let nonce = URL_SAFE_NO_PAD.encode(rand::random::<[u8; 16]>());
    let origin = request_origin(oauth, headers);
    let body = render_consent_page(&ConsentPage {
        client_id: &pending.client_id,
        client_name: &pending.client_name,
        redirect_host: &pending.redirect_host,
        kernel_origin: &origin,
        consent_id: &pending.consent_id,
        csrf: &pending.csrf,
        nonce: &nonce,
        error,
    });

    let mut response = (status, body).into_response();
    let h = response.headers_mut();
    h.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("text/html; charset=utf-8"),
    );
    h.insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    h.insert(
        axum::http::header::X_FRAME_OPTIONS,
        axum::http::HeaderValue::from_static("DENY"),
    );
    h.insert(
        axum::http::header::REFERRER_POLICY,
        axum::http::HeaderValue::from_static("no-referrer"),
    );
    if let Ok(csp) = axum::http::HeaderValue::from_str(&format!(
        "default-src 'none'; style-src 'nonce-{nonce}'; img-src 'none'; \
         base-uri 'none'; form-action 'self'; frame-ancestors 'none'"
    )) {
        h.insert(axum::http::header::CONTENT_SECURITY_POLICY, csp);
    }
    if let Ok(cookie) = axum::http::HeaderValue::from_str(&format!(
        "tylluan_oauth_consent={}; HttpOnly; SameSite=Lax; Path=/oauth; Max-Age={PENDING_TTL_SECS}",
        pending.consent_id
    )) {
        h.insert(axum::http::header::SET_COOKIE, cookie);
    }
    response
}

/// GET /oauth/authorize
///
/// `dev_mode=true` sigue auto-aprobando (R12, sin cambios). Con
/// `dev_mode=false` (D-15): si hay consentimiento vigente para
/// cliente+redirect_uri se emite el code con 0 acciones humanas; si no, se
/// sirve P1 y NO se emite nada hasta que el operador teclee su credencial.
pub async fn authorize_handler(
    State(oauth): State<Arc<OAuthState>>,
    headers: HeaderMap,
    Query(params): Query<AuthorizeQuery>,
) -> Response {
    let prepared = match validate_authorization(&oauth, &params) {
        Ok(p) => p,
        Err(resp) => return *resp,
    };

    if oauth.dev_mode {
        return issue_code(&oauth, &headers, &prepared);
    }

    // Sin client_id no hay a quién mostrar ni recordar (RFC 6749 §4.1.1 lo
    // exige). Fail-closed: 403 sin redirect.
    let Some(client_id) = prepared.client_id.clone() else {
        info!("[OAuth] Authorize refused: missing client_id (operator consent cannot be requested)");
        return oauth_error(
            StatusCode::FORBIDDEN,
            "access_denied",
            "authorization request is missing client_id; nothing was issued",
        );
    };

    if oauth
        .consents()
        .valid_consent(&client_id, &prepared.redirect_uri)
        .is_some()
    {
        info!(
            "[OAuth] Consent remembered for client {client_id} (redirect {}, scopes {:?})",
            prepared.redirect_uri, prepared.scopes
        );
        return issue_code(&oauth, &headers, &prepared);
    }

    let client_name = oauth
        .consents()
        .display_name(&client_id, &prepared.redirect_uri)
        .unwrap_or_else(|| client_id.clone());
    let pending = oauth
        .consents()
        .create_pending(super::oauth_consent::PendingConsent {
            consent_id: String::new(),
            csrf: String::new(),
            client_id: client_id.clone(),
            client_name,
            redirect_host: super::oauth_consent::redirect_host(&prepared.redirect_uri),
            redirect_uri: prepared.redirect_uri.clone(),
            code_challenge: prepared.code_challenge.clone(),
            state: prepared.state.clone(),
            resource: prepared.resource.clone(),
            scopes: prepared.scopes.clone(),
            created_at: 0,
            expires_at: 0,
        });
    info!(
        target: "oauth_consent",
        "decision=pending client_id={client_id} redirect={} scopes={}",
        pending.redirect_uri,
        pending.scopes.join(",")
    );
    consent_page_response(&oauth, &headers, &pending, None, StatusCode::OK)
}

/// POST /oauth/authorize — decisión del operador sobre P1 (D-15).
///
/// Nunca emite sin credencial correcta (C1) y nunca emite si el operador
/// deniega. Éxito → 302 con code; fallo → 403 sin `Location` (E2).
pub async fn authorize_decision_handler(
    State(oauth): State<Arc<OAuthState>>,
    parts: axum::http::request::Parts,
    Form(form): Form<ConsentDecisionForm>,
) -> Response {
    let headers = &parts.headers;
    if oauth.dev_mode {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "operator consent is disabled in dev_mode",
        );
    }

    // IP del socket si el router se sirvió con `into_make_service_with_connect_info`
    // (producción). En tests in-process no hay extensión: bucket "unknown".
    let ip = parts
        .extensions
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|c| c.0.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    if oauth.consents().is_locked(&ip) {
        // D-25: el bloqueo escala (30 s → 300 s), así que el `Retry-After` es el
        // real. El suelo de 60 s anterior contradecía el primer escalón.
        let retry = oauth.consents().retry_after_secs(&ip).max(1);
        let mut response = oauth_error(
            StatusCode::TOO_MANY_REQUESTS,
            "temporarily_unavailable",
            "too many failed credential attempts; wait before retrying",
        );
        if let Ok(v) = axum::http::HeaderValue::from_str(&retry.to_string()) {
            response.headers_mut().insert(axum::http::header::RETRY_AFTER, v);
        }
        info!(target: "oauth_consent", "decision=rate_limited ip={ip} retry_after={retry}");
        return response;
    }

    // CSRF: cookie HttpOnly emitida con P1 + nonce del formulario.
    match consent_cookie(headers) {
        Some(cookie) if constant_time_eq(&cookie, &form.consent_id) => {}
        _ => {
            oauth.consents().record_failure(&ip);
            return oauth_error(
                StatusCode::FORBIDDEN,
                "access_denied",
                "consent request does not match this browser session",
            );
        }
    }

    let Some(pending) = oauth.consents().peek_pending(&form.consent_id) else {
        return oauth_error(
            StatusCode::FORBIDDEN,
            "access_denied",
            "consent request expired or unknown",
        );
    };

    if !constant_time_eq(&form.csrf, &pending.csrf) {
        oauth.consents().record_failure(&ip);
        return oauth_error(
            StatusCode::FORBIDDEN,
            "access_denied",
            "invalid consent token",
        );
    }

    match form.decision.as_str() {
        "deny" => {
            oauth.consents().drop_pending(&pending.consent_id);
            info!(
                target: "oauth_consent",
                "decision=denied client_id={} redirect={} ip={ip}",
                pending.client_id, pending.redirect_uri
            );
            oauth_error(
                StatusCode::FORBIDDEN,
                "access_denied",
                "the operator denied this authorization request",
            )
        }
        "approve" => {
            let Some(expected) = oauth.operator_credential.as_deref() else {
                info!(
                    target: "oauth_consent",
                    "decision=refused_no_operator_credential client_id={}",
                    pending.client_id
                );
                return oauth_error(
                    StatusCode::FORBIDDEN,
                    "access_denied",
                    "this kernel has no operator credential configured; use a static Bearer token",
                );
            };

            if !constant_time_eq(&form.credential, expected) {
                oauth.consents().record_failure(&ip);
                // Un intento fallido no pierde la petición: se rota el CSRF y
                // se vuelve a mostrar P1 con el error.
                let refreshed = match oauth.consents().refresh_csrf(&pending.consent_id) {
                    Some(csrf) => super::oauth_consent::PendingConsent { csrf, ..pending },
                    None => {
                        return oauth_error(
                            StatusCode::FORBIDDEN,
                            "access_denied",
                            "consent request expired or unknown",
                        );
                    }
                };
                return consent_page_response(
                    &oauth,
                    headers,
                    &refreshed,
                    Some("Credencial incorrecta. Inténtalo de nuevo."),
                    StatusCode::FORBIDDEN,
                );
            }

            oauth.consents().clear_failures(&ip);
            let Some(granted) = oauth.consents().take_pending(&pending.consent_id) else {
                return oauth_error(
                    StatusCode::FORBIDDEN,
                    "access_denied",
                    "consent request expired or unknown",
                );
            };
            oauth.consents().record_consent(
                &granted.client_id,
                &granted.client_name,
                &granted.redirect_uri,
                &granted.scopes,
            );
            let prepared = PreparedAuth {
                redirect_uri: granted.redirect_uri,
                code_challenge: granted.code_challenge,
                state: granted.state,
                resource: granted.resource,
                client_id: Some(granted.client_id),
                scopes: granted.scopes,
            };
            issue_code(&oauth, headers, &prepared)
        }
        _ => oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "decision must be 'approve' or 'deny'",
        ),
    }
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

    // Fase 2 (D-2, RFC 8707): el recurso del canje manda si viene; si no,
    // vale el declarado en la autorización. Si es otro, `invalid_target`.
    if let Some(resource) = form.resource.as_deref().or(pending.resource.as_deref())
        && !resource_matches_canonical(resource, &oauth.canonical_resource())
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_target",
                "error_description": format!(
                    "resource must be this server ({})",
                    oauth.canonical_resource()
                )
            })),
        )
            .into_response();
    }

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
            resource: None,
            client_id: None,
            scope: None,
        }
    }

    /// R13: with dev_mode=false the authorize endpoint must REFUSE (Fase 1-a).
    #[tokio::test]
    async fn r13_authorize_refuses_when_dev_mode_false() {
        let st = Arc::new(OAuthState::new("http://localhost:3030".to_string()));
        assert!(!st.dev_mode(), "fail-closed default");
        let resp = authorize_handler(State(st), HeaderMap::new(), Query(q("http://127.0.0.1:9999/cb"))).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    /// R15: non-loopback redirect_uri is rejected even with dev_mode=true.
    #[tokio::test]
    async fn r15_redirect_must_be_loopback() {
        let st = Arc::new(OAuthState::new("http://localhost:3030".to_string()).with_dev_mode(true));
        let bad = authorize_handler(State(st.clone()), HeaderMap::new(), Query(q("http://evil.example/cb"))).await;
        assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
        let sneaky = authorize_handler(State(st), HeaderMap::new(), Query(q("http://127.0.0.1.evil.example/cb"))).await;
        assert_eq!(sneaky.status(), StatusCode::BAD_REQUEST, "prefix lookalike must not pass");
    }

    /// R12: dev_mode=true keeps the local auto-approval flow intact (302-ish redirect with code).
    #[tokio::test]
    async fn r12_dev_mode_approves_loopback_and_redirects() {
        let st = Arc::new(OAuthState::new("http://localhost:3030".to_string()).with_dev_mode(true));
        let resp = authorize_handler(State(st), HeaderMap::new(), Query(q("http://127.0.0.1:9999/cb"))).await;
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

/// ADR-019 OAuth Fase 2, fase A (D-2): descubrimiento (RFC 9728), audiencia
/// (RFC 8707) y `iss` en la respuesta de autorización (RFC 9207).
#[cfg(test)]
mod fase2_tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::http::header::LOCATION;

    const BASE: &str = "http://localhost:3030";

    fn state(dev_mode: bool) -> Arc<OAuthState> {
        Arc::new(OAuthState::new(BASE.to_string()).with_dev_mode(dev_mode))
    }

    fn q(redirect: &str, resource: Option<&str>) -> AuthorizeQuery {
        AuthorizeQuery {
            redirect_uri: redirect.to_string(),
            code_challenge: "abc".to_string(),
            code_challenge_method: Some("S256".to_string()),
            state: None,
            resource: resource.map(str::to_string),
            client_id: None,
            scope: None,
        }
    }

    fn s256(verifier: &str) -> String {
        URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
    }

    fn claims(token: &str) -> serde_json::Value {
        let payload = token.split('.').nth(1).expect("jwt payload");
        let bytes = URL_SAFE_NO_PAD.decode(payload).expect("b64 payload");
        serde_json::from_slice(&bytes).expect("json claims")
    }

    async fn body_json(resp: Response) -> serde_json::Value {
        let bytes = to_bytes(resp.into_body(), 64 * 1024).await.expect("body");
        serde_json::from_slice(&bytes).expect("json body")
    }

    /// D-2.1 (RFC 9728): el recurso protegido se declara a sí mismo y a su AS.
    #[tokio::test]
    async fn protected_resource_metadata_declares_resource_and_as() {
        let v =
            body_json(protected_resource_handler(State(state(false)), HeaderMap::new()).await.into_response()).await;
        assert_eq!(v["resource"], BASE);
        assert_eq!(v["authorization_servers"][0], BASE);
        assert_eq!(v["bearer_methods_supported"][0], "header");
    }

    /// D-2.3 (RFC 9207): los metadatos anuncian el parámetro `iss`.
    #[tokio::test]
    async fn metadata_advertises_iss_parameter_supported() {
        let v = body_json(metadata_handler(State(state(false)), HeaderMap::new()).await.into_response()).await;
        assert!(v["authorization_response_iss_parameter_supported"]
            .as_bool()
            .unwrap_or(false));
        assert_eq!(v["code_challenge_methods_supported"][0], "S256");
    }

    /// D-2.3: el redirect de authorize lleva `iss` = URI canónica.
    #[tokio::test]
    async fn authorize_response_carries_iss() {
        let resp =
            authorize_handler(State(state(true)), HeaderMap::new(), Query(q("http://127.0.0.1:9999/cb", None))).await;
        assert!(resp.status().is_redirection());
        let loc = resp
            .headers()
            .get(LOCATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(loc.contains("code="), "location: {loc}");
        assert!(
            loc.contains("iss=http%3A%2F%2Flocalhost%3A3030"),
            "location: {loc}"
        );
    }

    /// D-2.2: el JWT emitido lleva `aud` = URI canónica.
    #[test]
    fn signed_jwt_carries_canonical_audience() {
        let c = claims(&state(false).sign_jwt("jti-d2", 60));
        assert_eq!(c["aud"], BASE);
    }

    /// D-3: todo token de este flujo es de clase `mcp`; `operator` no se emite
    /// (no hay consentimiento todavía).
    #[test]
    fn signed_jwt_carries_mcp_scope_only() {
        let c = claims(&state(false).sign_jwt("jti-d3", 60));
        assert_eq!(c["scope"], SCOPE_MCP);
    }

    /// D-2.2: con dev_mode=false un token sin `aud` (legado) se rechaza.
    #[test]
    fn legacy_token_without_audience_rejected_in_prod() {
        let st = state(false);
        let now = chrono::Utc::now().timestamp();
        let legacy = st.sign_jwt_claims(serde_json::json!({
            "sub": "mcp", "iat": now, "exp": now + 60, "jti": "legacy"
        }));
        assert!(!st.validate_bearer(&legacy));
    }

    /// D-2.2: con dev_mode=true se acepta el token legado (motivo escrito en
    /// `validate_bearer`: no romper el flujo local ya existente).
    #[test]
    fn legacy_token_without_audience_accepted_in_dev_mode() {
        let st = state(true);
        let now = chrono::Utc::now().timestamp();
        let legacy = st.sign_jwt_claims(serde_json::json!({
            "sub": "mcp", "iat": now, "exp": now + 60, "jti": "legacy-dev"
        }));
        assert!(st.validate_bearer(&legacy));
    }

    /// D-2.2: un token con audiencia ajena se rechaza en ambos modos.
    #[test]
    fn foreign_audience_rejected_in_both_modes() {
        for dev in [true, false] {
            let st = state(dev);
            let now = chrono::Utc::now().timestamp();
            let foreign = st.sign_jwt_claims(serde_json::json!({
                "sub": "mcp", "iat": now, "exp": now + 60, "jti": "foreign",
                "aud": "http://other.example:1234"
            }));
            assert!(!st.validate_bearer(&foreign), "dev={dev}");
        }
    }

    /// D-2.2: el token propio se valida sin dev_mode (producción).
    #[test]
    fn own_token_validates_in_prod_mode() {
        let st = state(false);
        assert!(st.validate_bearer(&st.sign_jwt("jti-prod", 60)));
    }

    /// D-2.2: `resource` propio (con o sin path) se acepta; otro host/puerto no.
    #[test]
    fn resource_must_point_to_this_server() {
        let canonical = "http://localhost:3030";
        assert!(resource_matches_canonical("http://localhost:3030", canonical));
        assert!(resource_matches_canonical("http://localhost:3030/", canonical));
        assert!(resource_matches_canonical("http://localhost:3030/mcp", canonical));
        assert!(!resource_matches_canonical("http://localhost:3030.evil.com", canonical));
        assert!(!resource_matches_canonical("http://localhost:3031", canonical));
        assert!(!resource_matches_canonical("https://localhost:3030", canonical));
    }

    /// D-2.2: authorize rechaza un `resource` ajeno con `invalid_target`.
    #[tokio::test]
    async fn authorize_rejects_foreign_resource() {
        let resp = authorize_handler(
            State(state(true)),
            HeaderMap::new(),
            Query(q("http://127.0.0.1:9999/cb", Some("http://evil.example:9999"))),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let v = body_json(resp).await;
        assert_eq!(v["error"], "invalid_target");
    }

    /// D-2.2 end-to-end: authorize con `resource` propio → canje → `aud`.
    #[tokio::test]
    async fn authorized_token_carries_audience_end_to_end() {
        let st = state(true);
        let verifier = "f2-d2-verifier-0123456789abcdefghijklmnopqrstuv";
        let redirect = "http://127.0.0.1:9999/cb";
        let resp = authorize_handler(
            State(st.clone()),
            HeaderMap::new(),
            Query(AuthorizeQuery {
                redirect_uri: redirect.to_string(),
                code_challenge: s256(verifier),
                code_challenge_method: Some("S256".to_string()),
                state: None,
                resource: Some(format!("{BASE}/mcp")),
                client_id: None,
                scope: None,
            }),
        )
        .await;
        assert!(resp.status().is_redirection());
        let loc = resp
            .headers()
            .get(LOCATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let code = loc
            .split("code=")
            .nth(1)
            .unwrap_or("")
            .split('&')
            .next()
            .unwrap_or("")
            .to_string();
        assert!(!code.is_empty(), "location: {loc}");

        let token_resp = token_handler(
            State(st.clone()),
            Form(TokenForm {
                grant_type: "authorization_code".to_string(),
                code,
                redirect_uri: redirect.to_string(),
                code_verifier: verifier.to_string(),
                resource: Some(format!("{BASE}/mcp")),
            }),
        )
        .await;
        assert_eq!(token_resp.status(), StatusCode::OK);
        let v = body_json(token_resp).await;
        let token = v["access_token"].as_str().expect("access_token");
        assert_eq!(claims(token)["aud"], BASE);
        assert!(st.validate_bearer(token), "the freshly issued token must validate");
    }

    fn host_headers(host: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(HOST, host.parse().expect("host header"));
        h
    }

    /// D-2b: un cliente que usa 127.0.0.1 o [::1] (CLAUDE.md manda 127.0.0.1)
    /// declara un `resource` del mismo servidor; no debe recibir invalid_target.
    #[test]
    fn resource_accepts_loopback_aliases_of_this_server() {
        let canonical = "http://localhost:3030";
        for r in [
            "http://127.0.0.1:3030",
            "http://127.0.0.1:3030/mcp",
            "http://[::1]:3030/",
            "http://LOCALHOST:3030",
            "http://127.0.0.2:3030",
        ] {
            assert!(resource_matches_canonical(r, canonical), "must accept {r}");
        }
    }

    /// D-2b: los alias NO abren la puerta a otro host, puerto o esquema.
    #[test]
    fn resource_alias_does_not_widen_to_other_hosts() {
        let canonical = "http://localhost:3030";
        for r in [
            "http://127.0.0.1.evil.com:3030",
            "http://localhost.evil.com:3030",
            "http://127.0.0.1:3031",
            "https://127.0.0.1:3030",
            "http://127.0.0.1:3030@evil.com",
            "http://192.168.1.5:3030",
            "not a url",
        ] {
            assert!(!resource_matches_canonical(r, canonical), "must reject {r}");
        }
    }

    /// D-2b: la audiencia con alias de loopback se acepta; la ajena, no.
    #[test]
    fn audience_accepts_loopback_alias_and_rejects_foreign() {
        let canonical = "http://localhost:3030";
        assert!(audience_matches(&serde_json::json!("http://127.0.0.1:3030"), canonical));
        assert!(audience_matches(
            &serde_json::json!(["http://other.example", "http://[::1]:3030"]),
            canonical
        ));
        assert!(!audience_matches(&serde_json::json!("http://127.0.0.1:9999"), canonical));
        assert!(!audience_matches(&serde_json::json!("http://other.example:3030"), canonical));
    }

    /// D-2b: los metadatos reflejan el origen con el que se pidieron si es un
    /// alias de loopback del mismo puerto (RFC 9728: `resource` idéntico a la URL).
    #[tokio::test]
    async fn protected_resource_reflects_loopback_alias_host() {
        let v = body_json(
            protected_resource_handler(State(state(false)), host_headers("127.0.0.1:3030"))
                .await
                .into_response(),
        )
        .await;
        assert_eq!(v["resource"], "http://127.0.0.1:3030");
        assert_eq!(v["authorization_servers"][0], "http://127.0.0.1:3030");
    }

    /// D-2b: un `Host` hostil (otro host, otro puerto) no cambia lo que el
    /// servidor anuncia: sigue valiendo la URI canónica.
    #[tokio::test]
    async fn hostile_host_header_cannot_change_advertised_origin() {
        for host in ["evil.example", "evil.example:3030", "127.0.0.1:9999", "localhost.evil.com:3030"] {
            let v = body_json(
                protected_resource_handler(State(state(false)), host_headers(host))
                    .await
                    .into_response(),
            )
            .await;
            assert_eq!(v["resource"], BASE, "Host: {host}");
            let m = body_json(
                metadata_handler(State(state(false)), host_headers(host)).await.into_response(),
            )
            .await;
            assert_eq!(m["issuer"], BASE, "Host: {host}");
        }
    }

    /// D-2b: authorize con `resource` en 127.0.0.1 se acepta y el `iss` de la
    /// redirección usa el mismo origen que vio el cliente.
    #[tokio::test]
    async fn authorize_with_loopback_alias_resource_and_matching_iss() {
        let resp = authorize_handler(
            State(state(true)),
            host_headers("127.0.0.1:3030"),
            Query(q("http://127.0.0.1:9999/cb", Some("http://127.0.0.1:3030/mcp"))),
        )
        .await;
        assert!(resp.status().is_redirection(), "status {}", resp.status());
        let loc = resp.headers().get(LOCATION).and_then(|v| v.to_str().ok()).unwrap_or("");
        assert!(loc.contains("iss=http%3A%2F%2F127.0.0.1%3A3030"), "location: {loc}");
    }
}

/// El secreto de firma sobrevive a un reinicio (dos instancias, mismo directorio).
#[cfg(test)]
mod persisted_secret_tests {
    use super::*;

    fn tmp(label: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "tylluan_oauth_{label}_{}_{}",
            std::process::id(),
            rand::random::<u32>()
        ));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn jwt_signed_before_a_restart_validates_after_it() {
        let dir = tmp("restart");
        let a = OAuthState::new("http://localhost:3030".into()).with_persisted_secret(&dir);
        let token = a.sign_jwt("jti-restart", 60);
        let b = OAuthState::new("http://localhost:3030".into()).with_persisted_secret(&dir);
        assert!(b.validate_bearer(&token), "a restart must not invalidate OAuth JWTs");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn without_persistence_each_instance_has_its_own_secret() {
        let a = OAuthState::new("http://localhost:3030".into());
        let b = OAuthState::new("http://localhost:3030".into());
        assert!(!b.validate_bearer(&a.sign_jwt("jti-eph", 60)));
    }

    #[test]
    fn corrupt_secret_file_is_replaced_not_fatal() {
        let dir = tmp("corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(JWT_SECRET_FILE), "not hex at all").unwrap();
        let a = OAuthState::new("http://localhost:3030".into()).with_persisted_secret(&dir);
        let token = a.sign_jwt("jti-corrupt", 60);
        let b = OAuthState::new("http://localhost:3030".into()).with_persisted_secret(&dir);
        assert!(b.validate_bearer(&token), "the replaced file must persist");
        let text = std::fs::read_to_string(dir.join(JWT_SECRET_FILE)).unwrap();
        assert_eq!(text.trim().len(), 64);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unwritable_location_falls_back_to_an_ephemeral_secret() {
        // A path under a regular file cannot be created as a directory.
        let dir = tmp("blocked");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a_file");
        std::fs::write(&file, "x").unwrap();
        let state = OAuthState::new("http://localhost:3030".into())
            .with_persisted_secret(&file.join("sub"));
        assert!(state.validate_bearer(&state.sign_jwt("jti-fallback", 60)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
