/// Deterministic failure node ID for the routing feedback loop.
pub(crate) fn routing_failure_id(intent: &str) -> String {
    let hash: u64 = intent.bytes()
        .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
    format!("lesson:routing_failure:{hash:x}")
}

/// Write an audit log entry to ./data/audit.db for every tylluan_do tool call.
/// Uses SHA-256 hash chaining: each entry stores the hash of the previous entry,
/// making tampering detectable. Called fire-and-forget — errors are non-fatal.
#[allow(clippy::too_many_arguments)]
pub(crate) fn log_audit_entry(intent: &str, guild: &str, tool: &str, agent_id: &str, success: bool, preview: &str, latency_ms: u64, human_intervention: bool) -> Result<(), String> {
    let db_path = std::path::Path::new("./data/audit.db");
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("audit mkdir: {e}"))?;
    }
    let conn = crate::config::open_db(db_path).map_err(|e| format!("audit open: {e}"))?;
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
    for col in ["latency_ms INTEGER", "human_intervention INTEGER NOT NULL DEFAULT 0"] {
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

    conn.execute(
        "INSERT INTO guild_audit_log (timestamp, guild, tool_name, agent_id, intent, status, result_preview, prev_hash, hash, latency_ms, human_intervention)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        rusqlite::params![now, guild, tool, agent_id, intent, status, preview, prev_hash, hash, latency_ms as i64, if human_intervention { 1 } else { 0 }],
    ).map_err(|e| format!("audit insert: {e}"))?;
    Ok(())
}

/// Verify the integrity of the audit chain from oldest to newest.
/// Returns (ok_count, bad_count) — bad > 0 means tampering detected.
pub fn verify_audit_chain() -> Result<(usize, usize), String> {
    let db_path = std::path::Path::new("./data/audit.db");
    let conn = match crate::config::open_db(db_path) {
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
        // Same ALTER migration as production (adds latency_ms/human_intervention).
        for col in ["latency_ms INTEGER", "human_intervention INTEGER NOT NULL DEFAULT 0"] {
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
}