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
use crate::router::embeddings::EmbeddingEngine;
use anyhow::Result;
use serde_json::json;
use std::collections::HashMap;
use tracing::{info, warn};

/// Below this average cosine similarity against its own sources, a
/// synthesis node is flagged as semantically incoherent rather than
/// trusted outright. Same threshold Ouroboros uses for cluster
/// membership (SEMANTIC_CLUSTER_THRESHOLD) — one bar for "is this text
/// actually about what it claims to be about" across the codebase.
const SYNTHESIS_COHERENCE_THRESHOLD: f32 = 0.85;

/// Average cosine similarity of `target` against each vector in `sources`.
/// `None` when `sources` is empty — "no data to compare against", not "0.0 similarity".
fn average_cosine_to_sources(target: &[f32], sources: &[Vec<f32>]) -> Option<f32> {
    if sources.is_empty() {
        return None;
    }
    let sum: f32 = sources.iter().map(|s| cosine_similarity(target, s)).sum();
    Some(sum / sources.len() as f32)
}

pub struct ConsensusEngine {
    silva: std::sync::Arc<SilvaDB>,
}

impl ConsensusEngine {
    pub fn new(silva: std::sync::Arc<SilvaDB>) -> Self {
        Self { silva }
    }

    pub async fn consolidate(&self, topic_key: Option<&str>) -> Result<usize> {
        self.consolidate_with_engine(topic_key, None).await
    }

    /// Same as [`consolidate`], but when an `EmbeddingEngine` is provided,
    /// synthesis nodes (Case B: close-score automatic synthesis) are
    /// cross-verified with BGE-M3 before being trusted: the synthesized
    /// content is re-embedded and compared against each source node's
    /// stored embedding. This is the same encoder/decoder cross-check
    /// pattern Ouroboros uses for cluster membership — today's synthesis
    /// is literal source concatenation so it should always pass, but the
    /// gate exists so that when synthesis becomes generative (ADR-010),
    /// a hallucinated synthesis has to fool BOTH the generator and BGE-M3
    /// to be trusted, not just the generator.
    pub async fn consolidate_with_engine(
        &self,
        topic_key: Option<&str>,
        embedding_engine: Option<&EmbeddingEngine>,
    ) -> Result<usize> {
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
                let synth_id = self.apply_synthesis(&topic, &nodes, &tx_id, embedding_engine).await?;
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
                let cluster_id = format!("semantic_cluster_{id_a}");
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
    async fn apply_synthesis(
        &self,
        topic: &str,
        nodes: &[GraphNode],
        tx_id: &str,
        embedding_engine: Option<&EmbeddingEngine>,
    ) -> Result<String> {
        let synth_id = format!("sync_{}_{}", topic.replace(' ', "_"), uuid::Uuid::new_v4().to_string().split('-').next().expect("UUID should have dashes"));

        info!("[{}] 🔮 Synthesis: generating node '{}' as knowledge bridge", tx_id, synth_id);

        // Build synthesized content (initially: technical concatenation)
        let mut unified_content = format!("Synthesized Knowledge ({} sources):\n", nodes.len());
        for node in nodes {
            unified_content.push_str(&format!("- [{}] {}\n", node.id, node.content));
        }

        let coherence = self.verify_synthesis_coherence(&unified_content, nodes, embedding_engine).await;
        if let Some(score) = coherence {
            if score < SYNTHESIS_COHERENCE_THRESHOLD {
                warn!(
                    "[{}] 🚨 Synthesis coherence check FAILED for '{}': avg cosine {:.4} < {:.2} — \
                     synthesized content has drifted from its sources, flagging as unverified",
                    tx_id, synth_id, score, SYNTHESIS_COHERENCE_THRESHOLD
                );
            } else {
                info!("[{}] ✅ Synthesis coherence verified: avg cosine {:.4}", tx_id, score);
            }
        }

        let metadata = json!({
            "type": "synthesis",
            "topic": topic,
            "sources": nodes.iter().map(|n| n.id.clone()).collect::<Vec<String>>(),
            "synthesized_at": chrono::Utc::now().to_rfc3339(),
            "tx_id": tx_id,
            "coherence_score": coherence,
            "verified_coherent": coherence.map(|s| s >= SYNTHESIS_COHERENCE_THRESHOLD),
        }).to_string();

        // 1. Persist the synthesis node (allow_drift=true: Consensus is an internal cognitive module)
        self.silva.upsert_node_with_validity(&synth_id, "synthesis", &unified_content, &metadata, crate::memory::silva::NodeWriteOptions::new("agent_generated").drift_allowed(true)).await?;
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

    /// Cross-verifies synthesized content against its sources via BGE-M3.
    ///
    /// Embeds `unified_content` and compares it against each source node's
    /// already-stored embedding (`get_node_embedding`), returning the
    /// average cosine similarity. Returns `None` (not a failure) when no
    /// engine is available or no source has a stored embedding yet — the
    /// caller treats that as "unverifiable", not "incoherent".
    async fn verify_synthesis_coherence(
        &self,
        unified_content: &str,
        nodes: &[GraphNode],
        embedding_engine: Option<&EmbeddingEngine>,
    ) -> Option<f32> {
        let engine = embedding_engine?;
        let synth_emb = engine.embed(unified_content).ok()?;

        let mut source_embs = Vec::with_capacity(nodes.len());
        for node in nodes {
            if let Ok(Some(source_emb)) = self.silva.get_node_embedding(&node.id).await {
                source_embs.push(source_emb);
            }
        }
        average_cosine_to_sources(&synth_emb, &source_embs)
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

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup(topic: &str, entries: &[(&str, f64, bool)]) -> std::sync::Arc<SilvaDB> {
        let silva = std::sync::Arc::new(SilvaDB::in_memory().await.unwrap());
        for (id, weight, verified) in entries {
            let metadata = if *verified {
                json!({"topic": topic, "verified": true}).to_string()
            } else {
                json!({"topic": topic}).to_string()
            };
            // consolidate() is an internal cognitive module path — mirror that here
            // by writing via upsert_node_with_validity directly (bypasses drift guard,
            // not relevant for these node types, but keeps parity with production writers).
            silva.upsert_node_with_validity(id, "note", "content", &metadata, crate::memory::silva::NodeWriteOptions::new("agent_generated").drift_allowed(true)).await.unwrap();
            silva.set_weight(id, *weight).await.unwrap();
            silva.mark_conflicted(id, true).await.unwrap();
        }
        silva
    }

    /// score = weight * trust(1.0) + evidence_bonus * 2.0; trust and evidence are fixed
    /// in this module (get_agent_trust always 1.0, get_evidence_bonus 1.0 iff
    /// metadata.verified == true), so weight is the only free variable available
    /// to callers to steer win_percent deterministically in these tests.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_consolidate_clear_winner_reinforces_and_decays_loser() {
        let silva = setup("topic_a", &[("a", 1.0, false), ("b", 2.0, false)]).await;
        let engine = ConsensusEngine::new(silva.clone());

        let resolved = engine.consolidate(Some("topic_a")).await.unwrap();
        assert_eq!(resolved, 1);

        let winner = silva.get_node("b").await.unwrap().unwrap();
        let loser = silva.get_node("a").await.unwrap().unwrap();

        // Winner reinforced (1.15x, so > its pre-consolidate weight of 2.0) and no longer conflicted.
        assert!(winner.weight > 2.0, "winner should be reinforced, got {}", winner.weight);
        assert!(!winner.conflicted, "winner must not remain marked conflicted");

        // Loser decayed below its original weight and marked no-longer-conflicted
        // (accelerated decay penalty, not a re-open of the conflict).
        assert!(loser.weight < 1.0, "loser should decay below its original weight, got {}", loser.weight);
        assert!(!loser.conflicted, "loser should be un-marked conflicted after resolution");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_consolidate_protected_loser_is_not_decayed() {
        let silva = setup("topic_p", &[("a", 1.0, false), ("b", 2.0, false)]).await;
        silva.set_protected("a", true).await.unwrap();
        let engine = ConsensusEngine::new(silva.clone());

        engine.consolidate(Some("topic_p")).await.unwrap();

        let protected_loser = silva.get_node("a").await.unwrap().unwrap();
        // apply_resolution skips protected nodes entirely: no decay, no conflicted flip.
        assert!((protected_loser.weight - 1.0).abs() < 1e-9,
            "protected node weight must be untouched, got {}", protected_loser.weight);
        assert!(protected_loser.conflicted, "protected node's conflicted flag must be left as-is");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_consolidate_close_scores_trigger_synthesis() {
        // diff=0.1, winner_score=1.1 -> win_percent ~9.09%, inside the [5,15) synthesis band.
        let silva = setup("topic_s", &[("a", 1.0, false), ("b", 1.1, false)]).await;
        let engine = ConsensusEngine::new(silva.clone());

        let resolved = engine.consolidate(Some("topic_s")).await.unwrap();
        assert_eq!(resolved, 1);

        let a = silva.get_node("a").await.unwrap().unwrap();
        let b = silva.get_node("b").await.unwrap().unwrap();
        assert!(!a.conflicted && !b.conflicted, "both sources must be resolved, not left conflicted");

        let a_meta: serde_json::Value = serde_json::from_str(&a.metadata).unwrap();
        assert_eq!(a_meta.get("status").and_then(|v| v.as_str()), Some("ResolvedBySynthesis"));

        // A synthesis node must exist, be protected, and reference both sources.
        let synth_nodes = silva.search_by_topic("topic_s").await.unwrap();
        let synth = synth_nodes.iter().find(|n| n.node_type == "synthesis")
            .expect("a synthesis node must have been created");
        assert!(synth.protected, "synthesis node must be protected from decay");
        let synth_meta: serde_json::Value = serde_json::from_str(&synth.metadata).unwrap();
        let sources: Vec<String> = synth_meta.get("sources").unwrap()
            .as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
        assert!(sources.contains(&"a".to_string()) && sources.contains(&"b".to_string()));

        // No embedding engine passed via consolidate() -> coherence is unverifiable,
        // not "incoherent". null, not false.
        assert!(synth_meta.get("coherence_score").unwrap().is_null());
        assert!(synth_meta.get("verified_coherent").unwrap().is_null());
    }

    #[test]
    fn test_average_cosine_empty_sources_is_none() {
        assert_eq!(average_cosine_to_sources(&[1.0, 0.0], &[]), None);
    }

    #[test]
    fn test_average_cosine_identical_vectors_is_one() {
        let target = vec![1.0, 0.0, 0.0];
        let sources = vec![vec![1.0, 0.0, 0.0], vec![1.0, 0.0, 0.0]];
        let avg = average_cosine_to_sources(&target, &sources).unwrap();
        assert!((avg - 1.0).abs() < 1e-6, "identical vectors should average to ~1.0, got {avg}");
    }

    #[test]
    fn test_average_cosine_orthogonal_vector_pulls_average_down() {
        let target = vec![1.0, 0.0];
        // One source matches exactly (cos=1.0), one is orthogonal (cos=0.0) -> avg 0.5
        let sources = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        let avg = average_cosine_to_sources(&target, &sources).unwrap();
        assert!((avg - 0.5).abs() < 1e-6, "expected avg ~0.5, got {avg}");
        assert!(avg < SYNTHESIS_COHERENCE_THRESHOLD, "0.5 must fall below the coherence gate");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_consolidate_ambiguous_scores_mark_both_without_resolving() {
        // Identical weights -> win_percent = 0% < 5% -> Case C (critical ambiguity).
        let silva = setup("topic_c", &[("a", 1.0, false), ("b", 1.0, false)]).await;
        let engine = ConsensusEngine::new(silva.clone());

        let resolved = engine.consolidate(Some("topic_c")).await.unwrap();
        assert_eq!(resolved, 0, "ambiguous ties must not count as resolved");

        for id in ["a", "b"] {
            let node = silva.get_node(id).await.unwrap().unwrap();
            assert!((node.weight - 1.0).abs() < 1e-9, "ambiguous nodes must not be reinforced or decayed");
            assert!(node.conflicted, "ambiguous nodes must remain conflicted pending human review");
            let meta: serde_json::Value = serde_json::from_str(&node.metadata).unwrap();
            assert_eq!(meta.get("status").and_then(|v| v.as_str()), Some("Ambiguous"));
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_consolidate_skips_single_node_topics() {
        let silva = setup("topic_lonely", &[("a", 1.0, false)]).await;
        let engine = ConsensusEngine::new(silva.clone());

        let resolved = engine.consolidate(Some("topic_lonely")).await.unwrap();
        assert_eq!(resolved, 0, "a topic with a single candidate has nothing to resolve");

        let node = silva.get_node("a").await.unwrap().unwrap();
        assert!(node.conflicted, "untouched single-candidate node keeps its original conflicted flag");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_evidence_bonus_can_overturn_higher_raw_weight() {
        // Without evidence, "b" (weight 2.0) would win over "a" (weight 1.0) outright.
        // With "a" verified, its score becomes 1.0 + 2.0 = 3.0 vs "b"'s 2.0 -> "a" wins instead.
        let silva = setup("topic_evidence", &[("a", 1.0, true), ("b", 2.0, false)]).await;
        let engine = ConsensusEngine::new(silva.clone());

        engine.consolidate(Some("topic_evidence")).await.unwrap();

        let a = silva.get_node("a").await.unwrap().unwrap();
        let b = silva.get_node("b").await.unwrap().unwrap();
        assert!(a.weight > 1.0, "verified node 'a' should have won and been reinforced, got {}", a.weight);
        assert!(b.weight < 2.0, "outscored node 'b' should have been decayed, got {}", b.weight);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_human_override_reinforces_declared_winner_and_protects_it() {
        let silva = setup("topic_h", &[("a", 1.0, false), ("b", 1.0, false)]).await;
        let engine = ConsensusEngine::new(silva.clone());

        engine.human_override("topic_h", "a").await.unwrap();

        let winner = silva.get_node("a").await.unwrap().unwrap();
        let loser = silva.get_node("b").await.unwrap().unwrap();
        assert!(winner.weight > 1.0, "declared winner must be reinforced");
        assert!(winner.protected, "declared winner must be protected from future consensus churn");
        assert!(loser.weight < 1.0, "the non-chosen node must be decayed");
    }
}
