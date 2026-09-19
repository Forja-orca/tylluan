//! # PendingDispatch — HITL dispatch queue with cryptographic binding and CAS
//!
//! BWC-1 of `docs/architecture/coloquio_dispatcher_spec.md` (fb069dd).
//! Assigned to Buffy by arbitration (T567, Coloquio).
//!
//! **DESIGN CREDIT (ADR-013 provenance):** the architecture and a first
//! implementation were written by Deep (claim T564, uncommitted WIP — read
//! and adversarially reviewed line-by-line by Buffy before Deep withdrew it
//! after the arbitration). This file preserves Deep's design decisions:
//! the SHA-256 is computed at `enqueue` and never accepted from the caller
//! (the queue trusts nobody), CAS is `UPDATE ... WHERE state='Pending'`
//! (first writer wins), argv is persisted as JSON with no shell involved,
//! and his test suite — including the concurrent double-approval test that
//! is this contract's acceptance criterion and the durability-across-reopen
//! test — is kept. On top of it, this file applies the three findings from
//! that review:
//!
//! 1. `ApprovalOutcome::NotFound` distinguishes "never existed" from
//!    "already resolved" so the HTTP layer (BWC-2) can answer 404 vs 409
//!    honestly.
//! 2. The single-writer invariant is documented on the struct itself.
//! 3. Corrupt rows fail LOUDLY (`FromSqlConversionFailure`) instead of
//!    silently degrading to an empty command / `Expired` state — a security
//!    queue must never launder garbage into plausible behavior.
//!
//! No HTTP endpoints here (BWC-2); no broadcast subscriber (BWC-3); no
//! process spawning (BWC-4). Only the type, the persistence and the CAS.

use anyhow::{anyhow, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

/// Resolve the dispatch queue path — THE single owner of path resolution
/// for `./data/pending_dispatches.db`. Same seam pattern proven in
/// `confusion_db_path` / `audit_db_path` (the seam-unification pass):
/// `TYLLUAN_DISPATCH_DB` is checked in exactly one place; unset →
/// byte-identical default; set → writer and reader land on the same file.
/// Wired into the kernel's boot in BWC-3.
pub fn dispatch_db_path() -> std::path::PathBuf {
    std::env::var("TYLLUAN_DISPATCH_DB")
        .unwrap_or_else(|_| "./data/pending_dispatches.db".to_string())
        .into()
}

/// Lifecycle of a queued dispatch. Only `Pending` may transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DispatchState {
    Pending,
    Approved,
    Rejected,
    Expired,
}

impl DispatchState {
    pub fn as_str(&self) -> &'static str {
        match self {
            DispatchState::Pending => "Pending",
            DispatchState::Approved => "Approved",
            DispatchState::Rejected => "Rejected",
            DispatchState::Expired => "Expired",
        }
    }

    fn from_str(s: &str) -> Option<DispatchState> {
        match s {
            "Pending" => Some(DispatchState::Pending),
            "Approved" => Some(DispatchState::Approved),
            "Rejected" => Some(DispatchState::Rejected),
            "Expired" => Some(DispatchState::Expired),
            _ => None,
        }
    }
}

/// A queued agent wake-up, awaiting human approval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingDispatch {
    pub id: String,
    pub agent_id: String,
    pub author_id: String,
    pub channel: String,
    pub turn: i64,
    /// Exactly what the human reviewer will see.
    pub content_snapshot: String,
    /// SHA-256 of `content_snapshot` — what the approval binds to.
    pub content_hash: String,
    /// Fixed argv from WakeConfig — never built from the message.
    pub command: Vec<String>,
    pub state: DispatchState,
    pub queued_at: i64,
}

/// Error returned when the expected hash does not match the stored one.
#[derive(Debug, thiserror::Error)]
#[error("hash mismatch: approval was bound to a different content snapshot")]
pub struct HashMismatch;

/// Outcome of an approval/rejection attempt: either this writer won the CAS,
/// the dispatch was already resolved (by a concurrent approval or another
/// transition), or the id never existed. Exactly one writer ever sees
/// `Transitioned`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalOutcome {
    Transitioned,
    AlreadyResolved(DispatchState),
    /// The id does not exist at all — distinct from `AlreadyResolved` so the
    /// HTTP layer (BWC-2) can answer 404 instead of 409.
    NotFound,
}

/// The dispatch queue: SQLite-backed, CAS on every transition.
///
/// INVARIANT (single writer): every state transition flows through
/// [`approve`](Self::approve), [`reject`](Self::reject) or
/// [`expire`](Self::expire) — all serialized by the one `Mutex<Connection>`,
/// and each uses `UPDATE ... WHERE state='Pending'` as its CAS. Nothing else
/// in this module writes `state`; keep it that way as BWC-2/3 grow read-only
/// consumers around this queue.
pub struct DispatchQueue {
    conn: Mutex<Connection>,
}

impl DispatchQueue {
    /// Open (or create) the queue DB and ensure the schema.
    pub fn open(db_path: &str) -> Result<Self> {
        if let Some(parent) = Path::new(db_path).parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = crate::config::open_db(Path::new(db_path))
            .with_context(|| format!("Failed to open dispatch queue DB: {db_path}"))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS pending_dispatches (
                id              TEXT PRIMARY KEY,
                agent_id        TEXT NOT NULL,
                author_id       TEXT NOT NULL,
                channel         TEXT NOT NULL,
                turn            INTEGER NOT NULL,
                content_snapshot TEXT NOT NULL,
                content_hash    TEXT NOT NULL,
                command_json    TEXT NOT NULL,
                state           TEXT NOT NULL DEFAULT 'Pending',
                queued_at       INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_pending_state ON pending_dispatches(state);
            CREATE INDEX IF NOT EXISTS idx_pending_agent ON pending_dispatches(agent_id);",
        )?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// In-memory queue for tests.
    pub fn in_memory() -> Result<Self> {
        Self::open(":memory:")
    }

    /// SHA-256 hex of a snapshot — the binding between reviewer and approval.
    pub fn hash_of(content: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Enqueue a dispatch. `content_hash` is computed here from the snapshot —
    /// the queue never trusts a caller-supplied hash.
    pub fn enqueue(
        &self,
        agent_id: &str,
        author_id: &str,
        channel: &str,
        turn: i64,
        content_snapshot: &str,
        command: Vec<String>,
    ) -> Result<PendingDispatch> {
        let id = uuid::Uuid::new_v4().simple().to_string();
        let content_hash = Self::hash_of(content_snapshot);
        let command_json = serde_json::to_string(&command)?;
        let queued_at = chrono::Utc::now().timestamp();
        let dispatch = PendingDispatch {
            id,
            agent_id: agent_id.to_string(),
            author_id: author_id.to_string(),
            channel: channel.to_string(),
            turn,
            content_snapshot: content_snapshot.to_string(),
            content_hash,
            command,
            state: DispatchState::Pending,
            queued_at,
        };
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO pending_dispatches
             (id, agent_id, author_id, channel, turn, content_snapshot, content_hash, command_json, state, queued_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                dispatch.id,
                dispatch.agent_id,
                dispatch.author_id,
                dispatch.channel,
                dispatch.turn,
                dispatch.content_snapshot,
                dispatch.content_hash,
                command_json,
                dispatch.state.as_str(),
                dispatch.queued_at,
            ],
        )?;
        Ok(dispatch)
    }

    /// Fetch a dispatch by id (any state).
    pub fn get(&self, id: &str) -> Result<Option<PendingDispatch>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, author_id, channel, turn, content_snapshot, content_hash,
                    command_json, state, queued_at
             FROM pending_dispatches WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        let row = rows.next()?;
        Ok(row.map(dispatch_from_row).transpose()?)
    }

    /// All dispatches in `Pending` state.
    ///
    /// NOTE (honest limitation, pinned by test): the `state='Pending'` filter
    /// is SQL-side, so a row with a CORRUPT state string is excluded from
    /// this listing exactly like any legitimate non-Pending row — it cannot
    /// be surfaced here without dropping the index-backed filter. Corruption
    /// still fails LOUDLY on direct access via [`get`](Self::get), and BWC-2
    /// may add a total-row sanity count if the fleet wants listing-time
    /// detection too.
    pub fn list_pending(&self) -> Result<Vec<PendingDispatch>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, author_id, channel, turn, content_snapshot, content_hash,
                    command_json, state, queued_at
             FROM pending_dispatches WHERE state = 'Pending' ORDER BY queued_at ASC",
        )?;
        let rows = stmt.query_map([], dispatch_from_row)?;
        // Collect through the Result: a corrupt row must surface as an error,
        // not be silently skipped (review finding 3).
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// CAS approval: the first writer whose `expected_hash` matches wins;
    /// everyone else sees `AlreadyResolved`. A hash mismatch is a hard error
    /// and never transitions. A nonexistent id is `NotFound`.
    pub fn approve(&self, id: &str, expected_hash: &str) -> Result<ApprovalOutcome> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let current: Option<String> = conn
            .query_row(
                "SELECT state FROM pending_dispatches WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(current_state) = current else {
            // Review finding 1: "never existed" ≠ "already resolved".
            return Ok(ApprovalOutcome::NotFound);
        };
        let current = DispatchState::from_str(&current_state)
            .ok_or_else(|| anyhow!("corrupt state '{current_state}' for dispatch {id}"))?;
        if current != DispatchState::Pending {
            return Ok(ApprovalOutcome::AlreadyResolved(current));
        }
        let stored_hash: String = conn.query_row(
            "SELECT content_hash FROM pending_dispatches WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )?;
        if stored_hash != expected_hash {
            return Err(anyhow!(HashMismatch));
        }
        let affected = conn.execute(
            "UPDATE pending_dispatches SET state = 'Approved' WHERE id = ?1 AND state = 'Pending'",
            params![id],
        )?;
        if affected == 1 {
            Ok(ApprovalOutcome::Transitioned)
        } else {
            // Lost the race: someone else transitioned between our read and
            // our update. Report the current state.
            let now: String = conn.query_row(
                "SELECT state FROM pending_dispatches WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )?;
            Ok(ApprovalOutcome::AlreadyResolved(
                DispatchState::from_str(&now)
                    .ok_or_else(|| anyhow!("corrupt state '{now}' for dispatch {id}"))?,
            ))
        }
    }

    /// CAS rejection (human says no).
    pub fn reject(&self, id: &str) -> Result<ApprovalOutcome> {
        self.transition(id, DispatchState::Rejected)
    }

    /// CAS expiry — called by the kernel when a Pending dispatch outlives its
    /// human-decides window.
    pub fn expire(&self, id: &str) -> Result<ApprovalOutcome> {
        self.transition(id, DispatchState::Expired)
    }

    fn transition(&self, id: &str, target: DispatchState) -> Result<ApprovalOutcome> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let affected = conn.execute(
            "UPDATE pending_dispatches SET state = ?1 WHERE id = ?2 AND state = 'Pending'",
            params![target.as_str(), id],
        )?;
        if affected == 1 {
            return Ok(ApprovalOutcome::Transitioned);
        }
        let now: Option<String> = conn
            .query_row("SELECT state FROM pending_dispatches WHERE id = ?1", params![id], |r| r.get(0))
            .optional()?;
        Ok(match now {
            Some(s) => ApprovalOutcome::AlreadyResolved(
                DispatchState::from_str(&s)
                    .ok_or_else(|| anyhow!("corrupt state '{s}' for dispatch {id}"))?,
            ),
            None => ApprovalOutcome::NotFound,
        })
    }
}

fn dispatch_from_row(row: &rusqlite::Row) -> rusqlite::Result<PendingDispatch> {
    let command_json: String = row.get(7)?;
    let state_str: String = row.get(8)?;
    // Corruption fails LOUDLY (review finding 3): a security queue must never
    // silently turn garbage into an empty command or an `Expired` state.
    let command: Vec<String> = serde_json::from_str(&command_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(7, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let state = DispatchState::from_str(&state_str).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            8,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("corrupt dispatch state '{state_str}'"),
            )),
        )
    })?;
    Ok(PendingDispatch {
        id: row.get(0)?,
        agent_id: row.get(1)?,
        author_id: row.get(2)?,
        channel: row.get(3)?,
        turn: row.get(4)?,
        content_snapshot: row.get(5)?,
        content_hash: row.get(6)?,
        command,
        state,
        queued_at: row.get(9)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn queue() -> Arc<DispatchQueue> {
        Arc::new(DispatchQueue::in_memory().unwrap())
    }

    #[test]
    fn enqueue_binds_hash_to_snapshot() {
        let q = queue();
        let d = q
            .enqueue("deep", "claude-code", "general", 42, "wake up deep: review the PR", vec!["opencode".into(), "run".into()])
            .unwrap();
        assert_eq!(d.state, DispatchState::Pending);
        assert_eq!(d.content_hash, DispatchQueue::hash_of(&d.content_snapshot));
        assert_eq!(q.get(&d.id).unwrap().unwrap().agent_id, "deep");
    }

    #[test]
    fn approve_with_correct_hash_transitions() {
        let q = queue();
        let d = q.enqueue("deep", "claude-code", "general", 1, "snapshot", vec!["cmd".into()]).unwrap();
        match q.approve(&d.id, &d.content_hash).unwrap() {
            ApprovalOutcome::Transitioned => {}
            other => panic!("expected Transitioned, got {other:?}"),
        }
        assert_eq!(q.get(&d.id).unwrap().unwrap().state, DispatchState::Approved);
    }

    #[test]
    fn approve_with_wrong_hash_is_rejected_without_transition() {
        let q = queue();
        let d = q.enqueue("deep", "claude-code", "general", 2, "snapshot", vec!["cmd".into()]).unwrap();
        let err = q.approve(&d.id, "0000000000000000000000000000000000000000000000000000000000000000").unwrap_err();
        assert!(err.to_string().contains("hash mismatch"), "got: {err}");
        assert_eq!(q.get(&d.id).unwrap().unwrap().state, DispatchState::Pending);
    }

    #[test]
    fn concurrent_double_approval_runs_exactly_once() {
        // The BWC-1 acceptance test: two concurrent approvals of the same id
        // must yield exactly ONE Transitioned outcome — the CAS winner.
        let q = queue();
        let d = q.enqueue("deep", "claude-code", "general", 3, "snapshot", vec!["cmd".into()]).unwrap();
        let q1 = Arc::clone(&q);
        let q2 = Arc::clone(&q);
        let id1 = d.id.clone();
        let hash1 = d.content_hash.clone();
        let id2 = d.id.clone();
        let hash2 = d.content_hash.clone();

        let t1 = std::thread::spawn(move || q1.approve(&id1, &hash1).unwrap());
        let t2 = std::thread::spawn(move || q2.approve(&id2, &hash2).unwrap());
        let r1 = t1.join().unwrap();
        let r2 = t2.join().unwrap();

        let transitions = [&r1, &r2]
            .iter()
            .filter(|o| matches!(o, ApprovalOutcome::Transitioned))
            .count();
        assert_eq!(transitions, 1, "exactly one approval must win the CAS: {r1:?} {r2:?}");
        assert_eq!(q.get(&d.id).unwrap().unwrap().state, DispatchState::Approved);
    }

    #[test]
    fn reject_and_expire_only_transition_from_pending() {
        let q = queue();
        let d = q.enqueue("deep", "claude-code", "general", 4, "snapshot", vec!["cmd".into()]).unwrap();
        assert_eq!(q.reject(&d.id).unwrap(), ApprovalOutcome::Transitioned);
        assert_eq!(q.get(&d.id).unwrap().unwrap().state, DispatchState::Rejected);
        // A second transition from Rejected must not work.
        assert_eq!(q.approve(&d.id, &d.content_hash).unwrap(), ApprovalOutcome::AlreadyResolved(DispatchState::Rejected));

        let e = q.enqueue("deep", "claude-code", "general", 5, "snapshot2", vec!["cmd".into()]).unwrap();
        assert_eq!(q.expire(&e.id).unwrap(), ApprovalOutcome::Transitioned);
        assert_eq!(q.get(&e.id).unwrap().unwrap().state, DispatchState::Expired);
        assert_eq!(q.reject(&e.id).unwrap(), ApprovalOutcome::AlreadyResolved(DispatchState::Expired));
    }

    #[test]
    fn approve_nonexistent_id_is_not_found() {
        // Review finding 1: "never existed" must be distinguishable from
        // "already resolved" so the HTTP layer (BWC-2) can answer 404 vs 409.
        let q = queue();
        assert_eq!(q.approve("no-such-id", "deadbeef").unwrap(), ApprovalOutcome::NotFound);
        assert_eq!(q.reject("no-such-id").unwrap(), ApprovalOutcome::NotFound);
        assert_eq!(q.expire("no-such-id").unwrap(), ApprovalOutcome::NotFound);
        assert!(q.get("no-such-id").unwrap().is_none());
    }

    #[test]
    fn corrupt_row_fails_loudly() {
        // Review finding 3: garbage in the store must surface as an error,
        // not silently degrade to empty command / Expired state.
        let q = queue();
        {
            let conn = q.conn.lock().unwrap_or_else(|e| e.into_inner());
            conn.execute(
                "INSERT INTO pending_dispatches
                 (id, agent_id, author_id, channel, turn, content_snapshot, content_hash, command_json, state, queued_at)
                 VALUES ('corrupt', 'x', 'y', 'general', 1, 's', 'h', 'NOT-JSON', 'BogusState', 0)",
                [],
            )
            .unwrap();
        }
        assert!(q.get("corrupt").is_err(), "corrupt command_json must error, not default");
        // SQL-side filter: the corrupt row is not "silently converted" — it
        // is simply not a Pending row as far as this query can tell. It is
        // invisible here but fails loudly on direct access (assertion above).
        assert_eq!(q.list_pending().unwrap().len(), 0, "corrupt state must NOT be laundered into the pending listing");
    }

    #[test]
    fn list_pending_returns_only_pending() {
        let q = queue();
        let a = q.enqueue("deep", "claude-code", "general", 6, "s1", vec!["cmd".into()]).unwrap();
        let b = q.enqueue("buffy", "claude-code", "general", 7, "s2", vec!["cmd".into()]).unwrap();
        q.approve(&a.id, &a.content_hash).unwrap();
        let pending = q.list_pending().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, b.id);
    }

    #[test]
    fn queue_survives_reopen() {
        // Durability requirement: the queue must survive a kernel restart.
        let path = std::env::temp_dir().join(format!("dispatch_test_{}.db", uuid::Uuid::new_v4().simple()));
        {
            let q = DispatchQueue::open(path.to_str().unwrap()).unwrap();
            q.enqueue("deep", "claude-code", "general", 8, "durable", vec!["cmd".into()]).unwrap();
        }
        let q = DispatchQueue::open(path.to_str().unwrap()).unwrap();
        assert_eq!(q.list_pending().unwrap().len(), 1);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn dispatch_db_path_seam_unset_is_default_and_set_is_honored() {
        // Seam contract pinned in both directions (audit/confusion
        // precedent): unset → historical default; set → the env path wins.
        // Env is process-global — a static guard panics in development if a
        // second test in this binary ever races it.
        // SAFETY (set_var/remove_var): edition-2024 unsafe ops, guarded
        // single-user section.
        use std::sync::atomic::{AtomicBool, Ordering};
        static SEAM_TEST_IN_FLIGHT: AtomicBool = AtomicBool::new(false);
        assert!(
            !SEAM_TEST_IN_FLIGHT.swap(true, Ordering::SeqCst),
            "a second test in this binary is using TYLLUAN_DISPATCH_DB concurrently — env is process-global"
        );
        unsafe { std::env::remove_var("TYLLUAN_DISPATCH_DB") };
        assert_eq!(
            dispatch_db_path(),
            std::path::PathBuf::from("./data/pending_dispatches.db")
        );
        unsafe { std::env::set_var("TYLLUAN_DISPATCH_DB", "./elsewhere/dispatch.db") };
        assert_eq!(
            dispatch_db_path(),
            std::path::PathBuf::from("./elsewhere/dispatch.db")
        );
        unsafe { std::env::remove_var("TYLLUAN_DISPATCH_DB") };
        SEAM_TEST_IN_FLIGHT.store(false, Ordering::SeqCst);
    }
}
