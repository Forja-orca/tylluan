//! Friction logging: track where agents hit resistance in Tylluan workflows.
//!
//! Design validated from Qwen's friction research report. Three core entities:
//!
//! 1. **Sessions** — tracks an agent's work session from start to end.
//! 2. **Workflows** — a multi-step task within a session (e.g., "investigate bug X").
//! 3. **Events** — individual friction points (routing errors, manual interventions,
//!    Coloquio round-trips, timeouts, retries).
//!
//! Tables live in `data/audit.db` alongside the existing guild_audit_log.
//! Schema is additive — zero migration needed for existing databases
//! (CREATE TABLE IF NOT EXISTS).

use rusqlite::params;

/// Start a new friction session for an agent. Returns the session_id.
pub fn start_session(agent_id: &str) -> Result<i64, String> {
    let conn = open_friction_db()?;
    ensure_schema(&conn)?;
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO friction_sessions (agent_id, started_at, status) VALUES (?1, ?2, 'active')",
        params![agent_id, now],
    ).map_err(|e| format!("start_session: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// End a friction session.
pub fn end_session(session_id: i64) -> Result<(), String> {
    let conn = open_friction_db()?;
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE friction_sessions SET ended_at = ?1, status = 'closed' WHERE id = ?2",
        params![now, session_id],
    ).map_err(|e| format!("end_session: {e}"))?;
    Ok(())
}

/// Start a friction workflow within a session. Returns workflow_id.
pub fn start_workflow(session_id: i64, intent: &str, guild: &str, tool: &str) -> Result<i64, String> {
    let conn = open_friction_db()?;
    ensure_schema(&conn)?;
    let now = chrono::Utc::now().to_rfc3339();
    let preview = &intent[..intent.len().min(200)];
    conn.execute(
        "INSERT INTO friction_workflows (session_id, intent, guild, tool, started_at, status)
         VALUES (?1, ?2, ?3, ?4, ?5, 'pending')",
        params![session_id, preview, guild, tool, now],
    ).map_err(|e| format!("start_workflow: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// End a workflow with its final status and round-trip count.
pub fn end_workflow(workflow_id: i64, status: &str, round_trips: i64) -> Result<(), String> {
    let conn = open_friction_db()?;
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE friction_workflows SET completed_at = ?1, status = ?2, round_trips = ?3 WHERE id = ?4",
        params![now, status, round_trips, workflow_id],
    ).map_err(|e| format!("end_workflow: {e}"))?;

    // Compute TTFUA if first_result_at was set
    let ttfua: Option<f64> = conn.query_row(
        "SELECT CASE WHEN first_result_at IS NOT NULL
         THEN (julianday(first_result_at) - julianday(started_at)) * 86400.0
         ELSE NULL END FROM friction_workflows WHERE id = ?1",
        params![workflow_id],
        |r| r.get(0),
    ).unwrap_or(None);

    if let Some(seconds) = ttfua {
        conn.execute(
            "UPDATE friction_workflows SET ttfua_seconds = ?1 WHERE id = ?2",
            params![seconds, workflow_id],
        ).ok();
    }

    Ok(())
}

/// Record the first result from a workflow (successful or not).
/// Used to compute TTFUA: time from workflow start to first actionable output.
pub fn record_workflow_result(workflow_id: i64) -> Result<(), String> {
    let conn = open_friction_db()?;
    ensure_schema(&conn)?;
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE friction_workflows SET first_result_at = ?1
         WHERE id = ?2 AND first_result_at IS NULL",
        params![now, workflow_id],
    ).map_err(|e| format!("record_workflow_result: {e}"))?;
    Ok(())
}

/// Log a friction event without a workflow binding. For use from the router
/// and handler_do where workflow context is not available.
pub fn log_friction_event_standalone(event_type: &str, description: &str) -> Result<i64, String> {
    let conn = open_friction_db()?;
    ensure_schema(&conn)?;
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO friction_events (workflow_id, event_type, timestamp, description, resolved)
         VALUES (0, ?1, ?2, ?3, 0)",
        params![event_type, now, description],
    ).map_err(|e| format!("log_friction_standalone: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// Shorthand: log a routing mismatch (wrong guild selected, tool call failed).
pub fn log_routing_mismatch(intent: &str, guild: &str, tool: &str, error: &str) {
    let desc = format!("intent='{intent}' routed to guild={guild} tool={tool} — {error:.100}");
    let _ = log_friction_event_standalone("routing_mismatch", &desc);
}

/// Shorthand: log a command blocklist rejection.
pub fn log_blocklist_rejection(intent: &str, command: &str) {
    let desc = format!("intent='{intent}' blocked: '{command}' not in allowed command list");
    let _ = log_friction_event_standalone("command_blocklist_rejection", &desc);
}

/// Shorthand: log a semantic precision failure (retrieval returned noise).
pub fn log_semantic_noise(intent: &str, expected_guild: &str, actual_guild: &str) {
    let desc = format!("intent='{intent}' expected guild={expected_guild} but semantic similarity routed to guild={actual_guild}");
    let _ = log_friction_event_standalone("semantic_precision_failure", &desc);
}
pub fn log_friction_event(
    workflow_id: i64,
    event_type: &str,
    description: &str,
) -> Result<i64, String> {
    let conn = open_friction_db()?;
    ensure_schema(&conn)?;
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO friction_events (workflow_id, event_type, timestamp, description, resolved)
         VALUES (?1, ?2, ?3, ?4, 0)",
        params![workflow_id, event_type, now, description],
    ).map_err(|e| format!("log_friction_event: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// Mark a friction event as resolved.
pub fn resolve_friction_event(event_id: i64, resolution: &str) -> Result<(), String> {
    let conn = open_friction_db()?;
    conn.execute(
        "UPDATE friction_events SET resolved = 1, resolution = ?1 WHERE id = ?2",
        params![resolution, event_id],
    ).map_err(|e| format!("resolve_friction_event: {e}"))?;
    Ok(())
}

/// Friction summary for a session.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FrictionStats {
    pub total_workflows: i64,
    pub completed_workflows: i64,
    pub failed_workflows: i64,
    pub total_events: i64,
    pub resolved_events: i64,
    pub manual_interventions: i64,
    pub routing_errors: i64,
    pub routing_ambiguous: i64,
    pub coloquio_roundtrips: i64,
    pub timeouts: i64,
    pub retries: i64,
    pub guild_errors: i64,
    pub avg_round_trips: f64,
    pub total_friction_score: f64,
    pub avg_ttfua_seconds: f64,
    pub median_ttfua_seconds: f64,
}

/// Get friction stats for a session.
pub fn get_session_friction(session_id: i64) -> Result<FrictionStats, String> {
    let conn = open_friction_db()?;
    ensure_schema(&conn)?;

    let stats = FrictionStats {
        total_workflows: conn.query_row(
            "SELECT COUNT(*) FROM friction_workflows WHERE session_id = ?1",
            params![session_id], |r| r.get(0),
        ).unwrap_or(0),
        completed_workflows: conn.query_row(
            "SELECT COUNT(*) FROM friction_workflows WHERE session_id = ?1 AND status = 'completed'",
            params![session_id], |r| r.get(0),
        ).unwrap_or(0),
        failed_workflows: conn.query_row(
            "SELECT COUNT(*) FROM friction_workflows WHERE session_id = ?1 AND status = 'failed'",
            params![session_id], |r| r.get(0),
        ).unwrap_or(0),
        total_events: conn.query_row(
            "SELECT COUNT(*) FROM friction_events e JOIN friction_workflows w ON e.workflow_id = w.id WHERE w.session_id = ?1",
            params![session_id], |r| r.get(0),
        ).unwrap_or(0),
        resolved_events: conn.query_row(
            "SELECT COUNT(*) FROM friction_events e JOIN friction_workflows w ON e.workflow_id = w.id WHERE w.session_id = ?1 AND e.resolved = 1",
            params![session_id], |r| r.get(0),
        ).unwrap_or(0),
        manual_interventions: count_events(&conn, session_id, "manual_intervention"),
        routing_errors: count_events(&conn, session_id, "routing_error"),
        routing_ambiguous: count_events(&conn, session_id, "routing_ambiguous"),
        coloquio_roundtrips: count_events(&conn, session_id, "coloquio_roundtrip"),
        timeouts: count_events(&conn, session_id, "timeout"),
        retries: count_events(&conn, session_id, "retry"),
        guild_errors: count_events(&conn, session_id, "guild_error"),
        avg_round_trips: conn.query_row(
            "SELECT COALESCE(AVG(round_trips), 0.0) FROM friction_workflows WHERE session_id = ?1",
            params![session_id], |r| r.get(0),
        ).unwrap_or(0.0),
        total_friction_score: compute_friction_score(&conn, session_id),
        avg_ttfua_seconds: conn.query_row(
            "SELECT COALESCE(AVG(ttfua_seconds), 0.0) FROM friction_workflows WHERE session_id = ?1 AND ttfua_seconds IS NOT NULL",
            params![session_id], |r| r.get(0),
        ).unwrap_or(0.0),
        median_ttfua_seconds: compute_median_ttfua(&conn, session_id),
    };

    Ok(stats)
}

fn count_events(conn: &rusqlite::Connection, session_id: i64, event_type: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM friction_events e JOIN friction_workflows w ON e.workflow_id = w.id WHERE w.session_id = ?1 AND e.event_type = ?2",
        params![session_id, event_type],
        |r| r.get(0),
    ).unwrap_or(0)
}

fn compute_median_ttfua(conn: &rusqlite::Connection, session_id: i64) -> f64 {
    let values: Vec<f64> = conn
        .prepare("SELECT ttfua_seconds FROM friction_workflows WHERE session_id = ?1 AND ttfua_seconds IS NOT NULL ORDER BY ttfua_seconds")
        .unwrap()
        .query_map(params![session_id], |r| r.get(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    if values.is_empty() { return 0.0; }
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) { (values[mid-1] + values[mid]) / 2.0 } else { values[mid] }
}

/// Composite friction score: higher = more friction.
///
/// ## Weights (provisional — calibrated via synthetic analysis Jul 2026)
/// | Event type           | Weight | Rationale |
/// |----------------------|--------|-----------|
/// | manual_intervention  | 5.0    | Direct human cost — agent failed autonomously, worst signal |
/// | routing_error        | 3.0    | Wrong guild selected, wastes round-trip; medium-high severity |
/// | timeout              | 2.0    | Timeout == slow response + likely retry; pairs with retry at same weight |
/// | retry                | 2.0    | Wasted work + latency; pairs with timeout at same weight |
/// | guild_error          | 1.0    | Guild-level failure but may resolve internally; minor |
/// | routing_ambiguous    | 1.0    | Ambiguity is normal in routing; minor unless persistent |
/// | coloquio_roundtrip   | 0.5    | Coordination overhead; lowest severity, part of normal workflow |
///
/// **Status:** PROVISIONAL. Synthetic analysis (80 sessions, 6 profiles) shows
/// all 7 weights are FRAGILE under ±50% perturbation (~11-13 mean rank shift)
/// because event types are highly correlated — sessions with many manual_interventions
/// also have routing_errors, retries, etc. The weights are ADEQUATE for binary
/// separation (high-friction score ~108 vs smooth ~0) but granular ranking within
/// the middle band is unreliable. Re-calibrate when >50 real sessions with events exist.
/// Consider simplifying to 3 tiers: Critical=5, Significant=2, Minor=1.
fn compute_friction_score(conn: &rusqlite::Connection, session_id: i64) -> f64 {
    let weights: Vec<(&str, f64)> = vec![
        ("manual_intervention", 5.0),
        ("routing_error", 3.0),
        ("timeout", 2.0),
        ("retry", 2.0),
        ("guild_error", 1.0),
        ("routing_ambiguous", 1.0),
        ("coloquio_roundtrip", 0.5),
    ];
    let total: f64 = weights.iter().map(|(et, w)| {
        count_events(conn, session_id, et) as f64 * w
    }).sum();
    (total * 10.0).round() / 10.0
}

/// Global friction stats across all sessions (for dashboard).
#[derive(Debug, Clone, serde::Serialize)]
pub struct GlobalFrictionStats {
    pub total_sessions: i64,
    pub total_workflows: i64,
    pub total_events: i64,
    pub manual_interventions: i64,
    pub routing_errors: i64,
    pub routing_ambiguous: i64,
    pub coloquio_roundtrips: i64,
    pub timeouts: i64,
    pub retries: i64,
    pub guild_errors: i64,
    pub avg_round_trips_per_workflow: f64,
    pub total_friction_score: f64,
    pub avg_ttfua_seconds: f64,
}

pub fn get_global_friction_stats() -> GlobalFrictionStats {
    let conn = match open_friction_db() {
        Ok(c) => c,
        Err(_) => return GlobalFrictionStats {
            total_sessions: 0, total_workflows: 0, total_events: 0,
            manual_interventions: 0, routing_errors: 0, routing_ambiguous: 0,
            coloquio_roundtrips: 0, timeouts: 0, retries: 0, guild_errors: 0,
            avg_round_trips_per_workflow: 0.0, total_friction_score: 0.0,
            avg_ttfua_seconds: 0.0,
        },
    };
    let _ = ensure_schema(&conn);

    let total_sessions: i64 = conn.query_row("SELECT COUNT(*) FROM friction_sessions", [], |r| r.get(0)).unwrap_or(0);
    let total_workflows: i64 = conn.query_row("SELECT COUNT(*) FROM friction_workflows", [], |r| r.get(0)).unwrap_or(0);
    let total_events: i64 = conn.query_row("SELECT COUNT(*) FROM friction_events", [], |r| r.get(0)).unwrap_or(0);

    let avg_rt: f64 = conn.query_row(
        "SELECT COALESCE(AVG(round_trips), 0.0) FROM friction_workflows", [], |r| r.get(0)
    ).unwrap_or(0.0);

    // Sum friction score across all sessions
    let rows: Vec<i64> = conn.prepare("SELECT id FROM friction_sessions")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    let total_score: f64 = rows.iter().map(|sid| compute_friction_score(&conn, *sid)).sum();

    GlobalFrictionStats {
        total_sessions,
        total_workflows,
        total_events,
        manual_interventions: count_event_type_global(&conn, "manual_intervention"),
        routing_errors: count_event_type_global(&conn, "routing_error"),
        routing_ambiguous: count_event_type_global(&conn, "routing_ambiguous"),
        coloquio_roundtrips: count_event_type_global(&conn, "coloquio_roundtrip"),
        timeouts: count_event_type_global(&conn, "timeout"),
        retries: count_event_type_global(&conn, "retry"),
        guild_errors: count_event_type_global(&conn, "guild_error"),
        avg_round_trips_per_workflow: avg_rt,
        total_friction_score: (total_score * 10.0).round() / 10.0,
        avg_ttfua_seconds: conn.query_row(
            "SELECT COALESCE(AVG(ttfua_seconds), 0.0) FROM friction_workflows WHERE ttfua_seconds IS NOT NULL",
            [], |r| r.get(0),
        ).unwrap_or(0.0),
    }
}

fn count_event_type_global(conn: &rusqlite::Connection, event_type: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM friction_events WHERE event_type = ?1",
        params![event_type],
        |r| r.get(0),
    ).unwrap_or(0)
}

fn open_friction_db() -> Result<rusqlite::Connection, String> {
    let db_path = std::path::Path::new("./data/audit.db");
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("friction mkdir: {e}"))?;
    }
    let conn = crate::config::open_db(db_path).map_err(|e| format!("friction open: {e}"))?;
    conn.busy_timeout(std::time::Duration::from_secs(5)).ok();
    ensure_schema(&conn)?;
    Ok(conn)
}

fn ensure_schema(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS friction_sessions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            agent_id TEXT NOT NULL,
            started_at TEXT NOT NULL,
            ended_at TEXT,
            status TEXT NOT NULL DEFAULT 'active'
        );

        CREATE TABLE IF NOT EXISTS friction_workflows (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            intent TEXT NOT NULL,
            guild TEXT DEFAULT '',
            tool TEXT DEFAULT '',
            started_at TEXT NOT NULL,
            first_result_at TEXT,
            completed_at TEXT,
            status TEXT NOT NULL DEFAULT 'pending',
            round_trips INTEGER NOT NULL DEFAULT 0,
            ttfua_seconds REAL
        );

        CREATE TABLE IF NOT EXISTS friction_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            workflow_id INTEGER NOT NULL,
            event_type TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            resolved INTEGER NOT NULL DEFAULT 0,
            resolution TEXT DEFAULT ''
        );

        CREATE INDEX IF NOT EXISTS idx_friction_sessions_agent ON friction_sessions(agent_id);
        CREATE INDEX IF NOT EXISTS idx_friction_workflows_session ON friction_workflows(session_id);
        CREATE INDEX IF NOT EXISTS idx_friction_events_workflow ON friction_events(workflow_id);
        CREATE INDEX IF NOT EXISTS idx_friction_events_type ON friction_events(event_type);

        -- Schema v2: add TTFUA columns to existing tables
        ALTER TABLE friction_workflows ADD COLUMN first_result_at TEXT;
        ALTER TABLE friction_workflows ADD COLUMN ttfua_seconds REAL;"
    ).or_else(|e| {
        // Ignore "duplicate column name" errors from ALTER TABLE on existing DBs
        if e.to_string().contains("duplicate column") { Ok(()) } else { Err(e) }
    }).map_err(|e| format!("friction schema: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);
    static TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn unique_agent() -> String {
        let n = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("test-friction-agent-{n}")
    }

    #[test]
    fn test_full_friction_lifecycle() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let agent = unique_agent();

        // Start session
        let sid = start_session(&agent).expect("start_session failed");
        assert!(sid > 0);

        // Start workflow
        let wid = start_workflow(sid, "fix bug in routing", "code", "code_analyze")
            .expect("start_workflow failed");
        assert!(wid > 0);

        // Log events
        let e1 = log_friction_event(wid, "routing_error", "intent was routed to wrong guild")
            .expect("log_event failed");
        assert!(e1 > 0);

        let e2 = log_friction_event(wid, "manual_intervention", "agent specified guild=bash manually")
            .expect("log_event failed");
        assert!(e2 > 0);

        let e3 = log_friction_event(wid, "retry", "retried with correct guild")
            .expect("log_event failed");
        assert!(e3 > 0);

        // Resolve one event
        resolve_friction_event(e1, "added verb trigger to matcher.rs").expect("resolve failed");

        // End workflow with 3 round-trips
        end_workflow(wid, "completed", 3).expect("end_workflow failed");

        // End session
        end_session(sid).expect("end_session failed");

        // Get stats
        let stats = get_session_friction(sid).expect("get_session_friction failed");
        assert_eq!(stats.total_workflows, 1);
        assert_eq!(stats.completed_workflows, 1);
        assert_eq!(stats.total_events, 3);
        assert_eq!(stats.resolved_events, 1);
        assert_eq!(stats.routing_errors, 1);
        assert_eq!(stats.manual_interventions, 1);
        assert_eq!(stats.retries, 1);
        assert_eq!(stats.avg_round_trips, 3.0);
        assert!(stats.total_friction_score > 0.0);
    }

    #[test]
    fn test_empty_session_has_zero_friction() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let agent = unique_agent();
        let sid = start_session(&agent).expect("start_session failed");
        end_session(sid).expect("end_session failed");
        let stats = get_session_friction(sid).expect("get_session_friction failed");
        assert_eq!(stats.total_workflows, 0);
        assert_eq!(stats.total_events, 0);
        assert_eq!(stats.total_friction_score, 0.0);
    }

    #[test]
    fn test_multiple_workflows() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let agent = unique_agent();
        let sid = start_session(&agent).expect("start_session failed");

        for i in 0..3 {
            let wid = start_workflow(sid, &format!("task {i}"), "bash", "bash_execute")
                .expect("start_workflow failed");
            if i == 1 {
                log_friction_event(wid, "timeout", "command timed out").expect("log failed");
            }
            end_workflow(wid, "completed", 0).expect("end_workflow failed");
        }
        end_session(sid).expect("end_session failed");

        let stats = get_session_friction(sid).expect("get_session_friction failed");
        assert_eq!(stats.total_workflows, 3);
        assert_eq!(stats.completed_workflows, 3);
        assert_eq!(stats.timeouts, 1);
    }

    #[test]
    fn test_ttfua_computed_on_record_result() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let agent = unique_agent();
        let sid = start_session(&agent).expect("start_session failed");
        let wid = start_workflow(sid, "slow task", "bash", "bash_execute")
            .expect("start_workflow failed");

        // Simulate a delay between workflow start and first useful result.
        std::thread::sleep(std::time::Duration::from_millis(50));
        record_workflow_result(wid).expect("record_workflow_result failed");
        end_workflow(wid, "completed", 0).expect("end_workflow failed");
        end_session(sid).expect("end_session failed");

        let stats = get_session_friction(sid).expect("get_session_friction failed");
        assert!(stats.avg_ttfua_seconds > 0.0, "TTFUA should be positive after a real delay");
        assert!(stats.avg_ttfua_seconds < 5.0, "TTFUA should be small for a 50ms test delay, got {}", stats.avg_ttfua_seconds);
        assert_eq!(stats.median_ttfua_seconds, stats.avg_ttfua_seconds, "single-sample median must equal the average");
    }

    #[test]
    fn test_ttfua_null_without_recorded_result() {
        let agent = unique_agent();
        let sid = start_session(&agent).expect("start_session failed");
        let wid = start_workflow(sid, "no result recorded", "bash", "bash_execute")
            .expect("start_workflow failed");
        end_workflow(wid, "completed", 0).expect("end_workflow failed");
        end_session(sid).expect("end_session failed");

        let stats = get_session_friction(sid).expect("get_session_friction failed");
        assert_eq!(stats.avg_ttfua_seconds, 0.0, "no TTFUA samples should average to the COALESCE default");
    }
}
