//! Integration tests for GET /api/v1/stigmergy/zones (ADR-015 Fase 3, T753).
//!
//! `stigmergy_zones_handler` had zero tests — these pin its three documented
//! behaviors through the REAL router via the shared harness:
//!   1. Default zones present (and only them) while `work_traces` is empty.
//!   2. Correct aggregation when real traces exist (counts, agents, last touch).
//!   3. Fallback zone for a traced non-default URI, combined with defaults,
//!      with no duplicates anywhere in the list.
//!
//! Harness lives in `tests/support/mod.rs` (HttpState + api_v1_routes driver).
//! Each test builds a FRESH in-memory state — no env seams, no shared
//! process-global state — so the battery's one-sequential-test rule does not
//! apply here and these run safely in parallel. Time is wall-clock-relative:
//! the handler computes its own `now` internally (no injection point), so
//! seeds use `now - <small offset>` and heat assertions stay loose enough to
//! survive the seconds between seeding and the request.

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

/// Default-zone count in the handler (pinned here so an intentional change
/// to the zones table shows up as a reviewable diff, not silent drift).
const DEFAULT_ZONE_COUNT: usize = 12;

async fn seed_work_trace(
    state: &std::sync::Arc<tylluan_kernel::transport::http::HttpState>,
    uri: &str,
    agent_id: &str,
    trace_type: &str,
    weight: f64,
    touched_at: i64,
) {
    let conn_arc = state.silva.conn_lock();
    let conn = conn_arc.lock().await;
    conn.execute(
        "INSERT INTO work_traces (target_uri, target_kind, agent_id, trace_type, weight, touched_at)
         VALUES (?1, 'file', ?2, ?3, ?4, ?5)",
        rusqlite::params![uri, agent_id, trace_type, weight, touched_at],
    )
    .unwrap();
}

async fn get_zones(
    app: axum::Router,
) -> serde_json::Value {
    let req = Request::builder()
        .uri("/api/v1/stigmergy/zones")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "stigmergy/zones must return 200");
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zones_defaults_present_on_empty_work_traces() {
    let state = support::test_state("stg").await;
    let app = support::build_test_app(state);

    let json = get_zones(app).await;

    assert_eq!(json["count"].as_u64().unwrap() as usize, DEFAULT_ZONE_COUNT);
    let zones = json["zones"].as_array().unwrap();
    assert_eq!(zones.len(), DEFAULT_ZONE_COUNT);

    // Every default zone: zero heat, no traces, low contention, full shape.
    for z in zones {
        assert_eq!(z["heat"].as_f64().unwrap(), 0.0, "empty table must yield heat 0.0");
        assert_eq!(z["total_traces"].as_u64().unwrap(), 0);
        assert!(z["active_agents"].as_array().unwrap().is_empty());
        assert!(z["traces"].as_array().unwrap().is_empty());
        assert_eq!(z["contention_risk"].as_str().unwrap(), "low");
        // Default zones carry curated metadata.
        assert!(z["description"].as_str().unwrap().len() > 10, "default zones have real descriptions");
        assert_eq!(z["half_life_hours"].as_f64().unwrap(), 4.0);
    }

    // The union on an empty table must be EXACTLY the curated defaults —
    // pinned as a set so any intentional change shows up as a reviewable
    // diff (same philosophy as DEFAULT_ZONE_COUNT). Includes docs-site/src,
    // which does NOT share the docs/ prefix — caught by a prefix assertion,
    // fixed by pinning the real list instead.
    let mut ids: Vec<&str> = zones.iter().map(|z| z["zone_id"].as_str().unwrap()).collect();
    ids.sort_unstable();
    let mut expected: Vec<&str> = vec![
        "crates/tylluan-kernel/transport",
        "dashboard/src/components",
        "docs/reference/adr",
        "crates/tylluan-kernel/memory",
        "crates/tylluan-link/gossip",
        "guilds/core",
        "crates/tylluan-link/p2p",
        "docs-site/src",
        "crates/tylluan-kernel/config",
        "crates/tylluan-kernel/router",
        "guilds/vision",
        "docs/roadmap",
    ];
    expected.sort_unstable();
    assert_eq!(ids, expected, "empty table must expose exactly the curated defaults");

    // Envelope contract.
    assert_eq!(json["window_hours"].as_u64().unwrap(), 16);
    assert!(json["ts"].as_str().is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zones_aggregates_real_traces() {
    let state = support::test_state("stg").await;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Two agents, two trace types, on one REAL default zone (decay.rs math
    // itself is unit-tested separately; here the handler's aggregation).
    seed_work_trace(&state, "crates/tylluan-kernel/router", "agent-a", "direct", 1.0, now - 60).await;
    seed_work_trace(&state, "crates/tylluan-kernel/router", "agent-b", "diffuse", 1.0, now - 30).await;

    let app = support::build_test_app(state);
    let json = get_zones(app).await;

    let zones = json["zones"].as_array().unwrap();
    let hot: Vec<&serde_json::Value> = zones
        .iter()
        .filter(|z| z["zone_id"] == "crates/tylluan-kernel/router")
        .collect();
    assert_eq!(hot.len(), 1, "zone must appear exactly once — no duplicates from the union");
    let z = hot[0];

    assert_eq!(z["total_traces"].as_u64().unwrap(), 2);
    let agents = z["active_agents"].as_array().unwrap();
    assert_eq!(agents.len(), 2);
    let agent_ids: Vec<&str> = agents.iter().map(|a| a.as_str().unwrap()).collect();
    assert!(agent_ids.contains(&"agent-a") && agent_ids.contains(&"agent-b"));

    // last_touched_at = the max touched_at seed (now - 30).
    assert_eq!(z["last_touched_at"].as_i64().unwrap(), now - 30);

    // Two recent traces on one zone: fresh direct (1.0) + fresh diffuse
    // (0.3), decayed by seconds only → well above 1.0 after cap-free sum.
    let heat = z["heat"].as_f64().unwrap();
    assert!(heat > 1.0 && heat <= 2.0, "two fresh traces should sum ~1.3 (cap 2.0), got {heat}");
    // moderate risk: heat >= 1.0 but only if >1 agents → here 2 agents and
    // heat < 1.5 → "moderate" (handler's rule).
    assert_eq!(z["contention_risk"].as_str().unwrap(), "moderate");

    // Trace listing (capped at 10 per zone) carries the raw rows.
    let traces = z["traces"].as_array().unwrap();
    assert_eq!(traces.len(), 2);
    assert!(traces[0]["agent_id"].as_str().is_some());
    assert!(traces[0]["trace_id"].as_i64().is_some());

    // Hottest zone first: with every other zone at 0.0, ours must lead.
    assert_eq!(zones[0]["zone_id"], "crates/tylluan-kernel/router", "zones sorted by heat desc");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zones_fallback_for_non_default_uri_without_duplicates() {
    let state = support::test_state("stg").await;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // A traced URI that is NOT a default zone → fallback classification.
    let novel = "benchmarks/spikes/qwen3_digest";
    seed_work_trace(&state, novel, "agent-x", "direct", 1.0, now - 10).await;

    let app = support::build_test_app(state);
    let json = get_zones(app).await;

    let zones = json["zones"].as_array().unwrap();

    // No duplicates: unique zone_ids, total = defaults + 1 fallback.
    let ids: Vec<&str> = zones.iter().map(|z| z["zone_id"].as_str().unwrap()).collect();
    let mut unique = ids.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(ids.len(), unique.len(), "duplicate zone_ids are a union bug");
    assert_eq!(zones.len(), DEFAULT_ZONE_COUNT + 1);

    // The fallback zone: inferred subsystem ("other" — outside all prefixes),
    // synthesized description, empty neighbors, real heat.
    let fb = zones.iter().find(|z| z["zone_id"] == novel).expect("fallback zone must exist");
    assert_eq!(fb["subsystem"].as_str().unwrap(), "other");
    assert_eq!(fb["description"].as_str().unwrap(), format!("Workspace resource: {novel}"));
    assert!(fb["neighbor_zones"].as_array().unwrap().is_empty());
    assert!(fb["heat"].as_f64().unwrap() > 0.5);
    assert_eq!(fb["total_traces"].as_u64().unwrap(), 1);

    // All 12 defaults still present alongside it.
    for d in [
        "crates/tylluan-kernel/transport",
        "dashboard/src/components",
        "docs/reference/adr",
        "guilds/core",
    ] {
        assert!(ids.contains(&d), "default zone {d} must survive the union");
    }

    // Fresh single trace → the fallback (heat ~1.0) sorts first.
    assert_eq!(zones[0]["zone_id"], novel);
}
