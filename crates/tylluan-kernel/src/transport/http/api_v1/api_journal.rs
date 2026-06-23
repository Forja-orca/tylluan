use axum::{extract::{Path, State}, Json};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use crate::transport::http::HttpState;

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

fn is_stale(updated_at: i64) -> (bool, i64) {
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
