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

pub async fn guild_tool_call_handler(State(state): State<Arc<HttpState>>, Path((guild, tool)): Path<(String, String)>, Json(args): Json<serde_json::Value>) -> impl IntoResponse {
    // ACL check: verify the caller's role has access to this guild
    if let Ok(cfg_lock) = crate::config::TylluanConfig::load_cached() {
        {   let cfg = cfg_lock.read().await;
            let acl = &cfg.security.acl;
            if !acl.roles.is_empty() {
                let role = crate::transport::http::auth::current_acl_role();
                if !crate::transport::http::auth::acl_can_access(&role, &guild, acl) {
                    return (StatusCode::FORBIDDEN, Json(serde_json::json!({
                        "error": format!("Role '{}' does not have access to guild '{}'", role, guild)
                    }))).into_response();
                }
            }
        }
    }

    let req = CallToolRequestParam { name: tool.into(), arguments: args.as_object().cloned() };
    let agent_id = args.get("agent_id").and_then(|v| v.as_str()).unwrap_or("unknown");
    let _ = state.silva.touch_node(&format!("agent:{}", agent_id), agent_id, &format!("tool_call:{}", guild)).await;
    match state.registry.call_tool(&guild, req).await {
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
