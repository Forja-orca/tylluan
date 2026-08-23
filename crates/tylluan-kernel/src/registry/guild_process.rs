//! # Guild Process Manager
//!
//! Spawns Python FastMCP guilds as child processes, manages their lifecycle,
//! and provides tool discovery + call forwarding via McpProxy.
//!
//! ## Cross-Platform Python Detection
//!
//! - **Windows**: Tries `python` first, then `python3`
//! - **Linux/macOS/RPi**: Tries `python3` first, then `python`

use crate::registry::proxy::{McpProxy, HttpMcpProxy, SseMcpProxy, ProxyKind, error_result};
use crate::config::{TimeoutsConfig, GuildTimeoutsConfig};
use anyhow::{Result, bail};
use rmcp::model::{CallToolRequestParam, CallToolResult, Tool};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::process::Command;
use tracing::{info, error, warn, debug};
use std::sync::Arc;
use rusqlite::{Connection, params};

/// Possible ways to launch a guild.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GuildLauncher {
    /// Native Python guild (FastMCP)
    Python { module_path: String },
    /// External process (node, npx, binary, etc.) — stdio MCP
    External {
        command: String,
        args: Vec<String>,
        cwd: Option<PathBuf>,
        env: Option<HashMap<String, String>>,
    },
    /// Remote MCP server accessed via HTTP Streamable MCP (POST /messages)
    Http {
        url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
        timeout_ms: Option<u64>,
    },
    /// Remote MCP server accessed via Classic SSE MCP
    /// (GET {sse_url} persistent stream + POST {post_url}?sessionId=XXX)
    Sse {
        sse_url: String,
        post_url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
        timeout_ms: Option<u64>,
    },
}

/// Represents a guild (local or external) with an MCP proxy connection.
pub struct GuildProcess {
    /// Human-readable name (e.g., "bash", "playwright")
    pub name: String,
    /// How this guild is launched
    pub launcher: GuildLauncher,
    /// Active proxy connection (stdio subprocess or HTTP remote)
    proxy: Option<Arc<ProxyKind>>,
    /// Tools discovered from this guild via list_tools
    pub tools: Vec<Tool>,
    /// Last time this guild was accessed (for lifecycle management)
    pub last_access: Instant,
    /// Whether this guild is an always-on core guild
    pub always_on: bool,
    /// Custom timeout for this guild's tools
    pub tool_timeout: Option<std::time::Duration>,
    /// Consecutive crash/failure count for exponential backoff
    pub crash_count: u32,
    /// When the last crash occurred (for backoff window calculation)
    pub last_crash_at: Option<Instant>,
    /// Last recorded latency in ms
    pub last_latency_ms: Option<u64>,
    /// Total successful calls
    pub total_calls: u64,
    /// Timestamps of recent restarts for health monitoring
    pub restarts: Vec<Instant>,
    /// Concurrency limit per guild (max 3 simultaneous calls)
    concurrent_calls: Arc<tokio::sync::Semaphore>,
    /// Gremio ID this guild belongs to (e.g., "builders", "scholars", "wardens")
    pub guild_id: Option<String>,
    /// Agent roles available in this guild
    pub agent_roles: Vec<String>,
    /// Performance counters for collective reputation
    pub successful_calls: u64,
    pub total_latency_ms: u64,
    pub last_call_unix: u64,
    /// Interior mutability for performance counters (updated via &self in call_tool_with_proxy)
    pub perf_total_calls: AtomicU64,
    pub perf_successful_calls: AtomicU64,
    pub perf_total_latency_ms: AtomicU64,
    pub perf_last_call_unix: AtomicU64,
}

/// Determine if a guild is "destructive" based on its declared CAPABILITIES.
/// Destructive guilds can execute system commands or write outside their sandbox.
/// Guilds without CAPABILITIES (null) are conservatively treated as non-destructive.
pub fn is_destructive_guild(caps: &serde_json::Value) -> bool {
    // process_execution:
    //   true          → destructive (can run any command)
    //   false         → not destructive via this axis
    //   absent        → conservative: assume destructive
    //   ["git", ..]   → non-empty allowlist → can execute SOME commands → destructive
    //   []            → empty allowlist → no commands allowed → not destructive
    match caps.get("process_execution") {
        Some(v) => {
            if v.as_bool() == Some(true) {
                return true;
            }
            if v.as_bool() == Some(false) {
                // explicitly denied, not destructive via this axis
            } else if let Some(arr) = v.as_array()
                && !arr.is_empty() {
                    return true; // non-empty allowlist → can execute
                }
        }
        None => return true, // not declared → assume destructive
    }

    // filesystem_scope covering "/" → can write anywhere
    if let Some(scope) = caps.get("filesystem_scope").and_then(|v| v.as_array())
        && scope.iter().any(|v| v.as_str() == Some("/")) {
            return true;
        }

    false
}

/// Extract the first word (binary) from a tool call's "command" argument.
fn extract_requested_binary(params: &CallToolRequestParam) -> Option<String> {
    params.arguments.as_ref()
        .and_then(|a| a.get("command"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .and_then(|cmd| cmd.split_whitespace().next())
        .map(|s| s.to_string())
}

/// Check if a tool name looks like it executes commands.
fn is_exec_like_tool(params: &CallToolRequestParam) -> bool {
    let tn: &str = params.name.as_ref();
    tn.contains("execute") || tn.contains("run")
        || tn.contains("exec") || tn.contains("spawn")
        || tn.contains("shell") || tn.contains("command")
}

/// Deterministic capability enforcement — no config or catalog lookup.
/// Returns Some(block_message) if the call violates declared capabilities.
/// Guilds without capabilities (null) are never enforced.
pub fn enforce_capabilities(
    guild_name: &str,
    caps: &serde_json::Value,
    params: &CallToolRequestParam,
) -> Option<String> {
    // Check process_execution
    match caps.get("process_execution") {
        None => {} // absent → skip enforcement (same as before)
        Some(v) if v.is_array() => {
            let list: Vec<String> = v.as_array().unwrap()
                .iter().filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            if list.is_empty() {
                // Empty allowlist = no commands allowed, equivalent to false
                if is_exec_like_tool(params) {
                    return Some(format!(
                        "CAPABILITY_BLOCKED: guild '{guild_name}' declares process_execution=[] (empty allowlist). \
                         No commands are allowed."
                    ));
                }
            } else {
                // Non-empty allowlist: extract requested command and check
                if let Some(cmd) = extract_requested_binary(params)
                    && !list.iter().any(|allowed| allowed == &cmd) {
                        return Some(format!(
                            "CAPABILITY_BLOCKED: guild '{}' declares process_execution allowlist {:?}, \
                             but command '{}' is not allowed. Allowed commands: {}.",
                            guild_name, list, cmd, list.join(", ")
                        ));
                    }
                // If no command arg but tool looks exec-like, also block
                if is_exec_like_tool(params) && extract_requested_binary(params).is_none() {
                    return Some(format!(
                        "CAPABILITY_BLOCKED: guild '{guild_name}' declares process_execution allowlist {list:?}, \
                         but no command could be extracted to verify against the allowlist."
                    ));
                }
            }
        }
        Some(v) if v.as_bool() == Some(false) => {
            let is_exec_tool = is_exec_like_tool(params);
            let has_command_arg = params.arguments.as_ref()
                .and_then(|a| a.get("command"))
                .and_then(|v| v.as_str())
                .map(|s| !s.is_empty())
                .unwrap_or(false);
            if is_exec_tool || has_command_arg {
                return Some(format!(
                    "CAPABILITY_BLOCKED: guild '{guild_name}' declares process_execution=false. \
                     This guild may not execute system commands."
                ));
            }
        }
        _ => {} // true or unknown type → allow
    }

    // Check filesystem_scope
    if let Some(scope) = caps.get("filesystem_scope").and_then(|v| v.as_array())
        && !scope.is_empty() {
            let scope_paths: Vec<&str> = scope.iter()
                .filter_map(|v| v.as_str())
                .collect();
            for path_key in &["path", "cwd", "directory"] {
                if let Some(arg_val) = params.arguments.as_ref()
                    .and_then(|a| a.get(*path_key))
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                {
                    let allowed = scope_paths.iter().any(|prefix| {
                        arg_val.starts_with(prefix) || prefix == &"/"
                    });
                    if !allowed {
                        return Some(format!(
                            "CAPABILITY_BLOCKED: guild '{guild_name}' declares filesystem_scope={scope_paths:?}, \
                             but argument '{path_key}' references path '{arg_val}' outside that scope."
                        ));
                    }
                }
            }
        }

    None
}

impl GuildProcess {
    /// Create a new guild process descriptor (does not spawn yet).
    pub fn new(name: &str, launcher: GuildLauncher, always_on: bool, tool_timeout_ms: Option<u64>, max_concurrent: usize) -> Self {
        Self {
            name: name.to_string(),
            launcher,
            proxy: None,
            tools: Vec::new(),
            last_access: Instant::now(),
            always_on,
            tool_timeout: tool_timeout_ms.map(std::time::Duration::from_millis),
            crash_count: 0,
            last_crash_at: None,
            last_latency_ms: None,
            total_calls: 0,
            restarts: Vec::new(),
            concurrent_calls: Arc::new(tokio::sync::Semaphore::new(max_concurrent)),
            guild_id: None,
            agent_roles: Vec::new(),
            successful_calls: 0,
            total_latency_ms: 0,
            last_call_unix: 0,
            perf_total_calls: AtomicU64::new(0),
            perf_successful_calls: AtomicU64::new(0),
            perf_total_latency_ms: AtomicU64::new(0),
            perf_last_call_unix: AtomicU64::new(0),
        }
    }

    /// Create a new guild process with guild_id and agent_roles (V2).
    pub fn new_v2(name: &str, launcher: GuildLauncher, always_on: bool, tool_timeout_ms: Option<u64>, guild_id: &str, agent_roles: Vec<String>, max_concurrent: usize) -> Self {
        Self {
            name: name.to_string(),
            launcher,
            proxy: None,
            tools: Vec::new(),
            last_access: Instant::now(),
            always_on,
            tool_timeout: tool_timeout_ms.map(std::time::Duration::from_millis),
            crash_count: 0,
            last_crash_at: None,
            last_latency_ms: None,
            total_calls: 0,
            restarts: Vec::new(),
            concurrent_calls: Arc::new(tokio::sync::Semaphore::new(max_concurrent)),
            guild_id: Some(guild_id.to_string()),
            agent_roles,
            successful_calls: 0,
            total_latency_ms: 0,
            last_call_unix: 0,
            perf_total_calls: AtomicU64::new(0),
            perf_successful_calls: AtomicU64::new(0),
            perf_total_latency_ms: AtomicU64::new(0),
            perf_last_call_unix: AtomicU64::new(0),
        }
    }

    /// Check if the guild process is currently running and connected.
    pub fn is_running(&self) -> bool {
        self.proxy.is_some()
    }

    /// Get a human-readable description of the guild.
    pub fn description(&self) -> String {
        match &self.launcher {
            GuildLauncher::Python { module_path } => format!("Python guild: {module_path}"),
            GuildLauncher::External { command, args, .. } => format!("External: {} {}", command, args.join(" ")),
            GuildLauncher::Http { url, .. } => format!("HTTP MCP: {url}"),
            GuildLauncher::Sse { sse_url, .. } => format!("SSE MCP: {sse_url}"),
        }
    }

    /// Touch the last access timestamp (resets inactivity timer).
    pub fn touch(&mut self) {
        self.last_access = Instant::now();
    }

    /// Spawn the guild. For Http guilds, connects via HTTP; for others, launches subprocess.
    pub async fn spawn(&mut self, guilds_dir: &PathBuf, timeouts: &TimeoutsConfig) -> Result<()> {
        if self.is_running() {
            debug!("Guild '{}' already running, skipping spawn", self.name);
            return Ok(());
        }

        // HTTP Streamable MCP: no subprocess — connect via POST /messages
        let http_info = match &self.launcher {
            GuildLauncher::Http { url, headers, timeout_ms } => {
                Some((url.clone(), headers.clone(), *timeout_ms))
            }
            _ => None,
        };
        if let Some((url, headers, timeout_ms)) = http_info {
            let t = timeout_ms.unwrap_or(30_000);
            let name = self.name.clone();
            let (proxy, tools) = HttpMcpProxy::connect(&name, &url, &headers, t).await?;
            self.tools = tools;
            self.proxy = Some(Arc::new(ProxyKind::Http(proxy)));
            self.last_access = Instant::now();
            return Ok(());
        }

        // Classic SSE MCP: persistent GET stream + POST for requests
        let sse_info = match &self.launcher {
            GuildLauncher::Sse { sse_url, post_url, headers, timeout_ms } => {
                Some((sse_url.clone(), post_url.clone(), headers.clone(), *timeout_ms))
            }
            _ => None,
        };
        if let Some((sse_url, post_url, headers, timeout_ms)) = sse_info {
            let t = timeout_ms.unwrap_or(30_000);
            let name = self.name.clone();
            let (proxy, tools) = SseMcpProxy::connect(&name, &sse_url, &post_url, &headers, t).await?;
            self.tools = tools;
            self.proxy = Some(Arc::new(ProxyKind::Sse(proxy)));
            self.last_access = Instant::now();
            return Ok(());
        }

        // Stdio guild: build Command and spawn subprocess
        // Find the actual repo root where guilds/ lives.
        // When the kernel runs from crates/tylluan-kernel/, guilds_dir may point
        // there (tylluan.toml found first), but guilds/ is at the repo root.
        let workspace_root = {
            let mut root = guilds_dir.canonicalize()
                .unwrap_or_else(|_| guilds_dir.clone());
            for _ in 0..4 {
                if root.join("guilds").exists() { break; }
                match root.parent() {
                    Some(p) => root = p.to_path_buf(),
                    None => { root = guilds_dir.clone(); break; }
                }
            }
            root
        };

        let command = match &self.launcher {
GuildLauncher::Python { module_path } => {
                let python = find_python().await.map_err(|e| {
                    error!("❌ find_python() failed: {}. Check .venv exists!", e);
                    e
                })?;
                
                // S1: Docker sandbox — profile-based (guild-level only, no session)
                // Strict: all guilds, Balanced: bash/code only, Permissive: none
                // Uses resolve_docker_profile which excludes session overrides intentionally.
                let (profile, _origin) = crate::config::resolve_docker_profile(&self.name).await;
                let should_docker = profile.is_strict()
                    || (profile.is_balanced() && (self.name == "bash" || self.name == "code"));
                if should_docker && let Some(sb) = crate::config::load_sandbox_config()
                {
                    info!("🐳 Sandbox: guild '{}' running in Docker container '{}' (profile={:?})",
                        self.name, sb.image, profile);
                    // Strip Windows UNC prefix (\\?\) that canonicalize() adds — Docker doesn't understand it
                    let ws_path = workspace_root.display().to_string();
                    let ws_clean = ws_path.strip_prefix(r"\\?\").unwrap_or(&ws_path);
                    let volume_bind = format!("{ws_clean}:/workspace:ro");
                    let mut docker_cmd = Command::new("docker");
                    docker_cmd.args([
                        "run", "--rm",
                        "--network", if sb.network { "bridge" } else { "none" },
                        "--memory", sb.memory.as_str(),
                        "--pids-limit", "100",
                        "-v", volume_bind.as_str(),
                        "-w", "/workspace",
                        "-e", "PYTHONPATH=/workspace",
                        "-e", "PYTHONUNBUFFERED=1",
                        sb.image.as_str(),
                        "python", "-m", module_path.as_str(),
                    ]);
                    docker_cmd
                } else {
                    let mut cmd = Command::new(&python);
                    info!("🛠️ Pre-Spawn: Preparing guild '{}' with python: '{}' -m {} (workspace: {})", self.name, python, module_path, workspace_root.display());
                    cmd.arg("-m")
                       .arg(module_path)
                       .current_dir(&workspace_root);
                    cmd.env("PYTHONPATH", workspace_root.to_string_lossy().as_ref());
                    cmd.env("PYTHONUNBUFFERED", "1");
                    cmd
                }
            }
            GuildLauncher::External { command, args, cwd, env } => {
                info!("🛠️ Pre-Spawn: Preparing external guild '{}' using command: '{}'", self.name, command);
                // On Windows, batch scripts (npx.cmd, npm.cmd, yarn.cmd, etc.) cannot be
                // spawned directly by CreateProcess — they require cmd.exe as the host.
                let mut cmd = if cfg!(target_os = "windows") {
                    let mut c = Command::new("cmd");
                    c.arg("/c").arg(command).args(args);
                    c
                } else {
                    let mut c = Command::new(command);
                    c.args(args);
                    c
                };
                if let Some(c) = cwd {
                    cmd.current_dir(c);
                } else {
                    cmd.current_dir(guilds_dir);
                }
                if let Some(e) = env {
                    cmd.envs(e);
                }
                cmd
            }
            GuildLauncher::Http { .. } => unreachable!("Http handled above"),
            GuildLauncher::Sse { .. } => unreachable!("Sse handled above"),
        };

        let proxy = McpProxy::spawn(
            &self.name,
            command,
            timeouts,
        ).await?;

        // Discover tools from the guild
        self.tools = proxy.list_tools().await?;
        info!(
            "📦 Guild '{}' ready: {} tools registered",
            self.name, self.tools.len()
        );

        self.proxy = Some(Arc::new(ProxyKind::Stdio(proxy)));
        self.last_access = Instant::now();
        self.restarts.push(Instant::now());
        Ok(())
    }

    /// Forward a tool call to this guild via McpProxy.
    /// Returns a valid CallToolResult in ALL error cases — never propagates raw errors.
    pub async fn call_tool(&mut self, params: CallToolRequestParam) -> CallToolResult {
        self.touch();
        self.call_tool_with_proxy(params).await
    }
    
    /// Call tool without requiring mutable self (for use from read locks)
    /// Returns a valid CallToolResult in ALL error cases — never propagates raw errors.
    pub async fn call_tool_readonly(&self, params: CallToolRequestParam) -> CallToolResult {
        self.call_tool_with_proxy(params).await
    }

    /// Get proxy for external calls (releases lock before tool call)
    pub fn get_proxy(&self) -> Option<Arc<ProxyKind>> {
        self.proxy.clone()
    }

    /// Get concurrent calls semaphore to manage execution limit lock-free
    pub fn get_semaphore(&self) -> Arc<tokio::sync::Semaphore> {
        self.concurrent_calls.clone()
    }

    /// Check if a guild's capability declarations allow this tool call.
    /// Returns Some(error_message) if the call should be blocked, None if allowed.
    /// Resolves the effective SandboxProfile via the hierarchical cascade:
    ///   session (agent_id) > guild > global
    async fn check_capabilities(&self, params: &CallToolRequestParam) -> Option<String> {
        let agent_id = params.arguments.as_ref()
            .and_then(|a| a.get("agent_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("anonymous");
        let (profile, _origin) = crate::config::resolve_effective_profile(&self.name, agent_id).await;

        // Permissive: no enforcement
        if profile.is_permissive() {
            return None;
        }

        let catalog = crate::router::catalog::builtin_catalog();
        let caps = catalog.iter()
            .find(|d| d.name == self.name)
            .and_then(|d| d.capabilities.as_ref());

        let caps = match caps {
            Some(c) => c,
            None => return None, // no capabilities → allowed
        };

        // Strict: override caps to process_execution=false regardless of declaration
        if profile.is_strict() {
            // Build an overridden JSON that forces process_execution=false
            let mut overridden = serde_json::Map::new();
            if let Some(obj) = caps.as_object() {
                for (k, v) in obj.iter() {
                    if k == "process_execution" {
                        overridden.insert(k.clone(), serde_json::Value::Bool(false));
                    } else {
                        overridden.insert(k.clone(), v.clone());
                    }
                }
            }
            let overridden = serde_json::Value::Object(overridden);
            return enforce_capabilities(&self.name, &overridden, params);
        }

        // Balanced: enforce per declared capabilities
        let enforce = crate::config::TylluanConfig::load_cached()
            .ok()
            .and_then(|cfg| {
                let locked = cfg.try_read().ok()?;
                Some(locked.security_capabilities_enforce_enabled())
            })
            .unwrap_or(false);

        if !enforce {
            return None;
        }

        enforce_capabilities(&self.name, caps, params)
    }

    /// When `check_capabilities` blocks a call, offer a grant escalation.
    /// Returns `None` to proceed with the tool call, or `Some(result)` to abort.
    async fn handle_capabilities_grant(
        &self,
        params: &CallToolRequestParam,
        blocked_msg: &str,
    ) -> Option<CallToolResult> {
        let agent_id = params.arguments.as_ref()
            .and_then(|a| a.get("agent_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("anonymous");
        let tool_name = params.name.to_string();

        let (tx, rx) = tokio::sync::oneshot::channel();
        let id = crate::security::grants::register(
            crate::security::grants::GrantRequest {
                guild: self.name.clone(),
                tool_name: tool_name.clone(),
                agent_id: agent_id.to_string(),
                reason: blocked_msg.to_string(),
                arguments: params.arguments.clone().unwrap_or_default(),
                tx,
                expires_at: tokio::time::Instant::now()
                    + tokio::time::Duration::from_secs(300),
            },
        ).await;

        warn!(
            "🕐 [GRANT] Guild '{}' tool '{}' blocked — awaiting approval (id={})",
            self.name, tool_name, id
        );

        let decision = match tokio::time::timeout(
            tokio::time::Duration::from_secs(300),
            rx,
        ).await {
            Ok(Ok(level)) => level,
            Ok(Err(_)) | Err(_) => {
                warn!("⏱️ [GRANT] '{}' expired or cancelled", id);
                return Some(error_result(blocked_msg));
            }
        };

        match decision {
            crate::security::grants::GrantLevel::ThisTime => {
                info!("✅ [GRANT] '{}' approved this_time", id);
                None
            }
            crate::security::grants::GrantLevel::ThisSession => {
                info!("✅ [GRANT] '{}' approved this_session", id);
                crate::config::set_session_override(
                    agent_id,
                    crate::config::SandboxProfile::Permissive,
                ).await;
                if self.check_capabilities(params).await.is_some() {
                    error!("[GRANT] session override did not resolve block — this is a bug");
                    Some(error_result(blocked_msg))
                } else {
                    None
                }
            }
            crate::security::grants::GrantLevel::AlwaysForGuild => {
                info!("✅ [GRANT] '{}' approved always_for_guild", id);
                if let Err(e) = crate::config::persist_guild_override(&self.name).await {
                    error!("❌ [GRANT] Failed to persist guild override: {}", e);
                    return Some(error_result(blocked_msg));
                }
                if self.check_capabilities(params).await.is_some() {
                    error!("[GRANT] guild override did not resolve block — this is a bug");
                    Some(error_result(blocked_msg))
                } else {
                    None
                }
            }
        }
    }

    /// Internal call with proxy - for use when lock is already held
    /// Returns a valid CallToolResult in ALL cases — never propagates errors.
    pub async fn call_tool_with_proxy(&self, params: CallToolRequestParam) -> CallToolResult {
        // Capability enforcement check with grant loop
        // If blocked, the grant engine (M30-P3) offers three escalation paths:
        //   this_time: run once without persisting
        //   this_session: set session override to permissive (in-memory)
        //   always_for_guild: persist guild override to permissive (TOML)
        if let Some(msg) = self.check_capabilities(&params).await
            && let Some(result) = self.handle_capabilities_grant(&params, &msg).await {
                return result;
            }

        // Dry-run intercept: if config says dry_run=true and guild is destructive,
        // simulate the call without forwarding to the proxy.
        // Destructive classification respects the full hierarchical cascade:
        //   session (agent_id) > guild > global
        if let Ok(cfg) = crate::config::TylluanConfig::load_cached()
            && let Ok(locked) = cfg.try_read()
                && locked.guilds_dry_run() {
                    let agent_id = params.arguments.as_ref()
                        .and_then(|a| a.get("agent_id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("anonymous");
                    let (profile, _origin) = crate::config::resolve_effective_profile(&self.name, agent_id).await;
                    let is_destructive = match profile {
                        crate::config::SandboxProfile::Permissive => false,
                        crate::config::SandboxProfile::Strict => true,
                        _ => {
                            let catalog = crate::router::catalog::builtin_catalog();
                            catalog.iter()
                                .find(|d| d.name == self.name)
                                .and_then(|d| d.capabilities.as_ref())
                                .map(is_destructive_guild)
                                .unwrap_or(false)
                        }
                    };

                    if is_destructive {
                        let tool_name = params.name.as_ref();
                        let msg = format!(
                            "[DRY-RUN] Guild '{}' tool '{}' — execution simulated. \
                             Set dry_run=false in [guilds] to run for real.",
                            self.name, tool_name
                        );
                        info!("{}", msg);
                        return CallToolResult {
                            content: vec![rmcp::model::Content::text(msg)],
                            is_error: Some(false),
                        };
                    }
                }

        let permit = self.concurrent_calls.acquire()
            .await
            .map_err(|_| anyhow::anyhow!("Guild '{}' semaphore closed", self.name));
        
        if let Err(e) = &permit {
            return error_result(&format!("Guild '{}' semaphore error: {}", self.name, e));
        }

        match &self.proxy {
            Some(proxy) => {
                let tool_name = params.name.to_string();
                let call_start = std::time::Instant::now();
                // Timeout policy:
                //   Some(t) → network/fast guild: fixed deadline, then error.
                //   None    → CPU inference guild (vision, ML): no deadline, wait forever.
                //             Killing and restarting in-progress ONNX inference wastes all
                //             prior computation. Patience is the correct strategy on CPU.
                let result: Result<Result<CallToolResult, anyhow::Error>, ()> = if let Some(t) = self.tool_timeout {
                    let call_fut = proxy.call_tool(params);
                    tokio::time::timeout(t, call_fut).await.map_err(|_| ())
                } else {
                    // No timeout — CPU inference: run until complete, however long it takes.
                    tracing::info!(
                        "⚡ Guild '{}' tool '{}' — CPU inference mode, no timeout",
                        self.name, tool_name
                    );
                    match proxy.call_tool(params).await {
                        Ok(r)  => Ok(Ok(r)),
                        Err(e) => Ok(Err(e)),
                    }
                };

                let latency = call_start.elapsed().as_millis() as u64;
                self.perf_total_calls.fetch_add(1, Ordering::Relaxed);
                self.perf_total_latency_ms.fetch_add(latency, Ordering::Relaxed);
                self.perf_last_call_unix.store(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                    Ordering::Relaxed,
                );

                match result {
                    Ok(Ok(r)) => {
                        self.perf_successful_calls.fetch_add(1, Ordering::Relaxed);
                        r
                    }
                    Ok(Err(e)) => {
                        let err_msg = e.to_string();
                        if err_msg.contains("timeout") || err_msg.contains("timed out") {
                            let timeout_ms = self.tool_timeout
                                .map(|d| d.as_millis())
                                .unwrap_or(120_000);
                            let timeout_secs = timeout_ms / 1000;
                            tracing::warn!(
                                "Guild '{}' tool '{}' timed out after {}s",
                                self.name, tool_name, timeout_secs
                            );
                            error_result(&format!(
                                "GUILD_TIMEOUT|{}|{}s", self.name, timeout_secs
                            ))
                        } else {
                            tracing::error!(
                                "Guild '{}' tool '{}' failed: {}",
                                self.name, tool_name, err_msg
                            );
                            error_result(&format!(
                                "GUILD_ERROR|{}|{}", self.name, err_msg
                            ))
                        }
                    }
                    Err(_) => {
                        let timeout_ms = self.tool_timeout
                            .map(|d| d.as_millis())
                            .unwrap_or(120_000);
                        let timeout_secs = timeout_ms / 1000;
                        tracing::warn!(
                            "Guild '{}' tool '{}' deadline exceeded after {}s",
                            self.name, tool_name, timeout_secs
                        );
                        error_result(&format!(
                            "GUILD_TIMEOUT|{}|{}s", self.name, timeout_secs
                        ))
                    }
                }
            },
            None => error_result(&format!(
                "Guild '{}' is not running. Call request_guild first.",
                self.name
            )),
        }
    }

    pub async fn kill(&mut self) -> Result<()> {
        if let Some(proxy) = self.proxy.take() {
            info!("🛑 Killing guild '{}'", self.name);
            match Arc::try_unwrap(proxy) {
                Ok(p) => {
                    p.shutdown().await.ok();
                }
                Err(_) => {
                    crate::registry::proxy::McpProxy::kill_abandoned_child(&self.name);
                }
            }
            self.tools.clear();
        }
        Ok(())
    }

    /// Elapsed time since last access.
    pub fn idle_seconds(&self) -> u64 {
        self.last_access.elapsed().as_secs()
    }
}

/// Registry that manages all guild processes and provides tool routing.
/// Timeout tier a guild belongs to, per `[guilds.timeouts]`'s own doc
/// comments (config.rs) -- see `GuildRegistry::guild_timeout_category()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GuildTimeoutCategory {
    System,
    Analysis,
    Heavy,
}

pub struct GuildRegistry {
    /// All known guilds, keyed by name
    pub guilds: HashMap<String, GuildProcess>,
    /// Mapping: tool_name → guild_name (for routing tool calls)
    pub tool_to_guild: HashMap<String, String>,
    /// Path to the guilds directory (for subprocess cwd)
    pub guilds_dir: PathBuf,
    /// Inactivity timeout in seconds (from config)
    pub timeout_secs: u64,
    /// Handsake and call timeouts
    pub timeouts: TimeoutsConfig,
    /// Category-specific guild call timeouts ([guilds.timeouts] in tylluan.toml).
    /// `None` (the default for every existing `GuildRegistry::new()` call
    /// site, including all test helpers) preserves the pre-existing flat
    /// `timeouts.tool_call_secs` behavior exactly. Only `Some(..)` -- set via
    /// `with_guild_timeouts()`, which main.rs calls with the real config --
    /// overrides it per guild category. This distinction matters: this
    /// config's own defaults (15s/60s/180s) are far shorter than
    /// `tool_call_secs`'s default (3600s) -- silently applying them to every
    /// caller, including tests that spawn real guild subprocesses, would
    /// have been a real regression, not a neutral default.
    pub guild_timeouts: Option<GuildTimeoutsConfig>,
    /// Max simultaneous calls per guild (configurable)
    pub max_concurrent: usize,
    /// Optional metrics database connection for persisting guild metrics
    db_conn: Option<Arc<tokio::sync::Mutex<Connection>>>,
}

impl GuildRegistry {
    /// Create a new registry with the given guilds directory and timeout.
    pub fn new(guilds_dir: PathBuf, timeout_secs: u64, timeouts: TimeoutsConfig, max_concurrent: usize) -> Self {
        Self {
            guilds: HashMap::new(),
            tool_to_guild: HashMap::new(),
            guilds_dir,
            timeout_secs,
            timeouts,
            guild_timeouts: None,
            max_concurrent,
            db_conn: None,
        }
    }

    /// Wire real per-category guild timeouts (system/analysis/heavy) into an
    /// already-constructed registry. Builder-style, matching this codebase's
    /// existing `.with_curriculum()`/`.with_hormones()` pattern -- opt-in, so
    /// every pre-existing `GuildRegistry::new()` call site (7+ test helpers)
    /// keeps compiling unchanged.
    ///
    /// Found 2026-08-23 (config-muerta gate G1 triage): [guilds.timeouts]
    /// (system_guild_ms/analysis_guild_ms/heavy_guild_ms) was declared,
    /// defaulted, and documented, but never actually read anywhere -- every
    /// guild shared one flat `timeouts.tool_call_secs` ceiling regardless of
    /// category. Production (`tylluan.toml`) set that flat value generously
    /// (3600s, safe for heavy guilds like deep_analysis, but means a hung
    /// `bash` call would wait up to an hour before erroring). The example
    /// config new users copy (`tylluan.example.toml`) set it to 120s --
    /// too short for genuinely heavy guilds. Differentiating by category
    /// fixes both: light guilds fail fast, heavy guilds keep real headroom.
    pub fn with_guild_timeouts(mut self, guild_timeouts: GuildTimeoutsConfig) -> Self {
        self.guild_timeouts = Some(guild_timeouts);
        self
    }

    /// Category for a guild name, per the categories [guilds.timeouts]'s own
    /// doc comments already defined (config.rs) but never wired to anything.
    /// Unknown guilds fall back to "analysis" (the middle tier) rather than
    /// either extreme -- a new guild nobody has categorized yet should get a
    /// reasonable default, not the shortest or the longest ceiling by luck.
    fn guild_timeout_category(name: &str) -> GuildTimeoutCategory {
        const SYSTEM: &[&str] = &["bash", "git", "filesystem", "monitor"];
        const HEAVY: &[&str] = &[
            "docker", "database", "pdf", "vision",
            "deep_analysis", "knowledge", "comfy_ui", "n8n_bridge",
            "deep_web_research", "audio_tools", "ffmpeg_tools",
        ];
        if SYSTEM.contains(&name) {
            GuildTimeoutCategory::System
        } else if HEAVY.contains(&name) {
            GuildTimeoutCategory::Heavy
        } else {
            GuildTimeoutCategory::Analysis
        }
    }

    /// Guilds main.rs already treats as needing genuinely UNLIMITED patience
    /// (CPU-bound ONNX/local-LLM inference, killing which mid-run wastes all
    /// prior computation -- see main.rs's own `cpu_inference_guilds` list and
    /// its comment). Real bug caught before shipping (2026-08-23, reviewing
    /// this same commit for doc drift): the Heavy category's 180s
    /// (heavy_guild_ms default) would have UNDERCUT that existing design --
    /// deep_analysis alone is documented elsewhere in this project as taking
    /// 10+ minutes on CPU, and main.rs's `tool_timeout = None` override only
    /// skips the OUTER guild-level wrapper (guild_process.rs's call_tool) --
    /// it does nothing about the INNER proxy-level timeout this function
    /// feeds (proxy.rs's own `tokio::time::timeout`), which would have fired
    /// regardless and silently cut these guilds down from their previous,
    /// safe 3600s (the old flat tool_call_secs) to 180s. This list must stay
    /// in sync with main.rs's cpu_inference_guilds by hand -- there is no
    /// single source of truth for it yet (a real, separate, smaller finding
    /// than the one this function exists to fix).
    const UNLIMITED_PATIENCE_GUILDS: &'static [&'static str] =
        &["vision", "deep_analysis", "knowledge", "comfy_ui", "n8n_bridge"];

    /// Effective tool-call timeout (seconds) for a specific guild: the
    /// category-specific value from `guild_timeouts` if it's been wired via
    /// `with_guild_timeouts()`, otherwise falls back to the flat
    /// `timeouts.tool_call_secs` (this registry's pre-existing behavior,
    /// preserved exactly for anyone who hasn't opted in yet). Guilds in
    /// `UNLIMITED_PATIENCE_GUILDS` always get the flat value regardless of
    /// category, matching their pre-existing "wait as long as it takes"
    /// contract with main.rs.
    fn effective_tool_call_secs(&self, guild_name: &str) -> u64 {
        let Some(gt) = &self.guild_timeouts else {
            return self.timeouts.tool_call_secs;
        };
        if Self::UNLIMITED_PATIENCE_GUILDS.contains(&guild_name) {
            return self.timeouts.tool_call_secs;
        }
        match Self::guild_timeout_category(guild_name) {
            GuildTimeoutCategory::System => gt.system_guild_ms / 1000,
            GuildTimeoutCategory::Analysis => gt.analysis_guild_ms / 1000,
            GuildTimeoutCategory::Heavy => gt.heavy_guild_ms / 1000,
        }
    }

    /// `self.timeouts`, but with `tool_call_secs` swapped for this specific
    /// guild's category-appropriate value (or left untouched if
    /// `with_guild_timeouts()` was never called -- see `guild_timeouts`'s
    /// doc comment). `Guild::spawn()` is an instance method with no access
    /// to the registry's state, so the per-guild adjustment has to happen
    /// here, at the two call sites that already hold both `self` and the
    /// guild being spawned, rather than inside `Guild::spawn()` itself.
    fn timeouts_for(&self, guild_name: &str) -> TimeoutsConfig {
        TimeoutsConfig {
            handshake_secs: self.timeouts.handshake_secs,
            tool_call_secs: self.effective_tool_call_secs(guild_name),
        }
    }

    /// Initialize metrics database for persistence.
    pub fn init_metrics_db(&mut self, db_path: &str) -> Result<()> {
        let conn = crate::config::open_db(std::path::Path::new(db_path))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS guild_metrics (
                name TEXT PRIMARY KEY,
                total_calls INTEGER NOT NULL DEFAULT 0,
                error_count INTEGER NOT NULL DEFAULT 0,
                last_updated INTEGER NOT NULL DEFAULT (unixepoch())
            )"
        )?;
        self.db_conn = Some(Arc::new(tokio::sync::Mutex::new(conn)));
        Ok(())
    }

    /// Persist current guild metrics to the database.
    pub async fn persist_metrics(&self) -> Result<()> {
        if let Some(ref db_arc) = self.db_conn {
            let conn = db_arc.lock().await;
            for (name, guild) in &self.guilds {
                // error_count = total_calls - successful_calls
                let error_count = guild.total_calls.saturating_sub(guild.successful_calls);
                conn.execute(
                    "INSERT INTO guild_metrics (name, total_calls, error_count, last_updated)
                     VALUES (?1, ?2, ?3, unixepoch())
                     ON CONFLICT(name) DO UPDATE SET
                       total_calls=excluded.total_calls,
                       error_count=excluded.error_count,
                       last_updated=excluded.last_updated",
                    params![name, guild.total_calls as i64, error_count as i64],
                )?;
            }
        }
        Ok(())
    }

    /// Load persisted guild metrics from the database.
    pub async fn load_metrics(&mut self) -> Result<()> {
        if let Some(ref db_arc) = self.db_conn {
            let conn = db_arc.lock().await;
            let mut stmt = conn.prepare(
                "SELECT name, total_calls, error_count FROM guild_metrics"
            )?;
            let rows: Vec<(String, i64, i64)> = stmt.query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?.filter_map(|r| r.ok()).collect();

            for (name, calls, errors) in rows {
                if let Some(guild) = self.guilds.get_mut(&name) {
                    guild.total_calls = calls as u64;
                    // Calculate successful_calls from total_calls - error_count
                    guild.successful_calls = (calls as u64).saturating_sub(errors as u64);
                }
            }
        }
        Ok(())
    }

    /// Discovers unregistered guilds in the guilds directory (T24).
    pub fn discover_guilds(&mut self) -> Result<Vec<String>> {
        let mut discovered = Vec::new();
        if !self.guilds_dir.exists() {
            return Ok(discovered);
        }

        let entries = std::fs::read_dir(&self.guilds_dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_file()
                && let Some(ext) = path.extension()
                    && ext == "py" {
                        let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                        // Ignore standard python files
                        if !self.guilds.contains_key(&name) && name != "__init__" {
                            info!("🔍 [T24] Discovered unregistered Python guild: {}", name);
                            // Auto-register as python module
                            self.register(&name, &format!("guilds.{name}"), false, None);
                            discovered.push(name);
                        }
                    }
        }
        Ok(discovered)
    }

    /// Register a Python guild descriptor (does not spawn it).
    pub fn register(&mut self, name: &str, module_path: &str, always_on: bool, timeout_ms: Option<u64>) {
        self.guilds.insert(
            name.to_string(),
            GuildProcess::new(
                name,
                GuildLauncher::Python { module_path: module_path.to_string() },
                always_on,
                timeout_ms,
                self.max_concurrent,
            ),
        );
        let _ = self.save();
    }

    /// Register a V2 Python guild with guild_id and agent_roles.
    pub fn register_v2(&mut self, name: &str, module_path: &str, always_on: bool, timeout_ms: Option<u64>, guild_id: &str, agent_roles: Vec<String>) {
        self.guilds.insert(
            name.to_string(),
            GuildProcess::new_v2(
                name,
                GuildLauncher::Python { module_path: module_path.to_string() },
                always_on,
                timeout_ms,
                guild_id,
                agent_roles,
                self.max_concurrent,
            ),
        );
        let _ = self.save();
    }

    /// Register a remote HTTP/SSE MCP server (no subprocess — connects via HTTP).
    pub fn register_http_mcp(
        &mut self,
        name: &str,
        url: &str,
        headers: HashMap<String, String>,
        timeout_ms: Option<u64>,
    ) {
        self.guilds.insert(
            name.to_string(),
            GuildProcess::new(
                name,
                GuildLauncher::Http {
                    url: url.to_string(),
                    headers,
                    timeout_ms,
                },
                false,
                timeout_ms,
                self.max_concurrent,
            ),
        );
        let _ = self.save();
    }

    /// Register a Classic SSE MCP server (persistent GET stream + POST requests).
    pub fn register_sse_mcp(
        &mut self,
        name: &str,
        sse_url: &str,
        post_url: &str,
        headers: HashMap<String, String>,
        timeout_ms: Option<u64>,
    ) {
        self.guilds.insert(
            name.to_string(),
            GuildProcess::new(
                name,
                GuildLauncher::Sse {
                    sse_url: sse_url.to_string(),
                    post_url: post_url.to_string(),
                    headers,
                    timeout_ms,
                },
                false,
                timeout_ms,
                self.max_concurrent,
            ),
        );
        let _ = self.save();
    }

    /// Register an external MCP server descriptor.
    pub fn register_external(&mut self, name: &str, command: &str, args: Vec<String>, cwd: Option<PathBuf>, env: Option<HashMap<String, String>>, timeout_ms: Option<u64>) {
        self.guilds.insert(
            name.to_string(),
            GuildProcess::new(
                name,
                GuildLauncher::External {
                    command: command.to_string(),
                    args,
                    cwd,
                    env,
                },
                false, // External servers are on-demand by default
                timeout_ms,
                self.max_concurrent,
            ),
        );
        let _ = self.save();
    }

    /// Rebuild the tool→guild routing table from all running guilds.
    pub fn rebuild_tool_index(&mut self) {
        self.tool_to_guild.clear();
        for (guild_name, guild) in &self.guilds {
            for tool in &guild.tools {
                self.tool_to_guild.insert(
                    tool.name.to_string(),
                    guild_name.clone(),
                );
            }
        }
        debug!(
            "🗺️  Tool index rebuilt: {} tools across {} guilds",
            self.tool_to_guild.len(),
            self.guilds.len()
        );
    }

    /// Find which guild owns a specific tool.
    pub fn find_guild_for_tool(&self, tool_name: &str) -> Option<&str> {
        self.tool_to_guild.get(tool_name).map(|s| s.as_str())
    }

    /// Get all tools from all running guilds.
    pub fn all_tools(&self) -> Vec<Tool> {
        self.guilds
            .values()
            .flat_map(|g| g.tools.clone())
            .collect()
    }

    /// Get the names of all registered guilds and their status.
    pub fn status_all(&self) -> Vec<GuildStatus> {
        let catalog = crate::router::catalog::builtin_catalog();
        self.guilds
            .values()
            .map(|g| {
                let now = Instant::now();
                let restarts_5m = g.restarts.iter().filter(|t| now.duration_since(**t).as_secs() < 300).count() as u32;
                let launcher_type = match g.launcher {
                    GuildLauncher::Python { .. } => "python",
                    GuildLauncher::External { .. } => "external",
                    GuildLauncher::Http { .. } => "http",
                    GuildLauncher::Sse { .. } => "sse",
                }.to_string();
                let capabilities = catalog.iter()
                    .find(|d| d.name == g.name)
                    .and_then(|d| d.capabilities.clone());
                GuildStatus {
                    name: g.name.clone(),
                    running: g.is_running(),
                    always_on: g.always_on,
                    tools_count: g.tools.len(),
                    idle_seconds: g.idle_seconds(),
                    restarts_5m,
                    total_calls: g.perf_total_calls.load(Ordering::Relaxed),
                    last_latency_ms: g.last_latency_ms,
                    launcher_type,
                    capabilities,
                    agent_roles: g.agent_roles.clone(),
                }
            })
            .collect()
    }

    /// Get detailed health report for all guilds
    /// NOTE: Per-process CPU/memory requires storing child PIDs (future work).
    /// Currently reports honest zeros instead of fake placeholders.
    pub fn get_health_report(&self) -> Vec<GuildHealth> {
        self.guilds
            .values()
            .map(|g| {
                let status = if g.is_running() { "online" } else { "offline" };
                GuildHealth {
                    name: g.name.clone(),
                    status: status.to_string(),
                    cpu_usage: 0.0,
                    memory_kb: 0,
                    uptime_secs: if g.is_running() { g.last_access.elapsed().as_secs() } else { 0 },
                    tools_active: g.tools.len(),
                }
            })
            .collect()
    }

    /// Spawn all always-on core guilds.
    pub async fn spawn_core_guilds(&mut self) -> Result<()> {
        let core_names: Vec<String> = self
            .guilds
            .values()
            .filter(|g| g.always_on)
            .map(|g| g.name.clone())
            .collect();

        for name in core_names {
            let effective_timeouts = self.timeouts_for(&name);
            if let Some(guild) = self.guilds.get_mut(&name) {
                match guild.spawn(&self.guilds_dir, &effective_timeouts).await {
                    Ok(_) => {}
                    Err(e) => {
                        error!("Failed to spawn core guild '{}': {}", name, e);
                    }
                }
            }
        }

        // Rebuild tool index after spawning
        self.rebuild_tool_index();
        Ok(())
    }

    /// Spawn a guild by name (on-demand loading).
    /// Includes exponential backoff: if a guild crashed N times consecutively,
    /// it must wait 2^N seconds (max 300s) before retrying.
    pub async fn ensure_guild_running(&mut self, guild_name: &str) -> Result<()> {
        if let Some(guild) = self.guilds.get(guild_name) {
            if guild.is_running() {
                return Ok(());
            }
            
            // T13: Crash backoff — prevent crash-loop storms
            if guild.crash_count > 0 {
                let backoff_secs = std::cmp::min(2u64.pow(guild.crash_count), 300);
                if let Some(last_crash) = guild.last_crash_at {
                    let elapsed = last_crash.elapsed().as_secs();
                    if elapsed < backoff_secs {
                        bail!(
                            "Guild '{}' is in crash backoff ({}/{} failures). Retry in {}s.",
                            guild_name, guild.crash_count, 5, backoff_secs - elapsed
                        );
                    }
                }
            }
        }

        let guilds_dir = self.guilds_dir.clone();
        let timeouts = self.timeouts_for(guild_name);
        if let Some(guild) = self.guilds.get_mut(guild_name) {
            match guild.spawn(&guilds_dir, &timeouts).await {
                Ok(_) => {
                    // Reset crash counter on success
                    guild.crash_count = 0;
                    guild.last_crash_at = None;
                    self.rebuild_tool_index();
                    let _ = self.save();
                    Ok(())
                }
                Err(e) => {
                    // Increment crash counter
                    guild.crash_count += 1;
                    guild.last_crash_at = Some(Instant::now());
                    let backoff = std::cmp::min(2u64.pow(guild.crash_count), 300);
                    warn!(
                        "⚠️ Guild '{}' crash #{} — backoff {}s before next retry",
                        guild_name, guild.crash_count, backoff
                    );
                    let _ = self.save();
                    Err(e)
                }
            }
        } else {
            bail!("Unknown guild: '{guild_name}'");
        }
    }

    /// Kill idle non-core guilds that exceeded the timeout.
    pub async fn reap_idle_guilds(&mut self) {
        let idle_names: Vec<String> = self
            .guilds
            .values()
            .filter(|g| !g.always_on && g.is_running() && g.idle_seconds() > self.timeout_secs)
            .map(|g| g.name.clone())
            .collect();

        for name in &idle_names {
            if let Some(guild) = self.guilds.get_mut(name) {
                warn!("⏱️  Auto-unloading idle guild '{}'", name);
                guild.kill().await.ok();
            }
        }

        if !idle_names.is_empty() {
            self.rebuild_tool_index();
        }
    }

    /// Register a guild discovered via the ingestion pipeline (Phase B).
    ///
    /// Takes the output from `sandbox_ingest` and creates the appropriate
    /// GuildLauncher. Returns an `IngestResult` describing what was registered.
    pub fn register_from_ingest(
        &mut self,
        name: &str,
        workspace_path: &std::path::Path,
        guild_type: &str,
        entry_point: &str,
    ) -> IngestResult {
        // Sanitize name: only lowercase letters, digits, and hyphens
        let safe_name: String = name.chars()
            .map(|c| if c.is_alphanumeric() || c == '-' { c.to_ascii_lowercase() } else { '-' })
            .collect();

        if self.guilds.contains_key(&safe_name) {
            return IngestResult {
                name: safe_name,
                guild_type: guild_type.to_string(),
                status: IngestStatus::AlreadyRegistered,
                message: format!("Guild '{name}' is already registered. Kill it first to re-ingest."),
            };
        }

        match guild_type {
            "fastmcp-python" => {
                // Convert workspace path into a Python dotted module path.
                // e.g. data/guilds_workspace/my-tool/ + entry "main" → guilds_workspace.my_tool.main
                let module_name = safe_name.replace('-', "_");
                let ep = if entry_point.is_empty() { "main" } else { entry_point };
                // We use the guilds_workspace package under data/
                let module_path = format!("guilds_workspace.{module_name}.{ep}");

                self.register(&safe_name, &module_path, false, None);
                info!("📦 [Ingest] Registered FastMCP Python guild '{}' → {}", safe_name, module_path);

                IngestResult {
                    name: safe_name,
                    guild_type: guild_type.to_string(),
                    status: IngestStatus::Registered,
                    message: format!("Registered as Python module '{module_path}'"),
                }
            }
            "node-mcp" => {
                let ep = if entry_point.is_empty() { "index.js" } else { entry_point };
                let cwd = workspace_path.to_path_buf();

                self.register_external(
                    &safe_name,
                    "node",
                    vec![ep.to_string()],
                    Some(cwd),
                    None,
                    None,
                );
                info!("📦 [Ingest] Registered Node MCP guild '{}' → node {}", safe_name, ep);

                IngestResult {
                    name: safe_name,
                    guild_type: guild_type.to_string(),
                    status: IngestStatus::Registered,
                    message: format!("Registered as Node MCP server (entry: {ep})"),
                }
            }
            other => {
                IngestResult {
                    name: safe_name,
                    guild_type: other.to_string(),
                    status: IngestStatus::Unsupported,
                    message: format!(
                        "Guild type '{other}' is not yet supported for automatic registration. \
                         Supported: fastmcp-python, node-mcp."
                    ),
                }
            }
        }
    }
}

/// Status snapshot of a guild (for reporting/TUI).
#[derive(Debug, Clone, serde::Serialize)]
pub struct GuildStatus {
    pub name: String,
    pub running: bool,
    pub always_on: bool,
    pub tools_count: usize,
    pub idle_seconds: u64,
    pub restarts_5m: u32,
    pub total_calls: u64,
    pub last_latency_ms: Option<u64>,
    pub launcher_type: String,
    /// Capability declarations parsed from the Python guild file.
    /// Null if the guild doesn't declare capabilities.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<serde_json::Value>,
    /// Agent roles assigned to this guild (e.g. ["architect","backend-dev"]).
    /// Empty if the guild has no role restrictions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agent_roles: Vec<String>,
}

/// Result of a guild ingestion attempt (Phase B).
#[derive(Debug, Clone, serde::Serialize)]
pub struct IngestResult {
    pub name: String,
    pub guild_type: String,
    pub status: IngestStatus,
    pub message: String,
}

/// Status of a guild ingestion attempt.
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub enum IngestStatus {
    /// Successfully registered in the kernel registry.
    Registered,
    /// Guild with this name already exists.
    AlreadyRegistered,
    /// Guild type is not supported for automatic registration.
    Unsupported,
}

/// Detailed health metrics for a guild (Cycle 3)
#[derive(Debug, Clone, serde::Serialize)]
pub struct GuildHealth {
    pub name: String,
    pub status: String, // "online", "idle", "error"
    pub cpu_usage: f32,
    pub memory_kb: u64,
    pub uptime_secs: u64,
    pub tools_active: usize,
}

/// Detect the correct Python binary for the current platform.
///
/// - Windows: tries `python` then `python3`
/// - Linux/macOS: tries `python3` then `python`
///
/// Validates that the found binary is Python 3.x (not 2.x).
pub async fn find_python() -> Result<String> {
    // 0. Try local .venv with robust ABSOLUTE check (Sovereign priority)
    let current_dir = std::env::current_dir()?;
    
    // Find workspace root by looking for Cargo.toml upwards or using a known structure
    // rústico: try current, then parent, then parent's parent
    let mut root = current_dir.clone();
    let mut venv_python = None;

    for _ in 0..3 {
        let candidate = if cfg!(target_os = "windows") {
            root.join(".venv").join("Scripts").join("python.exe")
        } else {
            root.join(".venv").join("bin").join("python")
        };

        if candidate.exists() {
            venv_python = Some(candidate);
            break;
        }
        if let Some(parent) = root.parent() {
            root = parent.to_path_buf();
        } else {
            break;
        }
    }

    if let Some(venv_path) = venv_python {
        if let Ok(path_str) = venv_path.canonicalize() {
             let final_path = path_str.to_string_lossy().to_string();
             // Critical for Windows: remove UNC prefix \\?\ if present for subprocess stability
             let final_path = final_path.trim_start_matches(r"\\?\").to_string();
             debug!("Sovereign Venv found: {}", final_path);
             return Ok(final_path);
        } else {
            warn!("Failed to canonicalize .venv python path, trying direct path");
            let final_path = venv_path.to_string_lossy().to_string();
            return Ok(final_path);
        }
    } else {
        warn!(".venv not found in chain, falling back to system python");
    }

    let candidates = if cfg!(target_os = "windows") {
        vec!["python", "python3"]
    } else {
        vec!["python3", "python"]
    };

    for candidate in &candidates {
        let result = Command::new(candidate)
            .arg("--version")
            .output()
            .await;

        if let Ok(output) = result {
            let version = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let version_str = if version.contains("Python") {
                version.to_string()
            } else {
                stderr.to_string()
            };

            if version_str.contains("Python 3") {
                debug!("Found Python: {} → {}", candidate, version_str.trim());
                return Ok(candidate.to_string());
            }
        }
    }

    bail!(
        "Python 3 not found. Tried: {candidates:?}. \
         Please install Python 3.10+ and ensure it's in your PATH."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlimited_patience_guilds_keep_flat_timeout_not_heavy_category() {
        // Real regression caught before shipping (2026-08-23): deep_analysis
        // and friends are documented elsewhere as needing 10+ minutes on
        // CPU, but heavy_guild_ms defaults to 180s -- far too short. They
        // must always get the flat tool_call_secs value, never the Heavy
        // category's shorter ceiling, regardless of with_guild_timeouts().
        let registry = GuildRegistry::new(
            PathBuf::from("."), 300,
            TimeoutsConfig { handshake_secs: 120, tool_call_secs: 3600 },
            3,
        ).with_guild_timeouts(GuildTimeoutsConfig {
            system_guild_ms: 15_000,
            analysis_guild_ms: 60_000,
            heavy_guild_ms: 180_000, // 180s -- must NOT apply to these guilds
            mcp_client_heartbeat_ms: 8_000,
        });

        for name in ["vision", "deep_analysis", "knowledge", "comfy_ui", "n8n_bridge"] {
            assert_eq!(
                registry.effective_tool_call_secs(name), 3600,
                "'{name}' must keep the flat 3600s ceiling, not the 180s heavy category"
            );
        }

        // A genuinely non-inference heavy guild (docker) SHOULD get the
        // real category value -- confirms the carve-out is scoped to the
        // specific unlimited-patience list, not a blanket bypass.
        assert_eq!(registry.effective_tool_call_secs("docker"), 180);
        // A system guild gets its own, shorter category value.
        assert_eq!(registry.effective_tool_call_secs("bash"), 15);
    }

    #[test]
    fn test_guild_process_creation() {
        let launcher = GuildLauncher::Python { module_path: "guilds.core.bash".to_string() };
        let guild = GuildProcess::new("bash", launcher, true, None, 3);
        assert_eq!(guild.name, "bash");
        assert!(!guild.is_running());
        assert!(guild.always_on);
        assert!(guild.tools.is_empty());
    }

    #[test]
    fn test_guild_touch_resets_idle() {
        let launcher = GuildLauncher::Python { module_path: "test.module".to_string() };
        let mut guild = GuildProcess::new("test", launcher, false, None, 3);
        assert!(guild.idle_seconds() < 2);
        guild.touch();
        assert!(guild.idle_seconds() < 2);
    }

    #[test]
    fn test_registry_creation_and_register() {
        let mut registry = GuildRegistry::new(PathBuf::from("."), 300, TimeoutsConfig::default(), 3);
        registry.register("bash", "guilds.core.bash", true, None);
        registry.register("git", "guilds.builders.git", false, None);

        assert_eq!(registry.guilds.len(), 2);
        assert!(registry.guilds.contains_key("bash"));
        assert!(registry.guilds.contains_key("git"));
    }

    #[test]
    fn test_status_all() {
        let mut registry = GuildRegistry::new(PathBuf::from("."), 300, TimeoutsConfig::default(), 3);
        registry.register("bash", "guilds.core.bash", true, None);
        registry.register("git", "guilds.builders.git", false, None);

        let statuses = registry.status_all();
        assert_eq!(statuses.len(), 2);

        let bash = statuses.iter().find(|s| s.name == "bash").unwrap();
        assert!(bash.always_on);
        assert!(!bash.running);
    }

    #[test]
    fn test_tool_to_guild_routing() {
        let mut registry = GuildRegistry::new(PathBuf::from("."), 300, TimeoutsConfig::default(), 3);
        registry.register("bash", "guilds.core.bash", true, None);

        // No tools registered yet
        assert!(registry.find_guild_for_tool("bash_execute").is_none());
        assert!(registry.all_tools().is_empty());
    }

    #[test]
    fn test_rebuild_tool_index_empty() {
        let mut registry = GuildRegistry::new(PathBuf::from("."), 300, TimeoutsConfig::default(), 3);
        registry.register("bash", "guilds.core.bash", true, None);
        registry.rebuild_tool_index();
        assert!(registry.tool_to_guild.is_empty());
    }

    #[tokio::test]
    async fn test_find_python() {
        let result = find_python().await;
        // Just verify it returns Ok - path can be absolute or just "python"
        assert!(result.is_ok(), "find_python should succeed");
    }

    #[tokio::test]
    async fn test_reap_idle_with_no_guilds() {
        let mut registry = GuildRegistry::new(PathBuf::from("."), 1, TimeoutsConfig::default(), 3);
        registry.reap_idle_guilds().await;
    }

    #[tokio::test]
    async fn test_ensure_guild_running_unknown() {
        let mut registry = GuildRegistry::new(PathBuf::from("."), 300, TimeoutsConfig::default(), 3);
        let result = registry.ensure_guild_running("nonexistent").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_register_http_mcp() {
        let mut registry = GuildRegistry::new(PathBuf::from("."), 300, TimeoutsConfig::default(), 3);
        registry.register_http_mcp(
            "remote-mcp",
            "https://mcp.example.com",
            HashMap::new(),
            Some(30_000),
        );
        let guild = registry.guilds.get("remote-mcp").unwrap();
        assert!(!guild.is_running());
        assert!(matches!(guild.launcher, GuildLauncher::Http { .. }));
        assert_eq!(guild.description(), "HTTP MCP: https://mcp.example.com");
    }

    #[test]
    fn test_http_mcp_vs_stdio_registration() {
        let mut registry = GuildRegistry::new(PathBuf::from("."), 300, TimeoutsConfig::default(), 3);
        // HTTP MCP (url only)
        registry.register_http_mcp("api-mcp", "https://api.test.com/mcp", HashMap::new(), None);
        // Stdio external MCP (command)
        registry.register_external("npx-mcp", "npx", vec!["-y".into(), "@test/mcp".into()], None, None, None);

        assert_eq!(registry.guilds.len(), 2);
        assert!(matches!(registry.guilds["api-mcp"].launcher, GuildLauncher::Http { .. }));
        assert!(matches!(registry.guilds["npx-mcp"].launcher, GuildLauncher::External { .. }));
    }

    // ─── enforce_capabilities deterministic unit tests ───────────────────

    fn make_params(name: &str, args: &[(&str, &str)]) -> CallToolRequestParam {
        let mut m = serde_json::Map::new();
        for (k, v) in args {
            m.insert(k.to_string(), serde_json::Value::String(v.to_string()));
        }
        CallToolRequestParam {
            name: name.to_string().into(),
            arguments: if m.is_empty() { None } else { Some(m) },
        }
    }

    #[test]
    fn test_enforce_no_caps_passes() {
        let caps = serde_json::Value::Null;
        let params = make_params("bash_execute", &[("command", "rm -rf /")]);
        assert!(enforce_capabilities("test", &caps, &params).is_none());
    }

    #[test]
    fn test_enforce_process_execution_blocks_exec_tool() {
        let caps = serde_json::json!({"process_execution": false});
        let params = make_params("bash_execute", &[("command", "echo hi")]);
        let result = enforce_capabilities("test", &caps, &params);
        assert!(result.is_some());
        assert!(result.unwrap().contains("CAPABILITY_BLOCKED"));
    }

    #[test]
    fn test_enforce_process_execution_blocks_command_arg() {
        let caps = serde_json::json!({"process_execution": false});
        let params = make_params("filesystem_read", &[("command", "rm -rf /")]);
        let result = enforce_capabilities("test", &caps, &params);
        assert!(result.is_some());
    }

    #[test]
    fn test_enforce_process_execution_allows_safe_tool() {
        let caps = serde_json::json!({"process_execution": false});
        let params = make_params("vision_analyze", &[("image_path", "/tmp/photo.jpg")]);
        assert!(enforce_capabilities("test", &caps, &params).is_none());
    }

    #[test]
    fn test_enforce_filesystem_scope_blocks_outside() {
        let caps = serde_json::json!({"filesystem_scope": ["/tmp"]});
        let params = make_params("filesystem_read", &[("path", "/etc/passwd")]);
        let result = enforce_capabilities("test", &caps, &params);
        assert!(result.is_some());
        assert!(result.unwrap().contains("filesystem_scope"));
    }

    #[test]
    fn test_enforce_filesystem_scope_allows_inside() {
        let caps = serde_json::json!({"filesystem_scope": ["/tmp"]});
        let params = make_params("filesystem_read", &[("path", "/tmp/test.txt")]);
        assert!(enforce_capabilities("test", &caps, &params).is_none());
    }

    #[test]
    fn test_enforce_process_execution_true_allows_exec() {
        let caps = serde_json::json!({"process_execution": true});
        let params = make_params("bash_execute", &[("command", "echo hi")]);
        assert!(enforce_capabilities("test", &caps, &params).is_none());
    }

    #[test]
    fn test_enforce_no_caps_key_skips() {
        let caps = serde_json::json!({"process_execution": false});
        let params = make_params("noop_tool", &[]);
        // no command arg and tool name is safe
        assert!(enforce_capabilities("test", &caps, &params).is_none());
    }

    #[test]
    fn test_enforce_empty_filesystem_scope_skips() {
        let caps = serde_json::json!({"filesystem_scope": []});
        let params = make_params("filesystem_read", &[("path", "/etc/passwd")]);
        // empty scope = no restriction
        assert!(enforce_capabilities("test", &caps, &params).is_none());
    }

    #[tokio::test]
    async fn test_check_capabilities_guild_not_in_catalog_returns_none() {
        let launcher = GuildLauncher::Python { module_path: "nonexistent.module".to_string() };
        let guild = GuildProcess::new("no-such-guild", launcher, false, None, 3);
        let params = make_params("bash_execute", &[("command", "rm -rf /")]);
        assert!(guild.check_capabilities(&params).await.is_none());
    }

    #[tokio::test]
    async fn test_check_capabilities_default_config_returns_none() {
        // websearch has process_execution=false in catalog — but enforce is false by default
        let launcher = GuildLauncher::Python { module_path: "guilds.core.websearch".to_string() };
        let guild = GuildProcess::new("websearch", launcher, false, None, 3);
        let params = make_params("bash_execute", &[("command", "rm -rf /")]);
        assert!(guild.check_capabilities(&params).await.is_none());
    }

    #[test]
    fn test_enforce_check_cwd_argument() {
        let caps = serde_json::json!({"filesystem_scope": ["/workspace"]});
        let params = make_params("bash_execute", &[("command", "ls"), ("cwd", "/etc")]);
        let result = enforce_capabilities("test", &caps, &params);
        assert!(result.is_some());
        assert!(result.unwrap().contains("/etc"));

        let params_allowed = make_params("bash_execute", &[("command", "ls"), ("cwd", "/workspace/proj")]);
        assert!(enforce_capabilities("test", &caps, &params_allowed).is_none());
    }

    #[test]
    fn test_enforce_check_directory_argument() {
        let caps = serde_json::json!({"filesystem_scope": ["/data"]});
        let params = make_params("filesystem_list", &[("directory", "/home/user")]);
        let result = enforce_capabilities("test", &caps, &params);
        assert!(result.is_some());
        assert!(result.unwrap().contains("/home/user"));

        let params_allowed = make_params("filesystem_list", &[("directory", "/data/snapshots")]);
        assert!(enforce_capabilities("test", &caps, &params_allowed).is_none());
    }
}

impl GuildRegistry {
    /// Path to the registry persistence file
    fn persistence_path(&self) -> PathBuf {
        self.guilds_dir.join("registry.json")
    }

    /// Save registry state to disk (T27)
    /// Only persists metadata, not running state (proxy)
    pub fn save(&self) -> Result<()> {
        use serde::Serialize;

        #[derive(Serialize)]
        struct PersistedGuild {
            name: String,
            launcher: GuildLauncher,
            always_on: bool,
            tool_timeout_ms: Option<u64>,
            crash_count: u32,
            last_crash_unix: Option<i64>,
            last_latency_ms: Option<u64>,
            total_calls: u64,
            is_remote: bool,
        }

        let path = self.persistence_path();
        let persisted: Vec<PersistedGuild> = self.guilds.values().filter_map(|g| {
            // Mirror the ghost-detection logic from load(): skip Python guilds whose
            // module file no longer exists so they don't re-appear after restart.
            if let GuildLauncher::Python { module_path } = &g.launcher {
                let module_file = self.guilds_dir
                    .join(format!("{}.py", module_path.replace('.', "/")));
                if !module_file.exists() {
                    warn!("👻 [T27-save] Skipping ghost guild '{}' (missing {})", g.name, module_file.display());
                    return None;
                }
            }
            Some(PersistedGuild {
                name: g.name.clone(),
                launcher: g.launcher.clone(),
                always_on: g.always_on,
                tool_timeout_ms: g.tool_timeout.map(|d| d.as_millis() as u64),
                crash_count: g.crash_count,
                last_crash_unix: g.last_crash_at.map(|i| {
                    // Convert Instant to roughly unix (offset from now)
                    chrono::Utc::now().timestamp() - i.elapsed().as_secs() as i64
                }),
                last_latency_ms: g.last_latency_ms,
                total_calls: g.total_calls,
                is_remote: matches!(g.launcher, GuildLauncher::Http { .. } | GuildLauncher::Sse { .. }),
            })
        }).collect();

        let json = serde_json::to_string_pretty(&persisted)?;
        
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, json)?;
        
        debug!("📦 [T27] Registry saved to {:?}", path);
        Ok(())
    }

    /// Load registry state from disk (T27)
    pub fn load(&mut self) -> Result<()> {
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct PersistedGuild {
            name: String,
            launcher: GuildLauncher,
            always_on: bool,
            tool_timeout_ms: Option<u64>,
            crash_count: u32,
            last_crash_unix: Option<i64>,
        }

        let path = self.persistence_path();
        if !path.exists() {
            debug!("📦 [T27] No registry file found, starting fresh");
            return Ok(());
        }

        let content = std::fs::read_to_string(&path)?;
        let persisted: Vec<PersistedGuild> = serde_json::from_str(&content)?;

        let mut ghosts: Vec<String> = Vec::new();
        for pg in persisted {
            // [DT1] Reject ghost guilds whose Python module was deleted from the tree.
            // Without this, `cargo run` after `git rm guilds/core/X.py` loops forever
            // in crash-backoff trying to spawn a module that no longer exists.
            if let GuildLauncher::Python { module_path } = &pg.launcher {
                let module_file = self.guilds_dir
                    .join(format!("{}.py", module_path.replace('.', "/")));
                if !module_file.exists() {
                    ghosts.push(pg.name.clone());
                    continue;
                }
            }

            let _tool_timeout = pg.tool_timeout_ms.map(std::time::Duration::from_millis);
            let last_crash_at = pg.last_crash_unix.map(|_| Instant::now());

            let mut guild = GuildProcess::new(&pg.name, pg.launcher.clone(), pg.always_on, pg.tool_timeout_ms, self.max_concurrent);
            guild.crash_count = pg.crash_count;
            guild.last_crash_at = last_crash_at;

            self.guilds.insert(pg.name.clone(), guild);
        }

        if !ghosts.is_empty() {
            warn!("👻 [T27] Skipped {} ghost guild(s) with missing module(s): {:?}", ghosts.len(), ghosts);
            // Persist the cleaned state so the ghost entries don't reappear next boot.
            let _ = self.save();
        }

        self.rebuild_tool_index();
        info!("Registry loaded: {} guilds", self.guilds.len());
        Ok(())
    }
}

/// Performance statistics for a guild, computed from in-memory counters.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GuildCallStats {
    pub guild_name: String,
    pub total_calls: u64,
    pub successful_calls: u64,
    pub avg_latency_ms: f64,
    pub last_call_unix: u64,
    pub success_rate: f64,
}
