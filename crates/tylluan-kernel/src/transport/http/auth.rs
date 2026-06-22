use axum::{
    Json,
    extract::{Request, State},
    http::{StatusCode, HeaderMap},
    middleware::Next,
    response::{IntoResponse, Response},
};
use tokio::task_local;
use tracing::{info, warn};
use std::sync::Arc;
use std::time::{Duration, Instant};
use crate::transport::http::HttpState;
use crate::security::guard::ExecutionGuard;
use crate::config::AclConfig;

task_local! {
    /// Current ACL role for the request, set by bearer_auth_middleware.
    /// Defaults to "admin" when no ACL is configured (stdio/local access).
    pub static ACL_ROLE: String;
}

/// Get the current request's ACL role. Returns "admin" if unset (local/stdio access).
pub fn current_acl_role() -> String {
    ACL_ROLE.try_with(|r| r.clone()).unwrap_or_else(|_| "admin".to_string())
}

/// Check if a role has access to a guild based on ACL config.
/// admin role always has access. Unknown roles are denied.
pub fn acl_can_access(role: &str, guild: &str, acl: &AclConfig) -> bool {
    if role == "admin" { return true; }
    if let Some(allowed) = acl.roles.get(role) {
        allowed.iter().any(|g| g == "*" || g == guild)
    } else {
        false
    }
}

/// Resolve the ACL role for a token based on ACL config.
/// If the token is not listed, returns the default_role.
pub fn resolve_role_for_token(token: &str, acl: &AclConfig) -> String {
    acl.tokens.get(token).cloned().unwrap_or_else(|| acl.default_role.clone())
}

/// Resolve ACL role from the current request state and bearer token.
async fn resolve_acl_role(state: &Arc<HttpState>, bearer_token: Option<&str>) -> String {
    let config = state.config.read().await;
    let acl = &config.security.acl;
    match bearer_token {
        Some(token) => resolve_role_for_token(token, acl),
        None => acl.default_role.clone(),
    }
}

/// Bearer token authentication middleware.
pub async fn bearer_auth_middleware(
    State(state): State<Arc<HttpState>>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Response {
    // SECURITY: Allow SSE with token in query param for dashboard compatibility
    // Format: /sse?token=<bearer_token>
    let uri = request.uri().path();
    let method = request.method().to_string();
    let query = request.uri().query().unwrap_or("");
    
    // DEBUG: Log all incoming requests to help diagnose 405
    info!("🔍 [HTTP] {} {} (query: '{}')", method, uri, query);
    
    // Determine if request is authorized and resolve ACL role
    let is_authorized = {
        // 1. Explicit Public Bypass
        if uri == "/health" || uri == "/discovery" || uri == "/ui" || uri == "/" || uri == "/dashboard" ||
           uri.starts_with("/js/") || uri.starts_with("/css/") || uri.starts_with("/img/") || uri.starts_with("/fonts/") ||
           uri.ends_with(".js") || uri.ends_with(".css") || uri.ends_with(".html") || uri.ends_with(".png") || uri.ends_with(".svg")
        {
            true
        }
        // 2. Token in Query
        else if let Some(expected) = &state.auth_token {
            query.split('&').any(|pair| {
                if let Some((k, v)) = pair.split_once('=') {
                    (k == "token" || k == "Authorization") && ExecutionGuard::secure_compare(v, expected)
                } else {
                    false
                }
            })
        }
        // 3. Dev Mode Bypass
        else if state.dev_mode.unwrap_or(false) {
            true
        }
        // 4. Bearer Token
        else {
            let expected_token = match &state.auth_token {
                Some(t) => Some(t.as_str()),
                None => None,
            };
            match expected_token {
                Some(token) => {
                    let auth_header = headers
                        .get("authorization")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("");
                    if let Some(bearer) = auth_header.strip_prefix("Bearer ") {
                        ExecutionGuard::secure_compare(bearer, token)
                            || state.oauth.validate_bearer(bearer)
                    } else {
                        false
                    }
                }
                None => {
                    warn!("🚫 AUTH_FAILURE: No Master Token configured and dev_mode is false.");
                    false
                }
            }
        }
    };

    if !is_authorized {
        warn!("🚫 Unauthorized request to {}", request.uri());
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "unauthorized",
                "message": "Valid Bearer token required. Check your .tylluan-token file."
            })),
        ).into_response();
    }

    // ─── Rate Limiting by agent_id ──────────────────────────────────
    let mut agent_id = headers.get("X-Agent-Id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    if agent_id.is_none()
        && let Some(query) = request.uri().query() {
            for pair in query.split('&') {
                let mut parts = pair.splitn(2, '=');
                if let (Some("agent_id"), Some(v)) = (parts.next(), parts.next()) {
                    agent_id = Some(v.to_string());
                    break;
                }
            }
        }

    if let Some(aid) = agent_id {
        let max_req = {
            let config = state.config.read().await;
            config.limits.max_requests_per_agent_per_min
        };
        
        let now = Instant::now();
        let mut limiter_entry = state.agent_rate_limiter.entry(aid.clone()).or_insert((0, now));
        let (count, last_reset) = limiter_entry.value_mut();
        
        if now.duration_since(*last_reset) > Duration::from_secs(60) {
            *count = 1;
            *last_reset = now;
        } else {
            *count += 1;
            if *count > max_req {
                warn!("🚫 RATE_LIMIT: Agent '{}' exceeded {} req/min", aid, max_req);
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(serde_json::json!({
                        "error": "rate_limit",
                        "agent_id": aid,
                        "retry_after_secs": 10
                    })),
                ).into_response();
            }
        }
    }

    // Resolve ACL role for this request and process with role scope
    let bearer_token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    let acl_role = resolve_acl_role(&state, bearer_token.as_deref()).await;
    ACL_ROLE.scope(acl_role, async move {
        next.run(request).await
    }).await
}
