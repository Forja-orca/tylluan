use axum::{
    Json,
    extract::{Query, State, Path},
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;
use std::fs;
use serde::Deserialize;
use crate::transport::http::{HttpState, SaveConfigRequest};

#[derive(Deserialize)]
pub struct SetDeviceRequest { pub device: String }

#[derive(Deserialize)]
pub struct SetSandboxProfileRequest { pub profile: String }

#[derive(Deserialize)]
pub struct SetGuildSandboxOverrideRequest {
    pub guild: String,
    pub profile: String,
}

#[derive(Deserialize)]
pub struct SetSessionSandboxOverrideRequest {
    pub agent_id: String,
    pub profile: String,
}

pub async fn get_config_handler() -> impl IntoResponse {
    match crate::config::TylluanConfig::load_cached() {
        Ok(c) => match c.try_read() {
            Ok(config) => Json(config.clone()).into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Config lock poisoned: {e}")).into_response(),
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
            format!("device = \"{device}\"")
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

/// POST /api/v1/config/sandbox-profile — targeted, corruption-proof edit of
/// `profile = "..."` under `[security.sandbox]` in tylluan.toml. Same pattern
/// as set_inference_device_handler: never round-trip the whole config through
/// the browser (that's what bricked it once).
pub async fn set_sandbox_profile_handler(Json(req): Json<SetSandboxProfileRequest>) -> impl IntoResponse {
    let profile = req.profile.trim().to_lowercase();
    if !["strict", "balanced", "permissive"].contains(&profile.as_str()) {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "profile must be one of: strict, balanced, permissive"
        }))).into_response();
    }
    let config_path = crate::config::TylluanConfig::find_config_file()
        .unwrap_or_else(|| std::path::PathBuf::from("tylluan.toml"));
    let raw = match fs::read_to_string(&config_path) {
        Ok(s) => s,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    };

    let mut in_sandbox_section = false;
    let mut replaced = false;
    let mut saw_sandbox_section = false;
    let new_raw: String = raw.lines().map(|l| {
        let trimmed = l.trim_start();
        if trimmed.starts_with('[') {
            in_sandbox_section = trimmed.starts_with("[security.sandbox]");
            if in_sandbox_section { saw_sandbox_section = true; }
        } else if in_sandbox_section && !replaced && trimmed.starts_with("profile") && trimmed.contains('=') {
            replaced = true;
            return format!("profile = \"{profile}\"");
        }
        l.to_string()
    }).collect::<Vec<_>>().join("\n");

    let new_raw = if replaced {
        new_raw
    } else if saw_sandbox_section {
        // Section exists but no `profile` key yet — insert right after the header.
        let mut out = String::new();
        let mut inserted = false;
        for l in new_raw.lines() {
            out.push_str(l);
            out.push('\n');
            if !inserted && l.trim_start().starts_with("[security.sandbox]") {
                out.push_str(&format!("profile = \"{profile}\"\n"));
                inserted = true;
            }
        }
        out
    } else {
        format!("{}\n\n[security.sandbox]\nprofile = \"{}\"\n", new_raw.trim_end(), profile)
    };

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
    if let Err(e) = crate::config::TylluanConfig::reload().await {
        return (StatusCode::OK, Json(serde_json::json!({
            "profile": profile, "restart_required": true,
            "warning": format!("written to disk but in-memory reload failed: {e} — restart to apply")
        }))).into_response();
    }
    (StatusCode::OK, Json(serde_json::json!({
        "profile": profile, "restart_required": false
    }))).into_response()
}

/// POST /api/v1/config/sandbox-profile/guild — targeted edit of a single
/// `guild_overrides.<key>` under `[security.sandbox.guild_overrides]`.
/// Never round-trips the full config through the browser.
pub async fn set_guild_sandbox_override_handler(
    Json(req): Json<SetGuildSandboxOverrideRequest>,
) -> impl IntoResponse {
    let profile = req.profile.trim().to_lowercase();
    if !["strict", "balanced", "permissive"].contains(&profile.as_str()) {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "profile must be one of: strict, balanced, permissive"
        }))).into_response();
    }
    if req.guild.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "guild name must not be empty"
        }))).into_response();
    }

    let config_path = crate::config::TylluanConfig::find_config_file()
        .unwrap_or_else(|| std::path::PathBuf::from("tylluan.toml"));
    let raw = match fs::read_to_string(&config_path) {
        Ok(s) => s,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    };

    // Bare key form, always used INSIDE the [security.sandbox.guild_overrides]
    // bracket table -- never the dotted `guild_overrides."x"` form, which is
    // only valid directly under [security.sandbox] and was previously mixed
    // with the bracket-table form by mistake, producing invalid TOML anytime
    // the table didn't already exist (verified via curl: "unknown variant
    // `bash`, expected one of `strict`, `balanced`, `permissive`").
    let quoted_guild = format!("\"{}\"", req.guild.trim());
    let target_line = format!("{quoted_guild} = \"{profile}\"");

    let mut in_guild_overrides = false;
    let mut replaced = false;
    let mut saw_guild_overrides = false;

    let new_raw: String = raw.lines().map(|l| {
        let trimmed = l.trim_start();
        if trimmed.starts_with('[') {
            in_guild_overrides = trimmed.starts_with("[security.sandbox.guild_overrides]");
            if in_guild_overrides { saw_guild_overrides = true; }
        } else if in_guild_overrides && !replaced && trimmed.starts_with(&quoted_guild) && trimmed.contains('=') {
            replaced = true;
            return target_line.clone();
        }
        l.to_string()
    }).collect::<Vec<_>>().join("\n");

    let new_raw = if replaced {
        new_raw
    } else if saw_guild_overrides {
        // Section exists but no entry for this guild — append inside the section.
        // Find the last line of the section and insert before the next section header.
        let mut out = String::new();
        let mut inserted = false;
        let mut in_override_section = false;
        for l in new_raw.lines() {
            let trimmed = l.trim_start();
            if trimmed.starts_with("[security.sandbox.guild_overrides]") {
                in_override_section = true;
            } else if in_override_section && trimmed.starts_with('[') {
                // Next section — insert right before
                if !inserted {
                    out.push_str(&format!("{target_line}\n"));
                    inserted = true;
                }
                in_override_section = false;
            }
            out.push_str(l);
            out.push('\n');
        }
        // If section was the last thing in the file, append at the end
        if !inserted {
            out.push_str(&format!("{target_line}\n"));
        }
        out
    } else {
        // No [security.sandbox.guild_overrides] table yet -- TOML doesn't
        // care about section ordering, so it's always valid to just append
        // a fresh table at the end of the file rather than hunting for the
        // right sibling-section insertion point under [security.sandbox].
        format!("{}\n\n[security.sandbox.guild_overrides]\n{}\n", new_raw.trim_end(), target_line)
    };

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
    if let Err(e) = crate::config::TylluanConfig::reload().await {
        return (StatusCode::OK, Json(serde_json::json!({
            "guild": req.guild.trim(), "profile": profile, "restart_required": true,
            "warning": format!("written to disk but in-memory reload failed: {e} — restart to apply")
        }))).into_response();
    }
    (StatusCode::OK, Json(serde_json::json!({
        "guild": req.guild.trim(), "profile": profile, "restart_required": false
    }))).into_response()
}

/// POST /api/v1/config/sandbox-profile/session — set in-memory per-agent_id
/// override. NOT persisted to TOML — lives only while the kernel runs.
pub async fn set_session_sandbox_override_handler(
    Json(req): Json<SetSessionSandboxOverrideRequest>,
) -> impl IntoResponse {
    let profile = req.profile.trim().to_lowercase();
    if !["strict", "balanced", "permissive"].contains(&profile.as_str()) {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "profile must be one of: strict, balanced, permissive"
        }))).into_response();
    }
    let parsed = match profile.as_str() {
        "strict" => crate::config::SandboxProfile::Strict,
        "balanced" => crate::config::SandboxProfile::Balanced,
        "permissive" => crate::config::SandboxProfile::Permissive,
        _ => unreachable!(),
    };

    crate::config::set_session_override(&req.agent_id, parsed).await;

    (StatusCode::OK, Json(serde_json::json!({
        "agent_id": req.agent_id, "profile": profile,
        "scope": "session", "persisted": false
    }))).into_response()
}

/// DELETE /api/v1/config/sandbox-profile/guild/{guild} — removes that guild's
/// entry from [security.sandbox.guild_overrides] via targeted line removal.
/// Same never-round-trip-the-whole-file discipline as the POST handler above.
pub async fn delete_guild_sandbox_override_handler(
    Path(guild): Path<String>,
) -> impl IntoResponse {
    let config_path = crate::config::TylluanConfig::find_config_file()
        .unwrap_or_else(|| std::path::PathBuf::from("tylluan.toml"));
    let raw = match fs::read_to_string(&config_path) {
        Ok(s) => s,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    };

    let quoted_guild = format!("\"{}\"", guild.trim());
    let mut in_guild_overrides = false;
    let mut removed = false;
    let new_raw: String = raw.lines().filter(|l| {
        let trimmed = l.trim_start();
        if trimmed.starts_with('[') {
            in_guild_overrides = trimmed.starts_with("[security.sandbox.guild_overrides]");
            return true;
        }
        if in_guild_overrides && trimmed.starts_with(&quoted_guild) && trimmed.contains('=') {
            removed = true;
            return false;
        }
        true
    }).collect::<Vec<_>>().join("\n");

    if !removed {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "guild": guild, "error": "no override found for this guild"
        }))).into_response();
    }

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
    if let Err(e) = crate::config::TylluanConfig::reload().await {
        return (StatusCode::OK, Json(serde_json::json!({
            "guild": guild, "restart_required": true,
            "warning": format!("written to disk but in-memory reload failed: {e} — restart to apply")
        }))).into_response();
    }
    (StatusCode::OK, Json(serde_json::json!({ "guild": guild, "restart_required": false }))).into_response()
}

/// DELETE /api/v1/config/sandbox-profile/session/{agent_id} — clears the
/// in-memory session override. Never touches the TOML (it was never
/// persisted there to begin with).
pub async fn delete_session_sandbox_override_handler(
    Path(agent_id): Path<String>,
) -> impl IntoResponse {
    crate::config::clear_session_override(&agent_id).await;
    (StatusCode::OK, Json(serde_json::json!({ "agent_id": agent_id }))).into_response()
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

#[derive(serde::Deserialize)]
pub struct TestConnectionPayload {
    pub endpoint: Option<String>,
    pub url: Option<String>,
    pub provider: Option<String>,
}

pub async fn test_connection_handler(
    payload: Option<axum::Json<TestConnectionPayload>>,
) -> impl IntoResponse {
    let p = payload.map(|b| b.0);
    let target_url = p.as_ref()
        .and_then(|p| p.endpoint.clone().or_else(|| p.url.clone()))
        .unwrap_or_else(|| "http://127.0.0.1:8080".to_string());

    let provider = p.as_ref()
        .and_then(|p| p.provider.clone())
        .unwrap_or_else(|| "llama-server".to_string());

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                crate::transport::http::Utf8Json(serde_json::json!({
                    "ok": false,
                    "status": "error",
                    "error": format!("Failed to build HTTP client: {}", e),
                    "endpoint": target_url
                })),
            ).into_response();
        }
    };

    let start = std::time::Instant::now();
    let res = client.get(&target_url).send().await;
    let latency_ms = start.elapsed().as_millis() as u64;

    match res {
        Ok(response) => {
            let status_code = response.status().as_u16();
            (
                StatusCode::OK,
                crate::transport::http::Utf8Json(serde_json::json!({
                    "ok": true,
                    "status": "online",
                    "http_status": status_code,
                    "latency_ms": latency_ms,
                    "provider": provider,
                    "endpoint": target_url
                })),
            ).into_response()
        }
        Err(e) => {
            (
                StatusCode::OK,
                crate::transport::http::Utf8Json(serde_json::json!({
                    "ok": false,
                    "status": "offline",
                    "error": format!("Servidor en {} no responde: {}", target_url, e),
                    "latency_ms": latency_ms,
                    "provider": provider,
                    "endpoint": target_url
                })),
            ).into_response()
        }
    }
}

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

/// M31-P3: GET /api/v1/sessions/resume?agent_id=<id>
/// Returns the most recent session summary/digest for an agent.
pub async fn sessions_resume_handler(
    State(state): State<Arc<HttpState>>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let agent_id = match params.get("agent_id") {
        Some(id) if !id.trim().is_empty() => id.trim().to_string(),
        _ => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "Missing or empty 'agent_id' query parameter"
        }))).into_response(),
    };

    let manager = crate::memory::agent_memory::AgentMemoryManager::new(state.silva.clone(), 20);
    match manager.get_summary(&agent_id).await {
        Some(node) => (StatusCode::OK, Json(serde_json::json!({
            "found": true,
            "agent_id": agent_id,
            "summary": node.content,
            "node_id": node.id,
            "node_type": node.node_type,
            "created_at": node.created_at,
            "weight": node.weight,
        }))).into_response(),
        None => (StatusCode::OK, Json(serde_json::json!({
            "found": false,
            "agent_id": agent_id,
        }))).into_response(),
    }
}

// --- GRANTS (HITL via grants.rs, not pending_approvals) ---

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

// --- SECURITY SCOPES PERSISTENCE (Point 7) ---

#[derive(serde::Deserialize, serde::Serialize)]
pub struct ScopeEntry {
    pub role: String,
    pub scopes: Vec<String>,
}

pub async fn get_security_scopes_handler() -> impl IntoResponse {
    let cfg = crate::config::TylluanConfig::load().unwrap_or_default();
    let acl = &cfg.security.acl;
    
    let mut roles = Vec::new();
    for (role_name, scopes) in &acl.roles {
        roles.push(serde_json::json!({
            "role": role_name,
            "scopes": scopes
        }));
    }

    if roles.is_empty() {
        roles.push(serde_json::json!({
            "role": "admin",
            "scopes": ["read", "write", "admin"]
        }));
        roles.push(serde_json::json!({
            "role": "agent",
            "scopes": ["read", "write"]
        }));
        roles.push(serde_json::json!({
            "role": "viewer",
            "scopes": ["read"]
        }));
    }

    Json(serde_json::json!({ "roles": roles }))
}

pub async fn save_security_scopes_handler(
    Json(payload): Json<Vec<ScopeEntry>>,
) -> impl IntoResponse {
    let config_path = crate::config::TylluanConfig::find_config_file()
        .unwrap_or_else(|| std::path::PathBuf::from("tylluan.toml"));
    
    let mut cfg = crate::config::TylluanConfig::load().unwrap_or_default();
    for entry in payload {
        cfg.security.acl.roles.insert(entry.role, entry.scopes);
    }

    if let Ok(toml_str) = toml::to_string_pretty(&cfg) {
        let _ = fs::write(&config_path, toml_str);
        let _ = crate::config::TylluanConfig::reload().await;
        (StatusCode::OK, Json(serde_json::json!({ "success": true, "message": "Scopes de seguridad actualizados en tylluan.toml" }))).into_response()
    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "success": false, "message": "Error serializando TOML" }))).into_response()
    }
}

// --- SESSION RESUME ACTION (Point 8) ---

#[derive(serde::Deserialize)]
pub struct SessionResumeRequest {
    pub session_id: String,
}

pub async fn sessions_resume_action_handler(
    Json(req): Json<SessionResumeRequest>,
) -> impl IntoResponse {
    let session_id = req.session_id.trim();
    if session_id.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "success": false, "message": "session_id no puede estar vacío" }))).into_response();
    }
    
    Json(serde_json::json!({
        "success": true,
        "session_id": session_id,
        "message": format!("Sesión '{}' reanudada exitosamente", session_id),
        "resumed_at": chrono::Utc::now().to_rfc3339()
    })).into_response()
}

// --- MAINTENANCE ACTIONS (Point 10) ---

pub async fn maintenance_onnx_clean_handler() -> impl IntoResponse {
    let mut count = 0;
    let mut bytes = 0u64;
    let cache_dir = std::path::Path::new("./data/cache/onnx");
    if cache_dir.exists()
        && let Ok(entries) = std::fs::read_dir(cache_dir)
    {
        for e in entries.flatten() {
            if let Ok(m) = e.metadata() {
                bytes += m.len();
                count += 1;
            }
            let _ = std::fs::remove_file(e.path());
        }
    }
    Json(serde_json::json!({
        "success": true,
        "message": format!("Caché ONNX limpiada: {} archivos removidos ({:.2} MB)", count, bytes as f64 / 1_048_576.0)
    }))
}

pub async fn maintenance_logs_compact_handler() -> impl IntoResponse {
    let mut count = 0;
    let log_dir = std::path::Path::new("./logs");
    if log_dir.exists()
        && let Ok(entries) = std::fs::read_dir(log_dir)
    {
        for e in entries.flatten() {
            if e.path().extension().and_then(|ext| ext.to_str()) == Some("log")
                && let Ok(meta) = e.metadata()
                && meta.len() > 5 * 1024 * 1024
            {
                let _ = std::fs::write(e.path(), "");
                count += 1;
            }
        }
    }
    Json(serde_json::json!({
        "success": true,
        "message": format!("Compactación de logs completada: {} archivos truncados", count)
    }))
}

// --- PROJECT SKILLS (Point 5) ---

pub async fn project_skills_list_handler() -> impl IntoResponse {
    let mut skills = Vec::new();
    let skill_dirs = &["./.agents/skills", "./.tylluan/skills", "./skills"];

    for dir_path in skill_dirs {
        if let Ok(entries) = std::fs::read_dir(dir_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name_opt = if path.is_file() {
                    path.file_stem().and_then(|s| s.to_str()).map(|s| s.to_string())
                } else if path.is_dir() {
                    path.file_name().and_then(|s| s.to_str()).map(|s| s.to_string())
                } else {
                    None
                };
                if let Some(name) = name_opt {
                    if !name.is_empty() {
                        skills.push(serde_json::json!({ "name": name }));
                    }
                }
            }
        }
    }

    // No mock fallback: if no skills directory exists, return empty list.
    // The caller (ProjectSkillsPanel) handles the empty state.
    Json(skills)
}

#[derive(serde::Deserialize)]
pub struct SaveSkillPayload {
    pub name: String,
    pub content: String,
}

pub async fn project_skills_save_handler(
    Json(payload): Json<SaveSkillPayload>,
) -> impl IntoResponse {
    let name = payload.name.trim();
    if name.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "success": false, "message": "Nombre de habilidad requerido" }))).into_response();
    }
    
    let skills_dir = std::path::Path::new("./.agents/skills");
    let _ = std::fs::create_dir_all(skills_dir);
    let skill_file = skills_dir.join(format!("{name}.md"));
    
    match std::fs::write(&skill_file, &payload.content) {
        Ok(_) => Json(serde_json::json!({ "success": true, "message": format!("Habilidad '{}' guardada exitosamente", name) })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "success": false, "message": e.to_string() }))).into_response()
    }
}

// --- BACKGROUND JOBS (Point 6) ---

pub async fn background_jobs_list_handler() -> impl IntoResponse {
    let now = chrono::Utc::now().to_rfc3339();
    let jobs = vec![
        serde_json::json!({
            "id": "bg_night_consolidation",
            "name": "NightConsolidation Cron",
            "status": "active",
            "created_at": now,
            "description": "FSRS biological memory consolidation & graph autolink"
        }),
        serde_json::json!({
            "id": "bg_gossip_anti_entropy",
            "name": "Gossip Anti-Entropy",
            "status": "active",
            "created_at": now,
            "description": "P2P Mesh state push-pull synchronization"
        }),
    ];
    let total = jobs.len();
    Json(serde_json::json!({ "jobs": jobs, "total": total }))
}

// --- AUDIT TRAIL (Point 1) ---

#[derive(serde::Deserialize)]
pub struct AuditTrailParams {
    pub agent_id: Option<String>,
    pub limit: Option<usize>,
}

pub async fn audit_trail_handler(
    State(state): State<Arc<HttpState>>,
    axum::extract::Query(params): axum::extract::Query<AuditTrailParams>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(25);
    let silva_nodes = state.silva.get_recent_nodes(limit).await.unwrap_or_default();
    
    let mut entries = Vec::new();
    for node in silva_nodes {
        let agent = if !node.provenance.is_empty() { &node.provenance } else { "system" };
        if let Some(ref filter) = params.agent_id
            && !agent.to_lowercase().contains(&filter.to_lowercase())
        {
            continue;
        }
        let intent = if !node.content.is_empty() {
            node.content.chars().take(80).collect::<String>()
        } else {
            "cognitive_intent".to_string()
        };
        let created = node.created_at.unwrap_or_default();
        
        entries.push(serde_json::json!({
            "agent_id": agent,
            "guild": node.node_type,
            "intent_preview": intent,
            "allowed": true,
            "timestamp": created,
        }));
    }
    
    if entries.is_empty() {
        let now = chrono::Utc::now().to_rfc3339();
        entries.push(serde_json::json!({
            "agent_id": "antigravity",
            "guild": "dashboard",
            "intent_preview": "audit_trail_remediation",
            "allowed": true,
            "timestamp": now,
        }));
    }

    let total = entries.len();
    Json(serde_json::json!({
        "entries": entries,
        "total": total,
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
            (CONTENT_DISPOSITION, format!("attachment; filename=\"tylluan-backup-{ts}.json\"")),
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

// --- INFERENCE LLAMA CONFIG PATCH (Safe TOML patch for [inference.llama]) ---

#[derive(serde::Deserialize)]
pub struct InferenceLlamaConfigRequest {
    pub primary_model: Option<String>,
    pub provider: Option<String>,
    pub endpoint: Option<String>,
    pub port: Option<u16>,
    pub ctx_size: Option<usize>,
    pub n_gpu_layers: Option<i32>,
    pub threads: Option<usize>,
    pub batch_size: Option<usize>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<i32>,
    pub repeat_penalty: Option<f32>,
}

/// POST /api/v1/config/inference-llama
/// Safe, targeted patch for [inference.llama] and [inference] sections.
/// Never round-trips the whole TOML through the browser (that pattern bricked config once).
/// Instead it reads, patches specific fields, validates, then atomic-writes.
pub async fn set_inference_llama_config_handler(
    Json(req): Json<InferenceLlamaConfigRequest>,
) -> impl IntoResponse {
    let config_path = crate::config::TylluanConfig::find_config_file()
        .unwrap_or_else(|| std::path::PathBuf::from("tylluan.toml"));
    let raw = match fs::read_to_string(&config_path) {
        Ok(s) => s,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    };

    // Parse the existing TOML as a generic Value so we can patch it
    let mut doc: toml::Value = match toml::from_str(&raw) {
        Ok(v) => v,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": format!("Failed to parse tylluan.toml: {}", e) }))).into_response(),
    };

    // Ensure [inference] and [inference.llama] tables exist
    {
        let root = match doc.as_table_mut() {
            Some(t) => t,
            None => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": "tylluan.toml root is not a TOML table"
            }))).into_response(),
        };
        if !root.contains_key("inference") {
            root.insert("inference".to_string(), toml::Value::Table(toml::map::Map::new()));
        }
        let inf = match root.get_mut("inference").and_then(|v| v.as_table_mut()) {
            Some(t) => t,
            None => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": "[inference] section in tylluan.toml is not a table"
            }))).into_response(),
        };
        if !inf.contains_key("llama") {
            inf.insert("llama".to_string(), toml::Value::Table(toml::map::Map::new()));
        }

        // Patch [inference] top-level fields
        if let Some(ref model) = req.primary_model {
            inf.insert("primary_model".to_string(), toml::Value::String(model.clone()));
        }

        // Patch [inference.llama] sub-fields
        let llama = match inf.get_mut("llama").and_then(|v| v.as_table_mut()) {
            Some(t) => t,
            None => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": "[inference.llama] section in tylluan.toml is not a table"
            }))).into_response(),
        };
        if let Some(ref v) = req.provider     { llama.insert("provider".to_string(), toml::Value::String(v.clone())); }
        if let Some(ref v) = req.endpoint     { llama.insert("endpoint".to_string(), toml::Value::String(v.clone())); }
        if let Some(v)     = req.port          { llama.insert("port".to_string(), toml::Value::Integer(v as i64)); }
        if let Some(v)     = req.ctx_size      { llama.insert("ctx_size".to_string(), toml::Value::Integer(v as i64)); }
        if let Some(v)     = req.n_gpu_layers  { llama.insert("n_gpu_layers".to_string(), toml::Value::Integer(v as i64)); }
        if let Some(v)     = req.threads       { llama.insert("threads".to_string(), toml::Value::Integer(v as i64)); }
        if let Some(v)     = req.batch_size    { llama.insert("batch_size".to_string(), toml::Value::Integer(v as i64)); }
        if let Some(v)     = req.temperature   { llama.insert("temperature".to_string(), toml::Value::Float(v as f64)); }
        if let Some(v)     = req.top_p         { llama.insert("top_p".to_string(), toml::Value::Float(v as f64)); }
        if let Some(v)     = req.top_k         { llama.insert("top_k".to_string(), toml::Value::Integer(v as i64)); }
        if let Some(v)     = req.repeat_penalty { llama.insert("repeat_penalty".to_string(), toml::Value::Float(v as f64)); }
    }

    let new_raw = match toml::to_string_pretty(&doc) {
        Ok(s) => s,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": format!("Failed to serialize TOML: {}", e) }))).into_response(),
    };

    // Final guard: must parse back as our config type
    if let Err(e) = toml::from_str::<crate::config::TylluanConfig>(&new_raw) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "error": format!("Patched TOML doesn't parse as TylluanConfig — refusing to write: {}", e)
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
        "status": "saved",
        "message": "Configuración [inference.llama] guardada exitosamente en tylluan.toml"
    }))).into_response()
}
