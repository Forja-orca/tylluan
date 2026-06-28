use anyhow::Result;
use chrono::Utc;
use rusqlite::params;
use tracing::warn;

use super::GraphNode;

impl super::SilvaDB {
    /// Set the shareable flag on a node (used by federation sync).
    pub async fn set_shareable(&self, node_id: &str, shareable: bool) -> Result<()> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            conn.execute(
                "UPDATE nodes SET shareable = ?1 WHERE id = ?2",
                params![shareable as i32, node_id],
            )?;
            Ok::<(), anyhow::Error>(())
        })?;
        Ok(())
    }

    // ── Routing Anchors ───────────────────────────────────────────────────────

    /// Store a routing anchor (curated or learned example intent) for a guild.
    /// ID is deterministic: same guild+intent always maps to the same node.
    pub async fn upsert_routing_anchor(
        &self,
        guild: &str,
        intent: &str,
        source: &str, // "seed" | "learned" | "agent"
        embedding: Option<&[f32]>,
    ) -> Result<()> {
        use sha2::{Sha256, Digest};
        let hash_bytes = Sha256::digest(intent.as_bytes());
        let hash_str: String = hash_bytes[..6].iter().map(|b| format!("{:02x}", b)).collect();
        let id = format!("routing_anchor:{}:{}", guild, hash_str);
        let meta = serde_json::json!({"guild": guild, "source": source}).to_string();
        self.upsert_node(&id, "routing_anchor", intent, &meta).await?;
        if let Some(emb) = embedding {
            self.save_embedding(&id, emb, "bge-m3", None).await
                .map_err(|e| warn!("🌲 Failed to save anchor embedding for '{}': {}", id, e)).ok();
        }
        Ok(())
    }

    /// Pure vector search over `routing_anchor` nodes only.
    /// Returns `(guild_name, best_cosine_score)` sorted descending.
    /// Aggregates by guild — the top score per guild wins.
    pub async fn match_by_anchors(
        &self,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<(String, f32)>> {
        let q_emb = query_embedding.to_vec();
        let results = tokio::task::block_in_place(|| -> rusqlite::Result<Vec<(String, f32)>> {
            let conn = self.conn.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT ne.embedding, n.metadata
                 FROM node_embeddings ne
                 JOIN nodes n ON n.id = ne.node_id
                 WHERE n.type = 'routing_anchor'"
            )?;
            let mut guild_best: std::collections::HashMap<String, f32> =
                std::collections::HashMap::new();
            let rows = stmt.query_map([], |row| {
                let blob: Vec<u8> = row.get(0)?;
                let meta: String = row.get(1)?;
                Ok((blob, meta))
            })?;
            for row in rows.flatten() {
                let (blob, meta) = row;
                if blob.is_empty() { continue; }
                let stored: Vec<f32> = blob.chunks_exact(4)
                    .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect();
                if stored.len() != q_emb.len() { continue; }
                let sim = crate::memory::cosine::cosine_similarity(&q_emb, &stored);
                if sim < 0.35 { continue; }
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&meta)
                    && let Some(guild) = v.get("guild").and_then(|g| g.as_str()) {
                        guild_best.entry(guild.to_string())
                            .and_modify(|s| if *s < sim { *s = sim })
                            .or_insert(sim);
                    }
            }
            let mut ranked: Vec<(String, f32)> = guild_best.into_iter().collect();
            ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            ranked.truncate(top_k);
            Ok(ranked)
        })?;
        Ok(results)
    }

    /// Get all routing anchors, optionally filtered by guild.
    pub async fn get_routing_anchors(&self, guild_filter: Option<&str>, limit: usize) -> Result<Vec<GraphNode>> {
        let filter = guild_filter.map(|s| s.to_string());
        tokio::task::block_in_place(move || {
            let conn = self.conn.blocking_lock();
            let nodes: Vec<GraphNode> = if let Some(ref g) = filter {
                let mut stmt = conn.prepare(
                    "SELECT id, type, content, metadata, weight, protected, conflicted,
                            topic_key, created_at, updated_at, last_touched, valid_from, valid_until, shareable
                     FROM nodes WHERE type = 'routing_anchor' AND metadata LIKE ?1 LIMIT ?2"
                )?;
                let pattern = format!("%\"guild\":\"{}\"%", g);
                stmt.query_map(params![pattern, limit as i64], |row| Ok(GraphNode {
                    id: row.get(0)?,
                    node_type: row.get(1)?,
                    content: row.get(2)?,
                    metadata: row.get(3)?,
                    weight: row.get(4)?,
                    protected: row.get::<_, i32>(5)? != 0,
                    conflicted: row.get::<_, i32>(6)? != 0,
                    topic_key: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                    last_touched: chrono::Utc::now(),
                    valid_from: row.get(11)?,
                    valid_until: row.get(12)?,
                    shareable: row.get::<_, i32>(13)? != 0,
                }))?.flatten().collect()
            } else {
                let mut stmt = conn.prepare(
                    "SELECT id, type, content, metadata, weight, protected, conflicted,
                            topic_key, created_at, updated_at, last_touched, valid_from, valid_until, shareable
                     FROM nodes WHERE type = 'routing_anchor' LIMIT ?1"
                )?;
                stmt.query_map(params![limit as i64], |row| Ok(GraphNode {
                    id: row.get(0)?,
                    node_type: row.get(1)?,
                    content: row.get(2)?,
                    metadata: row.get(3)?,
                    weight: row.get(4)?,
                    protected: row.get::<_, i32>(5)? != 0,
                    conflicted: row.get::<_, i32>(6)? != 0,
                    topic_key: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                    last_touched: chrono::Utc::now(),
                    valid_from: row.get(11)?,
                    valid_until: row.get(12)?,
                    shareable: row.get::<_, i32>(13)? != 0,
                }))?.flatten().collect()
            };
            Ok(nodes)
        })
    }

    /// Get all shareable local nodes (excludes nodes received from federation to prevent echo loops).
    pub async fn get_shareable_nodes(&self) -> Result<Vec<GraphNode>> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT id, type, content, metadata, weight, protected, conflicted, topic_key, created_at, updated_at, valid_from, valid_until, shareable
                 FROM nodes
                 WHERE shareable = 1 AND federation_source IS NULL"
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(GraphNode {
                    id: row.get(0)?,
                    node_type: row.get(1)?,
                    content: row.get(2)?,
                    metadata: row.get(3)?,
                    weight: row.get(4)?,
                    protected: row.get::<_, i32>(5)? != 0,
                    conflicted: row.get::<_, i32>(6)? != 0,
                    topic_key: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                    valid_from: row.get(10)?,
                    valid_until: row.get(11)?,
                    shareable: row.get::<_, i32>(12)? != 0,
                    last_touched: Utc::now(),
                })
            })?;
            let mut results = Vec::new();
            for node in rows.flatten() {
                results.push(node);
            }
            Ok(results)
        })
    }

    /// Query nodes by federation provenance.
    /// - `source = Some("local")` or `source = None` → nodes with `federation_source IS NULL`
    /// - `source = Some("peer-name")` → nodes received from that specific peer
    pub async fn get_nodes_by_source(&self, source: Option<&str>, limit: usize) -> Result<Vec<GraphNode>> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let nodes: Vec<GraphNode> = if source.is_none() || source == Some("local") {
                let mut stmt = conn.prepare(
                    "SELECT id, type, content, metadata, weight, protected, conflicted, topic_key, created_at, updated_at, valid_from, valid_until, shareable
                     FROM nodes WHERE federation_source IS NULL LIMIT ?1"
                )?;
                stmt.query_map(rusqlite::params![limit as i64], |row| {
                    Ok(GraphNode {
                        id: row.get(0)?, node_type: row.get(1)?, content: row.get(2)?,
                        metadata: row.get(3)?, weight: row.get(4)?,
                        protected: row.get::<_, i32>(5)? != 0, conflicted: row.get::<_, i32>(6)? != 0,
                        topic_key: row.get(7)?, created_at: row.get(8)?, updated_at: row.get(9)?,
                        valid_from: row.get(10)?, valid_until: row.get(11)?,
                        shareable: row.get::<_, i32>(12)? != 0, last_touched: Utc::now(),
                    })
                })?.flatten().collect()
            } else {
                let peer = source.unwrap();
                let mut stmt = conn.prepare(
                    "SELECT id, type, content, metadata, weight, protected, conflicted, topic_key, created_at, updated_at, valid_from, valid_until, shareable
                     FROM nodes WHERE federation_source = ?1 LIMIT ?2"
                )?;
                stmt.query_map(rusqlite::params![peer, limit as i64], |row| {
                    Ok(GraphNode {
                        id: row.get(0)?, node_type: row.get(1)?, content: row.get(2)?,
                        metadata: row.get(3)?, weight: row.get(4)?,
                        protected: row.get::<_, i32>(5)? != 0, conflicted: row.get::<_, i32>(6)? != 0,
                        topic_key: row.get(7)?, created_at: row.get(8)?, updated_at: row.get(9)?,
                        valid_from: row.get(10)?, valid_until: row.get(11)?,
                        shareable: row.get::<_, i32>(12)? != 0, last_touched: Utc::now(),
                    })
                })?.flatten().collect()
            };
            Ok(nodes)
        })
    }

    /// Re-generates embeddings for all routing_anchor nodes that have no stored embedding.
    /// Called at startup after BGE-M3 is confirmed ready. Returns count of nodes re-embedded.
    pub async fn reembed_anchors(
        &self,
        engine: &crate::router::embeddings::EmbeddingEngine,
    ) -> Result<usize> {
        // Collect IDs and contents of anchors missing an embedding
        let missing: Vec<(String, String)> = tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT n.id, n.content FROM nodes n
                 LEFT JOIN node_embeddings ne ON ne.node_id = n.id
                 WHERE n.type = 'routing_anchor' AND ne.node_id IS NULL"
            )?;
            let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
            Ok::<_, anyhow::Error>(rows.flatten().collect())
        })?;

        let total = missing.len();
        let mut done = 0usize;
        for (id, content) in missing {
            if let Ok(emb) = engine.embed(&content) {
                self.save_embedding(&id, &emb, "bge-m3", None).await
                    .map_err(|e| warn!("🌲 Reembed save failed for '{}': {}", id, e)).ok();
                done += 1;
            }
        }
        if total > 0 {
            tracing::info!("🌱 Anchor reembed: {}/{} embeddings generated", done, total);
        }
        Ok(done)
    }
}
