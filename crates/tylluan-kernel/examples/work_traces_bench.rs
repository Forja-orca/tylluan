//! ADR-015 Fase 3 — SQL latency benchmark for the stigmergic `work_traces`
//! fabric (`work_traces_heat_exact` / `work_traces_heat_batch`, T901 scope).
//!
//! The T753/T10 battery already pins the MATH and the handler CONTRACT (unit +
//! integration tests); this harness measures the other half of ADR-015 Fase 3:
//! the SQL latency of the heat path at production scale and at its documented
//! stress point. Read-only against a throwaway in-memory SilvaDB — it never
//! touches `data/silva.db` and is NOT wired into CI or tests.
//!
//! What it measures (each reported as median of 7 runs, time injected via
//! `now_unix` — deterministic, no wall clock):
//!   [1] `work_traces_heat_exact` per call at realistic fleet scale
//!       (10 zones × 24h × 6 touches/h ≈ 1,440 rows, ~97 in window each).
//!   [2] `work_traces_heat_batch` over the 10 measured zones (dashboard
//!       heatmap shape: 12 default zones live here).
//!   [3] Heat-path SQL under load: one zone at the documented per-zone
//!       steady-state ceiling (~10k rows inside the 16h window) + the cap
//!       observation: at ceiling density the raw exponential sum far exceeds
//!       the documented cap of 2.0 — the signal saturates.
//!   [4] Scan cost at the stress ceiling: rows scanned per heat query vs the
//!       total table (proves `idx_work_traces_target` bounds the scan).
//!
//! Run: cargo run --release -p tylluan-kernel --example work_traces_bench
//! Output: human-readable table + a JSON block at the end.

use std::collections::HashMap;
use std::hint::black_box;
use std::time::Instant;

use rusqlite::params;
use tylluan_kernel::memory::silva::SilvaDB;

const HALF_LIFE_SECS: i64 = 14400; // T½ = 4h (decay.rs)
const WINDOW_SECS: i64 = 4 * HALF_LIFE_SECS; // 16h SQL window (decay.rs)
const HEAT_CAP: f64 = 2.0; // documented cap (decay.rs)

// ─── seeders (deterministic; same shapes as the T753 unit tests) ────────────

/// Insert one work trace. Same table shape as `seed_work_trace` in
/// `silva/tests.rs`, plus explicit agent and kind.
async fn seed(db: &SilvaDB, uri: &str, agent: &str, kind: &str, ty: &str, w: f64, at: i64) {
    let conn_arc = db.conn_lock();
    let conn = conn_arc.lock().await;
    conn.execute(
        "INSERT INTO work_traces (target_uri, target_kind, agent_id, trace_type, weight, touched_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![uri, kind, agent, ty, w, at],
    )
    .expect("seed work trace");
}

/// [1]/[2] scale: realistic fleet traffic — `n_zones` zones, 24h of history,
/// 6 touches/hour per zone across the flota (staggered agents + kinds).
/// With a 24h horizon and a 16h window, the last ~97 touches per zone are
/// live and the older ~47 are evanesced — exercising the SQL filter too.
async fn seed_fleet(db: &SilvaDB, n_zones: usize) {
    let now = 1_800_000_000i64;
    let agents = ["buffy", "deep", "claude-code", "antigravity", "jose"];
    for z in 0..n_zones {
        let uri = format!("zone:bench-{z:02}");
        for k in 0..144 {
            let at = now - (k as i64) * 600; // one touch every 10 minutes
            let agent = agents[(z + k) % agents.len()];
            // Every 4th trace is diffuse heat (w forced to 0.3 by decay.rs;
            // the weight column value is deliberately irrelevant there).
            let (ty, w) = if k % 4 == 3 { ("diffuse", 1.0) } else { ("direct", 1.0) };
            seed(db, &uri, agent, "zone", ty, w, at).await;
        }
    }
}

/// [3]/[4] stress: the documented per-zone ceiling — enough rows for one zone
/// that the 16h SQL window lets ~10k of them through to the Rust exp-sum.
async fn seed_stress(db: &SilvaDB, inside_window: usize, outside_window: usize) {
    let now = 1_800_000_000i64;
    // Inside: densest legitimate burst. Spread over the window, tiny jitter
    // (±3s) so the compound index walk is not perfectly sequential.
    let step = (WINDOW_SECS as usize / inside_window.max(1)).max(1);
    for k in 0..inside_window {
        let at = now - 1 - (k as i64) * step as i64 - (k as i64) % 3;
        let ty = if k % 4 == 3 { "diffuse" } else { "direct" };
        seed(db, "zone:stress", "flota", "zone", ty, 1.0, at).await;
    }
    // Outside: old evanesced traces (pre-window) — must be skipped by the SQL
    // filter and never touch the Rust sum.
    for k in 0..outside_window {
        let at = now - WINDOW_SECS - 3600 - (k as i64) * 2;
        seed(db, "zone:stress", "flota", "zone", "direct", 1.0, at).await;
    }
}

/// Uncapped exponential sum over one zone's in-window rows (the same rows
/// `work_traces_heat_exact` would read) — to report what the cap clamps away.
async fn raw_sum(db: &SilvaDB, uri: &str, now: i64) -> f64 {
    let conn_arc = db.conn_lock();
    let conn = conn_arc.lock().await;
    let mut stmt = conn
        .prepare(
            "SELECT trace_type, weight, touched_at FROM work_traces
             WHERE target_uri = ?1 AND touched_at >= ?2",
        )
        .unwrap();
    let mut sum = 0.0f64;
    let rows = stmt
        .query_map(params![uri, now - WINDOW_SECS], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?, r.get::<_, i64>(2)?))
        })
        .unwrap();
    for row in rows.flatten() {
        let (ty, w, at) = row;
        let dt = (now - at).max(0) as f64;
        let w = if ty == "diffuse" { 0.3 } else { w };
        sum += w * 2f64.powf(-dt / HALF_LIFE_SECS as f64);
    }
    sum
}

// ─── timing helper ──────────────────────────────────────────────────────────

/// Median of `runs` async calls.
async fn median_ms<F, Fut>(mut f: F, runs: usize) -> f64
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = f64>,
{
    let mut xs = Vec::with_capacity(runs);
    for _ in 0..runs {
        xs.push(f().await);
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    xs[runs / 2]
}

/// One timed `work_traces_heat_exact` call → elapsed ms.
async fn time_exact(db: &SilvaDB, uri: &str, now: i64) -> f64 {
    let t0 = Instant::now();
    let h = db.work_traces_heat_exact(black_box(uri), Some(now)).await.unwrap();
    black_box(h);
    t0.elapsed().as_secs_f64() * 1000.0
}

/// One timed `work_traces_heat_batch` call over `uris` → elapsed ms.
async fn time_batch(db: &SilvaDB, uris: &[String], now: i64) -> f64 {
    let t0 = Instant::now();
    let m = db.work_traces_heat_batch(black_box(uris), Some(now)).await.unwrap();
    black_box(&m);
    t0.elapsed().as_secs_f64() * 1000.0
}

// ─── main ───────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    println!("ADR-015 Fase 3 — work_traces heat SQL latency (T901)");
    println!("T½=14400s (4h) · SQL window=16h · deterministic seeds · median of 7\n");

    let now = 1_800_000_000i64;
    let mut json: HashMap<&str, serde_json::Value> = HashMap::new();

    // ── [1]+[2] realistic fleet scale: 10 zones × 144 traces ────────────────
    let db1 = SilvaDB::in_memory().await.expect("silva init");
    seed_fleet(&db1, 10).await;
    let rows1: i64 = {
        let conn_arc = db1.conn_lock();
        let conn = conn_arc.lock().await;
        conn.query_row("SELECT COUNT(*) FROM work_traces", [], |r| r.get(0)).unwrap()
    };
    println!("[1][2] fleet scale: {rows1} rows (10 zones × 144 touches, 24h horizon, 6/h)");

    let zone_uris: Vec<String> = (0..10).map(|z| format!("zone:bench-{z:02}")).collect();
    let e1 = median_ms(|| time_exact(&db1, "zone:bench-00", now), 7).await;
    let b1 = median_ms(|| time_batch(&db1, &zone_uris, now), 7).await;
    // Batch over an unknown URI set of the same size (every probe filters to
    // zero via the compound index): floor cost of the SQL path, 0 rows read.
    let cold_uris: Vec<String> = (0..10).map(|z| format!("zone:cold-{z:02}")).collect();
    let b1_cold = median_ms(|| time_batch(&db1, &cold_uris, now), 7).await;
    println!(
        "  [1] heat_exact (per call)     : {e1:8.3} ms   (~{:.0} calls/min single-thread)",
        60_000.0 / e1
    );
    println!(
        "  [2] heat_batch (10 zones)     : {b1:8.3} ms   (~{:.0} calls/min single-thread)",
        60_000.0 / b1
    );
    println!("      batch over cold URIs      : {b1_cold:8.3} ms   (index-only floor, 0 rows scanned)");
    json.insert(
        "fleet_scale",
        serde_json::json!({
            "rows": rows1,
            "heat_exact_ms": e1,
            "heat_batch_10_zones_ms": b1,
            "heat_batch_cold_uris_ms": b1_cold,
        }),
    );

    // ── [3] heat path under load: the per-zone stress ceiling ───────────────
    // ~10k rows INSIDE the 16h window for one zone — ≈1 trace every 5.8s for
    // 16h straight. 20k pre-window rows ride along to prove the SQL filter
    // skips evanesced history without reading it.
    let db2 = SilvaDB::in_memory().await.expect("silva init");
    seed_stress(&db2, 10_000, 20_000).await;
    println!("\n[3] stress ceiling: 30,000 rows total on one zone (~10k inside the 16h window)");

    // Observation at ceiling density: the raw exponential sum (~8.2k) far
    // exceeds the cap of 2.0 → the signal saturates at every t in the window
    // (the half-life CURVE itself is pinned by the T753 unit tests at sparse
    // densities; at fleet densities the cap dominates by orders of magnitude).
    let h0 = db2.work_traces_heat_exact("zone:stress", Some(now)).await.unwrap();
    let h_half = db2
        .work_traces_heat_exact("zone:stress", Some(now + HALF_LIFE_SECS))
        .await
        .unwrap();
    let raw0 = raw_sum(&db2, "zone:stress", now).await;
    let raw_half = raw_sum(&db2, "zone:stress", now + HALF_LIFE_SECS).await;
    println!("  heat(t=0)       : {h0:.3}  (raw sum {raw0:.0} → clamped by cap {HEAT_CAP})");
    println!("  heat(t=now+T½)  : {h_half:.3}  (raw sum {raw_half:.0} → still clamped)");
    assert!((h0 - HEAT_CAP).abs() < 1e-6, "cap at ceiling density: {h0}");
    assert!((h_half - HEAT_CAP).abs() < 1e-6, "cap survives T½ at ceiling density: {h_half}");

    let e3 = median_ms(|| time_exact(&db2, "zone:stress", now), 7).await;
    let e3_t_half = median_ms(
        || time_exact(&db2, "zone:stress", now + HALF_LIFE_SECS),
        7,
    )
    .await;
    println!("  [3] heat_exact @ ~10k rows in window : {e3:8.3} ms/median");
    println!("      same query at t=now+T½           : {e3_t_half:8.3} ms/median (same scan, same cap)");
    json.insert(
        "stress_ceiling",
        serde_json::json!({
            "rows_total": 30_000,
            "rows_in_window": 10_000,
            "heat_exact_ms": e3,
            "heat_exact_at_half_life_ms": e3_t_half,
            "raw_sum_at_t0": raw0,
            "raw_sum_at_half_life": raw_half,
            "cap": HEAT_CAP,
        }),
    );

    // ── [4] scan bound: index (target_uri, touched_at DESC) ─────────────────
    let (scanned, plan) = {
        let conn_arc = db2.conn_lock();
        let conn = conn_arc.lock().await;
        let scanned: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM work_traces WHERE target_uri='zone:stress' AND touched_at >= ?1",
                params![now - WINDOW_SECS],
                |r| r.get(0),
            )
            .unwrap();
        let plan: String = conn
            .query_row(
                "EXPLAIN QUERY PLAN SELECT trace_type, weight, touched_at FROM work_traces
                 WHERE target_uri = ?1 AND touched_at >= ?2",
                params!["zone:stress", now - WINDOW_SECS],
                |r| r.get(3),
            )
            .unwrap();
        (scanned, plan)
    };
    println!(
        "  [4] rows scanned per query @ ceiling : {scanned} (vs 30,000 total; idx_work_traces_target bounds it)"
    );
    println!("      query plan: {plan}");
    json.insert(
        "scan_bound",
        serde_json::json!({
            "rows_scanned_per_query": scanned,
            "rows_total": 30_000,
            "query_plan": plan,
        }),
    );

    // ── verdict ─────────────────────────────────────────────────────────────
    println!("\nVerdict (re-derive from numbers above, do not trust this line):");
    println!("  heat path is O(rows_in_window) via the compound index; compare");
    println!("  [1]/[2] against recall p50 (~seconds) before calling it a cost.");
    println!("  [3] additionally documents cap saturation at ceiling density.");

    println!("\nJSON:");
    println!("{}", serde_json::to_string_pretty(&json).unwrap());
}
