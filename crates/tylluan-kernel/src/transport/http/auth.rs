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
    
    let sanitized_query = sanitize_query(query);
    
    // DEBUG: Log all incoming requests to help diagnose 405
    info!("🔍 [HTTP] {} {} (query: '{}')", method, uri, sanitized_query);
    
    // Determine if request is authorized and resolve ACL role
    let is_authorized = {
        // 1. Explicit Public Bypass
        if uri == "/health" || uri == "/discovery" || uri == "/ui" || uri == "/" || uri == "/dashboard" ||
           uri.starts_with("/js/") || uri.starts_with("/css/") || uri.starts_with("/img/") || uri.starts_with("/fonts/") ||
           uri.ends_with(".js") || uri.ends_with(".css") || uri.ends_with(".html") || uri.ends_with(".png") || uri.ends_with(".svg")
        {
            true
        }
        // 2. Dev Mode Bypass
        else if state.dev_mode.unwrap_or(false) {
            true
        }
        // 3. Token Authentication (Header or Query)
        else if let Some(expected) = &state.auth_token {
            // A. Check Bearer Token in Authorization Header
            let auth_header = headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            
            let has_valid_bearer = if let Some(bearer) = auth_header.strip_prefix("Bearer ") {
                ExecutionGuard::secure_compare(bearer, expected)
                    || state.oauth.validate_bearer(bearer)
            } else {
                false
            };

            // B. Check Token in Query String (with URL decoding to support / and +)
            let has_valid_query = query.split('&').any(|pair| {
                if let Some((k, v)) = pair.split_once('=') {
                    if k == "token" || k == "Authorization" {
                        if let Ok(decoded) = urlencoding::decode(v) {
                            ExecutionGuard::secure_compare(&decoded, expected)
                        } else {
                            ExecutionGuard::secure_compare(v, expected)
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            });

            has_valid_bearer || has_valid_query
        }
        else {
            warn!("🚫 AUTH_FAILURE: No Master Token configured and dev_mode is false.");
            false
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
    let query_str = request.uri().query().unwrap_or("");
    let bearer_token = extract_token(&headers, query_str);

    let acl_role = resolve_acl_role(&state, bearer_token.as_deref()).await;
    ACL_ROLE.scope(acl_role, async move {
        next.run(request).await
    }).await
}

/// Sanitizes query string to prevent token leakage in logs.
pub fn sanitize_query(query: &str) -> String {
    if query.contains("token=") || query.contains("Authorization=") {
        query.split('&').map(|param| {
            if param.starts_with("token=") {
                "token=[REDACTED]"
            } else if param.starts_with("Authorization=") {
                "Authorization=[REDACTED]"
            } else {
                param
            }
        }).collect::<Vec<_>>().join("&")
    } else {
        query.to_string()
    }
}

/// Extracts auth token from HeaderMap or query string.
pub fn extract_token(headers: &HeaderMap, query: &str) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .or_else(|| {
            query.split('&').find_map(|pair| {
                if let Some((k, v)) = pair.split_once('=') {
                    if k == "token" || k == "Authorization" {
                        if let Ok(decoded) = urlencoding::decode(v) {
                            Some(decoded.into_owned())
                        } else {
                            Some(v.to_string())
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_query() {
        assert_eq!(sanitize_query(""), "");
        assert_eq!(sanitize_query("foo=bar"), "foo=bar");
        assert_eq!(sanitize_query("token=xyz"), "token=[REDACTED]");
        assert_eq!(sanitize_query("Authorization=abc"), "Authorization=[REDACTED]");
        assert_eq!(sanitize_query("foo=bar&token=xyz&baz=123"), "foo=bar&token=[REDACTED]&baz=123");
        assert_eq!(sanitize_query("Authorization=123&foo=bar"), "Authorization=[REDACTED]&foo=bar");
    }

    #[test]
    fn test_extract_token() {
        // 1. From header
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer my-secret-token".parse().unwrap());
        assert_eq!(extract_token(&headers, ""), Some("my-secret-token".to_string()));

        // 2. From query param
        let headers_empty = HeaderMap::new();
        assert_eq!(extract_token(&headers_empty, "token=my-secret-token"), Some("my-secret-token".to_string()));
        assert_eq!(extract_token(&headers_empty, "Authorization=my-secret-token-2"), Some("my-secret-token-2".to_string()));
        assert_eq!(extract_token(&headers_empty, "token=encoded%20token"), Some("encoded token".to_string()));
        assert_eq!(extract_token(&headers_empty, "foo=bar&token=xyz&baz=123"), Some("xyz".to_string()));

        // 3. Header takes priority
        assert_eq!(extract_token(&headers, "token=query-token"), Some("my-secret-token".to_string()));

        // 4. No token
        assert_eq!(extract_token(&headers_empty, "foo=bar&baz=123"), None);
    }
}
