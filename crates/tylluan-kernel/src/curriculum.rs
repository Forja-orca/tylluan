//! # Curriculum Learner
//!
//! Experience-based routing that learns which guilds work for which intents.
//! Uses Thompson sampling to balance exploration vs exploitation.
//! Persisted to SQLite for survival across restarts.

use crate::router::catalog::GuildDescriptor;
use rand::Rng;
use rusqlite::{params, Connection, Result as SqlResult};
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Wrapper for thread-safe access
pub type CurriculumHandle = Arc<Mutex<CurriculumLearner>>;
pub use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct CurriculumEntry {
    pub intent_signature: String,
    pub guild: String,
    pub successes: u32,
    pub failures: u32,
    pub total_latency_ms: u64,
    pub last_used: i64,
}

pub struct CurriculumLearner {
    conn: Connection,
    min_samples: u32,
}

impl CurriculumLearner {
    /// Create a new curriculum learner with SQLite persistence.
    pub fn new(db_path: &str, min_samples: u32) -> SqlResult<Self> {
        // Create parent directory if needed
        if let Some(parent) = Path::new(db_path).parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA busy_timeout = 5000; PRAGMA journal_mode = WAL;")?;

        let learner = Self { conn, min_samples };
        learner.init_schema()?;
        Ok(learner)
    }

    /// In-memory fallback (for when DB fails)
    pub fn new_in_memory(min_samples: u32) -> SqlResult<Self> {
        Self::new(":memory:", min_samples)
    }

    fn init_schema(&self) -> SqlResult<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS curriculum_entries (
                intent_sig TEXT NOT NULL,
                guild TEXT NOT NULL,
                successes INTEGER DEFAULT 0,
                failures INTEGER DEFAULT 0,
                total_latency_ms INTEGER DEFAULT 0,
                last_used INTEGER DEFAULT 0,
                PRIMARY KEY (intent_sig, guild)
            );
            CREATE INDEX IF NOT EXISTS idx_curriculum_guild ON curriculum_entries(guild);"
        )?;
        Ok(())
    }

    /// Detect agent role based on ID.
    pub fn detect_role(agent_id: &str) -> &str {
        if agent_id.contains("code") { "engineer" }
        else if agent_id.contains("doc") { "writer" }
        else if agent_id.contains("ops") { "operator" }
        else { "generalist" }
    }

    /// Pre-seed the curriculum from the guild catalog on first run.
    ///
    /// For each guild, records synthetic successes for well-known intent keywords,
    /// so the router starts with useful priors instead of learning from scratch.
    /// Only inserts when the `curriculum_entries` table is empty.
    pub fn seed_from_catalog(&mut self, catalog: &[GuildDescriptor], role: &str) -> SqlResult<usize> {
        let count: usize = self.conn.query_row(
            "SELECT COUNT(*) FROM curriculum_entries", [], |row| row.get(0)
        )?;
        if count > 0 {
            return Ok(0); // Already seeded
        }

        // Intent keywords → guild mapping for a-priori knowledge
        let seeds: &[(&[&str], &str)] = &[
            (&["bash", "sh", "shell", "terminal", "comando", "ejecuta",  "run", "command", "cargo", "npm", "pip", "pytest", "test", "script", "execute"], "bash"),
            (&["ls", "list", "dir", "directory", "file", "archivo", "ruta", "path", "glob", "find", "read", "write", "create", "delete", "move", "cat", "open"], "filesystem"),
            (&["remember", "recall", "memor", "store", "save", "knowledge", "graph", "agente", "mailbox", "retrieve", "search"], "memory"),
            (&["monitor", "watch", "tail", "log", "proceso", "resource", "activity"], "monitor"),
            (&["git", "commit", "push", "pull", "branch", "checkout", "merge", "rebase", "stash", "clone", "version"], "git"),
            (&["docker", "container", "image", "compose", "postgres", "redis", "volume"], "docker"),
            (&["sql", "database", "query", "postgresql", "sqlite", "schema", "table", "select"], "database"),
            (&["code", "edit", "write", "refactor", "implement", "function", "class", "dify", "patch", "format"], "code"),
            (&["search", "internet", "web", "google", "duckduckgo", "wikipedia", "research", "lookup"], "search"),
            (&["analyze", "analysis", "symbol", "dependency", "import", "architecture", "static", "inspect"], "code_analysis"),
            (&["pdf", "document", "extract", "parse", "ocr"], "pdf"),
            (&["image", "vision", "screenshot", "photo", "ocr", "visual", "see"], "vision"),
            (&["health", "status", "cpu", "memory", "disk", "system", "metrics", "uptime", "performance"], "system_metrics"),
            (&["deep", "architecture", "bottleneck", "structural", "map", "oversight"], "deep_analysis"),
            (&["audit", "inventory", "compliance", "security", "diagnostics", "kernel"], "audit"),
            (&["think", "step", "reason", "plan", "decompose", "break down", "chain", "thought"], "sequential_thinking"),
            (&["ingest", "import", "load", "seed", "index", "repository"], "ingest"),
            (&["knowledge", "triple", "ner", "subject", "predicate", "object", "entity", "relationship"], "knowledge"),
            (&["json", "yaml", "csv", "data", "parse", "transform", "convert", "reshape"], "data_tools"),
            (&["format", "prettier", "ruff", "rustfmt", "gofmt", "style", "indentation"], "formatter"),
        ];

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let mut seeded = 0usize;
        for &(keywords, guild_name) in seeds {
            if !catalog.iter().any(|g| g.name == guild_name) {
                continue;
            }

            // Role-based weighting for seed priors
            let weight = match (role, guild_name) {
                ("engineer", "code") => 8,
                ("engineer", "bash") => 7,
                ("writer", "filesystem") => 8,
                ("writer", "code") => 3,
                _ => 3,
            };

            for keyword in keywords {
                self.conn.execute(
                    "INSERT INTO curriculum_entries (intent_sig, guild, successes, failures, total_latency_ms, last_used)
                     VALUES (?1, ?2, ?3, 0, 0, ?4)
                     ON CONFLICT(intent_sig, guild) DO UPDATE SET
                        successes = successes + ?3,
                        last_used = ?4",
                    params![keyword, guild_name, { weight }, now],
                )?;
                seeded += 1;

            }
            // Correcting the execute call below in a separate edit for clarity
        }
        Ok(seeded)
    }

    /// Record an outcome (success or failure) for an intent+guild pair.
    pub fn record_outcome(
        &mut self,
        intent: &str,
        guild: &str,
        success: bool,
        latency_ms: u64,
    ) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        // Upsert: increment existing or insert new.
        // On success, halve accumulated failures so disuse decay doesn't permanently penalise
        // a guild that comes back into active use.
        let _ = self.conn.execute(
            "INSERT INTO curriculum_entries (intent_sig, guild, successes, failures, total_latency_ms, last_used)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(intent_sig, guild) DO UPDATE SET
                successes = successes + ?3,
                failures = CASE WHEN ?3 > 0 THEN MAX(0, failures / 2) ELSE failures + ?4 END,
                total_latency_ms = total_latency_ms + ?5,
                last_used = ?6",
            params![
                intent,
                guild,
                if success { 1 } else { 0 },
                if success { 0 } else { 1 },
                latency_ms as i64,
                now
            ],
        );
    }

    /// Recommend a guild using Thompson sampling.
    /// Returns (guild_name, score, confidence) or None if insufficient data.
    pub fn recommend(
        &self,
        intent: &str,
        candidates: &[String],
    ) -> Option<(String, f64, f64)> {
        let mut rng = rand::thread_rng();
        let mut best: Option<(String, f64, f64)> = None;

        for guild in candidates {
            // Get entry from DB
            let result: SqlResult<(u32, u32, u64)> = self.conn.query_row(
                "SELECT successes, failures, total_latency_ms FROM curriculum_entries 
                 WHERE intent_sig = ?1 AND guild = ?2",
                params![intent, guild],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            );

            if let Ok((successes, failures, _)) = result {
                let total = successes + failures;
                if total < self.min_samples {
                    continue;
                }

                // Thompson sampling: Beta(successes+1, failures+1)
                let alpha = (successes + 1) as f64;
                let beta = (failures + 1) as f64;
                let mean = alpha / (alpha + beta);
                let uncertainty = 1.0 / (alpha + beta).sqrt();
                let noise: f64 = rng.r#gen::<f64>() * 2.0 - 1.0;
                let thompson_sample = (mean + noise * uncertainty * 0.3).clamp(0.0, 1.0);

                let confidence = total as f64 / (total as f64 + 5.0);
                let score = thompson_sample * confidence;

                if best.as_ref().is_none_or(|(_, s, _)| score > *s) {
                    best = Some((guild.clone(), score, confidence));
                }
            }
        }

        best
    }

    /// Get all stats for debugging/display.
    pub fn get_stats(&self) -> serde_json::Value {
        let mut stmt = match self.conn.prepare(
            "SELECT intent_sig, guild, successes, failures, total_latency_ms 
             FROM curriculum_entries ORDER BY last_used DESC"
        ) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("get_stats prepare failed: {}", e);
                return serde_json::json!({"total_entries": 0, "error": e.to_string()});
            }
        };

        let entries: Vec<serde_json::Value> = match stmt.query_map([], |row| {
            let intent: String = row.get(0)?;
            let guild: String = row.get(1)?;
            let successes: u32 = row.get(2)?;
            let failures: u32 = row.get(3)?;
            let total_latency_ms: u64 = row.get(4)?;
            let total = successes + failures;
            let success_rate = if total > 0 { successes as f64 / total as f64 } else { 0.0 };
            Ok(serde_json::json!({
                "intent": intent,
                "guild": guild,
                "successes": successes,
                "failures": failures,
                "total": total,
                "success_rate": (success_rate * 100.0).round() / 100.0,
                "avg_latency_ms": if total > 0 { total_latency_ms / total as u64 } else { 0 },
            }))
        }) {
            Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
            Err(e) => {
                tracing::warn!("get_stats query_map failed: {}", e);
                vec![]
            }
        };

        serde_json::json!({
            "total_entries": entries.len(),
            "entries": entries,
        })
    }

    /// Get failure rate for a guild across all intents (0.0–1.0).
    /// Returns None if no data.
    pub fn get_failure_rate(&self, guild: &str) -> Option<f64> {
        let result: SqlResult<(u32, u32)> = self.conn.query_row(
            "SELECT SUM(successes), SUM(failures) FROM curriculum_entries WHERE guild = ?1",
            params![guild],
            |row| Ok((row.get(0)?, row.get(1)?)),
        );

        match result {
            Ok((successes, failures)) => {
                let total = successes + failures;
                if total == 0 { None } else { Some(failures as f64 / total as f64) }
            }
            Err(_) => None,
        }
    }

    /// Apply decay to guilds not used in the last 7 days.
    pub fn apply_disuse_decay(&mut self) -> SqlResult<usize> {
        let seven_days_ago = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64 - (7 * 24 * 3600);

        let modified = self.conn.execute(
            "UPDATE curriculum_entries 
             SET failures = failures + 1 
             WHERE last_used < ?1",
            params![seven_days_ago],
        )?;
        Ok(modified)
    }

    /// Serializa el estado del curriculum a JSON para persistirlo
    pub fn serialize_state(&self) -> serde_json::Value {
        self.get_stats()
    }

    /// Carga el estado desde un JSON (resultado de serialize_state)
    pub fn load_state(&mut self, state: &serde_json::Value) {
        if let Some(entries) = state.get("entries").and_then(|v| v.as_array()) {
            for entry in entries {
                let intent = entry.get("intent").and_then(|v| v.as_str()).unwrap_or("");
                let guild = entry.get("guild").and_then(|v| v.as_str()).unwrap_or("");
                let successes = entry.get("successes").and_then(|v| v.as_i64()).unwrap_or(0) as u32;
                let failures = entry.get("failures").and_then(|v| v.as_i64()).unwrap_or(0) as u32;
                let total_latency_ms = entry.get("total_latency_ms").and_then(|v| v.as_i64()).unwrap_or(0) as u64;
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
                
                let _ = self.conn.execute(
                    "INSERT INTO curriculum_entries (intent_sig, guild, successes, failures, total_latency_ms, last_used)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT(intent_sig, guild) DO UPDATE SET
                        successes = ?3,
                        failures = ?4,
                        total_latency_ms = ?5,
                        last_used = ?6",
                    params![intent, guild, successes, failures, total_latency_ms as i64, now],
                );
            }
        }
    }

    pub async fn persist(&self, silva: &crate::memory::silva::SilvaDB) -> anyhow::Result<()> {
        let state_json = self.serialize_state().to_string();
        silva.upsert_node(
            "__curriculum_state__",
            "system",
            "Curriculum Thompson sampling state",
            &state_json,
        ).await?;
        silva.set_weight("__curriculum_state__", 999.0).await?; // never decays
        Ok(())
    }

    pub async fn restore(silva: &crate::memory::silva::SilvaDB) -> anyhow::Result<Option<Self>> {
        match silva.get_node("__curriculum_state__").await? {
            Some(node) => {
                let state: serde_json::Value = serde_json::from_str(&node.content)
                    .unwrap_or(serde_json::json!({}));
                let mut learner = Self::new_in_memory(5)?;
                learner.load_state(&state);
                Ok(Some(learner))
            }
            None => Ok(None),
        }
    }
}

