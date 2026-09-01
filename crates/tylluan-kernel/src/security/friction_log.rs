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
//!
//! [`FrictionStore`] wraps a `rusqlite::Connection` with a stable path, so
//! production and tests can each open their own isolated database without
//! sharing a global mutex.

use rusqlite::params;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// FrictionStore — owns a connection to a specific friction DB
// ---------------------------------------------------------------------------

/// A handle to a single friction-database instance.
///
/// Call [`FrictionStore::open`] to create one; pass the returned store to
/// any function that needs DB access. Production uses `./data/audit.db`;
/// tests use a dedicated [`tempfile::TempDir`] path.
pub struct FrictionStore {
    conn: rusqlite::Connection,
    #[allow(dead_code)]
    path: PathBuf,
}

impl FrictionStore {
    /// Open (or create) the friction database at `path`.
    ///
    /// Creates the parent directory if needed, applies `busy_timeout(5s)`,
    /// and ensures the schema is up-to-date.
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("friction mkdir: {e}"))?;
        }
        let conn =
            crate::config::open_db(path).map_err(|e| format!("friction open: {e}"))?;
        conn.busy_timeout(std::time::Duration::from_secs(5)).ok();
        ensure_schema(&conn)?;
        Ok(Self { conn, path: path.to_path_buf() })
    }

    /// Borrow the underlying connection.
    pub fn conn(&self) -> &rusqlite::Connection {
        &self.conn
    }
}

// ---------------------------------------------------------------------------
// Backward-compatible helpers — delegate to FrictionStore internally
// ---------------------------------------------------------------------------

/// Test-only override for the friction DB path. Every lib test that touches
/// friction log writes to its OWN temp DB (see the tests module) so the suite
/// never contends with the live kernel, which holds `./data/audit.db` open and
/// writes friction entries on every tool call. TEST_MUTEX serializes the tests
/// among themselves but cannot serialize them against another process — that
/// cross-process contention was the root cause of the flaky
/// "friction open: unable to open database file: ./data/audit.db" failures.
#[cfg(test)]
static TEST_DB_PATH: std::sync::Mutex<Option<std::path::PathBuf>> = std::sync::Mutex::new(None);

/// Serializa los tests que escriben a la DB de test (friction_log,
/// llm_examples, router matcher): TODOS apuntan al mismo TEST_DB_PATH global,
/// así que un seteo concurrente desviaría los INSERTs a otra db temporal.
/// Tómalo en cualquier test que (transitivamente) escriba al audit path.
#[cfg(test)]
pub(crate) static TEST_DB_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
static TEST_COUNTER: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Point every friction write at a dedicated temp DB (unique within the
/// process). Production keeps `./data/audit.db`; tests never touch it, so the
/// live kernel can hold it without flaking us. Visible to other modules'
/// tests (e.g. router::matcher) whose code paths log friction events.
#[cfg(test)]
pub(crate) fn set_unique_test_db() {
    let n = TEST_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "tylluan_friction_test_{}_{}.db",
        std::process::id(),
        n
    ));
    *TEST_DB_PATH.lock().unwrap() = Some(path);
}

/// Resolve the friction DB path. Tests may redirect it via
/// `set_unique_test_db()`; other modules (e.g. `llm_examples`) reuse it so
/// all audit-style writes share the same DB and test isolation.
pub(crate) fn friction_db_path() -> std::path::PathBuf {
    #[cfg(test)]
    {
        if let Some(p) = TEST_DB_PATH.lock().unwrap().clone() {
            return p;
        }
    }

    std::path::PathBuf::from("./data/audit.db")
}

/// Open the friction database using the default path resolution
/// (`friction_db_path()`). All production functions delegate here.
fn open_friction_db() -> Result<FrictionStore, String> {
    FrictionStore::open(&friction_db_path())
}

// ---------------------------------------------------------------------------
// Schema — each ALTER TABLE is verified independently
// ---------------------------------------------------------------------------

fn ensure_schema(conn: &rusqlite::Connection) -> Result<(), String> {
    // Core tables (idempotent)
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
        CREATE INDEX IF NOT EXISTS idx_friction_events_type ON friction_events(event_type);"
    )
    .map_err(|e| format!("friction schema: {e}"))?;

    // Schema v2: add TTFUA columns only if missing.
    // Each ALTER is verified independently so a partial v19→v20 DB does not
    // cause the entire batch to fail.
    ensure_column(conn, "friction_workflows", "first_result_at", "TEXT")?;
    ensure_column(conn, "friction_workflows", "ttfua_seconds", "REAL")?;

    Ok(())
}

/// If `table` does not already have `column`, add it with the given SQL type.
///
/// Table and column names are interpolated into SQL (PRAGMA/ALTER don't take
/// bound parameters), so both are whitelisted against the known friction
/// schema -- anything else is rejected before it can reach the connection.
const KNOWN_TABLES: &[&str] = &["friction_sessions", "friction_workflows", "friction_events"];
const KNOWN_COLUMNS: &[&str] = &[
    "id", "agent_id", "started_at", "ended_at", "status", "task_goal",
    "first_result_at", "ttfua_seconds", "workflow_id", "event_type",
    "timestamp", "description", "resolved", "resolution",
];

fn ensure_column(
    conn: &rusqlite::Connection,
    table: &str,
    column: &str,
    sql_type: &str,
) -> Result<(), String> {
    if !KNOWN_TABLES.contains(&table) {
        return Err(format!("ensure_column: table '{table}' not in friction schema whitelist"));
    }
    if !KNOWN_COLUMNS.contains(&column) {
        return Err(format!("ensure_column: column '{column}' not in friction schema whitelist"));
    }
    if !sql_type.chars().all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '(' || c == ')') {
        return Err(format!("ensure_column: suspicious sql_type '{sql_type}'"));
    }
    let exists: bool = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|e| format!("friction ensure_column pragma: {e}"))?
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| format!("friction ensure_column query: {e}"))?
        .filter_map(|r| r.ok())
        .any(|name| name == column);

    if !exists {
        conn.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {sql_type}"
        ))
        .map_err(|e| format!("friction ensure_column add {table}.{column}: {e}"))?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Public API — session / workflow / event management
// ---------------------------------------------------------------------------

/// Start a new friction session for an agent. Returns the session_id.
pub fn start_session(agent_id: &str) -> Result<i64, String> {
    let store = open_friction_db()?;
    let now = chrono::Utc::now().to_rfc3339();
    store.conn().execute(
        "INSERT INTO friction_sessions (agent_id, started_at, status) VALUES (?1, ?2, 'active')",
        params![agent_id, now],
    ).map_err(|e| format!("start_session: {e}"))?;
    Ok(store.conn().last_insert_rowid())
}

/// End a friction session.
pub fn end_session(session_id: i64) -> Result<(), String> {
    let store = open_friction_db()?;
    let now = chrono::Utc::now().to_rfc3339();
    store.conn().execute(
        "UPDATE friction_sessions SET ended_at = ?1, status = 'closed' WHERE id = ?2",
        params![now, session_id],
    ).map_err(|e| format!("end_session: {e}"))?;
    Ok(())
}

/// Start a friction workflow within a session. Returns workflow_id.
pub fn start_workflow(session_id: i64, intent: &str, guild: &str, tool: &str) -> Result<i64, String> {
    let store = open_friction_db()?;
    let now = chrono::Utc::now().to_rfc3339();
    let preview = &intent[..intent.len().min(200)];
    store.conn().execute(
        "INSERT INTO friction_workflows (session_id, intent, guild, tool, started_at, status)
         VALUES (?1, ?2, ?3, ?4, ?5, 'pending')",
        params![session_id, preview, guild, tool, now],
    ).map_err(|e| format!("start_workflow: {e}"))?;
    Ok(store.conn().last_insert_rowid())
}

/// End a workflow with its final status and round-trip count.
pub fn end_workflow(workflow_id: i64, status: &str, round_trips: i64) -> Result<(), String> {
    let store = open_friction_db()?;
    let now = chrono::Utc::now().to_rfc3339();
    store.conn().execute(
        "UPDATE friction_workflows SET completed_at = ?1, status = ?2, round_trips = ?3 WHERE id = ?4",
        params![now, status, round_trips, workflow_id],
    ).map_err(|e| format!("end_workflow: {e}"))?;

    // Compute TTFUA if first_result_at was set
    let ttfua: Option<f64> = store.conn().query_row(
        "SELECT CASE WHEN first_result_at IS NOT NULL
         THEN (julianday(first_result_at) - julianday(started_at)) * 86400.0
         ELSE NULL END FROM friction_workflows WHERE id = ?1",
        params![workflow_id],
        |r| r.get(0),
    ).unwrap_or(None);

    if let Some(seconds) = ttfua {
        store.conn().execute(
            "UPDATE friction_workflows SET ttfua_seconds = ?1 WHERE id = ?2",
            params![seconds, workflow_id],
        ).ok();
    }

    Ok(())
}

/// Record the first result from a workflow (successful or not).
/// Used to compute TTFUA: time from workflow start to first actionable output.
pub fn record_workflow_result(workflow_id: i64) -> Result<(), String> {
    let store = open_friction_db()?;
    let now = chrono::Utc::now().to_rfc3339();
    store.conn().execute(
        "UPDATE friction_workflows SET first_result_at = ?1
         WHERE id = ?2 AND first_result_at IS NULL",
        params![now, workflow_id],
    ).map_err(|e| format!("record_workflow_result: {e}"))?;
    Ok(())
}

/// Log a friction event without a workflow binding. For use from the router
/// and handler_do where workflow context is not available.
pub fn log_friction_event_standalone(event_type: &str, description: &str) -> Result<i64, String> {
    let store = open_friction_db()?;
    let now = chrono::Utc::now().to_rfc3339();
    store.conn().execute(
        "INSERT INTO friction_events (workflow_id, event_type, timestamp, description, resolved)
         VALUES (0, ?1, ?2, ?3, 0)",
        params![event_type, now, description],
    ).map_err(|e| format!("log_friction_standalone: {e}"))?;
    Ok(store.conn().last_insert_rowid())
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
    let store = open_friction_db()?;
    let now = chrono::Utc::now().to_rfc3339();
    store.conn().execute(
        "INSERT INTO friction_events (workflow_id, event_type, timestamp, description, resolved)
         VALUES (?1, ?2, ?3, ?4, 0)",
        params![workflow_id, event_type, now, description],
    ).map_err(|e| format!("log_friction_event: {e}"))?;
    Ok(store.conn().last_insert_rowid())
}

/// Mark a friction event as resolved.
pub fn resolve_friction_event(event_id: i64, resolution: &str) -> Result<(), String> {
    let store = open_friction_db()?;
    store.conn().execute(
        "UPDATE friction_events SET resolved = 1, resolution = ?1 WHERE id = ?2",
        params![resolution, event_id],
    ).map_err(|e| format!("resolve_friction_event: {e}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

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
    let store = open_friction_db()?;
    let conn = store.conn();

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
        manual_interventions: count_events(conn, session_id, "manual_intervention"),
        routing_errors: count_events(conn, session_id, "routing_error"),
        routing_ambiguous: count_events(conn, session_id, "routing_ambiguous"),
        coloquio_roundtrips: count_events(conn, session_id, "coloquio_roundtrip"),
        timeouts: count_events(conn, session_id, "timeout"),
        retries: count_events(conn, session_id, "retry"),
        guild_errors: count_events(conn, session_id, "guild_error"),
        avg_round_trips: conn.query_row(
            "SELECT COALESCE(AVG(round_trips), 0.0) FROM friction_workflows WHERE session_id = ?1",
            params![session_id], |r| r.get(0),
        ).unwrap_or(0.0),
        total_friction_score: compute_friction_score(conn, session_id),
        avg_ttfua_seconds: conn.query_row(
            "SELECT COALESCE(AVG(ttfua_seconds), 0.0) FROM friction_workflows WHERE session_id = ?1 AND ttfua_seconds IS NOT NULL",
            params![session_id], |r| r.get(0),
        ).unwrap_or(0.0),
        median_ttfua_seconds: compute_median_ttfua(conn, session_id),
    };

    Ok(stats)
}

fn count_events(conn: &rusqlite::Connection, session_id: i64, event_type: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM friction_events e JOIN friction_workflows w ON e.workflow_id = w.id WHERE w.session_id = ?1 AND e.event_type = ?2",
        params![session_id, event_type], |r| r.get(0),
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
    let store = match open_friction_db() {
        Ok(s) => s,
        Err(_) => return GlobalFrictionStats {
            total_sessions: 0, total_workflows: 0, total_events: 0,
            manual_interventions: 0, routing_errors: 0, routing_ambiguous: 0,
            coloquio_roundtrips: 0, timeouts: 0, retries: 0, guild_errors: 0,
            avg_round_trips_per_workflow: 0.0, total_friction_score: 0.0,
            avg_ttfua_seconds: 0.0,
        },
    };
    let conn = store.conn();

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

    let total_score: f64 = rows.iter().map(|sid| compute_friction_score(conn, *sid)).sum();

    GlobalFrictionStats {
        total_sessions,
        total_workflows,
        total_events,
        manual_interventions: count_event_type_global(conn, "manual_intervention"),
        routing_errors: count_event_type_global(conn, "routing_error"),
        routing_ambiguous: count_event_type_global(conn, "routing_ambiguous"),
        coloquio_roundtrips: count_event_type_global(conn, "coloquio_roundtrip"),
        timeouts: count_event_type_global(conn, "timeout"),
        retries: count_event_type_global(conn, "retry"),
        guild_errors: count_event_type_global(conn, "guild_error"),
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
        params![event_type], |r| r.get(0),
    ).unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);
    use super::TEST_DB_MUTEX as TEST_MUTEX;

    fn unique_agent() -> String {
        let n = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("test-friction-agent-{n}")
    }

    /// Point every friction call in THIS test at a dedicated temp DB (one per
    /// test, unique within the process). Production keeps `./data/audit.db`;
    /// tests never touch it, so the live kernel can hold it without flaking us.
    fn unique_test_db() {
        set_unique_test_db();
    }

    #[test]
    fn test_full_friction_lifecycle() {
        let _guard = TEST_MUTEX.lock().unwrap();
        unique_test_db();
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
        unique_test_db();
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
        unique_test_db();
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
        unique_test_db();
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
        let _guard = TEST_MUTEX.lock().unwrap();
        unique_test_db();
        let agent = unique_agent();
        let sid = start_session(&agent).expect("start_session failed");
        let wid = start_workflow(sid, "no result recorded", "bash", "bash_execute")
            .expect("start_workflow failed");
        end_workflow(wid, "completed", 0).expect("end_workflow failed");
        end_session(sid).expect("end_session failed");

        let stats = get_session_friction(sid).expect("get_session_friction failed");
        assert_eq!(stats.avg_ttfua_seconds, 0.0, "no TTFUA samples should average to the COALESCE default");
    }

    // -----------------------------------------------------------------------
    // FrictionStore isolation test — two stores, two paths, no shared mutex
    // -----------------------------------------------------------------------

    #[test]
    fn test_two_stores_concurrent_no_shared_mutex() {
        use std::thread;

        // Create two fully independent temp directories — no TEST_DB_PATH,
        // no TEST_DB_MUTEX needed. Each thread owns its own FrictionStore.
        let dir_a = tempfile::tempdir().unwrap();
        let dir_b = tempfile::tempdir().unwrap();
        let path_a = dir_a.path().join("friction_a.db");
        let path_b = dir_b.path().join("friction_b.db");
        let verify_path_a = path_a.clone();

        let handle_a = thread::spawn(move || {
            let store = FrictionStore::open(&path_a).expect("open store A");
            let conn = store.conn();
            conn.execute(
                "INSERT INTO friction_sessions (agent_id, started_at, status) VALUES ('agent_a', '2026-01-01', 'active')",
                [],
            )
            .unwrap();
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM friction_sessions", [], |r| r.get(0))
                .unwrap();
            assert_eq!(count, 1, "store A should have exactly 1 session");
        });

        let handle_b = thread::spawn(move || {
            let store = FrictionStore::open(&path_b).expect("open store B");
            let conn = store.conn();
            // Insert 3 sessions — store B has more data than store A
            for i in 0..3 {
                conn.execute(
                    "INSERT INTO friction_sessions (agent_id, started_at, status) VALUES (?1, '2026-01-01', 'active')",
                    params![format!("agent_b_{i}")],
                )
                .unwrap();
            }
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM friction_sessions", [], |r| r.get(0))
                .unwrap();
            assert_eq!(count, 3, "store B should have exactly 3 sessions");
        });

        handle_a.join().expect("thread A panicked");
        handle_b.join().expect("thread B panicked");

        // Verify store A was not contaminated by store B
        let verify = FrictionStore::open(&verify_path_a).unwrap();
        let count_a: i64 = verify
            .conn()
            .query_row("SELECT COUNT(*) FROM friction_sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count_a, 1, "store A still has exactly 1 session after both threads ran");
    }
}
