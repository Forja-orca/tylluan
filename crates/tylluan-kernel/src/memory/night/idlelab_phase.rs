use super::{Phase, PhaseContext, PhaseReport};
use crate::memory::idle_lab::IdleLab;

pub struct IdleLabPhase;

#[async_trait::async_trait]
impl Phase for IdleLabPhase {
    fn name(&self) -> &'static str {
        "IdleLab"
    }

    async fn run(&self, ctx: &PhaseContext) -> PhaseReport {
        let idle = IdleLab::new(ctx.silva.clone(), &ctx.data_dir);
        let rerank_ref = ctx.server.read().await.reranker.clone();
        idle.run_experiments(ctx.matcher.engine().as_deref(), rerank_ref.as_deref(), 9).await;
        PhaseReport { name: self.name(), duration_ms: 0, ok: true, detail: "hill-climb complete".to_string() }
    }
}
