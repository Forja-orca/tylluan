use axum::{
    Json,
    extract::{State, Path},
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;
use std::fs;
use serde::Deserialize;
use crate::transport::http::{HttpState, SaveConfigRequest};

#[derive(Deserialize)]
pub struct SetDeviceRequest { pub device: String }

pub async fn get_config_handler() -> impl IntoResponse {
    match crate::config::TylluanConfig::load_cached() {
        Ok(c) => match c.try_read() {
            Ok(config) => Json(config.clone()).into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Config lock poisoned: {}", e)).into_response(),
        },
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
    }
}

/// POST /api/v1/config/device — targeted, corruption-proof edit of the
/// `device = "..."` line in tylluan.toml. Server-side; the browser never
/// round-trips the whole config (that's what bricked it once).
pub async fn set_inference_device_handler(Json(req): Json<SetDeviceRequest>) -> impl IntoResponse {
    let device = req.device.trim().to_lowercase();
    if !["cpu", "directml", "cuda"].contains(&device.as_str()) {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "device must be one of: cpu, directml, cuda"
        }))).into_response();
    }
    let config_path = crate::config::TylluanConfig::find_config_file()
        .unwrap_or_else(|| std::path::PathBuf::from("tylluan.toml"));
    let raw = match fs::read_to_string(&config_path) {
        Ok(s) => s,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    };
    let mut replaced = false;
    let new_raw: String = raw.lines().map(|l| {
        if !replaced && l.trim_start().starts_with("device") && l.contains('=') {
            replaced = true;
            format!("device = \"{}\"", device)
        } else {
            l.to_string()
        }
    }).collect::<Vec<_>>().join("\n");
    let new_raw = if replaced { new_raw } else {
        format!("{}\n\n[inference]\ndevice = \"{}\"\n", new_raw.trim_end(), device)
    };
    // Never write something that doesn't parse back.
    if let Err(e) = toml::from_str::<crate::config::TylluanConfig>(&new_raw) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "error": format!("refusing to write invalid TOML: {}", e)
        }))).into_response();
    }
    let tmp_path = config_path.with_extension("toml.tmp");
    if let Err(e) = fs::write(&tmp_path, &new_raw) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response();
    }
    if let Err(e) = fs::rename(&tmp_path, &config_path) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response();
    }
    (StatusCode::OK, Json(serde_json::json!({
        "device": device, "restart_required": true
    }))).into_response()
}

pub async fn save_config_handler(State(state): State<Arc<HttpState>>, Json(req): Json<SaveConfigRequest>) -> impl IntoResponse {
    // Guard: never write content that doesn't parse as our config TOML.
    // (A dashboard bug once wrote JSON here and bricked the kernel config on restart.)
    if let Err(e) = toml::from_str::<crate::config::TylluanConfig>(&req.content) {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": format!("content is not valid tylluan.toml — refusing to write: {}", e)
        }))).into_response();
    }

    let old_config = state.config.read().await.clone();
    let old_embedding = old_config.memory.embedding_model.clone();

    let config_path = crate::config::TylluanConfig::find_config_file()
        .unwrap_or_else(|| std::path::PathBuf::from("tylluan.toml"));
    let tmp_path = config_path.with_extension("toml.tmp");
    if let Err(e) = fs::write(&tmp_path, &req.content) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response();
    }
    if let Err(e) = fs::rename(&tmp_path, &config_path) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response();
    }

    if let Ok(new_config) = toml::from_str::<crate::config::TylluanConfig>(&req.content) {
        let new_embedding = new_config.memory.embedding_model.clone();
        if new_embedding != old_embedding {
            let _ = state.broadcast_tx.send(serde_json::json!({
                "type": "config_changed",
                "field": "embedding_model",
                "old_value": old_embedding,
                "new_value": new_embedding,
                "requires_restart": true,
                "message": "Cambio de modelo de embedding detectado. Se requiere reiniciar el kernel y reindexar todos los nodos.",
                "ts": chrono::Utc::now().timestamp_millis()
            }));
        }
    }
    (StatusCode::OK, Json(serde_json::json!({ "status": "saved" }))).into_response()
}

// --- SYSTEM ---

pub async fn audit_logs_handler() -> impl IntoResponse {
    let log_path = "logs/kernel.log";
    let content = fs::read_to_string(log_path).unwrap_or_default();
    let events: Vec<serde_json::Value> = content.lines().rev().take(100).map(|line| {
        let parts: Vec<&str> = line.splitn(4, ' ').collect();
        let ts = parts.first().cloned().unwrap_or("");
        let level = parts.get(1).cloned().unwrap_or("INFO");
        let source = parts.get(2).cloned().unwrap_or("kernel");
        let msg = parts.get(3).cloned().unwrap_or(line);

        serde_json::json!({
            "type": level.to_lowercase(),
            "source": source,
            "data": { "message": msg },
            "ts": ts
        })
    }).collect();

    Json(serde_json::json!({ "logs": events, "count": events.len() }))
}

pub async fn system_status_handler(
    State(state): State<Arc<HttpState>>,
) -> impl IntoResponse {
    let report = state.doctor.diagnose().await;
    let curr_count = {
        let curr_learner = state.doctor.curriculum();
        let curr = curr_learner.lock().unwrap_or_else(|e| e.into_inner());
        curr.get_stats()["total_entries"].as_u64().unwrap_or(0)
    };

    let status_json = serde_json::json!({
        "silva_healthy": report.storage.silva_db_ok,
        "mailbox_healthy": report.storage.memory_db_ok,
        "curriculum_entries": curr_count,
        "uptime_secs": state.start_time.elapsed().as_secs(),
        "embeddings_loaded": true,
        "score": if report.status == "healthy" { 100 } else if report.status == "degraded" { 65 } else { 30 },
        "system": {
            "cpu_usage": report.system.cpu_usage_percent,
            "memory_percent": report.system.memory_percent,
            "used_memory_mb": report.system.used_memory_mb,
            "total_memory_mb": report.system.total_memory_mb,
            "process_count": report.system.process_count,
        }
    });

    crate::transport::http::Utf8Json(status_json)
}

pub async fn test_connection_handler() -> impl IntoResponse { StatusCode::OK }
pub async fn update_wsl_config_handler() -> impl IntoResponse { StatusCode::OK }

pub async fn list_inference_providers_handler() -> impl IntoResponse { StatusCode::OK }
pub async fn add_inference_provider_handler() -> impl IntoResponse { StatusCode::OK }

pub async fn health_detailed_handler(
    State(state): State<Arc<HttpState>>
) -> impl IntoResponse {
    let node_count = state.silva.node_count().await.unwrap_or(0);
    let edge_count = state.silva.edge_count().await.unwrap_or(0);

    // Guild health
    let (total_guilds, active_guilds) = state.registry.guild_stats().await.unwrap_or((0, 0));

    // Server capabilities
    let (embeddings_loaded, reranker_loaded) = if let Some(ref srv_arc) = state.server {
        if let Ok(s) = srv_arc.try_read() {
            let emb = s.matcher.engine().is_some();
            let rer = s.reranker.is_some();
            (emb, rer)
        } else { (false, false) }
    } else { (false, false) };

    // Overall health score (0-100)
    let mut score = 100u8;
    if !embeddings_loaded { score = score.saturating_sub(20); }
    if !reranker_loaded   { score = score.saturating_sub(10); }
    if active_guilds == 0 { score = score.saturating_sub(30); }
    if node_count == 0    { score = score.saturating_sub(10); }

    let status = if score >= 80 { "healthy" }
                 else if score >= 50 { "degraded" }
                 else { "critical" };

    Json(serde_json::json!({
        "status": status,
        "score": score,
        "version": &state.version,
        "components": {
            "embeddings": { "ok": embeddings_loaded, "model": "bge-m3" },
            "reranker":   { "ok": reranker_loaded,   "model": "jina-reranker-v1-turbo-en" },
            "guilds":     { "ok": active_guilds > 0,
                            "active": active_guilds, "total": total_guilds },
            "silva":      { "ok": node_count > 0,
                            "nodes": node_count, "edges": edge_count },
            "tunnel":     { "ok": state.tunnel_wsl_url.is_some(),
                            "wsl_url": state.tunnel_wsl_url }
        }
    }))
}

pub async fn admin_reload_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    // Check dev_mode
    if !state.dev_mode.unwrap_or(false) {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({
            "error": "admin/reload only available in dev_mode"
        }))).into_response();
    }

    let start = std::time::Instant::now();

    // Get all active guilds
    let guild_statuses = match state.registry.status_all().await {
        Ok(statuses) => statuses,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": format!("Failed to get guild status: {}", e)
            }))).into_response();
        }
    };

    let guild_names: Vec<String> = guild_statuses
        .iter()
        .map(|s| s.name.clone())
        .collect();

    // Kill and restart each guild
    let mut reloaded = 0;
    for name in &guild_names {
        if let Err(e) = state.registry.kill_guild(name).await {
            tracing::warn!("Failed to kill guild {}: {}", name, e);
            continue;
        }

        // Small delay to ensure process fully exits
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        if let Err(e) = state.registry.ensure_running(name).await {
            tracing::warn!("Failed to restart guild {}: {}", name, e);
            continue;
        }

        reloaded += 1;
    }

    let elapsed_ms = start.elapsed().as_millis() as u64;

    (StatusCode::OK, Json(serde_json::json!({
        "reloaded": true,
        "guilds": reloaded,
        "attempted": guild_names.len(),
        "elapsed_ms": elapsed_ms,
        "guild_names": guild_names
    }))).into_response()
}

pub async fn meta_prune_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    match state.silva.meta_cognitive_prune(0.15, 168, 48) {
        Ok(count) => Json(serde_json::json!({"archived": count, "status": "ok"})).into_response(),
        Err(e) => Json(serde_json::json!({"error": e.to_string(), "status": "error"})).into_response(),
    }
}

pub async fn admin_shutdown_handler(
    State(state): State<Arc<HttpState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let host = headers.get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let is_local = host.starts_with("127.0.0.1") || host.starts_with("localhost") || host.starts_with("[::1]");
    if !is_local {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Admin actions allowed only from localhost"})),
        ).into_response();
    }

    tracing::info!("🛑 Admin shutdown requested via POST /api/v1/admin/shutdown. Cancelling tokio token...");
    state.cancel_token.cancel();

    (StatusCode::OK, Json(serde_json::json!({"status": "shutdown_initiated"}))).into_response()
}

/// Emergency kill: stop all guilds immediately without restart, then shutdown kernel.
pub async fn admin_emergency_kill_handler(
    State(state): State<Arc<HttpState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let host = headers.get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let is_local = host.starts_with("127.0.0.1") || host.starts_with("localhost") || host.starts_with("[::1]");
    if !is_local {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Emergency kill allowed only from localhost"})),
        ).into_response();
    }

    tracing::warn!("🚨 EMERGENCY KILL requested. Killing all guilds and shutting down.");

    let mut killed = 0;
    if let Ok(statuses) = state.registry.status_all().await {
        for gs in &statuses {
            if let Err(e) = state.registry.kill_guild(&gs.name).await {
                tracing::error!("Failed to kill guild {}: {}", gs.name, e);
            } else {
                killed += 1;
            }
        }
    }

    state.cancel_token.cancel();

    (StatusCode::OK, Json(serde_json::json!({
        "status": "emergency_kill_complete",
        "guilds_killed": killed
    }))).into_response()
}

/// Kill a specific guild by name (for rogue agent mitigation).
pub async fn admin_kill_guild_handler(
    State(state): State<Arc<HttpState>>,
    headers: axum::http::HeaderMap,
    Path(guild_name): Path<String>,
) -> impl IntoResponse {
    let host = headers.get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let is_local = host.starts_with("127.0.0.1") || host.starts_with("localhost") || host.starts_with("[::1]");
    if !is_local {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Admin actions allowed only from localhost"})),
        ).into_response();
    }

    tracing::warn!("🛑 Kill requested for guild '{}'", guild_name);

    match state.registry.kill_guild(&guild_name).await {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({
            "status": "killed",
            "guild": guild_name
        }))).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "error": format!("Failed to kill guild '{}': {}", guild_name, e)
        }))).into_response(),
    }
}

// --- SESSIONS ---

pub async fn list_sessions_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let sessions = state.sessions.read().await;
    let list: Vec<serde_json::Value> = sessions.values().map(|s| {
        serde_json::json!({
            "id": s.id,
            "client_name": s.client_name,
            "agent_id": s.agent_id,
            "tool_count": s.tool_count,
            "last_intent": s.last_intent,
            "last_guild": s.last_guild,
            "last_active_unix": s.last_active_unix,
            "created_unix": s.created_unix,
        })
    }).collect();
    Json(serde_json::json!({ "sessions": list }))
}

pub async fn session_detail_handler(
    State(state): State<Arc<HttpState>>,
    Path(session_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let sessions = state.sessions.read().await;
    match sessions.get(&session_id) {
        Some(s) => Json(serde_json::json!({
            "id": s.id,
            "client_name": s.client_name,
            "agent_id": s.agent_id,
            "tool_count": s.tool_count,
            "last_intent": s.last_intent,
            "last_guild": s.last_guild,
            "last_active_unix": s.last_active_unix,
            "created_unix": s.created_unix,
        })).into_response(),
        None => (StatusCode::NOT_FOUND,
                 Json(serde_json::json!({"error":"session not found"}))).into_response(),
    }
}

pub async fn revoke_session_handler(
    State(state): State<Arc<HttpState>>,
    Path(session_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let mut sessions = state.sessions.write().await;
    if sessions.remove(&session_id).is_some() {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

// --- APPROVALS ---

pub async fn approval_list_handler(State(state): State<Arc<HttpState>>) -> axum::response::Response {
    let srv_arc = match state.server.as_ref() {
        Some(s) => s,
        None => return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "kernel server not initialized"}))
        ).into_response(),
    };
    let server = srv_arc.read().await;
    let pending = server.pending_approvals.read().await;
    let approvals: Vec<serde_json::Value> = pending.iter().map(|(id, action)| {
        serde_json::json!({
            "id": id,
            "tool": action.name,
            "arguments": action.arguments,
            "created_at": chrono::Utc::now().to_rfc3339(),
        })
    }).collect();
    Json(serde_json::Value::Array(approvals)).into_response()
}

pub async fn approval_approve_handler(State(state): State<Arc<HttpState>>, Path(id): axum::extract::Path<String>) -> axum::response::Response {
    let srv_arc = match state.server.as_ref() {
        Some(s) => s,
        None => return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "kernel server not initialized"}))
        ).into_response(),
    };
    let server = srv_arc.read().await;
    let mut pending = server.pending_approvals.write().await;
    if let Some(action) = pending.remove(&id) {
        let _ = action.tx.send(Ok(rmcp::model::CallToolResult {
            content: vec![],
            is_error: Some(false),
        }));
        StatusCode::OK.into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

pub async fn approval_reject_handler(State(state): State<Arc<HttpState>>, Path(id): axum::extract::Path<String>) -> axum::response::Response {
    let srv_arc = match state.server.as_ref() {
        Some(s) => s,
        None => return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "kernel server not initialized"}))
        ).into_response(),
    };
    let server = srv_arc.read().await;
    let mut pending = server.pending_approvals.write().await;
    if let Some(action) = pending.remove(&id) {
        let _ = action.tx.send(Err(rmcp::Error::internal_error("AcciÃ³n rechazada por el usuario.", None)));
        StatusCode::OK.into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

// --- MAINTENANCE ---

pub async fn maintenance_status_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let mut total_bytes = 0u64;
    for path in &["./data/silva.db", "./data/silva.db-wal", "./data/silva.db-shm", "./data/tylluan.db"] {
        if let Ok(meta) = fs::metadata(path) { total_bytes += meta.len(); }
    }

    let last_export = fs::read_dir("./data/exports").ok()
        .and_then(|dir| {
            dir.filter_map(|e| e.ok())
               .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
               .filter_map(|e| e.metadata().ok())
               .filter_map(|m| m.modified().ok())
               .max()
        })
        .map(|t| { let dt: chrono::DateTime<chrono::Utc> = t.into(); dt.format("%Y-%m-%d %H:%M").to_string() })
        .unwrap_or_else(|| "Never".to_string());

    let brain_size_human = if total_bytes > 1_073_741_824 {
        format!("{:.2} GB", total_bytes as f64 / 1_073_741_824.0)
    } else {
        format!("{:.2} MB", total_bytes as f64 / 1_048_576.0)
    };

    let node_count = state.silva.node_count().await.unwrap_or(0);
    let edge_count = state.silva.edge_count().await.unwrap_or(0);
    let orphan_count = state.silva.orphan_node_count().await.unwrap_or(0);

    Json(serde_json::json!({
        "status": "ok",
        "brain_size_bytes": total_bytes,
        "brain_size_human": brain_size_human,
        "last_export": last_export,
        "storage_mode": "SQLite WAL",
        "node_count": node_count,
        "edge_count": edge_count,
        "orphan_node_count": orphan_count,
    }))
}

pub async fn maintenance_export_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    use axum::http::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
    let nodes = state.silva.get_nodes_paginated(10_000, 0).await.unwrap_or_default();
    let edges = state.silva.get_all_edges().await.unwrap_or_default();
    let ts = chrono::Utc::now().format("%Y%m%dT%H%M%S");
    let body = serde_json::json!({
        "version": "1.0",
        "exported_at": chrono::Utc::now().to_rfc3339(),
        "graph": { "nodes": nodes, "edges": edges }
    });
    (
        [
            (CONTENT_DISPOSITION, format!("attachment; filename=\"tylluan-backup-{}.json\"", ts)),
            (CONTENT_TYPE, "application/json".to_string()),
        ],
        Json(body),
    ).into_response()
}

pub async fn maintenance_purge_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    tracing::warn!("âš ï¸ MANUAL PURGE REQUESTED via dashboard.");
    let conn = Arc::clone(&state.silva.conn_lock());
    let result = tokio::task::spawn_blocking(move || {
        let conn = conn.blocking_lock();
        conn.execute_batch("DELETE FROM edges; DELETE FROM nodes;")?;
        Ok::<_, anyhow::Error>(())
    }).await;
    match result {
        Ok(Ok(_)) => { tracing::info!("âœ… SilvaDB purged successfully."); StatusCode::OK }
        _ => { tracing::error!("âŒ SilvaDB purge failed"); StatusCode::INTERNAL_SERVER_ERROR }
    }
}

pub async fn maintenance_vacuum_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    state.silva.vacuum().await.map(|_| StatusCode::OK).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
}

pub async fn maintenance_checkpoint_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    state.silva.checkpoint().await.map(|_| StatusCode::OK).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
}

pub async fn maintenance_decay_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let half_life = {
        let cfg = state.config.read().await;
        cfg.silva.decay_half_life_hours
    };
    state.silva.apply_decay(half_life).await.map(|_| StatusCode::OK).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
}

pub async fn maintenance_purge_lessons_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    match state.silva.purge_deprecated_lessons().await {
        Ok(count) => (StatusCode::OK, Json(serde_json::json!({ "purged": count }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

pub async fn maintenance_clean_orphans_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    match state.silva.cleanup_orphan_nodes().await {
        Ok(count) => {
            tracing::info!("✅ Cleaned up {} orphan nodes successfully.", count);
            (StatusCode::OK, Json(serde_json::json!({ "status": "success", "deleted_count": count }))).into_response()
        }
        Err(e) => {
            tracing::error!("❌ Failed to cleanup orphan nodes: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "status": "error", "error": e.to_string() }))).into_response()
        }
    }
}
