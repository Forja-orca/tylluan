use super::{Phase, PhaseContext, PhaseReport};
use crate::memory::agent_memory::harvest_failures_from_audit;

pub struct OuroborosPhase;

#[async_trait::async_trait]
impl Phase for OuroborosPhase {
    fn name(&self) -> &'static str {
        "Ouroboros"
    }

    async fn run(&self, ctx: &PhaseContext) -> PhaseReport {
        let audit_path = ctx.data_dir.join("audit.db");
        let audit_str = audit_path.to_string_lossy();
        let harvested = harvest_failures_from_audit(&ctx.silva, &audit_str, 86_400, 3).await;
        let detail = if harvested > 0 {
            format!("{} repeated-failure pattern(s) promoted", harvested)
        } else {
            "no failure patterns found".to_string()
        };
        PhaseReport { name: self.name(), duration_ms: 0, ok: true, detail }
    }
}
