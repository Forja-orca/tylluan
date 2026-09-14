//! Cognitive Scheduler — WS3 confusion-matrix collector (observation-only half).
//!
//! For every real `tylluan_do` dispatch that reaches Stage 3 (execution
//! happened), this module records ONE complete row pairing:
//! - what the Scheduler WOULD have decided (`decide()` on the same real
//!   Stage-1 inputs `observe_scheduling` already saw), and
//! - what the cascade ACTUALLY did (routed guild/tool, success, whether the
//!   reactive fallback fired, the final guild the request landed on).
//!
//! The pair is classified with the ONLY comparison that is honest to make
//! today: `StandardGuild{guild_id:""}` — the matrix's unfilled guild — is
//! treated as "scheduler defers to the dispatcher's guild choice". Any
//! OTHER scheduler verdict (FastLane, DeliberativeCoordinator,
//! HumanAuthorizationRequired, OfflineNightBatch, RemoteMeshPeer) on a task
//! the cascade dispatched to a guild is a `Differ` row: it means the
//! scheduler's model of the task disagrees with what production routing
//! did. Accumulated Differ tallies per guild+tool are the eventual cutover
//! input — and they are also deliberately NOT an action: nothing here
//! reads the store back into dispatch. Cutover needs Tech Lead sign-off
//! (ADR + separate cycle); this module only accumulates the evidence.
//!
//! Storage: SQLite at `./data/scheduler_confusion.db` by default, resolved
//! by `confusion_db_path()` — the single owner of path resolution (both the
//! write path and the `/scheduler/confusion` read path go through it;
//! `TYLLUAN_CONFUSION_DB` is checked in exactly one place). Same pattern as
//! the audit log: `config::open_db`, idempotent CREATE TABLE, fire-and-forget
//! via `spawn_blocking`, errors non-fatal. One row per completed dispatch,
//! inserted at Stage 3 when both halves of the pair are known — no
//! pending-row correlation keys, no partial rows. Plan-mode dispatches never
//! reach Stage 3 and are excluded by construction (nothing executed, so
//! there is no "what the cascade did" half to pair).
//!
//! Schema (documented for the cutover decision):
//! ```sql
//! CREATE TABLE IF NOT EXISTS scheduler_confusion (
//!   id INTEGER PRIMARY KEY AUTOINCREMENT,
//!   ts TEXT NOT NULL,                -- RFC 3339 record time
//!   intent TEXT NOT NULL,
//!   agent_id TEXT NOT NULL DEFAULT '',
//!   complexity_score REAL NOT NULL,  -- same pure scorer the cascade used
//!   risk_tier TEXT NOT NULL,         -- effective RiskTier decide() saw
//!   scheduler_verdict TEXT NOT NULL, -- ExecutionClass short name
//!   scheduler_detail TEXT NOT NULL,  -- variant-specific fields, JSON
//!   routed_guild TEXT NOT NULL,      -- Stage-1 resolution (actual dispatch)
//!   routed_tool TEXT NOT NULL,
//!   final_guild TEXT NOT NULL,       -- after reactive cascade, if any
//!   cascade_fired INTEGER NOT NULL,  -- reactive fallback to coordinator ran
//!   success INTEGER NOT NULL,
//!   classification TEXT NOT NULL,    -- Agrees | Differ
//!   system_snapshot TEXT             -- WS5 boot snapshot JSON (nullable)
//! );
//! ```

use serde_json::json;

/// Resolve the confusion store path — THE single owner of path resolution
/// for `./data/scheduler_confusion.db`. The writer (`record_dispatch_pair`)
/// and the reader behind `/api/v1/scheduler/confusion` (`tallies`) both call
/// THIS; `TYLLUAN_CONFUSION_DB` is checked in exactly one place. Unset env →
/// byte-identical to the historical hardcoded path; set → writer and reader
/// land on the same file (audit store got the same treatment for
/// `TYLLUAN_AUDIT_DB` in the seam-unification pass).
pub fn confusion_db_path() -> std::path::PathBuf {
    std::env::var("TYLLUAN_CONFUSION_DB")
        .unwrap_or_else(|_| "./data/scheduler_confusion.db".to_string())
        .into()
}

/// Scheduler verdict short name for storage. `StandardGuild` carries empty
/// ids in the current matrix (§5) — stored bare; `FastLane` keeps its
/// handler; the rest keep their discriminant only (details go to
/// `scheduler_detail`).
fn verdict_name(ec: &super::types::ExecutionClass) -> &'static str {
    match ec {
        super::types::ExecutionClass::FastLane { .. } => "FastLane",
        super::types::ExecutionClass::StandardGuild { .. } => "StandardGuild",
        super::types::ExecutionClass::DeliberativeCoordinator { .. } => "DeliberativeCoordinator",
        super::types::ExecutionClass::HumanAuthorizationRequired { .. } => "HumanAuthorizationRequired",
        super::types::ExecutionClass::RemoteMeshPeer { .. } => "RemoteMeshPeer",
        super::types::ExecutionClass::OfflineNightBatch { .. } => "OfflineNightBatch",
    }
}

/// Classification of one (scheduler verdict, cascade outcome) pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agreement {
    /// Scheduler's model matches what production routing did (today: the
    /// matrix deferred via the unfilled `StandardGuild`, and a guild ran).
    Agrees,
    /// Scheduler's model disagrees with production routing for this task.
    /// These tallies are the cutover evidence — not an error, not acted on.
    Differ,
}

/// Classify one (verdict, outcome) pair. Total function over the verdict
/// domain: an unfilled `StandardGuild{guild_id:""}` is a deferral → Agrees;
/// a FILLED `StandardGuild` agrees only when it names the guild production
/// actually routed to; every other verdict on a guild-dispatched task is a
/// `Differ` — a real model disagreement worth counting, never acted on.
pub fn classify_pair(
    decision: &super::types::SchedulingDecision,
    routed_guild: &str,
) -> Agreement {
    match &decision.execution_class {
        // The matrix's guild rows carry guild_id:"" (§5 leaves guild naming
        // to the dispatcher). Empty = deferral: production's choice IS the
        // scheduler's choice.
        super::types::ExecutionClass::StandardGuild { guild_id, .. } if guild_id.is_empty() => Agreement::Agrees,
        super::types::ExecutionClass::StandardGuild { guild_id, .. } => {
            if guild_id == routed_guild { Agreement::Agrees } else { Agreement::Differ }
        }
        _ => Agreement::Differ,
    }
}

/// Everything Stage 3 knows about one completed dispatch. Built by the
/// caller (handler_do post-process), consumed fire-and-forget.
pub struct ConfusionRecord<'a> {
    pub intent: &'a str,
    pub agent_id: Option<&'a str>,
    /// The Scheduler's verdict for this exact dispatch (same TaskContext
    /// inputs `observe_scheduling` built at the end of Stage 1).
    pub decision: &'a super::types::SchedulingDecision,
    pub routed_guild: &'a str,
    pub routed_tool: &'a str,
    /// Guild the request finally landed on (differs from `routed_guild` only
    /// when the reactive cascade replaced the result).
    pub final_guild: &'a str,
    pub cascade_fired: bool,
    pub success: bool,
}

/// Append one confusion row to the production store. Fire-and-forget by
/// contract: failures are logged and never propagate — the collector must
/// never make a dispatch fail (same posture as `log_audit_entry`).
///
/// `TYLLUAN_CONFUSION_DB` relocates the store: primarily a test seam so the
/// integration test exercises THIS function against a temp file instead of
/// writing test rows into the developer's live tallies, but also lets an
/// operator place the store elsewhere.
pub fn record_dispatch_pair(rec: &ConfusionRecord<'_>) -> Result<(), String> {
    record_into(&confusion_db_path(), rec)
}

/// Real insert logic, path-injected so tests exercise THIS code (not a
/// copy of its SQL) against a temp file.
pub(crate) fn record_into(db_path: &std::path::Path, rec: &ConfusionRecord<'_>) -> Result<(), String> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("confusion mkdir: {e}"))?;
    }
    let conn = crate::config::open_db(db_path).map_err(|e| format!("confusion open: {e}"))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS scheduler_confusion (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ts TEXT NOT NULL,
            intent TEXT NOT NULL,
            agent_id TEXT NOT NULL DEFAULT '',
            complexity_score REAL NOT NULL,
            risk_tier TEXT NOT NULL,
            scheduler_verdict TEXT NOT NULL,
            scheduler_detail TEXT NOT NULL,
            routed_guild TEXT NOT NULL,
            routed_tool TEXT NOT NULL,
            final_guild TEXT NOT NULL,
            cascade_fired INTEGER NOT NULL,
            success INTEGER NOT NULL,
            classification TEXT NOT NULL,
            system_snapshot TEXT
        );"
    ).map_err(|e| format!("confusion schema: {e}"))?;

    let agreement = classify_pair(rec.decision, rec.routed_guild);
    let classification = match agreement { Agreement::Agrees => "Agrees", Agreement::Differ => "Differ" };
    let detail = serde_json::to_string(&rec.decision.execution_class)
        .unwrap_or_else(|_| "{}".to_string());
    let snapshot_json = crate::router::system_snapshot::global()
        .and_then(|s| serde_json::to_string(s).ok());

    conn.execute(
        "INSERT INTO scheduler_confusion (
            ts, intent, agent_id, complexity_score, risk_tier,
            scheduler_verdict, scheduler_detail,
            routed_guild, routed_tool, final_guild, cascade_fired, success,
            classification, system_snapshot
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        rusqlite::params![
            chrono::Utc::now().to_rfc3339(),
            rec.intent,
            rec.agent_id.unwrap_or(""),
            rec.decision.complexity_score,
            format!("{:?}", rec.decision.risk_tier),
            verdict_name(&rec.decision.execution_class),
            detail,
            rec.routed_guild,
            rec.routed_tool,
            rec.final_guild,
            if rec.cascade_fired { 1 } else { 0 },
            if rec.success { 1 } else { 0 },
            classification,
            snapshot_json,
        ],
    ).map_err(|e| format!("confusion insert: {e}"))?;
    Ok(())
}

/// Aggregate tallies for the cutover conversation: per classification counts
/// overall and per guild, plus the cascade-fires that a scheduler verdict of
/// DeliberativeCoordinator would have owned.
pub fn tallies() -> Result<serde_json::Value, String> {
    tallies_from(&confusion_db_path())
}

/// Real aggregation logic, path-injected for tests.
pub(crate) fn tallies_from(db_path: &std::path::Path) -> Result<serde_json::Value, String> {
    let conn = match crate::config::open_db(db_path) {
        Ok(c) => c,
        Err(_) => return Ok(json!({ "total": 0, "agrees": 0, "differ": 0, "differ_with_cascade_fired": 0, "by_guild": {}, "note": "store not created yet" })),
    };
    let _ = conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS scheduler_confusion (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ts TEXT NOT NULL, intent TEXT NOT NULL, agent_id TEXT NOT NULL DEFAULT '',
            complexity_score REAL NOT NULL, risk_tier TEXT NOT NULL,
            scheduler_verdict TEXT NOT NULL, scheduler_detail TEXT NOT NULL,
            routed_guild TEXT NOT NULL, routed_tool TEXT NOT NULL, final_guild TEXT NOT NULL,
            cascade_fired INTEGER NOT NULL, success INTEGER NOT NULL,
            classification TEXT NOT NULL, system_snapshot TEXT
        );"
    );

    let (total, agrees, differs): (i64, i64, i64) = conn.query_row(
        "SELECT COUNT(*),
                SUM(CASE WHEN classification = 'Agrees' THEN 1 ELSE 0 END),
                SUM(CASE WHEN classification = 'Differ' THEN 1 ELSE 0 END)
         FROM scheduler_confusion",
        [],
        // SUM over zero rows yields NULL — unwrap_or(0) BEFORE the outer Ok,
        // never a stray `?` on a plain value.
        |r| Ok((r.get(0)?, r.get(1).unwrap_or(0), r.get(2).unwrap_or(0))),
    ).map_err(|e| format!("confusion tally: {e}"))?;

    let mut stmt = conn.prepare(
        "SELECT routed_guild, classification, COUNT(*) FROM scheduler_confusion GROUP BY routed_guild, classification"
    ).map_err(|e| format!("confusion by-guild: {e}"))?;
    let mut by_guild: std::collections::BTreeMap<String, serde_json::Value> = std::collections::BTreeMap::new();
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))
    }).map_err(|e| format!("confusion by-guild query: {e}"))?;
    for row in rows {
        let (guild, class, n) = row.map_err(|e| format!("confusion by-guild row: {e}"))?;
        let entry = by_guild.entry(guild).or_insert_with(|| json!({ "Agrees": 0, "Differ": 0 }));
        entry[class] = json!(n);
    }

    let (differs_cascade,): (i64,) = conn.query_row(
        "SELECT COUNT(*) FROM scheduler_confusion WHERE classification = 'Differ' AND cascade_fired = 1",
        [],
        |r| Ok((r.get(0)?,)),
    ).map_err(|e| format!("confusion cascade tally: {e}"))?;

    Ok(json!({
        "total": total,
        "agrees": agrees,
        "differ": differs,
        "differ_with_cascade_fired": differs_cascade,
        "by_guild": by_guild,
        "note": "observation-only tallies; cutover is a separate Tech Lead decision"
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::scheduler::types::{ExecutionClass, RiskTier, SchedulingDecision};
    use std::time::Duration;

    fn decision(ec: ExecutionClass) -> SchedulingDecision {
        SchedulingDecision {
            execution_class: ec,
            complexity_score: 0.5,
            risk_tier: RiskTier::StateMutation,
            latency_budget: Duration::from_secs(5),
            explanation: "test".to_string(),
        }
    }

    // ── classification truth table (fabricated verdict+outcome pairs) ────

    #[test]
    fn unfilled_standard_guild_deferral_agrees_with_any_guild_dispatch() {
        // §5's guild rows carry guild_id:"" — the matrix deferred; whatever
        // guild production picked IS the agreement case.
        let d = decision(ExecutionClass::StandardGuild { guild_id: String::new(), tool_name: String::new() });
        assert_eq!(classify_pair(&d, "bash"), Agreement::Agrees);
        assert_eq!(classify_pair(&d, "coloquio"), Agreement::Agrees);
        assert_eq!(classify_pair(&d, "coordinator"), Agreement::Agrees);
    }

    #[test]
    fn filled_standard_guild_classifies_by_guild_match() {
        // If the matrix ever fills guild_id, agreement depends on whether it
        // named the guild production actually routed to — same name agrees,
        // a mismatch is a real disagreement, never silently one or the other.
        let d_other = decision(ExecutionClass::StandardGuild { guild_id: "other".to_string(), tool_name: "t".to_string() });
        assert_eq!(classify_pair(&d_other, "bash"), Agreement::Differ);
        let d_match = decision(ExecutionClass::StandardGuild { guild_id: "bash".to_string(), tool_name: "t".to_string() });
        assert_eq!(classify_pair(&d_match, "bash"), Agreement::Agrees);
    }

    #[test]
    fn non_deferral_verdicts_on_guild_dispatched_tasks_differ() {
        // The cutover-evidence cases: the scheduler would have taken the task
        // somewhere the cascade did not.
        let cases = vec![
            ExecutionClass::FastLane { target_handler: "direct".to_string() },
            ExecutionClass::DeliberativeCoordinator { plan_mode: false },
            ExecutionClass::HumanAuthorizationRequired {
                reason: "critical risk".to_string(),
                suggested_action: "approve_action".to_string(),
            },
            ExecutionClass::OfflineNightBatch { job_type: "night".to_string() },
            ExecutionClass::RemoteMeshPeer { peer_id: "p".to_string(), capability: "guild-dispatch".to_string() },
        ];
        for ec in cases {
            let d = decision(ec);
            assert_eq!(classify_pair(&d, "bash"), Agreement::Differ, "verdict {:?}", d.execution_class);
        }
    }

    #[test]
    fn verdict_names_are_stable_storage_keys() {
        // The DB stores these strings; renaming them silently breaks queries.
        assert_eq!(verdict_name(&ExecutionClass::StandardGuild { guild_id: String::new(), tool_name: String::new() }), "StandardGuild");
        assert_eq!(verdict_name(&ExecutionClass::FastLane { target_handler: String::new() }), "FastLane");
        assert_eq!(verdict_name(&ExecutionClass::DeliberativeCoordinator { plan_mode: false }), "DeliberativeCoordinator");
        assert_eq!(verdict_name(&ExecutionClass::HumanAuthorizationRequired { reason: String::new(), suggested_action: String::new() }), "HumanAuthorizationRequired");
        assert_eq!(verdict_name(&ExecutionClass::RemoteMeshPeer { peer_id: String::new(), capability: String::new() }), "RemoteMeshPeer");
        assert_eq!(verdict_name(&ExecutionClass::OfflineNightBatch { job_type: String::new() }), "OfflineNightBatch");
    }

    // ── persistence — through the REAL injected functions, temp DB ────────

    #[test]
    fn rows_persist_through_real_record_into_and_tallies_aggregate() {
        let db_path = std::env::temp_dir().join(format!("confusion_real_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&db_path);

        // Four fabricated verdict+outcome pairs covering the classification
        // space, recorded through record_into — the same function production
        // calls (only the path differs).
        let mk_rec = |ec: ExecutionClass, routed: &'static str, final_g: &'static str, cascade: bool, ok: bool| {
            let d = decision(ec);
            ConfusionRecord {
                intent: "test intent pair",
                agent_id: Some("buffy"),
                decision: Box::leak(Box::new(d)),
                routed_guild: routed,
                routed_tool: "bash_execute",
                final_guild: final_g,
                cascade_fired: cascade,
                success: ok,
            }
        };

        let d_std = || ExecutionClass::StandardGuild { guild_id: String::new(), tool_name: String::new() };
        let d_coord = || ExecutionClass::DeliberativeCoordinator { plan_mode: false };

        let rows = vec![
            mk_rec(d_std(), "bash", "bash", false, true),            // deferral, plain success
            mk_rec(d_std(), "coloquio", "coloquio", false, true),    // deferral, different guild
            mk_rec(d_coord(), "bash", "bash", false, false),         // differ, no cascade
            mk_rec(d_coord(), "git", "coordinator", true, true),     // differ, cascade fired
        ];
        for r in &rows {
            record_into(&db_path, r).expect("record_into must succeed on temp db");
        }

        let t = tallies_from(&db_path).expect("tallies_from must succeed");
        assert_eq!(t["total"].as_i64(), Some(4));
        assert_eq!(t["agrees"].as_i64(), Some(2), "two deferral rows agree");
        assert_eq!(t["differ"].as_i64(), Some(2), "two coordinator-verdict rows differ");
        assert_eq!(t["differ_with_cascade_fired"].as_i64(), Some(1));
        let by_guild = t["by_guild"].as_object().expect("by_guild object");
        assert_eq!(by_guild["bash"]["Agrees"].as_i64(), Some(1));
        assert_eq!(by_guild["bash"]["Differ"].as_i64(), Some(1));
        assert_eq!(by_guild["coloquio"]["Agrees"].as_i64(), Some(1));
        assert_eq!(by_guild["git"]["Differ"].as_i64(), Some(1));
        assert!(t["note"].as_str().unwrap().contains("observation-only"),
            "tallies must self-describe as non-actionable");

        // The stamped snapshot column: real WS5 global when set (boot order in
        // tests is unspecified), so verify presence-or-NULL honestly.
        let conn = crate::config::open_db(&db_path).unwrap();
        let stamped: i64 = conn.query_row(
            "SELECT COUNT(*) FROM scheduler_confusion WHERE system_snapshot IS NOT NULL", [], |r| r.get(0),
        ).unwrap_or(0);
        let has_global = crate::router::system_snapshot::global().is_some();
        assert_eq!(stamped > 0, has_global, "snapshot column populated iff WS5 global is set");

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn tallies_on_missing_store_reports_zeroed_shape_not_error() {
        // A fresh kernel that has no recorded rows gets an honest empty
        // aggregate — but tallies_from CREATES the file via open_db when the
        // parent exists, so use a path whose parent does not exist to hit the
        // not-created-yet branch deterministically.
        let missing = std::env::temp_dir().join(format!("no_such_dir_{}", std::process::id())).join("confusion.db");
        let t = tallies_from(&missing).expect("missing store must not error");
        assert_eq!(t["total"].as_i64(), Some(0));
        assert_eq!(t["agrees"].as_i64(), Some(0));
        assert_eq!(t["differ"].as_i64(), Some(0));
        assert_eq!(t["differ_with_cascade_fired"].as_i64(), Some(0));
        assert!(t["by_guild"].as_object().is_some_and(|g| g.is_empty()));
    }

    /// Seam-unification round trip through the path-injected pair (no env):
    /// the two-directional contract — same rows appear through writer and
    /// reader when both resolve the SAME temp path.
    #[test]
    fn seam_round_trip_write_and_read_same_path() {
        let db_path = std::env::temp_dir().join(format!("confusion_seam_rt_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&db_path);
        let rec = ConfusionRecord {
            intent: "seam round-trip",
            agent_id: Some("buffy"),
            decision: Box::leak(Box::new(decision(ExecutionClass::StandardGuild { guild_id: String::new(), tool_name: String::new() }))),
            routed_guild: "bash",
            routed_tool: "bash_execute",
            final_guild: "bash",
            cascade_fired: false,
            success: true,
        };
        record_into(&db_path, &rec).expect("write must succeed");
        let t = tallies_from(&db_path).expect("read must succeed");
        assert_eq!(t["total"].as_i64(), Some(1), "writer and reader resolve to the same file");
        let _ = std::fs::remove_file(&db_path);
    }

    /// Seam-unification contract for the ENV RESOLUTION itself, pinned in
    /// both directions: the writer (`record_dispatch_pair`) and the reader
    /// (`tallies`) must land on the same file when `TYLLUAN_CONFUSION_DB` is
    /// set, and the resolver's unset default is byte-identical to the
    /// historical hardcoded path. Env is process-global — a static guard
    /// panics in development if a second test in this binary ever races it.
    /// SAFETY (set_var): edition-2024 unsafe op, guarded single-user section.
    #[test]
    fn confusion_seam_write_and_read_land_on_same_file() {
        use std::sync::atomic::{AtomicBool, Ordering};
        static SEAM_TEST_IN_FLIGHT: AtomicBool = AtomicBool::new(false);
        assert!(
            !SEAM_TEST_IN_FLIGHT.swap(true, Ordering::SeqCst),
            "a second test in this binary is using TYLLUAN_CONFUSION_DB concurrently — env is process-global"
        );

        let db = std::env::temp_dir().join(format!("confusion_env_rt_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&db);
        // 1) Unset default is byte-identical to the historical hardcoded path.
        unsafe { std::env::remove_var("TYLLUAN_CONFUSION_DB") };
        assert_eq!(confusion_db_path(), std::path::PathBuf::from("./data/scheduler_confusion.db"));
        // 2) Writer through the env seam...
        unsafe { std::env::set_var("TYLLUAN_CONFUSION_DB", &db) };
        let d = decision(ExecutionClass::StandardGuild { guild_id: String::new(), tool_name: String::new() });
        let rec = ConfusionRecord {
            intent: "seam env round-trip",
            agent_id: Some("buffy"),
            decision: Box::leak(Box::new(d)),
            routed_guild: "bash",
            routed_tool: "bash_execute",
            final_guild: "bash",
            cascade_fired: false,
            success: true,
        };
        record_dispatch_pair(&rec).expect("write through env seam must succeed");
        // 3) ...and the reader (the /scheduler/confusion read path) sees the SAME file.
        let t = tallies().expect("read through env seam must succeed");
        assert_eq!(t["total"].as_i64(), Some(1), "env-set writer and reader must land on the same file: {t}");

        unsafe { std::env::remove_var("TYLLUAN_CONFUSION_DB") };
        let _ = std::fs::remove_file(&db);
        SEAM_TEST_IN_FLIGHT.store(false, Ordering::SeqCst);
    }
}
