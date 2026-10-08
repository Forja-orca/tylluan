use crate::registry::guild_process::{GuildRegistry, GuildStatus, GuildCallStats, GuardedKill};
use anyhow::Result;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::info;

/// Mensajes que el RegistryActor puede procesar
pub enum RegistryMessage {
    Register {
        name: String,
        module_path: String,
        always_on: bool,
        timeout_ms: Option<u64>,
        resp: oneshot::Sender<()>,
    },
    EnsureRunning {
        name: String,
        resp: oneshot::Sender<Result<()>>,
    },
    CallTool {
        guild_name: String,
        params: rmcp::model::CallToolRequestParam,
        requested_by: Option<String>,
        resp: oneshot::Sender<Result<rmcp::model::CallToolResult>>,
    },
    GetTools {
        resp: oneshot::Sender<rmcp::model::Tool>,
    },
    StatusAll {
        resp: oneshot::Sender<Vec<GuildStatus>>,
    },
    FindGuildForTool {
        tool_name: String,
        resp: oneshot::Sender<Option<String>>,
    },
    GetGuildStats {
        resp: oneshot::Sender<(usize, usize)>,
    },
    ListGuilds {
        query: Option<String>,
        resp: oneshot::Sender<Vec<serde_json::Value>>,
    },
    KillGuild {
        name: String,
        resp: oneshot::Sender<Result<()>>,
    },
    ReapIdle,
    ResetBackoff {
        name: String,
        resp: oneshot::Sender<Result<()>>,
    },
    Shutdown {
        resp: oneshot::Sender<()>,
    },
}

pub struct RegistryActor {
    receiver: mpsc::Receiver<RegistryMessage>,
    registry: Arc<RwLock<GuildRegistry>>,
}

/// A transport-level failure (child process died / stdio closed) vs a business
/// error returned by the guild's tool. Only transport failures warrant killing
/// the dead proxy and respawning — the process is gone either way.
fn is_transport_failure(call_str: &str) -> bool {
    call_str.contains("Transport") || call_str.contains("disconnected")
}

/// Structured classification of one retry-loop attempt, captured where the
/// result is constructed instead of re-parsed from the serialized JSON.
/// T981: sniffing `call_str.contains("GUILD_TIMEOUT" | "disconnected")` let a
/// legitimate tool output that happened to contain those literals steer the
/// loop into killing + re-running a call that had already succeeded (double
/// side effects), or into treating a business error as a dead transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AttemptOutcome {
    /// `Ok` result with `is_error` unset/false — the attempt is done.
    Success,
    /// Our own deadline fired (GUILD_TIMEOUT built below) — respawn + retry.
    Timeout,
    /// Proxy reported a dead transport — kill (if idle) + retry.
    TransportFailure,
    /// Business/other error — return immediately, no retry.
    TerminalError,
}

/// Classify a raw tool result BEFORE any marker string is formatted from it:
/// only `is_error` is authoritative for a result the guild actually returned.
fn classify_ok(res: &rmcp::model::CallToolResult) -> AttemptOutcome {
    if res.is_error.unwrap_or(false) {
        AttemptOutcome::TerminalError
    } else {
        AttemptOutcome::Success
    }
}

/// Classify a proxy-level error, scoped to the error text itself — the only
/// text that can legitimately describe the transport.
fn classify_err(err_text: &str) -> AttemptOutcome {
    if is_transport_failure(err_text) {
        AttemptOutcome::TransportFailure
    } else {
        AttemptOutcome::TerminalError
    }
}

impl RegistryActor {
    /// Create the actor + handle pair. The Arc<RwLock<GuildRegistry>> is shared:
    /// the actor serializes mutations through messages, but the same Arc can
    /// also be held by TylluanServer for legacy direct-access patterns.
    pub fn new(registry: Arc<RwLock<GuildRegistry>>) -> (Self, RegistryHandle) {
        let (sender, receiver) = mpsc::channel(100);
        let actor = Self { receiver, registry: registry.clone() };
        let handle = RegistryHandle::new(sender, registry);
        (actor, handle)
    }

    pub async fn run(mut self) {
        info!("🎭 Registry Actor started");
        while let Some(msg) = self.receiver.recv().await {
            match msg {
                RegistryMessage::Register { name, module_path, always_on, timeout_ms, resp } => {
                    self.registry.write().await.register(&name, &module_path, always_on, timeout_ms);
                    let _ = resp.send(());
                }
                RegistryMessage::EnsureRunning { name, resp } => {
                    let result = self.registry.write().await.ensure_guild_running(&name).await;
                    let _ = resp.send(result);
                }
                RegistryMessage::CallTool { guild_name, params, requested_by, resp } => {
                    let registry = Arc::clone(&self.registry);
                    tokio::spawn(async move {
                        let call_started_unix = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        let timeouts_secs = [30, 60, 120, 180];
                        let mut attempt = 0;
                        let mut final_result = None;

                        while attempt < timeouts_secs.len() {
                            let timeout_secs = timeouts_secs[attempt];
                            let timeout_dur = std::time::Duration::from_secs(timeout_secs);

                            // Step 1: Ensure guild is running — lazy guilds start on first demand
                            {
                                let mut reg = registry.write().await;
                                if !reg.guilds.contains_key(&guild_name) {
                                    let _ = resp.send(Err(anyhow::anyhow!("Guild '{guild_name}' not found")));
                                    return;
                                }
                                if attempt > 0 {
                                    // Fresh spawn for this retry — but only when no
                                    // other caller is mid-call: kill() yanks the
                                    // proxy out from under concurrent callers (T981).
                                    match reg.kill_guild_if_idle(&guild_name).await {
                                        GuardedKill::Killed => tracing::warn!(
                                            "🛑 [Retry] Killing guild '{}' for fresh spawn",
                                            guild_name
                                        ),
                                        GuardedKill::InFlight => tracing::warn!(
                                            "⚠️ [Retry] Guild '{}' has callers in flight — kill skipped, retrying same process",
                                            guild_name
                                        ),
                                        GuardedKill::Missing => {}
                                    }
                                }
                                let needs_start = reg.guilds.get(&guild_name).map(|g| !g.is_running()).unwrap_or(false);
                                if needs_start {
                                    let ao = reg.guilds.get(&guild_name).map(|g| g.always_on).unwrap_or(false);
                                    tracing::info!("🚀 [{}] Starting guild '{}' on demand (attempt {})",
                                        if ao { "always-on" } else { "lazy" }, guild_name, attempt);
                                    let _ = reg.ensure_guild_running(&guild_name).await;
                                }
                            }
                            // For lazy guilds just started, yield briefly so MCP handshake can complete
                            // The retry loop handles the case where it's still not ready
                            tokio::task::yield_now().await;

                            // Step 2: Brief read lock to clone proxy + semaphore + tool_timeout
                            let (proxy, semaphore, tool_timeout) = {
                                let reg = registry.read().await;
                                if let Some(guild) = reg.guilds.get(&guild_name) {
                                    (guild.get_proxy(), Some(guild.get_semaphore()), guild.tool_timeout)
                                } else {
                                    (None, None, None)
                                }
                            };

                            let (proxy, semaphore) = match (proxy, semaphore) {
                                (Some(p), Some(s)) => (p, s),
                                _ => {
                                    tracing::error!("❌ [Retry Loop] Guild '{}' proxy or semaphore missing", guild_name);
                                    attempt += 1;
                                    continue;
                                }
                            };

                            if tool_timeout.is_none() {
                                tracing::info!(
                                    "🔄 [Retry Loop] Guild '{}' tool call attempt {}/{} (CPU inference, unlimited timeout)",
                                    guild_name, attempt + 1, timeouts_secs.len()
                                );
                             } else {
                                 tracing::info!(
                                     "🔄 [Retry Loop] Guild '{}' tool call attempt {}/{} with {}s timeout",
                                     guild_name, attempt + 1, timeouts_secs.len(), timeout_secs
                                 );
                             }

                            // Step 3: Execute tool call outside of any lock!
                            let permit = semaphore.acquire()
                                .await
                                .map_err(|_| anyhow::anyhow!("Guild '{guild_name}' semaphore closed"));

                            let call_start = std::time::Instant::now();
                            // Outcome is assigned on EVERY arm below — the
                            // compiler enforces it, so no arm can forget to
                            // classify its own result.
                            let outcome;
                            let call_result = match permit {
                                Ok(_permit) => {
                                    let call_fut = proxy.call_tool(params.clone());
                                    if tool_timeout.is_some() {
                                        match tokio::time::timeout(timeout_dur, call_fut).await {
                                            Ok(Ok(res)) => {
                                                outcome = classify_ok(&res);
                                                res
                                            }
                                            Ok(Err(e)) => {
                                                outcome = classify_err(&e.to_string());
                                                crate::registry::proxy::error_result(&format!("GUILD_ERROR|{guild_name}|{e}"))
                                            }
                                            Err(_) => {
                                                outcome = AttemptOutcome::Timeout;
                                                crate::registry::proxy::error_result(&format!("GUILD_TIMEOUT|{guild_name}|{timeout_secs}s"))
                                            }
                                        }
                                    } else {
                                        tracing::info!(
                                            "⚡ [Actor] Guild '{}' tool '{}' — CPU inference mode, no timeout",
                                            guild_name, params.name
                                        );
                                        match call_fut.await {
                                            Ok(res) => {
                                                outcome = classify_ok(&res);
                                                res
                                            }
                                            Err(e) => {
                                                outcome = classify_err(&e.to_string());
                                                crate::registry::proxy::error_result(&format!("GUILD_ERROR|{guild_name}|{e}"))
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    outcome = AttemptOutcome::TerminalError;
                                    crate::registry::proxy::error_result(&format!("Guild '{guild_name}' semaphore error: {e}"))
                                }
                            };
                            let latency = call_start.elapsed().as_millis() as u64;
                            let success = !call_result.is_error.unwrap_or(false);

                            // Step 4: Update performance counters briefly under read lock
                            {
                                let reg = registry.read().await;
                                if let Some(guild) = reg.guilds.get(&guild_name) {
                                    guild.perf_total_calls.fetch_add(1, Ordering::Relaxed);
                                    guild.perf_total_latency_ms.fetch_add(latency, Ordering::Relaxed);
                                    guild.perf_last_call_unix.store(
                                        std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap_or_default()
                                            .as_secs(),
                                        Ordering::Relaxed,
                                    );
                                    if success {
                                        guild.perf_successful_calls.fetch_add(1, Ordering::Relaxed);
                                    }
                                }
                            }

                            // Step 5: Success or decide whether to retry. The
                            // decision comes from the structured `outcome`
                            // captured while building the result — never from
                            // re-parsing the result's text (T981: substring
                            // sniffing over the serialized JSON let tool output
                            // containing "GUILD_TIMEOUT"/"disconnected" fake a
                            // timeout or a dead transport and re-run a call
                            // that had already succeeded).
                            match outcome {
                                AttemptOutcome::Success => {
                                    final_result = Some(Ok(call_result));
                                    break;
                                }
                                AttemptOutcome::TransportFailure => {
                                    // The child process died behind the kernel's back (external
                                    // kill, OOM, stdio closed): the proxy slot stays Some(dead),
                                    // is_running() keeps reporting true, and ensure_guild_running
                                    // fast-paths forever — every call hits "Transport disconnected".
                                    // The supervisor only respawns always_on guilds (supervisor.rs),
                                    // so for LAZY guilds this cleanup + retry is the only recovery
                                    // path. kill() also resets the T13 backoff since the death was
                                    // not a spawn crash. Lifecycle bug observed live 2026-09-13.
                                    // Guarded kill: never fire while another caller is mid-call.
                                    tracing::warn!(
                                        "🛑 [Actor] Guild '{}' transport failure — killing dead proxy so the next attempt respawns it fresh",
                                        guild_name
                                    );
                                    {
                                        let mut reg = registry.write().await;
                                        if let GuardedKill::InFlight = reg.kill_guild_if_idle(&guild_name).await {
                                            tracing::warn!(
                                                "⚠️ [Actor] Guild '{}' has a call in flight — kill skipped this attempt",
                                                guild_name
                                            );
                                        }
                                    }
                                    attempt += 1;
                                    continue;
                                }
                                AttemptOutcome::TerminalError => {
                                    // Crash / business error — return immediately, no retry.
                                    tracing::warn!(
                                        "⚠️ [Actor] Guild '{}' tool call returned error on attempt {} — not retrying crash: {:?}",
                                        guild_name, attempt + 1, call_result
                                    );
                                    final_result = Some(Ok(call_result));
                                    break;
                                }
                                AttemptOutcome::Timeout => {
                                    // Timeout — kill+respawn and retry with more patience.
                                    // The kill itself happens in Step 1 of the next
                                    // iteration, also behind the in-flight guard.
                                    tracing::warn!(
                                        "⏳ [Actor] Guild '{}' timed out on attempt {}/{} ({}s) — respawning and retrying",
                                        guild_name, attempt + 1, timeouts_secs.len(), timeout_secs
                                    );
                                    final_result = Some(Ok(call_result));
                                    attempt += 1;
                                }
                            }
                        }

                        // bwc-d0fb0812: index guild outputs (if any) after the
                        // call resolved — observation only, never breaks the
                        // call path. Hashing runs off the async runtime; owned
                        // strings move into the closure so the borrows stay
                        // inside it.
                        if let Some(Ok(ref res)) = final_result {
                            let call_ended_unix = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs();
                            let result_json = serde_json::to_string(res).unwrap_or_default();
                            let guild_owned = guild_name.clone();
                            let tool_owned = params.name.to_string();
                            let requested_owned = requested_by
                                .clone()
                                .unwrap_or_else(|| "unknown".to_string());
                            let success = !res.is_error.unwrap_or(false);
                            let store = crate::registry::outputs::OutputsStore::at_default_root();
                            // Detached on purpose: the response must not wait
                            // on hashing. Failure is logged inside index_call.
                            let _outputs_index_join = tokio::task::spawn_blocking(move || {
                                let record = crate::registry::outputs::CallRecord {
                                    guild: &guild_owned,
                                    tool: &tool_owned,
                                    requested_by: &requested_owned,
                                    success,
                                    started_unix: call_started_unix,
                                    ended_unix: call_ended_unix,
                                    result_json: &result_json,
                                    run_id_override: None,
                                };
                                store.index_call(&record);
                            });
                        }
                        let _ = resp.send(final_result.unwrap_or_else(|| Err(anyhow::anyhow!("Guild '{guild_name}' call failed after all retries"))));
                    });
                }
                RegistryMessage::StatusAll { resp } => {
                    let _ = resp.send(self.registry.read().await.status_all());
                }
                RegistryMessage::FindGuildForTool { tool_name, resp } => {
                    let result = self.registry.read().await.find_guild_for_tool(&tool_name).map(|s| s.to_string());
                    let _ = resp.send(result);
                }
                RegistryMessage::GetGuildStats { resp } => {
                    let reg = self.registry.read().await;
                    let total = reg.guilds.len();
                    let active = reg.guilds.values().filter(|g| g.is_running()).count();
                    let _ = resp.send((total, active));
                }
                RegistryMessage::ListGuilds { query, resp } => {
                    let query_lower = query.map(|q| q.to_lowercase());
                    let reg = self.registry.read().await;
                    let guilds: Vec<serde_json::Value> = reg.guilds.values()
                        .filter(|g| {
                            if let Some(ref q) = query_lower {
                                g.name.to_lowercase().contains(q)
                            } else {
                                true
                            }
                        })
                        .map(|g| {
                            serde_json::json!({
                                "name": g.name,
                                "description": g.description(),
                                "always_on": g.always_on,
                                "tool_count": g.tools.len(),
                                "running": g.is_running(),
                            })
                        })
                        .collect();
                    let _ = resp.send(guilds);
                }
                RegistryMessage::KillGuild { name, resp } => {
                    let mut reg = self.registry.write().await;
                    let result = if let Some(guild) = reg.guilds.get_mut(&name) {
                        guild.kill().await
                    } else {
                        Err(anyhow::anyhow!("Guild '{name}' not found"))
                    };
                    let _ = resp.send(result);
                }
                RegistryMessage::ReapIdle => {
                    self.registry.write().await.reap_idle_guilds().await;
                }
                RegistryMessage::ResetBackoff { name, resp } => {
                    let mut reg = self.registry.write().await;
                    let result = if let Some(guild) = reg.guilds.get_mut(&name) {
                        guild.crash_count = 0;
                        guild.last_crash_at = None;
                        let _ = reg.save();
                        info!("🔄 [T13] Backoff reset for guild '{}'", name);
                        Ok(())
                    } else {
                        Err(anyhow::anyhow!("Guild '{name}' not found"))
                    };
                    let _ = resp.send(result);
                }
                RegistryMessage::Shutdown { resp } => {
                    info!("🛑 Registry Actor shutting down");
                    let _ = resp.send(());
                    break;
                }
                _ => {}
            }
        }
    }
}

#[derive(Clone)]
pub struct RegistryHandle {
    sender: mpsc::Sender<RegistryMessage>,
    /// Shared reference to the underlying GuildRegistry.
    /// Exposed via `arc()` for legacy callers (e.g. TylluanServer, NomadManager)
    /// that still use `registry.read().await` direct-access patterns.
    arc: Arc<RwLock<GuildRegistry>>,
}

impl RegistryHandle {
    pub fn new(sender: mpsc::Sender<RegistryMessage>, arc: Arc<RwLock<GuildRegistry>>) -> Self {
        Self { sender, arc }
    }

    /// Returns the shared Arc<RwLock<GuildRegistry>> for legacy direct-access.
    /// Prefer the actor methods (call_tool, ensure_running, etc.) over locking.
    pub fn arc(&self) -> Arc<RwLock<GuildRegistry>> {
        self.arc.clone()
    }

    pub async fn ensure_running(&self, name: &str) -> Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.sender.send(RegistryMessage::EnsureRunning {
            name: name.to_string(),
            resp: resp_tx,
        }).await?;
        resp_rx.await?
    }

    pub async fn call_tool(&self, guild_name: &str, params: rmcp::model::CallToolRequestParam) -> Result<rmcp::model::CallToolResult> {
        self.call_tool_as(guild_name, params, None).await
    }

    /// call_tool attributed to `requested_by` in the outputs ledger
    /// (bwc-d0fb0812). `None` behaves exactly like the legacy call_tool.
    pub async fn call_tool_as(
        &self,
        guild_name: &str,
        params: rmcp::model::CallToolRequestParam,
        requested_by: Option<String>,
    ) -> Result<rmcp::model::CallToolResult> {
        tracing::info!(
            gen_ai.operation.name = "tool_call",
            gen_ai.request.model = %guild_name,
            tool_name = %params.arguments.as_ref().and_then(|a| a.get("name")).and_then(|v| v.as_str()).unwrap_or("unknown"),
            "Guild tool call dispatch"
        );
        let (resp_tx, resp_rx) = oneshot::channel();
        self.sender.send(RegistryMessage::CallTool {
            guild_name: guild_name.to_string(),
            params,
            requested_by,
            resp: resp_tx,
        }).await?;
        resp_rx.await?
    }

    pub async fn status_all(&self) -> Result<Vec<GuildStatus>> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.sender.send(RegistryMessage::StatusAll { resp: resp_tx }).await?;
        Ok(resp_rx.await?)
    }

    pub async fn find_guild_for_tool(&self, tool_name: &str) -> Result<Option<String>> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.sender.send(RegistryMessage::FindGuildForTool {
            tool_name: tool_name.to_string(),
            resp: resp_tx,
        }).await?;
        Ok(resp_rx.await?)
    }

    pub async fn guild_stats(&self) -> Result<(usize, usize)> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.sender.send(RegistryMessage::GetGuildStats { resp: resp_tx }).await?;
        Ok(resp_rx.await?)
    }

    pub async fn list_guilds(&self, query: Option<String>) -> Result<Vec<serde_json::Value>> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.sender.send(RegistryMessage::ListGuilds { query, resp: resp_tx }).await?;
        Ok(resp_rx.await?)
    }

    pub async fn kill_guild(&self, name: &str) -> Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.sender.send(RegistryMessage::KillGuild { name: name.to_string(), resp: resp_tx }).await?;
        resp_rx.await?
    }

    pub async fn reap_idle(&self) {
        let _ = self.sender.send(RegistryMessage::ReapIdle).await;
    }

    pub async fn register(&self, name: &str, module_path: &str, always_on: bool, timeout_ms: Option<u64>) {
        let (resp_tx, _resp_rx) = oneshot::channel();
        let _ = self.sender.send(RegistryMessage::Register {
            name: name.to_string(),
            module_path: module_path.to_string(),
            always_on,
            timeout_ms,
            resp: resp_tx,
        }).await;
    }

    pub async fn reset_backoff(&self, name: &str) -> Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.sender.send(RegistryMessage::ResetBackoff {
            name: name.to_string(),
            resp: resp_tx,
        }).await?;
        resp_rx.await?
    }

    pub async fn guild_call_stats(&self) -> Result<Vec<GuildCallStats>> {
        let arc = self.arc();
        let registry = arc.read().await;
        Ok(registry.guilds.values().map(|g| {
            let perf_total = g.perf_total_latency_ms.load(Ordering::Relaxed);
            let perf_success = g.perf_successful_calls.load(Ordering::Relaxed);
            let total = perf_success + (g.total_calls.saturating_sub(perf_success));
            let avg = if total > 0 {
                perf_total as f64 / total as f64
            } else { 0.0 };
            let success_rate = if total > 0 {
                perf_success as f64 / total as f64
            } else { 0.0 };
            GuildCallStats {
                guild_name: g.name.clone(),
                total_calls: total,
                successful_calls: perf_success,
                avg_latency_ms: avg,
                last_call_unix: g.perf_last_call_unix.load(Ordering::Relaxed),
                success_rate,
            }
        }).collect())
    }

    pub async fn compute_health_scores(&self) -> std::collections::HashMap<String, f64> {
        match self.guild_call_stats().await {
            Ok(stats) => stats.iter().map(|s| {
                let health = if s.total_calls == 0 {
                    0.7
                } else {
                    (s.successful_calls as f64 + 1.0) / (s.total_calls as f64 + 2.0)
                };
                (s.guild_name.clone(), health)
            }).collect(),
            Err(_) => std::collections::HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{classify_err, classify_ok, is_transport_failure, AttemptOutcome};
    use rmcp::model::{CallToolResult, Content};

    #[test]
    fn transport_failure_detection() {
        // Real error shape from the live 2026-09-13 incident: the proxy of a
        // guild killed externally, dispatch hits "Transport disconnected".
        assert!(is_transport_failure(
            "GUILD_ERROR|coloquio|Transport(Custom { kind: Other, error: \"disconnected\" })"
        ));
        assert!(is_transport_failure("GUILD_ERROR|bash|Transport disconnected"));
        // Business errors from the guild's tool must NOT trigger respawn.
        assert!(!is_transport_failure(
            "GUILD_ERROR|coordinator|requires argument(s): task"
        ));
        // Timeouts are handled by a different branch.
        assert!(!is_transport_failure("GUILD_TIMEOUT|coloquio|30s"));
    }

    #[test]
    fn classification_ignores_marker_substrings_in_tool_output() {
        // T981 regression: the retry loop used to re-parse the serialized
        // result for "GUILD_TIMEOUT"/"disconnected" — a successful tool output
        // containing those literals was classified as a timeout, killing and
        // re-running a call that had actually succeeded (double side effects).
        let echo = CallToolResult {
            content: vec![Content::text("GUILD_TIMEOUT|bash|30s — peer disconnected")],
            is_error: Some(false),
        };
        assert_eq!(classify_ok(&echo), AttemptOutcome::Success);

        // A genuine tool error stays terminal even if its text mentions the
        // transport — only proxy-level errors describe the transport.
        let business = CallToolResult {
            content: vec![Content::text(
                "GUILD_ERROR|bash|Transport probe ok, but requires argument(s): task",
            )],
            is_error: Some(true),
        };
        assert_eq!(classify_ok(&business), AttemptOutcome::TerminalError);
    }

    #[test]
    fn classification_scopes_transport_check_to_proxy_error_text() {
        assert_eq!(
            classify_err("Transport(Custom { kind: Other, error: \"disconnected\" })"),
            AttemptOutcome::TransportFailure
        );
        assert_eq!(
            classify_err("requires argument(s): task"),
            AttemptOutcome::TerminalError
        );
    }
}
