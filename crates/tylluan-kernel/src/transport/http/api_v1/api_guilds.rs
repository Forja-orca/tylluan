use axum::{
    Json,
    extract::{State, Path, Query},
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;
use tylluan_link::dispatch::{GuildDispatchRequest, GuildDispatchResponse};
//
use crate::transport::http::{
    HttpState, GuildRequest, GuildRegisterRequest,
};
use rmcp::model::CallToolRequestParam;

pub async fn guilds_list_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    Json(state.registry.status_all().await.unwrap_or_default())
}

pub async fn guilds_health_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    Json(state.registry.status_all().await.unwrap_or_default())
}

pub async fn guild_register_handler(State(state): State<Arc<HttpState>>, Json(req): Json<GuildRegisterRequest>) -> impl IntoResponse {
    state.registry.register(&req.name, &req.module_path, req.always_on.unwrap_or(false), req.timeout_ms).await;
    StatusCode::OK
}

pub async fn guild_request_handler(State(state): State<Arc<HttpState>>, Json(req): Json<GuildRequest>) -> impl IntoResponse {
    state.registry.ensure_running(&req.name).await.map(|_| StatusCode::OK).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
}

pub async fn guild_start_handler(State(state): State<Arc<HttpState>>, Path(name): Path<String>) -> impl IntoResponse {
    match state.registry.ensure_running(&name).await {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({"status":"ok","guild":name}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"status":"error","error":e.to_string()}))).into_response(),
    }
}

pub async fn guild_stop_handler(State(state): State<Arc<HttpState>>, Path(name): Path<String>) -> impl IntoResponse {
    match state.registry.kill_guild(&name).await {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({"status":"ok","guild":name}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"status":"error","error":e.to_string()}))).into_response(),
    }
}

pub async fn guild_reset_backoff_handler(State(state): State<Arc<HttpState>>, Path(name): Path<String>) -> impl IntoResponse {
    state.registry.reset_backoff(&name).await.map(|_| StatusCode::OK).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
}

/// C3: only `bash` and `filesystem` require the operator class on the direct
/// HTTP guild endpoint. Other guilds keep their agent-class behavior.
fn is_operator_only_guild(guild: &str) -> bool {
    matches!(guild, "bash" | "filesystem")
}

fn guild_call_is_operator_denied(guild: &str, auth: Option<crate::transport::http::auth::AclAuthKind>) -> bool {
    is_operator_only_guild(guild) && !crate::transport::http::auth::operator_gate_allows(auth)
}

pub async fn guild_tool_call_handler(State(state): State<Arc<HttpState>>, Path((guild, tool)): Path<(String, String)>, Json(args): Json<serde_json::Value>) -> impl IntoResponse {
    let auth_kind = crate::transport::http::auth::current_auth_kind();
    if guild_call_is_operator_denied(&guild, auth_kind) {
        let agent_id = args.get("agent_id").and_then(|v| v.as_str()).unwrap_or("unknown");
        crate::transport::server::handler_do::audit_or_warn(
            "c3_operator_required",
            &guild,
            &tool,
            agent_id,
            false,
            "direct HTTP guild call denied: bash/filesystem require operator class",
            0,
            false,
        );
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({
            "error": "operator_required",
            "guild": guild,
            "reason": "C3: direct HTTP calls to bash/filesystem require the operator class; use tylluan_do/MCP for agents."
        }))).into_response();
    }

    // D-96: the direct REST guild path can also reach an external MCP server, so
    // the external ACL must not be skippable by going around `tylluan_do`.
    {
        let agent = args
            .get("agent_id")
            .and_then(|v| v.as_str())
            .unwrap_or("anonymous");
        let cfg = state.config.read().await;
        if cfg.external_mcp.iter().any(|e| e.name == guild)
            && !cfg.security.external_acl.allows(Some(agent), &guild)
        {
            crate::transport::server::handler_do::audit_or_warn(
                &format!("direct guild call {guild}/{tool}"),
                &format!("external_mcp:{guild}"),
                &tool,
                agent,
                false,
                "denied by [security.external_acl] (REST guild path)",
                0,
                false,
            );
            return (StatusCode::FORBIDDEN, Json(serde_json::json!({
                "error": "access_denied",
                "guild": guild,
                "reason": "D-96: this agent is not allowed to use this external MCP server; ask the operator to add it to [security.external_acl]",
            }))).into_response();
        }
    }

    // ACL check: verify the caller's role has access to this guild
    if let Ok(cfg_lock) = crate::config::TylluanConfig::load_cached() {
        {   let cfg = cfg_lock.read().await;
            let acl = &cfg.security.acl;
            // D-68: evaluate the guild gate for every authenticated HTTP
            // request, even when [security.acl.roles] is absent. A missing
            // task-local means an internal/direct call (no HTTP boundary),
            // not an unknown HTTP role.
            if let Some(role) = crate::transport::http::auth::current_acl_role()
                && !crate::transport::http::auth::acl_can_access(&role, &guild, acl)
            {
                return (StatusCode::FORBIDDEN, Json(serde_json::json!({
                    "error": format!("Role '{}' does not have access to guild '{}'", role, guild)
                }))).into_response();
            }
        }
    }

    // Per-guild rate limit check
    if let Some(ref srv_arc) = state.server {
        let srv = srv_arc.read().await;
        if let Err(_msg) = srv.guild_rate_limiter.check_and_record(&guild) {
            return (StatusCode::TOO_MANY_REQUESTS, Json(serde_json::json!({
                "error": format!("{} Rate limit for guild '{}' exceeded. Try again later.",
                    crate::transport::server::handler_do::error_prefixes::RATE_LIMITED, guild)
            }))).into_response();
        }
    }

    let req = CallToolRequestParam { name: tool.into(), arguments: args.as_object().cloned() };
    let agent_id = args.get("agent_id").and_then(|v| v.as_str()).unwrap_or("unknown");
    let _ = state.silva.touch_node(&format!("agent:{agent_id}"), agent_id, &format!("tool_call:{guild}")).await;
    // Fire-and-forget, matching record_activity_trace/coloquio_post_message's pattern
    // elsewhere in this ADR-015 §4.1 wiring -- a stigmergy trace write must never add
    // SQLite-write latency to a guild tool call's HTTP response.
    let target_guild_zone = if guild == "vision" { "guilds/vision" } else { "guilds/core" };
    let silva_gw = state.silva.clone();
    let agent_id_gw = agent_id.to_string();
    tokio::spawn(async move {
        let _ = silva_gw.record_work_trace(target_guild_zone, "guild", &agent_id_gw, "tool_call", 1.0).await;
    });
    // bwc-d0fb0812: attribute this call in the outputs ledger when the
    // caller identifies itself. The actor keeps indexing best-effort and
    // observation-only either way.
    let requested_by = (agent_id != "unknown").then(|| agent_id.to_string());
    match state.registry.call_tool_as(&guild, req, requested_by).await {
        Ok(res) => (StatusCode::OK, Json(res)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

pub async fn guild_test_handler(
    State(state): State<Arc<HttpState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let start = std::time::Instant::now();
    match state.registry.ensure_running(&name).await {
        Ok(_) => Json(serde_json::json!({
            "status": "ok",
            "guild": name,
            "latency_ms": start.elapsed().as_millis(),
        })).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "status": "error",
            "guild": name,
            "message": e.to_string()
        }))).into_response(),
    }
}

pub async fn guilds_utilization_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let statuses = state.registry.status_all().await.unwrap_or_default();
    let active = statuses.iter().filter(|s| s.running).count();
    let idle = statuses.iter().filter(|s| !s.running && s.always_on).count();
    let offline = statuses.iter().filter(|s| !s.running && !s.always_on).count();
    let utilization_pct = if !statuses.is_empty() { (active as f64 / statuses.len() as f64 * 100.0) as i64 } else { 0 };
    (StatusCode::OK, Json(serde_json::json!({
        "total": statuses.len(),
        "active": active,
        "idle": idle,
        "offline": offline,
        "utilization_percent": utilization_pct,
        "active_guilds": statuses.iter().filter(|s| s.running).map(|s| serde_json::json!({ "name": s.name, "tools": s.tools_count, "idle_secs": s.idle_seconds })).collect::<Vec<_>>(),
        "idle_guilds": statuses.iter().filter(|s| !s.running && s.always_on).map(|s| serde_json::json!({ "name": s.name, "always_on": s.always_on })).collect::<Vec<_>>()
    })))
}

pub async fn guild_start_alt_handler(
    State(state): State<Arc<HttpState>>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let name = match params.get("name") {
        Some(n) => n.clone(),
        None => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "missing 'name' param"}))).into_response(),
    };
    match state.registry.ensure_running(&name).await {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({"status": "started", "guild": name}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string(), "guild": name}))).into_response(),
    }
}

pub async fn guild_dispatch_execute_handler(
    State(state): State<Arc<HttpState>>,
    Json(req): Json<GuildDispatchRequest>,
) -> impl IntoResponse {
    let start = std::time::Instant::now();
    let executor_id = state.node_identity.node_id().to_string();

    let tool_req = rmcp::model::CallToolRequestParam {
        name: req.tool.into(),
        arguments: Some(req.args.as_object().cloned().unwrap_or_default()),
    };

    match state.registry.call_tool(&req.guild, tool_req).await {
        Ok(result) => {
            let duration_ms = start.elapsed().as_millis() as u64;
            let response = GuildDispatchResponse {
                request_id: req.request_id,
                success: !result.is_error.unwrap_or(false),
                result: serde_json::json!(result.content),
                error: None,
                executor_id,
                duration_ms,
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            let duration_ms = start.elapsed().as_millis() as u64;
            let response = GuildDispatchResponse {
                request_id: req.request_id,
                success: false,
                result: serde_json::Value::Null,
                error: Some(e.to_string()),
                executor_id,
                duration_ms,
            };
            (StatusCode::OK, Json(response)).into_response()
        }
    }
}

/// GET /api/v1/capability-registry — Tarea Raíz 2 (2026-09-28): read-only
/// Capability Registry MVP. Static index over TOOL_METADATA + the real guild
/// catalog (one capability per tool, Local providers, verbatim declared
/// risk). NOTE: distinct from the pre-existing runtime view at
/// /api/v1/capabilities (sessions + live guild tools) — this one is the
/// static declarative index TaskContext v2 / ADR-014 build on. Pure read
/// layer: zero changes to router/scheduler behavior.
pub async fn capability_registry_handler() -> impl IntoResponse {
    Json(crate::registry::capability_registry::registry_json())
}

/// GET /api/v1/capability-registry/{guild} — capabilities provided by one
/// guild (Local providers only in the MVP; empty list if the guild declares
/// no tools).
pub async fn capability_registry_guild_handler(
    Path(guild): Path<String>,
) -> impl IntoResponse {
    let caps = crate::registry::capability_registry::capabilities_for_guild(&guild);
    let capabilities: Vec<serde_json::Value> = caps
        .iter()
        .map(|c| serde_json::to_value(c).unwrap_or(serde_json::Value::Null))
        .collect();
    Json(serde_json::json!({
        "guild": guild,
        "capabilities": capabilities,
        "count": capabilities.len(),
    }))
}

#[cfg(test)]
mod c3_tests {
    use super::*;
    use crate::transport::http::auth::AclAuthKind;

    #[test]
    fn c3_agent_and_untrusted_classes_are_denied_for_bash_and_filesystem() {
        for guild in ["bash", "filesystem"] {
            for auth in [
                Some(AclAuthKind::OAuthJwt),
                Some(AclAuthKind::PublicBypass),
                Some(AclAuthKind::GuildToken),
                Some(AclAuthKind::MasterTokenQuery),
                None,
            ] {
                assert!(
                    guild_call_is_operator_denied(guild, auth),
                    "{guild} must require operator for {auth:?}"
                );
            }
        }
    }

    #[test]
    fn c3_operator_master_and_dev_bypass_are_allowed() {
        for guild in ["bash", "filesystem"] {
            for auth in [AclAuthKind::MasterToken, AclAuthKind::DevBypass] {
                assert!(
                    !guild_call_is_operator_denied(guild, Some(auth)),
                    "{guild} must allow operator {auth:?}"
                );
            }
        }
    }

    #[test]
    fn c3_other_guilds_keep_agent_behavior() {
        for guild in ["coloquio", "llama_backend", "vision", "code", "docker", "git"] {
            for auth in [
                Some(AclAuthKind::OAuthJwt),
                Some(AclAuthKind::PublicBypass),
                Some(AclAuthKind::GuildToken),
                None,
            ] {
                assert!(
                    !guild_call_is_operator_denied(guild, auth),
                    "{guild} must not be blocked by C3"
                );
            }
        }
        assert!(is_operator_only_guild("bash"));
        assert!(is_operator_only_guild("filesystem"));
        assert!(!is_operator_only_guild("code"));
        assert!(!is_operator_only_guild("docker"));
        assert!(!is_operator_only_guild("git"));
    }
}
