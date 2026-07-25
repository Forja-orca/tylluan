use super::{Phase, PhaseContext, PhaseReport};

pub struct CurriculumPhase;

#[async_trait::async_trait]
impl Phase for CurriculumPhase {
    fn name(&self) -> &'static str {
        "Curriculum"
    }

    async fn run(&self, ctx: &PhaseContext) -> PhaseReport {
        if let Ok(mut learner) = ctx.curriculum.lock() {
            match learner.apply_disuse_decay() {
                Ok(n) if n > 0 => PhaseReport { name: self.name(), duration_ms: 0, ok: true, detail: format!("decayed {} stale entries", n) },
                Err(e) => PhaseReport { name: self.name(), duration_ms: 0, ok: false, detail: format!("decay failed: {e}") },
                _ => PhaseReport { name: self.name(), duration_ms: 0, ok: true, detail: "no stale entries".to_string() },
            }
        } else {
            PhaseReport { name: self.name(), duration_ms: 0, ok: false, detail: "curriculum lock failed".to_string() }
        }
    }
}
