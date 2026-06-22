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
use crate::config::TimeoutsConfig;
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
            GuildLauncher::Python { module_path } => format!("Python guild: {}", module_path),
            GuildLauncher::External { command, args, .. } => format!("External: {} {}", command, args.join(" ")),
            GuildLauncher::Http { url, .. } => format!("HTTP MCP: {}", url),
            GuildLauncher::Sse { sse_url, .. } => format!("SSE MCP: {}", sse_url),
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
                
                // S1: Docker sandbox for bash/code guilds
                if (self.name == "bash" || self.name == "code")
                    && let Some(sb) = crate::config::load_sandbox_config()
                {
                    info!("🐳 Sandbox: guild '{}' running in Docker container '{}'", self.name, sb.image);
                    let volume_bind = format!("{}:/workspace:ro", workspace_root.display());
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

    /// Internal call with proxy - for use when lock is already held
    /// Returns a valid CallToolResult in ALL cases — never propagates errors.
    pub async fn call_tool_with_proxy(&self, params: CallToolRequestParam) -> CallToolResult {
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
            max_concurrent,
            db_conn: None,
        }
    }

    /// Initialize metrics database for persistence.
    pub fn init_metrics_db(&mut self, db_path: &str) -> Result<()> {
        let conn = Connection::open(db_path)?;
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
                            self.register(&name, &format!("guilds.{}", name), false, None);
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
            if let Some(guild) = self.guilds.get_mut(&name) {
                match guild.spawn(&self.guilds_dir, &self.timeouts).await {
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
        let timeouts = self.timeouts.clone();
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
            bail!("Unknown guild: '{}'", guild_name);
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
                message: format!("Guild '{}' is already registered. Kill it first to re-ingest.", name),
            };
        }

        match guild_type {
            "fastmcp-python" => {
                // Convert workspace path into a Python dotted module path.
                // e.g. data/guilds_workspace/my-tool/ + entry "main" → guilds_workspace.my_tool.main
                let module_name = safe_name.replace('-', "_");
                let ep = if entry_point.is_empty() { "main" } else { entry_point };
                // We use the guilds_workspace package under data/
                let module_path = format!("guilds_workspace.{}.{}", module_name, ep);

                self.register(&safe_name, &module_path, false, None);
                info!("📦 [Ingest] Registered FastMCP Python guild '{}' → {}", safe_name, module_path);

                IngestResult {
                    name: safe_name,
                    guild_type: guild_type.to_string(),
                    status: IngestStatus::Registered,
                    message: format!("Registered as Python module '{}'", module_path),
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
                    message: format!("Registered as Node MCP server (entry: {})", ep),
                }
            }
            other => {
                IngestResult {
                    name: safe_name,
                    guild_type: other.to_string(),
                    status: IngestStatus::Unsupported,
                    message: format!(
                        "Guild type '{}' is not yet supported for automatic registration. \
                         Supported: fastmcp-python, node-mcp.",
                        other
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
        "Python 3 not found. Tried: {:?}. \
         Please install Python 3.10+ and ensure it's in your PATH.",
        candidates
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
