use super::guild_process::GuildRegistry;
use crate::memory::silva::SilvaDB;
use crate::consensus::ConsensusEngine;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Start the lifecycle reaper as a background task.
///
/// Checks every `check_interval` seconds for idle guilds
/// and kills those that exceed the configured timeout.
/// Also performs periodic WAL checkpoint every 5 minutes.
pub fn start_lifecycle_reaper(
    registry: Arc<RwLock<GuildRegistry>>,
    check_interval_secs: u64,
) -> tokio::task::JoinHandle<()> {
    start_lifecycle_reaper_with_silva(registry, check_interval_secs, None, 336)
}

/// Start lifecycle reaper with SilvaDB for WAL checkpoint (P1 fix)
pub fn start_lifecycle_reaper_with_silva(
    registry: Arc<RwLock<GuildRegistry>>,
    check_interval_secs: u64,
    silva: Option<Arc<SilvaDB>>,
    decay_half_life_hours: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let interval = Duration::from_secs(check_interval_secs);
        let mut loop_count = 0;
        let monitoring_freq = 10 * 60 / check_interval_secs; // Log status every 10 mins
        let checkpoint_freq = 5 * 60 / check_interval_secs; // WAL checkpoint every 5 mins

        // Initialize Truth Consensus if Silva is present
        let consensus = silva.as_ref().map(|s| ConsensusEngine::new(s.clone()));

        loop {
            tokio::time::sleep(interval).await;
            debug!("🔄 Lifecycle reaper: checking for idle guilds...");

            let mut reg = registry.write().await;
            reg.reap_idle_guilds().await;

            // Watchdog: restart always_on guilds that have stopped or crashed
            let dead_core: Vec<String> = reg.guilds.values()
                .filter(|g| g.always_on && !g.is_running())
                .map(|g| g.name.clone())
                .collect();
            for name in dead_core {
                match reg.ensure_guild_running(&name).await {
                    Ok(()) => info!("🔄 Watchdog: guild '{}' restarted", name),
                    Err(e) => warn!("⚠️ Watchdog: guild '{}' not restartable yet: {}", name, e),
                }
            }

            // P1 Fix: Periodic WAL checkpoint & Truth Consensus
            loop_count += 1;
            if loop_count >= checkpoint_freq {
                loop_count = 0;
                if let Some(silva_db) = &silva {
                    if let Err(e) = silva_db.checkpoint().await {
                        tracing::warn!("⚠️ WAL checkpoint failed: {}", e);
                    } else {
                        info!("💾 [P1] WAL checkpoint completed");
                    }
                    
                    // Run Truth Consensus (T25)
                    if let Some(engine) = &consensus
                        && let Err(e) = engine.resolve_conflicts().await {
                            tracing::warn!("⚠️ Truth Consensus failed: {}", e);
                        }
                }
            }

            // Step 2: Biological Decay (T26) - Every 24 hours (or simulated period)
            // For o3, we run it every 100 intervals to ensure it happens occasionally in long sessions.
            if loop_count % 100 == 0
                && let Some(silva_db) = &silva {
                    info!("🧠 [T26] Applying biological decay to SilvaDB...");
                    let _ = silva_db.apply_decay(decay_half_life_hours).await;
                    let deleted = silva_db.apply_cleanup(0.05).await.unwrap_or(0);
                    if deleted > 0 {
                        info!("🧹 [T26] Biological pruning removed {} dead memories.", deleted);
                    }
                }

            if loop_count >= monitoring_freq {
                loop_count = 0;
                let online_count = reg.guilds.values().filter(|g| g.is_running()).count();
                let idle_count = reg.guilds.len() - online_count;
                
                tracing::info!(
                    "📊 Resilience Monitor [10m Heartbeat]: {} Online, {} Idle.", 
                    online_count, idle_count
                );
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::guild_process::GuildRegistry;
    use std::path::PathBuf;
    use crate::config::TimeoutsConfig;

    #[tokio::test]
    async fn test_lifecycle_reaper_starts_and_runs() {
        let registry = Arc::new(RwLock::new(
            GuildRegistry::new(PathBuf::from("."), 300, TimeoutsConfig::default(), 3),
        ));

        let handle = start_lifecycle_reaper(registry, 60);
        // Just verify it starts without panicking
        // The task runs forever, so we abort it
        handle.abort();
        assert!(handle.await.unwrap_err().is_cancelled());
    }
}
