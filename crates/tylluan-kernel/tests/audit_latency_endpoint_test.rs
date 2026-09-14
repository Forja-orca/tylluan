//! MD-7 closure: integration test for GET /api/v1/audit/latency.
//!
//! Drives the real `api_v1_routes()` router with `TYLLUAN_AUDIT_DB` pointed
//! at a temp file, so the dev's live `data/audit.db` (whose rows form a hash
//! chain) is never touched by test data. Harness (HttpState builder +
//! api_v1_routes driver) lives in the shared `tests/support/mod.rs`.
//!
//! Coverage this buys, honestly stated: route registration through the real
//! router, query-param deserialization, the readonly DB open, the SELECT,
//! and the exact aggregation math — end to end. The audit *writer* side
//! (`log_audit_entry`) is exercised by its own lib tests; spawning real
//! guilds to produce rows through the full do-path is out of scope here
//! (same reasoning as the WS3 collector test).

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use std::sync::atomic::{AtomicU64, Ordering};
use tower::ServiceExt;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Fixture: 5 dispatches — three with real durations (100, 200, 50 ms),
/// one error, one NULL latency, one literal 0 (secondary path). Expected
/// math (nearest-rank, n=3 sampled): sorted [50,100,200] → p50=100 (ceil(1.5)=2),
/// p95=200, p99=200, max=200; total=5, errors=1 (20.0%), zero_or_null=2.
fn seed_temp_audit_db(path: &std::path::Path) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
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
        );",
    )
    .unwrap();
    let now = chrono::Utc::now().to_rfc3339();
    for (status, latency) in [
        ("ok", Some(100i64)),
        ("ok", Some(200)),
        ("error", Some(50)),
        ("ok", None),
        ("ok", Some(0)),
    ] {
        conn.execute(
            "INSERT INTO guild_audit_log (timestamp, guild, tool_name, status, latency_ms, prev_hash, hash)
             VALUES (?1, 'guild_x', 'tool_y', ?2, ?3, 'p', 'h')",
            rusqlite::params![now, status, latency],
        )
        .unwrap();
    }
}

fn unique_db_path(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "tylluan_audit_ep_{}_{}_{}.db",
        tag,
        std::process::id(),
        TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

/// One sequential test (not several): `TYLLUAN_AUDIT_DB` is process-global
/// and cargo runs a binary's tests concurrently, so all env-seam phases run
/// in a single test body in a fixed order. Three phases: exact stats math,
/// window param, missing-DB degradation.
/// SAFETY (set_var): edition-2024 unsafe op, but this test binary contains
/// exactly one #[test], so no concurrent access to the env is possible.
#[tokio::test(flavor = "multi_thread")]
async fn audit_latency_endpoint_end_to_end_through_real_router() {
    // ── Phase 1: exact stats math through the real router ────────────────
    let db = unique_db_path("main");
    seed_temp_audit_db(&db);
    unsafe { std::env::set_var("TYLLUAN_AUDIT_DB", &db) };

    let state = support::test_state("al").await;
    let app = support::build_test_app(state);

    let res = app
        .oneshot(Request::get("/api/v1/audit/latency").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(body["available"], true, "body: {body}");
    assert_eq!(body["source"], "guild_audit_log");
    assert_eq!(body["latency_ms"]["sampled"], 3);
    assert_eq!(body["latency_ms"]["zero_latency_excluded"], 2);
    // Sorted sampled = [50, 100, 200]: nearest-rank p50 = ceil(1.5) = idx 1
    // → 100; p95/p99 → idx 2 → 200.
    assert_eq!(body["latency_ms"]["p50"], 100);
    assert_eq!(body["latency_ms"]["p95"], 200);
    assert_eq!(body["latency_ms"]["p99"], 200);
    assert_eq!(body["latency_ms"]["max"], 200);
    assert_eq!(body["error_rate"]["total"], 5);
    assert_eq!(body["error_rate"]["errors"], 1);
    let pct = body["error_rate"]["percent"].as_f64().unwrap();
    assert!((pct - 20.0).abs() < 0.01, "got {pct}");

    // ── Phase 2: window param accepted, 'now'-stamped rows all included ──
    let state = support::test_state("al").await;
    let app = support::build_test_app(state);
    let res = app
        .oneshot(
            Request::get("/api/v1/audit/latency?window_minutes=10080")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["available"], true, "body: {body}");
    assert_eq!(body["window_minutes"], 10080);
    assert_eq!(body["latency_ms"]["sampled"], 3);
    assert_eq!(body["error_rate"]["total"], 5);

    // ── Phase 3: missing DB degrades to available:false, never a 500 ─────
    let missing = unique_db_path("missing");
    unsafe { std::env::set_var("TYLLUAN_AUDIT_DB", &missing) };
    let state = support::test_state("al").await;
    let app = support::build_test_app(state);
    let res = app
        .oneshot(Request::get("/api/v1/audit/latency").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["available"], false, "body: {body}");
    assert!(body["reason"].is_string());

    let _ = std::fs::remove_file(&db);
}
