//! E2E Sovereign Tool Tests — tylluan_do, tylluan_remember, tylluan_recall
//! Tests the full HTTP → MCP → handler chain in-memory

use tylluan_kernel::transport::http::api_v1::api_v1_routes;
use tylluan_kernel::transport::http::api_v1::mcp_handler;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);
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
use tower::ServiceExt; // for .oneshot()
use std::collections::HashMap;
use std::time::Instant;

// Helper: builds a minimal HttpState for testing
async fn test_state() -> Arc<HttpState> {
    let workspace_root = std::env::current_dir().unwrap_or_default();
    let registry_raw = GuildRegistry::new(workspace_root.clone(), 5, TimeoutsConfig::default(), 5);
    let registry_arc = Arc::new(RwLock::new(registry_raw));
    let (registry_actor, registry_handle) = RegistryActor::new(registry_arc.clone());
    tokio::spawn(async move {
        registry_actor.run().await;
    });

    // Pre-register builtin guilds so tylluan_do can route to them in tests
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
        registry_arc.clone(),
        matcher.clone(),
        memory.clone(),
        silva.clone(),
        mailbox.clone(),
        doctor.clone(),
        node_router.clone(),
    );

    let (broadcast_tx, _) = tokio::sync::broadcast::channel(10);
    let (download_tx, _) = tokio::sync::broadcast::channel(10);

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
        node_identity: Arc::new(tylluan_link::identity::NodeIdentity::load_or_create(&std::env::temp_dir().join(format!("tylluan_id_sov_{}", TEST_COUNTER.fetch_add(1, Ordering::Relaxed)))).unwrap()),
        nat_cache: Arc::new(tokio::sync::RwLock::new(None)),
        dht_routing_table: Arc::new(tokio::sync::RwLock::new(tylluan_link::dht::RoutingTable::new("test-node".to_string()))),
        p2p_pool: 
Arc::new(tokio::sync::Mutex::new(tylluan_link::p2p::P2pSessionPool::new(16, 300))),
        gossip_engine: 
Arc::new(tokio::sync::RwLock::new(tylluan_link::gossip::GossipEngine::new(
            "test-node".to_string(),
            tylluan_link::gossip::GossipConfig::default(),
        ))),
    })
}

// Helper: send a JSON-RPC 2.0 MCP request to the server
async fn mcp_call(
    app: axum::Router,
    method: &str,
    tool_name: &str,
    args: serde_json::Value,
) -> serde_json::Value {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": {
            "name": tool_name,
            "arguments": args
        }
    });
    let req = Request::builder()
        .method("POST")
        .uri("/mcp") // We'll route this to mcp_handler for the test
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(serde_json::json!({}))
}

fn build_test_app(state: Arc<HttpState>) -> axum::Router {
    axum::Router::new()
        .merge(api_v1_routes())
        .route("/mcp", axum::routing::post(mcp_handler))
        .with_state(state)
}

#[tokio::test(flavor = "multi_thread")]
#[cfg(feature = "integration")]
async fn test_tylluan_do_basic_intent() {
    let state = test_state().await;
    let app = build_test_app(state);
    let res = mcp_call(app, "tools/call", "tylluan_do", serde_json::json!({"intent": "list files in /tmp"})).await;
    let result = &res["result"];
    assert!(!result.is_null(), "Expected result object: {:?}", res);
    assert!(res["error"].is_null(), "Expected no error: {:?}", res);
    let content = result["content"].as_array().expect("Content must be an array");
    assert!(!content.is_empty(), "Content should not be empty");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tylluan_do_missing_intent_returns_error() {
    let state = test_state().await;
    let app = build_test_app(state);
    let res = mcp_call(app, "tools/call", "tylluan_do", serde_json::json!({})).await;
    assert!(!res["error"].is_null() || res["result"]["isError"].as_bool() == Some(true) || res["result"]["content"][0]["text"].as_str().unwrap_or("").contains("error"), "Expected error response: {:?}", res);
}

#[tokio::test(flavor = "multi_thread")]
#[cfg(feature = "integration")]
async fn test_tylluan_do_routing_metadata_present() {
    let state = test_state().await;
    let app = build_test_app(state);
    let res = mcp_call(app, "tools/call", "tylluan_do", serde_json::json!({"intent": "check git status"})).await;
    let content = res["result"]["content"].as_array().expect("Content should be an array");
    let text = content.iter().filter_map(|c| c["text"].as_str()).collect::<String>();
    assert!(text.contains("Routing:") || text.contains("guild="), "Metadata footer missing: {}", text);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tylluan_remember_stores_node() {
    let state = test_state().await;
    let app = build_test_app(state.clone());
    let res = mcp_call(app, "tools/call", "tylluan_remember", serde_json::json!({
        "content": "Rust ownership rules",
        "importance": 0.9
    })).await;
    assert!(res["error"].is_null(), "Expected no error in remember: {:?}", res);
    
    // Verify node_count > 0 using SilvaDB directly
    let count = state.silva.node_count().await.unwrap();
    assert!(count > 0, "Node should be stored in SilvaDB");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tylluan_remember_requires_content() {
    let state = test_state().await;
    let app = build_test_app(state);
    let res = mcp_call(app, "tools/call", "tylluan_remember", serde_json::json!({})).await;
    assert!(!res["error"].is_null() || res["result"]["isError"].as_bool() == Some(true) || res["result"]["content"][0]["text"].as_str().unwrap_or("").to_lowercase().contains("missing"), "Expected error: {:?}", res);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tylluan_recall_returns_results() {
    let state = test_state().await;
    let app = build_test_app(state.clone());
    
    // Remember first
    let _ = mcp_call(app.clone(), "tools/call", "tylluan_remember", serde_json::json!({
        "content": "test knowledge node",
        "importance": 0.8
    })).await;
    
    // Recall
    let res = mcp_call(app, "tools/call", "tylluan_recall", serde_json::json!({"query": "test knowledge"})).await;
    assert!(res["error"].is_null(), "Expected no error in recall: {:?}", res);
    let content = res["result"]["content"].as_array().unwrap();
    let text = content.iter().filter_map(|c| c["text"].as_str()).collect::<String>();
    assert!(!text.is_empty(), "Recall should return non-empty text");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tylluan_recall_empty_query_handled() {
    let state = test_state().await;
    let app = build_test_app(state);
    let res = mcp_call(app, "tools/call", "tylluan_recall", serde_json::json!({"query": ""})).await;
    assert!(!res.is_null());
}

#[tokio::test(flavor = "multi_thread")]
#[cfg(feature = "integration")]
async fn test_all_three_tools_sequential() {
    let state = test_state().await;
    let app = build_test_app(state);
    
    let res1 = mcp_call(app.clone(), "tools/call", "tylluan_remember", serde_json::json!({"content": "Python decorators explained", "importance": 0.7})).await;
    assert!(res1["error"].is_null(), "Error in remember: {:?}", res1);
    
    let res2 = mcp_call(app.clone(), "tools/call", "tylluan_do", serde_json::json!({"intent": "explain what we know about Python"})).await;
    assert!(res2["error"].is_null(), "Error in do: {:?}", res2);
    
    let res3 = mcp_call(app, "tools/call", "tylluan_recall", serde_json::json!({"query": "Python decorators"})).await;
    assert!(res3["error"].is_null(), "Error in recall: {:?}", res3);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_sovereign_tools_only() {
    let state = test_state().await;
    let app = build_test_app(state);
    
    // tools/list doesn't take 'name' or 'arguments' params, so we send empty params
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {}
    });
    let req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let res: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    
    let tools = res["result"]["tools"].as_array().expect("Should have tools array");
    let mut names: Vec<String> = tools.iter().map(|t| t["name"].as_str().unwrap().to_string()).collect();
    names.sort();
    
    let expected = vec!["tylluan_do", "tylluan_graph", "tylluan_recall", "tylluan_remember", "tylluan_think"];
    assert_eq!(names, expected, "Sovereign invariant violated: exact 5 tools required");
}
