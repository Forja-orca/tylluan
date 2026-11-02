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

/// T-DSH-169: DOS almacenes con nombres distintos, ambos bajo el DATA_DIR.
/// - `audit.db`: almacen generico de auditoria (audit_log, friction_*, ejemplos).
/// - `guild_audit.db`: SOLO la cadena `guild_audit_log` + `guild_audit_chain_meta`.
///
/// Antes, la cadena vivia en `audit_db_path()` = `./data/audit.db` relativo al
/// CWD: segun desde donde arrancara el kernel acababa en `data/audit.db` o en
/// `crates/tylluan-kernel/data/audit.db`, fuera del backup y mezclada con el
/// almacen generico. Los overrides explicitos (`TYLLUAN_AUDIT_DB`,
/// `TYLLUAN_GUILD_AUDIT_DB`) siguen mandando (seam de tests).
fn audit_store_paths_from(
    data_dir: &std::path::Path,
    audit_env: Option<String>,
    guild_env: Option<String>,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let audit = audit_env
        .clone()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| data_dir.join("audit.db"));
    // Compatibilidad de seam: `TYLLUAN_AUDIT_DB` solo (sin
    // `TYLLUAN_GUILD_AUDIT_DB`) mantiene el comportamiento de un unico fichero
    // para la cadena, que usan los tests historicos. En produccion (sin env)
    // los nombres quedan separados: audit.db vs guild_audit.db.
    let guild = guild_env
        .or(audit_env)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| data_dir.join("guild_audit.db"));
    (audit, guild)
}

/// Ruta del almacen generico de auditoria (`audit.db` bajo el DATA_DIR).
pub fn audit_db_path() -> std::path::PathBuf {
    audit_store_paths_from(
        &crate::config::data_dir(),
        std::env::var("TYLLUAN_AUDIT_DB").ok(),
        std::env::var("TYLLUAN_GUILD_AUDIT_DB").ok(),
    )
    .0
}

/// Ruta de la base que contiene la cadena `guild_audit_log` (`guild_audit.db`).
pub fn guild_audit_db_path() -> std::path::PathBuf {
    audit_store_paths_from(
        &crate::config::data_dir(),
        std::env::var("TYLLUAN_AUDIT_DB").ok(),
        std::env::var("TYLLUAN_GUILD_AUDIT_DB").ok(),
    )
    .1
}

/// Numero de filas de `guild_audit_log` en una base, si la tabla existe.
fn chain_rows_in(path: &std::path::Path) -> Option<i64> {
    let conn = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .ok()?;
    conn.query_row("SELECT COUNT(*) FROM guild_audit_log", [], |r| r.get(0))
        .ok()
}

/// Rutas heredadas donde la cadena pudo quedar antes del arreglo.
fn legacy_chain_candidates() -> Vec<std::path::PathBuf> {
    let mut out = vec![
        audit_db_path(),
        std::path::PathBuf::from("./data/audit.db"),
        std::path::PathBuf::from("./crates/tylluan-kernel/data/audit.db"),
    ];
    if let Ok(explicit) = std::env::var("TYLLUAN_AUDIT_DB") {
        let p = std::path::PathBuf::from(explicit);
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// Copia la cadena heredada a `target` si alli aun no existe. NO borra ni mueve
/// el origen: la copia es la unica operacion destructiva permitida (y solo
/// escribe un fichero nuevo). Devuelve el origen copiado.
fn migrate_chain_from_candidates(
    target: &std::path::Path,
    candidates: &[std::path::PathBuf],
) -> Result<Option<std::path::PathBuf>, String> {
    if target.exists() {
        return Ok(None);
    }
    for cand in candidates {
        if cand == target || !cand.exists() {
            continue;
        }
        let Some(rows) = chain_rows_in(cand) else { continue };
        if rows == 0 {
            continue;
        }
        if let Some(parent) = target.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| format!("guild_audit dir: {e}"))?;
        }
        std::fs::copy(cand, target).map_err(|e| {
            format!("copiando cadena heredada {} -> {}: {e}", cand.display(), target.display())
        })?;
        // Copiar tambien los sidecars WAL/SHM si existen: una base abierta
        // puede tener filas recientes solo en el -wal, y copiar solo el .db
        // las perderia en silencio. Nunca se borra el origen.
        for suffix in ["-wal", "-shm"] {
            let mut src_os = cand.as_os_str().to_os_string();
            src_os.push(suffix);
            let src = std::path::PathBuf::from(src_os);
            if src.exists() {
                let mut dst_os = target.as_os_str().to_os_string();
                dst_os.push(suffix);
                let dst = std::path::PathBuf::from(dst_os);
                std::fs::copy(&src, &dst).map_err(|e| {
                    format!("copiando sidecar {} -> {}: {e}", src.display(), dst.display())
                })?;
            }
        }
        tracing::warn!(
            source = %cand.display(),
            target = %target.display(),
            rows,
            "guild_audit: cadena heredada migrada por COPIA (el original se conserva intacto)"
        );
        return Ok(Some(cand.clone()));
    }
    Ok(None)
}

/// T-DSH-169: migra (por copia) la cadena heredada a `guild_audit.db` del
/// DATA_DIR. Idempotente: si el destino ya existe, no hace nada.
pub fn migrate_legacy_guild_audit_chain() -> Result<Option<std::path::PathBuf>, String> {
    migrate_chain_from_candidates(&guild_audit_db_path(), &legacy_chain_candidates())
}

/// D-84: metadatos de la cadena cuando se purgan filas antiguas. El ancla
/// conserva el último hash borrado para que el tramo vivo siga verificando.
fn ensure_chain_meta(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS guild_audit_chain_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );",
    )
    .map_err(|e| format!("audit meta schema: {e}"))
}

fn chain_anchor(conn: &rusqlite::Connection) -> String {
    conn.query_row(
        "SELECT value FROM guild_audit_chain_meta WHERE key = 'anchor'",
        [],
        |r| r.get(0),
    )
    .unwrap_or_default()
}

fn timestamp_is_older_than(ts: &str, cutoff: chrono::DateTime<chrono::Utc>) -> bool {
    let parsed = chrono::DateTime::parse_from_rfc3339(ts)
        .map(|d| d.with_timezone(&chrono::Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S%.f")
                .map(|d| d.and_utc())
        })
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S")
                .map(|d| d.and_utc())
        });
    parsed.map(|d| d < cutoff).unwrap_or(false)
}

/// D-84: purga solo el prefijo más antiguo de la cadena, en orden de `id`.
/// Antes de borrar guarda el hash del último eliminado como ancla; el
/// siguiente `log_audit_entry` y `verify_audit_chain` arrancan desde ahí.
pub fn prune_guild_audit_log_at(
    path: &std::path::Path,
    retention_days: u64,
) -> Result<usize, String> {
    if retention_days == 0 || !path.exists() {
        return Ok(0);
    }
    let mut conn = crate::config::open_db(path).map_err(|e| format!("audit open: {e}"))?;
    ensure_chain_meta(&conn)?;
    let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days as i64);
    let mut stmt = conn
        .prepare("SELECT id, timestamp, hash FROM guild_audit_log ORDER BY id ASC")
        .map_err(|e| format!("audit retention prepare: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| format!("audit retention query: {e}"))?;
    let mut last_id: Option<i64> = None;
    let mut last_hash = String::new();
    for row in rows {
        let (id, ts, hash) = row.map_err(|e| format!("audit retention row: {e}"))?;
        if !timestamp_is_older_than(&ts, cutoff) {
            break;
        }
        last_id = Some(id);
        last_hash = hash;
    }
    let Some(last_id) = last_id else {
        return Ok(0);
    };
    drop(stmt);
    let tx = conn
        .transaction()
        .map_err(|e| format!("audit retention tx: {e}"))?;
    let deleted = tx
        .execute("DELETE FROM guild_audit_log WHERE id <= ?1", [last_id])
        .map_err(|e| format!("audit retention delete: {e}"))?;
    tx.execute(
        "INSERT INTO guild_audit_chain_meta (key, value) VALUES ('anchor', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [last_hash],
    )
    .map_err(|e| format!("audit retention anchor: {e}"))?;
    tx.commit()
        .map_err(|e| format!("audit retention commit: {e}"))?;
    Ok(deleted)
}

/// D-84: purga de `guild_audit_log` sobre la ruta de auditoría del proceso.
pub fn prune_guild_audit_log(retention_days: u64) -> Result<usize, String> {
    prune_guild_audit_log_at(&guild_audit_db_path(), retention_days)
}

/// Write an audit log entry for every tylluan_do tool call.
/// Uses SHA-256 hash chaining: each entry stores the hash of the previous entry,
/// making tampering detectable. Called fire-and-forget — errors are non-fatal.
#[allow(clippy::too_many_arguments)]
pub(crate) fn log_audit_entry(intent: &str, guild: &str, tool: &str, agent_id: &str, success: bool, preview: &str, latency_ms: u64, human_intervention: bool) -> Result<(), String> {
    // T-DSH-169: la cadena vive en guild_audit.db; migrar por copia si viene de
    // una ubicacion heredada (idempotente y no destructivo).
    if let Err(e) = migrate_legacy_guild_audit_chain() {
        tracing::warn!("guild_audit: migracion heredada fallida (se sigue intentando): {e}");
    }
    log_audit_entry_at(
        &guild_audit_db_path(),
        intent,
        guild,
        tool,
        agent_id,
        success,
        preview,
        latency_ms,
        human_intervention,
    )
}

/// Igual que [`log_audit_entry`] pero sobre una ruta explicita (tests/seam).
#[allow(clippy::too_many_arguments)]
fn log_audit_entry_at(path: &std::path::Path, intent: &str, guild: &str, tool: &str, agent_id: &str, success: bool, preview: &str, latency_ms: u64, human_intervention: bool) -> Result<(), String> {
    // A fresh checkout (CI, first run) has no `./data`: without this the open
    // failed and the audit row was silently lost.
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| format!("audit dir: {e}"))?;
    }
    let mut conn = crate::config::open_db(path).map_err(|e| format!("audit open: {e}"))?;
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

    // D-95: two writers that read "the last hash" and then insert on their own
    // connections both chain to the same predecessor and the second one breaks
    // the link. `BEGIN IMMEDIATE` serializes the read+insert pair, so the chain
    // stays linear without rewriting a single existing row.
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("audit tx: {e}"))?;
    ensure_chain_meta(&tx)?;
    // Get previous hash for chaining. After a retention purge the table can be
    // empty; the anchor preserves the last deleted hash so the live chain still
    // verifies.
    let prev_hash: String = tx
        .query_row("SELECT hash FROM guild_audit_log ORDER BY id DESC LIMIT 1", [], |r| {
            r.get(0)
        })
        .or_else(|_| {
            tx.query_row(
                "SELECT value FROM guild_audit_chain_meta WHERE key = 'anchor'",
                [],
                |r| r.get(0),
            )
        })
        .unwrap_or_default();

    // Chain hash: SHA-256 of (prev_hash || timestamp || guild || tool || agent_id || status)
    let chain_input = format!("{prev_hash}|{now}|{guild}|{tool}|{agent_id}|{status}");
    use sha2::Digest;
    let hash = format!("{:x}", sha2::Sha256::digest(chain_input.as_bytes()));

    let snapshot_json = crate::router::system_snapshot::global()
        .and_then(|s| serde_json::to_string(s).ok());

    tx.execute(
        "INSERT INTO guild_audit_log (timestamp, guild, tool_name, agent_id, intent, status, result_preview, prev_hash, hash, latency_ms, human_intervention, system_snapshot)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        rusqlite::params![now, guild, tool, agent_id, intent, status, preview, prev_hash, hash, latency_ms as i64, if human_intervention { 1 } else { 0 }, snapshot_json],
    ).map_err(|e| format!("audit insert: {e}"))?;
    tx.commit().map_err(|e| format!("audit commit: {e}"))?;
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

/// D-95: verdict of one chain pass. The two failure kinds are kept apart because
/// they mean different things:
/// - `hash_inconsistent`: the stored hash does not match the row's own content
///   (the row was edited);
/// - `link_breaks`: the row's `prev_hash` does not match its predecessor's hash
///   (a hole in the chain, typically two writers chaining to the same
///   predecessor).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AuditChainReport {
    /// Rows that are self-consistent and correctly linked.
    pub ok: usize,
    /// Row ids whose stored hash does not match their content.
    pub hash_inconsistent: Vec<i64>,
    /// Row ids whose `prev_hash` does not match the previous row's hash.
    pub link_breaks: Vec<i64>,
}

impl AuditChainReport {
    pub fn bad(&self) -> usize {
        self.hash_inconsistent.len() + self.link_breaks.len()
    }

    pub fn is_clean(&self) -> bool {
        self.bad() == 0
    }
}

/// Verify the integrity of the audit chain from oldest to newest.
pub fn verify_audit_chain() -> Result<AuditChainReport, String> {
    verify_audit_chain_at(&guild_audit_db_path())
}

/// D-95: the scan **resynchronizes** after every failure. Before, a link break
/// left `prev` pointing at the old hash, so every later row was reported as
/// tampered too (6.200 "tampered" for 25 real breaks on the production DB).
pub(crate) fn verify_audit_chain_at(path: &std::path::Path) -> Result<AuditChainReport, String> {
    let conn = match crate::config::open_db(path) {
        Ok(c) => c,
        Err(_) => return Ok(AuditChainReport::default()),
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
    ensure_chain_meta(&conn)?;
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

    let mut prev = chain_anchor(&conn);
    let mut report = AuditChainReport::default();
    for row_res in rows {
        let row = row_res.map_err(|e| format!("audit row: {e}"))?;
        let (id, ts, guild, tool_name, agent_id, status, stored_prev, stored_hash) = row;
        let chain_input = format!("{stored_prev}|{ts}|{guild}|{tool_name}|{agent_id}|{status}");
        use sha2::Digest;
        let computed = format!("{:x}", sha2::Sha256::digest(chain_input.as_bytes()));

        if computed != stored_hash {
            report.hash_inconsistent.push(id);
        } else if stored_prev != prev {
            report.link_breaks.push(id);
        } else {
            report.ok += 1;
        }

        // Resynchronize: the chain continues from what is actually stored, so a
        // single bad row is reported once instead of poisoning the rest.
        prev = stored_hash;
    }
    Ok(report)
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
    use super::verify_audit_chain_at;

    #[test]
    fn guild_audit_retention_prunes_old_prefix_and_preserves_anchor() {
        use super::prune_guild_audit_log_at;

        let db = std::env::temp_dir().join(format!(
            "audit_retention_test_{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&db);
        let conn = crate::config::open_db(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE guild_audit_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL, guild TEXT NOT NULL, tool_name TEXT NOT NULL,
                agent_id TEXT NOT NULL DEFAULT '', intent TEXT,
                status TEXT NOT NULL DEFAULT 'ok', result_preview TEXT,
                prev_hash TEXT NOT NULL DEFAULT '', hash TEXT NOT NULL,
                latency_ms INTEGER, human_intervention INTEGER NOT NULL DEFAULT 0,
                system_snapshot TEXT
            );",
        )
        .unwrap();

        let chain_hash = |prev: &str, ts: &str, guild: &str, tool: &str, agent: &str, status: &str| {
            use sha2::Digest;
            let input = format!("{prev}|{ts}|{guild}|{tool}|{agent}|{status}");
            format!("{:x}", sha2::Sha256::digest(input.as_bytes()))
        };
        let old_ts = "2020-01-01T00:00:00Z";
        let new_ts = "2030-01-01T00:00:00Z";
        let h1 = chain_hash("", old_ts, "g1", "t1", "a1", "ok");
        let h2 = chain_hash(&h1, old_ts, "g2", "t2", "a2", "ok");
        let h3 = chain_hash(&h2, new_ts, "g3", "t3", "a3", "ok");
        for (ts, guild, tool, agent, prev, hash) in [
            (old_ts, "g1", "t1", "a1", "", h1.as_str()),
            (old_ts, "g2", "t2", "a2", h1.as_str(), h2.as_str()),
            (new_ts, "g3", "t3", "a3", h2.as_str(), h3.as_str()),
        ] {
            conn.execute(
                "INSERT INTO guild_audit_log
                 (timestamp, guild, tool_name, agent_id, intent, status, result_preview, prev_hash, hash)
                 VALUES (?1, ?2, ?3, ?4, 'retention', 'ok', '', ?5, ?6)",
                rusqlite::params![ts, guild, tool, agent, prev, hash],
            )
            .unwrap();
        }
        drop(conn);

        assert_eq!(prune_guild_audit_log_at(&db, 90).unwrap(), 2);
        let conn = crate::config::open_db(&db).unwrap();
        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM guild_audit_log", [], |r| r.get(0))
            .unwrap();
        let anchor: String = conn
            .query_row(
                "SELECT value FROM guild_audit_chain_meta WHERE key = 'anchor'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(remaining, 1);
        assert_eq!(anchor, h2);
        drop(conn);
        let report = verify_audit_chain_at(&db).unwrap();
        assert_eq!(report.ok, 1);
        assert!(report.is_clean(), "pruning must not break the live chain");
        let _ = std::fs::remove_file(&db);
    }

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
        assert_eq!(audit_db_path(), crate::config::data_file("audit.db"));
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

    #[test]
    fn audit_stores_are_named_and_live_under_data_dir() {
        // T-DSH-169: antes la cadena resolvia "./data/audit.db" relativo al CWD
        // (dos ubicaciones distintas segun desde donde arrancara el kernel).
        // Ahora ambas cuelgan del DATA_DIR y con nombres separados.
        let dir = std::path::PathBuf::from("/tmp/t169-data");
        let (audit, guild) = super::audit_store_paths_from(&dir, None, None);
        assert_eq!(audit, dir.join("audit.db"));
        assert_eq!(guild, dir.join("guild_audit.db"));
        assert_ne!(audit, guild, "almacen generico y cadena no comparten fichero");

        let (audit2, guild2) = super::audit_store_paths_from(
            &dir,
            Some("/x/audit-custom.db".to_string()),
            Some("/y/guild-custom.db".to_string()),
        );
        assert_eq!(audit2, std::path::PathBuf::from("/x/audit-custom.db"));
        assert_eq!(guild2, std::path::PathBuf::from("/y/guild-custom.db"));

        // Seam historico: solo TYLLUAN_AUDIT_DB => un unico fichero para todo.
        let (audit3, guild3) = super::audit_store_paths_from(
            &dir,
            Some("/x/audit-custom.db".to_string()),
            None,
        );
        assert_eq!(audit3, std::path::PathBuf::from("/x/audit-custom.db"));
        assert_eq!(guild3, audit3, "compatibilidad con el seam anterior");
    }

    #[test]
    fn legacy_chain_is_copied_without_deleting_source() {
        // T-DSH-169: migracion por COPIA. El origen se conserva intacto.
        let tmp = tempfile::tempdir().unwrap();
        let legacy = tmp.path().join("legacy_audit.db");
        {
            let conn = crate::config::open_db(&legacy).unwrap();
            conn.execute_batch(
                "CREATE TABLE guild_audit_log (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp TEXT NOT NULL, guild TEXT NOT NULL, tool_name TEXT NOT NULL, agent_id TEXT NOT NULL DEFAULT '', intent TEXT, status TEXT NOT NULL DEFAULT 'ok', result_preview TEXT, prev_hash TEXT NOT NULL DEFAULT '', hash TEXT NOT NULL);",
            ).unwrap();
            conn.execute(
                "INSERT INTO guild_audit_log (timestamp, guild, tool_name, agent_id, status, prev_hash, hash) VALUES ('t','kernel','doctor','a','ok','', 'h1')",
                [],
            ).unwrap();
        }
        let target = tmp.path().join("guild_audit.db");
        let copied = super::migrate_chain_from_candidates(&target, std::slice::from_ref(&legacy)).unwrap();
        assert_eq!(copied.as_deref(), Some(legacy.as_path()), "debe copiar la heredada");
        assert_eq!(super::chain_rows_in(&target), Some(1), "la cadena llega al destino");
        assert!(legacy.exists(), "el origen NO se borra");
        assert_eq!(super::chain_rows_in(&legacy), Some(1), "el origen conserva sus filas");

        // Idempotente: con destino ya creado no vuelve a copiar.
        let again = super::migrate_chain_from_candidates(&target, std::slice::from_ref(&legacy)).unwrap();
        assert_eq!(again, None);
    }

    #[test]
    fn guild_entries_do_not_land_in_the_generic_audit_db() {
        // T-DSH-169: la cadena se escribe en guild_audit.db, no en audit.db.
        let tmp = tempfile::tempdir().unwrap();
        let guild_db = tmp.path().join("guild_audit.db");
        let generic_db = tmp.path().join("audit.db");
        super::log_audit_entry_at(&guild_db, "intent", "kernel", "doctor", "test", true, "ok", 5, false)
            .expect("escribir guild_audit.db");
        assert!(guild_db.exists());
        assert_eq!(super::chain_rows_in(&guild_db), Some(1));
        assert!(!generic_db.exists(), "el almacen generico no recibe la cadena");
    }

    /// D-95: build a synthetic chain of `n` rows directly in SQLite. Each row
    /// chains to the previous row's stored hash, except the ones the caller
    /// overrides (that is exactly what a second concurrent writer produces: a
    /// self-consistent row whose `prev_hash` is not its predecessor).
    fn build_chain_with(db: &std::path::Path, n: usize, overrides: &[(usize, &str)]) -> Vec<i64> {
        use sha2::Digest;
        let conn = crate::config::open_db(db).expect("open");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS guild_audit_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL, guild TEXT NOT NULL, tool_name TEXT NOT NULL,
                agent_id TEXT NOT NULL DEFAULT '', intent TEXT,
                status TEXT NOT NULL DEFAULT 'ok', result_preview TEXT,
                prev_hash TEXT NOT NULL DEFAULT '', hash TEXT NOT NULL
            );",
        )
        .expect("schema");
        let mut prev = String::new();
        let mut ids = Vec::new();
        for i in 0..n {
            let ts = format!("2026-11-0{}T00:00:0{}Z", (i % 9) + 1, i % 10);
            let guild = "coloquio";
            let tool = "read_channel";
            let agent = "deep";
            let status = "ok";
            let used_prev = overrides
                .iter()
                .find(|(idx, _)| *idx == i)
                .map(|(_, p)| (*p).to_string())
                .unwrap_or_else(|| prev.clone());
            let input = format!("{used_prev}|{ts}|{guild}|{tool}|{agent}|{status}");
            let hash = format!("{:x}", sha2::Sha256::digest(input.as_bytes()));
            conn.execute(
                "INSERT INTO guild_audit_log (timestamp, guild, tool_name, agent_id, status, prev_hash, hash)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![ts, guild, tool, agent, status, used_prev, hash],
            )
            .expect("insert");
            ids.push(conn.last_insert_rowid());
            prev = hash;
        }
        ids
    }

    fn build_chain(db: &std::path::Path, n: usize) -> Vec<i64> {
        build_chain_with(db, n, &[])
    }

    /// D-95: two link breaks do not poison the rows that come after them. The
    /// old scan kept `prev` at the pre-break hash, so every later row was also
    /// reported as tampered (6.200 of them on the production DB for 25 breaks).
    #[test]
    fn d95_two_link_breaks_are_reported_without_cascading() {
        let db = std::env::temp_dir().join(format!("audit_d95_breaks_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&db);
        // Rows 2 and 4 chain to the anchor instead of their predecessor, the way a
        // second writer that read the same "last hash" would.
        let ids = build_chain_with(&db, 5, &[(1, ""), (3, "")]);

        let report = verify_audit_chain_at(&db).expect("verify");
        assert_eq!(report.link_breaks, vec![ids[1], ids[3]]);
        assert!(report.hash_inconsistent.is_empty());
        assert_eq!(report.ok, 3, "rows 1, 3 and 5 are intact");
        assert!(!report.is_clean());
        let _ = std::fs::remove_file(&db);
    }

    /// D-95: an edited row is reported as `hash_inconsistent`, not as a link
    /// break, and the rows after it stay valid (the scan resynchronizes on the
    /// stored hash).
    #[test]
    fn d95_edited_row_is_reported_apart_and_does_not_cascade() {
        let db = std::env::temp_dir().join(format!("audit_d95_edit_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&db);
        let ids = build_chain(&db, 4);
        let conn = crate::config::open_db(&db).expect("open");
        conn.execute(
            "UPDATE guild_audit_log SET status = 'error' WHERE id = ?1",
            rusqlite::params![ids[1]],
        )
        .expect("tamper");

        let report = verify_audit_chain_at(&db).expect("verify");
        assert_eq!(report.hash_inconsistent, vec![ids[1]]);
        assert!(report.link_breaks.is_empty(), "an edited row is not a broken link");
        assert_eq!(report.ok, 3, "the rows after the edited one stay valid");
        let _ = std::fs::remove_file(&db);
    }

    /// D-95: a clean chain stays clean (no false positives from the rewrite).
    #[test]
    fn d95_clean_synthetic_chain_has_no_findings() {
        let db = std::env::temp_dir().join(format!("audit_d95_clean_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&db);
        build_chain(&db, 6);
        let report = verify_audit_chain_at(&db).expect("verify");
        assert!(report.is_clean(), "{report:?}");
        assert_eq!(report.ok, 6);
        let _ = std::fs::remove_file(&db);
    }

    /// D-95: 8 writers × 25 rows. With `BEGIN IMMEDIATE` the read+insert pair
    /// serializes, so every row lands and the chain stays linear; the connection
    /// waits on the 5 s busy_timeout instead of dropping audit rows.
    #[test]
    fn d95_many_concurrent_writers_keep_the_chain_linear() {
        use super::log_audit_entry_at;
        let db = std::env::temp_dir().join(format!("audit_d95_stress_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&db);

        let mut handles = Vec::new();
        for w in 0..8 {
            let db = db.clone();
            handles.push(std::thread::spawn(move || {
                for i in 0..25 {
                    let _ = log_audit_entry_at(
                        &db,
                        &format!("stress-{w}-{i}"),
                        "g",
                        "t",
                        &format!("agent-{w}"),
                        true,
                        "",
                        0,
                        false,
                    );
                }
            }));
        }
        for h in handles {
            h.join().expect("writer thread");
        }

        let report = verify_audit_chain_at(&db).expect("verify");
        assert_eq!(
            report.ok, 200,
            "every row must land and chain: {:?} link breaks, {:?} edited",
            report.link_breaks, report.hash_inconsistent
        );
        assert!(report.is_clean(), "{report:?}");
        let _ = std::fs::remove_file(&db);
    }

    /// D-95: the writer serializes "read last hash + insert", so concurrent
    /// writers cannot both chain to the same predecessor.
    #[test]
    fn d95_concurrent_writes_keep_the_chain_linear() {
        use super::log_audit_entry_at;
        let db = std::env::temp_dir().join(format!("audit_d95_conc_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&db);

        let db_a = db.clone();
        let a = std::thread::spawn(move || {
            for i in 0..10 {
                let _ = log_audit_entry_at(&db_a, &format!("intent-a-{i}"), "g", "t", "a", true, "", 0, false);
            }
        });
        let db_b = db.clone();
        let b = std::thread::spawn(move || {
            for i in 0..10 {
                let _ = log_audit_entry_at(&db_b, &format!("intent-b-{i}"), "g", "t", "b", true, "", 0, false);
            }
        });
        a.join().expect("thread a");
        b.join().expect("thread b");

        let report = verify_audit_chain_at(&db).expect("verify");
        assert!(
            report.link_breaks.is_empty(),
            "two writers must not break the chain: {:?}",
            report.link_breaks
        );
        assert_eq!(report.ok, 20);
        let _ = std::fs::remove_file(&db);
    }

}
