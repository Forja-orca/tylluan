use axum::{
    Json,
    extract::{Request, State},
    http::{StatusCode, HeaderMap},
    middleware::Next,
    response::{IntoResponse, Response},
};
use tracing::{info, warn};
use std::sync::Arc;
use std::time::{Duration, Instant};
use crate::transport::http::HttpState;
use crate::security::guard::ExecutionGuard;

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
    
    // 1. Explicit Public Bypass (events and SSE streams are no longer public)
    if uri == "/health" || uri == "/discovery" || uri == "/ui" || uri == "/" || uri == "/dashboard" ||
       uri.starts_with("/js/") || uri.starts_with("/css/") || uri.starts_with("/img/") || uri.starts_with("/fonts/") ||
       uri.ends_with(".js") || uri.ends_with(".css") || uri.ends_with(".html") || uri.ends_with(".png") || uri.ends_with(".svg") {
        return next.run(request).await;
    }

    // 2. Token in Query (SSE/MCP Compatibility) - Securely parsed and compared in constant-time
    if let Some(expected) = &state.auth_token {
        let has_valid_token = query.split('&').any(|pair| {
            if let Some((k, v)) = pair.split_once('=') {
                (k == "token" || k == "Authorization") && ExecutionGuard::secure_compare(v, expected)
            } else {
                false
            }
        });
        if has_valid_token {
            return next.run(request).await;
        }
    }

    // 3. Dev Mode Bypass
    if state.dev_mode.unwrap_or(false) {
        return next.run(request).await;
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

    // 4. Token Enforcement
    let expected_token = match &state.auth_token {
        Some(t) => t,
        None => {
            // If dev_mode was true, it would have returned at step 3.
            // If we are here and auth_token is None, it means the system is misconfigured
            // or we are in a state that MUST be unauthorized.
            warn!("🚫 AUTH_FAILURE: No Master Token configured in kernel and dev_mode is false.");
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "unauthorized",
                    "message": "Kernel is in SECURE mode but no Master Token is set. Configure TYLLUAN_TOKEN."
                })),
            ).into_response();
        }
    };

    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if let Some(token) = auth_header.strip_prefix("Bearer ") {
        // Static master token
        if ExecutionGuard::secure_compare(token, expected_token) {
            return next.run(request).await;
        }
        // OAuth JWT issued by local OAuthServer
        if state.oauth.validate_bearer(token) {
            return next.run(request).await;
        }
    }

    warn!("🚫 Unauthorized request to {}", request.uri());
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({
            "error": "unauthorized",
            "message": "Valid Bearer token required. Check your .tylluan-token file."
        })),
    )
        .into_response()
}
