//! # MCP Proxy
//!
//! Connects the TylluanNexus kernel to Python guild subprocesses using the
//! MCP protocol (JSON-RPC over stdio). Uses `rmcp` as an MCP **client** to
//! communicate with the child process, which runs as an MCP **server**.
//!
//! ## Flow
//!
//! ```text
//! IDE Client                  TylluanNexus Kernel                  Python Guild
//! ─────────── call_tool ───►  TylluanServer                        bash.py
//!                             │                                  (FastMCP server)
//!                             ▼                                  │
//!                             McpProxy                           │
//!                             │  uses Peer<RoleClient>           │
//!                             ├── list_all_tools() ──────────►   │
//!                             │◄── [bash_execute, ...] ──────    │
//!                             ├── call_tool(args) ───────────►   │
//!                             │◄── CallToolResult ───────────    │
//! ◄── CallToolResult ────────┘
//! ```

use crate::config::TimeoutsConfig;
use crate::registry::tools::enrich_tool;
use anyhow::{Context, Result};
use rmcp::model::{
    CallToolRequestParam, CallToolResult, Tool, Content,
};
use rmcp::service::{Peer, RoleClient, RunningService, serve_client};
use rmcp::transport::child_process::TokioChildProcess;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::{oneshot, Mutex};
use tokio::time::timeout;
use tracing::{info, debug, error, warn};

/// A proxy connection to a single Python guild subprocess.
/// Holds the rmcp RunningService and provides list_tools/call_tool.
pub struct McpProxy {
    /// Name of the guild this proxy connects to
    guild_name: String,
    /// The rmcp running service (holds the event loop)
    service: RunningService<RoleClient, ()>,
    /// Tool call timeout in seconds
    tool_call_timeout: u64,
}

impl McpProxy {
    /// Spawn a generic MCP process and establish MCP connection.
    ///
    /// This:
    /// 1. Launches the provided command as a child process
    /// 2. Connects via MCP stdio protocol (JSON-RPC)
    /// 3. Completes the MCP initialize handshake
    /// 4. Returns a proxy ready for list_tools/call_tool
    ///
    /// ANTI-ORPHAN: If the handshake fails, the child process is killed
    /// to prevent zombie processes consuming RAM on user hardware.
    pub async fn spawn(
        guild_name: &str,
        mut command: Command,
        timeouts: &TimeoutsConfig,
    ) -> Result<Self> {
        info!("⚙️  McpProxy: spawning '{}'...", guild_name);

        // Ensure stderr is piped so we can capture errors, and stdout/stdin for MCP
        info!("🔧 McpProxy: creating TokioChildProcess for '{}'...", guild_name);
        
        // Configure common pipes
        command
            .stdout(std::process::Stdio::piped())
            .stdin(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit());

        // Create TokioChildProcess - this internally MUST handle spawning
        let child_process = TokioChildProcess::new(&mut command)
            .with_context(|| {
                error!("❌ TokioChildProcess::new() FAILED for guild '{}'", guild_name);
                format!("Failed to spawn guild '{}' subprocess.", guild_name)
            })?;

        info!("🔧 McpProxy: TokioChildProcess created for '{}'", guild_name);

        // Connect as MCP client — this performs the initialize handshake
        let handler = ();
        info!("🔌 McpProxy: calling serve_client for '{}'...", guild_name);
        
        // ANTI-ORPHAN: We don't have the PID handle (rmcp hides it), 
        // so we must use a reactive cleanup if the handshake fails.
        let service = match timeout(
            Duration::from_secs(timeouts.handshake_secs),
            serve_client(handler, child_process)
        ).await {
            Ok(Ok(service)) => service,
            Ok(Err(e)) => {
                error!("❌ MCP handshake FAILED for guild '{}': {}. Cleaning up orphans...", guild_name, e);
                Self::kill_abandoned_child(guild_name);
                return Err(anyhow::anyhow!(
                    "MCP handshake failed with guild '{}': {}. Is the server a valid MCP stdio server?",
                    guild_name, e
                ));
            }
            Err(_) => {
                error!("❌ Handshake TIMEOUT for guild '{}'. Cleaning up orphans...", guild_name);
                Self::kill_abandoned_child(guild_name);
                return Err(anyhow::anyhow!(
                    "Handshake timeout after {}s with guild '{}'.",
                    timeouts.handshake_secs, guild_name
                ));
            }
        };

        let server_info = service.peer_info();
        info!(
            "✅ McpProxy: connected to '{}' (server: {} v{})",
            guild_name,
            server_info.server_info.name,
            server_info.server_info.version
        );

        if let Err(e) = crate::process_isolation::isolate_guild_process(guild_name, 20) {
            warn!("⚠️ Could not apply CPU isolation to '{}': {}", guild_name, e);
        }

        Ok(Self {
            guild_name: guild_name.to_string(),
            service,
            tool_call_timeout: timeouts.tool_call_secs,
        })
    }

    /// Reactive orphan cleanup: Find and kill any Python processes launched by this kernel
    /// that are associated with the guild name but failed to complete handshake.
    /// Enhanced to also clean stale processes from previous sessions.
    pub(crate) fn kill_abandoned_child(guild_name: &str) {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        
        let self_pid = std::process::id();
        let mut killed_count = 0;
        let guild_pattern = guild_name.replace('-', "_");

        for (pid, process) in sys.processes() {
            let cmd = process.cmd().iter()
                .map(|s| s.to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join(" ");
            let exe_name = process.exe().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            
            // Strategy 1: Direct children of this kernel
            if let Some(parent_pid) = process.parent()
                && parent_pid.as_u32() == self_pid {
                    let cmd_lower = cmd.to_lowercase();
                    let name_lower = guild_name.to_lowercase();
                    let pattern_lower = guild_pattern.to_lowercase();
                    let is_guild = cmd.contains("guilds.") || cmd.contains("guilds/") || cmd.contains("guilds\\");
                    let matches_name = cmd_lower.contains(&name_lower) 
                        || cmd_lower.contains(&pattern_lower)
                        || (guild_name == "scrapling" && cmd_lower.contains("scrapling_web"));
                    if is_guild && matches_name {
                        info!("🧹 [P0] Anti-Orphan: Killing child PID {} for '{}'", pid, guild_name);
                        process.kill();
                        killed_count += 1;
                    }
                }
            
            // Strategy 2: Stale Python processes from previous sessions (P0 fix)
            // These are Python processes that started but never completed handshake
            // and may have lost their parent reference
            if exe_name.to_lowercase().contains("python") {
                let cmd_lower = cmd.to_lowercase();
                let name_lower = guild_name.to_lowercase();
                let pattern_lower = guild_pattern.to_lowercase();
                let is_guild = cmd.contains("guilds.") || cmd.contains("guilds/") || cmd.contains("guilds\\");
                let matches_name = cmd_lower.contains(&name_lower) 
                    || cmd_lower.contains(&pattern_lower)
                    || (guild_name == "scrapling" && cmd_lower.contains("scrapling_web"));
                if is_guild && matches_name {
                    // Additional check: if it's using < 10MB it's likely a stuck stub
                    let memory_kb = process.memory() / 1024;
                    if memory_kb < 10000 {
                        info!("🧹 [P0] Anti-Orphan: Killing stale Python PID {} ({} KB) for '{}'", 
                            pid, memory_kb, guild_name);
                        process.kill();
                        killed_count += 1;
                    }
                }
            }
        }
        
        if killed_count == 0 {
            debug!("🧹 [P0] Anti-Orphan: No orphan processes found for '{}'.", guild_name);
        } else {
            // NOTE(reaping): sysinfo.kill() sends SIGKILL/TerminateProcess but does NOT
            // reap (wait) the child handle. Full reaping requires access to the
            // tokio::process::Child handle owned by TokioChildProcess inside the rmcp
            // service. The service.cancel() call in shutdown() is expected to drop that
            // handle, triggering OS-level cleanup. This warn log documents the gap so it
            // is visible in production logs rather than silent.
            tracing::warn!("🧹 Anti-orphan killed {} process(es) for guild '{}' — OS handle reaping delegated to rmcp service drop", killed_count, guild_name);
        }
    }


    /// Get the MCP peer for sending requests to the guild.
    fn peer(&self) -> &Peer<RoleClient> {
        self.service.peer()
    }

    /// List all tools exposed by this guild.
    pub async fn list_tools(&self) -> Result<Vec<Tool>> {
        debug!("📋 McpProxy: listing tools from '{}'", self.guild_name);
        let tools = timeout(
            Duration::from_secs(self.tool_call_timeout),
            self.peer().list_all_tools()
        ).await
        .map_err(|_| anyhow::anyhow!("Timeout listing tools from guild '{}' after {}s", self.guild_name, self.tool_call_timeout))?
        .with_context(|| format!(
            "Failed to list tools from guild '{}'",
            self.guild_name
        ))?;

        info!(
            "📋 McpProxy: guild '{}' exposes {} tools",
            self.guild_name,
            tools.len()
        );

        // Enrich tools with Tylluan metadata before returning them
        let enriched_tools = tools.into_iter().map(enrich_tool).collect();
        
        Ok(enriched_tools)
    }

    /// Forward a tool call to this guild and return the result.
    pub async fn call_tool(&self, params: CallToolRequestParam) -> Result<CallToolResult> {
        debug!(
            "🔧 McpProxy: forwarding '{}' to guild '{}'",
            params.name, self.guild_name
        );
        
        let result = timeout(
            Duration::from_secs(self.tool_call_timeout),
            self.peer().call_tool(params.clone())
        ).await
        .map_err(|_| {
            error!("⏱️ Guild '{}' timed out after {}s for tool '{}'", self.guild_name, self.tool_call_timeout, params.name);
            anyhow::anyhow!(
                "Guild '{}' timeout: tool '{}' exceeded {}s limit. Guild may be overloaded or unresponsive. Try again or check guild status.",
                self.guild_name, params.name, self.tool_call_timeout
            )
        })
        .with_context(|| {
            error!("❌ Tool call '{}' failed in guild '{}'", params.name, self.guild_name);
            format!("Tool call '{}' failed in guild '{}'", params.name, self.guild_name)
        })?;

        result.map_err(|e| anyhow::anyhow!("Guild '{}' internal error: {:?}", self.guild_name, e))
    }

    /// Health check: verify guild is responsive.
    /// Returns Ok(true) if guild responds to a ping, Err otherwise.
    pub async fn health_check(&self) -> Result<bool> {
        let ping_params = CallToolRequestParam {
            name: "ping".into(),
            arguments: None,
        };
        
        match timeout(
            Duration::from_secs(5),
            self.peer().call_tool(ping_params)
        ).await {
            Ok(Ok(_)) => Ok(true),
            Ok(Err(e)) => {
                warn!("Guild '{}' health check failed: {}", self.guild_name, e);
                Err(anyhow::anyhow!("Guild '{}' is not responsive: {}", self.guild_name, e))
            }
            Err(_) => {
                warn!("Guild '{}' health check timed out", self.guild_name);
                Err(anyhow::anyhow!("Guild '{}' timed out during health check", self.guild_name))
            }
        }
    }

    /// Get the guild name this proxy is connected to.
    #[allow(dead_code)]
    pub fn guild_name(&self) -> &str {
        &self.guild_name
    }

    /// Gracefully shut down the proxy and kill the child process.
    pub async fn shutdown(self) -> Result<()> {
        info!("🛑 McpProxy: shutting down guild '{}'", self.guild_name);
        let _ = self.service.cancel().await;
        Self::kill_abandoned_child(&self.guild_name);
        Ok(())
    }
}

/// Create an error CallToolResult for returning errors to the client.
pub fn error_result(message: &str) -> CallToolResult {
    CallToolResult {
        content: vec![Content::text(format!("❌ Error: {}", message))],
        is_error: Some(true),
    }
}

// ─── HTTP/SSE MCP CLIENT ──────────────────────────────────────────────────────

/// MCP client that communicates with a remote server over HTTP Streamable MCP
/// (POST /messages, JSON-RPC, with optional Mcp-Session-Id header).
pub struct HttpMcpProxy {
    client: reqwest::Client,
    /// Full endpoint URL (e.g. "https://mcp.example.com/messages")
    endpoint: String,
    guild_name: String,
    /// Returned by the server after initialize (2025-06-18 spec)
    session_id: Option<String>,
    tool_call_timeout: Duration,
    request_id: AtomicU32,
}

impl HttpMcpProxy {
    /// Connect to a remote MCP server, perform the initialize handshake,
    /// and return (proxy, discovered_tools).
    pub async fn connect(
        guild_name: &str,
        base_url: &str,
        headers: &HashMap<String, String>,
        timeout_ms: u64,
    ) -> Result<(Self, Vec<Tool>)> {
        use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
        use std::str::FromStr;

        let mut header_map = HeaderMap::new();
        header_map.insert("content-type", HeaderValue::from_static("application/json"));
        header_map.insert("accept", HeaderValue::from_static("application/json, text/event-stream"));
        for (k, v) in headers {
            if let (Ok(name), Ok(val)) = (HeaderName::from_str(k), HeaderValue::from_str(v)) {
                header_map.insert(name, val);
            } else {
                warn!("⚠️ HTTP MCP '{}': skipping invalid header '{}'", guild_name, k);
            }
        }

        let tool_timeout = Duration::from_millis(timeout_ms);
        let client = reqwest::Client::builder()
            .default_headers(header_map)
            .timeout(tool_timeout * 3)
            .build()
            .context("Failed to build HTTP client for external MCP")?;

        // Normalize endpoint: append /messages if not already present
        // Enhanced for n8n: don't append if it contains mcp-server or mcp-session
        let endpoint = {
            let base = base_url.trim_end_matches('/');
            if base.ends_with("/messages") || base.ends_with("/mcp") || base.contains("/mcp-server/") {
                base.to_string()
            } else {
                format!("{}/messages", base)
            }
        };

        let request_id = AtomicU32::new(1);

        // ── Initialize handshake ──
        info!("🌐 HttpMcpProxy: connecting to '{}' → {}", guild_name, endpoint);
        let init_id = request_id.fetch_add(1, Ordering::SeqCst);
        let init_body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "initialize",
            "id": init_id,
            "params": {
                "protocolVersion": "2024-11-05",
                "clientInfo": { "name": "tylluan-nexus", "version": "3.0.0" },
                "capabilities": { "tools": {} }
            }
        });

        let init_resp = tokio::time::timeout(
            tool_timeout,
            client.post(&endpoint).json(&init_body).send(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Initialize timeout for HTTP MCP '{}'", guild_name))??;

        let session_id = init_resp
            .headers()
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let is_sse = init_resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.contains("text/event-stream"))
            .unwrap_or(false);

        let init_json: serde_json::Value = if is_sse {
            // Read first chunk and extract 'data: {JSON}'
            use futures_util::StreamExt;
            let mut stream = init_resp.bytes_stream();
            let mut first_event = String::new();
            if let Some(chunk_result) = stream.next().await {
                let chunk = chunk_result.map_err(|e| anyhow::anyhow!("SSE stream error: {}", e))?;
                first_event = String::from_utf8_lossy(&chunk).to_string();
            }
            
            // Standard SSE format: data: {...}\n\n
            let json_str = first_event
                .lines()
                .find(|l| l.starts_with("data: "))
                .map(|l| &l[6..])
                .ok_or_else(|| anyhow::anyhow!("Missing 'data:' in SSE init response for '{}'. Raw: {}", guild_name, first_event))?;
            
            serde_json::from_str(json_str).map_err(|e| {
                anyhow::anyhow!("SSE JSON parse error for '{}': {} (raw: {})", guild_name, e, json_str)
            })?
        } else {
            init_resp.json().await.map_err(|e| {
                anyhow::anyhow!("Initialize response parse error for '{}': {}", guild_name, e)
            })?
        };

        if let Some(err) = init_json.get("error") {
            return Err(anyhow::anyhow!(
                "MCP initialize error from '{}': {}",
                guild_name,
                err
            ));
        }

        // ── Send initialized notification (no response expected) ──
        let notif_body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        });
        let mut notif_req = client.post(&endpoint).json(&notif_body);
        if let Some(sid) = &session_id {
            notif_req = notif_req.header("mcp-session-id", sid);
        }
        let _ = notif_req.send().await;

        let proxy = Self {
            client,
            endpoint,
            guild_name: guild_name.to_string(),
            session_id,
            tool_call_timeout: tool_timeout,
            request_id,
        };

        // ── Discover tools ──
        let tools = proxy.list_tools().await?;
        info!("📦 HTTP guild '{}' connected: {} tools", guild_name, tools.len());

        Ok((proxy, tools))
    }

    /// Send a JSON-RPC request and return the `result` field.
    async fn jsonrpc(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "id": id,
            "params": params
        });

        let mut req = self.client.post(&self.endpoint).json(&body);
        if let Some(sid) = &self.session_id {
            req = req.header("mcp-session-id", sid);
        }

        let resp = tokio::time::timeout(self.tool_call_timeout, req.send())
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "Timeout calling '{}' on HTTP MCP '{}'",
                    method,
                    self.guild_name
                )
            })??;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!(
                "HTTP {} calling '{}' on '{}'",
                resp.status(),
                method,
                self.guild_name
            ));
        }

        let is_sse = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.contains("text/event-stream"))
            .unwrap_or(false);

        let json: serde_json::Value = if is_sse {
            use futures_util::StreamExt;
            let mut stream = resp.bytes_stream();
            let mut first_event = String::new();
            if let Some(chunk_result) = stream.next().await {
                let chunk = chunk_result.map_err(|e| anyhow::anyhow!("SSE stream error during '{}': {}", method, e))?;
                first_event = String::from_utf8_lossy(&chunk).to_string();
            }
            
            let json_str = first_event
                .lines()
                .find(|l| l.starts_with("data: "))
                .map(|l| &l[6..])
                .ok_or_else(|| anyhow::anyhow!("Missing 'data:' in SSE response for '{}' call to '{}'. Raw: {}", method, self.guild_name, first_event))?;
            
            serde_json::from_str(json_str).map_err(|e| {
                anyhow::anyhow!("SSE JSON parse error for '{}' call to '{}': {} (raw: {})", method, self.guild_name, e, json_str)
            })?
        } else {
            resp.json().await.map_err(|e| {
                anyhow::anyhow!(
                    "Response parse error calling '{}' on '{}': {}",
                    method,
                    self.guild_name,
                    e
                )
            })?
        };

        if let Some(err) = json.get("error") {
            return Err(anyhow::anyhow!(
                "MCP error calling '{}' on '{}': {}",
                method,
                self.guild_name,
                err
            ));
        }

        Ok(json["result"].clone())
    }

    pub async fn list_tools(&self) -> Result<Vec<Tool>> {
        let result = self.jsonrpc("tools/list", serde_json::json!({})).await?;
        let tools_val = result.get("tools").ok_or_else(|| {
            anyhow::anyhow!("tools/list response missing 'tools' for '{}'", self.guild_name)
        })?;
        serde_json::from_value::<Vec<Tool>>(tools_val.clone()).map_err(|e| {
            anyhow::anyhow!("Tool parse error for '{}': {}", self.guild_name, e)
        })
    }

    pub async fn call_tool(&self, params: CallToolRequestParam) -> Result<CallToolResult> {
        let result = self
            .jsonrpc(
                "tools/call",
                serde_json::json!({
                    "name": params.name,
                    "arguments": params.arguments.unwrap_or_default()
                }),
            )
            .await?;

        // Try direct deserialization first (handles camelCase isError etc.)
        if let Ok(r) = serde_json::from_value::<CallToolResult>(result.clone()) {
            return Ok(r);
        }

        // Fallback: manual construction
        let is_error = result
            .get("isError")
            .or_else(|| result.get("is_error"))
            .and_then(|v| v.as_bool());
        let content: Vec<Content> = result
            .get("content")
            .and_then(|c| serde_json::from_value(c.clone()).ok())
            .unwrap_or_else(|| vec![Content::text(result.to_string())]);

        Ok(CallToolResult { content, is_error })
    }
}

// ─── CLASSIC SSE MCP CLIENT ──────────────────────────────────────────────────
//
// Classic SSE MCP lifecycle:
//   1. GET  {sse_url}  → persistent event stream (server sends JSON-RPC responses)
//   2. POST {post_url}?sessionId=XXX → send requests to the server
//
// This is fundamentally different from HTTP Streamable (request/response).
// A background task owns the stream; call_tool() uses a oneshot channel
// keyed by JSON-RPC id to correlate requests with responses.

/// Pending call registry: maps JSON-RPC request id → oneshot sender.
type PendingCalls = Arc<Mutex<HashMap<u32, oneshot::Sender<serde_json::Value>>>>;

pub struct SseMcpProxy {
    client: reqwest::Client,
    /// POST endpoint for sending requests (e.g. "http://host/messages")
    post_url: String,
    guild_name: String,
    /// Session id extracted from the SSE endpoint on connect
    session_id: String,
    tool_call_timeout: Duration,
    request_id: AtomicU32,
    /// In-flight calls waiting for a response event from the stream
    pending: PendingCalls,
    /// Shutdown signal for the background stream listener
    _shutdown_tx: tokio::sync::watch::Sender<bool>,
}

impl SseMcpProxy {
    /// Connect to a Classic SSE MCP server:
    ///   1. Open GET {sse_url} → receive the `endpoint` event with session URL
    ///   2. Perform initialize handshake via POST
    ///   3. Spawn background listener task
    ///   4. Return (proxy, discovered_tools)
    pub async fn connect(
        guild_name: &str,
        sse_url: &str,
        post_url: &str,
        headers: &HashMap<String, String>,
        timeout_ms: u64,
    ) -> Result<(Self, Vec<Tool>)> {
        use futures_util::StreamExt;
        use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
        use std::str::FromStr;

        let tool_timeout = Duration::from_millis(timeout_ms);

        let mut header_map = HeaderMap::new();
        header_map.insert("accept", HeaderValue::from_static("text/event-stream"));
        for (k, v) in headers {
            if let (Ok(name), Ok(val)) = (HeaderName::from_str(k), HeaderValue::from_str(v)) {
                header_map.insert(name, val);
            }
        }

        let client = reqwest::Client::builder()
            .default_headers(header_map.clone())
            .timeout(Duration::from_secs(300)) // long timeout for persistent stream
            .build()
            .context("Failed to build SSE HTTP client")?;

        info!("🔌 SseMcpProxy '{}': opening SSE stream → {}", guild_name, sse_url);

        // Open the SSE stream — server sends an `endpoint` event first
        let sse_resp = tokio::time::timeout(tool_timeout, client.get(sse_url).send())
            .await
            .map_err(|_| anyhow::anyhow!("SSE connect timeout for '{}'", guild_name))??;

        if !sse_resp.status().is_success() {
            return Err(anyhow::anyhow!(
                "SSE connect HTTP {} for '{}'", sse_resp.status(), guild_name
            ));
        }

        // Extract session_id from the first `endpoint` event
        // Format: event: endpoint\ndata: /messages?sessionId=XXXX\n\n
        let mut stream = sse_resp.bytes_stream();
        let mut accumulated = String::new();

        let session_id: String = {
            let parse_deadline = tokio::time::Instant::now() + tool_timeout;
            loop {
                if tokio::time::Instant::now() > parse_deadline {
                    return Err(anyhow::anyhow!(
                        "Timeout waiting for SSE endpoint event from '{}'", guild_name
                    ));
                }
                match stream.next().await {
                    Some(Ok(chunk)) => {
                        accumulated.push_str(&String::from_utf8_lossy(&chunk));
                        if let Some(sid) = Self::extract_session_id(&accumulated) {
                            break sid;
                        }
                    }
                    Some(Err(e)) => return Err(anyhow::anyhow!("SSE stream error for '{}': {}", guild_name, e)),
                    None => return Err(anyhow::anyhow!("SSE stream closed before endpoint event for '{}'", guild_name)),
                }
            }
        };

        info!("🔌 SseMcpProxy '{}': session_id = {}", guild_name, session_id);

        let pending: PendingCalls = Arc::new(Mutex::new(HashMap::new()));
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        // Spawn the background listener — takes ownership of the SSE stream
        let pending_clone = pending.clone();
        let guild_name_clone = guild_name.to_string();
        let sse_url_clone = sse_url.to_string();
        let headers_clone = headers.clone();
        let timeout_ms_clone = timeout_ms;
        tokio::spawn(Self::stream_listener(
            guild_name_clone,
            sse_url_clone,
            headers_clone,
            timeout_ms_clone,
            pending_clone,
            shutdown_rx,
            accumulated, // pass buffered data already read
        ));

        // Build a client without the SSE Accept header for POST requests
        let mut post_header_map = reqwest::header::HeaderMap::new();
        post_header_map.insert("content-type", HeaderValue::from_static("application/json"));
        post_header_map.insert("accept", HeaderValue::from_static("application/json"));
        for (k, v) in headers {
            if let (Ok(name), Ok(val)) = (
                HeaderName::from_str(k),
                HeaderValue::from_str(v),
            ) {
                post_header_map.insert(name, val);
            }
        }
        let post_client = reqwest::Client::builder()
            .default_headers(post_header_map)
            .timeout(tool_timeout * 3)
            .build()
            .context("Failed to build POST client for SSE MCP")?;

        let proxy = Self {
            client: post_client,
            post_url: post_url.to_string(),
            guild_name: guild_name.to_string(),
            session_id: session_id.clone(),
            tool_call_timeout: tool_timeout,
            request_id: AtomicU32::new(1),
            pending,
            _shutdown_tx: shutdown_tx,
        };

        // Initialize handshake
        let tools = proxy.handshake_and_list_tools().await?;
        info!("📦 SSE guild '{}' connected: {} tools", guild_name, tools.len());

        Ok((proxy, tools))
    }

    /// Parse SSE buffer for an `endpoint` event and extract the sessionId.
    fn extract_session_id(buf: &str) -> Option<String> {
        // SSE events are separated by blank lines.
        // We look for an event block containing "event: endpoint" and a "data:" line.
        let mut current_event_type = String::new();
        let mut current_data = String::new();

        for line in buf.lines() {
            if line.is_empty() {
                // End of event block
                if current_event_type == "endpoint" && !current_data.is_empty() {
                    // data may be "/messages?sessionId=XXX" or a full URL
                    if let Some(sid) = current_data
                        .split("sessionId=")
                        .nth(1)
                        .map(|s| s.split('&').next().unwrap_or(s).trim().to_string())
                        && !sid.is_empty() {
                            return Some(sid);
                        }
                }
                current_event_type.clear();
                current_data.clear();
            } else if let Some(val) = line.strip_prefix("event: ") {
                current_event_type = val.trim().to_string();
            } else if let Some(val) = line.strip_prefix("data: ") {
                current_data = val.trim().to_string();
            }
        }
        None
    }

    /// Background task: reads the SSE stream and dispatches responses to pending callers.
    /// Reconnects with exponential backoff if the stream drops.
    async fn stream_listener(
        guild_name: String,
        sse_url: String,
        headers: HashMap<String, String>,
        timeout_ms: u64,
        pending: PendingCalls,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
        buffered: String,
    ) {
        use futures_util::StreamExt;
        use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
        use std::str::FromStr;

        let mut backoff_ms = 500u64;
        let mut last_event_id: Option<String> = None;

        // Process buffered data already read during connect
        Self::process_sse_buffer(&buffered, &pending, &mut last_event_id).await;

        loop {
            if *shutdown_rx.borrow() {
                info!("🛑 SSE listener '{}': shutdown signal received", guild_name);
                return;
            }

            let mut hdr_map = HeaderMap::new();
            hdr_map.insert("accept", HeaderValue::from_static("text/event-stream"));
            if let Some(ref id) = last_event_id
                && let Ok(val) = HeaderValue::from_str(id) {
                    hdr_map.insert("last-event-id", val);
                }
            for (k, v) in &headers {
                if let (Ok(name), Ok(val)) = (HeaderName::from_str(k), HeaderValue::from_str(v)) {
                    hdr_map.insert(name, val);
                }
            }

            let client = match reqwest::Client::builder()
                .default_headers(hdr_map)
                .timeout(Duration::from_secs(300))
                .build()
            {
                Ok(c) => c,
                Err(e) => {
                    error!("SSE listener '{}': failed to build client: {}", guild_name, e);
                    return;
                }
            };

            debug!("SSE listener '{}': (re)connecting to {}", guild_name, sse_url);
            let resp = match tokio::time::timeout(
                Duration::from_millis(timeout_ms),
                client.get(&sse_url).send(),
            )
            .await
            {
                Ok(Ok(r)) if r.status().is_success() => r,
                Ok(Ok(r)) => {
                    warn!("SSE listener '{}': server returned {}", guild_name, r.status());
                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                    backoff_ms = (backoff_ms * 2).min(30_000);
                    continue;
                }
                Ok(Err(e)) => {
                    warn!("SSE listener '{}': connect error: {}", guild_name, e);
                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                    backoff_ms = (backoff_ms * 2).min(30_000);
                    continue;
                }
                Err(_) => {
                    warn!("SSE listener '{}': connect timeout", guild_name);
                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                    backoff_ms = (backoff_ms * 2).min(30_000);
                    continue;
                }
            };

            backoff_ms = 500; // reset on successful connect
            let mut stream = resp.bytes_stream();
            let mut buf = String::new();

            loop {
                tokio::select! {
                    chunk = stream.next() => {
                        match chunk {
                            Some(Ok(bytes)) => {
                                buf.push_str(&String::from_utf8_lossy(&bytes));
                                Self::process_sse_buffer(&buf, &pending, &mut last_event_id).await;
                                // Keep only the last incomplete event in the buffer
                                if let Some(pos) = buf.rfind("\n\n") {
                                    buf.drain(..pos + 2);
                                }
                            }
                            Some(Err(e)) => {
                                warn!("SSE listener '{}': stream error: {}", guild_name, e);
                                break;
                            }
                            None => {
                                debug!("SSE listener '{}': stream ended, reconnecting", guild_name);
                                break;
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            info!("🛑 SSE listener '{}': shutdown", guild_name);
                            return;
                        }
                    }
                }
            }

            tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            backoff_ms = (backoff_ms * 2).min(30_000);
        }
    }

    /// Parse complete SSE events from `buf` and wake pending callers.
    async fn process_sse_buffer(
        buf: &str,
        pending: &PendingCalls,
        last_event_id: &mut Option<String>,
    ) {
        let mut event_type = String::new();
        let mut data = String::new();
        let mut id_seen: Option<String> = None;

        for line in buf.lines() {
            if line.is_empty() {
                // Complete event
                if !data.is_empty()
                    && let Ok(json) = serde_json::from_str::<serde_json::Value>(&data)
                        && let Some(id_val) = json.get("id").and_then(|v| v.as_u64()) {
                            let mut map = pending.lock().await;
                            if let Some(tx) = map.remove(&(id_val as u32)) {
                                let _ = tx.send(json);
                            }
                        }
                if let Some(id) = id_seen.take() {
                    *last_event_id = Some(id);
                }
                event_type.clear();
                data.clear();
            } else if let Some(val) = line.strip_prefix("id: ") {
                id_seen = Some(val.trim().to_string());
            } else if let Some(val) = line.strip_prefix("event: ") {
                event_type = val.trim().to_string();
            } else if let Some(val) = line.strip_prefix("data: ") {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(val.trim());
            }
            let _ = event_type.as_str(); // suppress unused warning
        }
    }

    /// Send JSON-RPC request via POST and await the response from the SSE stream.
    async fn jsonrpc(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "id": id,
            "params": params
        });

        let (tx, rx) = oneshot::channel();
        {
            let mut map = self.pending.lock().await;
            map.insert(id, tx);
        }

        let post_url = format!("{}?sessionId={}", self.post_url, self.session_id);
        let send_result = tokio::time::timeout(
            self.tool_call_timeout,
            self.client.post(&post_url).json(&body).send(),
        )
        .await;

        match send_result {
            Ok(Ok(resp)) if resp.status().is_success() => {}
            Ok(Ok(resp)) => {
                let mut map = self.pending.lock().await;
                map.remove(&id);
                return Err(anyhow::anyhow!(
                    "SSE POST HTTP {} for method '{}' on '{}'",
                    resp.status(), method, self.guild_name
                ));
            }
            Ok(Err(e)) => {
                let mut map = self.pending.lock().await;
                map.remove(&id);
                return Err(anyhow::anyhow!(
                    "SSE POST error for method '{}' on '{}': {}", method, self.guild_name, e
                ));
            }
            Err(_) => {
                let mut map = self.pending.lock().await;
                map.remove(&id);
                return Err(anyhow::anyhow!(
                    "SSE POST timeout for method '{}' on '{}'", method, self.guild_name
                ));
            }
        }

        // Wait for the response to arrive on the SSE stream
        let json = tokio::time::timeout(self.tool_call_timeout, rx)
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "SSE response timeout for method '{}' on '{}'", method, self.guild_name
                )
            })?
            .map_err(|_| anyhow::anyhow!("SSE listener dropped before response for '{}'", self.guild_name))?;

        if let Some(err) = json.get("error") {
            return Err(anyhow::anyhow!(
                "MCP error calling '{}' on '{}': {}", method, self.guild_name, err
            ));
        }

        Ok(json["result"].clone())
    }

    async fn handshake_and_list_tools(&self) -> Result<Vec<Tool>> {
        // Initialize
        let _init = self.jsonrpc("initialize", serde_json::json!({
            "protocolVersion": "2024-11-05",
            "clientInfo": { "name": "tylluan-nexus", "version": "3.0.0" },
            "capabilities": { "tools": {} }
        })).await?;

        // notifications/initialized (no response expected)
        let notif_url = format!("{}?sessionId={}", self.post_url, self.session_id);
        let _ = self.client.post(&notif_url)
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized",
                "params": {}
            }))
            .send()
            .await;

        self.list_tools().await
    }

    pub async fn list_tools(&self) -> Result<Vec<Tool>> {
        let result = self.jsonrpc("tools/list", serde_json::json!({})).await?;
        let tools_val = result.get("tools").ok_or_else(|| {
            anyhow::anyhow!("tools/list missing 'tools' for '{}'", self.guild_name)
        })?;
        serde_json::from_value::<Vec<Tool>>(tools_val.clone())
            .map_err(|e| anyhow::anyhow!("Tool parse error for '{}': {}", self.guild_name, e))
    }

    pub async fn call_tool(&self, params: CallToolRequestParam) -> Result<CallToolResult> {
        let result = self.jsonrpc(
            "tools/call",
            serde_json::json!({
                "name": params.name,
                "arguments": params.arguments.unwrap_or_default()
            }),
        ).await?;

        if let Ok(r) = serde_json::from_value::<CallToolResult>(result.clone()) {
            return Ok(r);
        }

        let is_error = result.get("isError").or_else(|| result.get("is_error")).and_then(|v| v.as_bool());
        let content: Vec<Content> = result
            .get("content")
            .and_then(|c| serde_json::from_value(c.clone()).ok())
            .unwrap_or_else(|| vec![Content::text(result.to_string())]);

        Ok(CallToolResult { content, is_error })
    }

    /// Signal the background listener to stop.
    pub fn shutdown_signal(&self) {
        let _ = self._shutdown_tx.send(true);
    }
}

// ─── UNIFIED PROXY ────────────────────────────────────────────────────────────

/// Wraps either a stdio, HTTP Streamable, or Classic SSE MCP connection.
pub enum ProxyKind {
    Stdio(McpProxy),
    Http(HttpMcpProxy),
    Sse(SseMcpProxy),
}

impl ProxyKind {
    pub async fn list_tools(&self) -> Result<Vec<Tool>> {
        match self {
            ProxyKind::Stdio(p) => p.list_tools().await,
            ProxyKind::Http(p) => p.list_tools().await,
            ProxyKind::Sse(p) => p.list_tools().await,
        }
    }

    pub async fn call_tool(&self, params: CallToolRequestParam) -> Result<CallToolResult> {
        match self {
            ProxyKind::Stdio(p) => p.call_tool(params).await,
            ProxyKind::Http(p) => p.call_tool(params).await,
            ProxyKind::Sse(p) => p.call_tool(params).await,
        }
    }

    /// Graceful shutdown.
    pub async fn shutdown(self) -> Result<()> {
        match self {
            ProxyKind::Stdio(p) => p.shutdown().await,
            ProxyKind::Http(_) => Ok(()),
            ProxyKind::Sse(p) => {
                p.shutdown_signal();
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_result() {
        let result = error_result("something went wrong");
        assert_eq!(result.is_error, Some(true));
        assert_eq!(result.content.len(), 1);
    }
}
