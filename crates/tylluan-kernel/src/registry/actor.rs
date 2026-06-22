use crate::registry::guild_process::{GuildRegistry, GuildStatus, GuildCallStats};
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
                RegistryMessage::CallTool { guild_name, params, resp } => {
                    let registry = Arc::clone(&self.registry);
                    tokio::spawn(async move {
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
                                    let _ = resp.send(Err(anyhow::anyhow!("Guild '{}' not found", guild_name)));
                                    return;
                                }
                                if attempt > 0
                                    && let Some(guild) = reg.guilds.get_mut(&guild_name) {
                                        tracing::warn!("🛑 [Retry] Killing guild '{}' for fresh spawn", guild_name);
                                        let _ = guild.kill().await;
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
                                .map_err(|_| anyhow::anyhow!("Guild '{}' semaphore closed", guild_name));

                            let call_start = std::time::Instant::now();
                            let call_result = match permit {
                                Ok(_permit) => {
                                    let call_fut = proxy.call_tool(params.clone());
                                    if tool_timeout.is_some() {
                                        match tokio::time::timeout(timeout_dur, call_fut).await {
                                            Ok(Ok(res)) => res,
                                            Ok(Err(e)) => {
                                                crate::registry::proxy::error_result(&format!("GUILD_ERROR|{}|{}", guild_name, e))
                                            }
                                            Err(_) => {
                                                crate::registry::proxy::error_result(&format!("GUILD_TIMEOUT|{}|{}s", guild_name, timeout_secs))
                                            }
                                        }
                                    } else {
                                        tracing::info!(
                                            "⚡ [Actor] Guild '{}' tool '{}' — CPU inference mode, no timeout",
                                            guild_name, params.name
                                        );
                                        match call_fut.await {
                                            Ok(res) => res,
                                            Err(e) => {
                                                crate::registry::proxy::error_result(&format!("GUILD_ERROR|{}|{}", guild_name, e))
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    crate::registry::proxy::error_result(&format!("Guild '{}' semaphore error: {}", guild_name, e))
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

                            // Step 5: Success or decide whether to retry.
                            // Only retry on TIMEOUT (process alive but slow) — a genuine
                            // transport error (disconnected/crash) won't improve with a
                            // kill+respawn at the caller level; let the supervisor handle it.
                            let call_str = serde_json::to_string(&call_result).unwrap_or_default();
                            let is_success = !call_result.is_error.unwrap_or(false)
                                && !call_str.contains("GUILD_TIMEOUT")
                                && !call_str.contains("GUILD_ERROR");
                            let is_timeout = call_str.contains("GUILD_TIMEOUT");

                            if is_success {
                                final_result = Some(Ok(call_result));
                                break;
                            }

                            if !is_timeout {
                                // Crash / transport error — return immediately, no retry.
                                tracing::warn!(
                                    "⚠️ [Actor] Guild '{}' tool call returned error on attempt {} — not retrying crash: {:?}",
                                    guild_name, attempt + 1, call_result
                                );
                                final_result = Some(Ok(call_result));
                                break;
                            }

                            // Timeout — kill+respawn and retry with more patience.
                            tracing::warn!(
                                "⏳ [Actor] Guild '{}' timed out on attempt {}/{} ({}s) — respawning and retrying",
                                guild_name, attempt + 1, timeouts_secs.len(), timeout_secs
                            );
                            final_result = Some(Ok(call_result));
                            attempt += 1;
                        }

                        let _ = resp.send(final_result.unwrap_or_else(|| Err(anyhow::anyhow!("Guild '{}' call failed after all retries", guild_name))));
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
                        Err(anyhow::anyhow!("Guild '{}' not found", name))
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
                        Err(anyhow::anyhow!("Guild '{}' not found", name))
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
        let (resp_tx, resp_rx) = oneshot::channel();
        self.sender.send(RegistryMessage::CallTool {
            guild_name: guild_name.to_string(),
            params,
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
