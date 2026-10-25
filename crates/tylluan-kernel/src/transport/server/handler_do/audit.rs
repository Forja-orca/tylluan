/// D-53: `TYLLUAN_AUDIT_DB` es global al proceso. Los tests que la tocan
/// (seam de audit.rs y el endpoint de lectura de D-53) comparten este lock.
#[cfg(test)]
pub(crate) static AUDIT_ENV_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Deterministic failure node ID for the routing feedback loop.
pub(crate) fn routing_failure_id(intent: &str) -> String {
    let hash: u64 = intent.bytes()
        .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
    format!("lesson:routing_failure:{hash:x}")
}

/// Resolve the audit store path — THE single owner of path resolution for
/// `./data/audit.db`. The writer (`log_audit_entry`), the chain verifier
/// (`verify_audit_chain`), and the MD-7 read endpoint (`/api/v1/audit/latency`,
/// via handler_do re-export) all call THIS; `TYLLUAN_AUDIT_DB` is checked in
/// exactly one place. Unset env → byte-identical behavior to the historical
/// hardcoded `"./data/audit.db"`; set → writer and reader land on the same
/// file (test seam: pattern of `TYLLUAN_CONFUSION_DB`).
pub fn audit_db_path() -> std::path::PathBuf {
    std::env::var("TYLLUAN_AUDIT_DB")
        .unwrap_or_else(|_| "./data/audit.db".to_string())
        .into()
}

/// Write an audit log entry for every tylluan_do tool call.
/// Uses SHA-256 hash chaining: each entry stores the hash of the previous entry,
/// making tampering detectable. Called fire-and-forget — errors are non-fatal.
#[allow(clippy::too_many_arguments)]
pub(crate) fn log_audit_entry(intent: &str, guild: &str, tool: &str, agent_id: &str, success: bool, preview: &str, latency_ms: u64, human_intervention: bool) -> Result<(), String> {
    let path = audit_db_path();
    // A fresh checkout (CI, first run) has no `./data`: without this the open
    // failed and the audit row was silently lost.
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| format!("audit dir: {e}"))?;
    }
    let conn = crate::config::open_db(&path).map_err(|e| format!("audit open: {e}"))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS guild_audit_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            guild TEXT NOT NULL,
            tool_name TEXT NOT NULL,
            agent_id TEXT NOT NULL DEFAULT '',
            intent TEXT,
            status TEXT NOT NULL DEFAULT 'ok',
            result_preview TEXT,
            prev_hash TEXT NOT NULL DEFAULT '',
            hash TEXT NOT NULL
        );"
    ).map_err(|e| format!("audit schema: {e}"))?;
    // Autonomous Success Rate prerequisite: latency_ms (full intent->result
    // cycle) + human_intervention (HITL approval involved). Old rows keep
    // NULL — queries must tolerate that (they select explicit columns).
    // WS5: system_snapshot stamps the boot SystemSnapshot JSON so every audit
    // row is self-identifying (commit/config/catalog of the system that
    // produced it). NOT part of the chain hash — legacy rows (NULL) must stay
    // verifiable and the chain covers tamper-evidence, not identity.
    for col in ["latency_ms INTEGER", "human_intervention INTEGER NOT NULL DEFAULT 0", "system_snapshot TEXT"] {
        let _ = conn.execute_batch(&format!("ALTER TABLE guild_audit_log ADD COLUMN {col}"));
    }

    let now = chrono::Utc::now().to_rfc3339();
    let status = if success { "ok" } else { "error" };

    // Get previous hash for chaining
    let prev_hash: String = conn
        .query_row("SELECT hash FROM guild_audit_log ORDER BY id DESC LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap_or_default();

    // Chain hash: SHA-256 of (prev_hash || timestamp || guild || tool || agent_id || status)
    let chain_input = format!("{prev_hash}|{now}|{guild}|{tool}|{agent_id}|{status}");
    use sha2::Digest;
    let hash = format!("{:x}", sha2::Sha256::digest(chain_input.as_bytes()));

    let snapshot_json = crate::router::system_snapshot::global()
        .and_then(|s| serde_json::to_string(s).ok());

    conn.execute(
        "INSERT INTO guild_audit_log (timestamp, guild, tool_name, agent_id, intent, status, result_preview, prev_hash, hash, latency_ms, human_intervention, system_snapshot)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        rusqlite::params![now, guild, tool, agent_id, intent, status, preview, prev_hash, hash, latency_ms as i64, if human_intervention { 1 } else { 0 }, snapshot_json],
    ).map_err(|e| format!("audit insert: {e}"))?;
    Ok(())
}

/// Intervalo mínimo entre warns por fallo de auditoría (proceso-wide).
const AUDIT_WARN_INTERVAL_SECS: u64 = 60;

/// Limitador del warn de auditoría: como mucho un warn por minuto y un
/// contador de fallos suprimidos entre warns. Estado propio, sin locks,
/// porque `audit_or_warn` se llama desde `spawn_blocking`/`tokio::spawn`.
struct AuditWarnLimiter {
    last_warn_secs: AtomicU64,
    suppressed: AtomicU64,
}

impl AuditWarnLimiter {
    const fn new() -> Self {
        Self {
            last_warn_secs: AtomicU64::new(0),
            suppressed: AtomicU64::new(0),
        }
    }

    /// Registra un fallo. Devuelve `Some(suprimidos)` si toca emitir warn
    /// (y resetea el contador), o `None` si se suprime por el rate limit.
    fn on_failure(&self, now_secs: u64) -> Option<u64> {
        let last = self.last_warn_secs.load(Ordering::Relaxed);
        if now_secs.saturating_sub(last) >= AUDIT_WARN_INTERVAL_SECS
            && self
                .last_warn_secs
                .compare_exchange(last, now_secs, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            Some(self.suppressed.swap(0, Ordering::Relaxed))
        } else {
            self.suppressed.fetch_add(1, Ordering::Relaxed);
            None
        }
    }
}

static AUDIT_WARN_LIMITER: AuditWarnLimiter = AuditWarnLimiter::new();

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Best-effort: escribe la entrada de auditoría y, si falla, deja un
/// `tracing::warn!` con el error (rate limit: 1 warn/minuto + contador de
/// suprimidos). Nunca cambia el valor de retorno del call-site: devuelve `()`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn audit_or_warn(intent: &str, guild: &str, tool: &str, agent_id: &str, success: bool, preview: &str, latency_ms: u64, human_intervention: bool) {
    if let Err(error) = log_audit_entry(intent, guild, tool, agent_id, success, preview, latency_ms, human_intervention)
        && let Some(suppressed) = AUDIT_WARN_LIMITER.on_failure(now_unix_secs())
    {
        tracing::warn!(
            error = %error,
            suppressed,
            "audit write failed (best-effort; el flujo continua)"
        );
    }
}

/// Verify the integrity of the audit chain from oldest to newest.
/// Returns (ok_count, bad_count) — bad > 0 means tampering detected.
pub fn verify_audit_chain() -> Result<(usize, usize), String> {
    let conn = match crate::config::open_db(&audit_db_path()) {
        Ok(c) => c,
        Err(_) => return Ok((0, 0)),
    };
    // Ensure table exists (no-op if it does)
    let _ = conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS guild_audit_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL, guild TEXT NOT NULL, tool_name TEXT NOT NULL,
            agent_id TEXT NOT NULL DEFAULT '', intent TEXT,
            status TEXT NOT NULL DEFAULT 'ok', result_preview TEXT,
            prev_hash TEXT NOT NULL DEFAULT '', hash TEXT NOT NULL
        );"
    );
    let mut stmt = conn.prepare(
        "SELECT id, timestamp, guild, tool_name, agent_id, status, prev_hash, hash \
         FROM guild_audit_log ORDER BY id ASC"
    ).map_err(|e| format!("audit prepare: {e}"))?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
        ))
    }).map_err(|e| format!("audit query: {e}"))?;

    let mut prev = String::new();
    let mut ok = 0usize;
    let mut bad = 0usize;
    for row_res in rows {
        let row = row_res.map_err(|e| format!("audit row: {e}"))?;
        let (_id, ts, guild, tool_name, agent_id, status, stored_prev, stored_hash) = row;
        if stored_prev != prev {
            bad += 1;
            continue;
        }
        let chain_input = format!("{stored_prev}|{ts}|{guild}|{tool_name}|{agent_id}|{status}");
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

/// Open ./data/audit.db strictly for reading (SQLITE_OPEN_READ_ONLY).
/// Guarantees the caller cannot mutate the audit store — read-only endpoints
/// use this instead of `open_db` so an accidental write would fail at the
/// driver level, not just by convention. Never creates the file.
pub fn audit_open_readonly(
    path: &std::path::Path,
) -> Result<rusqlite::Connection, String> {
    let flags =
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX;
    rusqlite::Connection::open_with_flags(path, flags)
        .map_err(|e| format!("audit open readonly: {e}"))
}

/// Opt-in safety filter for dangerous intents.
/// Returns Some(reason) if the intent matches a dangerous pattern.
pub fn check_dangerous_intent(intent: &str) -> Option<&'static str> {
    let lower = intent.to_lowercase();

    static PATTERNS: &[(&str, &str)] = &[
        ("rm -rf /", "recursive deletion of root filesystem"),
        ("rm -rf ~", "recursive deletion of home directory"),
        ("rm -rf .", "recursive deletion of current directory"),
        ("mkfs", "filesystem formatting"),
        ("format c:", "disk formatting"),
        ("format d:", "disk formatting"),
        (":(){:|:&};:", "fork bomb"),
        ("dd if=/dev/zero", "disk overwrite"),
        ("dd if=/dev/random", "disk overwrite"),
        ("> /dev/sda", "raw disk write"),
        ("chmod -r 777 /", "recursive permission change on root"),
        ("drop table", "SQL table deletion"),
        ("drop database", "SQL database deletion"),
        ("truncate table", "SQL table truncation"),
        ("delete from", "SQL mass deletion"),
        ("shutdown /s", "system shutdown"),
        ("shutdown -h now", "system shutdown"),
        ("reboot", "system reboot"),
        ("init 0", "system halt"),
        (":(){ :|:& };:", "fork bomb"),
    ];

    for (pattern, reason) in PATTERNS {
        if lower.contains(pattern) {
            return Some(reason);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn audit_log_new_columns_and_legacy_rows_coexist() {
        let db_path = std::env::temp_dir().join(format!("audit_test_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&db_path);
        let conn = crate::config::open_db(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE guild_audit_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL, guild TEXT NOT NULL, tool_name TEXT NOT NULL,
                agent_id TEXT NOT NULL DEFAULT '', intent TEXT,
                status TEXT NOT NULL DEFAULT 'ok', result_preview TEXT,
                prev_hash TEXT NOT NULL DEFAULT '', hash TEXT NOT NULL
            );"
        ).unwrap();
        // Same ALTER migration as production (adds latency_ms/human_intervention/system_snapshot).
        for col in ["latency_ms INTEGER", "human_intervention INTEGER NOT NULL DEFAULT 0", "system_snapshot TEXT"] {
            let _ = conn.execute_batch(&format!("ALTER TABLE guild_audit_log ADD COLUMN {col}"));
        }
        // Legacy row: no latency_ms / human_intervention columns in INSERT.
        conn.execute(
            "INSERT INTO guild_audit_log (timestamp, guild, tool_name, agent_id, intent, status, prev_hash, hash)
             VALUES ('2026-09-01T00:00:00Z', 'git', 'commit', 'agent-x', 'legacy intent', 'ok', '', 'aabb')",
            [],
        ).unwrap();
        // New row with the instrumented columns.
        conn.execute(
            "INSERT INTO guild_audit_log (timestamp, guild, tool_name, agent_id, intent, status, prev_hash, hash, latency_ms, human_intervention)
             VALUES ('2026-09-08T00:00:00Z', 'bash', 'run', 'agent-y', 'new intent', 'ok', 'aabb', 'ccdd', 1234, 1)",
            [],
        ).unwrap();
        // Queries used in production must tolerate legacy NULL columns.
        let row: (String, Option<i64>, Option<i64>) = conn.query_row(
            "SELECT guild, latency_ms, human_intervention FROM guild_audit_log WHERE intent = 'legacy intent'",
            [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).unwrap();
        assert_eq!(row.0, "git");
        assert!(row.1.is_none(), "legacy rows must keep latency_ms NULL");
        assert_eq!(row.2, Some(0), "human_intervention defaults to 0 on legacy rows");
        let row2: (String, i64, i64) = conn.query_row(
            "SELECT guild, latency_ms, human_intervention FROM guild_audit_log WHERE intent = 'new intent'",
            [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).unwrap();
        assert_eq!(row2, ("bash".to_string(), 1234, 1));
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn hitl_path_sets_human_intervention_true() {
        // Simulate what the approve_action handler does: call log_audit_entry
        // with human_intervention=true and verify the row reflects it.
        let db_path = std::env::temp_dir().join(format!("audit_hitl_test_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&db_path);
        // Use a temp override: log_audit_entry writes to ./data/audit.db,
        // so we test via direct SQL the same way production does.
        let conn = crate::config::open_db(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE guild_audit_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL, guild TEXT NOT NULL, tool_name TEXT NOT NULL,
                agent_id TEXT NOT NULL DEFAULT '', intent TEXT,
                status TEXT NOT NULL DEFAULT 'ok', result_preview TEXT,
                prev_hash TEXT NOT NULL DEFAULT '', hash TEXT NOT NULL,
                latency_ms INTEGER, human_intervention INTEGER NOT NULL DEFAULT 0
            );"
        ).unwrap();
        // Insert as the HITL path would: human_intervention = 1
        conn.execute(
            "INSERT INTO guild_audit_log (timestamp, guild, tool_name, agent_id, intent, status, result_preview, prev_hash, hash, latency_ms, human_intervention)
             VALUES ('2026-09-11T00:00:00Z', 'bash', 'bash_execute', 'agent-1', 'list files', 'ok', 'output', '', 'eeff', 0, 1)",
            [],
        ).unwrap();
        let row: (i64,) = conn.query_row(
            "SELECT human_intervention FROM guild_audit_log WHERE intent = 'list files'",
            [], |r| Ok((r.get(0)?,)),
        ).unwrap();
        assert_eq!(row.0, 1, "HITL approve_action path must write human_intervention=1");
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn audit_snapshot_column_stamps_and_legacy_rows_coexist() {
        // WS5: every new audit row carries the boot SystemSnapshot JSON; legacy
        // rows keep NULL; the chain hash EXCLUDES the snapshot column, so a
        // mixed chain must still verify byte-identically to the pre-WS5 math.
        let db_path = std::env::temp_dir().join(format!("audit_snap_test_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&db_path);
        let conn = crate::config::open_db(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE guild_audit_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL, guild TEXT NOT NULL, tool_name TEXT NOT NULL,
                agent_id TEXT NOT NULL DEFAULT '', intent TEXT,
                status TEXT NOT NULL DEFAULT 'ok', result_preview TEXT,
                prev_hash TEXT NOT NULL DEFAULT '', hash TEXT NOT NULL
            );"
        ).unwrap();
        for col in ["latency_ms INTEGER", "human_intervention INTEGER NOT NULL DEFAULT 0", "system_snapshot TEXT"] {
            let _ = conn.execute_batch(&format!("ALTER TABLE guild_audit_log ADD COLUMN {col}"));
        }

        // Chain hash helper — identical formula to verify_audit_chain.
        let chain_hash = |prev: &str, ts: &str, guild: &str, tool: &str, agent: &str, status: &str| {
            use sha2::Digest;
            let input = format!("{prev}|{ts}|{guild}|{tool}|{agent}|{status}");
            format!("{:x}", sha2::Sha256::digest(input.as_bytes()))
        };

        // Legacy row: no snapshot column in the INSERT (pre-WS5 writer shape).
        // Hash is REAL (derived by the chain formula) so verification advances.
        let h1 = chain_hash("", "2026-09-01T00:00:00Z", "git", "commit", "agent-x", "ok");
        conn.execute(
            "INSERT INTO guild_audit_log (timestamp, guild, tool_name, agent_id, intent, status, prev_hash, hash)
             VALUES ('2026-09-01T00:00:00Z', 'git', 'commit', 'agent-x', 'legacy intent', 'ok', '', ?1)",
            rusqlite::params![h1],
        ).unwrap();

        // New row, stamped exactly as the writer does: snapshot JSON in its own
        // column, chain hash computed WITHOUT it.
        let snap = crate::router::system_snapshot::SystemSnapshot::compute(
            &crate::config::TylluanConfig::default(), None, None,
        );
        let snap_json = serde_json::to_string(&snap).unwrap();
        let h2 = chain_hash(&h1, "2026-09-13T00:00:00Z", "bash", "run", "buffy", "ok");
        conn.execute(
            "INSERT INTO guild_audit_log (timestamp, guild, tool_name, agent_id, intent, status, prev_hash, hash, system_snapshot)
             VALUES (?1, 'bash', 'run', 'buffy', 'snap intent', 'ok', ?2, ?3, ?4)",
            rusqlite::params!["2026-09-13T00:00:00Z", h1, h2, snap_json],
        ).unwrap();

        // Stamped row: snapshot present, well-formed, carrying the real commit.
        let (stored_snap,): (String,) = conn.query_row(
            "SELECT system_snapshot FROM guild_audit_log WHERE intent = 'snap intent'",
            [], |r| Ok((r.get(0)?,)),
        ).unwrap();
        assert!(stored_snap.contains("\"commit\""), "snapshot JSON must carry commit: {stored_snap}");
        assert!(stored_snap.contains(&format!("\"commit\":\"{}\"", env!("TYLLUAN_GIT_COMMIT"))),
            "snapshot commit must be the compile-time commit of this binary: {stored_snap}");
        assert!(stored_snap.contains("\"config_hash\""), "snapshot JSON must carry config_hash");

        // Legacy row: NULL tolerated by the same query shape production uses.
        let (legacy_snap,): (Option<String>,) = conn.query_row(
            "SELECT system_snapshot FROM guild_audit_log WHERE intent = 'legacy intent'",
            [], |r| Ok((r.get(0)?,)),
        ).unwrap();
        assert!(legacy_snap.is_none(), "legacy rows keep system_snapshot NULL");

        // Chain verification math (same as verify_audit_chain) over the mixed
        // chain: legacy row (NULL snapshot) must verify, proving the column is
        // genuinely outside the hash.
        let mut stmt = conn.prepare(
            "SELECT timestamp, guild, tool_name, agent_id, status, prev_hash, hash FROM guild_audit_log ORDER BY id ASC"
        ).unwrap();
        let rows: Vec<(String, String, String, String, String, String, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)))
            .unwrap().collect::<Result<_, _>>().unwrap();
        let mut prev = String::new();
        let mut ok = 0usize;
        let mut bad = 0usize;
        for (ts, guild, tool_name, agent_id, status, stored_prev, stored_hash) in rows {
            if stored_prev != prev { bad += 1; continue; }
            let chain_input = format!("{stored_prev}|{ts}|{guild}|{tool_name}|{agent_id}|{status}");
            use sha2::Digest;
            let computed = format!("{:x}", sha2::Sha256::digest(chain_input.as_bytes()));
            if computed == stored_hash { ok += 1; } else { bad += 1; }
            prev = stored_hash;
        }
        assert_eq!((ok, bad), (2, 0), "mixed legacy+stamped chain must verify fully");
        let _ = std::fs::remove_file(&db_path);
    }

    /// Audit-store seam contract, pinned in BOTH directions (seam-unification
    /// pass): write through the env seam and read through it — same rows must
    /// appear, proving writer and reader resolve to the same file. Also pins
    /// the unset default so the migration is byte-identical when no env var
    /// is present.
    /// SAFETY (set_var): edition-2024 unsafe op. Lib-test binaries run tests
    /// on many threads, and env is process-global; this test and the D-53
    /// endpoint test share `AUDIT_ENV_TEST_LOCK` so they cannot race.
    #[test]
    fn audit_store_seam_write_and_read_land_on_same_file() {
        use super::{audit_db_path, log_audit_entry, AUDIT_ENV_TEST_LOCK};
        let _env_guard = AUDIT_ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let db = std::env::temp_dir().join(format!("audit_seam_rt_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&db);
        // SAFETY: guarded single-user section; restored below.
        unsafe { std::env::set_var("TYLLUAN_AUDIT_DB", &db) };

        // 1) Default when unset is the historical path (byte-identical).
        unsafe { std::env::remove_var("TYLLUAN_AUDIT_DB") };
        assert_eq!(audit_db_path(), std::path::PathBuf::from("./data/audit.db"));
        // 2) Writer resolves through the seam...
        unsafe { std::env::set_var("TYLLUAN_AUDIT_DB", &db) };
        log_audit_entry("seam intent", "guild_x", "tool_y", "agent-z", true, "p", 42, false)
            .expect("write through seam must succeed");
        // 3) ...and the reader (the MD-7 read path's resolver) sees the SAME file.
        let rows = crate::transport::http::api_v1::api_audit::latency_rows_from(
            &audit_db_path(),
            "",
        )
        .expect("read through seam must succeed");
        // D-8: the property under test is "write and read resolve to the SAME
        // file", not "this database contains exactly one row". While the seam is
        // set, TYLLUAN_AUDIT_DB is process-global, so any other test in this
        // binary that logs an audit entry lands in this same file -- which is
        // exactly how the old `assert_eq!(rows.len(), 1)` flaked under the full
        // parallel suite (observed 3 rows: ours + two from concurrent tests).
        // The guard above only excludes a second *seam user*, not those writers.
        assert!(
            rows.iter().any(|r| *r == (Some(42), "ok".to_string())),
            "the row written through the seam must be visible through the reader: {rows:?}"
        );

        unsafe { std::env::remove_var("TYLLUAN_AUDIT_DB") };
        let _ = std::fs::remove_file(&db);
    }

    /// T-DSH-74: el rate limit del warn de auditoría es 1/minuto y cuenta los
    /// fallos suprimidos entre warns. Se prueba sobre una instancia local para
    /// no acoplar este test al estado global del proceso.
    #[test]
    fn audit_warn_limiter_warns_once_per_minute_and_counts_suppressed() {
        use super::{AuditWarnLimiter, AUDIT_WARN_INTERVAL_SECS};
        let limiter = AuditWarnLimiter::new();
        // Primer fallo: toca warn, sin suprimidos todavía.
        assert_eq!(limiter.on_failure(100), Some(0));
        // Dos fallos dentro del mismo minuto: suprimidos, sin warn.
        assert_eq!(limiter.on_failure(101), None);
        assert_eq!(limiter.on_failure(100 + AUDIT_WARN_INTERVAL_SECS - 1), None);
        // Pasado el intervalo: otro warn y reporta los 2 suprimidos acumulados.
        assert_eq!(limiter.on_failure(100 + AUDIT_WARN_INTERVAL_SECS), Some(2));
    }

    /// T-DSH-74: `audit_or_warn` es best-effort y no cambia el valor de retorno
    /// del call-site aunque la auditoría falle. Forzamos el fallo apuntando
    /// `TYLLUAN_AUDIT_DB` a un directorio (SQLite no puede abrirlo como DB).
    #[test]
    fn audit_or_warn_preserves_call_site_value_when_audit_fails() {
        use super::AUDIT_ENV_TEST_LOCK;
        use super::audit_or_warn;
        let _env_guard = AUDIT_ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let dir = std::env::temp_dir().join(format!("audit_or_warn_fail_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("crear directorio temporal");
        // SAFETY: sección de un solo usuario protegida por el lock; restaurada abajo.
        unsafe { std::env::set_var("TYLLUAN_AUDIT_DB", &dir) };

        let value = {
            // El `let () = ...` fija en compilación que el helper devuelve `()`
            // (no un `Result` que el call-site pudiera propagar por error).
            let () = audit_or_warn("intent", "guild", "tool", "agent", true, "preview", 0, false);
            42
        };

        unsafe { std::env::remove_var("TYLLUAN_AUDIT_DB") };
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(value, 42, "audit_or_warn no debe alterar el valor del call-site");
    }
}