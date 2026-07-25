use std::time::Duration;
use super::{Phase, PhaseContext, PhaseReport};
use crate::memory::agent_memory::AgentMemoryManager;
use crate::memory::agent_profile::sync_agent_reputation_to_silva;
use tracing::warn;

pub struct AgentPhase;

#[async_trait::async_trait]
impl Phase for AgentPhase {
    fn name(&self) -> &'static str {
        "Agent"
    }

    async fn run(&self, ctx: &PhaseContext) -> PhaseReport {
        let mut details = Vec::new();

        if let Some(ref ap_mutex) = ctx.agent_profiles {
            // Per-agent memory consolidation
            let agent_ids: Vec<String> = {
                let deadline = std::time::Instant::now() + Duration::from_secs(5);
                loop {
                    if let Ok(ap) = ap_mutex.try_lock() {
                        break ap.list_profiles().unwrap_or_default().into_iter().map(|p| p.agent_id).collect();
                    }
                    if std::time::Instant::now() > deadline {
                        warn!("⚠️ AgentPhase: agent_profiles lock timeout (5s), skipping agent consolidation");
                        break vec![];
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            };
            if !agent_ids.is_empty() {
                let amm = AgentMemoryManager::new(ctx.silva.clone(), 20);
                for aid in &agent_ids {
                    amm.decay_agent_memories(aid).await;
                    amm.consolidate_if_needed(aid).await;
                }
                details.push(format!("processed {} agents", agent_ids.len()));
            }

            // Sync agent reputation scores to SilvaDB
            let profiles: Vec<_> = {
                let deadline = std::time::Instant::now() + Duration::from_secs(5);
                loop {
                    if let Ok(ap) = ap_mutex.try_lock() {
                        break ap.list_profiles().unwrap_or_default();
                    }
                    if std::time::Instant::now() > deadline {
                        break vec![];
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            };
            if !profiles.is_empty() {
                sync_agent_reputation_to_silva(&ctx.silva, &profiles).await;
                details.push(format!("synced {} reputation scores", profiles.len()));
            }
        }

        let detail = if details.is_empty() { "no agents configured".to_string() } else { details.join(", ") };
        PhaseReport { name: self.name(), duration_ms: 0, ok: true, detail }
    }
}
