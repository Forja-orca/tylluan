use axum::{extract::{Path, State}, Json};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use crate::transport::http::HttpState;

// --- Audit types ---

#[derive(Debug, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: i64,
    pub timestamp: i64,
    pub agent_id: String,
    pub guild: String,
    pub tool: String,
    pub args_preview: String,
    pub status: String,
    pub prev_hash: String,
    pub hash: String,
}

#[derive(Deserialize)]
pub struct AuditQuery {
    pub limit: Option<i64>,
    pub agent_id: Option<String>,
    pub guild: Option<String>,
}

// --- Schema (initialised in HttpState::new via JournalDb::open) ---

pub struct JournalDb {
    conn: Arc<Mutex<Connection>>,
}

impl JournalDb {
    pub fn open(db_path: &str) -> anyhow::Result<Self> {
        let conn = crate::config::open_db(std::path::Path::new(db_path))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS agent_journal (
                 agent_id    TEXT PRIMARY KEY,
                 task        TEXT NOT NULL,
                 updated_at  INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS guild_audit_log (
                 id          INTEGER PRIMARY KEY AUTOINCREMENT,
                 timestamp   INTEGER NOT NULL,
                 agent_id    TEXT NOT NULL DEFAULT '',
                 guild       TEXT NOT NULL,
                 tool        TEXT NOT NULL,
                 args_preview TEXT NOT NULL DEFAULT '',
                 status      TEXT NOT NULL,
                 prev_hash   TEXT NOT NULL,
                 hash        TEXT NOT NULL
             );",
        )?;
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    pub fn checkin(&self, agent_id: &str, task: &str) -> rusqlite::Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        self.conn.lock().expect("journal mutex poisoned").execute(
            "INSERT INTO agent_journal(agent_id, task, updated_at)
             VALUES(?1,?2,?3)
             ON CONFLICT(agent_id) DO UPDATE SET task=?2, updated_at=?3",
            params![agent_id, task, now],
        )?;
        Ok(())
    }

    pub fn recover(&self, agent_id: &str) -> rusqlite::Result<Option<JournalEntry>> {
        let conn = self.conn.lock().expect("journal mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT agent_id, task, updated_at FROM agent_journal WHERE agent_id=?1",
        )?;
        match stmt.query_row(params![agent_id], |row| {
            Ok(JournalEntry {
                agent_id: row.get(0)?,
                task: row.get(1)?,
                updated_at: row.get(2)?,
                stale: None,
                stale_secs: None,
            })
        }) {
            Ok(e) => Ok(Some(e)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn cleanup_stale(&self, max_age_secs: i64) -> rusqlite::Result<usize> {
        let cutoff = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64 - max_age_secs;
        let deleted = self.conn.lock().expect("journal mutex poisoned").execute(
            "DELETE FROM agent_journal WHERE updated_at < ?1",
            params![cutoff],
        )?;
        Ok(deleted)
    }

    /// Append-only audit log entry for a guild tool call.
    /// Each entry includes a SHA-256 hash chained to the previous entry
    /// so tampering (modifying or deleting a row) breaks the chain.
    pub fn log_guild_call(
        &self,
        agent_id: &str,
        guild: &str,
        tool: &str,
        args: &str,
        status: &str,
    ) -> rusqlite::Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let preview: String = args.chars().take(200).collect();
        let conn = self.conn.lock().expect("journal mutex poisoned");

        // Get previous hash for chaining
        let prev_hash: String = conn
            .query_row("SELECT hash FROM guild_audit_log ORDER BY id DESC LIMIT 1", [], |r| {
                r.get(0)
            })
            .unwrap_or_default();

        // Chain hash: SHA-256 of (prev_hash || timestamp || agent_id || guild || tool || status)
        let chain_input = format!("{prev_hash}|{now}|{agent_id}|{guild}|{tool}|{status}");
        use sha2::Digest;
        let hash = format!("{:x}", sha2::Sha256::digest(chain_input.as_bytes()));

        conn.execute(
            "INSERT INTO guild_audit_log (timestamp, agent_id, guild, tool, args_preview, status, prev_hash, hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![now, agent_id, guild, tool, preview, status, prev_hash, hash],
        )?;
        Ok(())
    }

    /// Query audit log with optional filters.
    pub fn query_audit(&self, q: &AuditQuery) -> rusqlite::Result<Vec<AuditEntry>> {
        let limit = q.limit.unwrap_or(50).min(1000);
        let conn = self.conn.lock().expect("journal mutex poisoned");
        let mut rows = Vec::new();

        // Build dynamic query using rusqlite's variadic params
        let sql = "SELECT id, timestamp, agent_id, guild, tool, args_preview, status, prev_hash, hash \
                    FROM guild_audit_log WHERE 1=1";
        let mut conditions = Vec::new();
        let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        if let Some(ref agent) = q.agent_id {
            params_vec.push(Box::new(agent.clone()));
            conditions.push(format!(" AND agent_id = ?{}", params_vec.len()));
        }
        if let Some(ref g) = q.guild {
            params_vec.push(Box::new(g.clone()));
            conditions.push(format!(" AND guild = ?{}", params_vec.len()));
        }
        let full_sql = format!("{} {} ORDER BY id DESC LIMIT ?{}",
            sql, conditions.join(""), params_vec.len() + 1);
        params_vec.push(Box::new(limit));

        let mut stmt = conn.prepare(&full_sql)?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        let mapped = stmt.query_map(param_refs.as_slice(), |row| {
            Ok(AuditEntry {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                agent_id: row.get(2)?,
                guild: row.get(3)?,
                tool: row.get(4)?,
                args_preview: row.get(5)?,
                status: row.get(6)?,
                prev_hash: row.get(7)?,
                hash: row.get(8)?,
            })
        })?;
        for entry in mapped {
            rows.push(entry?);
        }
        Ok(rows)
    }

    /// Verify audit chain integrity from oldest to newest entry.
    /// Returns (ok_count, bad_count) — bad > 0 means tampering detected.
    pub fn verify_audit_chain(&self) -> rusqlite::Result<(usize, usize)> {
        let conn = self.conn.lock().expect("journal mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, agent_id, guild, tool, status, prev_hash, hash \
             FROM guild_audit_log ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
            ))
        })?;

        let mut prev = String::new();
        let mut ok = 0usize;
        let mut bad = 0usize;
        for row in rows.flatten() {
            let (_id, ts, agent, guild, tool, status, stored_prev, stored_hash) = row;
            // Check prev_hash linkage
            if stored_prev != prev {
                bad += 1;
                continue;
            }
            // Recompute hash
            let chain_input = format!("{stored_prev}|{ts}|{agent}|{guild}|{tool}|{status}");
            use sha2::Digest;
            let computed = format!("{:x}", sha2::Sha256::digest(chain_input.as_bytes()));
            if computed != stored_hash {
                bad += 1;
                continue;
            }
            prev = stored_hash;
            ok += 1;
        }
        Ok((ok, bad))
    }

    pub fn all(&self) -> rusqlite::Result<Vec<JournalEntry>> {
        let conn = self.conn.lock().expect("journal mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT agent_id, task, updated_at FROM agent_journal ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(JournalEntry {
                agent_id: row.get(0)?,
                task: row.get(1)?,
                updated_at: row.get(2)?,
                stale: None,
                stale_secs: None,
            })
        })?;
        rows.collect()
    }
}

// --- Types ---

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JournalEntry {
    pub agent_id: String,
    pub task: String,
    pub updated_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_secs: Option<i64>,
}

pub(crate) fn is_stale(updated_at: i64) -> (bool, i64) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let elapsed = now - updated_at;
    (elapsed > 300, elapsed)
}

#[derive(Deserialize)]
pub struct CheckinPayload {
    pub task: String,
}

// --- Handlers ---

pub async fn journal_checkin(
    State(state): State<Arc<HttpState>>,
    Path(agent_id): Path<String>,
    Json(payload): Json<CheckinPayload>,
) -> impl axum::response::IntoResponse {
    match state.journal.checkin(&agent_id, &payload.task) {
        Ok(_) => (axum::http::StatusCode::OK, Json(serde_json::json!({
            "ok": true, "agent_id": agent_id, "task": payload.task
        }))),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "ok": false, "error": e.to_string()
        }))),
    }
}

pub async fn journal_recover(
    State(state): State<Arc<HttpState>>,
    Path(agent_id): Path<String>,
) -> impl axum::response::IntoResponse {
    match state.journal.recover(&agent_id) {
        Ok(Some(mut entry)) => {
            let (stale, stale_secs) = is_stale(entry.updated_at);
            entry.stale = Some(stale);
            entry.stale_secs = Some(stale_secs);
            (axum::http::StatusCode::OK, Json(serde_json::json!({
                "ok": true, "entry": entry
            })))
        }
        Ok(None) => (axum::http::StatusCode::NOT_FOUND, Json(serde_json::json!({
            "ok": false, "error": "no journal entry for this agent"
        }))),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "ok": false, "error": e.to_string()
        }))),
    }
}

pub async fn journal_list(
    State(state): State<Arc<HttpState>>,
) -> impl axum::response::IntoResponse {
    match state.journal.all() {
        Ok(mut entries) => {
            for entry in &mut entries {
                let (stale, stale_secs) = is_stale(entry.updated_at);
                entry.stale = Some(stale);
                entry.stale_secs = Some(stale_secs);
            }
            (axum::http::StatusCode::OK, Json(serde_json::json!({
                "ok": true, "entries": entries
            })))
        }
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "ok": false, "error": e.to_string()
        }))),
    }
}
