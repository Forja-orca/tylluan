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
    /// M31-P1: Bound agent_id for the request, resolved from the bearer token.
    /// Empty if no binding is configured (backwards compatible — any agent_id allowed).
    pub static ACL_AGENT_ID: String;
}

/// Get the current request's ACL role. Returns "admin" if unset (local/stdio access).
pub fn current_acl_role() -> String {
    ACL_ROLE.try_with(|r| r.clone()).unwrap_or_else(|_| "admin".to_string())
}

/// Get the bound agent_id for the current request.
/// Returns None if no token-agent binding is configured (any agent_id allowed).
pub fn current_bound_agent_id() -> Option<String> {
    ACL_AGENT_ID.try_with(|a| {
        let s = a.as_str();
        if s.is_empty() { None } else { Some(s.to_string()) }
    }).ok().flatten()
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

/// M31-P1: Resolve the bound agent_id for a token from ACL config.
/// Returns empty string if no binding exists (backwards compatible).
pub fn resolve_agent_id_for_token(token: &str, acl: &AclConfig) -> String {
    acl.token_agent_bindings.get(token).cloned().unwrap_or_default()
}

/// M31-P1: Check if an agent_id is allowed to call a specific tool.
/// Returns None if allowed, Some(error_message) if denied.
pub fn check_agent_id_tool_allowed(agent_id: &str, tool_name: &str, acl: &AclConfig) -> Option<String> {
    if let Some(perm) = acl.agent_permissions.get(agent_id) {
        if perm.denied_tools.iter().any(|t| t == tool_name) {
            return Some(format!("ACCESS_DENIED: agent '{agent_id}' is not allowed to use tool '{tool_name}'"));
        }
        if perm.scope == "read-only" && matches!(tool_name, "tylluan_remember" | "tylluan_do" | "tylluan_graph") {
            return Some(format!("ACCESS_DENIED: agent '{agent_id}' has read-only scope, cannot use tool '{tool_name}'"));
        }
    }
    None
}

/// M31-P1: Check if an agent's memory is isolated (only sees its own episodes).
pub fn agent_has_memory_isolation(agent_id: &str, acl: &AclConfig) -> bool {
    acl.agent_permissions.get(agent_id).map(|p| p.memory_isolation).unwrap_or(false)
}

/// Pure resolution logic: resolve ACL role from token, config, contract, and agent_id.
/// Extracted as a pure function for testability (M19-P5 / ADR-009).
///
/// Resolution order:
/// 1. Explicit token mapping in `acl.tokens` → use that role (always wins).
/// 2. Token resolves to `default_role` AND agent_id is supplied → consult
///    `agents_contract` for the agent's declared role if valid in `acl.roles`.
/// 3. Otherwise → `acl.default_role` (unchanged fallback).
pub fn resolve_acl_role_inner(
    token: Option<&str>,
    agent_id: Option<&str>,
    acl: &AclConfig,
    contract: &crate::security::agents_contract::AgentsContract,
) -> String {
    let base_role = match token {
        Some(tok) => resolve_role_for_token(tok, acl),
        None => acl.default_role.clone(),
    };

    // Step 1: Explicit token mapping always wins (token is in acl.tokens)
    if let Some(tok) = token {
        if acl.tokens.contains_key(tok) {
            return base_role;
        }
    }

    // Step 2: If we hit default_role AND have an agent_id, check the contract
    if base_role == acl.default_role {
        if let Some(aid) = agent_id {
            let contract_role = contract.get_role(aid);
            if let Some(declared_role) = contract_role {
                if acl.roles.contains_key(declared_role) || declared_role == "admin" {
                    return declared_role.to_string();
                }
            }
        }
    }

    base_role
}

/// Resolve ACL role from the current request state, bearer token, and optional agent_id.
async fn resolve_acl_role(
    state: &Arc<HttpState>,
    bearer_token: Option<&str>,
    agent_id: Option<&str>,
) -> String {
    let config = state.config.read().await;
    resolve_acl_role_inner(
        bearer_token,
        agent_id,
        &config.security.acl,
        &state.agents_contract,
    )
}

/// Paths that bypass bearer auth entirely, regardless of dev_mode. Kept as a
/// pure, testable function rather than inline in the middleware so a
/// regression like exempting an endpoint with no internal verification of its
/// own (found 2026-07-12: `/api/v1/gossip` was briefly exempted despite
/// `gossip_handler` having zero signature/identity check) gets caught by a
/// test instead of only by manual audit.
pub fn is_public_bypass_path(uri: &str) -> bool {
    uri == "/health" || uri == "/discovery" || uri == "/ui" || uri == "/" || uri == "/dashboard" ||
    uri.starts_with("/js/") || uri.starts_with("/css/") || uri.starts_with("/img/") || uri.starts_with("/fonts/") ||
    uri.ends_with(".js") || uri.ends_with(".css") || uri.ends_with(".html") || uri.ends_with(".png") || uri.ends_with(".svg") ||
    // sync/receive and sync/export do their own bearer-token check against the
    // approved-peers list internally -- exempting them here just moves the
    // check to the right place, it doesn't remove it.
    uri == "/api/v1/federation/sync/receive" || uri == "/api/v1/federation/sync/export" || uri == "/api/v1/federation/ping"
    // /api/v1/gossip must NOT be added here: gossip_handler has zero internal
    // verification (no signature/identity check on GossipEntry). Exempting it
    // would leave it completely open in production (dev_mode=false) -- anyone
    // reaching the port could inject arbitrary DHT routing entries. If gossip
    // ever needs to bypass bearer auth, it needs its own verification first
    // (e.g. Noise-signed envelopes, matching the federation sync pattern).
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

    // ─── Rate Limiting by IP (independent of client-controlled agent_id) ───
    // agent_id below comes from a client-supplied header/query param -- a
    // caller can omit it or rotate a fresh value on every request to fully
    // evade that limiter. This check is keyed by the actual TCP peer address
    // instead, so it can't be bypassed the same way. Bypass paths (health,
    // static assets, federation endpoints with their own auth) are exempt,
    // same as bearer auth itself.
    if !is_public_bypass_path(uri)
        && let Some(axum::extract::ConnectInfo(addr)) =
            request.extensions().get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
    {
        let ip_key = addr.ip().to_string();
        if let Err(reason) = state.ip_rate_limiter.check_and_record(&ip_key) {
            warn!("🚫 RATE_LIMIT: IP '{}' exceeded limit: {}", ip_key, reason);
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({
                    "error": "rate_limit",
                    "scope": "ip",
                    "retry_after_secs": 10
                })),
            ).into_response();
        }
    }

    // Determine if request is authorized and resolve ACL role
    let is_authorized = {
        // 1. Explicit Public Bypass, or 2. Dev Mode Bypass
        if is_public_bypass_path(uri) || state.dev_mode.unwrap_or(false) {
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

    if let Some(aid) = &agent_id {
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

    // Resolve ACL role + bound agent_id for this request
    let query_str = request.uri().query().unwrap_or("");
    let bearer_token = extract_token(&headers, query_str);

    let acl_role = resolve_acl_role(&state, bearer_token.as_deref(), agent_id.as_deref()).await;
    let acl_agent_id = {
        let config = state.config.read().await;
        let acl = &config.security.acl;
        match bearer_token {
            Some(ref token) => resolve_agent_id_for_token(token, acl),
            None => String::new(),
        }
    };

    ACL_ROLE.scope(acl_role, async move {
        ACL_AGENT_ID.scope(acl_agent_id, async move {
            next.run(request).await
        }).await
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
    fn test_gossip_endpoint_never_bypasses_auth() {
        // Regression test for the 2026-07-12 audit finding: /api/v1/gossip was
        // briefly exempted from bearer auth even though gossip_handler performs
        // zero verification of its own on incoming entries. This must stay
        // authenticated unless gossip_handler gains its own signature check.
        assert!(!is_public_bypass_path("/api/v1/gossip"));
    }

    #[test]
    fn test_federation_sync_endpoints_bypass_auth() {
        // These legitimately bypass the general middleware because they run
        // their own bearer-token check against the approved-peers list.
        assert!(is_public_bypass_path("/api/v1/federation/sync/receive"));
        assert!(is_public_bypass_path("/api/v1/federation/sync/export"));
        assert!(is_public_bypass_path("/api/v1/federation/ping"));
    }

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

    // M31-P1 (2026-07-25): these functions were implemented and wired weeks
    // ago (commit 53b7fac) but had zero direct unit coverage — only exercised
    // indirectly through handler-level integration paths. Added here as the
    // real remaining gap, not a new feature.

    use crate::config::AgentPermission;

    fn acl_with_permission(agent_id: &str, perm: AgentPermission) -> AclConfig {
        let mut acl = AclConfig::default();
        acl.agent_permissions.insert(agent_id.to_string(), perm);
        acl
    }

    #[test]
    fn test_agent_has_memory_isolation_true_when_configured() {
        let acl = acl_with_permission("alice", AgentPermission {
            scope: "read-write".to_string(),
            denied_tools: vec![],
            memory_isolation: true,
        });
        assert!(agent_has_memory_isolation("alice", &acl));
    }

    #[test]
    fn test_agent_has_memory_isolation_false_by_default() {
        let acl = AclConfig::default();
        assert!(!agent_has_memory_isolation("alice", &acl), "unconfigured agent must not be isolated");

        let acl_no_isolation = acl_with_permission("alice", AgentPermission {
            scope: "read-write".to_string(),
            denied_tools: vec![],
            memory_isolation: false,
        });
        assert!(!agent_has_memory_isolation("alice", &acl_no_isolation));
    }

    #[test]
    fn test_check_agent_id_tool_allowed_denies_listed_tool() {
        let acl = acl_with_permission("bob", AgentPermission {
            scope: "read-write".to_string(),
            denied_tools: vec!["tylluan_graph".to_string()],
            memory_isolation: false,
        });
        assert!(check_agent_id_tool_allowed("bob", "tylluan_graph", &acl).is_some());
        assert!(check_agent_id_tool_allowed("bob", "tylluan_recall", &acl).is_none());
    }

    #[test]
    fn test_check_agent_id_tool_allowed_denies_write_tools_for_readonly_scope() {
        let acl = acl_with_permission("readonly-bot", AgentPermission {
            scope: "read-only".to_string(),
            denied_tools: vec![],
            memory_isolation: false,
        });
        assert!(check_agent_id_tool_allowed("readonly-bot", "tylluan_remember", &acl).is_some());
        assert!(check_agent_id_tool_allowed("readonly-bot", "tylluan_do", &acl).is_some());
        assert!(check_agent_id_tool_allowed("readonly-bot", "tylluan_graph", &acl).is_some());
        assert!(check_agent_id_tool_allowed("readonly-bot", "tylluan_recall", &acl).is_none(), "read-only scope must still allow recall");
    }

    #[test]
    fn test_check_agent_id_tool_allowed_allows_unconfigured_agent() {
        let acl = AclConfig::default();
        assert!(check_agent_id_tool_allowed("nobody", "tylluan_remember", &acl).is_none(), "backward compat: no config means no restriction");
    }

    #[test]
    fn test_resolve_agent_id_for_token_returns_empty_when_unbound() {
        let acl = AclConfig::default();
        assert_eq!(resolve_agent_id_for_token("some-token", &acl), "");
    }

    #[test]
    fn test_resolve_agent_id_for_token_returns_bound_agent() {
        let mut acl = AclConfig::default();
        acl.token_agent_bindings.insert("tok-123".to_string(), "alice".to_string());
        assert_eq!(resolve_agent_id_for_token("tok-123", &acl), "alice");
    }

    // ── M19-P5: AgentsContract resolution tests ──────────────────────────

    use crate::security::agents_contract::{AgentsContract, AgentContractEntry};

    fn contract_with_entry(agent_id: &str, role: &str) -> AgentsContract {
        let mut agents = std::collections::HashMap::new();
        agents.insert(agent_id.to_string(), AgentContractEntry {
            role: role.to_string(),
            description: String::new(),
        });
        AgentsContract { agents }
    }

    fn acl_with_role(role_name: &str, guilds: Vec<&str>) -> AclConfig {
        let mut roles = std::collections::HashMap::new();
        roles.insert(role_name.to_string(), guilds.iter().map(|s| s.to_string()).collect());
        AclConfig { default_role: "viewer".to_string(), roles, ..Default::default() }
    }

    #[test]
    fn test_resolve_acl_role_inner_explicit_token_wins() {
        // Explicitly-mapped token must always take precedence over any contract role.
        let contract = contract_with_entry("deep", "admin");
        let mut acl = acl_with_role("contributor", vec!["bash", "git"]);
        acl.tokens.insert("tok-admin".to_string(), "admin".to_string());
        acl.default_role = "viewer".to_string();

        let role = resolve_acl_role_inner(Some("tok-admin"), Some("deep"), &acl, &contract);
        assert_eq!(role, "admin", "explicit token mapping must win over contract");
    }

    #[test]
    fn test_resolve_acl_role_inner_contract_applied_when_default_token() {
        // Unmapped token + contract entry for agent_id → use contract role.
        let contract = contract_with_entry("deep", "contributor");
        let acl = acl_with_role("contributor", vec!["bash", "git"]);

        let role = resolve_acl_role_inner(Some("some-generic-token"), Some("deep"), &acl, &contract);
        assert_eq!(role, "contributor", "contract role must apply when token is not explicitly mapped");
    }

    #[test]
    fn test_resolve_acl_role_inner_no_contract_no_agent_falls_back() {
        let contract = AgentsContract::empty();
        let acl = acl_with_role("contributor", vec!["bash"]);

        let role = resolve_acl_role_inner(Some("unknown-token"), None, &acl, &contract);
        assert_eq!(role, "viewer", "must fall back to default_role when no contract match");
    }

    #[test]
    fn test_resolve_acl_role_inner_contract_applied_with_no_token() {
        // No bearer token at all + agent_id with contract entry → use contract role.
        let contract = contract_with_entry("ci-bot", "contributor");
        let acl = acl_with_role("contributor", vec!["bash"]);

        let role = resolve_acl_role_inner(None, Some("ci-bot"), &acl, &contract);
        assert_eq!(role, "contributor", "contract must apply even without a bearer token");
    }

    #[test]
    fn test_resolve_acl_role_inner_contract_unknown_agent_id_falls_back() {
        let contract = contract_with_entry("known-agent", "admin");
        let acl = acl_with_role("contributor", vec!["bash"]);

        let role = resolve_acl_role_inner(Some("tok"), Some("unknown-agent"), &acl, &contract);
        assert_eq!(role, "viewer", "unlisted agent_id must get default_role");
    }

    #[test]
    fn test_resolve_acl_role_inner_invalid_contract_role_falls_back() {
        // Agent declares a role that doesn't exist in acl.roles → fails safe to default_role.
        let contract = contract_with_entry("deep", "nonexistent-role");
        let acl = acl_with_role("contributor", vec!["bash"]);

        let role = resolve_acl_role_inner(Some("tok"), Some("deep"), &acl, &contract);
        assert_eq!(role, "viewer", "nonexistent contract role must fall back safely");
    }

    #[test]
    fn test_resolve_acl_role_inner_missing_file_is_noop() {
        // Empty contract (equivalent to missing .tylluan/agents.toml) must not change behavior.
        let contract = AgentsContract::empty();
        let acl = acl_with_role("contributor", vec!["bash"]);

        let role = resolve_acl_role_inner(Some("tok"), Some("any-agent"), &acl, &contract);
        assert_eq!(role, "viewer", "empty contract must not apply any role");
    }
}
