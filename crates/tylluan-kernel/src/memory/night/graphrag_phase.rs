use super::{Phase, PhaseContext, PhaseReport};
use crate::memory::graph_rag::GraphRagManager;
use tracing::warn;

pub struct GraphRagPhase;

#[async_trait::async_trait]
impl Phase for GraphRagPhase {
    fn name(&self) -> &'static str {
        "GraphRAG"
    }

    async fn run(&self, ctx: &PhaseContext) -> PhaseReport {
        let rag = GraphRagManager::new(ctx.silva.clone());
        match rag.identify_summarization_targets(3).await {
            Err(e) => PhaseReport { name: self.name(), duration_ms: 0, ok: false, detail: format!("identify failed: {e}") },
            Ok(targets) if targets.is_empty() => {
                PhaseReport { name: self.name(), duration_ms: 0, ok: true, detail: "0 targets (no components >= 3 nodes)".to_string() }
            }
            Ok(targets) => {
                let mut saved = 0usize;
                for target in &targets {
                    let member_ids: Vec<String> = target.nodes.iter().map(|n| n.id.clone()).collect();
                    let summary = target.nodes.iter()
                        .map(|n| n.content.chars().take(150).collect::<String>())
                        .collect::<Vec<_>>()
                        .join("\n---\n");
                    if summary.len() > 30 {
                        match rag.save_summary(&target.cluster_id, &summary, member_ids).await {
                            Ok(_) => saved += 1,
                            Err(e) => warn!("🧠 GraphRAG save_summary error: {e}"),
                        }
                    }
                }
                PhaseReport { name: self.name(), duration_ms: 0, ok: true, detail: format!("{saved} clusters summarized") }
            }
        }
    }
}
