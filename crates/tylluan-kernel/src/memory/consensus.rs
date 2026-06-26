//! # Consensus Engine for SilvaDB
//! 
//! Implements the Sovereing Consensus algorithm:
//! `score = (weight * trust) + (evidence_bonus * 2.0)`
//! 
//! - **Automatic Resolution**: Higher score wins, reinforces, and accelerates decay of losers.
//! - **Topic Clustering**: Groups related nodes for comparison.
//! - **Human Authority**: Allows manual override by the sovereign (the operator).

use crate::memory::silva::{SilvaDB, GraphNode};
use crate::memory::cosine::cosine_similarity;
use anyhow::Result;
use serde_json::json;
use std::collections::HashMap;
use tracing::{info, warn};

pub struct ConsensusEngine {
    silva: std::sync::Arc<SilvaDB>,
}

impl ConsensusEngine {
    pub fn new(silva: std::sync::Arc<SilvaDB>) -> Self {
        Self { silva }
    }

    pub async fn consolidate(&self, topic_key: Option<&str>) -> Result<usize> {
        let tx_id = format!("tx_{}", uuid::Uuid::new_v4().to_string().split('-').next().expect("UUID should have dashes"));
        let mut resolved_count = 0;
        
        let conflicts = if let Some(topic) = topic_key {
            let mut map = HashMap::new();
            let nodes = self.silva.search_by_topic(topic).await?;
            if !nodes.is_empty() {
                map.insert(topic.to_string(), nodes);
            }
            map
        } else {
            self.get_semantic_conflicted_groups().await?
        };
        
        info!("[{}] ⚖️ Consensus: processing {} conflict groups", tx_id, conflicts.len());
        
        for (topic, nodes) in conflicts {
            if nodes.len() < 2 { continue; }
            
            info!("[{}] ⚖️ Resolving topic: '{}' ({} candidates)", tx_id, topic, nodes.len());
            
            // 2. Calculate scores
            let mut scores: Vec<(String, f64)> = Vec::new();
            for node in &nodes {
                let trust = self.get_agent_trust(&node.id).await;
                let evidence = self.get_evidence_bonus(node).await;
                let score = (node.weight * trust) + (evidence * 2.0);
                scores.push((node.id.clone(), score));
            }
            
            // 3. Find winner
            scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            let (winner_id, winner_score) = &scores[0];
            let runner_up_score = scores.get(1).map(|s| s.1).unwrap_or(0.0);
            
            // 4. Multi-level resolution logic (o3 Optimized)
            let win_diff = (winner_score - runner_up_score).abs();
            let win_percent = (win_diff / winner_score.max(1.0)) * 100.0;

            info!("[{}] ⚖️ Topic='{}': winner='{}' ({:.2}), runner_up={:.2}, diff={:.1}%", 
                tx_id, topic, winner_id, winner_score, runner_up_score, win_percent);

            if win_percent >= 15.0 {
                // Case A: Clear Winner (>= 15%)
                self.apply_resolution(winner_id, &scores[1..]).await?;
                resolved_count += 1;
                info!("[{}] ✅ Resolved: clear winner='{}' (diff: {:.1}%)", tx_id, winner_id, win_percent);
            } 
            else if win_percent >= 5.0 {
                // Case B: Automatic Synthesis (5% - 15%)
                info!("[{}] 🔮 Synthesis: scores close ({:.1}%). Generating unified node...", tx_id, win_percent);
                let synth_id = self.apply_synthesis(&topic, &nodes, &tx_id).await?;
                resolved_count += 1;
                info!("[{}] ✅ Resolved: synthesis created='{}'", tx_id, synth_id);
            }
            else {
                // Case C: Critical Ambiguity (< 5%)
                warn!("[{}] ⚠️ Ambiguity: difference too small ({:.1}%). Manual intervention required.", 
                    tx_id, win_percent);
                for node in &nodes {
                    let _ = self.silva.set_status(&node.id, "Ambiguous").await;
                }
            }
        }
        
        Ok(resolved_count)
    }

    #[allow(dead_code)]
    pub async fn get_conflicted_groups(&self) -> Result<Vec<Vec<String>>> {
        Ok(vec![])
    }

    /// Group nodes by deep semantic meaning (Greedy Clustering).
    /// Uses cosine similarity > 0.85 to group different terms for the same concept.
    async fn get_semantic_conflicted_groups(&self) -> Result<HashMap<String, Vec<GraphNode>>> {
        let mut groups = HashMap::new();
        let conflicted_embs = self.silva.get_conflicted_embeddings().await?;
        
        if conflicted_embs.is_empty() {
            return Ok(groups);
        }

        info!("🔍 Semantic Consensus: Analyzing {} conflicted embeddings", conflicted_embs.len());

        let mut processed_ids = std::collections::HashSet::new();
        
        for (i, (id_a, emb_a)) in conflicted_embs.iter().enumerate() {
            if processed_ids.contains(id_a) { continue; }
            
            let mut current_group = Vec::new();
            if let Ok(Some(node_a)) = self.silva.get_node(id_a).await {
                current_group.push(node_a);
                processed_ids.insert(id_a.clone());
            } else { continue; }

            // Greedy search for similar neighbors
            for (id_b, emb_b) in conflicted_embs.iter().skip(i + 1) {
                if processed_ids.contains(id_b) { continue; }
                
                let similarity = cosine_similarity(emb_a, emb_b);
                if similarity > 0.80 { // Refined threshold for TylluanNexus v3.5 sovereignty
                    if let Ok(Some(node_b)) = self.silva.get_node(id_b).await {
                        info!("🔮 Semantic Match Found: '{}' matches '{}' (sim: {:.4})", id_a, id_b, similarity);
                        current_group.push(node_b);
                        processed_ids.insert(id_b.clone());
                    }
                } else if similarity > 0.70 {
                        info!("📡 Semantic Close Miss: '{}' and '{}' (sim: {:.4}) - ignoring", id_a, id_b, similarity);
                }
            }

            if current_group.len() > 1 {
                // Use the first node's ID as cluster identifier
                let cluster_id = format!("semantic_cluster_{}", id_a);
                info!("⚖️ Semantic Cluster formed: {} ({} nodes)", cluster_id, current_group.len());
                groups.insert(cluster_id, current_group);
            }
        }

        info!("🔍 Semantic Consensus: Formed {} active clusters from {} candidates", groups.len(), conflicted_embs.len());
        Ok(groups)
    }


    async fn get_agent_trust(&self, _node_id: &str) -> f64 {
        // Placeholder: would query identity.rs for agent trust levels.
        // TylluanNexus default: 1.0. High-trust agents can reach 1.5.
        1.0
    }

    async fn get_evidence_bonus(&self, node: &GraphNode) -> f64 {
        // Evidence if metadata contains 'file_ref' or 'test_result: success'
        let meta: serde_json::Value = serde_json::from_str(&node.metadata).unwrap_or(json!({}));
        if meta.get("file_ref").is_some() || meta.get("verified").and_then(|v| v.as_bool()) == Some(true) {
            1.0
        } else {
            0.0
        }
    }

    async fn apply_resolution(&self, winner_id: &str, losers: &[(String, f64)]) -> Result<()> {
        // Reinforce winner
        self.silva.reinforce_node(winner_id, 1.15).await?;
        
        // Accelerated decay for losers (skip protected)
        for (loser_id, _) in losers {
            if let Ok(Some(node)) = self.silva.get_node(loser_id).await
                && node.protected {
                    info!("🛡️ Skipping protected node: {}", loser_id);
                    continue;
                }
            // Mark as no longer conflicted since it's now a "loser" with penalty
            self.silva.mark_conflicted(loser_id, false).await?;
            self.silva.decay_node(loser_id, 604800).await?; // 7d half-life penalty
        }
        
        Ok(())
    }

    /// Creates a synthesis node combining knowledge from a close-score cluster.
    async fn apply_synthesis(&self, topic: &str, nodes: &[GraphNode], tx_id: &str) -> Result<String> {
        let synth_id = format!("sync_{}_{}", topic.replace(' ', "_"), uuid::Uuid::new_v4().to_string().split('-').next().expect("UUID should have dashes"));
        
        info!("[{}] 🔮 Synthesis: generating node '{}' as knowledge bridge", tx_id, synth_id);

        // Build synthesized content (initially: technical concatenation)
        let mut unified_content = format!("Synthesized Knowledge ({} sources):\n", nodes.len());
        for node in nodes {
            unified_content.push_str(&format!("- [{}] {}\n", node.id, node.content));
        }

        let metadata = json!({
            "type": "synthesis",
            "topic": topic,
            "sources": nodes.iter().map(|n| n.id.clone()).collect::<Vec<String>>(),
            "synthesized_at": chrono::Utc::now().to_rfc3339(),
            "tx_id": tx_id
        }).to_string();

        // 1. Persist the synthesis node (allow_drift=true: Consensus is an internal cognitive module)
        self.silva.upsert_node_with_validity(&synth_id, "synthesis", &unified_content, &metadata, None, true).await?;
        self.silva.reinforce_node(&synth_id, 1.25).await?;
        self.silva.set_protected(&synth_id, true).await?;

        // 2. Link sources to synthesis and resolve them
        for node in nodes {
            info!("[{}] 🔗 Linking contributor: '{}' -> '{}'", tx_id, node.id, synth_id);
            let _ = self.silva.add_edge(&node.id, &synth_id, "contributed_to", 1.0, "{}").await;
            let _ = self.silva.mark_conflicted(&node.id, false).await;
            let _ = self.silva.set_status(&node.id, "ResolvedBySynthesis").await;
        }

        Ok(synth_id)
    }

    /// Manual override by the sovereign (the operator).
    pub async fn human_override(&self, topic_key: &str, winner_id: &str) -> Result<()> {
        info!("👑 Sovereign Override: the operator declared '{}' as winner for topic '{}'", winner_id, topic_key);
        let nodes = self.silva.search_by_topic(topic_key).await?;
        let losers: Vec<(String, f64)> = nodes.into_iter()
            .filter(|n| n.id != winner_id)
            .map(|n| (n.id, 0.0))
            .collect();
            
        self.apply_resolution(winner_id, &losers).await?;
        // Set as protected so it doesn't enter consensus again easily
        self.silva.set_protected(winner_id, true).await?;
        Ok(())
    }
}
