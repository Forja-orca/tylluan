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
                details.push(format!("decayed {decayed} low-weight nodes"));
            }
        }

        // Auto-purge contaminated lesson nodes
        if let Ok(count) = ctx.silva.purge_deprecated_lessons().await
            && count > 0 {
                details.push(format!("purged {count} contaminated lesson nodes"));
            }

        // H-51: retencion acotada de coloquio_memory (silva.coloquio_retention_turns,
        // default 0 = APAGADO, cero cambio de comportamiento). Es BORRADO de datos:
        // opt-in del operador, nunca activado en produccion sin decision explicita
        // (T2099). Solo corre aqui, en NightConsolidation.
        match ctx.silva.prune_coloquio_retention(ctx.coloquio_retention_turns).await {
            Ok(count) if count > 0 => details.push(format!(
                "coloquio retention: {} nodos coloquio_memory eliminados (retencion {} turnos/canal)",
                count, ctx.coloquio_retention_turns
            )),
            Ok(_) => {}
            Err(e) => details.push(format!("coloquio retention failed: {e}")),
        }

        let detail = if details.is_empty() { "no decay needed".to_string() } else { details.join(", ") };
        PhaseReport { name: self.name(), duration_ms: 0, ok: true, detail }
    }
}
