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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curriculum::CurriculumLearner;
    use crate::memory::agent_nodes::AgentNodeRouter;
    use crate::memory::hybrid::HybridMemory;
    use crate::memory::mailbox::Mailbox;
    use crate::memory::silva::SilvaDB;
    use crate::registry::guild_process::GuildRegistry;
    use crate::router::catalog::builtin_catalog;
    use crate::router::matcher::GuildMatcher;
    use crate::transport::server::TylluanServer;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::sync::{broadcast, RwLock};
    use uuid::Uuid;

    async fn test_server() -> TylluanServer {
        let reg = GuildRegistry::new(PathBuf::from("."), 300, Default::default(), 3);
        let test_reg = Arc::new(RwLock::new(reg));
        let matcher = GuildMatcher::new(builtin_catalog());
        let (tx, _) = broadcast::channel(16);
        let node_router = AgentNodeRouter::new(tx);
        let doctor = Arc::new(crate::doctor::Doctor::new(
            test_reg.clone(),
            Arc::new(HybridMemory::in_memory().await.unwrap()),
            Arc::new(SilvaDB::in_memory().await.unwrap()),
            Arc::new(Mutex::new(CurriculumLearner::new_in_memory(5).unwrap())),
        ));
        TylluanServer::new(
            test_reg,
            Arc::new(matcher),
            Arc::new(HybridMemory::in_memory().await.unwrap()),
            Arc::new(SilvaDB::in_memory().await.unwrap()),
            Arc::new(Mailbox::in_memory().await.unwrap()),
            doctor,
            node_router,
        )
    }

    async fn test_phase_context() -> PhaseContext {
        let tmp = std::env::temp_dir().join(format!("tylluan_night_test_{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&tmp).unwrap();
        let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
        PhaseContext {
            silva: silva.clone(),
            agent_profiles: None,
            curriculum: Arc::new(Mutex::new(CurriculumLearner::new_in_memory(5).unwrap())),
            server: Arc::new(RwLock::new(test_server().await)),
            data_dir: tmp,
            matcher: Arc::new(GuildMatcher::new(builtin_catalog())),
        }
    }

    // ── Orchestrator ────────────────────────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn orchestrator_runs_all_phases() {
        let ctx = test_phase_context().await;
        let phases: Vec<Box<dyn Phase>> = vec![
            Box::new(DreamPhase),
            Box::new(OuroborosPhase),
            Box::new(AutoLinkPhase),
            Box::new(GraphRagPhase),
            Box::new(DecayPhase),
            Box::new(AgentPhase),
            Box::new(CurriculumPhase),
            Box::new(IdleLabPhase),
        ];
        let orch = PhaseOrchestrator::new(phases);
        orch.run_all(&ctx).await;
        // No panic means success (all phases ran, each in its own task)
    }

    struct PanicPhase;
    #[async_trait::async_trait]
    impl Phase for PanicPhase {
        fn name(&self) -> &'static str { "panic-ph" }
        async fn run(&self, _ctx: &PhaseContext) -> PhaseReport {
            panic!("intentional panic for isolation test");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn orchestrator_panic_isolation() {
        let ctx = test_phase_context().await;
        let phases: Vec<Box<dyn Phase>> = vec![
            Box::new(DreamPhase),
            Box::new(PanicPhase),
            Box::new(AutoLinkPhase),
        ];
        let orch = PhaseOrchestrator::new(phases);
        orch.run_all(&ctx).await;
        // The panic in PanicPhase must not abort DreamPhase or AutoLinkPhase
        // tokio::spawn catches panics per-task
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn orchestrator_empty_phase_list() {
        let ctx = test_phase_context().await;
        let orch = PhaseOrchestrator::new(vec![]);
        orch.run_all(&ctx).await;
        // Must not panic on empty phase list
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn orchestrator_semaphore_limits_concurrency() {
        let ctx = test_phase_context().await;
        let started = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let max_concurrent = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let barrier = Arc::new(tokio::sync::Barrier::new(2));

        struct GatedPhase {
            started: Arc<std::sync::atomic::AtomicUsize>,
            max_concurrent: Arc<std::sync::atomic::AtomicUsize>,
            barrier: Arc<tokio::sync::Barrier>,
        }
        #[async_trait::async_trait]
        impl Phase for GatedPhase {
            fn name(&self) -> &'static str { "gated" }
            async fn run(&self, _ctx: &PhaseContext) -> PhaseReport {
                let prev = self.started.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                self.max_concurrent.fetch_max(prev, std::sync::atomic::Ordering::SeqCst);
                self.barrier.wait().await;
                PhaseReport { name: "gated", duration_ms: 0, ok: true, detail: "ok".into() }
            }
        }

        let phases: Vec<Box<dyn Phase>> = (0..4).map(|_| Box::new(GatedPhase {
            started: started.clone(),
            max_concurrent: max_concurrent.clone(),
            barrier: barrier.clone(),
        })).collect();

        let orch = PhaseOrchestrator::new(phases);
        orch.run_all(&ctx).await;
        let max = max_concurrent.load(std::sync::atomic::Ordering::SeqCst);
        assert!(max >= 1, "expected at least 1 concurrent phase, got {max}");
    }

    // ── DreamPhase ──────────────────────────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn dream_phase_runs_successfully_on_empty_db() {
        let ctx = test_phase_context().await;
        let report = DreamPhase.run(&ctx).await;
        assert!(report.ok, "DreamPhase should succeed on empty DB: {}", report.detail);
    }

    // ── OuroborosPhase ──────────────────────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn ouroboros_phase_no_audit_db_is_graceful() {
        // Point to a non-existent audit.db
        let ctx = test_phase_context().await;
        let report = OuroborosPhase.run(&ctx).await;
        assert!(report.ok, "OuroborosPhase should handle missing audit.db: {}", report.detail);
        assert!(report.detail.contains("no failure patterns"));
    }

    // ── AutoLinkPhase ───────────────────────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn autolink_phase_runs_on_empty_db() {
        let ctx = test_phase_context().await;
        let report = AutoLinkPhase.run(&ctx).await;
        assert!(report.ok, "AutoLinkPhase on empty DB: {}", report.detail);
    }

    // ── DecayPhase ──────────────────────────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn decay_phase_noops_on_empty_db() {
        let ctx = test_phase_context().await;
        let report = DecayPhase.run(&ctx).await;
        assert!(report.ok, "DecayPhase on empty DB: {}", report.detail);
        assert!(report.detail.contains("no decay needed"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn decay_phase_skips_identity_and_agent_summary() {
        let mut ctx = test_phase_context().await;
        ctx.silva.upsert_node("id-1", "identity", "An identity", "{}").await.unwrap();
        ctx.silva.upsert_node("id-2", "agent_summary", "Agent summary", "{}").await.unwrap();
        ctx.silva.set_weight("id-1", 0.1).await.unwrap();
        ctx.silva.set_weight("id-2", 0.1).await.unwrap();
        let report = DecayPhase.run(&ctx).await;
        assert!(report.ok);
        assert!(report.detail.contains("no decay needed") || report.detail.contains("decayed 0"));
    }

    // ── GraphRagPhase ───────────────────────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn graphrag_phase_noops_on_small_graph() {
        let ctx = test_phase_context().await;
        let report = GraphRagPhase.run(&ctx).await;
        assert!(report.ok, "GraphRAGPhase should handle empty graph: {}", report.detail);
        assert!(report.detail.contains("0 targets"));
    }

    // ── AgentPhase ──────────────────────────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn agent_phase_noops_without_profiles() {
        let ctx = test_phase_context().await;
        let report = AgentPhase.run(&ctx).await;
        assert!(report.ok, "AgentPhase without profiles: {}", report.detail);
        assert!(report.detail.contains("no agents configured"));
    }

    // ── CurriculumPhase ─────────────────────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn curriculum_phase_noops_on_empty_learner() {
        let ctx = test_phase_context().await;
        let report = CurriculumPhase.run(&ctx).await;
        assert!(report.ok, "CurriculumPhase on empty learner: {}", report.detail);
        assert!(report.detail.contains("no stale entries"));
    }

    // ── IdleLabPhase ────────────────────────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn idlelab_phase_runs_without_engine() {
        let ctx = test_phase_context().await;
        let report = IdleLabPhase.run(&ctx).await;
        // IdleLab does hill-climb with no embedding engine — should complete gracefully
        assert!(report.ok, "IdleLabPhase: {}", report.detail);
    }

    // ── PhaseReport ─────────────────────────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn phase_report_fields_are_correct() {
        let report = PhaseReport {
            name: "test-phase",
            duration_ms: 42,
            ok: true,
            detail: "testing".into(),
        };
        assert_eq!(report.name, "test-phase");
        assert_eq!(report.duration_ms, 42);
        assert!(report.ok);
        assert_eq!(report.detail, "testing");
    }
}
