//! # Sovereign Supervisor
//! 
//! Periodically monitors "Always-On" guilds and automatically re-spawns
//! them if they crash or stop unexpectedly. Mimics systemd behavior.

use super::guild_process::GuildRegistry;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use tokio::sync::RwLock;
use tracing::{info, warn, error};

/// Start the sovereign supervisor as a background task.
/// T13: Enhanced with exponential backoff to prevent CPU thrashing.
pub fn start_supervisor(
    registry: Arc<RwLock<GuildRegistry>>,
    check_interval_secs: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let interval = Duration::from_secs(check_interval_secs);
        let mut backoffs: HashMap<String, (u32, Instant)> = HashMap::new();
        
        loop {
            tokio::time::sleep(interval).await;
            
            let mut reg = registry.write().await;
            
            // Collect names of guilds that should be running but are not
            let crashed_guilds: Vec<String> = reg.guilds
                .values()
                .filter(|g| g.always_on && !g.is_running())
                .map(|g| g.name.clone())
                .collect();
            
            for name in crashed_guilds {
                // T13: Check backoff
                let entry = backoffs.entry(name.clone()).or_insert((0, Instant::now().checked_sub(Duration::from_secs(3600)).unwrap_or(Instant::now())));
                let (retries, last_attempt) = entry;

                if *retries >= 3 {
                    error!("🔥 [Supervisor] Guild '{}' failed {} times — marking as degraded, no further restarts", name, retries);
                    continue;
                }

                let backoff_secs = (2u64.pow(*retries)).min(600); // Max 10 mins backoff
                if last_attempt.elapsed() < Duration::from_secs(backoff_secs) {
                    continue; // Still in backoff period
                }

                warn!("🚨 [Supervisor] Detected crash/stop in '{}' (Retry #{}). Re-spawning in {}s...", 
                    name, retries, backoff_secs);
                
                *last_attempt = Instant::now();
                match reg.ensure_guild_running(&name).await {
                    Ok(_) => {
                        info!("✅ [Supervisor] Successfully restored guild '{}'", name);
                        *retries = 0; // Reset on success
                    }
                    Err(e) => {
                        error!("❌ [Supervisor] Failure restoring '{}': {}", name, e);
                        *retries += 1;
                    }
                }
            }
        }
    })
}
