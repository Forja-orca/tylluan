use super::{Phase, PhaseContext, PhaseReport};

pub struct DecayPhase;

#[async_trait::async_trait]
impl Phase for DecayPhase {
    fn name(&self) -> &'static str {
        "Decay+Purge"
    }

    async fn run(&self, ctx: &PhaseContext) -> PhaseReport {
        let mut details = Vec::new();

        // Selective decay: nodes with weight < 0.5
        if let Ok(nodes) = ctx.silva.get_nodes_limited(500, 0.01).await {
            let mut decayed = 0usize;
            for node in &nodes {
                if node.weight < 0.5 && node.node_type != "identity" && node.node_type != "agent_summary" {
                    let _ = ctx.silva.decay_node(&node.id, 43200).await;
                    decayed += 1;
                }
            }
            if decayed > 0 {
                details.push(format!("decayed {} low-weight nodes", decayed));
            }
        }

        // Auto-purge contaminated lesson nodes
        if let Ok(count) = ctx.silva.purge_deprecated_lessons().await
            && count > 0 {
                details.push(format!("purged {} contaminated lesson nodes", count));
            }

        let detail = if details.is_empty() { "no decay needed".to_string() } else { details.join(", ") };
        PhaseReport { name: self.name(), duration_ms: 0, ok: true, detail }
    }
}
