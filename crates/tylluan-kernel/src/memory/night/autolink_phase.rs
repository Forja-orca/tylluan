use super::{Phase, PhaseContext, PhaseReport};
use crate::memory::auto_link::AutoLinker;

pub struct AutoLinkPhase;

#[async_trait::async_trait]
impl Phase for AutoLinkPhase {
    fn name(&self) -> &'static str {
        "AutoLink"
    }

    async fn run(&self, ctx: &PhaseContext) -> PhaseReport {
        let linker = AutoLinker::new(ctx.silva.clone());
        let lr = linker.run(ctx.matcher.engine().as_deref()).await;
        let added = lr.edges_after.saturating_sub(lr.edges_before);
        let detail = if added > 0 {
            format!("+{} edges (file_ref={} tool_ref={} topic={} orphan={})",
                added, lr.file_ref_edges, lr.tool_ref_edges, lr.topic_edges, lr.orphan_edges)
        } else {
            "no new edges".to_string()
        };
        PhaseReport { name: self.name(), duration_ms: 0, ok: true, detail }
    }
}
