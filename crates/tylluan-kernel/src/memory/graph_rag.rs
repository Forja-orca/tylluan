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

    /// Check if a cluster already has an identical summary with the same members.
    /// Used for idempotency: skip summarizing if nothing changed since last cycle.
    #[allow(dead_code)]
    pub(crate) fn has_identical_summary(&self, cluster_id: &str, member_ids: &[String]) -> bool {
        let members_json = serde_json::to_string(member_ids).unwrap_or_default();
        tokio::task::block_in_place(|| {
            let conn = self.silva.conn_timed();
            let mut stmt = conn.prepare(
                "SELECT 1 FROM cluster_summaries WHERE cluster_id = ?1 AND members = ?2 LIMIT 1"
            ).ok()?;
            let exists: bool = stmt.query_row(
                rusqlite::params![cluster_id, members_json],
                |r| r.get(0),
            ).unwrap_or(false);
            Some(exists)
        }).unwrap_or(false)
    }

    /// Find connected components in the knowledge graph using BFS on existing edges.
    /// Returns clusters with at least `min_size` nodes.
    /// This replaces the Louvain-based approach which silently panicked on large graphs.
    pub async fn identify_summarization_targets(&self, min_size: usize) -> Result<Vec<ClusterSummaryTarget>> {
        // Load edges and nodes via proven SQL methods (same pattern as get_detailed_stats)
        let (node_ids, adjacency) = tokio::task::block_in_place(|| {
            let conn = self.silva.conn.blocking_lock();

            // Get all node IDs (only content-bearing types worth summarizing).
            // Bug fix: exclude 'summary' type and 'graphrag_summary:*' ids to prevent
            // re-summarizing previous cycle output (unbounded nesting).
            let mut stmt = conn.prepare(
                "SELECT id FROM nodes WHERE type IN \
                ('document','episode','lesson','concept','synthesis','agent_memory','memory') \
                AND id NOT LIKE 'graphrag_summary:%' \
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
            let cluster_id = format!("cluster:{hub_id}");

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

    /// Save a generated summary to both the nodes table and the cluster_summaries table
    /// in a single atomic SQLite transaction (conn.transaction()).
    ///
    /// Reduces lock contention from ~23 individual mutex acquisitions down to 1.
    pub async fn save_summary(&self, cluster_id: &str, summary: &str, member_ids: Vec<String>) -> Result<String> {
        // Guard: reject nested graphrag_summary: prefixes (prevents unbounded nesting).
        if cluster_id.contains("graphrag_summary:") {
            warn!("GraphRAG: refusing to save nested summary for cluster_id={cluster_id} (already contains graphrag_summary: prefix)");
            anyhow::bail!("Cluster ID contains nested graphrag_summary: prefix — refusing to save.");
        }

        let node_id = format!("graphrag_summary:{cluster_id}");
        let metadata = serde_json::json!({
            "type": "cluster_summary",
            "member_count": member_ids.len(),
            "generated_at": chrono::Utc::now().to_rfc3339()
        }).to_string();
        let members_json = serde_json::to_string(&member_ids).unwrap_or_default();

        let (already_exists, linked) = tokio::task::block_in_place(|| -> Result<(bool, usize)> {
            let mut conn = self.silva.conn_timed();
            let tx = conn.transaction()?;

            // Idempotency: skip if cluster already has an identical summary with same members.
            let exists: bool = {
                let mut stmt = tx.prepare(
                    "SELECT 1 FROM cluster_summaries WHERE cluster_id = ?1 AND members = ?2 LIMIT 1",
                )?;
                stmt.query_row(
                    rusqlite::params![cluster_id, members_json],
                    |r| r.get(0),
                ).unwrap_or(false)
            };

            if exists {
                return Ok((true, 0));
            }

            // 1. Upsert summary node (allow_drift=true: GraphRAG is an internal cognitive module)
            use sha2::Digest;
            let content_hash = format!("{:x}", sha2::Sha256::digest(summary.as_bytes()));
            let vf = chrono::Utc::now().timestamp();

            tx.execute(
                "INSERT INTO nodes (id, type, content, metadata, weight, protected, conflicted, topic_key, updated_at, valid_from, valid_until, shareable, federation_source, content_hash, provenance, owner_scope, source, author, evidence_url)
                 VALUES (?1, 'summary', ?2, ?3, 1.0, 0, 0, NULL, CURRENT_TIMESTAMP, ?4, NULL, 0, NULL, ?5, 'agent_generated', NULL, NULL, NULL, NULL)
                 ON CONFLICT(id) DO UPDATE SET
                    content = excluded.content,
                    metadata = excluded.metadata,
                    weight = CASE
                        WHEN nodes.type = 'identity' OR nodes.protected = 1 THEN nodes.weight
                        ELSE MAX(nodes.weight, excluded.weight)
                    END,
                    protected = excluded.protected,
                    topic_key = COALESCE(excluded.topic_key, nodes.topic_key),
                    valid_from = COALESCE(excluded.valid_from, nodes.valid_from),
                    valid_until = COALESCE(excluded.valid_until, nodes.valid_until),
                    shareable = excluded.shareable,
                    federation_source = COALESCE(excluded.federation_source, nodes.federation_source),
                    content_hash = COALESCE(excluded.content_hash, nodes.content_hash),
                    provenance = excluded.provenance,
                    owner_scope = COALESCE(excluded.owner_scope, nodes.owner_scope),
                    source = COALESCE(excluded.source, nodes.source),
                    author = COALESCE(excluded.author, nodes.author),
                    evidence_url = COALESCE(excluded.evidence_url, nodes.evidence_url),
                    lifecycle_state = COALESCE(excluded.lifecycle_state, nodes.lifecycle_state),
                    last_agent_access = COALESCE(excluded.last_agent_access, nodes.last_agent_access),
                    reactivation_count = COALESCE(excluded.reactivation_count, nodes.reactivation_count),
                    updated_at = CURRENT_TIMESTAMP",
                rusqlite::params![node_id, summary, metadata, vf, content_hash],
            )?;

            // Sync FTS5 index within the same transaction
            if let Ok(rowid) = tx.query_row("SELECT rowid FROM nodes WHERE id = ?1", rusqlite::params![node_id], |r| r.get::<_, i64>(0)) {
                let _ = tx.execute(
                    "INSERT INTO nodes_fts(rowid, id, content, metadata) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![rowid, node_id, summary, metadata],
                );
            }

            // 2. Link members to summary (fixed arg order: source, target, edge_type, weight, metadata)
            let mut linked = 0usize;
            {
                let mut edge_stmt = tx.prepare(
                    "INSERT INTO edges (source, target, type, weight, metadata, valid_from, valid_until)
                     VALUES (?1, ?2, 'member_of', 1.0, '{}', NULL, NULL)
                     ON CONFLICT(source, target, type) DO UPDATE SET
                        weight = excluded.weight,
                        metadata = excluded.metadata,
                        valid_from = excluded.valid_from,
                        valid_until = excluded.valid_until",
                )?;

                for member_id in &member_ids {
                    match edge_stmt.execute(rusqlite::params![node_id, member_id]) {
                        Ok(_) => linked += 1,
                        Err(e) => warn!("GraphRAG: edge {}->{} failed: {}", node_id, member_id, e),
                    }
                }
            }

            // 3. Write to cluster_summaries table with dedup:
            //    If cluster_id + summary content already exists, keep the original created_at
            //    so the canary inflation alert doesn't fire for unchanged summaries.
            tx.execute(
                "INSERT OR REPLACE INTO cluster_summaries (cluster_id, summary, members, created_at) \
                 VALUES (?1, ?2, ?3, COALESCE( \
                     (SELECT created_at FROM cluster_summaries WHERE cluster_id = ?1 AND summary = ?2), \
                     strftime('%s','now') \
                 ))",
                rusqlite::params![cluster_id, summary, members_json],
            )?;

            tx.commit()?;
            Ok((false, linked))
        })?;

        if already_exists {
            info!("GraphRAG: cluster {} already has identical summary with same members — skipping (idempotent)", cluster_id);
            return Ok(node_id);
        }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::silva::SilvaDB;

    // Regression test for the CPU runaway found live 2026-08-30 (measured:
    // ~4257% CPU sustained 9.5h idle, 323 threads): a previous cycle's
    // summary node became the "hub" of a new cluster
    // (cluster_id = format!("cluster:{hub_id}")), producing
    // cluster:graphrag_summary:cluster:X, then graphrag_summary: that ->
    // unbounded nesting, confirmed 20+ levels deep in real kernel.log
    // output. This locks in the save_summary() guard so the exact failure
    // mode can't silently regress -- find_clusters() also excludes
    // graphrag_summary:% from its candidate pool, but that path needs a
    // populated graph fixture; this is the cheap, decisive lock on the
    // defense-in-depth layer.
    #[tokio::test(flavor = "multi_thread")]
    async fn save_summary_rejects_nested_graphrag_summary_prefix() {
        let db = Arc::new(SilvaDB::in_memory().await.unwrap());
        let manager = GraphRagManager::new(db);

        // Simulates a hub_id that was itself a previous cycle's summary node.
        let nested_cluster_id = "graphrag_summary:cluster:some-hub-id";
        let result = manager
            .save_summary(nested_cluster_id, "a summary", vec!["member1".to_string()])
            .await;

        assert!(
            result.is_err(),
            "save_summary must reject a cluster_id that already contains \
             'graphrag_summary:' -- accepting it is exactly the bug that \
             produced 20+ levels of nesting in production"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn save_summary_accepts_clean_cluster_id() {
        let db = Arc::new(SilvaDB::in_memory().await.unwrap());
        let manager = GraphRagManager::new(db);

        let result = manager
            .save_summary("cluster:some-hub-id", "a summary", vec!["member1".to_string()])
            .await;

        assert!(result.is_ok(), "a clean, non-nested cluster_id must still work: {:?}", result.err());
        assert_eq!(result.unwrap(), "graphrag_summary:cluster:some-hub-id");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn save_summary_persists_node_edges_and_cluster_summary_in_single_tx() {
        let db = Arc::new(SilvaDB::in_memory().await.unwrap());
        let manager = GraphRagManager::new(db.clone());

        // Pre-populate two member nodes
        db.upsert_node("member-1", "concept", "Concept 1", "{}").await.unwrap();
        db.upsert_node("member-2", "concept", "Concept 2", "{}").await.unwrap();

        let cluster_id = "cluster:member-1";
        let summary_text = "Cluster summary for members 1 and 2";
        let member_ids = vec!["member-1".to_string(), "member-2".to_string()];

        let saved_id = manager
            .save_summary(cluster_id, summary_text, member_ids.clone())
            .await
            .expect("save_summary must succeed");
        assert_eq!(saved_id, "graphrag_summary:cluster:member-1");

        // 1. Verify summary node in nodes table
        let node = db.get_node(&saved_id).await.unwrap().expect("summary node must exist");
        assert_eq!(node.node_type, "summary");
        assert_eq!(node.content, summary_text);
        assert_eq!(node.provenance, "agent_generated");

        // 2. Verify edges table has member_of links for both members
        let targets: Vec<String> = tokio::task::block_in_place(|| {
            let conn = db.conn.blocking_lock();
            let mut stmt = conn.prepare("SELECT target FROM edges WHERE source = ?1 AND type = 'member_of'").unwrap();
            stmt.query_map(rusqlite::params![saved_id], |r| r.get(0))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect()
        });
        assert_eq!(targets.len(), 2, "must have 2 member_of edges");
        assert!(targets.contains(&"member-1".to_string()));
        assert!(targets.contains(&"member-2".to_string()));

        // 3. Verify cluster_summaries table row
        let cs_count: i64 = tokio::task::block_in_place(|| {
            let conn = db.conn.blocking_lock();
            conn.query_row(
                "SELECT COUNT(*) FROM cluster_summaries WHERE cluster_id = ?1 AND summary = ?2",
                rusqlite::params![cluster_id, summary_text],
                |r| r.get(0),
            ).unwrap()
        });
        assert_eq!(cs_count, 1, "cluster_summaries row must exist");

        // 4. Verify idempotency: calling save_summary again returns existing ID and does not duplicate
        let second_id = manager
            .save_summary(cluster_id, summary_text, member_ids)
            .await
            .expect("second save_summary call must succeed (idempotent)");
        assert_eq!(second_id, saved_id);

        let cs_count_after: i64 = tokio::task::block_in_place(|| {
            let conn = db.conn.blocking_lock();
            conn.query_row(
                "SELECT COUNT(*) FROM cluster_summaries WHERE cluster_id = ?1",
                rusqlite::params![cluster_id],
                |r| r.get(0),
            ).unwrap()
        });
        assert_eq!(cs_count_after, 1, "cluster_summaries count must remain 1 after second call");
    }
}
