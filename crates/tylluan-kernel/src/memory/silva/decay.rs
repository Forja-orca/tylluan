use anyhow::Result;
use rusqlite::params;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;
use tylluan_fsrs::FsrsItem;

impl super::SilvaDB {
    /// Apply FSRS-based retrievability decay to nodes, then prune dead memories.
    ///
    /// Replaces the old fixed half-life (`weight *= 0.5^(t / 14d)`) with
    /// **per-node stability**: retrievability = 2^(-elapsed_days / fsrs_stability).
    ///
    /// Nodes with retrievability below `prune_threshold` AND stability below
    /// a minimum are candidates for pruning.
    pub async fn apply_decay(&self, half_life_hours: u64) -> Result<usize> {
        tokio::task::block_in_place(|| {
            let mut conn = self.conn.blocking_lock();
            let now_unix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            // Step 1: Query nodes that need decay — those not accessed recently
            let nodes: Vec<(String, f64, f64, i64)> = {
                let mut stmt = conn.prepare(
                    "SELECT id, weight, fsrs_stability, fsrs_last_review
                     FROM nodes
                     WHERE type != 'identity' AND protected = 0
                     AND julianday('now') - julianday(updated_at) > 1"
                )?;
                stmt.query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, f64>(1)?,
                        row.get::<_, f64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                })?
                .filter_map(|r| r.ok())
                .collect()
            };
            // Step 2: Compute retrievability from FSRS, update weight
            let _hl = half_life_hours as f64;
            let node_changes = if !nodes.is_empty() {
                let tx = conn.transaction()?;
                let mut count = 0;
                {
                    let mut update_stmt = tx.prepare(
                        "UPDATE nodes SET weight = ?1 WHERE id = ?2"
                    )?;
                    for (id, weight, stability, last_review) in &nodes {
                        let item = FsrsItem::with_params(*stability, 0.3, *last_review);
                        let elapsed_days = if *last_review > 0 {
                            ((now_unix - last_review) as f64 / 86400.0).max(0.0)
                        } else {
                            0.0
                        };
                        let retrievability = item.retrievability(elapsed_days);

                        // Map retrievability 0..1 to weight 0.01..1.0
                        // R=1.0 → w=1.0, R=0.0 → w≈0.01
                        let new_weight = (retrievability * 0.99 + 0.01).max(weight.min(0.1));

                        update_stmt.execute(rusqlite::params![new_weight, id])?;
                        count += 1;
                    }
                }
                tx.commit()?;
                count
            } else {
                0
            };

            // Step 3: Prune dead memories
            let pruned = conn.execute(
                "DELETE FROM nodes
                 WHERE type NOT IN ('identity', 'agent_summary', 'session_digest', 'consolidated_summary')
                   AND protected = 0
                   AND weight < 0.15
                   AND julianday('now') - julianday(updated_at) > 30
                   AND fsrs_stability < 7.0",
                [],
            )?;

            // Step 4: Clean orphaned edges after node deletion
            let edge_cleanup = conn.execute(
                "DELETE FROM edges WHERE source NOT IN (SELECT id FROM nodes) OR target NOT IN (SELECT id FROM nodes)",
                [],
            )?;

            let total = node_changes + pruned + edge_cleanup;
            if total > 0 {
                info!("🌲 SilvaDB: FSRS decay applied. {} nodes recomputed, {} pruned, {} edges cleaned.", node_changes, pruned, edge_cleanup);
            }
            Ok(total)
        })
    }

    /// Apply long-term stability erosion to very old, unaccessed nodes.
    ///
    /// This is separate from FSRS retrievability decay. Where retrievability
    /// answers "how likely am I to recall this today?", stability erosion
    /// answers "has this memory structurally weakened from total disuse?"
    ///
    /// Erosion is applied per-node using Rust-side computation:
    ///   new_stability = stability * (1 - factor)^(elapsed_days / 30)
    ///
    /// Default factor is 0.01 (1% erosion per 30 days of no access) — about
    /// 1/70th the speed of retrievability decay, so it only dominates for
    /// memories untouched for years. Pass `factor = 0.0` to disable entirely.
    pub async fn apply_stability_erosion(&self, factor: f64) -> Result<usize> {
        tokio::task::block_in_place(|| {
            let mut conn = self.conn.blocking_lock();
            let now_unix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            if factor <= 0.0 {
                return Ok(0);
            }

            // Select nodes with ancient last_review
            let candidates: Vec<(String, f64, i64)> = {
                let mut stmt = conn.prepare(
                    "SELECT id, fsrs_stability, fsrs_last_review
                     FROM nodes
                     WHERE type != 'identity' AND protected = 0
                       AND fsrs_last_review > 0
                       AND ?1 - fsrs_last_review > 60 * 86400"
                )?;
                stmt.query_map(params![now_unix], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, f64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                })?
                .filter_map(|r| r.ok())
                .collect()
            };

            if candidates.is_empty() {
                return Ok(0);
            }

            let tx = conn.transaction()?;
            let mut count = 0;
            {
                let mut update = tx.prepare(
                    "UPDATE nodes SET fsrs_stability = ?1 WHERE id = ?2"
                )?;
                for (id, stability, last_review) in &candidates {
                    let elapsed_days = (now_unix - last_review) as f64 / 86400.0;
                    let periods = (elapsed_days / 30.0).max(1.0);
                    let new_stability = (stability * (1.0 - factor).powf(periods)).max(1.0);
                    update.execute(params![new_stability, id])?;
                    count += 1;
                }
            }
            tx.commit()?;

            if count > 0 {
                info!("🧊 stability erosion: {} nodes eroded (factor={})", count, factor);
            }
            Ok(count)
        })
    }

    /// Remove node_traces older than `keep_days` days. Returns count of deleted rows.
    pub async fn prune_old_traces(&self, keep_days: i64) -> Result<usize> {
        let conn = Arc::clone(&self.conn);
        let result = tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let deleted = conn.execute(
                "DELETE FROM node_traces WHERE touched_at < (strftime('%s', 'now') - ?1)",
                rusqlite::params![keep_days * 86400],
            );
            match deleted {
                Ok(count) => {
                    if count > 0 {
                        info!("🧹 pruned {} old node_traces (>{} days)", count, keep_days);
                    }
                    Ok::<usize, anyhow::Error>(count)
                }
                Err(e) => Err(anyhow::anyhow!("prune_old_traces failed: {e}")),
            }
        })
        .await
        .map_err(|e| anyhow::anyhow!("prune_old_traces spawn failed: {e}"))??;
        Ok(result)
    }

    /// Record that an agent touched a node. Creates a stigmergy trace entry,
    /// updates the node's last_touched timestamp, and applies an FSRS review
    /// (Good rating) to boost stability on successful access.
    ///
    /// For v2, use `touch_node_with_rating` to pass an explicit Rating (e.g.
    /// from retrieval score).
    pub async fn touch_node(&self, node_id: &str, agent_id: &str, trace_type: &str) -> Result<()> {
        self.touch_node_with_rating(node_id, agent_id, trace_type, None).await
    }

    /// Like `touch_node`, but accepts an optional `Rating` for the FSRS review.
    /// When `None`, defaults to `Rating::Good`.
    pub async fn touch_node_with_rating(
        &self,
        node_id: &str,
        agent_id: &str,
        trace_type: &str,
        rating: Option<tylluan_fsrs::Rating>,
    ) -> Result<()> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            conn.execute(
                "INSERT INTO node_traces (node_id, agent_id, touched_at, trace_type) VALUES (?1, ?2, ?3, ?4)",
                params![node_id, agent_id, now, trace_type],
            )?;
            conn.execute(
                "UPDATE nodes SET last_touched = ?1 WHERE id = ?2",
                params![now, node_id],
            )?;

            // FSRS review: every access to a node triggers a review.
            // Default is Good; callers can pass an explicit Rating when they
            // have signal (e.g. retrieval score → Easy for top-1, Hard for low).
            let rating = rating.unwrap_or(tylluan_fsrs::Rating::Good);
            if let Ok((fsrs_stability, fsrs_difficulty, fsrs_last_review)) =
                conn.query_row::<(f64, f64, i64), _, _>(
                    "SELECT fsrs_stability, fsrs_difficulty, fsrs_last_review FROM nodes WHERE id = ?1",
                    params![node_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
            {
                let mut item = tylluan_fsrs::FsrsItem::with_params(
                    fsrs_stability, fsrs_difficulty, fsrs_last_review,
                );
                item.review(rating, now);
                let _ = conn.execute(
                    "UPDATE nodes SET fsrs_stability = ?1, fsrs_difficulty = ?2, fsrs_last_review = ?3 WHERE id = ?4",
                    params![item.stability, item.difficulty, item.last_review, node_id],
                );
            }
            // Stigmergy: propagate heat to direct neighbors (1 hop, up to 10).
            // Each neighbor gets a "stigmergy_propagation" trace — lighter signal than
            // a direct touch. This implements pheromone diffusion: accessing a node
            // reinforces its semantic neighborhood (arXiv 2408.14285).
            let mut neighbor_stmt = conn.prepare_cached(
                "SELECT DISTINCT CASE WHEN source = ?1 THEN target ELSE source END as neighbor
                 FROM edges WHERE (source = ?1 OR target = ?1) LIMIT 10"
            )?;
            let neighbors: Vec<String> = neighbor_stmt
                .query_map(params![node_id], |r| r.get(0))?
                .filter_map(|r| r.ok())
                .collect();
            for neighbor_id in neighbors {
                let _ = conn.execute(
                    "INSERT INTO node_traces (node_id, agent_id, touched_at, trace_type) VALUES (?1, ?2, ?3, 'stigmergy_propagation')",
                    params![neighbor_id, agent_id, now],
                );
            }

            // Semantic heat propagation: find embedding-similar nodes
            let embedding_result = conn.query_row(
                "SELECT embedding FROM node_embeddings WHERE node_id = ?1 LIMIT 1",
                params![node_id],
                |r| r.get::<_, Vec<u8>>(0),
            );
            if let Ok(vec_bytes) = embedding_result {
                // Load up to 20 candidate nodes with embeddings (excluding self)
                let mut candidates = conn.prepare_cached(
                    "SELECT e.node_id, e.embedding FROM node_embeddings e
                     WHERE e.node_id != ?1
                     ORDER BY e.rowid DESC LIMIT 20"
                )?;
                let similar: Vec<(String, f64)> = candidates
                    .query_map(params![node_id], |r| {
                        let nid: String = r.get(0)?;
                        let vec: Vec<u8> = r.get(1)?;
                        Ok((nid, vec))
                    })?
                    .filter_map(|r| r.ok())
                    .filter_map(|(nid, candidate_vec)| {
                        let sim = super::cosine_similarity(&vec_bytes, &candidate_vec);
                        if sim > 0.85 { Some((nid, sim)) } else { None }
                    })
                    .take(5)
                    .collect();
                for (similar_id, sim) in &similar {
                    // sim 0.85→1.0 mapea a 1→3 traces (heat difuso proporcional)
                    let trace_count = (((sim - 0.85) / 0.15 * 2.0).round() as u32 + 1).min(3);
                    for _ in 0..trace_count {
                        let _ = conn.execute(
                            "INSERT INTO node_traces (node_id, agent_id, touched_at, trace_type)
                             VALUES (?1, ?2, ?3, 'diffuse_heat')",
                            params![similar_id, agent_id, now],
                        );
                    }
                }
            }

            Ok::<(), anyhow::Error>(())
        })?;
        Ok(())
    }

    /// Calculate the heat (trace frequency) for a given node within a time window.
    /// Heat = count / (hours * 10), capped at 2.0 (1.0 = 10 touches per hour).
    pub async fn get_stigmergy_heat(&self, node_id: &str, window_hours: u64) -> Result<f64> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            // touched_at is i64 seconds
            let sql = format!(
                "SELECT COUNT(*) FROM node_traces WHERE node_id = ?1 AND touched_at >= (strftime('%s', 'now') - {})",
                window_hours * 3600
            );
            let count: i64 = conn.query_row(&sql, params![node_id], |r| r.get(0))?;
            // Normalize heat: 1.0 = 10 touches per hour, capped at 2.0
            let hours = window_hours as f64;
            let heat = (count as f64 / (hours * 10.0)).min(2.0);
            Ok(heat)
        })
    }

    /// Batch heat calculation for multiple nodes.
    pub async fn get_heat_batch(&self, node_ids: &[String], window_hours: u64) -> Result<HashMap<String, f64>> {
        if node_ids.is_empty() {
            return Ok(HashMap::new());
        }

        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let since = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64 - (window_hours * 3600) as i64;

            let mut heats = HashMap::new();
            let hours = window_hours as f64;

            for chunk in node_ids.chunks(500) {
                let placeholders = std::iter::repeat_n("?", chunk.len())
                    .collect::<Vec<_>>()
                    .join(",");

                let sql = format!(
                    "SELECT node_id, COUNT(*) as c FROM node_traces \
                     WHERE node_id IN ({placeholders}) AND touched_at >= ? \
                     GROUP BY node_id"
                );

                let mut params: Vec<rusqlite::types::Value> = chunk
                    .iter()
                    .map(|id| rusqlite::types::Value::Text(id.clone()))
                    .collect();
                params.push(rusqlite::types::Value::Integer(since));

                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(rusqlite::params_from_iter(params), |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                })?;

                for row in rows {
                    let (node_id, count) = row?;
                    let heat = (count as f64 / (hours * 10.0)).min(2.0);
                    heats.insert(node_id, heat);
                }
            }

            Ok(heats)
        })
    }

    /// For each node, find which agent touched it most recently within the time window.
    pub async fn get_active_agents_batch(&self, node_ids: &[String], window_hours: u64) -> Result<HashMap<String, String>> {
        if node_ids.is_empty() {
            return Ok(HashMap::new());
        }

        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let since = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64 - (window_hours * 3600) as i64;

            let mut active_agents = HashMap::new();

            for chunk in node_ids.chunks(500) {
                let placeholders = std::iter::repeat_n("?", chunk.len())
                    .collect::<Vec<_>>()
                    .join(",");

                let sql = format!(
                    "SELECT t.node_id, t.agent_id FROM node_traces t \
                     INNER JOIN ( \
                         SELECT node_id, MAX(touched_at) as max_touch FROM node_traces \
                         WHERE node_id IN ({placeholders}) AND touched_at >= ? \
                         GROUP BY node_id \
                     ) m ON t.node_id = m.node_id AND t.touched_at = m.max_touch"
                );

                let mut params: Vec<rusqlite::types::Value> = chunk
                    .iter()
                    .map(|id| rusqlite::types::Value::Text(id.clone()))
                    .collect();
                params.push(rusqlite::types::Value::Integer(since));

                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(rusqlite::params_from_iter(params), |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?;

                for row in rows {
                    let (node_id, agent_id) = row?;
                    if !agent_id.trim().is_empty() && agent_id != "anonymous" {
                        active_agents.insert(node_id, agent_id);
                    }
                }
            }

            Ok(active_agents)
        })
    }

    /// Count the number of traces of a specific type in the given window.
    pub async fn count_recent_traces(&self, node_id: &str, trace_type: &str, window_hours: u64) -> Result<usize> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let sql = "SELECT COUNT(*) FROM node_traces WHERE node_id = ?1 AND trace_type = ?2 AND touched_at >= (strftime('%s', 'now') - ?3)";
            let count: i64 = conn.query_row(sql, params![node_id, trace_type, (window_hours * 3600) as i64], |r| r.get(0))?;
            Ok(count as usize)
        })
    }

    /// Remove nodes with salience_score below threshold.
    ///
    /// Durable summaries are intentionally excluded: their purpose is to keep
    /// a recoverable memory floor after source episodes cool down. They can
    /// still be removed explicitly through an operator-controlled cleanup.
    pub async fn prune_by_salience(&self, threshold: f64) -> Result<usize> {
        tokio::task::block_in_place(|| {
            let mut conn = self.conn.blocking_lock();
            let ids: Vec<String> = {
                let mut stmt = conn.prepare(
                    "SELECT id FROM nodes
                     WHERE protected = 0
                       AND type NOT IN ('identity', 'agent_summary', 'session_digest', 'consolidated_summary')
                       AND salience_score < ?1"
                )?;
                stmt.query_map(params![threshold], |r| r.get::<_, String>(0))?
                    .filter_map(|r| r.ok())
                    .collect()
            };
            let count = ids.len();
            if count > 0 {
                let tx = conn.transaction()?;
                for id in &ids {
                    tx.execute("DELETE FROM edges WHERE source = ?1 OR target = ?1", params![id])?;
                    tx.execute("DELETE FROM node_traces WHERE node_id = ?1", params![id])?;
                    tx.execute("DELETE FROM node_embeddings WHERE node_id = ?1", params![id])?;
                    tx.execute("DELETE FROM nodes WHERE id = ?1", params![id])?;
                }
                tx.commit()?;
                tracing::info!("🧹 pruned {} nodes with salience < {}", count, threshold);
            }
            Ok(count)
        })
    }

    /// Remove nodes with weight below threshold (ignoring protected nodes).
    pub async fn prune_cold_nodes(&self, threshold_weight: f64) -> Result<usize> {
        tokio::task::block_in_place(|| {
            let mut conn = self.conn.blocking_lock();
            
            // Find nodes to delete (not protected, weight < threshold, not identity)
            let ids: Vec<String> = {
                let mut stmt = conn.prepare(
                    "SELECT id FROM nodes
                     WHERE protected = 0
                       AND weight < ?1
                       AND type NOT IN ('identity', 'agent_summary', 'session_digest', 'consolidated_summary')"
                )?;
                stmt.query_map(params![threshold_weight], |r| r.get(0))?
                    .filter_map(|r| r.ok())
                    .collect()
            };
            
            let count = ids.len();
            if count > 0 {
                let tx = conn.transaction()?;
                // Delete edges associated with these nodes
                for id in &ids {
                    tx.execute("DELETE FROM edges WHERE source = ?1 OR target = ?1", params![id])?;
                    tx.execute("DELETE FROM node_traces WHERE node_id = ?1", params![id])?;
                    tx.execute("DELETE FROM nodes WHERE id = ?1", params![id])?;
                }
                tx.commit()?;
            }
            
            Ok(count)
        })
    }

}
