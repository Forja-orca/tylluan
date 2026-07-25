use super::{Phase, PhaseContext, PhaseReport};
use crate::memory::dream_cycle::DreamCycle;

pub struct DreamPhase;

#[async_trait::async_trait]
impl Phase for DreamPhase {
    fn name(&self) -> &'static str {
        "DreamCycle"
    }

    async fn run(&self, ctx: &PhaseContext) -> PhaseReport {
        let dream = DreamCycle::new(ctx.silva.clone());
        let dr = dream.run().await;
        let detail = format!(
            "merged={} decayed={} contradictions={} exact_groups={} pairs={}/{} nodes graph={}n/{}e",
            dr.duplicates_merged, dr.nodes_decayed, dr.contradictions_flagged,
            dr.exact_content_groups, dr.pair_comparisons, dr.nodes_processed,
            dr.graph_nodes_total, dr.graph_edges_total
        );
        ctx.server.read().await.notify("dream_cycle_complete", serde_json::json!({
            "duplicates_merged": dr.duplicates_merged,
            "nodes_decayed": dr.nodes_decayed,
            "contradictions_flagged": dr.contradictions_flagged,
            "salience_pruned": dr.salience_pruned,
            "graph_nodes_total": dr.graph_nodes_total,
            "graph_edges_total": dr.graph_edges_total,
            "ts": chrono::Utc::now().timestamp_millis()
        }));
        PhaseReport { name: self.name(), duration_ms: 0, ok: true, detail }
    }
}
