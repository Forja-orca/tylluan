use super::{Phase, PhaseContext, PhaseReport};

pub struct LifecyclePhase;

#[async_trait::async_trait]
impl Phase for LifecyclePhase {
    fn name(&self) -> &'static str {
        "Lifecycle"
    }

    async fn run(&self, ctx: &PhaseContext) -> PhaseReport {
        match ctx.silva.apply_lifecycle_transitions().await {
            Ok((a2q, q2c, c2a, emb_purged)) => {
                let mut parts = Vec::new();
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