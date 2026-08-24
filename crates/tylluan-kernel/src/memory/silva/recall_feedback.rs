//! ADR-011 Signal Loop — implicit usefulness feedback for `tylluan_recall` results.
//!
//! Two halves: `log_recall_feedback` writes one pending row per memory returned
//! by a recall call (called from `handler_recall.rs`); `resolve_pending_feedback`
//! later scans `guild_audit_log` for that agent's next actions and marks each
//! row useful/not-useful by word-overlap between the memory's content and the
//! agent's subsequent intents. This is a heuristic proxy signal, not ground
//! truth — the audit chain shows what happened, not why. Documented as such in
//! ADR-011 rather than treated as an infallible label.

use super::SilvaDB;
use super::jaccard_similarity;
use anyhow::Result;

/// Minimum word-overlap between a recalled memory's content and a subsequent
/// intent to count as "this memory was referenced" — same order of magnitude
/// as the pre-filter threshold `dream_cycle.rs` uses before falling back to
/// real cosine similarity.
const REFERENCE_OVERLAP_THRESHOLD: f64 = 0.15;

/// How many of an agent's next `tylluan_do` calls count as the resolution window.
const RESOLUTION_WINDOW: usize = 3;

impl SilvaDB {
    /// Records one pending feedback row for a memory returned by recall.
    /// `task_hash` groups all memories returned for the same query/turn.
    /// Idempotent per (memory_id, task_hash) — a repeated recall of the same
    /// memory for the same task does not duplicate the row.
    pub async fn log_recall_feedback(
        &self,
        memory_id: &str,
        agent_id: &str,
        task_hash: &str,
        query_text: &str,
        rank_position: i64,
    ) -> Result<()> {
        if agent_id.is_empty() {
            return Ok(()); // no agent_id -> nothing to correlate against later
        }
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            conn.execute(
                "INSERT OR IGNORE INTO recall_feedback \
                 (memory_id, agent_id, task_hash, query_text, rank_position) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![memory_id, agent_id, task_hash, query_text, rank_position],
            )?;
            Ok::<(), anyhow::Error>(())
        })
    }

    /// Resolves pending (`useful = 0`) feedback rows older than `min_age_secs`
    /// against `guild_audit_log`: for each row, looks at that agent's next
    /// `RESOLUTION_WINDOW` audit entries after `accessed_at`. If any entry's
    /// `intent` overlaps the recalled memory's content above threshold, marks
    /// `useful = 1`; otherwise `useful = -1`. Returns (resolved_useful, resolved_not_useful).
    pub async fn resolve_pending_feedback(
        &self,
        audit_db_path: &str,
        min_age_secs: i64,
    ) -> Result<(usize, usize)> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::seconds(min_age_secs)).to_rfc3339();

        let pending: Vec<(i64, String, String, String)> = tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT id, memory_id, agent_id, accessed_at FROM recall_feedback \
                 WHERE useful = 0 AND accessed_at < ?1",
            )?;
            let rows = stmt.query_map(rusqlite::params![cutoff], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))
            })?;
            Ok::<_, anyhow::Error>(rows.flatten().collect())
        })?;

        if pending.is_empty() {
            return Ok((0, 0));
        }

        let audit_conn = match crate::config::open_db(std::path::Path::new(audit_db_path)) {
            Ok(c) => c,
            Err(_) => return Ok((0, 0)), // no audit log yet -> nothing resolvable
        };

        let mut useful_count = 0usize;
        let mut not_useful_count = 0usize;

        for (row_id, memory_id, agent_id, accessed_at) in pending {
            let content = match self.get_node(&memory_id).await {
                Ok(Some(node)) => node.content,
                _ => continue, // memory deleted/decayed since recall -> leave unresolved
            };

            // Scoped in its own block so `stmt` (borrows `audit_conn`, whose
            // rusqlite internals aren't Send) goes out of LEXICAL scope before
            // the `.await` below. A bare `drop(stmt)` is not enough here --
            // rustc's async-fn state machine captures a variable across an
            // await point based on lexical liveness, not just logical drop
            // order, so an explicit block is the reliable fix. Found as a real
            // compile error when wiring this into NightConsolidation via
            // #[async_trait]'s Send-bound future in FeedbackSignalPhase::run.
            let intents: Vec<String> = {
                let mut stmt = audit_conn.prepare(
                    "SELECT intent FROM guild_audit_log \
                     WHERE agent_id = ?1 AND timestamp > ?2 \
                     ORDER BY timestamp ASC LIMIT ?3",
                )?;
                stmt.query_map(rusqlite::params![agent_id, accessed_at, RESOLUTION_WINDOW as i64], |r| {
                    r.get::<_, Option<String>>(0)
                })?
                    .flatten()
                    .flatten()
                    .collect()
            };

            let referenced = intents.iter().any(|intent| jaccard_similarity(&content, intent) >= REFERENCE_OVERLAP_THRESHOLD);
            let useful = if referenced { 1 } else { -1 };
            if referenced { useful_count += 1 } else { not_useful_count += 1 }

            tokio::task::block_in_place(|| {
                let conn = self.conn.blocking_lock();
                conn.execute(
                    "UPDATE recall_feedback SET useful = ?1, resolved_at = datetime('now') WHERE id = ?2",
                    rusqlite::params![useful, row_id],
                )
            })?;

            // ADR-012/ADR-011 integration: confirmed real usage (useful=1) is a
            // stronger signal than the passive time-based active->quiet trigger
            // that ADR-012 D2 uses. Route it through the same atomic,
            // quarantine-safe path recall/remember already use -- this
            // reactivates an archived memory the agent actually acted on, and
            // refreshes last_agent_access for a quiet one, using real proof of
            // usefulness instead of mere access. Deliberately NOT symmetric:
            // useful=-1 does NOT downgrade lifecycle_state here. ADR-012 §8.2
            // treats "not referenced afterward" as weak evidence (the agent may
            // not have needed it yet, not that it's useless) -- punitive
            // demotion on a heuristic proxy signal is a real policy decision
            // this integration does not make on its own.
            if referenced {
                let now = chrono::Utc::now().timestamp();
                let _ = self.record_agent_access(&memory_id, now).await;
            }
        }

        Ok((useful_count, not_useful_count))
    }

    /// Count of resolved feedback rows (useful != 0) — the gate NightConsolidation
    /// and the future LightReranker trainer check against ADR-011's minimum
    /// training-data threshold (5,000 rows).
    pub async fn resolved_feedback_count(&self) -> Result<i64> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            conn.query_row("SELECT COUNT(*) FROM recall_feedback WHERE useful != 0", [], |r| r.get(0))
                .map_err(anyhow::Error::from)
        })
    }

    /// Count of feedback rows still awaiting resolution (`useful == 0`) — the
    /// complement of `resolved_feedback_count`, for dashboard observability.
    pub async fn pending_feedback_count(&self) -> Result<i64> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            conn.query_row("SELECT COUNT(*) FROM recall_feedback WHERE useful = 0", [], |r| r.get(0))
                .map_err(anyhow::Error::from)
        })
    }

    /// Compute agent affinity for a specific memory: the fraction of resolved
    /// feedback rows where this memory was useful to this specific agent.
    /// Falls back to global affinity (all agents) if no agent-specific data,
    /// then to 0.0 for completely new memories. Returns a value in [0.0, 1.0].
    pub async fn agent_affinity_for_memory(&self, memory_id: &str, agent_id: &str) -> Result<f32> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();

            // Try agent-specific first (COALESCE handles NULL from empty result sets)
            let agent_result: (i64, i64) = conn.query_row(
                "SELECT
                    COALESCE(SUM(CASE WHEN useful > 0 THEN 1 ELSE 0 END), 0),
                    COUNT(*)
                 FROM recall_feedback
                 WHERE memory_id = ?1 AND agent_id = ?2 AND useful != 0",
                rusqlite::params![memory_id, agent_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            ).map_err(anyhow::Error::from)?;

            if agent_result.1 > 0 {
                return Ok(agent_result.0 as f32 / agent_result.1 as f32);
            }

            // Fall back to global (all agents)
            let global_result: (i64, i64) = conn.query_row(
                "SELECT
                    COALESCE(SUM(CASE WHEN useful > 0 THEN 1 ELSE 0 END), 0),
                    COUNT(*)
                 FROM recall_feedback
                 WHERE memory_id = ?1 AND useful != 0",
                rusqlite::params![memory_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            ).map_err(anyhow::Error::from)?;

            if global_result.1 > 0 {
                return Ok(global_result.0 as f32 / global_result.1 as f32);
            }

            Ok(0.0)
        })
    }

    /// CoherenceGate P4-P2: bulk lookup of resolved post-hoc outcomes for a set
    /// of node ids, used to enrich `llm_decision_examples` exports with real
    /// ground truth instead of only the teacher-vs-gate agreement measured in
    /// phase 1. Same honesty caveat as the rest of this module: this is the
    /// word-overlap heuristic proxy signal from ADR-011, not infallible truth
    /// — it answers "was this memory referenced again afterward", not "was the
    /// gate's decision correct" in any deeper sense.
    ///
    /// Returns memory_id -> latest resolved `useful` value (1 or -1). A node
    /// with no resolved row (still pending, or never recalled) is simply
    /// absent from the map — callers must treat missing as "no ground truth
    /// yet", not as a negative label.
    pub async fn get_resolved_feedback_map(&self, memory_ids: &[String]) -> Result<std::collections::HashMap<String, i64>> {
        if memory_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let placeholders = memory_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT memory_id, useful FROM recall_feedback \
                 WHERE useful != 0 AND memory_id IN ({placeholders}) \
                 ORDER BY resolved_at ASC"
            );
            let mut stmt = conn.prepare(&sql)?;
            let params: Vec<&dyn rusqlite::ToSql> = memory_ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
            let rows = stmt.query_map(params.as_slice(), |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?;
            // ORDER BY resolved_at ASC + insert-overwrite: last write wins, so
            // the map ends up holding each memory_id's most recent resolution.
            let mut map = std::collections::HashMap::new();
            for row in rows.flatten() {
                map.insert(row.0, row.1);
            }
            Ok(map)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn log_recall_feedback_is_idempotent_per_task() {
        let db = SilvaDB::in_memory().await.unwrap();
        db.log_recall_feedback("mem1", "agent-a", "task-1", "query text", 0).await.unwrap();
        db.log_recall_feedback("mem1", "agent-a", "task-1", "query text", 0).await.unwrap();
        let count: i64 = tokio::task::block_in_place(|| {
            let conn = db.conn.blocking_lock();
            conn.query_row("SELECT COUNT(*) FROM recall_feedback", [], |r| r.get(0)).unwrap()
        });
        assert_eq!(count, 1, "repeated recall of same memory for same task must not duplicate");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn log_recall_feedback_skips_empty_agent_id() {
        let db = SilvaDB::in_memory().await.unwrap();
        db.log_recall_feedback("mem1", "", "task-1", "query text", 0).await.unwrap();
        let count: i64 = tokio::task::block_in_place(|| {
            let conn = db.conn.blocking_lock();
            conn.query_row("SELECT COUNT(*) FROM recall_feedback", [], |r| r.get(0)).unwrap()
        });
        assert_eq!(count, 0, "no agent_id means no way to correlate later -> nothing logged");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn resolve_pending_feedback_marks_referenced_memory_useful() {
        let db = SilvaDB::in_memory().await.unwrap();
        db.upsert_node("mem1", "concept", "how to configure the deployment pipeline", "{}").await.unwrap();

        let old_ts = (chrono::Utc::now() - chrono::Duration::seconds(120)).to_rfc3339();
        tokio::task::block_in_place(|| {
            let conn = db.conn.blocking_lock();
            conn.execute(
                "INSERT INTO recall_feedback (memory_id, agent_id, task_hash, query_text, rank_position, accessed_at) \
                 VALUES ('mem1', 'agent-a', 'task-1', 'deployment pipeline', 0, ?1)",
                rusqlite::params![old_ts],
            ).unwrap();
        });

        let tmp = std::env::temp_dir().join(format!("test_recall_feedback_audit_{}", uuid::Uuid::new_v4().simple()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let audit_path = tmp.join("audit.db");
        {
            let conn = crate::config::open_db(&audit_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE guild_audit_log (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp TEXT, guild TEXT, tool_name TEXT, agent_id TEXT, intent TEXT, status TEXT);"
            ).unwrap();
            let now = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO guild_audit_log (timestamp,guild,tool_name,agent_id,intent,status) VALUES (?1,'bash','run','agent-a','configure the deployment pipeline now','ok')",
                rusqlite::params![now],
            ).unwrap();
        }

        let (useful, not_useful) = db.resolve_pending_feedback(&audit_path.to_string_lossy(), 60).await.unwrap();
        assert_eq!(useful, 1, "memory content overlaps the following intent -> useful");
        assert_eq!(not_useful, 0);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn resolve_pending_feedback_marks_unreferenced_memory_not_useful() {
        let db = SilvaDB::in_memory().await.unwrap();
        db.upsert_node("mem1", "concept", "completely unrelated content about cooking recipes", "{}").await.unwrap();

        let old_ts = (chrono::Utc::now() - chrono::Duration::seconds(120)).to_rfc3339();
        tokio::task::block_in_place(|| {
            let conn = db.conn.blocking_lock();
            conn.execute(
                "INSERT INTO recall_feedback (memory_id, agent_id, task_hash, query_text, rank_position, accessed_at) \
                 VALUES ('mem1', 'agent-a', 'task-1', 'cooking', 0, ?1)",
                rusqlite::params![old_ts],
            ).unwrap();
        });

        let tmp = std::env::temp_dir().join(format!("test_recall_feedback_audit_{}", uuid::Uuid::new_v4().simple()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let audit_path = tmp.join("audit.db");
        {
            let conn = crate::config::open_db(&audit_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE guild_audit_log (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp TEXT, guild TEXT, tool_name TEXT, agent_id TEXT, intent TEXT, status TEXT);"
            ).unwrap();
            let now = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO guild_audit_log (timestamp,guild,tool_name,agent_id,intent,status) VALUES (?1,'git','commit','agent-a','deploy the kernel binary','ok')",
                rusqlite::params![now],
            ).unwrap();
        }

        let (useful, not_useful) = db.resolve_pending_feedback(&audit_path.to_string_lossy(), 60).await.unwrap();
        assert_eq!(useful, 0);
        assert_eq!(not_useful, 1, "no overlap with any subsequent intent -> not useful");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn resolve_pending_feedback_reactivates_archived_memory_confirmed_useful() {
        let db = SilvaDB::in_memory().await.unwrap();
        db.upsert_node("mem1", "concept", "how to configure the deployment pipeline", "{}").await.unwrap();
        tokio::task::block_in_place(|| {
            let conn = db.conn.blocking_lock();
            conn.execute("UPDATE nodes SET lifecycle_state = 'archived' WHERE id = 'mem1'", []).unwrap();
        });

        let old_ts = (chrono::Utc::now() - chrono::Duration::seconds(120)).to_rfc3339();
        tokio::task::block_in_place(|| {
            let conn = db.conn.blocking_lock();
            conn.execute(
                "INSERT INTO recall_feedback (memory_id, agent_id, task_hash, query_text, rank_position, accessed_at) \
                 VALUES ('mem1', 'agent-a', 'task-1', 'deployment pipeline', 0, ?1)",
                rusqlite::params![old_ts],
            ).unwrap();
        });

        let tmp = std::env::temp_dir().join(format!("test_recall_feedback_audit_{}", uuid::Uuid::new_v4().simple()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let audit_path = tmp.join("audit.db");
        {
            let conn = crate::config::open_db(&audit_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE guild_audit_log (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp TEXT, guild TEXT, tool_name TEXT, agent_id TEXT, intent TEXT, status TEXT);"
            ).unwrap();
            let now = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO guild_audit_log (timestamp,guild,tool_name,agent_id,intent,status) VALUES (?1,'bash','run','agent-a','configure the deployment pipeline now','ok')",
                rusqlite::params![now],
            ).unwrap();
        }

        let (useful, _) = db.resolve_pending_feedback(&audit_path.to_string_lossy(), 60).await.unwrap();
        assert_eq!(useful, 1);

        let state: String = tokio::task::block_in_place(|| {
            let conn = db.conn.blocking_lock();
            conn.query_row("SELECT lifecycle_state FROM nodes WHERE id = 'mem1'", [], |r| r.get(0)).unwrap()
        });
        assert_eq!(state, "active", "real usage confirmation must reactivate an archived memory");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn resolve_pending_feedback_does_not_downgrade_lifecycle_on_not_useful() {
        let db = SilvaDB::in_memory().await.unwrap();
        db.upsert_node("mem1", "concept", "completely unrelated content about cooking recipes", "{}").await.unwrap();

        let old_ts = (chrono::Utc::now() - chrono::Duration::seconds(120)).to_rfc3339();
        tokio::task::block_in_place(|| {
            let conn = db.conn.blocking_lock();
            conn.execute(
                "INSERT INTO recall_feedback (memory_id, agent_id, task_hash, query_text, rank_position, accessed_at) \
                 VALUES ('mem1', 'agent-a', 'task-1', 'cooking', 0, ?1)",
                rusqlite::params![old_ts],
            ).unwrap();
        });

        // Must be a real, openable audit DB with no matching rows -- an
        // unopenable path makes resolve_pending_feedback bail out early with
        // Ok((0, 0)) for the whole batch (found as a real test bug: this test
        // originally used a nonexistent path expecting not_useful=1, which
        // never happened because the function short-circuits before scoring
        // any row).
        let tmp = std::env::temp_dir().join(format!("test_recall_feedback_audit_{}", uuid::Uuid::new_v4().simple()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let audit_path = tmp.join("audit.db");
        {
            let conn = crate::config::open_db(&audit_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE guild_audit_log (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp TEXT, guild TEXT, tool_name TEXT, agent_id TEXT, intent TEXT, status TEXT);"
            ).unwrap();
        }

        let (_, not_useful) = db.resolve_pending_feedback(&audit_path.to_string_lossy(), 60).await.unwrap();
        assert_eq!(not_useful, 1);
        let _ = std::fs::remove_dir_all(&tmp);

        let state: String = tokio::task::block_in_place(|| {
            let conn = db.conn.blocking_lock();
            conn.query_row("SELECT lifecycle_state FROM nodes WHERE id = 'mem1'", [], |r| r.get(0)).unwrap()
        });
        assert_eq!(state, "active", "a not-useful heuristic signal must not demote lifecycle_state -- that policy is not decided yet");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn resolve_pending_feedback_ignores_rows_within_window() {
        let db = SilvaDB::in_memory().await.unwrap();
        db.upsert_node("mem1", "concept", "recent memory not yet in resolution window", "{}").await.unwrap();
        db.log_recall_feedback("mem1", "agent-a", "task-1", "query", 0).await.unwrap();

        let (useful, not_useful) = db.resolve_pending_feedback("/nonexistent/audit.db", 60).await.unwrap();
        assert_eq!(useful, 0);
        assert_eq!(not_useful, 0, "row is younger than min_age_secs -> must stay pending");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn resolved_feedback_count_only_counts_resolved_rows() {
        let db = SilvaDB::in_memory().await.unwrap();
        db.log_recall_feedback("mem1", "agent-a", "task-1", "q", 0).await.unwrap();
        assert_eq!(db.resolved_feedback_count().await.unwrap(), 0);

        tokio::task::block_in_place(|| {
            let conn = db.conn.blocking_lock();
            conn.execute("UPDATE recall_feedback SET useful = 1 WHERE memory_id = 'mem1'", []).unwrap();
        });
        assert_eq!(db.resolved_feedback_count().await.unwrap(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn get_resolved_feedback_map_excludes_pending_and_missing() {
        let db = SilvaDB::in_memory().await.unwrap();
        db.log_recall_feedback("mem-resolved", "agent-a", "task-1", "q", 0).await.unwrap();
        db.log_recall_feedback("mem-pending", "agent-a", "task-2", "q", 0).await.unwrap();
        tokio::task::block_in_place(|| {
            let conn = db.conn.blocking_lock();
            conn.execute("UPDATE recall_feedback SET useful = -1, resolved_at = datetime('now') WHERE memory_id = 'mem-resolved'", []).unwrap();
        });

        let ids = vec!["mem-resolved".to_string(), "mem-pending".to_string(), "mem-never-recalled".to_string()];
        let map = db.get_resolved_feedback_map(&ids).await.unwrap();

        assert_eq!(map.get("mem-resolved"), Some(&-1));
        assert!(!map.contains_key("mem-pending"), "still useful=0 -> must not appear as ground truth");
        assert!(!map.contains_key("mem-never-recalled"), "no row at all -> must not appear");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn get_resolved_feedback_map_empty_input_returns_empty_map() {
        let db = SilvaDB::in_memory().await.unwrap();
        let map = db.get_resolved_feedback_map(&[]).await.unwrap();
        assert!(map.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn agent_affinity_for_memory_returns_zero_for_unknown() {
        let db = SilvaDB::in_memory().await.unwrap();
        let affinity = db.agent_affinity_for_memory("mem-never-seen", "agent-x").await.unwrap();
        assert_eq!(affinity, 0.0, "unknown memory must return 0.0 affinity");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn agent_affinity_for_memory_computes_agent_specific_ratio() {
        let db = SilvaDB::in_memory().await.unwrap();
        db.upsert_node("mem1", "concept", "test", "{}").await.unwrap();

        let old_ts = (chrono::Utc::now() - chrono::Duration::seconds(120)).to_rfc3339();
        tokio::task::block_in_place(|| {
            let conn = db.conn.blocking_lock();
            // agent-a: 2 useful, 1 not-useful -> 2/3
            conn.execute("INSERT INTO recall_feedback (memory_id, agent_id, task_hash, query_text, rank_position, accessed_at, useful) VALUES ('mem1', 'agent-a', 't1', 'q', 0, ?1, 1)", rusqlite::params![old_ts]).unwrap();
            conn.execute("INSERT INTO recall_feedback (memory_id, agent_id, task_hash, query_text, rank_position, accessed_at, useful) VALUES ('mem1', 'agent-a', 't2', 'q', 1, ?1, 1)", rusqlite::params![old_ts]).unwrap();
            conn.execute("INSERT INTO recall_feedback (memory_id, agent_id, task_hash, query_text, rank_position, accessed_at, useful) VALUES ('mem1', 'agent-a', 't3', 'q', 2, ?1, -1)", rusqlite::params![old_ts]).unwrap();
            // agent-b: 1 useful -> 1.0
            conn.execute("INSERT INTO recall_feedback (memory_id, agent_id, task_hash, query_text, rank_position, accessed_at, useful) VALUES ('mem1', 'agent-b', 't4', 'q', 0, ?1, 1)", rusqlite::params![old_ts]).unwrap();
        });

        let affinity_a = db.agent_affinity_for_memory("mem1", "agent-a").await.unwrap();
        let affinity_b = db.agent_affinity_for_memory("mem1", "agent-b").await.unwrap();
        let affinity_c = db.agent_affinity_for_memory("mem1", "agent-c").await.unwrap();

        assert!((affinity_a - 2.0/3.0).abs() < 0.01, "agent-a affinity should be 2/3, got {affinity_a}");
        assert!((affinity_b - 1.0).abs() < 0.01, "agent-b affinity should be 1.0, got {affinity_b}");
        assert!((affinity_c - 0.75).abs() < 0.01, "agent-c has no agent-specific data, should fall back to global 3/4, got {affinity_c}");
    }
}
