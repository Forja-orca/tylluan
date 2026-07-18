use tylluan_kernel::transport::http::api_v1::api_v1_routes;
use std::sync::atomic::{AtomicU64, Ordering};
use tylluan_kernel::transport::http::HttpState;
use tylluan_kernel::transport::server::TylluanServer;
use tylluan_kernel::registry::guild_process::GuildRegistry;
use tylluan_kernel::config::TimeoutsConfig;
use tylluan_kernel::router::matcher::GuildMatcher;
use tylluan_kernel::memory::hybrid::HybridMemory;
use tylluan_kernel::memory::silva::SilvaDB;
use tylluan_kernel::memory::mailbox::Mailbox;
use tylluan_kernel::memory::coloquio::ColoquioDb;
use tylluan_kernel::doctor::Doctor;
use tylluan_kernel::registry::actor::RegistryActor;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;
use std::collections::HashMap;
use std::time::Instant;

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
    let curriculum = Arc::new(std::sync::Mutex::new(tylluan_kernel::curriculum::CurriculumLearner::new_in_memory(1).unwrap()));
    let doctor = Arc::new(Doctor::new(registry_arc.clone(), memory.clone(), silva.clone(), curriculum));
    let matcher = Arc::new(GuildMatcher::new(tylluan_kernel::router::catalog::builtin_catalog()));
    let node_router = tylluan_kernel::memory::agent_nodes::AgentNodeRouter::new(tokio::sync::broadcast::channel(1).0);
    let server = TylluanServer::new(
        registry_arc, matcher.clone(), memory.clone(), silva.clone(),
        mailbox.clone(), doctor.clone(), node_router.clone(),
    );
    let (broadcast_tx, _) = tokio::sync::broadcast::channel(10);
    let (download_tx, _) = tokio::sync::broadcast::channel(10);

    // Build repo map from the current (test) directory
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
        silva,
        mailbox,
        coloquio,
        broadcast_tx,
        download_progress_tx: download_tx,
        sessions: Arc::new(RwLock::new(HashMap::new())),
        guild_status_cache: Arc::new(std::sync::Mutex::new(None)),
        agent_rate_limiter: Arc::new(dashmap::DashMap::new()),
        ip_rate_limiter: Arc::new(tylluan_kernel::security::rate_limiter::RateLimiter::new(Some(300))),
        config: tylluan_kernel::config::TylluanConfig::load_cached().unwrap_or_else(|_| Arc::new(RwLock::new(tylluan_kernel::config::TylluanConfig::default()))),
        matcher,
        tunnel_wsl_url: None,
        oauth: Arc::new(tylluan_kernel::transport::http::oauth::OAuthState::new("http://localhost:3030".to_string())),
        metrics_ring: Arc::new(RwLock::new(tylluan_kernel::metrics_ring::MetricsRingBuffer::new())),
        jobs: Arc::new(tylluan_kernel::memory::jobs::JobQueue::open(std::path::Path::new(":memory:")).unwrap()),
        cancel_token: tokio_util::sync::CancellationToken::new(),
        node_router,
        health_ready: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        journal: Arc::new(tylluan_kernel::transport::http::api_v1::api_journal::JournalDb::open(":memory:").unwrap()),
        agent_registry: tylluan_kernel::transport::http::api_v1::api_agents::AgentRegistry::new(7200),
        contract_registry: tylluan_kernel::transport::http::api_v1::api_contracts::ContractRegistry::new(),
        contract_db: Arc::new(tylluan_kernel::transport::http::api_v1::api_contracts::ContractDb::open(":memory:").unwrap()),
        peer_db: Arc::new(tylluan_kernel::federation::PeerDb::open(":memory:").unwrap()),
        node_identity: Arc::new(tylluan_link::identity::NodeIdentity::load_or_create(&std::env::temp_dir().join(format!("tylluan_id_rm_{}", TEST_COUNTER.fetch_add(1, Ordering::Relaxed)))).unwrap()),
        nat_cache: Arc::new(tokio::sync::RwLock::new(None)),
        dht_routing_table: Arc::new(tokio::sync::RwLock::new(tylluan_link::dht::RoutingTable::new("test-node".to_string()))),
        p2p_pool: Arc::new(tokio::sync::Mutex::new(tylluan_link::p2p::P2pSessionPool::new(16, 300))),
        gossip_engine: Arc::new(tokio::sync::RwLock::new(tylluan_link::gossip::GossipEngine::new("test-node".to_string(), tylluan_link::gossip::GossipConfig::default()))),
        capability_registry: Arc::new(std::sync::Mutex::new(tylluan_link::capability::CapabilityRegistry::new(std::time::Duration::from_secs(300)))),
        dispatch_router: Arc::new(std::sync::Mutex::new(tylluan_link::dispatch::DispatchRouter::new(
            Arc::new(std::sync::Mutex::new(tylluan_link::capability::CapabilityRegistry::new(std::time::Duration::from_secs(300)))),
            std::time::Duration::from_secs(60),
        ))),
        dispatch_queue: Arc::new(std::sync::Mutex::new(tylluan_link::dispatch::DispatchQueue::new(1000))),
        repo_map,
        a2a_task_manager: Arc::new(tylluan_kernel::transport::http::a2a::A2aTaskManager::new()),
    })
}

fn build_test_app(state: Arc<HttpState>) -> axum::Router {
    axum::Router::new()
        .merge(api_v1_routes())
        .with_state(state)
}

#[tokio::test(flavor = "multi_thread")]
async fn test_repo_map_endpoint_returns_200() {
    let state = test_state().await;
    let app = build_test_app(state);

    let req = Request::builder()
        .uri("/api/v1/repo-map")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "repo-map should return 200");

    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json["total_files"].as_u64().is_some(), "total_files should be present");
    assert!(json["total_dirs"].as_u64().is_some(), "total_dirs should be present");
    assert!(json["total_lines"].as_u64().is_some(), "total_lines should be present");
    assert!(json["build_duration_ms"].as_u64().is_some(), "build_duration_ms should be present");
    assert!(json["root"].as_str().is_some(), "root should be present");
    assert!(json["languages"].is_object(), "languages should be an object");
    assert!(json["top_level_dirs"].is_array(), "top_level_dirs should be an array");
    assert!(json["build_duration_ms"].as_u64().unwrap() > 0, "build should take >0ms");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_repo_map_contains_rust_identifiers() {
    let state = test_state().await;
    let app = build_test_app(state);

    let req = Request::builder()
        .uri("/api/v1/repo-map")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    // The test runner's current dir should contain Rust files with identifiers
    let idents = json["identifiers"].as_object().unwrap();
    // At minimum, the repo_map.rs file itself should have its own identifiers
    let has_repo_map_idents = idents.keys().any(|k| k.contains("repo_map"));
    assert!(has_repo_map_idents || !idents.is_empty(),
        "should have at least some Rust identifiers");
}
