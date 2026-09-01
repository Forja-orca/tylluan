use super::{Phase, PhaseContext, PhaseReport};

/// Grace period (days) before a superseded summary node is physically pruned.
/// The node must already carry metadata.superseded_by (set by the system when
/// a newer summary replaced it) — see ADR-012 §2 D3.
const SUPERSEDED_PRUNE_DAYS: u32 = 14;

pub struct LifecyclePhase;

#[async_trait::async_trait]
impl Phase for LifecyclePhase {
    fn name(&self) -> &'static str {
        "Lifecycle"
    }

    async fn run(&self, ctx: &PhaseContext) -> PhaseReport {
        // Prune superseded agent_summary nodes past the grace window, so the
        // canary's active count reflects the pruned state on this tick.
        let pruned = ctx.silva.prune_superseded("agent_summary", SUPERSEDED_PRUNE_DAYS)
            .await
            .unwrap_or(0);
        match ctx.silva.apply_lifecycle_transitions().await {
            Ok((a2q, q2c, c2a, emb_purged)) => {
                let mut parts = Vec::new();
                if pruned > 0 { parts.push(format!("pruned-superseded:{pruned}")); }
                if a2q > 0 { parts.push(format!("active→quiet:{a2q}")); }
                if q2c > 0 { parts.push(format!("quiet→consolidated:{q2c}")); }
                if c2a > 0 { parts.push(format!("consolidated→archived:{c2a}")); }
                if emb_purged > 0 { parts.push(format!("embeddings-purged:{emb_purged}")); }
                let detail = if parts.is_empty() { "no transitions".to_string() } else { parts.join(", ") };
                PhaseReport { name: self.name(), duration_ms: 0, ok: true, detail }
            }
            Err(e) => PhaseReport {
                name: self.name(),
                duration_ms: 0,
                ok: false,
                detail: format!("error: {e}"),
            },
        }
    }
}