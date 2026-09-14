//! MD-7 closure: integration test for GET /api/v1/audit/latency.
//!
//! Drives the real `api_v1_routes()` router (same harness as
//! `repo_map_endpoint_test.rs`) with `TYLLUAN_AUDIT_DB` pointed at a temp
//! file, so the dev's live `data/audit.db` (whose rows form a hash chain)
//! is never touched by test data.
//!
//! Coverage this buys, honestly stated: route registration through the real
//! router, query-param deserialization, the readonly DB open, the SELECT,
//! and the exact aggregation math — end to end. The audit *writer* side
//! (`log_audit_entry`) is exercised by its own lib tests; spawning real
//! guilds to produce rows through the full do-path is out of scope here
//! (same reasoning as the WS3 collector test).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tower::ServiceExt;

use tylluan_kernel::config::TimeoutsConfig;
use tylluan_kernel::doctor::Doctor;
use tylluan_kernel::memory::coloquio::ColoquioDb;
use tylluan_kernel::memory::hybrid::HybridMemory;
use tylluan_kernel::memory::mailbox::Mailbox;
use tylluan_kernel::memory::silva::SilvaDB;
use tylluan_kernel::registry::actor::RegistryActor;
use tylluan_kernel::registry::guild_process::GuildRegistry;
use tylluan_kernel::router::matcher::GuildMatcher;
use tylluan_kernel::transport::http::api_v1::api_v1_routes;
use tylluan_kernel::transport::http::HttpState;
use tylluan_kernel::transport::server::TylluanServer;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

async fn test_state() -> Arc<HttpState> {
    let workspace_root = std::env::current_dir().unwrap_or_default();
    let registry_raw = GuildRegistry::new(workspace_root.clone(), 5, TimeoutsConfig::default(), 5);
    let registry_arc = Arc::new(RwLock::new(registry_raw));
    let (registry_actor, registry_handle) = RegistryActor::new(registry_arc.clone());
    tokio::spawn(async move { registry_actor.run().await; });

    {
        let mut reg = registry_arc.write().await;
        for g in tylluan_kernel::router::catalog::builtin_catalog() {
            reg.register(&g.name, &g.module_path, false, None);
        }
    }

    let memory = Arc::new(HybridMemory::in_memory().await.unwrap());
    let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
    silva.init().await.unwrap();
    let mailbox = Arc::new(Mailbox::in_memory().await.unwrap());
    mailbox.init().await.unwrap();
    let coloquio = Arc::new(ColoquioDb::new(":memory:").unwrap());
    let curriculum = Arc::new(std::sync::Mutex::new(
        tylluan_kernel::curriculum::CurriculumLearner::new_in_memory(1).unwrap(),
    ));
    let doctor = Arc::new(Doctor::new(registry_arc.clone(), memory.clone(), silva.clone(), curriculum));
    let matcher = Arc::new(GuildMatcher::new(tylluan_kernel::router::catalog::builtin_catalog()));
    let node_router = tylluan_kernel::memory::agent_nodes::AgentNodeRouter::new(tokio::sync::broadcast::channel(1).0);
    let server = TylluanServer::new(
        registry_arc, matcher.clone(), memory.clone(), silva.clone(),
        mailbox.clone(), doctor.clone(), node_router.clone(),
    );
    let (broadcast_tx, _) = tokio::sync::broadcast::channel(10);
    let (download_tx, _) = tokio::sync::broadcast::channel(10);

    let cwd = std::env::current_dir().unwrap_or_default();
    let repo_map = tylluan_kernel::repo_map::RepoMap::build(&cwd);

    Arc::new(HttpState {
        version: "test".to_string(),
        auth_token: None,
        dev_mode: Some(true),
        start_time: Instant::now(),
        server: Some(Arc::new(RwLock::new(server))),
        registry: registry_handle,
        doctor,
        memory,
        silva: silva.clone(),
        mailbox,
        coloquio,
        broadcast_tx,
        download_progress_tx: download_tx,
        sessions: Arc::new(RwLock::new(HashMap::new())),
        guild_status_cache: Arc::new(std::sync::Mutex::new(None)),
        agent_rate_limiter: Arc::new(dashmap::DashMap::new()),
        ip_rate_limiter: Arc::new(tylluan_kernel::security::rate_limiter::RateLimiter::new(Some(300))),
        config: tylluan_kernel::config::TylluanConfig::load_cached().unwrap_or_else(|_| {
            Arc::new(RwLock::new(tylluan_kernel::config::TylluanConfig::default()))
        }),
        matcher,
        tunnel_wsl_url: None,
        oauth: Arc::new(tylluan_kernel::transport::http::oauth::OAuthState::new("http://localhost:3030".to_string())),
        metrics_ring: Arc::new(RwLock::new(tylluan_kernel::metrics_ring::MetricsRingBuffer::new())),
        jobs: Arc::new(tylluan_kernel::memory::jobs::JobQueue::open(std::path::Path::new(":memory:")).unwrap()),
        agents_contract: Arc::new(tylluan_kernel::security::agents_contract::AgentsContract::empty()),
        cancel_token: tokio_util::sync::CancellationToken::new(),
        node_router,
        health_ready: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        journal: Arc::new(tylluan_kernel::transport::http::api_v1::api_journal::JournalDb::open(":memory:").unwrap()),
        agent_registry: tylluan_kernel::transport::http::api_v1::api_agents::AgentRegistry::new(7200),
        contract_registry: tylluan_kernel::transport::http::api_v1::api_contracts::ContractRegistry::new(),
        contract_db: Arc::new(tylluan_kernel::transport::http::api_v1::api_contracts::ContractDb::open(":memory:").unwrap()),
        peer_db: Arc::new(tylluan_kernel::federation::PeerDb::open(":memory:").unwrap()),
        node_identity: Arc::new(tylluan_link::identity::NodeIdentity::load_or_create(
            &std::env::temp_dir().join(format!("tylluan_id_al_{}", TEST_COUNTER.fetch_add(1, Ordering::Relaxed))),
        ).unwrap()),
        nat_cache: Arc::new(tokio::sync::RwLock::new(None)),
        dht_routing_table: Arc::new(tokio::sync::RwLock::new(tylluan_link::dht::RoutingTable::new("test-node".to_string()))),
        p2p_pool: Arc::new(tokio::sync::Mutex::new(tylluan_link::p2p::P2pSessionPool::new(16, 300))),
        gossip_engine: Arc::new(tokio::sync::RwLock::new(tylluan_link::gossip::GossipEngine::new(
            "test-node".to_string(),
            tylluan_link::gossip::GossipConfig::default(),
        ))),
        capability_registry: Arc::new(std::sync::Mutex::new(tylluan_link::capability::CapabilityRegistry::new(
            std::time::Duration::from_secs(300),
        ))),
        dispatch_router: Arc::new(std::sync::Mutex::new(tylluan_link::dispatch::DispatchRouter::new(
            Arc::new(std::sync::Mutex::new(tylluan_link::capability::CapabilityRegistry::new(std::time::Duration::from_secs(300)))),
            std::time::Duration::from_secs(60),
        ))),
        dispatch_queue: Arc::new(std::sync::Mutex::new(tylluan_link::dispatch::DispatchQueue::new(1000))),
        repo_map,
        a2a_task_manager: Arc::new(tylluan_kernel::transport::http::a2a::A2aTaskManager::new(silva.clone())),
        a2a_agents: Arc::new(tylluan_kernel::transport::http::a2a_client::A2aAgentStore::new(silva.clone())),
        a2a_client: Arc::new(tylluan_kernel::transport::http::a2a_client::A2aClient::new().unwrap()),
    })
}

fn build_test_app(state: Arc<HttpState>) -> axum::Router {
    axum::Router::new().merge(api_v1_routes()).with_state(state)
}

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

    let state = test_state().await;
    let app = build_test_app(state);

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
    let state = test_state().await;
    let app = build_test_app(state);
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
    let state = test_state().await;
    let app = build_test_app(state);
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
