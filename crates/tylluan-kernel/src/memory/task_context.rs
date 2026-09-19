//! # Task Context Capsule — per-task working memory (TCC-1, task_context_capsule_spec.md)
//!
//! A structured, additive blackboard keyed by the existing work-contract id
//! (spec §4.1: no new namespace). While a task lives, agents append
//! decisions and pointers; nothing is ever edited or deleted individually
//! (spec §6, MMP P1). The ONLY exit is the close hook (TCC-3): on
//! `/close` the capsule is synthesized into a `task_synthesis` memory node
//! via `tylluan_remember` and its row deleted — synthesized or discarded,
//! never orphaned (spec §7, SSGM event-governed eviction).
//!
//! Scope of TCC-1 (spec §9.1): types + SQLite persistence via
//! `config::open_db()` (the encrypt-at-rest choke point, `965e8ba`
//! lesson). No HTTP endpoints (TCC-2), no close hook (TCC-3).
//!
//! Storage choice: a dedicated `data/task_context.db` with THIS module as
//! the single owner of its path resolution (`task_context_db_path`, same
//! seam pattern as `dispatch_db_path`) — one owner per store path, per the
//! seam-unification campaign. The FK to `work_contracts` is logical (the
//! capsule is worthless without its contract), not enforced across files.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

/// Single owner of the capsule store path (seam-unification pattern:
/// `TYLLUAN_TASK_CONTEXT_DB` checked in exactly one place; unset →
/// historical default; set → writer and reader land on the same file).
pub fn task_context_db_path() -> std::path::PathBuf {
    std::env::var("TYLLUAN_TASK_CONTEXT_DB")
        .unwrap_or_else(|_| "./data/task_context.db".to_string())
        .into()
}

/// One decision entry — additive, never overwritten or deleted
/// individually. `by` is mandatory so every claim stays traceable to its
/// source (spec §5, MMP P2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapsuleDecision {
    pub by: String,
    pub what: String,
    pub at: i64,
}

/// What kind of artifact a pointer refers to — never the content itself,
/// only the reference (the capsule must stay cheap to read and write).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PointerKind {
    Doc,
    Commit,
    File,
    ColoquioTurn,
    ContractId,
}

impl PointerKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            PointerKind::Doc => "Doc",
            PointerKind::Commit => "Commit",
            PointerKind::File => "File",
            PointerKind::ColoquioTurn => "ColoquioTurn",
            PointerKind::ContractId => "ContractId",
        }
    }

    fn from_str(s: &str) -> Option<PointerKind> {
        match s {
            "Doc" => Some(PointerKind::Doc),
            "Commit" => Some(PointerKind::Commit),
            "File" => Some(PointerKind::File),
            "ColoquioTurn" => Some(PointerKind::ColoquioTurn),
            "ContractId" => Some(PointerKind::ContractId),
            _ => None,
        }
    }
}

/// One pointer entry — additive like decisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapsulePointer {
    pub kind: PointerKind,
    pub reference: String,
    pub added_by: String,
    pub at: i64,
}

/// The capsule itself — one row per contract_id, mutated additively.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskContextCapsule {
    pub contract_id: String,
    pub decisions: Vec<CapsuleDecision>,
    pub pointers: Vec<CapsulePointer>,
    pub updated_at: i64,
    pub updated_by: String,
}

/// The capsule store: SQLite-backed, additive-only writes.
///
/// INVARIANT (single writer): every write flows through
/// [`add_decision`](Self::add_decision) or
/// [`add_pointer`](Self::add_pointer) — read-modify-write of the JSON
/// columns under the one `Mutex<Connection>`, appending only. Nothing else
/// in this module writes the columns; the delete belongs to the TCC-3
/// close hook, not here.
pub struct TaskContextStore {
    conn: Mutex<Connection>,
}

impl TaskContextStore {
    /// Open (or create) the store DB and ensure the schema.
    pub fn open(db_path: &str) -> Result<Self> {
        if let Some(parent) = Path::new(db_path).parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = crate::config::open_db(Path::new(db_path))
            .with_context(|| format!("Failed to open task context DB: {db_path}"))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS task_context (
                contract_id  TEXT PRIMARY KEY,
                decisions    TEXT NOT NULL DEFAULT '[]',
                pointers     TEXT NOT NULL DEFAULT '[]',
                updated_at   INTEGER NOT NULL,
                updated_by   TEXT NOT NULL
            );",
        )?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// In-memory store for tests.
    pub fn in_memory() -> Result<Self> {
        Self::open(":memory:")
    }

    /// Read a capsule. A contract that never had a capsule reads as clean
    /// `None` — not an error (spec §9.1 acceptance test).
    pub fn get(&self, contract_id: &str) -> Result<Option<TaskContextCapsule>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        capsule_from_row(&conn, contract_id)
    }

    /// Append a decision. Creates the capsule row if absent; never touches
    /// the decisions already stored. Returns the full post-write capsule.
    pub fn add_decision(&self, contract_id: &str, by: &str, what: &str) -> Result<TaskContextCapsule> {
        let entry = CapsuleDecision {
            by: by.to_string(),
            what: what.to_string(),
            at: chrono::Utc::now().timestamp(),
        };
        self.append(contract_id, &entry.by, |capsule| {
            capsule.decisions.push(entry.clone())
        })
    }

    /// Append a pointer. Creates the capsule row if absent; never touches
    /// the pointers already stored. Returns the full post-write capsule.
    pub fn add_pointer(&self, contract_id: &str, mut pointer: CapsulePointer) -> Result<TaskContextCapsule> {
        let writer = pointer.added_by.clone();
        self.append(contract_id, &writer, |capsule| {
            pointer.at = capsule.updated_at;
            capsule.pointers.push(pointer.clone());
        })
    }

    /// The only mutation path: read → append in memory → write back, all
    /// under the single connection mutex (additive invariant).
    fn append(
        &self,
        contract_id: &str,
        writer: &str,
        mutate: impl FnOnce(&mut TaskContextCapsule),
    ) -> Result<TaskContextCapsule> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut capsule = capsule_from_row(&conn, contract_id)?.unwrap_or(TaskContextCapsule {
            contract_id: contract_id.to_string(),
            decisions: Vec::new(),
            pointers: Vec::new(),
            updated_at: chrono::Utc::now().timestamp(),
            updated_by: writer.to_string(),
        });
        capsule.updated_at = chrono::Utc::now().timestamp();
        capsule.updated_by = writer.to_string();
        mutate(&mut capsule);
        conn.execute(
            "INSERT INTO task_context (contract_id, decisions, pointers, updated_at, updated_by)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(contract_id) DO UPDATE SET
                decisions = excluded.decisions,
                pointers = excluded.pointers,
                updated_at = excluded.updated_at,
                updated_by = excluded.updated_by",
            params![
                capsule.contract_id,
                serde_json::to_string(&capsule.decisions)?,
                serde_json::to_string(&capsule.pointers)?,
                capsule.updated_at,
                capsule.updated_by,
            ],
        )?;
        Ok(capsule)
    }
}

fn capsule_from_row(conn: &Connection, contract_id: &str) -> Result<Option<TaskContextCapsule>> {
    let mut stmt = conn.prepare(
        "SELECT decisions, pointers, updated_at, updated_by
         FROM task_context WHERE contract_id = ?1",
    )?;
    let mut rows = stmt.query(params![contract_id])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let decisions_json: String = row.get(0)?;
    let pointers_json: String = row.get(1)?;
    // Corruption fails LOUDLY (dispatch_queue precedent): a coordination
    // store must never launder garbage into an empty capsule.
    let decisions: Vec<CapsuleDecision> = serde_json::from_str(&decisions_json)
        .map_err(|e| corrupt(0, e))?;
    let pointers_raw: Vec<StoredPointer> = serde_json::from_str(&pointers_json)
        .map_err(|e| corrupt(1, e))?;
    let pointers = pointers_raw
        .into_iter()
        .map(|p| {
            Ok(CapsulePointer {
                kind: PointerKind::from_str(&p.kind).ok_or_else(|| {
                    rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Text,
                        Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("corrupt pointer kind '{}'", p.kind),
                        )),
                    )
                })?,
                reference: p.reference,
                added_by: p.added_by,
                at: p.at,
            })
        })
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Some(TaskContextCapsule {
        contract_id: contract_id.to_string(),
        decisions,
        pointers,
        updated_at: row.get(2)?,
        updated_by: row.get(3)?,
    }))
}

/// Stored form of a pointer: kind kept as its string so an unknown kind
/// surfaces as corruption instead of silently coercing.
#[derive(serde::Serialize, serde::Deserialize)]
struct StoredPointer {
    kind: String,
    reference: String,
    added_by: String,
    at: i64,
}

fn corrupt(col: usize, e: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        col,
        rusqlite::types::Type::Text,
        Box::new(e),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> TaskContextStore {
        TaskContextStore::in_memory().unwrap()
    }

    #[test]
    fn nonexistent_contract_reads_as_clean_none() {
        // Spec §9.1 acceptance: no capsule ever written → None, no error.
        let s = store();
        assert!(s.get("bwc-does-not-exist").unwrap().is_none());
    }

    #[test]
    fn additive_writes_never_lose_a_previous_entry() {
        // Spec §9.1 acceptance: two decisions + two pointers appended in
        // order must all come back, in order, with nothing replaced.
        let s = store();
        let cid = "bwc-test-1";
        s.add_decision(cid, "buffy", "decided A over B").unwrap();
        s.add_pointer(
            cid,
            CapsulePointer {
                kind: PointerKind::Commit,
                reference: "f2cd321".into(),
                added_by: "buffy".into(),
                at: 0,
            },
        )
        .unwrap();
        s.add_decision(cid, "deep", "revised: A stays, C deferred").unwrap();
        s.add_pointer(
            cid,
            CapsulePointer {
                kind: PointerKind::Doc,
                reference: "docs/architecture/task_context_capsule_spec.md".into(),
                added_by: "claude-code".into(),
                at: 0,
            },
        )
        .unwrap();

        let c = s.get(cid).unwrap().unwrap();
        assert_eq!(c.decisions.len(), 2);
        assert_eq!(c.decisions[0].what, "decided A over B");
        assert_eq!(c.decisions[0].by, "buffy");
        assert_eq!(c.decisions[1].what, "revised: A stays, C deferred");
        assert_eq!(c.decisions[1].by, "deep");
        assert_eq!(c.pointers.len(), 2);
        assert_eq!(c.pointers[0].kind, PointerKind::Commit);
        assert_eq!(c.pointers[1].kind, PointerKind::Doc);
        assert!(c.updated_at > 0);
    }

    #[test]
    fn first_write_creates_capsule_and_tracks_last_writer() {
        let s = store();
        let cid = "bwc-test-2";
        s.add_decision(cid, "buffy", "first").unwrap();
        s.add_decision(cid, "deep", "second").unwrap();
        let c = s.get(cid).unwrap().unwrap();
        assert_eq!(c.updated_by, "deep");
        assert_eq!(c.decisions.len(), 2);
    }

    #[test]
    fn pointer_kinds_roundtrip_and_unknown_kind_fails_loudly() {
        // Every kind survives a write/read cycle; a foreign kind in the
        // store is corruption, not a silent default.
        for (i, kind) in [
            PointerKind::Doc,
            PointerKind::Commit,
            PointerKind::File,
            PointerKind::ColoquioTurn,
            PointerKind::ContractId,
        ]
        .into_iter()
        .enumerate()
        {
            let s = store();
            let cid = format!("bwc-kinds-{i}");
            s.add_pointer(
                &cid,
                CapsulePointer {
                    kind,
                    reference: "ref".into(),
                    added_by: "buffy".into(),
                    at: 0,
                },
            )
            .unwrap();
            assert_eq!(s.get(&cid).unwrap().unwrap().pointers[0].kind, kind);
            assert_eq!(kind.as_str(), PointerKind::from_str(kind.as_str()).unwrap().as_str());
        }

        let s = store();
        s.add_pointer(
            "bwc-corrupt-kind",
            CapsulePointer {
                kind: PointerKind::Doc,
                reference: "ref".into(),
                added_by: "buffy".into(),
                at: 0,
            },
        )
        .unwrap();
        {
            let conn = s.conn.lock().unwrap_or_else(|e| e.into_inner());
            conn.execute(
                "UPDATE task_context SET pointers = '[{\"kind\":\"BogusKind\",\"reference\":\"r\",\"added_by\":\"x\",\"at\":1}]'
                 WHERE contract_id = 'bwc-corrupt-kind'",
                [],
            )
            .unwrap();
        }
        assert!(s.get("bwc-corrupt-kind").is_err(), "unknown kind must fail loudly");
    }

    #[test]
    fn corrupt_decisions_json_fails_loudly() {
        let s = store();
        s.add_decision("bwc-corrupt", "buffy", "ok").unwrap();
        {
            let conn = s.conn.lock().unwrap_or_else(|e| e.into_inner());
            conn.execute(
                "UPDATE task_context SET decisions = 'NOT-JSON' WHERE contract_id = 'bwc-corrupt'",
                [],
            )
            .unwrap();
        }
        assert!(s.get("bwc-corrupt").is_err(), "garbage must surface as an error, not an empty capsule");
    }

    #[test]
    fn store_survives_reopen() {
        // Durability: the capsule must survive a kernel restart.
        let path = std::env::temp_dir().join(format!("tcc_test_{}.db", uuid::Uuid::new_v4().simple()));
        {
            let s = TaskContextStore::open(path.to_str().unwrap()).unwrap();
            s.add_decision("bwc-durable", "buffy", "survives").unwrap();
        }
        let s = TaskContextStore::open(path.to_str().unwrap()).unwrap();
        assert_eq!(s.get("bwc-durable").unwrap().unwrap().decisions[0].what, "survives");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn task_context_db_path_seam_unset_is_default_and_set_is_honored() {
        // Seam contract pinned in both directions (dispatch/audit/confusion
        // precedent). Env is process-global — a static guard panics in
        // development if a second test in this binary ever races it.
        // SAFETY (set_var/remove_var): edition-2024 unsafe ops, guarded
        // single-user section.
        use std::sync::atomic::{AtomicBool, Ordering};
        static SEAM_TEST_IN_FLIGHT: AtomicBool = AtomicBool::new(false);
        assert!(
            !SEAM_TEST_IN_FLIGHT.swap(true, Ordering::SeqCst),
            "a second test in this binary is using TYLLUAN_TASK_CONTEXT_DB concurrently — env is process-global"
        );
        unsafe { std::env::remove_var("TYLLUAN_TASK_CONTEXT_DB") };
        assert_eq!(
            task_context_db_path(),
            std::path::PathBuf::from("./data/task_context.db")
        );
        unsafe { std::env::set_var("TYLLUAN_TASK_CONTEXT_DB", "./elsewhere/tcc.db") };
        assert_eq!(task_context_db_path(), std::path::PathBuf::from("./elsewhere/tcc.db"));
        unsafe { std::env::remove_var("TYLLUAN_TASK_CONTEXT_DB") };
        SEAM_TEST_IN_FLIGHT.store(false, Ordering::SeqCst);
    }
}
