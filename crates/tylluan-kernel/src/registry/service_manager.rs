//! # Auxiliary Service Manager
//!
//! Manages non-MCP background processes (e.g., Browser, Cron runners, Proxies).
//! These services are defined in `tylluan.toml` under the `[services]` section.

use crate::config::ServiceConfig;
use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::time::timeout;
use tracing::{info, error, warn, debug};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Represents a running background service.
pub struct RunningService {
    pub name: String,
    pub config: ServiceConfig,
    pub child: Child,
}

/// Orchestrates background processes that operate alongside the MCP hub.
pub struct ServiceManager {
    /// Active services keyed by their name.
    services: Arc<RwLock<HashMap<String, RunningService>>>,
}

impl ServiceManager {
    /// Create a new ServiceManager.
    pub fn new() -> Self {
        Self {
            services: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Spawn all services configured as `always_on`.
    pub async fn spawn_configured_services(&self, configs: &HashMap<String, ServiceConfig>) -> Result<()> {
        for (name, config) in configs {
            if config.always_on
                && let Err(e) = self.spawn_service(name, config).await {
                    error!("Failed to spawn auxiliary service '{}': {}", name, e);
                }
        }
        Ok(())
    }

    /// Spawn a specific service by name.
    pub async fn spawn_service(&self, name: &str, config: &ServiceConfig) -> Result<()> {
        let mut services = self.services.write().await;
        
        if services.contains_key(name) {
            debug!("Service '{}' already running, skipping spawn", name);
            return Ok(());
        }

        let cmd_str = match &config.command {
            Some(c) => c,
            None => {
                if let Some(url) = &config.url {
                    info!("Service '{}' is a remote URL ({}), no process to spawn.", name, url);
                    return Ok(());
                }
                return Err(anyhow!("Service '{}' has no command or URL", name));
            }
        };

        info!("🚀 Spawning auxiliary service '{}': {}", name, cmd_str);

        let mut command = Command::new(cmd_str);
        
        if let Some(args) = &config.args {
            command.args(args);
        }

        if let Some(env_vars) = &config.env {
            command.envs(env_vars);
        }

        // Buffer stdout/stderr to avoid blocking
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        // Apply explicit timeout (default 25s if not configured)
        let timeout_ms = config.timeout_ms.unwrap_or(25000);
        let timeout_dur = Duration::from_millis(timeout_ms);

        // Spawn in blocking thread to avoid holding the reactor, then enforce timeout
        let spawn_fut = tokio::task::spawn_blocking(move || command.spawn());
        match timeout(timeout_dur, spawn_fut).await {
            Ok(Ok(Ok(child))) => {
                services.insert(name.to_string(), RunningService {
                    name: name.to_string(),
                    config: config.clone(),
                    child,
                });
                info!("✅ Service '{}' started successfully ({}ms)", name, timeout_ms);
                Ok(())
            }
            Ok(Ok(Err(e))) => {
                Err(anyhow!("Failed to spawn subprocess for '{}': {}. Command was: {}", name, e, cmd_str))
            }
            Ok(Err(e)) => {
                Err(anyhow!("Spawn task failed for '{}': {}", name, e))
            }
            Err(_) => {
                let err = anyhow!("Service '{}' spawn timed out after {}ms. The subprocess failed to start within the deadline.", name, timeout_ms);
                error!("🚨 {}", err);
                Err(err)
            }
        }
    }

    /// Check health of a specific service. Returns (is_alive, exit_code_or_none).
    pub async fn check_health(&self, name: &str) -> (bool, Option<i32>) {
        let mut services = self.services.write().await;
        if let Some(svc) = services.get_mut(name) {
            match svc.child.try_wait() {
                Ok(None) => (true, None), // Still running
                Ok(Some(code)) => {
                    warn!("🚨 Service '{}' exited with code {}", name, code);
                    (false, code.code())
                }
                Err(e) => {
                    warn!("🚨 Service '{}' health check error: {}", name, e);
                    (false, None)
                }
            }
        } else {
            (false, None)
        }
    }

    /// Get a descriptive health report for a service (for error propagation).
    pub async fn get_health_report(&self, name: &str) -> Option<String> {
        let (alive, code) = self.check_health(name).await;
        if alive {
            Some(format!("Service '{}' is running", name))
        } else {
            Some(format!("Service '{}' is dead (exit code: {:?})", name, code))
        }
    }

    /// Stop all running services gracefully.
    pub async fn shutdown_all(&self) {
        let mut services = self.services.write().await;
        info!("🛑 Shutting down {} auxiliary services...", services.len());
        
        for (name, mut service) in services.drain() {
            debug!("Killing service '{}'...", name);
            if let Err(e) = service.child.kill().await {
                warn!("Could not kill service '{}' cleanly: {}", name, e);
            }
        }
    }

    /// Get names of all currently running services.
    pub async fn list_running(&self) -> Vec<String> {
        let services = self.services.read().await;
        services.keys().cloned().collect()
    }

    /// Spawn a background task that checks every 30s whether always_on services
    /// have exited and restarts them if so.
    pub fn start_watchdog(&self, configs: HashMap<String, ServiceConfig>) {
        let services_ref = Arc::clone(&self.services);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                interval.tick().await;

                // Collect dead always_on services and remove them from the map
                let dead: Vec<(String, ServiceConfig)> = {
                    let mut map = services_ref.write().await;
                    let dead = map
                        .iter_mut()
                        .filter_map(|(name, svc)| {
                            if svc.child.try_wait().ok().flatten().is_some() {
                                Some((name.clone(), svc.config.clone()))
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>();
                    for (name, _) in &dead {
                        map.remove(name);
                    }
                    dead
                };

                // Perform health checks on remaining services
                let running_names: Vec<String> = {
                    let map = services_ref.read().await;
                    map.keys().cloned().collect()
                };
                for name in running_names {
                    let mut map = services_ref.write().await;
                    if let Some(svc) = map.get_mut(&name)
                         && svc.child.try_wait().ok().flatten().is_some() {
                             warn!("🚨 Watchdog: '{}' detected as dead, removing.", name);
                             map.remove(&name);
                         }
                }

                // Also pick up always_on services that were never started or previously failed
                let missing: Vec<(String, ServiceConfig)> = {
                    let map = services_ref.read().await;
                    configs
                        .iter()
                        .filter(|(name, cfg)| cfg.always_on && !map.contains_key(*name))
                        .map(|(name, cfg)| (name.clone(), cfg.clone()))
                        .collect()
                };

                for (name, cfg) in dead.into_iter().chain(missing) {
                    let Some(cmd_str) = &cfg.command else { continue };
                    warn!("🔄 Watchdog: restarting service '{}'", name);
                    let mut command = Command::new(cmd_str);
                    if let Some(a) = &cfg.args { command.args(a); }
                    if let Some(e) = &cfg.env { command.envs(e); }
                    command.stdout(Stdio::piped()).stderr(Stdio::piped());
                    match command.spawn() {
                        Ok(child) => {
                            let mut map = services_ref.write().await;
                            map.insert(name.clone(), RunningService {
                                name: name.clone(),
                                config: cfg,
                                child,
                            });
                            info!("✅ Watchdog: '{}' restarted", name);
                        }
                        Err(e) => error!("🚨 Watchdog: failed to restart '{}': {}", name, e),
                    }
                }
            }
        });
    }
}

impl Default for ServiceManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_service_manager_spawn_and_list() {
        let manager = ServiceManager::new();
        let mut configs = HashMap::new();
        
        let cmd = if cfg!(target_os = "windows") { "cmd".to_string() } else { "sh".to_string() };
        let args = if cfg!(target_os = "windows") {
            vec!["/c".to_string(), "echo hello".to_string()]
        } else {
            vec!["-c".to_string(), "echo hello".to_string()]
        };

        configs.insert("test-service".to_string(), ServiceConfig {
            command: Some(cmd),
            args: Some(args),
            always_on: true,
            url: None,
            env: None,
            timeout_ms: None,
        });

        manager.spawn_configured_services(&configs).await.unwrap();
        let running = manager.list_running().await;
        
        assert!(running.contains(&"test-service".to_string()));
        manager.shutdown_all().await;
        
        let after_shutdown = manager.list_running().await;
        assert_eq!(after_shutdown.len(), 0);
    }
}
