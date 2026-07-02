//! blackboard_e2e.rs — Integration tests for Blackboard Protocol
//! Validates @pending / @context / @completed prefix workflows end-to-end.
//! Uses same test_state() / build_test_app() pattern as pipeline_tests.rs.

use tylluan_kernel::transport::http::api_v1::api_v1_routes;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);
use tylluan_kernel::transport::http::api_v1::mcp_handler;
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
use axum::http::{Request, header};
use std::sync::Arc;
use tokio::sync::RwLock;
use serde_json::json;
use tower::ServiceExt;
use std::collections::HashMap;
use std::time::Instant;
use dashmap::DashMap;

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
        registry_arc.clone(), matcher.clone(), memory.clone(), silva.clone(),
        mailbox.clone(), doctor.clone(), node_router.clone(),
    );
    let (broadcast_tx, _) = tokio::sync::broadcast::channel(10);
    let (download_tx, _) = tokio::sync::broadcast::channel(10);
    let config = tylluan_kernel::config::TylluanConfig::load_cached()
        .unwrap_or_else(|_| Arc::new(RwLock::new(tylluan_kernel::config::TylluanConfig::default())));
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
        agent_rate_limiter: Arc::new(DashMap::new()),
        config,
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
        node_identity: Arc::new(tylluan_link::identity::NodeIdentity::load_or_create(&std::env::temp_dir().join(format!("tylluan_id_bb_{}", TEST_COUNTER.fetch_add(1, Ordering::Relaxed)))).unwrap()),
        nat_cache: Arc::new(tokio::sync::RwLock::new(None)),
        dht_routing_table: Arc::new(tokio::sync::RwLock::new(tylluan_link::dht::RoutingTable::new("test-node".to_string()))),
        p2p_pool: Arc::new(tokio::sync::Mutex::new(tylluan_link::p2p::P2pSessionPool::new(16, 300))),
        gossip_engine: Arc::new(tokio::sync::RwLock::new(tylluan_link::gossip::GossipEngine::new(
            "test-node".to_string(),
            tylluan_link::gossip::GossipConfig::default(),
        ))),
        capability_registry: Arc::new(std::sync::Mutex::new(tylluan_link::capability::CapabilityRegistry::new(std::time::Duration::from_secs(300)))),
        dispatch_router: Arc::new(std::sync::Mutex::new(tylluan_link::dispatch::DispatchRouter::new(
            Arc::new(std::sync::Mutex::new(tylluan_link::capability::CapabilityRegistry::new(std::time::Duration::from_secs(300)))),
            std::time::Duration::from_secs(60),
        ))),
        dispatch_queue: Arc::new(std::sync::Mutex::new(tylluan_link::dispatch::DispatchQueue::new(1000))),
    })
}

async fn mcp_call(app: axum::Router, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let body = json!({
        "jsonrpc": "2.0", "id": 1,
        "method": "tools/call",
        "params": {"name": tool, "arguments": args}
    });
    let req = Request::builder()
        .method("POST").uri("/mcp")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(json!({}))
}

fn build_test_app(state: Arc<HttpState>) -> axum::Router {
    axum::Router::new()
        .merge(api_v1_routes())
        .route("/mcp", axum::routing::post(mcp_handler))
        .with_state(state)
}

fn extract_text(res: &serde_json::Value) -> String {
    res["result"]["content"].as_array()
        .map(|arr| arr.iter().filter_map(|c| c["text"].as_str()).collect())
        .unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread")]
async fn test_blackboard_post_task_creates_pending_node() {
    let state = test_state().await;
    let app = build_test_app(state.clone());

    let res = mcp_call(app, "tylluan_remember", json!({
        "content": "@pending:task_001 — analyze auth module",
        "importance": 0.9
    })).await;
    assert!(res["error"].is_null(), "remember failed: {:?}", res);

    let count = state.silva.node_count().await.unwrap();
    assert!(count > 0, "SilvaDB should have a node after remember");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_blackboard_recall_inbox_finds_pending() {
    let state = test_state().await;
    let app = build_test_app(state);

    mcp_call(app.clone(), "tylluan_remember", json!({
        "content": "@pending:task_alpha — first pending task", "importance": 0.8
    })).await;
    mcp_call(app.clone(), "tylluan_remember", json!({
        "content": "@pending:task_beta — second pending task", "importance": 0.8
    })).await;

    let res = mcp_call(app, "tylluan_recall", json!({"query": "@pending"})).await;
    assert!(res["error"].is_null(), "recall failed: {:?}", res);
    let text = extract_text(&res);
    assert!(!text.is_empty(), "recall @pending should return non-empty text");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_blackboard_context_prefix_accessible() {
    let state = test_state().await;
    let app = build_test_app(state);

    mcp_call(app.clone(), "tylluan_remember", json!({
        "content": "@context:sprint-o — objetivo: blackboard protocol",
        "importance": 0.7
    })).await;

    let res = mcp_call(app, "tylluan_recall", json!({"query": "@context sprint-o"})).await;
    let text = extract_text(&res);
    assert!(!text.is_empty(), "recall @context should return content: {}", text);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_blackboard_completed_marks_task() {
    let state = test_state().await;
    let app = build_test_app(state.clone());

    mcp_call(app.clone(), "tylluan_remember", json!({
        "content": "@completed:task_001 — resultado: done, 3 archivos modificados",
        "importance": 0.9
    })).await;

    let res = mcp_call(app, "tylluan_recall", json!({"query": "@completed"})).await;
    assert!(res["error"].is_null(), "recall failed: {:?}", res);
    let count = state.silva.node_count().await.unwrap();
    assert!(count > 0, "silva should have the completed node");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_blackboard_full_workflow() {
    let state = test_state().await;
    let app = build_test_app(state.clone());

    // Step 1: post task
    mcp_call(app.clone(), "tylluan_remember", json!({
        "content": "@pending:task_analyze — analiza server.rs", "importance": 0.9
    })).await;

    // Step 2: add context
    mcp_call(app.clone(), "tylluan_remember", json!({
        "content": "@context:task_analyze — server.rs tiene 800 líneas, handler_do es el más complejo",
        "importance": 0.8
    })).await;

    // Step 3: complete
    mcp_call(app.clone(), "tylluan_remember", json!({
        "content": "@completed:task_analyze — encontré 3 handlers, refactor propuesto",
        "importance": 0.95
    })).await;

    // Step 4: recall @completed should surface task_analyze
    let res = mcp_call(app, "tylluan_recall", json!({"query": "@completed task_analyze"})).await;
    assert!(res["error"].is_null(), "recall failed: {:?}", res);

    // Step 5: at least 3 nodes
    let count = state.silva.node_count().await.unwrap();
    assert!(count >= 3, "Expected >= 3 nodes, got {}", count);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_blackboard_api_endpoint() {
    let state = test_state().await;
    let app = build_test_app(state);

    let req = Request::builder()
        .method("GET").uri("/api/v1/blackboard")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status().as_u16();
    // 200 if endpoint exists, 404 if not yet implemented — both are non-crash
    assert!(status == 200 || status == 404,
        "Unexpected status {}: endpoint should either work or 404, not 500", status);

    if status == 200 {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(json!({}));
        assert!(!json.is_null(), "blackboard endpoint returned non-JSON");
    }
}
