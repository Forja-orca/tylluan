use super::guild_process::GuildRegistry;
use crate::memory::silva::SilvaDB;
use crate::consensus::ConsensusEngine;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Cadence (in loop iterations) of the biological decay + pruning pass.
/// The comment above its old call site said "every 100 intervals ... to ensure
/// it happens occasionally in long sessions" — that is the only cadence the
/// decay has ever documented, and it is independent of check_interval_secs.
const DECAY_FREQ: u64 = 100;

/// Pure decision core of the lifecycle reaper (hallazgo #1 de la auditoría
/// interna 2026-09-30): dada la iteración 1-based del bucle y las cadencias
/// por tarea, decide qué trabajos periódicos disparan en ESTA iteración.
/// Cada trabajo va en su propio módulo de iteración — el bug era un ÚNICO
/// loop_count compartido que el checkpoint reseteaba a 0 cada
/// `checkpoint_freq` iteraciones, por lo que `loop_count % 100 == 0` era
/// verdadero inmediatamente y el decay+cleanup (borrado REAL de memorias)
/// se disparaba cada ~5 minutos en vez de cada ~100, y el heartbeat de 10
/// min nunca llegaba. Extraído como función pura para poder testear las
/// cadencias sin sleeps reales (flaky). Los freq llegan como 0 si
/// check_interval_secs > su ventana — se tratan como "cada iteración"
/// (mismo espíritu que el `>= 0` del código viejo, sin pánicos de módulo 0).
fn periodic_actions(
    iteration: u64,
    checkpoint_freq: u64,
    decay_freq: u64,
    monitoring_freq: u64,
) -> (bool, bool, bool) {
    (
        iteration.is_multiple_of(checkpoint_freq.max(1)),
        iteration.is_multiple_of(decay_freq.max(1)),
        iteration.is_multiple_of(monitoring_freq.max(1)),
    )
}

/// Start the lifecycle reaper as a background task.
///
/// Checks every `check_interval` seconds for idle guilds
/// and kills those that exceed the configured timeout.
/// Also performs periodic WAL checkpoint every 5 minutes.
pub fn start_lifecycle_reaper(
    registry: Arc<RwLock<GuildRegistry>>,
    check_interval_secs: u64,
) -> tokio::task::JoinHandle<()> {
    start_lifecycle_reaper_with_silva(registry, check_interval_secs, None)
}

/// Start lifecycle reaper with SilvaDB for WAL checkpoint (P1 fix)
pub fn start_lifecycle_reaper_with_silva(
    registry: Arc<RwLock<GuildRegistry>>,
    check_interval_secs: u64,
    silva: Option<Arc<SilvaDB>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let interval = Duration::from_secs(check_interval_secs);
        let checkpoint_freq = (5 * 60 / check_interval_secs.max(1)).max(1); // WAL checkpoint every 5 mins
        let monitoring_freq = (10 * 60 / check_interval_secs.max(1)).max(1); // Log status every 10 mins
        let mut iteration: u64 = 0;

        // Initialize Truth Consensus if Silva is present
        let consensus = silva.as_ref().map(|s| ConsensusEngine::new(s.clone()));

        loop {
            tokio::time::sleep(interval).await;
            iteration = iteration.wrapping_add(1);
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
            // Hallazgo #1: soltar el write-lock ANTES del trabajo lento de
            // Silva (checkpoint/consensus/decay/cleanup borra memorias) — el
            // bloqueo bloqueaba TODAS las llamadas a guilds durante el borrado.
            drop(reg);

            // P1 Fix: Periodic WAL checkpoint & Truth Consensus
            let (checkpoint_due, decay_due, monitoring_due) =
                periodic_actions(iteration, checkpoint_freq, DECAY_FREQ, monitoring_freq);

            if checkpoint_due
                && let Some(silva_db) = &silva {
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

            // Step 2: Biological Decay (T26) — cada DECAY_FREQ iteraciones REALES
            // (con check_interval=60s: ~100 min). Con el loop_count compartido
            // viejo, el reset del checkpoint hacía que esto se disparara cada
            // ~5 minutos y borrara memorias vivas mucho antes de tiempo.
            if decay_due
                && let Some(silva_db) = &silva {
                    info!("🧠 [T26] Applying biological decay to SilvaDB...");
                    let _ = silva_db.apply_decay().await;
                    let deleted = silva_db.apply_cleanup(0.05).await.unwrap_or(0);
                    if deleted > 0 {
                        info!("🧹 [T26] Biological pruning removed {} dead memories.", deleted);
                    }
                }

            if monitoring_due {
                // Solo-lectura: el heartbeat solo cuenta guilds, no necesita
                // excluir a nadie (hallazgo #1: antes corría bajo el write-lock).
                let reg = registry.read().await;
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

    // ── Hallazgo #1 (auditoría 2026-09-30): cadencias del reaper ──

    #[test]
    fn periodic_actions_decay_fires_every_100_not_every_checkpoint() {
        // Con check_interval=60s: checkpoint cada 5, monitoring cada 10,
        // decay cada 100. EL BUG: en la iteración 5 el checkpoint reseteaba
        // loop_count a 0 y `loop_count % 100 == 0` disparaba el decay+cleanup
        // (borrado REAL de memorias) inmediatamente, cada ~5 min.
        let (cp, decay, mon) = periodic_actions(5, 5, 100, 10);
        assert!(cp && !decay && !mon, "iter 5: checkpoint sí, decay NO (era el bug), monitoring no");
        let (cp, decay, mon) = periodic_actions(10, 5, 100, 10);
        assert!(cp && !decay && mon, "iter 10: checkpoint+heartbeat sí, decay no");
        let (_, decay, _) = periodic_actions(100, 5, 100, 10);
        assert!(decay, "iter 100: el decay por fin dispara (cadencia documentada)");
        let all = periodic_actions(7, 5, 100, 10);
        assert_eq!(all, (false, false, false));
    }

    #[test]
    fn periodic_actions_counts_over_500_iterations() {
        let mut checkpoint = 0;
        let mut decay = 0;
        let mut monitoring = 0;
        for i in 1..=500u64 {
            let (cp, d, m) = periodic_actions(i, 5, 100, 10);
            checkpoint += cp as u64;
            decay += d as u64;
            monitoring += m as u64;
        }
        assert_eq!(checkpoint, 100);
        assert_eq!(monitoring, 50, "el heartbeat de 10m nunca se alcanzaba con el contador compartido");
        assert_eq!(decay, 5, "decay/cleanup 5 veces en 500 iter (~100 min con interval=60s), NO 100 veces como con el bug");
    }

    #[test]
    fn periodic_actions_zero_freq_clamps_to_every_iteration_without_panic() {
        // check_interval_secs > ventana (p.ej. 600s): la división da 0.
        // Antes `>= 0` disparaba siempre; el clamp mantiene ese espíritu sin
        // pánico de módulo por cero.
        let (cp, decay, mon) = periodic_actions(1, 0, 100, 0);
        assert!(cp && !decay && mon);
        let (cp, _, mon) = periodic_actions(3, 0, 100, 0);
        assert!(cp && mon);
    }
}
