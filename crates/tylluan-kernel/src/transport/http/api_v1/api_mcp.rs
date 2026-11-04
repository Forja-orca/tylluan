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

/// D-104 fase 2: resuelve `vault:<nombre>` en un mapa de `headers`/`env` para las
/// rutas que registran servidores externos en caliente. Fail-closed: si la bóveda
/// no está o el secreto falta, la petición responde 400 con el motivo en vez de
/// registrar el servidor con una credencial vacía.
fn resolve_vault_or_400(
    map: &Option<std::collections::HashMap<String, String>>,
) -> Result<std::collections::HashMap<String, String>, String> {
    let Some(map) = map else {
        return Ok(std::collections::HashMap::new());
    };
    crate::security::secret_vault::resolve_with_vault(map).map_err(|e| e.to_string())
}

/// Respuesta 400 uniforme cuando la bóveda no puede resolver una credencial.
fn vault_unresolved_response(detail: String) -> axum::response::Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({
            "error": "VAULT_UNRESOLVED",
            "detail": detail,
        })),
    )
        .into_response()
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
            "env": super::config_safe::redacted_map(&ext.env),
            "headers": super::config_safe::redacted_map(&ext.headers),
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

    let entry = match add_entry_from_request(&req) {
        Ok(entry) => entry,
        Err(key) => return unsafe_env_response(key),
    };
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
            // D-104 fase 2: misma resolución `vault:` que en el arranque; fail-closed.
            let headers = match resolve_vault_or_400(&entry.headers) {
                Ok(h) => h,
                Err(e) => return vault_unresolved_response(e),
            };
            reg.register_sse_mcp(&entry.name, sse_url, post_url, headers, entry.timeout_ms);
        } else if let Some(url) = &entry.url {
            let headers = match resolve_vault_or_400(&entry.headers) {
                Ok(v) => v,
                Err(e) => return vault_unresolved_response(e),
            };
            reg.register_http_mcp(&entry.name, url, headers, entry.timeout_ms);
        } else if let Some(command) = &entry.command {
            let env = match resolve_vault_or_400(&entry.env) {
                Ok(v) => v,
                Err(e) => return vault_unresolved_response(e),
            };
            reg.register_external(&entry.name, command, entry.args.clone().unwrap_or_default(), entry.cwd.clone().map(std::path::PathBuf::from), Some(env), entry.timeout_ms);
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

/// D-92: the same `UNSAFE_ENV_KEYS` rule the importer enforces, now on the manual
/// add path (and on update, see `apply_update_fields_checked`). The environment
/// of an external MCP server travels to the child process, so a `PATH` or
/// `LD_PRELOAD` there is code execution, not configuration.
fn check_mcp_env(env: &Option<HashMap<String, String>>) -> Result<(), String> {
    match env.as_ref().and_then(unsafe_env_key) {
        Some(key) => Err(key),
        None => Ok(()),
    }
}

fn unsafe_env_response(key: String) -> axum::response::Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({
            "error": "unsafe_env",
            "key": key,
            "reason": "this variable changes how the child process is launched; set it directly in tylluan.toml if you really need it",
        })),
    )
        .into_response()
}

fn add_entry_from_request(
    req: &AddMcpServerRequest,
) -> Result<crate::config::ExternalMcpConfig, String> {
    check_mcp_env(&req.env)?;
    Ok(add_entry_unchecked(req))
}

fn add_entry_unchecked(req: &AddMcpServerRequest) -> crate::config::ExternalMcpConfig {
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

/// D-92: apply the update and refuse it if it would set a code-execution variable.
fn apply_update_fields_checked(
    ext: &mut crate::config::ExternalMcpConfig,
    req: &UpdateMcpServerRequest,
) -> Result<(), String> {
    apply_update_fields(ext, req);
    check_mcp_env(&ext.env)
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
        if let Err(key) = apply_update_fields_checked(&mut c, &req) {
            return unsafe_env_response(key);
        }
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

        if let Err(key) = check_mcp_env(&req.env) {
            return unsafe_env_response(key);
        }

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
                let headers = match resolve_vault_or_400(&ext.headers) {
                    Ok(h) => h,
                    Err(e) => return vault_unresolved_response(e),
                };
                if let (Some(sse_url), Some(post_url)) = (&ext.sse_url, &ext.post_url) {
                    reg.register_sse_mcp(&name, sse_url, post_url, headers, ext.timeout_ms);
                } else if let Some(url) = &ext.url {
                    reg.register_http_mcp(&name, url, headers, ext.timeout_ms);
                } else if let Some(command) = &ext.command {
                    let env = match resolve_vault_or_400(&ext.env) {
                        Ok(v) => v,
                        Err(e) => return vault_unresolved_response(e),
                    };
                    reg.register_external(&name, command, ext.args.clone().unwrap_or_default(), ext.cwd.clone().map(std::path::PathBuf::from), Some(env), ext.timeout_ms);
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
        if !guild.tool_integrity.approve(&tool, &fingerprint).await {
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

/// DELETE /api/v1/mcp/external/{name}/tools/{tool}/approve
/// D-90: drop a persisted approval. The server goes back to the seeding state
/// on its next discovery; the running process keeps its current tools.
pub async fn revoke_mcp_tool_handler(
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
        let Some(fp) = guild.tool_integrity.approved_fingerprint(&tool) else {
            return (StatusCode::CONFLICT, Json(serde_json::json!({"error": "tool is not approved"}))).into_response();
        };
        fp.to_string()
    };

    let signature = format!("revoke_tool:{name}:{tool}:{fingerprint}");
    if let Err(response) = authorize_mcp_change(
        &signature,
        req.approval_nonce.as_deref(),
        req.approval_code.as_deref(),
    ) {
        return *response;
    }

    let reg_arc = state.registry.arc();
    let mut reg = reg_arc.write().await;
    let Some(guild) = reg.guilds.get_mut(&name) else {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "external MCP not found"}))).into_response();
    };
    if !guild.tool_integrity.revoke(&tool).await {
        return (StatusCode::CONFLICT, Json(serde_json::json!({"error": "tool is not approved"}))).into_response();
    }
    (StatusCode::OK, Json(serde_json::json!({
        "status": "revoked",
        "guild": name,
        "tool": tool,
        "note": "the server re-seeds its baseline on the next discovery"
    }))).into_response()
}

/// D-90: read-only view of one external server's tool integrity state.
pub fn tool_integrity_view(guild: &crate::registry::guild_process::GuildProcess) -> serde_json::Value {
    use crate::registry::tool_integrity::fingerprint;
    let mut tools: Vec<serde_json::Value> = guild
        .tools
        .iter()
        .map(|tool| {
            let name = tool.name.to_string();
            serde_json::json!({
                "name": name,
                "status": "active",
                "fingerprint": fingerprint(tool),
            })
        })
        .collect();
    let quarantined: Vec<String> = guild
        .tool_integrity
        .quarantined_names()
        .into_iter()
        .collect();
    for name in quarantined {
        tools.push(serde_json::json!({
            "name": name,
            "status": "quarantined",
            "fingerprint": guild.tool_integrity.quarantine_fingerprint(&name),
            "reason": guild.tool_integrity.quarantine_reason(&name),
        }));
    }
    tools.sort_by(|a, b| {
        a["name"]
            .as_str()
            .unwrap_or_default()
            .cmp(b["name"].as_str().unwrap_or_default())
    });
    serde_json::json!({
        "guild": guild.name,
        "persistence": if guild.tool_integrity.has_store() { "silva" } else { "memory" },
        "tools": tools,
    })
}

/// GET /api/v1/mcp/external/{name}/tools
pub async fn list_mcp_tools_handler(
    State(state): State<Arc<HttpState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let reg_arc = state.registry.arc();
    let reg = reg_arc.read().await;
    let Some(guild) = reg.guilds.get(&name) else {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "external MCP not found"}))).into_response();
    };
    (StatusCode::OK, Json(tool_integrity_view(guild))).into_response()
}

#[derive(Deserialize)]
pub struct ImportMcpServersRequest {
    /// The client config document as JSON (Claude / Cursor / any `mcpServers`).
    /// Treated as untrusted data: nothing from it is executed or spawned.
    config: serde_json::Value,
    /// Optional label of where it came from (claude, cursor, ...) for the report.
    #[serde(default)]
    source: Option<String>,
    /// D-86: nonce returned by the approval challenge.
    approval_nonce: Option<String>,
    /// D-86: one-time code shown only in the kernel console.
    approval_code: Option<String>,
}

/// D-91: environment variables that turn "a config value" into code execution
/// in the child. An imported config is untrusted data, so an entry that tries to
/// set any of these is rejected whole instead of being silently stripped.
const UNSAFE_ENV_KEYS: &[&str] = &[
    "PATH",
    "PATHEXT",
    "COMSPEC",
    "SYSTEMROOT",
    "SYSTEMDRIVE",
    "WINDIR",
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "DYLD_INSERT_LIBRARIES",
    "DYLD_LIBRARY_PATH",
    "NODE_OPTIONS",
    "NODE_PATH",
    "PYTHONPATH",
    "PYTHONHOME",
    "PYTHONSTARTUP",
    "BASH_ENV",
    "ENV",
    "PERL5OPT",
    "RUBYOPT",
    "JAVA_TOOL_OPTIONS",
    "_JAVA_OPTIONS",
    "GIT_EXEC_PATH",
    "GIT_SSH_COMMAND",
];

fn unsafe_env_key(env: &HashMap<String, String>) -> Option<String> {
    env.keys()
        .find(|key| {
            let upper = key.to_ascii_uppercase();
            UNSAFE_ENV_KEYS.contains(&upper.as_str()) || upper.starts_with("TYLLUAN_")
        })
        .cloned()
}

/// F8 (anti-self-reference): a config that points back at this kernel is never
/// imported nor proposed. Checks the command, **its arguments** and any URL, so
/// `npx tylluan-nexus` is caught even though the command itself is just `npx`.
fn is_self_reference(name: &str, command: &str, args: &[String], urls: &[&str]) -> bool {
    let command = command.to_lowercase();
    let args_joined = args.join(" ").to_lowercase();
    command.contains("tylluan-nexus")
        || args_joined.contains("tylluan-nexus")
        || urls.iter().any(|u| u.contains(":47004") || u.contains(":3030"))
        || name == "tylluannexus-o3"
}

/// D-91: an import is a plan, not an execution. Pure so the rules can be tested
/// without a kernel, a registry or a filesystem.
pub fn plan_mcp_import(
    config: &serde_json::Value,
    existing_names: &[String],
) -> (Vec<crate::config::ExternalMcpConfig>, Vec<(String, String)>) {
    let mut accepted: Vec<crate::config::ExternalMcpConfig> = Vec::new();
    let mut rejected: Vec<(String, String)> = Vec::new();

    let Some(servers) = config.get("mcpServers").and_then(|v| v.as_object()) else {
        rejected.push((
            "*".to_string(),
            "missing mcpServers object".to_string(),
        ));
        return (accepted, rejected);
    };

    for (name, def) in servers {
        let reject = |reason: &str| (name.clone(), reason.to_string());

        if name.trim().is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        {
            rejected.push(reject("invalid_name"));
            continue;
        }
        if existing_names.iter().any(|n| n == name) {
            rejected.push(reject("already_registered"));
            continue;
        }
        if accepted.iter().any(|e| &e.name == name) {
            rejected.push(reject("duplicate_in_import"));
            continue;
        }
        let Some(def) = def.as_object() else {
            rejected.push(reject("invalid_definition"));
            continue;
        };

        let command = def.get("command").and_then(|v| v.as_str()).unwrap_or("");
        let url = def.get("url").and_then(|v| v.as_str()).unwrap_or("");
        let sse_url = def.get("sse_url").and_then(|v| v.as_str()).unwrap_or("");
        let post_url = def.get("post_url").and_then(|v| v.as_str()).unwrap_or("");

        // Untrusted data: a non-string element is a malformed entry, not
        // something to silently drop.
        let args: Vec<String> = match def.get("args") {
            None => Vec::new(),
            Some(serde_json::Value::Array(items)) => {
                let mut out = Vec::with_capacity(items.len());
                let mut bad = false;
                for item in items {
                    match item.as_str() {
                        Some(s) => out.push(s.to_string()),
                        None => {
                            bad = true;
                            break;
                        }
                    }
                }
                if bad {
                    rejected.push(reject("invalid_args"));
                    continue;
                }
                out
            }
            Some(_) => {
                rejected.push(reject("invalid_args"));
                continue;
            }
        };

        // F8: never import a pointer back to this kernel.
        if is_self_reference(name, command, &args, &[url, sse_url, post_url]) {
            rejected.push(reject("self_reference"));
            continue;
        }

        if !command.is_empty()
            && let Err(e) =
                crate::registry::guild_process::validate_imported_command(command, &args)
        {
            rejected.push(reject(&format!("unsafe_command: {e}")));
            continue;
        }
        if command.is_empty() && url.is_empty() && (sse_url.is_empty() || post_url.is_empty()) {
            rejected.push(reject("missing_launcher"));
            continue;
        }

        let env: Option<HashMap<String, String>> = def
            .get("env")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            });
        if let Some(env) = &env
            && let Some(key) = unsafe_env_key(env)
        {
            rejected.push(reject(&format!("unsafe_env: {key}")));
            continue;
        }
        let headers: Option<HashMap<String, String>> = def
            .get("headers")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            });

        accepted.push(crate::config::ExternalMcpConfig {
            name: name.clone(),
            command: if command.is_empty() { None } else { Some(command.to_string()) },
            args: if args.is_empty() { None } else { Some(args) },
            cwd: def.get("cwd").and_then(|v| v.as_str()).map(String::from),
            url: if url.is_empty() { None } else { Some(url.to_string()) },
            sse_url: if sse_url.is_empty() { None } else { Some(sse_url.to_string()) },
            post_url: if post_url.is_empty() { None } else { Some(post_url.to_string()) },
            env,
            headers,
            timeout_ms: def.get("timeout_ms").and_then(|v| v.as_u64()),
            // D-91 criterion 1: imported servers are dormant until an operator
            // activates them with the D-86 one-time code on /api/v1/mcp/external/{name}.
            active: false,
        });
    }

    (accepted, rejected)
}

/// D-86 signature for one import: deterministic over the accepted launchers so
/// the code shown in the console is bound to exactly what will be written.
pub fn import_signature(accepted: &[crate::config::ExternalMcpConfig]) -> String {
    use sha2::{Digest, Sha256};
    // Everything that will be written, not only the launcher: `env`, `cwd` and
    // headers are part of what the operator is approving.
    let mut parts: Vec<String> = accepted
        .iter()
        .map(|e| {
            let mut env: Vec<String> = e
                .env
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect();
            env.sort();
            let mut headers: Vec<String> = e
                .headers
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect();
            headers.sort();
            format!(
                "{}|{}|{:?}|{}|{}|{}|{}|{}|{:?}|{}",
                e.name,
                launcher_signature(e),
                e.args,
                e.cwd.clone().unwrap_or_default(),
                e.url.clone().unwrap_or_default(),
                e.sse_url.clone().unwrap_or_default(),
                e.post_url.clone().unwrap_or_default(),
                env.join(","),
                e.timeout_ms,
                headers.join(","),
            )
        })
        .collect();
    parts.sort();
    format!(
        "import_mcp:{:x}",
        Sha256::digest(parts.join("|").as_bytes())
    )
}

/// POST /api/v1/mcp/import
/// D-91 (ADR-023 P-2): import `mcpServers` from client configs as **dormant**
/// entries. Nothing is spawned, nothing is executed, and every entry needs the
/// operator's one-time approval before it becomes active.
pub async fn import_mcp_servers_handler(
    State(state): State<Arc<HttpState>>,
    Json(req): Json<ImportMcpServersRequest>,
) -> impl IntoResponse {
    let existing_names: Vec<String> = {
        let config = state.config.read().await;
        config.external_mcp.iter().map(|e| e.name.clone()).collect()
    };

    let (accepted, rejected) = plan_mcp_import(&req.config, &existing_names);
    let rejected_json: Vec<serde_json::Value> = rejected
        .iter()
        .map(|(name, reason)| serde_json::json!({"name": name, "reason": reason}))
        .collect();

    if accepted.is_empty() {
        // Nothing would be written: no approval is consumed for an empty import.
        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "imported": 0,
                "servers": [],
                "rejected": rejected_json,
                "active": false,
                "requires_operator_approval": false,
                "source": req.source,
            })),
        )
            .into_response();
    }

    let signature = import_signature(&accepted);
    if let Err(response) = authorize_mcp_change(
        &signature,
        req.approval_nonce.as_deref(),
        req.approval_code.as_deref(),
    ) {
        return *response;
    }

    // Re-check under the config lock: a name may have appeared between the plan
    // and the approval round-trip.
    let mut written: Vec<String> = Vec::new();
    let mut late_rejections: Vec<serde_json::Value> = Vec::new();
    {
        let mut config = state.config.write().await;
        for entry in &accepted {
            if config.external_mcp.iter().any(|e| e.name == entry.name) {
                late_rejections.push(serde_json::json!({
                    "name": entry.name,
                    "reason": "already_registered",
                }));
                continue;
            }
            config.external_mcp.push(entry.clone());
            written.push(entry.name.clone());
            audit_mcp_change("import", &entry.name, None, entry);
        }
    }
    let _ = persist_external_mcp_config(&state.config).await;

    let servers: Vec<serde_json::Value> = accepted
        .iter()
        .filter(|e| written.contains(&e.name))
        .map(|e| {
            serde_json::json!({
                "name": e.name,
                "launcher": launcher_signature(e),
                "active": false,
            })
        })
        .collect();

    let mut all_rejected = rejected_json;
    all_rejected.extend(late_rejections);

    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "imported": written.len(),
            "servers": servers,
            "rejected": all_rejected,
            "active": false,
            "requires_operator_approval": true,
            "approval_signature": signature,
            "source": req.source,
            "note": "imported servers are dormant: activate one with POST/PUT /api/v1/mcp/external/{name} and the D-86 one-time code",
        })),
    )
        .into_response()
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
                // Skip self-references: command, its arguments or a URL pointing
                // back at this kernel (same rule the importer uses).
                let cmd = def.get("command").and_then(|v| v.as_str()).unwrap_or("");
                let url = def.get("url").and_then(|v| v.as_str()).unwrap_or("");
                let args_for_self: Vec<String> = def
                    .get("args")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                if is_self_reference(name, cmd, &args_for_self, &[url]) {
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

    /// D-90: the read-only view reports both active and quarantined tools with
    /// their fingerprint and reason, and whether the baseline is persisted.
    #[tokio::test]
    async fn d90_tool_integrity_view_lists_active_and_quarantined() {
        use crate::registry::guild_process::{GuildLauncher, GuildProcess};
        use std::sync::Arc;

        let mut guild = GuildProcess::new(
            "ext",
            GuildLauncher::External {
                command: "node".into(),
                args: vec![],
                cwd: None,
                env: None,
            },
            false,
            None,
            3,
        );
        let mk = |name: &str, desc: &str| {
            rmcp::model::Tool::new(
                name.to_string(),
                desc.to_string(),
                Arc::new(serde_json::json!({"type": "object"}).as_object().unwrap().clone()),
            )
        };
        let seeded = guild
            .tool_integrity
            .evaluate(&[mk("read", "reads a file")])
            .await;
        guild.tools = seeded.active;
        let quarantined = guild
            .tool_integrity
            .evaluate(&[mk("read", "reads a file (v2)")])
            .await;
        assert_eq!(quarantined.quarantined, vec!["read"]);

        let view = tool_integrity_view(&guild);
        assert_eq!(view["guild"], "ext");
        assert_eq!(view["persistence"], "memory");

        // The stale active copy is replaced by the quarantined entry.
        guild.tools.clear();
        let view = tool_integrity_view(&guild);
        let tools = view["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "read");
        assert_eq!(tools[0]["status"], "quarantined");
        assert_eq!(
            tools[0]["reason"],
            "fingerprint_changed: description or inputSchema changed"
        );
        assert!(tools[0]["fingerprint"].as_str().unwrap().len() == 64);
    }

    /// D-90: with a store attached the view says the baseline is persisted.
    #[tokio::test]
    async fn d90_tool_integrity_view_reports_persistence() {
        use crate::registry::guild_process::{GuildLauncher, GuildProcess};
        use std::sync::Arc;

        let mut guild = GuildProcess::new(
            "ext",
            GuildLauncher::External {
                command: "node".into(),
                args: vec![],
                cwd: None,
                env: None,
            },
            false,
            None,
            3,
        );
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        guild.attach_integrity_store(Arc::new(tokio::sync::Mutex::new(conn)));
        let tool = rmcp::model::Tool::new(
            "read".to_string(),
            "reads a file".to_string(),
            Arc::new(serde_json::json!({"type": "object"}).as_object().unwrap().clone()),
        );
        guild.tools = guild.tool_integrity.evaluate(&[tool]).await.active;

        let view = tool_integrity_view(&guild);
        assert_eq!(view["persistence"], "silva");
        assert_eq!(view["tools"][0]["status"], "active");
    }
    fn import_sample() -> serde_json::Value {
        serde_json::json!({
            "mcpServers": {
                "github": {
                    "command": "npx",
                    "args": ["-y", "@modelcontextprotocol/server-github"],
                    "env": {"GITHUB_TOKEN": "x"}
                },
                "slack": {
                    "url": "https://mcp.slack.example/mcp"
                },
                "files": {
                    "command": "npx",
                    "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
                }
            }
        })
    }

    /// D-91 criterion 1 + 4: three servers imported, all dormant (active=false),
    /// none executed or spawned by the import itself.
    #[test]
    fn d91_import_three_servers_enters_inactive() {
        let (accepted, rejected) = plan_mcp_import(&import_sample(), &[]);

        assert_eq!(accepted.len(), 3, "rejected: {rejected:?}");
        assert!(rejected.is_empty());
        let mut names: Vec<&str> = accepted.iter().map(|e| e.name.as_str()).collect();
        names.sort();
        assert_eq!(names, vec!["files", "github", "slack"]);
        for entry in &accepted {
            assert!(!entry.active, "{} must be dormant", entry.name);
        }
        let github = accepted.iter().find(|e| e.name == "github").unwrap();
        assert_eq!(github.command.as_deref(), Some("npx"));
        assert_eq!(
            github.env.as_ref().unwrap().get("GITHUB_TOKEN").map(String::as_str),
            Some("x")
        );
    }

    /// F8: a config pointing back at this kernel is never imported.
    #[test]
    fn d91_self_reference_is_rejected() {
        let config = serde_json::json!({
            "mcpServers": {
                "loop": {"command": "npx", "args": ["tylluan-nexus"]},
                "direct": {"url": "http://127.0.0.1:47004/mcp"},
                "legacy": {"url": "http://localhost:3030/sse"},
                "tylluannexus-o3": {"command": "node", "args": ["server.js"]}
            }
        });
        let (accepted, rejected) = plan_mcp_import(&config, &[]);

        assert!(accepted.is_empty(), "self references must not be imported");
        let names: Vec<&str> = rejected.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names.len(), 4);
        assert!(rejected.iter().all(|(_, reason)| reason == "self_reference"));
    }

    /// D-87/D-88 + criterion 4: shell metacharacters never make it into config,
    /// on any platform.
    #[test]
    fn d91_command_with_ampersand_is_rejected() {
        let config = serde_json::json!({
            "mcpServers": {
                "evil": {"command": "npx & calc.exe", "args": []},
                "evil_args": {"command": "npx", "args": ["a;rm -rf /"]},
                "ok": {"command": "npx", "args": ["-y", "pkg"]}
            }
        });
        let (accepted, rejected) = plan_mcp_import(&config, &[]);

        assert_eq!(accepted.len(), 1);
        assert_eq!(accepted[0].name, "ok");
        assert!(!accepted[0].active);
        let reasons: Vec<String> = rejected.iter().map(|(_, r)| r.clone()).collect();
        assert_eq!(reasons.len(), 2);
        assert!(reasons.iter().all(|r| r.starts_with("unsafe_command")));
    }

    /// Names, duplicates and empty definitions are rejected instead of written.
    #[test]
    fn d91_bad_names_duplicates_and_missing_launcher_are_rejected() {
        let config = serde_json::json!({
            "mcpServers": {
                "already": {"command": "npx"},
                "bad name": {"command": "npx"},
                "nothing": {"args": ["x"]},
                "sse_half": {"sse_url": "https://x/sse"}
            }
        });
        let (accepted, rejected) = plan_mcp_import(&config, &["already".to_string()]);

        assert!(accepted.is_empty());
        let pairs: Vec<(String, String)> = rejected.clone();
        assert!(pairs.contains(&("already".into(), "already_registered".into())));
        assert!(pairs.contains(&("bad name".into(), "invalid_name".into())));
        assert!(pairs.contains(&("nothing".into(), "missing_launcher".into())));
        assert!(pairs.contains(&("sse_half".into(), "missing_launcher".into())));
    }

    #[test]
    fn d91_missing_mcp_servers_object_is_reported() {
        let (accepted, rejected) = plan_mcp_import(&serde_json::json!({"other": 1}), &[]);
        assert!(accepted.is_empty());
        assert_eq!(rejected.len(), 1);
        assert_eq!(rejected[0].1, "missing mcpServers object");
    }

    fn env_of(pairs: &[(&str, &str)]) -> Option<HashMap<String, String>> {
        Some(
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
        )
    }

    fn add_req(env: Option<HashMap<String, String>>) -> AddMcpServerRequest {
        AddMcpServerRequest {
            name: "manual".into(),
            approval_nonce: None,
            approval_code: None,
            url: None,
            sse_url: None,
            post_url: None,
            command: Some("npx".into()),
            args: Some(vec!["-y".into(), "pkg".into()]),
            cwd: None,
            env,
            headers: None,
            timeout_ms: None,
        }
    }

    fn update_req(env: Option<HashMap<String, String>>) -> UpdateMcpServerRequest {
        UpdateMcpServerRequest {
            approval_nonce: None,
            approval_code: None,
            active: None,
            url: None,
            sse_url: None,
            post_url: None,
            command: None,
            args: None,
            cwd: None,
            env,
            headers: None,
            timeout_ms: None,
        }
    }

    fn manual_entry() -> crate::config::ExternalMcpConfig {
        crate::config::ExternalMcpConfig {
            name: "manual".into(),
            command: Some("npx".into()),
            args: None,
            cwd: None,
            url: None,
            sse_url: None,
            post_url: None,
            env: None,
            headers: None,
            timeout_ms: None,
            active: true,
        }
    }

    /// D-92: the manual add path applies the same UNSAFE_ENV_KEYS rule as the
    /// importer. Without it, `POST /api/v1/mcp/external` was a way around D-91.
    #[test]
    fn d92_manual_add_rejects_unsafe_env() {
        assert_eq!(
            add_entry_from_request(&add_req(env_of(&[("PATH", ".")]))).unwrap_err(),
            "PATH"
        );
        assert_eq!(
            add_entry_from_request(&add_req(env_of(&[("ld_preload", "/tmp/x.so")]))).unwrap_err(),
            "ld_preload"
        );
        assert_eq!(
            add_entry_from_request(&add_req(env_of(&[("TYLLUAN_TOKEN", "x")]))).unwrap_err(),
            "TYLLUAN_TOKEN"
        );

        let ok = add_entry_from_request(&add_req(env_of(&[("GITHUB_TOKEN", "y")])))
            .expect("a normal variable must pass");
        assert_eq!(
            ok.env.as_ref().unwrap().get("GITHUB_TOKEN").map(String::as_str),
            Some("y")
        );
    }

    /// D-92: the update path refuses to write a code-execution variable, and the
    /// entry is left untouched when the update is rejected.
    #[test]
    fn d92_manual_update_rejects_unsafe_env() {
        let mut entry = manual_entry();
        assert_eq!(
            apply_update_fields_checked(&mut entry, &update_req(env_of(&[("LD_PRELOAD", "/x.so")])))
                .unwrap_err(),
            "LD_PRELOAD"
        );
        assert_eq!(
            apply_update_fields_checked(
                &mut entry,
                &update_req(env_of(&[("NODE_OPTIONS", "--require /tmp/x.js")]))
            )
            .unwrap_err(),
            "NODE_OPTIONS"
        );

        apply_update_fields_checked(&mut entry, &update_req(env_of(&[("SLACK_TOKEN", "z")])))
            .expect("a normal variable must pass");
        assert_eq!(
            entry.env.as_ref().unwrap().get("SLACK_TOKEN").map(String::as_str),
            Some("z")
        );
    }

    /// D-91: `env` and `cwd` travel to the child process, so they are part of what
    /// the operator approves. A PATH/LD_PRELOAD injection is rejected outright.
    #[test]
    fn d91_env_injection_is_rejected_and_env_is_bound_by_the_signature() {
        let config = serde_json::json!({
            "mcpServers": {
                "pathhack": {"command": "npx", "args": ["-y", "x"], "env": {"PATH": "."}},
                "preload": {"command": "npx", "args": ["-y", "x"], "env": {"ld_preload": "/tmp/x.so"}},
                "kernel": {"command": "npx", "args": ["-y", "x"], "env": {"TYLLUAN_TOKEN": "x"}},
                "clean": {"command": "npx", "args": ["-y", "x"], "env": {"GITHUB_TOKEN": "y"}}
            }
        });
        let (accepted, rejected) = plan_mcp_import(&config, &[]);

        assert_eq!(accepted.len(), 1, "rejected: {rejected:?}");
        assert_eq!(accepted[0].name, "clean");
        let reasons: Vec<&str> = rejected.iter().map(|(_, r)| r.as_str()).collect();
        assert_eq!(reasons.len(), 3);
        assert!(reasons.iter().all(|r| r.starts_with("unsafe_env: ")));
        assert!(!reasons.contains(&"unsafe_env: "), "the key must be named");

        // The signature must change when only `env` changes: it is part of the
        // approved write, not just the launcher command.
        let mut other = accepted.clone();
        other[0]
            .env
            .get_or_insert_with(std::collections::HashMap::new)
            .insert("EXTRA".into(), "1".into());
        assert_ne!(import_signature(&accepted), import_signature(&other));
    }

    /// D-91: malformed args are rejected instead of being silently dropped.
    #[test]
    fn d91_non_string_args_are_rejected() {
        let config = serde_json::json!({
            "mcpServers": {
                "numbers": {"command": "npx", "args": [1, 2]},
                "not_array": {"command": "npx", "args": "npx"}
            }
        });
        let (accepted, rejected) = plan_mcp_import(&config, &[]);

        assert!(accepted.is_empty());
        assert_eq!(rejected.len(), 2);
        assert!(rejected.iter().all(|(_, r)| r == "invalid_args"));
    }

    /// D-86: the approval signature is bound to exactly what will be written and
    /// does not depend on the JSON key order.
    #[test]
    fn d91_import_signature_is_stable_and_order_independent() {
        let (a, _) = plan_mcp_import(&import_sample(), &[]);
        let (b, _) = plan_mcp_import(&import_sample(), &[]);
        let mut reordered = b.clone();
        reordered.reverse();

        assert_eq!(import_signature(&a), import_signature(&b));
        assert_eq!(import_signature(&a), import_signature(&reordered));
        assert!(import_signature(&a).starts_with("import_mcp:"));

        let mut changed = a.clone();
        changed[0].args = Some(vec!["--version".to_string()]);
        assert_ne!(import_signature(&a), import_signature(&changed));
    }

}
