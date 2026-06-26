use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::params;
use serde_json;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

use super::{GraphNode, SilvaStats};
use crate::transport::http::McpSession;

impl super::SilvaDB {
    pub async fn record_tool_call(
        &self,
        agent_id: &str,
        tool_name: &str,
        guild_name: &str,
        success: bool,
        latency_ms: u64,
    ) -> Result<()> {
        let agent_id = agent_id.to_string();
        let tool_name = tool_name.to_string();
        let guild_name = guild_name.to_string();
        let silva_conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let conn = silva_conn.blocking_lock();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            conn.execute(
                "INSERT INTO node_traces (node_id, agent_id, touched_at, trace_type) VALUES (?1, ?2, ?3, ?4)",
                params![format!("agent:{}", agent_id), agent_id, now, format!("tool_call:{}", tool_name)],
            ).ok();

            conn.execute(
                "INSERT INTO node_traces (node_id, agent_id, touched_at, trace_type) VALUES (?1, ?2, ?3, ?4)",
                params![format!("guild:{}", guild_name), agent_id, now, "guild_call"],
            ).ok();

            conn.execute(
                "INSERT INTO guild_call_stats (guild_name, total_calls, successful_calls, total_latency_ms, last_call_unix)
                 VALUES (?1, 1, ?2, ?3, ?4)
                 ON CONFLICT(guild_name) DO UPDATE SET
                   total_calls = total_calls + 1,
                   successful_calls = successful_calls + ?2,
                   total_latency_ms = total_latency_ms + ?3,
                   last_call_unix = ?4",
                params![guild_name, if success { 1i64 } else { 0i64 }, latency_ms as i64, now],
            ).ok();

            Ok::<(), anyhow::Error>(())
        }).await.context("record_tool_call spawn_blocking failed")??;
        Ok(())
    }

    /// Consolidate similar episode nodes into concept nodes (R15-2).
    ///
    /// Loads up to `max_batch` episode nodes, computes pairwise token Jaccard similarity,
    /// and merges pairs above `similarity_threshold` into a single "concept" node.
    /// Original episodes are soft-deprecated to weight 0.02.
    pub async fn consolidate_episodes(
        &self,
        similarity_threshold: f64,
        max_batch: usize,
    ) -> Result<usize> {
        if max_batch == 0 {
            return Ok(0);
        }

        let episodes: Vec<(String, String, f64)> = tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT id, content, weight FROM nodes WHERE type = 'episode' ORDER BY weight DESC LIMIT ?1"
            )?;
            let rows = stmt.query_map(
                rusqlite::params![max_batch as i64],
                |row| {
                    let id: String = row.get(0)?;
                    let content: String = row.get(1)?;
                    let weight: f64 = row.get(2)?;
                    Ok((id, content, weight))
                },
            )?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row?);
            }
            Ok::<_, anyhow::Error>(results)
        })?;

        if episodes.len() < 2 {
            return Ok(0);
        }

        let mut merges = 0usize;
        let n = episodes.len();

        for i in 0..n {
            for j in (i + 1)..n {
                let sim = super::jaccard_similarity(&episodes[i].1, &episodes[j].1);
                if sim > similarity_threshold {
                    let id_a = &episodes[i].0;
                    let id_b = &episodes[j].0;
                    let weight_a = episodes[i].2;
                    let weight_b = episodes[j].2;

                    let combined = format!("{}:{}", id_a, id_b);
                    let hash: u64 = combined.bytes().fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
                    let concept_id = format!("concept:merged:{:x}", hash);

                    let content = if episodes[i].1.len() >= episodes[j].1.len() {
                        episodes[i].1.clone()
                    } else {
                        episodes[j].1.clone()
                    };

                    let merged_weight = ((weight_a + weight_b) / 2.0) * 1.15;
                    let meta = serde_json::json!({
                        "source": "consolidation",
                        "merged_from": [id_a, id_b],
                        "similarity": sim
                    }).to_string();

                    self.upsert_node(&concept_id, "concept", &content, &meta).await?;
                    self.set_weight(&concept_id, merged_weight).await?;

                    self.set_weight(id_a, 0.02).await?;
                    self.set_weight(id_b, 0.02).await?;

                    merges += 1;
                }
            }
        }

        Ok(merges)
    }

    /// Meta-Cognitive Pruning (GC cognitivo) for peripheral/isolated low-weight old nodes (R17-3).
    pub fn meta_cognitive_prune(
        &self,
        weight_threshold: f32,
        age_hours: u64,
        neighbor_activity_window_hours: u64,
    ) -> Result<usize> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            use std::time::{SystemTime, UNIX_EPOCH};
            let now_secs = SystemTime::now()
                .duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
            let age_cutoff = now_secs - (age_hours as i64 * 3600);
            let activity_cutoff = now_secs - (neighbor_activity_window_hours as i64 * 3600);

            let candidates: Vec<String> = {
                let mut stmt = conn.prepare(
                    "SELECT id FROM nodes
                     WHERE weight < ?1 AND (
                         CASE 
                             WHEN typeof(created_at) = 'integer' THEN created_at
                             WHEN typeof(created_at) = 'real' THEN created_at
                             ELSE CAST(strftime('%s', created_at) AS INTEGER)
                         END
                     ) < ?2
                       AND type NOT IN ('concept', 'lesson')
                       AND weight > 0.005
                     LIMIT 100"
                )?;
                stmt.query_map(params![weight_threshold, age_cutoff], |row| row.get(0))?
                    .filter_map(|r| r.ok()).collect()
            };

            let mut archived = 0;
            for node_id in candidates {
                let active_neighbor: bool = conn.query_row(
                    "SELECT COUNT(*) > 0 FROM edges e
                     JOIN nodes n ON (n.id = e.target OR n.id = e.source)
                     WHERE (e.source = ?1 OR e.target = ?1)
                       AND n.last_accessed > ?2",
                    params![node_id, activity_cutoff],
                    |row| row.get(0),
                ).unwrap_or(false);

                if !active_neighbor {
                    conn.execute(
                        "UPDATE nodes SET weight = weight * 0.01, type = 'archived'
                         WHERE id = ?1",
                        params![node_id],
                    )?;
                    archived += 1;
                }
            }
            Ok(archived)
        })
    }

    /// Truth Maintenance System — deprecate contradicting nodes on remember (R17-1).
    /// High-similarity (>0.80 cosine) nodes with different content and same prefix → deprecated.
    pub async fn deprecate_contradictions(&self, node_id: &str, embedding: &[f32], content: &str) -> Result<usize> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let prefix = node_id.splitn(4, ':').take(3).collect::<Vec<_>>().join(":");
            let candidates: Vec<(String, Vec<u8>, String)> = {
                let mut stmt = conn.prepare(
                    "SELECT n.id, ne.embedding, n.content FROM nodes n
                     JOIN node_embeddings ne ON ne.node_id = n.id
                     WHERE n.id LIKE ?1 AND n.weight > 0.05 AND n.id != ?2
                     LIMIT 20"
                )?;
                stmt.query_map(params![format!("{}%", prefix), node_id], |row| {
                    Ok((row.get::<_,String>(0)?, row.get::<_,Vec<u8>>(1)?, row.get::<_,String>(2)?))
                })?.filter_map(|r| r.ok()).collect()
            };
            let mut deprecated = 0;
            for (cid, cemb_bytes, ccontent) in candidates {
                if ccontent == content { continue; }
                let cemb_f: Vec<f32> = cemb_bytes.chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap_or([0u8;4])))
                    .collect();
                let a_bytes: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();
                let b_bytes: Vec<u8> = cemb_f.iter().flat_map(|f| f.to_le_bytes()).collect();
                let sim = super::cosine_similarity(&a_bytes, &b_bytes);
                if sim > 0.80 {
                    conn.execute(
                        "UPDATE nodes SET weight = weight * 0.1,
                         content = '[DEPRECATED by ' || ?2 || '] ' || content
                         WHERE id = ?1 AND weight > 0.05",
                        params![cid, node_id],
                    )?;
                    deprecated += 1;
                }
            }
            Ok::<usize, anyhow::Error>(deprecated)
        })
    }

    /// Get deprecated nodes (whose content starts with "[DEPRECATED by") ordered by weight ASC up to a limit (R18-3).
    pub async fn get_deprecated_nodes(&self, limit: usize) -> Result<Vec<GraphNode>> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT id, type, content, metadata, weight, protected, conflicted, topic_key, created_at, updated_at, shareable \
                 FROM nodes WHERE content LIKE '[DEPRECATED by%' ORDER BY weight ASC LIMIT ?1"
            )?;
            let rows = stmt.query_map(
                rusqlite::params![limit as i64],
                |row| Ok(GraphNode {
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
                    shareable: row.get::<_, i32>(10)? != 0,
                    last_touched: Utc::now(),
                    valid_from: None,
                    valid_until: None,
                }),
            )?;

            let mut results = Vec::new();
            for row in rows {
                results.push(row?);
            }
            Ok(results)
        })
    }

    pub async fn save_sessions(&self, sessions: &HashMap<String, McpSession>) -> Result<()> {
        let sessions: HashMap<String, McpSession> = sessions.clone();
        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            for session in sessions.values() {
                let _ = conn.execute(
                    "INSERT INTO mcp_sessions (id, agent_id, client_name, last_active_unix, tool_count, last_intent, last_guild, created_unix)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                     ON CONFLICT(agent_id) DO UPDATE SET
                         client_name = ?3, last_active_unix = ?4, tool_count = ?5, last_intent = ?6, last_guild = ?7, id = ?1",
                    rusqlite::params![
                        session.id, session.agent_id.as_deref().unwrap_or("anonymous"),
                        session.client_name, session.last_active_unix, session.tool_count as i64,
                        session.last_intent, session.last_guild, session.created_unix as i64
                    ],
                );
            }
        }).await.ok();
        Ok(())
    }
    pub async fn load_sessions(&self) -> Result<HashMap<String, McpSession>> {
        info!("🌲 SilvaDB: Loading persistent sessions...");
        let conn = Arc::clone(&self.conn);
        let result = tokio::task::spawn_blocking(move || -> Result<HashMap<String, McpSession>> {
            let conn = conn.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT id, agent_id, client_name, last_active_unix, tool_count, last_intent, last_guild, created_unix FROM mcp_sessions"
            )?;
            let rows: Vec<McpSession> = stmt.query_map([], |row: &rusqlite::Row| {
                Ok(McpSession {
                    id: row.get(0)?,
                    agent_id: row.get::<_, Option<String>>(1)?,
                    client_name: row.get(2)?,
                    last_active_unix: row.get::<_, i64>(3)? as u64,
                    tool_count: row.get::<_, i64>(4)? as u64,
                    last_intent: row.get(5)?,
                    last_guild: row.get(6)?,
                    created_unix: row.get::<_, i64>(7)? as u64,
                    created_at: std::time::Instant::now(),
                    last_active: std::time::Instant::now(),
                })
            })?.filter_map(|r: Result<McpSession, rusqlite::Error>| r.ok()).collect();
            let mut sessions = HashMap::new();
            for s in rows { sessions.insert(s.id.clone(), s); }
            Ok(sessions)
        }).await.context("Failed to spawn blocking task for sessions")??;
        info!("🌲 SilvaDB: Loaded {} persistent sessions.", result.len());
        Ok(result)
    }
    pub async fn get_detailed_stats(&self) -> Result<serde_json::Value> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let node_count: i64 = conn.query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))?;
            let edge_count: i64 = conn.query_row("SELECT COUNT(*) FROM edges", [], |r| r.get(0))?;
            let protected_count: i64 = conn.query_row("SELECT COUNT(*) FROM nodes WHERE protected = 1", [], |r| r.get(0))?;
            let conflicted_count: i64 = conn.query_row("SELECT COUNT(*) FROM nodes WHERE conflicted = 1", [], |r| r.get(0))?;
            let identity_count: i64 = conn.query_row("SELECT COUNT(*) FROM nodes WHERE type = 'identity'", [], |r| r.get(0))?;

            let mut by_type = serde_json::Map::new();
            {
                let mut stmt = conn.prepare("SELECT type, COUNT(*) as count FROM nodes GROUP BY type ORDER BY count DESC")?;
                let rows = stmt.query_map([], |r| {
                    let node_type: String = r.get(0)?;
                    let count: i64 = r.get(1)?;
                    Ok((node_type, count))
                })?;
                for (t, c) in rows.flatten() {
                    by_type.insert(t, serde_json::json!(c));
                }
            }

            // IVF Index stats: count centroids and check readiness
            let n_centroids: i64 = conn.query_row(
                "SELECT COUNT(*) FROM cluster_centroids",
                [],
                |r| r.get(0),
            ).unwrap_or(0);
            let ivf_ready = n_centroids > 0;

            // last_build: most recent centroid creation time (SQLite rowid as proxy, or created_at if available)
            let last_build: Option<i64> = conn.query_row(
                "SELECT MAX(rowid) FROM cluster_centroids",
                [],
                |r| r.get(0),
            ).ok().flatten();

            Ok(serde_json::json!({
                "node_count": node_count,
                "edge_count": edge_count,
                "protected_count": protected_count,
                "conflicted_count": conflicted_count,
                "identity_count": identity_count,
                "by_type": by_type,
                "ivf_ready": ivf_ready,
                "n_centroids": n_centroids,
                "last_build": last_build
            }))

        })
    }
    pub async fn apply_cleanup(&self, threshold: f32) -> Result<usize> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            // Delete nodes that are below threshold and NOT marked as protected/identity.
            let deleted = conn.execute(
                "DELETE FROM nodes WHERE weight < ?1 AND protected = 0",
                [threshold],
            )?;
            
            // Cleanup orphan edges that point to non-existent nodes
            let _ = conn.execute(
                "DELETE FROM edges WHERE source NOT IN (SELECT id FROM nodes) OR target NOT IN (SELECT id FROM nodes)",
                []
            )?;
            
            // Cleanup orphan embeddings
            let _ = conn.execute(
                "DELETE FROM node_embeddings WHERE node_id NOT IN (SELECT id FROM nodes)",
                []
            )?;
            
            Ok(deleted)
        })
    }
    pub async fn prune_dead_nodes(&self, min_weight: f32) -> Result<usize> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let deleted = conn.execute(
                "DELETE FROM nodes WHERE weight < ?1 AND protected = 0 AND type != 'identity' AND type != 'agent_summary'",
                [min_weight],
            )?;
            let _ = conn.execute(
                "DELETE FROM edges WHERE source NOT IN (SELECT id FROM nodes) OR target NOT IN (SELECT id FROM nodes)",
                [],
            )?;
            let _ = conn.execute(
                "DELETE FROM node_embeddings WHERE node_id NOT IN (SELECT id FROM nodes)",
                [],
            )?;
            if deleted > 0 {
                info!("🌲 prune_dead_nodes: removed {} nodes below weight {}", deleted, min_weight);
            }
            Ok(deleted)
        })
    }
    pub async fn purge_deprecated_lessons(&self) -> Result<usize> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let deleted = conn.execute(
                "DELETE FROM nodes WHERE type = ?1 AND id LIKE ?2 AND weight < ?3 AND protected = 0",
                params!["lesson", "lesson:intent:%", 0.15f32],
            )?;
            let _ = conn.execute(
                "DELETE FROM edges WHERE source NOT IN (SELECT id FROM nodes) OR target NOT IN (SELECT id FROM nodes)",
                [],
            )?;
            let _ = conn.execute(
                "DELETE FROM node_embeddings WHERE node_id NOT IN (SELECT id FROM nodes)",
                [],
            )?;
            if deleted > 0 {
                info!("🧹 Purged {} contaminated lesson nodes", deleted);
            }
            Ok(deleted)
        })
    }
    pub async fn edge_count(&self) -> Result<i64> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM edges", [], |row| row.get(0)
            )?;
            Ok(count)
        })
    }
    pub async fn stats(&self) -> Result<SilvaStats> {
        let (page_count, page_size) = tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let page_count: i64 = conn.query_row(
                "PRAGMA page_count", [], |row| row.get(0)
            )?;
            let page_size: i64 = conn.query_row(
                "PRAGMA page_size", [], |row| row.get(0)
            )?;
            Ok::<(i64, i64), anyhow::Error>((page_count, page_size))
        })?;
        Ok(SilvaStats {
            page_count,
            page_size,
            total_bytes: page_count * page_size,
            node_count: self.node_count().await? as i64,
            edge_count: self.edge_count().await?,
        })
    }
    pub async fn vacuum(&self) -> Result<()> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            conn.execute_batch("VACUUM")?;
            Ok::<(), anyhow::Error>(())
        })?;
        info!("🌲 SilvaDB vacuumed.");
        Ok(())
    }
    pub async fn top_hot_nodes(&self, limit: usize) -> Result<Vec<(String, u64)>> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT node_id, COUNT(*) as cnt FROM node_traces GROUP BY node_id ORDER BY cnt DESC LIMIT ?1"
            )?;
            let rows = stmt.query_map(params![limit as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)? as u64,
                ))
            })?;
            let mut results = Vec::new();
            for row in rows { results.push(row?); }
            Ok(results)
        })
    }

    pub async fn cleanup_orphan_nodes(&self) -> Result<usize> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let deleted = conn.execute(
                "DELETE FROM nodes 
                 WHERE id NOT IN (SELECT DISTINCT source FROM edges) 
                   AND id NOT IN (SELECT DISTINCT target FROM edges)
                   AND protected = 0 
                   AND type != 'identity' 
                   AND type != 'agent_summary'",
                [],
            )?;
            
            // Clean up their embeddings
            let _ = conn.execute(
                "DELETE FROM node_embeddings WHERE node_id NOT IN (SELECT id FROM nodes)",
                [],
            )?;
            
            if deleted > 0 {
                info!("🌲 cleanup_orphan_nodes: removed {} orphan nodes with no connections", deleted);
            }
            Ok(deleted)
        })
    }

    pub async fn orphan_node_count(&self) -> Result<i64> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM nodes 
                 WHERE id NOT IN (SELECT DISTINCT source FROM edges) 
                   AND id NOT IN (SELECT DISTINCT target FROM edges)
                   AND protected = 0 
                   AND type != 'identity' 
                   AND type != 'agent_summary'",
                [],
                |row| row.get(0),
            )?;
            Ok(count)
        })
    }
}
