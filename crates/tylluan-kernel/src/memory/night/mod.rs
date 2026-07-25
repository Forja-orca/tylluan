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
    phases: Vec<Arc<dyn Phase>>,
}

impl PhaseOrchestrator {
    pub fn new(phases: Vec<Box<dyn Phase>>) -> Self {
        Self { phases: phases.into_iter().map(Arc::from).collect() }
    }

    /// Runs every phase concurrently, capped to the machine's real core count.
    ///
    /// Sized off `available_parallelism()` rather than a fixed number so the
    /// same code gives a Raspberry Pi (2-4 cores) safe, non-contending
    /// concurrency and a many-core workstation full 8-way parallelism —
    /// no config knob, no hardcoded thread count either direction.
    /// Phases are independent (each touches SilvaDB through its own
    /// `Arc<Mutex<Connection>>`-serialized calls), so concurrent execution
    /// is safe. Spawning also means one phase panicking no longer aborts
    /// the rest — each is isolated in its own task.
    pub async fn run_all(&self, ctx: &PhaseContext) {
        let start = Instant::now();
        let max_parallel = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            .min(self.phases.len().max(1));
        let semaphore = Arc::new(tokio::sync::Semaphore::new(max_parallel));
        info!(
            "🌙 NightConsolidation: starting {} phase(s), up to {} in parallel",
            self.phases.len(),
            max_parallel
        );

        let mut handles = Vec::with_capacity(self.phases.len());
        for phase in &self.phases {
            let phase = phase.clone();
            let ctx = ctx.clone();
            let sem = semaphore.clone();
            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire_owned().await.expect("semaphore never closed");
                let phase_start = Instant::now();
                let report = phase.run(&ctx).await;
                (report, phase_start.elapsed())
            }));
        }

        for handle in handles {
            match handle.await {
                Ok((report, elapsed)) => {
                    if report.ok {
                        info!("  ✅ {} ({:?}) — {}", report.name, elapsed, report.detail);
                    } else {
                        tracing::warn!("  ⚠️ {} ({:?}) — {}", report.name, elapsed, report.detail);
                    }
                }
                Err(e) => tracing::warn!("  ⚠️ phase task panicked: {e}"),
            }
        }
        info!("🌙 NightConsolidation: all phases complete in {:?}", start.elapsed());
    }
}
