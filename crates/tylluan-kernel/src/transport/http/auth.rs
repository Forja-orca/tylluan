use axum::{
    Json,
    extract::{Request, State},
    http::{StatusCode, HeaderMap, header::{HOST, ORIGIN, WWW_AUTHENTICATE}},
    middleware::Next,
    response::{IntoResponse, Response},
};
use tokio::task_local;
use tracing::{info, warn};
use std::sync::Arc;
use std::time::{Duration, Instant};
use crate::transport::http::HttpState;
use crate::transport::http::oauth::{OAuthState, SCOPE_MCP, SCOPE_OPERATOR};
use crate::security::guard::ExecutionGuard;
use crate::config::AclConfig;

/// Explicit authorization context for a request or internal invocation.
/// The absence of a context is deliberately not equivalent to `admin`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AclContext {
    /// HTTP/SSE passed bearer authentication and has a resolved ACL role.
    HttpAuthenticated { role: String },
    /// The kernel's local MCP stdio channel.
    StdioLocalTrusted,
    /// A trusted in-process or CLI invocation.
    InternalTrusted,
    /// An explicit operator-selected development bypass.
    DevBypass,
}

impl AclContext {
    /// Resolve the role represented by this explicit context.
    pub fn role(&self) -> Option<&str> {
        match self {
            Self::HttpAuthenticated { role } => Some(role.as_str()),
            Self::StdioLocalTrusted | Self::InternalTrusted | Self::DevBypass => Some("admin"),
        }
    }
}

task_local! {
    /// Current ACL role for the request, set by bearer_auth_middleware.
    /// Absence is not a role and is handled fail-closed.
    pub static ACL_ROLE: String;
    /// Explicit transport/invocation context. Trusted contexts must be
    /// installed by their boundary; it has no implicit default.
    pub static ACL_CONTEXT: AclContext;
    /// M31-P1: Bound agent_id for the request, resolved from the bearer token.
    /// Empty if no binding is configured (backwards compatible — any agent_id allowed).
    pub static ACL_AGENT_ID: String;
    /// ADR-019/OAuth Fase 1(b): HOW the request was authenticated. The
    /// operator-class gate requires this to be MasterToken/DevBypass;
    /// absence is fail-closed.
    pub static ACL_AUTH_KIND: AclAuthKind;
}

/// How an HTTP request was authenticated (ADR-019/OAuth Fase 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AclAuthKind {
    /// The static master token (env/`.tylluan-token`) — the operator credential.
    MasterToken,
    /// The master token presented via `?token=` query-string. Authenticates the
    /// request (dashboard `/sse` + sandbox files), but does NOT satisfy the
    /// operator class: operator routes require the token in the Authorization
    /// header (E1, Jose via TL turno 1237).
    MasterTokenQuery,
    /// A JWT issued by the local OAuth server.
    OAuthJwt,
    /// D-34: per-process credential the kernel issues to ONE stdio guild.
    /// It authenticates agent-class routes and NEVER satisfies the operator
    /// class (`operator_gate_allows`); the single audited exception is
    /// `POST /api/v1/silva/edge` (`guild_operator_exception`).
    GuildToken,
    /// dev_mode=true local bypass.
    DevBypass,
    /// Endpoint with internal verification (federation peers, mcp probe…).
    PublicBypass,
}

/// Current request's auth kind. Missing context is fail-closed (None).
pub fn current_auth_kind() -> Option<AclAuthKind> {
    ACL_AUTH_KIND.try_with(|k| *k).ok()
}

/// Get the explicit current ACL context. The middleware's ACL_ROLE task-local
/// is accepted as the authenticated HTTP context for existing protected
/// routes; local and internal transports must install ACL_CONTEXT explicitly.
pub fn current_acl_context() -> Option<AclContext> {
    ACL_CONTEXT.try_with(|ctx| ctx.clone()).ok().or_else(|| {
        ACL_ROLE.try_with(|role| AclContext::HttpAuthenticated { role: role.clone() }).ok()
    })
}

/// Get the current request's ACL role. Missing context is fail-closed.
pub fn current_acl_role() -> Option<String> {
    current_acl_context().and_then(|ctx| ctx.role().map(str::to_owned))
}

/// Derive the explicit ACL context required at a transport boundary.
pub fn context_for_channel(channel: &tylluan_common::types::Channel) -> Option<AclContext> {
    use tylluan_common::types::Channel;

    match channel {
        Channel::Stdio => Some(AclContext::StdioLocalTrusted),
        Channel::Cli | Channel::Local => Some(AclContext::InternalTrusted),
        Channel::Http { authenticated: true } | Channel::Sse { authenticated: true } => {
            current_acl_role().map(|role| AclContext::HttpAuthenticated { role })
        }
        Channel::Http { authenticated: false }
        | Channel::Sse { authenticated: false }
        | Channel::Unknown(_) => None,
    }
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
    if let Some(tok) = token
        && acl.tokens.contains_key(tok) {
            return base_role;
        }

    // Step 2: If we hit default_role AND have an agent_id, check the contract
    if base_role == acl.default_role
        && let Some(aid) = agent_id {
            let contract_role = contract.get_role(aid);
            if let Some(declared_role) = contract_role
                && (acl.roles.contains_key(declared_role) || declared_role == "admin") {
                    return declared_role.to_string();
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
    // Fail-closed for /api/ routes: only explicitly designated endpoints with internal verification qualify
    if uri.starts_with("/api/") {
        return uri == "/api/v1/federation/sync/receive"
            || uri == "/api/v1/federation/sync/export"
            || uri == "/api/v1/federation/ping"
            || uri == "/api/v1/mcp/probe";
    }

    uri == "/health"
        || uri == "/discovery"
        || uri == "/ui"
        || uri == "/"
        || uri == "/dashboard"
        || uri == "/index.html"
        || uri == "/favicon.ico"
        || uri == "/favicon.svg"
        || uri.starts_with("/assets/")
        || uri.starts_with("/js/")
        || uri.starts_with("/css/")
        || uri.starts_with("/img/")
        || uri.starts_with("/fonts/")
}

// ── ADR-019/OAuth Fase 1(b): operator-class gate ────────────────────────────
//
// The class-“Operador” surfaces (management/execution) require the MASTER
// token (or the local dev bypass). An OAuth JWT — obtainable by any process
// in dev_mode — and peer bypasses never satisfy them. ONE table, unit-tested;
// see docs/internal/DESIGN_oauth_operator_2026-10.md §2/§3.

/// Paths/methods whose class is “Operador” (management + direct execution).
/// Pure and exhaustive: the single source of truth for the gate.
pub fn operator_class_required(method: &str, path: &str) -> bool {
    let m = method.to_ascii_uppercase();
    // Whole management areas
    if path.starts_with("/api/v1/admin/")
        || path.starts_with("/api/v1/maintenance/")
        || path == "/api/v1/config"
        || path.starts_with("/api/v1/config/")
        || path.starts_with("/api/v1/mcp/external")
        || path.starts_with("/api/v1/security/sentinel")
        || path == "/api/v1/inference/llama/start"
        || path == "/api/v1/inference/llama/stop"
        || (m == "POST" && path.starts_with("/api/v1/inference/points/"))
        || path.starts_with("/api/v1/autoresearch/")
        || path.starts_with("/api/v1/a2a/agents")
        || path.starts_with("/api/v1/dispatches/")
    {
        return true;
    }
    // Exact endpoints
    if path == "/api/v1/bash"
        || path == "/api/v1/doctor/repair"
        || path == "/api/v1/guild-start"
        || path == "/api/v1/setup-hint/apply"
        || path == "/api/v1/guilds/register"
        || path == "/api/v1/guilds/request"
        || path == "/api/v1/guilds/dispatch/execute"
        || path == "/api/v1/guilds/dispatch/remote"
        // T1253: operator-class diagnostic reads (audit + conflict views)
        || path == "/api/v1/decide/shadow"
        || path == "/api/v1/silva/conflicted"
    {
        return true;
    }
    // Guild lifecycle mutations (NOT tool calls — those stay agent-class)
    if m == "POST"
        && path.starts_with("/api/v1/guilds/")
        && (path.ends_with("/start")
            || path.ends_with("/stop")
            || path.ends_with("/reset-backoff")
            || path.ends_with("/test"))
    {
        return true;
    }
    // Federation MANAGEMENT mutations only (sync/receive/export keep their own auth)
    if path.starts_with("/api/v1/federation/peers") && m != "GET" {
        return true;
    }
    if path.starts_with("/api/v1/federation/sharing/") {
        return true;
    }
    // Operator-initiated federation sync runs (mutations on OUR state);
    // /sync/receive stays peer-class with its own bypass auth.
    if path == "/api/v1/federation/sync"
        || path == "/api/v1/federation/sync/pull"
        || path == "/api/v1/federation/sync/both"
    {
        return true;
    }
    // Config-like writes surfaced by the route inventory (TL 1210):
    // provider management, security scopes and project skills.
    if m == "POST"
        && (path == "/api/v1/inference/providers"
            || path == "/api/v1/security/scopes"
            || path == "/api/v1/skills")
    {
        return true;
    }
    // Silva destructive writes (adopted from the adversarial review of the design)
    if m == "DELETE" && path.starts_with("/api/v1/silva/nodes/") {
        return true;
    }
    // D-12: ASI06 quarantine. The GET exposes text the write gate judged
    // hostile and the POST undoes a security decision — both operator class.
    if path.starts_with("/api/v1/memory/quarantine") {
        return true;
    }
    if m == "POST" && (path == "/api/v1/silva/edge" || path == "/api/v1/silva/consolidate") {
        return true;
    }
    false
}

/// Which auth kinds satisfy the operator class. Fail-closed on absence.
/// E1 (turno 1237): el token maestro por query-string
/// (`AclAuthKind::MasterTokenQuery`) NO satisface la clase operador — las
/// rutas de `operator_class_required` exigen el token por CABECERA (403).
pub fn operator_gate_allows(kind: Option<AclAuthKind>) -> bool {
    matches!(
        kind,
        Some(AclAuthKind::MasterToken) | Some(AclAuthKind::DevBypass)
    )
}

/// D-68: the master credential is explicitly operator-class for ACL purposes.
/// It must not inherit `default_role` (which is now `guest`, not `admin`).
/// Query-string master remains authenticated but cannot satisfy the operator
/// gate; it still gets the operator ACL role for guild access.
pub fn acl_role_for_auth_kind(kind: AclAuthKind) -> Option<&'static str> {
    matches!(
        kind,
        AclAuthKind::MasterToken | AclAuthKind::MasterTokenQuery | AclAuthKind::DevBypass
    )
    .then_some("admin")
}

/// D-34: the ONE audited exception where a guild credential may cross into the
/// operator class (Jose/TL 2026-10-23, option A). `guilds/core/silva_utils.py`
/// writes knowledge triples through this route and no non-operator path exists
/// yet; every use is recorded in `guild_audit_log` by the handler. Any other
/// operator-class route keeps returning 403 to a guild (see
/// `d34_guild_credential_has_exactly_one_operator_exception` and the live sweep
/// in `tests/cola_d34_acceptance.rs`).
///
/// Retirement: when `write_edge` has a non-operator route, delete this function
/// and its use in `operator_gate_middleware`; the guild-side helper must stop
/// writing triples through `/api/v1/silva/edge` at the same time.
pub fn guild_operator_exception(method: &str, path: &str) -> bool {
    method.eq_ignore_ascii_case("POST") && path == "/api/v1/silva/edge"
}

/// Resolve the guild that issued a bearer, if it is a guild credential.
fn guild_authenticated_guild(token: Option<&str>) -> Option<String> {
    token.and_then(crate::security::guild_credential::authenticate)
}

/// Fase 2 (D-3): desafío de un 401 (RFC 6750 + RFC 9728). Apunta al documento
/// del recurso protegido para que el cliente descubra el AS sin configuración
/// previa, y pide el único scope que un cliente MCP necesita.
///
/// El origen del `resource_metadata` es el de la petición cuando llegó por un
/// alias de loopback (`127.0.0.1`, `[::1]`): si anunciáramos `localhost`, un
/// cliente que se configuró con `127.0.0.1` seguiría el enlace a `localhost`,
/// que en Windows resuelve primero a `::1` mientras el kernel solo escucha en
/// IPv4, y no podría reconectar tras un reinicio (se invalidan los JWT).
pub fn bearer_challenge(oauth: &OAuthState, headers: &HeaderMap) -> String {
    format!(
        "Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource\", scope=\"{SCOPE_MCP}\"",
        crate::transport::http::oauth::request_origin(oauth, headers)
    )
}

/// Fase 2 (D-3): desafío de scope insuficiente. MCP exige
/// `error="insufficient_scope"` y `scope=...` con TODOS los scopes necesarios
/// en un único desafío (no una respuesta por scope).
pub fn insufficient_scope_challenge(
    oauth: &OAuthState,
    headers: &HeaderMap,
    scopes: &[&str],
) -> String {
    format!(
        "Bearer error=\"insufficient_scope\", scope=\"{}\", resource_metadata=\"{}/.well-known/oauth-protected-resource\"",
        scopes.join(" "),
        crate::transport::http::oauth::request_origin(oauth, headers)
    )
}

/// Layer applied INSIDE the bearer middleware (which installs ACL_AUTH_KIND):
/// refuses non-master auth on operator-class paths with 403.
pub async fn operator_gate_middleware(
    State(state): State<Arc<HttpState>>,
    request: Request,
    next: Next,
) -> Response {
    let method = request.method().to_string();
    let path = request.uri().path().to_string();
    let guild_exception = matches!(current_auth_kind(), Some(AclAuthKind::GuildToken))
        && guild_operator_exception(&method, &path);
    if operator_class_required(&method, &path)
        && !operator_gate_allows(current_auth_kind())
        && !guild_exception
    {
        warn!(
            "🚫 OPERATOR_REQUIRED: refusing {:?} auth on {} {}",
            current_auth_kind(),
            method,
            path
        );
        // Fase 2 (D-3): un JWT OAuth válido pero de clase `mcp` recibe el
        // desafío de scope (MCP: 403 + WWW-Authenticate insufficient_scope).
        // Otros medios (token por query-string, ausencia) no se arreglan
        // pidiendo un scope, así que conservan el 403 genérico.
        if matches!(current_auth_kind(), Some(AclAuthKind::OAuthJwt)) {
            return (
                StatusCode::FORBIDDEN,
                [(
                    WWW_AUTHENTICATE,
                    insufficient_scope_challenge(&state.oauth, request.headers(), &[SCOPE_OPERATOR]),
                )],
                Json(serde_json::json!({
                    "error": "insufficient_scope",
                    "scope": SCOPE_OPERATOR,
                    "message": "This endpoint requires the operator class. The OAuth token presented has scope=mcp only."
                })),
            )
                .into_response();
        }
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "operator_required",
                "message": "This endpoint requires the master operator token in the Authorization header. \
                    OAuth tokens, query-string tokens and peer bypasses are not accepted."
            })),
        )
            .into_response();
    }
    next.run(request).await
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

    // Determine if request is authorized, HOW it was authenticated (Fase 1),
    // and resolve ACL role
    let (is_authorized, auth_kind) = {
        // 1. Explicit Public Bypass
        if is_public_bypass_path(uri) {
            (true, AclAuthKind::PublicBypass)
        }
        // 2. Dev Mode Bypass (localhost development only)
        else if state.dev_mode.unwrap_or(false) {
            (true, AclAuthKind::DevBypass)
        }
        // 3. Token Authentication (Header or Query)
        else if let Some(expected) = &state.auth_token {
            // A. Check Bearer Token in Authorization Header
            let auth_header = headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");

            // D-34: a guild credential authenticates as its own class. It is
            // resolved from the header only (never the query string) so a guild
            // can never ride the master token or the OAuth `mcp` scope.
            let mut guild_identity: Option<String> = None;
            let (has_master_bearer, has_oauth_bearer) = if let Some(bearer) = auth_header.strip_prefix("Bearer ") {
                guild_identity = crate::security::guild_credential::authenticate(bearer);
                (
                    ExecutionGuard::secure_compare(bearer, expected),
                    state.oauth.validate_bearer(bearer),
                )
            } else {
                (false, false)
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

            if has_master_bearer {
                (true, AclAuthKind::MasterToken)
            } else if has_valid_query {
                // E1 (turno 1237): el token por query-string autentica, pero
                // el MEDIO no satisface la clase operador (solo cabecera).
                (true, AclAuthKind::MasterTokenQuery)
            } else if guild_identity.is_some() {
                (true, AclAuthKind::GuildToken)
            } else if has_oauth_bearer {
                (true, AclAuthKind::OAuthJwt)
            } else {
                // Irrelevant when denied; kept for type completeness.
                (false, AclAuthKind::MasterToken)
            }
        }
        else {
            warn!("🚫 AUTH_FAILURE: No Master Token configured and dev_mode is false.");
            (false, AclAuthKind::MasterToken)
        }
    };

    if !is_authorized {
        warn!("🚫 Unauthorized request to {}", request.uri());
        let no_credentials = headers.get("authorization").is_none()
            && !query.split('&').any(|pair| {
                pair.starts_with("token=") || pair.starts_with("Authorization=")
            });
        return (
            StatusCode::UNAUTHORIZED,
            [(WWW_AUTHENTICATE, bearer_challenge(&state.oauth, request.headers()))],
            Json(serde_json::json!({
                "error": "unauthorized",
                "code": if no_credentials { "missing_token" } else { "invalid_token" },
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

    // D-34: a guild credential is identified by the guild that minted it, not
    // by the ACL token tables (it is created at spawn and lives in memory).
    // Its ACL role is `default_role` -- identical to what the guild had with
    // the master token -- because the privilege reduction happens at the
    // operator gate, not in the per-guild ACL maps. `agent_id` is stamped as
    // `guild:<name>` so rate limits, audit rows and memory attribution stay
    // per guild.
    let guild_identity = if auth_kind == AclAuthKind::GuildToken {
        guild_authenticated_guild(bearer_token.as_deref())
    } else {
        None
    };
    let acl_role = if let Some(role) = acl_role_for_auth_kind(auth_kind) {
        role.to_string()
    } else if guild_identity.is_some() {
        let config = state.config.read().await;
        config.security.acl.default_role.clone()
    } else {
        resolve_acl_role(&state, bearer_token.as_deref(), agent_id.as_deref()).await
    };
    let acl_agent_id = if let Some(ref guild) = guild_identity {
        format!("guild:{guild}")
    } else {
        let config = state.config.read().await;
        let acl = &config.security.acl;
        match bearer_token {
            Some(ref token) => resolve_agent_id_for_token(token, acl),
            None => String::new(),
        }
    };

    ACL_AUTH_KIND.scope(auth_kind, async move {
        ACL_ROLE.scope(acl_role, async move {
            ACL_AGENT_ID.scope(acl_agent_id, async move {
                next.run(request).await
            }).await
        }).await
    }).await
}

// ── T-DSH-86: DNS-rebinding defense for the MCP/SSE HTTP endpoints ──────────
//
// The MCP spec (2025-06-18) requires servers to validate the `Origin` header on
// incoming connections. Only `/messages`, `/mcp`, `/api/v1/mcp`, `/sse` and
// `/sse/download` are guarded; federation/p2p and the rest of the API keep
// their existing behavior. This runs before the CORS layer and before auth.

/// Endpoints covered by [`validate_mcp_origin_host_middleware`].
pub const MCP_ORIGIN_GUARD_PATHS: [&str; 5] =
    ["/messages", "/mcp", "/api/v1/mcp", "/sse", "/sse/download"];

/// Host part of an `Origin`/`Host` authority (`[::1]:3030` → `::1`,
/// `127.0.0.1:47004` → `127.0.0.1`).
fn hostname_from_authority(authority: &str) -> &str {
    if let Some(stripped) = authority.strip_prefix('[') {
        stripped.split(']').next().unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    }
}

/// `127.0.0.1`, `localhost` or `::1` (case-insensitive).
pub(crate) fn is_loopback_hostname(host: &str) -> bool {
    matches!(
        host.to_ascii_lowercase().as_str(),
        "127.0.0.1" | "localhost" | "::1"
    )
}

/// `http(s)://<loopback>[:port][/...]` — any port is accepted.
fn is_loopback_origin(origin: &str) -> bool {
    let Some((scheme, rest)) = origin.split_once("://") else {
        return false;
    };
    if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") {
        return false;
    }
    let authority = rest.split('/').next().unwrap_or("");
    !authority.is_empty() && is_loopback_hostname(hostname_from_authority(authority))
}

/// Loopback origins are always accepted; `allowed_origins` entries are an
/// explicit escape hatch for non-loopback callers (default: empty). `null`
/// (sandboxed page / opaque origin) is never accepted, not even if listed.
fn origin_is_allowed(origin: &str, allowed: &[String]) -> bool {
    let origin = origin.trim();
    if origin.is_empty() || origin.eq_ignore_ascii_case("null") {
        return false;
    }
    is_loopback_origin(origin)
        || allowed
            .iter()
            .any(|entry| entry.trim().eq_ignore_ascii_case(origin))
}

/// Loopback hosts and the configured host are accepted, plus `allowed_hosts`
/// entries (hostname or authority form). Only called when the kernel itself is
/// bound to loopback.
fn host_is_allowed(host_header: &str, configured_host: &str, allowed: &[String]) -> bool {
    let host = hostname_from_authority(host_header);
    is_loopback_hostname(host)
        || host.eq_ignore_ascii_case(configured_host.trim())
        || allowed.iter().any(|entry| {
            let entry = entry.trim();
            entry.eq_ignore_ascii_case(host_header)
                || hostname_from_authority(entry).eq_ignore_ascii_case(host)
        })
}

fn forbidden_origin_host(code: &str, message: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({ "error": code, "message": message })),
    )
        .into_response()
}

/// T-DSH-86: rejects DNS-rebinding attempts against the MCP/SSE endpoints.
///
/// - `Origin` present and not loopback → 403 (also covers `OPTIONS` preflight
///   and `Origin: null`).
/// - `Host` present and not loopback/configured/allowed while the kernel is
///   bound to loopback → 403.
/// - No `Origin` (Claude Code, mcp-remote, agents, curl) → passes untouched.
///
/// No exemption for authenticated callers: a valid bearer token does not make
/// an arbitrary web origin safe.
pub async fn validate_mcp_origin_host_middleware(
    State(state): State<Arc<HttpState>>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();
    // T-DSH-87: the mutating REST surface (`/api/v1/*`, any method but GET/HEAD)
    // gets the same guard. A browser "simple" POST (no body or text/plain) needs
    // no preflight, so CORS alone does not stop it from purging memory or
    // reloading guilds. Non-browser clients send no Origin and pass untouched.
    let guarded = MCP_ORIGIN_GUARD_PATHS.contains(&path.as_str())
        || (path.starts_with("/api/v1/")
            && !matches!(
                *request.method(),
                axum::http::Method::GET | axum::http::Method::HEAD
            ));
    if !guarded {
        return next.run(request).await;
    }
    let (configured_host, allowed_origins, allowed_hosts) = {
        let config = state.config.read().await;
        (
            config.nexus.host.clone(),
            config.nexus.allowed_origins.clone(),
            config.nexus.allowed_hosts.clone(),
        )
    };
    if let Some(origin) = request.headers().get(ORIGIN) {
        let origin = origin.to_str().unwrap_or("");
        if !origin_is_allowed(origin, &allowed_origins) {
            warn!("MCP origin guard: rejected Origin '{origin}' on {path}");
            return forbidden_origin_host(
                "forbidden_origin",
                "Origin not allowed for MCP/SSE endpoints",
            );
        }
    }
    if is_loopback_hostname(&configured_host)
        && let Some(host) = request.headers().get(HOST)
    {
        let host = host.to_str().unwrap_or("");
        if !host_is_allowed(host, &configured_host, &allowed_hosts) {
            warn!("MCP origin guard: rejected Host '{host}' on {path}");
            return forbidden_origin_host(
                "forbidden_host",
                "Host not allowed for loopback-bound MCP/SSE endpoints",
            );
        }
    }
    next.run(request).await
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
///
/// The query-string fallback (`?token=` / `?Authorization=`) exists only for
/// SSE clients (dashboard, some MCP clients) that cannot set a custom header
/// on an `EventSource` connection. External audit (2026-08-12) correctly
/// flagged this as a real exposure surface: a query-string token ends up in
/// reverse-proxy access logs and browser history in a way a header never
/// does. `sanitize_query()` already redacts it from *our own* logs; the
/// `warn!` below makes every other use visible in the operator's own logs
/// too, so the header path stays the one to actually prefer.
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
                        warn!(
                            "extract_token(): auth token supplied via query string, not header -- \
                             this leaks into reverse-proxy access logs and browser history. \
                             Prefer 'Authorization: Bearer <token>' where the client can set it."
                        );
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
    fn test_static_assets_and_prefixes_bypass_auth() {
        assert!(is_public_bypass_path("/assets/index-D7b39a.js"));
        assert!(is_public_bypass_path("/assets/vendor.css"));
        assert!(is_public_bypass_path("/js/main.js"));
        assert!(is_public_bypass_path("/css/app.css"));
        assert!(is_public_bypass_path("/index.html"));
        assert!(is_public_bypass_path("/favicon.ico"));
        assert!(is_public_bypass_path("/favicon.svg"));
        assert!(is_public_bypass_path("/health"));
        assert!(is_public_bypass_path("/ui"));
        assert!(is_public_bypass_path("/dashboard"));
        assert!(is_public_bypass_path("/"));
    }

    #[test]
    fn test_api_routes_never_bypass_by_file_extension() {
        // Anti-regression: arbitrary API routes ending in .js/.html/.css must never bypass auth
        assert!(!is_public_bypass_path("/api/v1/bash.html"));
        assert!(!is_public_bypass_path("/api/v1/export.js"));
        assert!(!is_public_bypass_path("/api/v1/user.css"));
        assert!(!is_public_bypass_path("/api/v1/image.png"));
        assert!(!is_public_bypass_path("/api/v1/data.svg"));
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
            wake: None,
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

    #[test]
    fn test_current_acl_role_fails_closed_outside_scope() {
        assert_eq!(current_acl_role(), None);
        assert_eq!(current_acl_context(), None);
    }

    #[test]
    fn test_context_for_channel_requires_authenticated_http_context() {
        use tylluan_common::types::Channel;

        assert!(context_for_channel(&Channel::Http { authenticated: false }).is_none());
        assert!(context_for_channel(&Channel::Unknown("test".into())).is_none());
        assert_eq!(context_for_channel(&Channel::Stdio), Some(AclContext::StdioLocalTrusted));
        assert_eq!(context_for_channel(&Channel::Local), Some(AclContext::InternalTrusted));
    }

    #[tokio::test]
    async fn test_explicit_trusted_context_is_required_for_admin_role() {
        ACL_CONTEXT
            .scope(AclContext::StdioLocalTrusted, async {
                assert_eq!(current_acl_role().as_deref(), Some("admin"));
            })
            .await;
    }
}

#[cfg(test)]
mod fase1_operator_tests {
    use super::*;

    #[test]
    fn operator_table_covers_management_and_excludes_agent_surfaces() {
        // Operator class (management / execution):
        // T-DSH-114: el interruptor del centinela es clase operador (config).
        assert!(operator_class_required("GET", "/api/v1/security/sentinel"));
        assert!(operator_class_required("POST", "/api/v1/security/sentinel/mode"));
        assert!(operator_class_required("POST", "/api/v1/admin/shutdown"));
        assert!(operator_class_required("POST", "/api/v1/maintenance/purge"));
        assert!(operator_class_required("PUT", "/api/v1/federation/peers/x/permissions"));
        assert!(operator_class_required("POST", "/api/v1/federation/peers/x/approve"));
        assert!(operator_class_required("DELETE", "/api/v1/federation/peers/x"));
        assert!(operator_class_required("POST", "/api/v1/federation/sharing/enable"));
        assert!(operator_class_required("POST", "/api/v1/bash"));
        assert!(operator_class_required("POST", "/api/v1/guilds/foo/start"));
        assert!(operator_class_required("POST", "/api/v1/guilds/foo/stop"));
        assert!(operator_class_required("POST", "/api/v1/mcp/external/discover"));
        assert!(operator_class_required("DELETE", "/api/v1/silva/nodes/n1"));
        assert!(operator_class_required("POST", "/api/v1/silva/edge"));
        assert!(operator_class_required("POST", "/api/v1/silva/consolidate"));
        assert!(operator_class_required("POST", "/api/v1/dispatches/42/approve"));
        // D-12: quarantine list exposes hostile text, release undoes a security
        // decision — both operator-class, GET included.
        assert!(
            operator_class_required("GET", "/api/v1/memory/quarantine"),
            "quarantine list must require the operator class"
        );
        assert!(
            operator_class_required("POST", "/api/v1/memory/quarantine/abc/release"),
            "quarantine release must require the operator class"
        );
        // T1253: operator-class diagnostic reads:
        assert!(
            operator_class_required("GET", "/api/v1/decide/shadow"),
            "shadow audit read is operator-class"
        );
        assert!(
            operator_class_required("GET", "/api/v1/silva/conflicted"),
            "conflicted-nodes read is operator-class"
        );
        // NOT operator (agent-class surfaces):
        assert!(!operator_class_required("GET", "/api/v1/federation/peers"));
        assert!(!operator_class_required("POST", "/api/v1/memory/write"));
        assert!(!operator_class_required("POST", "/api/v1/coloquio/channels/general/post"));
        assert!(!operator_class_required("POST", "/api/v1/guilds/foo/tools/git.status"));
        assert!(!operator_class_required("POST", "/api/v1/guilds/foo/call/git.status"));
        assert!(!operator_class_required("POST", "/api/v1/silva/node"));
        assert!(!operator_class_required("GET", "/api/v1/silva/nodes/n1"));
        // Inventory-forced classifications (TL 1210):
        assert!(operator_class_required("POST", "/api/v1/config"), "bare /api/v1/config save");
        assert!(operator_class_required("POST", "/api/v1/federation/sync"));
        assert!(operator_class_required("POST", "/api/v1/federation/sync/pull"));
        assert!(operator_class_required("POST", "/api/v1/federation/sync/both"));
        assert!(!operator_class_required("POST", "/api/v1/federation/sync/receive"), "peer-class stays");
        assert!(operator_class_required("POST", "/api/v1/inference/providers"));
        assert!(operator_class_required("POST", "/api/v1/security/scopes"));
        assert!(operator_class_required("POST", "/api/v1/skills"));
        assert!(!operator_class_required("POST", "/api/v1/inference/providers/openai/test"));
        assert!(!operator_class_required("GET", "/api/v1/inference/providers"), "reads stay generic");
    }

    #[test]
    fn operator_gate_allows_only_master_and_dev() {
        assert!(operator_gate_allows(Some(AclAuthKind::MasterToken)));
        assert!(operator_gate_allows(Some(AclAuthKind::DevBypass)));
        assert!(!operator_gate_allows(Some(AclAuthKind::OAuthJwt)));
        assert!(!operator_gate_allows(Some(AclAuthKind::PublicBypass)));
        assert!(!operator_gate_allows(None), "absence is fail-closed");
        assert!(
            !operator_gate_allows(Some(AclAuthKind::GuildToken)),
            "D-34: a guild credential never satisfies the operator class"
        );
    }

    /// D-34: the exception is exactly one route, one method. A list, not a
    /// prefix: `silva/edge/search` (read-only) and `silva/consolidate` stay
    /// operator-only.
    #[test]
    fn d34_guild_credential_has_exactly_one_operator_exception() {
        assert!(guild_operator_exception("POST", "/api/v1/silva/edge"));
        assert!(guild_operator_exception("post", "/api/v1/silva/edge"));
        assert!(!guild_operator_exception("GET", "/api/v1/silva/edge"));
        assert!(!guild_operator_exception("POST", "/api/v1/silva/edge/search"));
        assert!(!guild_operator_exception("POST", "/api/v1/silva/consolidate"));
        assert!(!guild_operator_exception("POST", "/api/v1/silva/node"));
        assert!(!guild_operator_exception("POST", "/api/v1/admin/shutdown"));
        assert!(!guild_operator_exception("POST", "/api/v1/maintenance/purge"));
    }

    /// Structural inventory (TL 1210): EVERY mutating route in the api_v1
    /// routing table must be explicitly classified — Operador through
    /// `operator_class_required`, or Agente/Par through the documented
    /// allowlist below (this test is the single classification record).
    /// A new unclassified mutating route FAILS here, so the blacklist in
    /// `operator_class_required` cannot silently leave new routes reachable
    /// by OAuth JWTs.
    #[test]
    fn route_inventory_every_mutating_route_is_classified() {
        const ROUTES_RS: &str = include_str!("api_v1/routes.rs");
        // Agente / Par / a-decidir — NON-operator mutating surfaces.
        // "a-decidir" marks the few the design (v2 §4) deferred to the TL.
        const AGENT_OR_PEER_CLASS: &[&str] = &[
            "/api/v1/do",
            "/api/v1/decide",
            "/api/v1/guilds/{guild_name}/tools/{tool_name}",
            "/api/v1/guilds/{guild_name}/call/{tool_name}",
            "/api/v1/silva/node",
            "/api/v1/silva/node/{id}/shareable",
            "/api/v1/silva/save-cluster-summary",
            "/api/v1/silva/communities",
            "/api/v1/silva/graphrag-trigger", // a-decidir
            "/api/v1/silva/analyze",          // any(): POST accepted, read-only
            "/api/v1/silva/edge/search",      // POST read-only search
            "/api/v1/sessions/resume",
            "/api/v1/sessions/{session_id}",
            "/api/v1/ingest",
            "/api/v1/ingest/upload",
            "/api/v1/memory/reindex",
            "/api/v1/memory/write",
            "/api/v1/memory/search", // any(): read-only
            "/api/v1/inference/providers/{name}/test", // a-decidir (ADR-020)
            "/api/v1/external-providers/{name}/test",  // a-decidir (ADR-020)
            "/api/v1/test-connection",
            "/api/v1/mailbox/send",
            "/api/v1/agent-memories/{agent_id}",
            "/api/v1/session-digest",
            "/api/v1/blackboard/plan",
            "/api/v1/blackboard/tasks/{msg_id}/done",
            "/api/v1/tylluan_graph",
            "/api/v1/federation/sync/receive", // Par (own bypass auth)
            "/api/v1/routing/anchors",
            "/api/v1/routing/anchors/reembed",
            "/api/v1/coloquio/channels",
            "/api/v1/coloquio/channels/{id}",
            "/api/v1/coloquio/channels/{id}/post",
            "/api/v1/coloquio/channels/{id}/read",
            "/api/v1/coloquio/channels/{id}/typing",
            "/api/v1/coloquio/repair-msgids",
            "/api/v1/coloquio/documents",
            "/api/v1/coloquio/documents/{id}",
            "/api/v1/coloquio/documents/{id}/append",
            "/api/v1/canvas/{channel}/nodes",
            "/api/v1/nodes/{agent_id}/register",
            "/api/v1/nodes/{agent_id}/send",
            "/api/v1/nodes/broadcast",
            "/api/v1/nodes/{agent_id}/program",
            "/api/v1/nodes/{agent_id}/unregister",
            "/api/v1/journal/{agent_id}/checkin",
            "/api/v1/agents/session/start",
            "/api/v1/agents/session/stop",
            "/api/v1/agents/heartbeat",
            "/api/v1/work-contracts",
            "/api/v1/work-contracts/{id}/tick",
            "/api/v1/work-contracts/{id}/deliver",
            "/api/v1/work-contracts/{id}/vote",
            "/api/v1/work-contracts/{id}/close",
            "/api/v1/work-contracts/{id}/context/decision",
            "/api/v1/work-contracts/{id}/context/pointer",
            "/api/v1/mesh/refresh", // a-decidir
            "/api/v1/gossip",       // a-decidir
            "/api/v1/eval/run",     // a-decidir
            "/api/v1/embed",        // a-decidir
            "/api/v1/stigmergy/traces",
        ];

        let mut unclassified: Vec<String> = Vec::new();
        let mut mutating_total = 0usize;
        let mut operator_total = 0usize;
        for line in ROUTES_RS.lines() {
            let Some(pos) = line.find(".route(\"") else { continue };
            let rest = &line[pos + 8..];
            let Some(end) = rest.find('"') else { continue };
            let path = &rest[..end];
            let handlers = &rest[end..];
            let mut methods: Vec<&str> = Vec::new();
            if handlers.contains("post(") { methods.push("POST"); }
            if handlers.contains("put(") { methods.push("PUT"); }
            if handlers.contains("delete(") { methods.push("DELETE"); }
            if handlers.contains("patch(") { methods.push("PATCH"); }
            if handlers.contains("any(") {
                methods.extend_from_slice(&["POST", "PUT", "DELETE", "PATCH"]);
            }
            if methods.is_empty() { continue; }
            mutating_total += 1;
            if methods.iter().any(|m| operator_class_required(m, path)) {
                operator_total += 1;
                continue;
            }
            if AGENT_OR_PEER_CLASS.contains(&path) { continue; }
            unclassified.push(path.to_string());
        }
        assert!(
            mutating_total >= 110,
            "parser sanity: expected the full mutating table (~121), saw {mutating_total}"
        );
        assert!(
            unclassified.is_empty(),
            "Unclassified mutating route(s) — classify each as Operador in \
             operator_class_required(), or add it to AGENT_OR_PEER_CLASS in this test:\n{}",
            unclassified.join("\n")
        );
        assert!(
            operator_total >= 50,
            "operator-class surfaces sanity (split too small): {operator_total} of {mutating_total}"
        );
    }
}

/// Fase 2 (D-3): desafios `WWW-Authenticate`. Funciones puras; la respuesta
/// HTTP completa (401/403 sobre el router real) vive en
/// `tests/oauth_f2_scope_challenges.rs`.
#[cfg(test)]
mod fase3_scope_tests {
    use super::*;

    fn oauth() -> OAuthState {
        OAuthState::new("http://localhost:3030/".to_string())
    }

    #[test]
    fn bearer_challenge_points_to_protected_resource_metadata() {
        assert_eq!(
            bearer_challenge(&oauth(), &HeaderMap::new()),
            "Bearer resource_metadata=\"http://localhost:3030/.well-known/oauth-protected-resource\", scope=\"mcp\""
        );
    }

    /// Un cliente que entró por 127.0.0.1 debe recibir un `resource_metadata` en
    /// 127.0.0.1 (no en localhost, que en Windows va primero a ::1 y el kernel
    /// solo escucha en IPv4); un `Host` hostil no cambia lo anunciado.
    #[test]
    fn challenge_reflects_loopback_alias_but_ignores_hostile_host() {
        use axum::http::header::HOST;
        let mut loopback = HeaderMap::new();
        loopback.insert(HOST, "127.0.0.1:3030".parse().unwrap());
        assert!(
            bearer_challenge(&oauth(), &loopback)
                .contains("resource_metadata=\"http://127.0.0.1:3030/.well-known/oauth-protected-resource\""),
            "{}",
            bearer_challenge(&oauth(), &loopback)
        );
        assert!(
            insufficient_scope_challenge(&oauth(), &loopback, &[SCOPE_OPERATOR])
                .contains("resource_metadata=\"http://127.0.0.1:3030/")
        );
        for host in ["evil.example", "evil.example:3030", "127.0.0.1:9999", "localhost.evil.com:3030"] {
            let mut h = HeaderMap::new();
            h.insert(HOST, host.parse().unwrap());
            assert!(
                bearer_challenge(&oauth(), &h).contains("http://localhost:3030/.well-known/"),
                "Host: {host}"
            );
        }
    }

    #[test]
    fn insufficient_scope_challenge_names_error_and_scope() {
        let c = insufficient_scope_challenge(&oauth(), &HeaderMap::new(), &[SCOPE_OPERATOR]);
        assert!(c.starts_with("Bearer error=\"insufficient_scope\""), "{c}");
        assert!(c.contains("scope=\"operator\""), "{c}");
        assert!(
            c.contains("resource_metadata=\"http://localhost:3030/.well-known/oauth-protected-resource\""),
            "{c}"
        );
    }

    /// MCP exige TODOS los scopes necesarios en un unico desafio, no una
    /// respuesta por scope.
    #[test]
    fn insufficient_scope_challenge_lists_every_scope_in_one_header() {
        let c = insufficient_scope_challenge(&oauth(), &HeaderMap::new(), &[SCOPE_OPERATOR, "federation:admin"]);
        assert!(c.contains("scope=\"operator federation:admin\""), "{c}");
        assert_eq!(c.matches("resource_metadata=").count(), 1, "{c}");
    }
}

/// T-DSH-86 tests: Origin/Host rules for the MCP/SSE DNS-rebinding guard.
#[cfg(test)]
mod mcp_origin_guard_tests {
    use super::*;

    #[test]
    fn loopback_origins_are_accepted_any_port() {
        for origin in [
            "http://127.0.0.1:47004",
            "http://localhost",
            "https://[::1]:3030",
            "http://LOCALHOST:5173",
        ] {
            assert!(is_loopback_origin(origin), "{origin}");
            assert!(origin_is_allowed(origin, &[]), "{origin}");
        }
    }

    #[test]
    fn hostile_origins_are_rejected() {
        for origin in [
            "http://evil.example",
            "https://127.0.0.1.evil.example",
            "null",
            "",
            "http://[::2]:3030",
            "file:///tmp/x",
        ] {
            assert!(!is_loopback_origin(origin), "{origin}");
            assert!(!origin_is_allowed(origin, &[]), "{origin}");
        }
    }

    #[test]
    fn allowed_origins_list_is_an_explicit_escape_hatch() {
        let allowed = vec!["http://app.example".to_string()];
        assert!(origin_is_allowed("http://app.example", &allowed));
        assert!(!origin_is_allowed("http://other.example", &allowed));
        assert!(!origin_is_allowed("null", &["null".to_string()]));
    }

    #[test]
    fn host_validation_accepts_loopback_configured_and_allowed() {
        let none: Vec<String> = Vec::new();
        assert!(host_is_allowed("127.0.0.1:47004", "127.0.0.1", &none));
        assert!(host_is_allowed("localhost:3030", "127.0.0.1", &none));
        assert!(host_is_allowed("[::1]:47004", "127.0.0.1", &none));
        assert!(host_is_allowed("127.0.0.1", "localhost", &none));
        assert!(!host_is_allowed("evil.example", "127.0.0.1", &none));
        assert!(!host_is_allowed("127.0.0.1.evil.example", "127.0.0.1", &none));
        let allowed = vec!["tunnel.example".to_string()];
        assert!(host_is_allowed("tunnel.example:47004", "127.0.0.1", &allowed));
    }

    #[test]
    fn authority_hostname_strips_ipv6_brackets_and_port() {
        assert_eq!(hostname_from_authority("[::1]:3030"), "::1");
        assert_eq!(hostname_from_authority("127.0.0.1:47004"), "127.0.0.1");
        assert_eq!(hostname_from_authority("localhost"), "localhost");
    }

    #[test]
    fn guard_covers_exactly_the_mcp_sse_paths() {
        assert_eq!(
            MCP_ORIGIN_GUARD_PATHS,
            ["/messages", "/mcp", "/api/v1/mcp", "/sse", "/sse/download"]
        );
    }
}

#[cfg(test)]
mod d68_acl_tests {
    use super::*;

    #[test]
    fn default_acl_role_is_guest_and_denies_every_guild() {
        let acl = AclConfig::default();
        assert_eq!(acl.default_role, "guest");
        assert!(acl.roles.is_empty(), "default roles must be empty");
        assert!(!acl_can_access("guest", "bash", &acl));
        assert!(!acl_can_access("guest", "knowledge", &acl));
        assert!(acl_can_access("admin", "bash", &acl), "admin remains the explicit operator role");
    }

    #[test]
    fn master_credential_is_explicit_operator_role() {
        assert_eq!(acl_role_for_auth_kind(AclAuthKind::MasterToken), Some("admin"));
        assert_eq!(acl_role_for_auth_kind(AclAuthKind::MasterTokenQuery), Some("admin"));
        assert_eq!(acl_role_for_auth_kind(AclAuthKind::DevBypass), Some("admin"));
        assert_eq!(acl_role_for_auth_kind(AclAuthKind::OAuthJwt), None);
        assert_eq!(acl_role_for_auth_kind(AclAuthKind::GuildToken), None);
    }

    #[test]
    fn unlisted_oauth_identity_gets_guest_not_admin() {
        let acl = AclConfig::default();
        let contract = crate::security::agents_contract::AgentsContract::empty();
        let role = resolve_acl_role_inner(Some("jwt-without-mapping"), Some("agent-x"), &acl, &contract);
        assert_eq!(role, "guest");
        assert!(!acl_can_access(&role, "bash", &acl));
    }
}

#[cfg(test)]
mod d71_oauth_reader_tests {
    use super::*;
    use crate::security::agents_contract::{AgentContractEntry, AgentsContract};

    fn reader_acl_and_contract() -> (AclConfig, AgentsContract) {
        let mut roles = std::collections::HashMap::new();
        roles.insert(
            "reader".to_string(),
            vec!["knowledge".to_string(), "monitor".to_string()],
        );
        let acl = AclConfig {
            default_role: "guest".to_string(),
            roles,
            ..Default::default()
        };

        let mut agents = std::collections::HashMap::new();
        agents.insert(
            "oauth-reader".to_string(),
            AgentContractEntry {
                role: "reader".to_string(),
                description: "read-only OAuth client".to_string(),
                wake: None,
            },
        );
        (acl, AgentsContract { agents })
    }

    #[test]
    fn mapped_oauth_agent_gets_reader_and_unmapped_stays_guest() {
        let (acl, contract) = reader_acl_and_contract();

        let mapped = resolve_acl_role_inner(
            Some("opaque-oauth-jwt"),
            Some("oauth-reader"),
            &acl,
            &contract,
        );
        assert_eq!(mapped, "reader");
        assert!(acl_can_access(&mapped, "knowledge", &acl));
        assert!(!acl_can_access(&mapped, "bash", &acl));

        let unmapped = resolve_acl_role_inner(
            Some("opaque-oauth-jwt"),
            None,
            &acl,
            &contract,
        );
        assert_eq!(unmapped, "guest");
        assert!(!acl_can_access(&unmapped, "knowledge", &acl));
    }
}
