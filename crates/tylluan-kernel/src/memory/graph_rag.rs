//! # Hierarchical GraphRAG Summarization
//!
//! Clusters the knowledge graph using BFS connected components (reliable,
//! uses existing edges), then generates summaries via the deep_analysis guild.

use anyhow::Result;
use crate::memory::silva::SilvaDB;
use std::sync::Arc;
use std::collections::{HashMap, HashSet, VecDeque};
use tracing::{info, warn};

pub struct GraphRagManager {
    silva: Arc<SilvaDB>,
}

impl GraphRagManager {
    pub fn new(silva: Arc<SilvaDB>) -> Self {
        Self { silva }
    }

    /// Find connected components in the knowledge graph using BFS on existing edges.
    /// Returns clusters with at least `min_size` nodes.
    /// This replaces the Louvain-based approach which silently panicked on large graphs.
    pub async fn identify_summarization_targets(&self, min_size: usize) -> Result<Vec<ClusterSummaryTarget>> {
        // Load edges and nodes via proven SQL methods (same pattern as get_detailed_stats)
        let (node_ids, adjacency) = tokio::task::block_in_place(|| {
            let conn = self.silva.conn.blocking_lock();

            // Get all node IDs (only content-bearing types worth summarizing)
            let mut stmt = conn.prepare(
                "SELECT id FROM nodes WHERE type IN \
                ('document','episode','lesson','concept','synthesis','agent_memory','memory','summary') \
                LIMIT 3000"
            )?;
            let ids: Vec<String> = stmt.query_map([], |r| r.get(0))?
                .filter_map(|r| r.ok())
                .collect();

            // Build adjacency from edges
            let mut adj: HashMap<String, Vec<String>> = ids.iter()
                .map(|id| (id.clone(), Vec::new()))
                .collect();
            let mut stmt_e = conn.prepare("SELECT source, target FROM edges")?;
            let edges: Vec<(String, String)> = stmt_e.query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?.filter_map(|r| r.ok()).collect();

            for (src, tgt) in &edges {
                if adj.contains_key(src) && adj.contains_key(tgt) {
                    if let Some(neighbors) = adj.get_mut(src) {
                        neighbors.push(tgt.clone());
                    }
                    if let Some(neighbors) = adj.get_mut(tgt) {
                        neighbors.push(src.clone());
                    }
                }
            }

            Ok::<_, anyhow::Error>((ids, adj))
        })?;

        // BFS to find connected components
        let mut visited: HashSet<String> = HashSet::new();
        let mut components: Vec<Vec<String>> = Vec::new();

        for start in &node_ids {
            if visited.contains(start) { continue; }
            let mut comp = Vec::new();
            let mut queue = VecDeque::new();
            queue.push_back(start.clone());
            while let Some(n) = queue.pop_front() {
                if visited.contains(&n) { continue; }
                visited.insert(n.clone());
                comp.push(n.clone());
                if let Some(neighbors) = adjacency.get(&n) {
                    for nb in neighbors {
                        if !visited.contains(nb) {
                            queue.push_back(nb.clone());
                        }
                    }
                }
            }
            if comp.len() >= min_size {
                components.push(comp);
            }
        }

        info!("🧠 GraphRAG: BFS found {} components >= {} nodes", components.len(), min_size);

        // Resolve node objects for each component (cap at 20 nodes per cluster for performance).
        // Cluster ID is derived from the hub node (highest intra-component degree) — stable under
        // membership drift because adding/removing peripheral nodes doesn't change the hub.
        let mut targets = Vec::new();
        for comp in components.into_iter().take(30) {
            // Find hub: member with most neighbors inside this component
            let hub_id = comp.iter()
                .max_by_key(|id| {
                    adjacency.get(*id)
                        .map(|neighbors| neighbors.iter().filter(|n| comp.contains(n)).count())
                        .unwrap_or(0)
                })
                .cloned()
                .unwrap_or_else(|| comp[0].clone());
            let cluster_id = format!("cluster:{}", hub_id);

            let sample: Vec<String> = comp.into_iter().take(20).collect();
            let mut nodes = Vec::new();
            for node_id in &sample {
                if let Ok(Some(node)) = self.silva.get_node(node_id).await {
                    nodes.push(node);
                }
            }
            if nodes.len() >= min_size {
                targets.push(ClusterSummaryTarget {
                    cluster_id,
                    nodes,
                });
            }
        }

        info!("🧠 GraphRAG: {} summarization targets ready", targets.len());
        Ok(targets)
    }

    /// Save a generated summary to both the nodes table and the cluster_summaries table.
    /// Bug fix: previous version had wrong add_edge argument order.
    pub async fn save_summary(&self, cluster_id: &str, summary: &str, member_ids: Vec<String>) -> Result<String> {
        let node_id = format!("graphrag_summary:{}", cluster_id);
        let metadata = serde_json::json!({
            "type": "cluster_summary",
            "member_count": member_ids.len(),
            "generated_at": chrono::Utc::now().to_rfc3339()
        }).to_string();

        // 1. Upsert summary node (allow_drift=true: GraphRAG is an internal cognitive module)
        self.silva.upsert_node_with_validity(&node_id, "summary", summary, &metadata, None, true).await?;

        // 2. Link members to summary (fixed arg order: source, target, edge_type, weight, metadata)
        let mut linked = 0usize;
        for member_id in &member_ids {
            match self.silva.add_edge(&node_id, member_id, "member_of", 1.0, "{}").await {
                Ok(_) => linked += 1,
                Err(e) => warn!("GraphRAG: edge {}->{} failed: {}", node_id, member_id, e),
            }
        }

        // 3. Write to cluster_summaries table with dedup:
        //    If cluster_id + summary content already exists, keep the original created_at
        //    so the canary inflation alert doesn't fire for unchanged summaries.
        let members_json = serde_json::to_string(&member_ids).unwrap_or_default();
        tokio::task::block_in_place(|| {
            let conn = self.silva.conn.blocking_lock();
            conn.execute(
                "INSERT OR REPLACE INTO cluster_summaries (cluster_id, summary, members, created_at) \
                 VALUES (?1, ?2, ?3, COALESCE( \
                     (SELECT created_at FROM cluster_summaries WHERE cluster_id = ?1 AND summary = ?2), \
                     strftime('%s','now') \
                 ))",
                rusqlite::params![cluster_id, summary, members_json],
            )?;
            Ok::<(), rusqlite::Error>(())
        })?;

        info!("📝 GraphRAG: summary saved for cluster {} ({} members linked)", cluster_id, linked);
        Ok(node_id)
    }

    /// One-shot migration: collapse duplicate summary nodes (type=summary) with identical content.
    /// Operates on the `nodes` table directly — groups by exact content, keeps the heaviest
    /// node per group, merges the rest. This is the layer where recall actually reads from.
    pub async fn collapse_legacy_summaries(&self) -> Result<usize> {
        let all_summaries = self.silva.get_nodes_by_types(&["summary"], 3000).await?;

        // Group by exact content
        let mut by_content: HashMap<&str, Vec<(String, f64)>> = HashMap::new();
        for node in &all_summaries {
            by_content.entry(node.content.as_str())
                .or_default()
                .push((node.id.clone(), node.weight));
        }

        let mut total_merged = 0usize;
        let mut groups_found = 0usize;

        for (content, group) in &by_content {
            if group.len() < 2 { continue; }
            groups_found += 1;
            // Keep the one with highest weight, merge others into it
            let keep_idx = group.iter().enumerate()
                .max_by(|(_, a), (_, b)| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(i, _)| i).unwrap_or(0);
            let (keep_id, _) = &group[keep_idx];
            let mut merged_in_group = 0usize;
            for (i, (drop_id, _)) in group.iter().enumerate() {
                if i == keep_idx { continue; }
                if self.silva.merge_node_into(drop_id, keep_id).await.is_ok() {
                    total_merged += 1;
                    merged_in_group += 1;
                }
            }
            if merged_in_group > 0 {
                info!("🧹 GraphRAG: collapsed {} summaries (content '{}…' len={}) into 1",
                    merged_in_group + 1,
                    content.get(..60).unwrap_or(content),
                    content.len());
            }
        }

        if groups_found > 0 {
            info!("🧹 GraphRAG: collapsed {} groups, {} total duplicates merged (type=summary, exact content)", groups_found, total_merged);
        } else {
            info!("🧹 GraphRAG: no duplicate summary groups found — clean state");
        }
        Ok(total_merged)
    }
}

#[derive(serde::Serialize)]
pub struct ClusterSummaryTarget {
    pub cluster_id: String,
    pub nodes: Vec<crate::memory::silva::GraphNode>,
}
