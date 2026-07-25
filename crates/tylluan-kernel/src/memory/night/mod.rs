mod dream_phase;
mod ouroboros_phase;
mod autolink_phase;
mod graphrag_phase;
mod decay_phase;
mod agent_phase;
mod curriculum_phase;
mod idlelab_phase;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::info;

use crate::curriculum::CurriculumLearner;
use crate::memory::agent_profile::AgentProfileStore;
use crate::memory::silva::SilvaDB;
use crate::router::matcher::GuildMatcher;
use crate::transport::server::TylluanServer;

pub use dream_phase::DreamPhase;
pub use ouroboros_phase::OuroborosPhase;
pub use autolink_phase::AutoLinkPhase;
pub use graphrag_phase::GraphRagPhase;
pub use decay_phase::DecayPhase;
pub use agent_phase::AgentPhase;
pub use curriculum_phase::CurriculumPhase;
pub use idlelab_phase::IdleLabPhase;

#[derive(Clone)]
pub struct PhaseContext {
    pub silva: Arc<SilvaDB>,
    pub agent_profiles: Option<Arc<Mutex<AgentProfileStore>>>,
    pub curriculum: Arc<Mutex<CurriculumLearner>>,
    pub server: Arc<RwLock<TylluanServer>>,
    pub data_dir: PathBuf,
    pub matcher: Arc<GuildMatcher>,
}

pub struct PhaseReport {
    pub name: &'static str,
    pub duration_ms: u64,
    pub ok: bool,
    pub detail: String,
}

#[async_trait::async_trait]
pub trait Phase: Send + Sync {
    fn name(&self) -> &'static str;
    async fn run(&self, ctx: &PhaseContext) -> PhaseReport;
}

pub struct PhaseOrchestrator {
    phases: Vec<Box<dyn Phase>>,
}

impl PhaseOrchestrator {
    pub fn new(phases: Vec<Box<dyn Phase>>) -> Self {
        Self { phases }
    }

    pub async fn run_all(&self, ctx: &PhaseContext) {
        let start = Instant::now();
        info!("🌙 NightConsolidation: starting {} phase(s)", self.phases.len());
        for phase in &self.phases {
            let phase_start = Instant::now();
            let report = phase.run(ctx).await;
            let elapsed = phase_start.elapsed();
            if report.ok {
                info!("  ✅ {} ({:?}) — {}", report.name, elapsed, report.detail);
            } else {
                tracing::warn!("  ⚠️ {} ({:?}) — {}", report.name, elapsed, report.detail);
            }
        }
        info!("🌙 NightConsolidation: all phases complete in {:?}", start.elapsed());
    }
}
