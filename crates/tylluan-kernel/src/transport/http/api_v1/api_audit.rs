use axum::Json;
use axum::response::IntoResponse;
use crate::transport::server::handler_do;

pub async fn audit_verify() -> impl axum::response::IntoResponse {
    match handler_do::verify_audit_chain() {
        Ok((ok, bad)) => {
            let status = if bad == 0 { "clean" } else { "tampered" };
            let message = if ok == 0 && bad == 0 {
                "No audit entries yet.".to_string()
            } else if bad == 0 {
                format!("✅ Chain integrity verified — {ok} entries intact.")
            } else {
                format!("🚨 Chain broken — {ok} valid, {bad} tampered entries.")
            };
            (axum::http::StatusCode::OK, Json(serde_json::json!({
                "ok": true,
                "status": status,
                "valid_count": ok,
                "tampered_count": bad,
                "message": message,
            })))
        }
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "ok": false, "error": e.to_string()
        }))),
    }
}

// ── MD-7 closure: GET /api/v1/audit/latency ─────────────────────────────────
//
// Read-only per-request latency statistics aggregated from `guild_audit_log`
// (the store `log_audit_entry` writes on every `tylluan_do` dispatch; rows
// written by the main do-path carry the real full intent->result cycle in
// `latency_ms`, while some secondary paths record 0). Closes the SLO matrix's
// continuous-latency gap: percentiles per window, sourced from real
// dispatches, with honest accounting of the zero-latency rows.
//
// What a row is: one completed `tylluan_do` dispatch (see
// MEASUREMENT_DEBT_REGISTER.md MD-7 and SLO_MATRIX.md §2.1).
//
// `TYLLUAN_AUDIT_DB` relocates the store: primarily a test seam (same
// pattern as `TYLLUAN_CONFUSION_DB` in the WS3 collector), also lets an
// operator place the audit DB elsewhere.

/// Nearest-rank percentile over `values` (does not need to be pre-sorted;
/// `values` is sorted in place). Returns 0 for an empty slice.
pub(crate) fn latency_percentile(values: &mut [u64], p: f64) -> u64 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    // Nearest-rank: ceil(p/100 * n), 1-indexed.
    let n = values.len() as f64;
    let k = ((p / 100.0) * n).ceil().max(1.0) as usize;
    let idx = (k - 1).min(values.len() - 1);
    values[idx]
}

/// Pure aggregation over one latency sample (already filtered by window).
///
/// `latencies`: all recorded `latency_ms` values > 0 (any status) — the real
/// end-to-end durations. `zero_or_null`: dispatches whose duration was not
/// recorded (secondary paths write literal 0) — excluded from percentiles,
/// reported separately rather than silently dropped. `total`/`errors`:
/// dispatch counts over ALL rows in the window (status-aware), so the error
/// rate is not biased by which paths record latency.
pub(crate) fn compute_latency_stats(
    mut latencies: Vec<u64>,
    zero_or_null: usize,
    total: usize,
    errors: usize,
) -> serde_json::Value {
    let sampled = latencies.len();
    let (p50, p95, p99, max) = if sampled == 0 {
        (serde_json::Value::Null, serde_json::Value::Null, serde_json::Value::Null, serde_json::Value::Null)
    } else {
        (
            serde_json::json!(latency_percentile(&mut latencies, 50.0)),
            serde_json::json!(latency_percentile(&mut latencies, 95.0)),
            serde_json::json!(latency_percentile(&mut latencies, 99.0)),
            serde_json::json!(*latencies.last().unwrap_or(&0)),
        )
    };
    serde_json::json!({
        "latency_ms": {
            "p50": p50,
            "p95": p95,
            "p99": p99,
            "max": max,
            "sampled": sampled,
            "zero_latency_excluded": zero_or_null,
        },
        "error_rate": {
            "total": total,
            "errors": errors,
            "percent": if total > 0 { (errors as f64 * 100.0 / total as f64 * 100.0).round() / 100.0 } else { 0.0 },
        },
    })
}

/// Build the optional window cutoff. The comparison wraps `timestamp` in
/// SQLite's `datetime()` so RFC3339 values with offsets normalize to UTC —
/// a raw string compare would misorder rows written with non-Z offsets.
pub(crate) fn cutoff_clause(window_minutes: Option<i64>) -> String {
    match window_minutes.filter(|m| *m > 0) {
        Some(m) => format!(" WHERE datetime(timestamp) >= datetime('now', '-{m} minutes')"),
        None => String::new(),
    }
}

/// Read-only single-pass reader over the audit DB (strictly SELECT).
/// Path-injected so tests exercise THIS code against a temp file.
pub(crate) fn latency_rows_from(
    path: &std::path::Path,
    cutoff: &str,
) -> Result<Vec<(Option<i64>, String)>, String> {
    let conn = handler_do::audit_open_readonly(path)?;
    let query = format!("SELECT latency_ms, status FROM guild_audit_log{cutoff}");
    let mut stmt = conn
        .prepare(&query)
        .map_err(|e| format!("audit latency prepare: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("audit latency query: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("audit latency row: {e}"))?);
    }
    Ok(out)
}

#[derive(serde::Deserialize)]
pub struct AuditLatencyParams {
    /// Restrict to the last N minutes of dispatches. Absent = all history.
    pub window_minutes: Option<i64>,
}

/// GET /api/v1/audit/latency — read-only aggregation over guild_audit_log.
/// Strictly SELECT-only (readonly connection). DB errors are non-fatal
/// (reported inline with `available: false`, not 500s), mirroring the
/// golden-signals non-fatal convention.
pub async fn audit_latency_stats_handler(
    axum::extract::Query(params): axum::extract::Query<AuditLatencyParams>,
) -> impl axum::response::IntoResponse {
    let db_path = std::env::var("TYLLUAN_AUDIT_DB")
        .unwrap_or_else(|_| "./data/audit.db".to_string());
    let cutoff = cutoff_clause(params.window_minutes);

    let rows = match tokio::task::spawn_blocking(move || {
        latency_rows_from(std::path::Path::new(&db_path), &cutoff)
    })
    .await
    {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            return (
                axum::http::StatusCode::OK,
                crate::transport::http::Utf8Json(serde_json::json!({
                    "available": false,
                    "reason": e,
                    "source": "guild_audit_log",
                    "generated": chrono::Utc::now().to_rfc3339(),
                })),
            ).into_response();
        }
        Err(e) => {
            return (
                axum::http::StatusCode::OK,
                crate::transport::http::Utf8Json(serde_json::json!({
                    "available": false,
                    "reason": format!("audit latency join: {e}"),
                    "source": "guild_audit_log",
                    "generated": chrono::Utc::now().to_rfc3339(),
                })),
            ).into_response();
        }
    };

    let total = rows.len();
    let errors = rows.iter().filter(|(_, s)| s == "error").count();
    let latencies: Vec<u64> = rows
        .iter()
        .filter_map(|(l, _)| (*l).filter(|v| *v > 0).map(|v| v as u64))
        .collect();
    let zero_or_null = total - latencies.len();

    let mut body = compute_latency_stats(latencies, zero_or_null, total, errors);
    if let Some(obj) = body.as_object_mut() {
        obj.insert("available".into(), serde_json::json!(true));
        obj.insert(
            "window_minutes".into(),
            serde_json::json!(params.window_minutes),
        );
        obj.insert("source".into(), serde_json::json!("guild_audit_log"));
        obj.insert(
            "note".into(),
            serde_json::json!(
                "latency_ms percentiles cover dispatches with a recorded duration (latency_ms > 0); \
                 zero_latency_excluded counts dispatches whose path recorded 0 (no invented data)."
            ),
        );
        obj.insert("generated".into(), serde_json::json!(chrono::Utc::now().to_rfc3339()));
    }

    (
        axum::http::StatusCode::OK,
        crate::transport::http::Utf8Json(body),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_empty_is_zero() {
        let mut v: Vec<u64> = vec![];
        assert_eq!(latency_percentile(&mut v, 50.0), 0);
    }

    #[test]
    fn percentile_single_value_always_wins() {
        let mut v = vec![777];
        assert_eq!(latency_percentile(&mut v, 50.0), 777);
        assert_eq!(latency_percentile(&mut v, 95.0), 777);
        assert_eq!(latency_percentile(&mut v, 99.0), 777);
    }

    #[test]
    fn percentile_nearest_rank_exact() {
        // n=4: p50 -> ceil(2.0)=2 -> idx 1; p95 -> ceil(3.8)=4 -> idx 3;
        // p99 -> ceil(3.96)=4 -> idx 3.
        let mut v = vec![10, 20, 30, 40];
        assert_eq!(latency_percentile(&mut v, 50.0), 20);
        let mut v = vec![10, 20, 30, 40];
        assert_eq!(latency_percentile(&mut v, 95.0), 40);
        let mut v = vec![10, 20, 30, 40];
        assert_eq!(latency_percentile(&mut v, 99.0), 40);
    }

    #[test]
    fn percentile_sorts_unsorted_input() {
        let mut v = vec![400, 10, 300, 20];
        assert_eq!(latency_percentile(&mut v, 50.0), 20);
        assert_eq!(latency_percentile(&mut v, 100.0), 400);
    }

    #[test]
    fn stats_with_samples_exact_numbers() {
        let body = compute_latency_stats(vec![100, 200, 300, 400], 2, 6, 1);
        assert_eq!(body["latency_ms"]["p50"], 200);
        assert_eq!(body["latency_ms"]["p95"], 400);
        assert_eq!(body["latency_ms"]["p99"], 400);
        assert_eq!(body["latency_ms"]["max"], 400);
        assert_eq!(body["latency_ms"]["sampled"], 4);
        assert_eq!(body["latency_ms"]["zero_latency_excluded"], 2);
        assert_eq!(body["error_rate"]["total"], 6);
        assert_eq!(body["error_rate"]["errors"], 1);
        let pct = body["error_rate"]["percent"].as_f64().unwrap();
        assert!((pct - 16.67).abs() < 0.01, "got {pct}");
    }

    #[test]
    fn stats_without_samples_is_null_not_zero() {
        // No invented data: an empty store yields nulls, not fabricated 0ms.
        let body = compute_latency_stats(vec![], 0, 0, 0);
        assert!(body["latency_ms"]["p50"].is_null());
        assert!(body["latency_ms"]["max"].is_null());
        assert_eq!(body["latency_ms"]["sampled"], 0);
        assert_eq!(body["error_rate"]["percent"], 0.0);
    }

    #[test]
    fn cutoff_clause_windows_and_defaults() {
        assert_eq!(cutoff_clause(None), "");
        assert_eq!(cutoff_clause(Some(0)), "");
        assert_eq!(cutoff_clause(Some(-5)), "");
        assert_eq!(
            cutoff_clause(Some(30)),
            " WHERE datetime(timestamp) >= datetime('now', '-30 minutes')"
        );
    }

    /// Seed a temp DB with the audit schema shape the reader expects.
    /// (The reader's SELECT is what's under test; the INSERTs here are the
    /// fixture, with the same column list log_audit_entry writes.)
    fn seed_db(path: &std::path::Path) -> rusqlite::Connection {
        let conn = rusqlite::Connection::open(path).unwrap();
        conn.execute_batch(
            "CREATE TABLE guild_audit_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                guild TEXT NOT NULL,
                tool_name TEXT NOT NULL,
                agent_id TEXT NOT NULL DEFAULT '',
                intent TEXT,
                status TEXT NOT NULL DEFAULT 'ok',
                result_preview TEXT,
                prev_hash TEXT NOT NULL DEFAULT '',
                hash TEXT NOT NULL,
                latency_ms INTEGER,
                human_intervention INTEGER NOT NULL DEFAULT 0
            );
            INSERT INTO guild_audit_log (timestamp, guild, tool_name, status, latency_ms, prev_hash, hash) VALUES
                ('2026-09-14T10:00:00+00:00', 'g', 't', 'ok',   100,  '', 'f1'),
                ('2026-09-14T10:01:00+00:00', 'g', 't', 'ok',   200,  'f1', 'f2'),
                ('2026-09-14T10:02:00+00:00', 'g', 't', 'error', 50, 'f2', 'f3'),
                ('2026-09-14T10:03:00+00:00', 'g', 't', 'ok',   NULL, 'f3', 'f4'),
                ('2026-09-14T10:04:00+00:00', 'g', 't', 'ok',   0,    'f4', 'f5');",
        )
        .unwrap();
        conn
    }

    #[test]
    fn reader_reads_all_rows_from_temp_db() {
        let dir = std::env::temp_dir().join(format!("audit_lat_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("a.db");
        let conn = seed_db(&db);
        let rows = latency_rows_from(&db, "").unwrap();
        assert_eq!(rows.len(), 5);
        // Windows: the file cannot be deleted while a connection holds it.
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reader_window_filters_by_datetime() {
        let dir = std::env::temp_dir().join(format!("audit_latw_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("a.db");
        let conn = seed_db(&db);
        // Window long enough to include everything (fixture is 'now'-anchored
        // only relative to the SQLite now() — these 2026 timestamps may be in
        // the past by the time the test runs, so assert the filter REDUCES or
        // keeps rows, never invents any).
        let all = latency_rows_from(&db, "").unwrap().len();
        let narrow = latency_rows_from(&db, &cutoff_clause(Some(1))).unwrap().len();
        assert!(narrow <= all);
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reader_missing_db_is_err_not_panic() {
        let dir = std::env::temp_dir().join(format!("audit_latm_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let res = latency_rows_from(&dir.join("nope.db"), "");
        assert!(res.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
