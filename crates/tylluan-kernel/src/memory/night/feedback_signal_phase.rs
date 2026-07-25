use super::{Phase, PhaseContext, PhaseReport};

/// ADR-011 Signal Loop. Resolves pending `recall_feedback` rows against
/// `guild_audit_log` (was a recalled memory referenced in the agent's next
/// few actions?), then reports the running total of resolved rows toward
/// the 5,000-row minimum ADR-011 §3.3 requires before the LightReranker
/// spike is even attempted.
pub struct FeedbackSignalPhase;

/// Only resolve rows old enough for the 3-turn window to plausibly have
/// closed — matches ADR-011's stated resolution window semantics.
const MIN_AGE_SECS: i64 = 300;

#[async_trait::async_trait]
impl Phase for FeedbackSignalPhase {
    fn name(&self) -> &'static str {
        "FeedbackSignal"
    }

    async fn run(&self, ctx: &PhaseContext) -> PhaseReport {
        let audit_path = ctx.data_dir.join("audit.db");
        let audit_str = audit_path.to_string_lossy().to_string();

        match ctx.silva.resolve_pending_feedback(&audit_str, MIN_AGE_SECS).await {
            Ok((useful, not_useful)) => {
                let total = ctx.silva.resolved_feedback_count().await.unwrap_or(0);
                PhaseReport {
                    name: self.name(),
                    duration_ms: 0,
                    ok: true,
                    detail: format!(
                        "resolved {useful} useful + {not_useful} not-useful this pass ({total}/5000 total toward LightReranker spike)"
                    ),
                }
            }
            Err(e) => PhaseReport { name: self.name(), duration_ms: 0, ok: false, detail: format!("resolve failed: {e}") },
        }
    }
}
