//! Pipeline Integration Tests — cross-tool flows and invariant checks
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
    tokio::spawn(async move {
        registry_actor.run().await;
    });
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
        node_identity: Arc::new(tylluan_link::identity::NodeIdentity::load_or_create(&std::env::temp_dir().join(format!("tylluan_id_pipe_{}", TEST_COUNTER.fetch_add(1, Ordering::Relaxed)))).unwrap()),
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
        .uri("/mcp")
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

fn extract_text(res: &serde_json::Value) -> String {
    res["result"]["content"]
        .as_array()
        .map(|arr| arr.iter()
            .filter_map(|c| c["text"].as_str())
            .collect::<String>())
        .unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread")]
async fn test_remember_then_recall_finds_node() {
    let state = test_state().await;
    let app = build_test_app(state);

    let res1 = mcp_call(app.clone(), "tools/call", "tylluan_remember",
        json!({"content": "Rust lifetime rules prevent dangling pointers", "importance": 0.9})).await;
    assert!(res1["error"].is_null(), "remember failed: {:?}", res1);

    let res2 = mcp_call(app, "tools/call", "tylluan_recall",
        json!({"query": "Rust lifetime"})).await;
    let text = extract_text(&res2);
    assert!(text.contains("Rust") || text.contains("lifetime"),
        "recall no encontró el nodo: {}", text);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_remember_echoes_node_id() {
    let state = test_state().await;
    let app = build_test_app(state);

    let res = mcp_call(app, "tools/call", "tylluan_remember",
        json!({"content": "test content for echo", "importance": 0.5})).await;
    let text = extract_text(&res);
    assert!(text.contains("node_") || text.contains("Stored"),
        "remember no devuelve node_id: {}", text);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_think_finds_remembered_nodes() {
    let state = test_state().await;
    let app = build_test_app(state.clone());

    let _ = mcp_call(app.clone(), "tools/call", "tylluan_remember",
        json!({"content": "Python generators are lazy iterators that yield values on demand",
               "importance": 0.85})).await;

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let res = mcp_call(app, "tools/call", "tylluan_think",
        json!({"query": "Python generators"})).await;
    let text = extract_text(&res);
    assert!(!text.contains("No encontré conocimiento previo"),
        "BUG-01 REGRESION: tylluan_think no encontró nodos existentes. text={}", text);
    assert!(text.contains("Python") || text.contains("generator") || text.contains("lazy"),
        "tylluan_think no usa el contenido correcto: {}", text);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_recall_inbox_prefix() {
    let state = test_state().await;
    let app = build_test_app(state.clone());

    state.mailbox.send_mail("agent-x", "agent-test",
        r#"{"type":"task","body":"analyze the codebase"}"#).await.unwrap();

    let res = mcp_call(app, "tools/call", "tylluan_recall",
        json!({"query": "@inbox", "agent_id": "agent-test"})).await;
    let text = extract_text(&res);
    assert!(text.contains("agent-x") || text.contains("analyze") || text.contains("task"),
        "recall @inbox no devuelve mensajes: {}", text);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_graph_add_and_retrieve() {
    let state = test_state().await;
    let app = build_test_app(state.clone());

    let _ = mcp_call(app.clone(), "tools/call", "tylluan_graph",
        json!({"command": "add_triple", "subject": "Rust", "predicate": "is_a",
               "object": "language", "agent_id": "test"})).await;

    let res = mcp_call(app, "tools/call", "tylluan_graph",
        json!({"command": "list_neighbors", "entity": "Rust", "agent_id": "test"})).await;
    let text = extract_text(&res);
    assert!(text.contains("language") || text.contains("Rust"),
        "graph neighbors no encontrado: {}", text);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_sovereign_tools_exactly_5() {
    let state = test_state().await;
    let app = build_test_app(state);

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
    assert_eq!(names,
        vec!["tylluan_do", "tylluan_graph", "tylluan_recall", "tylluan_remember", "tylluan_think"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_think_shows_stigmergy_heat() {
    let state = test_state().await;
    let app = build_test_app(state.clone());

    // Insert a node directly into silva so it has traces
    let node_id = format!("stigmergy_test_{}", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());
    state.silva.upsert_node(&node_id, "concept", "Stigmergy concept for heat test",
        r#"{"source":"test","confidence":1.0}"#).await.unwrap();

    // Touch the node 3 times so it accumulates traces
    for _ in 0..3 {
        state.silva.touch_node(&node_id, "agent-heat-test", "stigmergy_seed").await.unwrap();
    }
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let res = mcp_call(app, "tools/call", "tylluan_think",
        json!({"query": "Stigmergy concept", "agent_id": "agent-stig-test"})).await;
    let text = extract_text(&res);
    assert!(text.contains("accesos"),
        "stigmergy heat: esperado 'accesos' en resultado, got: {}", text);
}