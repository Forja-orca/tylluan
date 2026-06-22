use axum::{
    Json,
    extract::{State, Path},
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;
use std::collections::HashMap;
use serde::Deserialize;
use crate::transport::http::HttpState;

#[derive(Deserialize)]
pub struct AddMcpServerRequest {
    name: String,
    /// HTTP Streamable MCP endpoint
    url: Option<String>,
    /// Classic SSE MCP: GET endpoint (persistent stream)
    sse_url: Option<String>,
    /// Classic SSE MCP: POST endpoint for requests
    post_url: Option<String>,
    command: Option<String>,
    args: Option<Vec<String>>,
    cwd: Option<String>,
    env: Option<HashMap<String, String>>,
    headers: Option<HashMap<String, String>>,
    timeout_ms: Option<u64>,
}

pub async fn list_mcp_servers_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let config = state.config.read().await;
    let reg_arc = state.registry.arc();
    let reg = reg_arc.read().await;

    let servers: Vec<serde_json::Value> = config.external_mcp.iter().map(|ext| {
        let (running, tools) = if let Some(guild) = reg.guilds.get(&ext.name) {
            let tools_list: Vec<serde_json::Value> = guild.tools.iter().map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description
                })
            }).collect();
            (guild.is_running(), tools_list)
        } else {
            (false, Vec::new())
        };

        serde_json::json!({
            "name": ext.name,
            "url": ext.url,
            "sse_url": ext.sse_url,
            "post_url": ext.post_url,
            "command": ext.command,
            "args": ext.args,
            "cwd": ext.cwd,
            "env": ext.env,
            "headers": ext.headers,
            "timeout_ms": ext.timeout_ms,
            "active": ext.active,
            "running": running,
            "tools": tools,
        })
    }).collect();
    (StatusCode::OK, Json(servers)).into_response()
}

pub async fn add_mcp_server_handler(State(state): State<Arc<HttpState>>, Json(req): Json<AddMcpServerRequest>) -> impl IntoResponse {
    if req.name.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "name is required"}))).into_response();
    }

    // Check for duplicate
    {
        let config = state.config.read().await;
        if config.external_mcp.iter().any(|e| e.name == req.name) {
            return (StatusCode::CONFLICT, Json(serde_json::json!({"error": format!("External MCP '{}' already exists", req.name)}))).into_response();
        }
    }

    // Validate: must have url, sse_url+post_url, or command
    let has_sse = req.sse_url.is_some() && req.post_url.is_some();
    if req.url.is_none() && req.command.is_none() && !has_sse {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "Provide 'url' (HTTP Streamable), 'sse_url'+'post_url' (Classic SSE), or 'command' (stdio)"
        }))).into_response();
    }

    // Register with guild registry
    {
        let reg_arc = state.registry.arc();
        let mut reg = reg_arc.write().await;
        if has_sse {
            let sse_url = req.sse_url.as_deref().unwrap_or("");
            let post_url = req.post_url.as_deref().unwrap_or("");
            if sse_url.is_empty() || post_url.is_empty() {
                return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "sse_url and post_url required for SSE MCP"}))).into_response();
            }
            reg.register_sse_mcp(
                &req.name,
                sse_url,
                post_url,
                req.headers.clone().unwrap_or_default(),
                req.timeout_ms,
            );
        } else if let Some(url) = &req.url {
            reg.register_http_mcp(&req.name, url, req.headers.clone().unwrap_or_default(), req.timeout_ms);
        } else if let Some(command) = &req.command {
            reg.register_external(&req.name, command, req.args.clone().unwrap_or_default(), req.cwd.clone().map(std::path::PathBuf::from), req.env.clone(), req.timeout_ms);
        }
    }

    // Spawn the new server
    let _ = state.registry.ensure_running(&req.name).await;

    // Add to config
    {
        let mut config = state.config.write().await;
        config.external_mcp.push(crate::config::ExternalMcpConfig {
            name: req.name.clone(),
            command: req.command.clone(),
            args: req.args.clone(),
            cwd: req.cwd.clone(),
            url: req.url.clone(),
            sse_url: req.sse_url.clone(),
            post_url: req.post_url.clone(),
            env: req.env.clone(),
            headers: req.headers.clone(),
            timeout_ms: req.timeout_ms,
            active: true,
        });
    }

    // Persist config
    let _ = persist_external_mcp_config(&state.config).await;

    (StatusCode::CREATED, Json(serde_json::json!({"status": "registered", "name": req.name}))).into_response()
}

pub async fn remove_mcp_server_handler(State(state): State<Arc<HttpState>>, Path(name): Path<String>) -> impl IntoResponse {
    // Kill guild process if running
    let _ = state.registry.kill_guild(&name).await;

    // Remove from guild registry hashmap
    {
        let reg_arc = state.registry.arc();
        let mut reg = reg_arc.write().await;
        reg.guilds.remove(&name);
    }

    // Remove from config
    {
        let mut config = state.config.write().await;
        config.external_mcp.retain(|e| e.name != name);
    }

    // Persist config
    let _ = persist_external_mcp_config(&state.config).await;

    (StatusCode::OK, Json(serde_json::json!({"status": "removed", "name": name}))).into_response()
}

#[derive(Deserialize)]
pub struct UpdateMcpServerRequest {
    active: Option<bool>,
    url: Option<String>,
    sse_url: Option<String>,
    post_url: Option<String>,
    command: Option<String>,
    args: Option<Vec<String>>,
    cwd: Option<String>,
    env: Option<HashMap<String, String>>,
    headers: Option<HashMap<String, String>>,
    timeout_ms: Option<u64>,
}

pub async fn update_mcp_server_handler(
    State(state): State<Arc<HttpState>>,
    Path(name): Path<String>,
    Json(req): Json<UpdateMcpServerRequest>,
) -> impl IntoResponse {
    // Validate: at least one field to update
    if req.active.is_none() && req.url.is_none() && req.sse_url.is_none()
        && req.post_url.is_none() && req.command.is_none()
        && req.args.is_none() && req.cwd.is_none() && req.env.is_none()
        && req.headers.is_none() && req.timeout_ms.is_none()
    {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "No fields to update"}))).into_response();
    }

    // Phase 1: Update config entry and detect changes (inside config lock)
    let (old_active, new_active, launcher_changed) = {
        let mut config = state.config.write().await;
        let ext = match config.external_mcp.iter_mut().find(|e| e.name == name) {
            Some(e) => e,
            None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": format!("External MCP '{}' not found", name)}))).into_response(),
        };

        let old_active = ext.active;
        let old_url = ext.url.clone();
        let old_sse_url = ext.sse_url.clone();
        let old_post_url = ext.post_url.clone();
        let old_command = ext.command.clone();

        // Apply field updates
        if let Some(url) = req.url {
            ext.url = if url.is_empty() { None } else { Some(url) };
        }
        if let Some(sse_url) = req.sse_url {
            ext.sse_url = if sse_url.is_empty() { None } else { Some(sse_url) };
        }
        if let Some(post_url) = req.post_url {
            ext.post_url = if post_url.is_empty() { None } else { Some(post_url) };
        }
        if let Some(command) = req.command {
            ext.command = if command.is_empty() { None } else { Some(command) };
        }
        if let Some(args) = req.args {
            ext.args = if args.is_empty() { None } else { Some(args) };
        }
        if let Some(cwd) = req.cwd {
            ext.cwd = if cwd.is_empty() { None } else { Some(cwd) };
        }
        if let Some(env) = req.env {
            ext.env = if env.is_empty() { None } else { Some(env) };
        }
        if let Some(headers) = req.headers {
            ext.headers = if headers.is_empty() { None } else { Some(headers) };
        }
        if let Some(timeout_ms) = req.timeout_ms {
            ext.timeout_ms = Some(timeout_ms);
        }

        // Detect if launcher type changed
        let launcher_changed = ext.url != old_url
            || ext.sse_url != old_sse_url
            || ext.post_url != old_post_url
            || ext.command != old_command;

        // Apply active toggle
        let new_active = req.active.unwrap_or(ext.active);
        ext.active = new_active;

        (old_active, new_active, launcher_changed)
    };

    // Phase 2: Guild lifecycle management (outside config lock)
    if old_active && (!new_active || launcher_changed) {
        let _ = state.registry.kill_guild(&name).await;
        let reg_arc = state.registry.arc();
        let mut reg = reg_arc.write().await;
        reg.guilds.remove(&name);
    }

    if new_active && (!old_active || launcher_changed) {
        // Re-register with updated launcher
        {
            let config = state.config.read().await;
            let reg_arc = state.registry.arc();
            let mut reg = reg_arc.write().await;
            if let Some(ext) = config.external_mcp.iter().find(|e| e.name == name) {
                if let (Some(sse_url), Some(post_url)) = (&ext.sse_url, &ext.post_url) {
                    reg.register_sse_mcp(
                        &name,
                        sse_url,
                        post_url,
                        ext.headers.clone().unwrap_or_default(),
                        ext.timeout_ms,
                    );
                } else if let Some(url) = &ext.url {
                    reg.register_http_mcp(&name, url, ext.headers.clone().unwrap_or_default(), ext.timeout_ms);
                } else if let Some(command) = &ext.command {
                    reg.register_external(&name, command, ext.args.clone().unwrap_or_default(), ext.cwd.clone().map(std::path::PathBuf::from), ext.env.clone(), ext.timeout_ms);
                }
            }
        }
        let _ = state.registry.ensure_running(&name).await;
    }

    // Persist config
    let _ = persist_external_mcp_config(&state.config).await;

    (StatusCode::OK, Json(serde_json::json!({"status": "updated", "name": name, "active": new_active}))).into_response()
}

/// POST /api/v1/mcp/external/discover
/// Scans known MCP config files on this system and registers new servers as inactive.
/// Sources: Claude Desktop (claude_desktop_config.json), Claude Code (~/.claude/settings.json).
/// Self-references (any server pointing to tylluan-nexus.exe or localhost:3030) are skipped.
pub async fn discover_mcp_servers_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let mut discovered: Vec<String> = Vec::new();
    let mut skipped_existing: Vec<String> = Vec::new();
    let mut skipped_self: Vec<String> = Vec::new();
    let mut sources_scanned: Vec<String> = Vec::new();

    // Paths to scan on Windows
    let appdata = std::env::var("APPDATA").unwrap_or_default();
    let userprofile = std::env::var("USERPROFILE").unwrap_or_default();
    let config_paths = vec![
        format!("{appdata}\\Claude\\claude_desktop_config.json"),
        format!("{userprofile}\\.claude\\settings.json"),
        format!("{userprofile}\\.gemini\\my-custom-agent\\mcp_config.json"),
    ];

    // Collect existing names to skip duplicates
    let existing_names: Vec<String> = {
        let config = state.config.read().await;
        config.external_mcp.iter().map(|e| e.name.clone()).collect()
    };

    let mut new_servers: Vec<crate::config::ExternalMcpConfig> = Vec::new();

    for path in &config_paths {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => { sources_scanned.push(path.clone()); c }
            Err(_) => continue,
        };
        let json: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let servers_obj = json.get("mcpServers").and_then(|v| v.as_object());
        if let Some(servers) = servers_obj {
            for (name, def) in servers {
                // Skip self-references: any command containing tylluan-nexus or url pointing to :3030
                let cmd = def.get("command").and_then(|v| v.as_str()).unwrap_or("");
                let url = def.get("url").and_then(|v| v.as_str()).unwrap_or("");
                let is_self = cmd.to_lowercase().contains("tylluan-nexus")
                    || url.contains(":3030")
                    || name == "tylluannexus-o3";
                if is_self {
                    skipped_self.push(name.clone());
                    continue;
                }
                // Skip already registered
                if existing_names.contains(name) || new_servers.iter().any(|s| &s.name == name) {
                    skipped_existing.push(name.clone());
                    continue;
                }
                // Parse env
                let env: Option<std::collections::HashMap<String, String>> = def
                    .get("env")
                    .and_then(|v| v.as_object())
                    .map(|obj| obj.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect());
                // Parse args
                let args: Option<Vec<String>> = def
                    .get("args")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect());

                let entry = crate::config::ExternalMcpConfig {
                    name: name.clone(),
                    command: if cmd.is_empty() { None } else { Some(cmd.to_string()) },
                    args,
                    cwd: None,
                    url: if url.is_empty() { None } else { Some(url.to_string()) },
                    sse_url: None,
                    post_url: None,
                    env,
                    headers: None,
                    timeout_ms: None,
                    active: false,
                };
                discovered.push(name.clone());
                new_servers.push(entry);
            }
        }
    }

    if !new_servers.is_empty() {
        {
            let mut config = state.config.write().await;
            config.external_mcp.extend(new_servers);
        }
        let _ = persist_external_mcp_config(&state.config).await;
    }

    (StatusCode::OK, Json(serde_json::json!({
        "discovered": discovered.len(),
        "servers": discovered,
        "skipped_existing": skipped_existing,
        "skipped_self": skipped_self,
        "sources_scanned": sources_scanned,
    }))).into_response()
}

pub async fn persist_external_mcp_config(config_lock: &Arc<tokio::sync::RwLock<crate::config::TylluanConfig>>) -> Result<(), String> {
    let content = {
        let config = config_lock.read().await;
        toml::to_string_pretty(&*config).map_err(|e| e.to_string())?
    };
    let config_path = crate::config::TylluanConfig::find_config_file()
        .unwrap_or_else(|| std::path::PathBuf::from("tylluan.toml"));
    let tmp_path = config_path.with_extension("toml.tmp");
    std::fs::write(&tmp_path, &content).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp_path, &config_path).map_err(|e| e.to_string())?;
    Ok(())
}
