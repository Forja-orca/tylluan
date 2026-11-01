use axum::{
    Json,
    extract::{State, Path},
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;
use std::sync::{Mutex, OnceLock};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use serde::Deserialize;
use crate::transport::http::HttpState;

#[derive(Deserialize)]
pub struct AddMcpServerRequest {
    name: String,
    /// D-86: nonce returned by the approval challenge.
    approval_nonce: Option<String>,
    /// D-86: one-time code shown only in the kernel console.
    approval_code: Option<String>,
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

    let entry = add_entry_from_request(&req);
    let expected = launcher_signature(&entry);
    if let Err(response) = authorize_mcp_change(
        &expected,
        req.approval_nonce.as_deref(),
        req.approval_code.as_deref(),
    ) {
        return *response;
    }
    audit_mcp_change("add", &entry.name, None, &entry);

    // Register with guild registry
    {
        let reg_arc = state.registry.arc();
        let mut reg = reg_arc.write().await;
        if has_sse {
            let sse_url = entry.sse_url.as_deref().unwrap_or("");
            let post_url = entry.post_url.as_deref().unwrap_or("");
            if sse_url.is_empty() || post_url.is_empty() {
                return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "sse_url and post_url required for SSE MCP"}))).into_response();
            }
            reg.register_sse_mcp(
                &entry.name,
                sse_url,
                post_url,
                entry.headers.clone().unwrap_or_default(),
                entry.timeout_ms,
            );
        } else if let Some(url) = &entry.url {
            reg.register_http_mcp(&entry.name, url, entry.headers.clone().unwrap_or_default(), entry.timeout_ms);
        } else if let Some(command) = &entry.command {
            reg.register_external(&entry.name, command, entry.args.clone().unwrap_or_default(), entry.cwd.clone().map(std::path::PathBuf::from), entry.env.clone(), entry.timeout_ms);
        }
    }

    // Spawn the new server
    let _ = state.registry.ensure_running(&entry.name).await;

    // Add to config
    {
        let mut config = state.config.write().await;
        config.external_mcp.push(entry.clone());
    }

    // Persist config
    let _ = persist_external_mcp_config(&state.config).await;

    (StatusCode::CREATED, Json(serde_json::json!({"status": "registered", "name": req.name}))).into_response()
}

fn launcher_signature_from_parts(
    command: Option<&str>,
    args: Option<&[String]>,
    url: Option<&str>,
    sse_url: Option<&str>,
    post_url: Option<&str>,
) -> String {
    if let Some(command) = command.filter(|c| !c.trim().is_empty()) {
        let mut out = command.trim().to_string();
        if let Some(args) = args {
            for arg in args {
                out.push(' ');
                out.push_str(arg);
            }
        }
        return out;
    }
    if let Some(url) = url.filter(|u| !u.trim().is_empty()) {
        return url.trim().to_string();
    }
    if let (Some(sse), Some(post)) = (sse_url, post_url) {
        return format!("sse:{}|post:{}", sse.trim(), post.trim());
    }
    "(none)".to_string()
}

fn launcher_signature(ext: &crate::config::ExternalMcpConfig) -> String {
    launcher_signature_from_parts(
        ext.command.as_deref(),
        ext.args.as_deref(),
        ext.url.as_deref(),
        ext.sse_url.as_deref(),
        ext.post_url.as_deref(),
    )
}

const MCP_APPROVAL_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApprovalError {
    UnknownOrUsed,
    Expired,
    WrongSignature,
    WrongCode,
}

struct PendingApproval {
    signature: String,
    code: String,
    expires_at: Instant,
}

#[derive(Default)]
struct ApprovalStore {
    pending: Mutex<HashMap<String, PendingApproval>>,
}

impl ApprovalStore {
    fn issue_with_ttl(&self, signature: &str, ttl: Duration) -> (String, String) {
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let code = format!(
            "{:010x}",
            (uuid::Uuid::new_v4().as_u128() >> 88) as u64
        );
        let mut pending = self.pending.lock().unwrap_or_else(|p| p.into_inner());
        let now = Instant::now();
        pending.retain(|_, approval| approval.expires_at > now);
        pending.insert(
            nonce.clone(),
            PendingApproval {
                signature: signature.to_string(),
                code: code.clone(),
                expires_at: now + ttl,
            },
        );
        (nonce, code)
    }

    fn consume(&self, nonce: &str, code: &str, signature: &str) -> Result<(), ApprovalError> {
        let mut pending = self.pending.lock().unwrap_or_else(|p| p.into_inner());
        let approval = pending
            .get(nonce)
            .ok_or(ApprovalError::UnknownOrUsed)?;
        if approval.expires_at <= Instant::now() {
            pending.remove(nonce);
            return Err(ApprovalError::Expired);
        }
        if approval.signature != signature {
            return Err(ApprovalError::WrongSignature);
        }
        if approval.code != code {
            return Err(ApprovalError::WrongCode);
        }
        pending.remove(nonce);
        Ok(())
    }
}

fn approval_store() -> &'static ApprovalStore {
    static STORE: OnceLock<ApprovalStore> = OnceLock::new();
    STORE.get_or_init(ApprovalStore::default)
}

fn issue_mcp_approval(signature: &str) -> (String, String) {
    let (nonce, code) = approval_store().issue_with_ttl(signature, MCP_APPROVAL_TTL);
    tracing::warn!(
        nonce = %nonce,
        code = %code,
        "MCP external approval code (60s, single-use) — read it in the kernel console"
    );
    (nonce, code)
}

fn authorize_mcp_change(
    signature: &str,
    nonce: Option<&str>,
    code: Option<&str>,
) -> Result<(), Box<axum::response::Response>> {
    match (nonce, code) {
        (Some(nonce), Some(code)) => match approval_store().consume(nonce, code, signature) {
            Ok(()) => Ok(()),
            Err(reason) => Err(Box::new(
                (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({
                        "error": "approval_invalid",
                        "reason": format!("{reason:?}"),
                    })),
                )
                    .into_response(),
            )),
        },
        (None, None) => {
            let (nonce, _code) = issue_mcp_approval(signature);
            Err(Box::new(
                (
                    StatusCode::PRECONDITION_REQUIRED,
                    Json(serde_json::json!({
                        "error": "approval_required",
                        "nonce": nonce,
                        "expires_in_secs": MCP_APPROVAL_TTL.as_secs(),
                        "message": format!(
                            "approval_required: nonce={nonce}. Read the one-time code in the kernel console and retry with approval_nonce + approval_code."
                        )
                    })),
                )
                    .into_response(),
            ))
        }
        _ => Err(Box::new(
            (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": "approval_invalid",
                    "reason": "approval_nonce and approval_code must be sent together",
                })),
            )
                .into_response(),
        )),
    }
}

fn add_entry_from_request(req: &AddMcpServerRequest) -> crate::config::ExternalMcpConfig {
    crate::config::ExternalMcpConfig {
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
    }
}

fn update_touches_launcher(req: &UpdateMcpServerRequest) -> bool {
    req.url.is_some()
        || req.sse_url.is_some()
        || req.post_url.is_some()
        || req.command.is_some()
        || req.args.is_some()
        || req.cwd.is_some()
        || req.env.is_some()
        || req.headers.is_some()
}

fn apply_update_fields(ext: &mut crate::config::ExternalMcpConfig, req: &UpdateMcpServerRequest) {
    if let Some(url) = req.url.clone() {
        ext.url = if url.is_empty() { None } else { Some(url) };
    }
    if let Some(sse_url) = req.sse_url.clone() {
        ext.sse_url = if sse_url.is_empty() { None } else { Some(sse_url) };
    }
    if let Some(post_url) = req.post_url.clone() {
        ext.post_url = if post_url.is_empty() { None } else { Some(post_url) };
    }
    if let Some(command) = req.command.clone() {
        ext.command = if command.is_empty() { None } else { Some(command) };
    }
    if let Some(args) = req.args.clone() {
        ext.args = if args.is_empty() { None } else { Some(args) };
    }
    if let Some(cwd) = req.cwd.clone() {
        ext.cwd = if cwd.is_empty() { None } else { Some(cwd) };
    }
    if let Some(env) = req.env.clone() {
        ext.env = if env.is_empty() { None } else { Some(env) };
    }
    if let Some(headers) = req.headers.clone() {
        ext.headers = if headers.is_empty() { None } else { Some(headers) };
    }
    if let Some(timeout_ms) = req.timeout_ms {
        ext.timeout_ms = Some(timeout_ms);
    }
}

fn discovery_proposals(entries: &[crate::config::ExternalMcpConfig]) -> Vec<serde_json::Value> {
    entries
        .iter()
        .map(|entry| {
            serde_json::json!({
                "name": entry.name,
                "signature": launcher_signature(entry),
                "config": entry,
            })
        })
        .collect()
}

fn audit_mcp_change(action: &str, name: &str, old: Option<&crate::config::ExternalMcpConfig>, new: &crate::config::ExternalMcpConfig) {
    let old_sig = old.map(launcher_signature).unwrap_or_else(|| "(none)".to_string());
    let new_sig = launcher_signature(new);
    let preview = format!(
        "name={name}\nold={}\nnew={}",
        crate::logging::scrub(&old_sig),
        crate::logging::scrub(&new_sig)
    );
    crate::transport::server::handler_do::audit_or_warn(
        &format!("mcp_external_{action}"),
        "mcp_external",
        action,
        "operator",
        true,
        &preview,
        0,
        true,
    );
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
    /// D-86: nonce returned by the approval challenge.
    approval_nonce: Option<String>,
    /// D-86: one-time code shown only in the kernel console.
    approval_code: Option<String>,
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

    // D-86: validate the normalized launcher and its one-time approval before
    // touching the live config.
    let old_entry = {
        let config = state.config.read().await;
        match config.external_mcp.iter().find(|e| e.name == name) {
            Some(e) => e.clone(),
            None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": format!("External MCP '{}' not found", name)}))).into_response(),
        }
    };
    let candidate = {
        let mut c = old_entry.clone();
        apply_update_fields(&mut c, &req);
        c
    };
    let expected = launcher_signature(&candidate);
    if update_touches_launcher(&req)
        && let Err(response) = authorize_mcp_change(
            &expected,
            req.approval_nonce.as_deref(),
            req.approval_code.as_deref(),
        )
    {
        return *response;
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

        apply_update_fields(ext, &req);

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

    audit_mcp_change("update", &name, Some(&old_entry), &candidate);

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

#[derive(Deserialize)]
pub struct ApproveMcpToolRequest {
    approval_nonce: Option<String>,
    approval_code: Option<String>,
}

/// POST /api/v1/mcp/external/{name}/tools/{tool}/approve
/// D-89: release a quarantined tool after the D-86 one-time approval.
pub async fn approve_mcp_tool_handler(
    State(state): State<Arc<HttpState>>,
    Path((name, tool)): Path<(String, String)>,
    Json(req): Json<ApproveMcpToolRequest>,
) -> impl IntoResponse {
    let fingerprint = {
        let reg_arc = state.registry.arc();
        let reg = reg_arc.read().await;
        let Some(guild) = reg.guilds.get(&name) else {
            return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "external MCP not found"}))).into_response();
        };
        let Some(fp) = guild.tool_integrity.quarantine_fingerprint(&tool) else {
            return (StatusCode::CONFLICT, Json(serde_json::json!({"error": "tool is not quarantined"}))).into_response();
        };
        fp.to_string()
    };

    let signature = format!("tool:{name}:{tool}:{fingerprint}");
    if let Err(response) = authorize_mcp_change(
        &signature,
        req.approval_nonce.as_deref(),
        req.approval_code.as_deref(),
    ) {
        return *response;
    }

    let reg_arc = state.registry.arc();
    let mut reg = reg_arc.write().await;
    {
        let Some(guild) = reg.guilds.get_mut(&name) else {
            return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "external MCP not found"}))).into_response();
        };
        if !guild.tool_integrity.approve(&tool, &fingerprint) {
            return (StatusCode::CONFLICT, Json(serde_json::json!({"error": "approval_fingerprint_mismatch"}))).into_response();
        }
    }
    // Re-read tools/list and rebuild tool_to_guild: the approved tool has to
    // become routable, otherwise `tylluan_do` still answers "unknown tool".
    if let Err(e) = reg.refresh_guild_tools(&name).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response();
    }
    (StatusCode::OK, Json(serde_json::json!({"status": "approved", "guild": name, "tool": tool}))).into_response()
}

/// POST /api/v1/mcp/external/discover
/// Scans known MCP config files on this system and registers new servers as inactive.
/// Sources: Claude Desktop (claude_desktop_config.json), Claude Code (~/.claude/settings.json).
/// Self-references (any server pointing to tylluan-nexus.exe or localhost:47004/3030) are skipped.
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
                // Skip self-references: any command containing tylluan-nexus or url pointing to :47004/:3030
                let cmd = def.get("command").and_then(|v| v.as_str()).unwrap_or("");
                let url = def.get("url").and_then(|v| v.as_str()).unwrap_or("");
                let is_self = cmd.to_lowercase().contains("tylluan-nexus")
                    || url.contains(":47004")
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

    // D1: discover is a proposal, not a registration. Nothing is persisted or
    // spawned until the operator POSTs the proposal to /api/v1/mcp/external
    // with the D-86 approval nonce + one-time code.
    let proposals = discovery_proposals(&new_servers);

    (StatusCode::OK, Json(serde_json::json!({
        "discovered": discovered.len(),
        "servers": discovered,
        "proposals": proposals,
        "skipped_existing": skipped_existing,
        "skipped_self": skipped_self,
        "sources_scanned": sources_scanned,
        "registered": 0,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(command: &str, args: &[&str], url: Option<&str>) -> crate::config::ExternalMcpConfig {
        crate::config::ExternalMcpConfig {
            name: "test".into(),
            command: Some(command.into()),
            args: Some(args.iter().map(|s| s.to_string()).collect()),
            cwd: None,
            url: url.map(str::to_string),
            sse_url: None,
            post_url: None,
            env: None,
            headers: None,
            timeout_ms: None,
            active: false,
        }
    }

    #[test]
    fn a4_launcher_signature_is_stable_and_normalized() {
        let e = entry("npx", &["-y", "@scope/server"], None);
        assert_eq!(launcher_signature(&e), "npx -y @scope/server");
        let http = entry("", &[], Some("https://mcp.example/mcp"));
        assert_eq!(launcher_signature(&http), "https://mcp.example/mcp");
    }

    #[test]
    fn d86_without_code_requests_approval() {
        let response = authorize_mcp_change("npx a", None, None)
            .expect_err("without approval code the handler must not authorize");
        assert_eq!(response.status(), StatusCode::PRECONDITION_REQUIRED);
    }

    #[test]
    fn d86_reused_code_is_rejected() {
        let store = ApprovalStore::default();
        let (nonce, code) = store.issue_with_ttl("npx a", Duration::from_secs(60));
        assert_eq!(store.consume(&nonce, &code, "npx a"), Ok(()));
        assert_eq!(
            store.consume(&nonce, &code, "npx a"),
            Err(ApprovalError::UnknownOrUsed)
        );
    }

    #[test]
    fn d86_expired_code_is_rejected() {
        let store = ApprovalStore::default();
        let (nonce, code) = store.issue_with_ttl("npx b", Duration::from_secs(0));
        assert_eq!(
            store.consume(&nonce, &code, "npx b"),
            Err(ApprovalError::Expired)
        );
    }

    #[test]
    fn d86_code_is_bound_to_launcher_signature() {
        let store = ApprovalStore::default();
        let (nonce, code) = store.issue_with_ttl("npx c", Duration::from_secs(60));
        assert_eq!(
            store.consume(&nonce, &code, "npx other"),
            Err(ApprovalError::WrongSignature)
        );
    }

    #[test]
    fn d86_wrong_code_is_rejected_without_consuming_the_challenge() {
        let store = ApprovalStore::default();
        let (nonce, code) = store.issue_with_ttl("npx d", Duration::from_secs(60));
        assert_eq!(
            store.consume(&nonce, "wrong", "npx d"),
            Err(ApprovalError::WrongCode)
        );
        assert_eq!(store.consume(&nonce, &code, "npx d"), Ok(()));
    }

    #[test]
    fn a4_update_signature_reflects_new_command_before_mutation() {
        let mut old = entry("npx", &["old"], None);
        old.name = "updatable".into();
        let req = UpdateMcpServerRequest {
            approval_nonce: None,
            approval_code: None,
            active: None,
            url: None,
            sse_url: None,
            post_url: None,
            command: Some("npx".into()),
            args: Some(vec!["new".into(), "--flag".into()]),
            cwd: None,
            env: None,
            headers: None,
            timeout_ms: None,
        };
        let mut candidate = old.clone();
        apply_update_fields(&mut candidate, &req);
        assert_eq!(launcher_signature(&candidate), "npx new --flag");
        assert_eq!(launcher_signature(&old), "npx old", "old entry stays unchanged");
    }

    #[test]
    fn d1_discovery_returns_proposals_not_registrations() {
        let a = entry("npx", &["a"], None);
        let mut b = entry("", &[], Some("https://mcp.example/b"));
        b.name = "b".into();
        let proposals = discovery_proposals(&[a, b]);
        assert_eq!(proposals.len(), 2);
        assert_eq!(proposals[0]["name"], "test");
        assert_eq!(proposals[0]["signature"], "npx a");
        assert_eq!(proposals[1]["signature"], "https://mcp.example/b");
        assert_eq!(proposals[0]["config"]["active"], false);
    }
}
