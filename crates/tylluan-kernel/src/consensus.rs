//! # Truth Consensus Engine
//!
//! Processes the conflict queue using semantic clustering and truth weighting.

use crate::memory::silva::{GraphNode, SilvaDB};
use anyhow::Result;
use std::sync::Arc;
use tracing::info;
use chrono::Utc;

pub struct ConsensusEngine {
    silva: Arc<SilvaDB>,
}

impl ConsensusEngine {
    pub fn new(silva: Arc<SilvaDB>) -> Self {
        Self { silva }
    }

    pub async fn resolve_conflicts(&self) -> Result<()> {
        let conflicted = self.silva.get_all_conflicted().await?;
        if conflicted.is_empty() {
            return Ok(());
        }

        info!("🧠 [Consensus] Found {} conflicted nodes. Starting resolution...", conflicted.len());

        for node in conflicted {
            let node_id = node.id.clone();

            if let Ok(Some((matched, _score))) = self.find_similar(&node.content, 0.88).await {
                info!("🔄 [Consensus] Merge '{}' -> '{}'", node_id, matched.id);

                let current_meta: serde_json::Value = serde_json::from_str(&matched.metadata).unwrap_or(serde_json::json!({}));
                let mut updated_meta = current_meta.clone();

                let weight = updated_meta.get("weight").and_then(|v| v.as_f64()).unwrap_or(1.0);
                updated_meta["weight"] = serde_json::json!(weight + 0.1);
                updated_meta["last_reinforced"] = serde_json::json!(Utc::now().to_rfc3339());

                self.silva.upsert_node(&matched.id, &matched.node_type, &matched.content, &updated_meta.to_string()).await?;
                self.silva.set_weight(&node_id, 0.0).await?;
            } else {
                let _ = self.silva.mark_conflicted(&node_id, false).await;
                info!("✅ [Consensus] Approved unique thought: '{}'", node_id);
            }
        }

        Ok(())
    }

    async fn find_similar(&self, content: &str, threshold: f64) -> Result<Option<(GraphNode, f64)>> {
        let emb = self.silva.get_node_embedding(&format!("query:{}", content)).await?;
        let Some(emb) = emb else { return Ok(None); };

        let results = self.silva.search_vector(&emb, 5).await?;
        for (node, score) in results {
            if score as f64 >= threshold && node.id != format!("query:{}", content) {
                return Ok(Some((node, score as f64)));
            }
        }
        Ok(None)
    }
}
