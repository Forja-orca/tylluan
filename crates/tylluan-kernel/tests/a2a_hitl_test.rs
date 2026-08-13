//! Integration Test: A2A HITL (Human-In-The-Loop) Grant Flow
//!
//! Tests the grant flow end-to-end through the actual grants module:
//!   1. register → list_pending → resolve → task completes
//!   2. register → list_pending → remove → task fails
//!   3. JSON-RPC protocol: unknown method returns -32601
//!
//! Also verifies collision-safe UUIDs in grant IDs.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use std::collections::HashMap;

use tylluan_kernel::config::{TimeoutsConfig, TylluanConfig};
use tylluan_kernel::transport::http::HttpState;
use tylluan_kernel::transport::http::a2a;
use tylluan_kernel::transport::server::TylluanServer;
use tylluan_kernel::registry::guild_process::GuildRegistry;
use tylluan_kernel::registry::actor::RegistryActor;
use tylluan_kernel::router::matcher::GuildMatcher;
use tylluan_kernel::memory::hybrid::HybridMemory;
use tylluan_kernel::memory::silva::SilvaDB;
use tylluan_kernel::memory::mailbox::Mailbox;
use tylluan_kernel::memory::coloquio::ColoquioDb;
use tylluan_kernel::doctor::Doctor;
use tylluan_kernel::security::grants;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

// ── Unit tests for the grant system itself ─────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_grant_approval_flow() {
    grants::init();

    let (tx, rx) = tokio::sync::oneshot::channel::<grants::GrantLevel>();

    let grant_id = grants::register(grants::GrantRequest {
        guild: "bash".into(),
        tool_name: "bash_execute".into(),
        agent_id: "test-agent".into(),
        reason: "CAPABILITY_BLOCKED: bash in Strict profile".into(),
        arguments: serde_json::Map::new(),
        tx,
        expires_at: tokio::time::Instant::now() + Duration::from_secs(300),
    }).await;

    // Verify hex ID format (used for collision-safe short IDs)
    assert_eq!(grant_id.len(), 8, "Grant ID should be 8 hex chars, got: '{grant_id}'");
    assert!(grant_id.chars().all(|c| c.is_ascii_hexdigit()), "Grant ID must be hex");

    // Verify grant appears in list_pending
    let pending = grants::list_pending().await;
    let found = pending.iter().any(|g| {
        g.get("id").and_then(|v| v.as_str()) == Some(&grant_id)
            && g.get("agent_id").and_then(|v| v.as_str()) == Some("test-agent")
    });
    assert!(found, "Grant should appear in list_pending: {pending:?}");

    // Resolve the grant — ThisTime allows one execution
    let resolved = grants::resolve(&grant_id, grants::GrantLevel::ThisTime).await;
    assert!(resolved, "resolve() should return true");

    // Verify the oneshot receiver got the granted level
    let level = rx.await.expect("Receiver should get Ok(level)");
    assert_eq!(level, grants::GrantLevel::ThisTime, "Should receive ThisTime");

    // Verify grant removed from pending
    let pending = grants::list_pending().await;
    let still_pending = pending.iter().any(|g| {
        g.get("id").and_then(|v| v.as_str()) == Some(&grant_id)
    });
    assert!(!still_pending, "Grant should be removed after resolve");

    // Resolving a non-existent grant returns false
    let double = grants::resolve("g_nonexistent00000000000000000000000", grants::GrantLevel::ThisTime).await;
    assert!(!double, "Resolving unknown grant should return false");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_grant_rejection_flow() {
    grants::init();

    let (tx, rx) = tokio::sync::oneshot::channel::<grants::GrantLevel>();

    let grant_id = grants::register(grants::GrantRequest {
        guild: "vision".into(),
        tool_name: "vision_analyze".into(),
        agent_id: "reject-test-agent".into(),
        reason: "CAPABILITY_BLOCKED".into(),
        arguments: serde_json::Map::new(),
        tx,
        expires_at: tokio::time::Instant::now() + Duration::from_secs(300),
    }).await;

    // Verify grant appears
    let pending = grants::list_pending().await;
    assert!(pending.iter().any(|g| g.get("id").and_then(|v| v.as_str()) == Some(&grant_id)));

    // Remove the grant (simulates rejection — drops the oneshot sender)
    let removed = grants::remove(&grant_id).await;
    assert!(removed, "remove() should return true");

    // Verify the oneshot receiver gets Err(Canceled)
    let result = rx.await;
    assert!(result.is_err(), "Receiver should get Err(Canceled) after remove");

    // Verify grant gone from pending
    let pending = grants::list_pending().await;
    assert!(!pending.iter().any(|g| g.get("id").and_then(|v| v.as_str()) == Some(&grant_id)));

    // Removing a non-existent grant returns false
    let double = grants::remove("g_nonexistent00000000000000000000000").await;
    assert!(!double, "Removing unknown grant should return false");
}

// ── A2A Protocol Tests ─────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_a2a_unknown_method_returns_minus_32601() {
    let state = build_minimal_state().await;

    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tasks/subscribe",
        "params": {},
        "id": 42,
    });

    let response = a2a::a2a_jsonrpc_handler(
        axum::extract::State(state),
        axum::body::Bytes::from(serde_json::to_vec(&body).unwrap()),
    ).await;
    let body_bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    let resp: a2a::JsonRpcResponse = serde_json::from_slice(&body_bytes).unwrap();

    assert!(resp.result.is_none(), "Unknown method should not have a result");
    let err = resp.error.expect("Unknown method should return error");
    assert_eq!(err.code, -32601, "Should return Method not found error");
    assert_eq!(err.message, "Method not found");
}

// F3: message/stream responds with W3C SSE framing (text/event-stream) and
// each `data:` line is a JSON-RPC message. Empty intent => single JSON-RPC
// error event (-32602), then the stream closes.
#[tokio::test(flavor = "multi_thread")]
async fn test_message_stream_returns_sse_error_for_empty_intent() {
    let state = build_minimal_state().await;

    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "message/stream",
        "params": {},
        "id": 7,
    });

    let response = a2a::a2a_jsonrpc_handler(
        axum::extract::State(state),
        axum::body::Bytes::from(serde_json::to_vec(&body).unwrap()),
    ).await;

    let content_type = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        content_type.starts_with("text/event-stream"),
        "SSE content type, got: {content_type}"
    );

    let body_bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    let sse = String::from_utf8_lossy(&body_bytes);
    let data_line = sse.lines().find(|l| l.starts_with("data: ")).expect("SSE data line");
    let payload: serde_json::Value =
        serde_json::from_str(data_line.strip_prefix("data: ").unwrap()).unwrap();

    assert_eq!(payload["jsonrpc"], "2.0");
    assert_eq!(payload["id"], 7);
    assert_eq!(payload["error"]["code"], -32602);
    let msg = payload["error"]["message"].as_str().unwrap();
    assert!(
        msg.contains("intent") || msg.contains("message"),
        "error should mention the missing intent/message, got: {msg}"
    );
}

// F4: oversized JSON-RPC bodies are rejected with -32600 before any work.
#[tokio::test(flavor = "multi_thread")]
async fn test_jsonrpc_oversized_body_rejected() {
    let state = build_minimal_state().await;

    let big_text = "x".repeat(a2a::MAX_JSONRPC_BODY_BYTES + 200);
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "message/send",
        "params": { "intent": big_text },
        "id": 11,
    });

    let response = a2a::a2a_jsonrpc_handler(
        axum::extract::State(state),
        axum::body::Bytes::from(serde_json::to_vec(&body).unwrap()),
    ).await;
    let body_bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    let resp: a2a::JsonRpcResponse = serde_json::from_slice(&body_bytes).unwrap();

    let err = resp.error.expect("oversized body should return an error");
    assert_eq!(err.code, -32600, "should be an invalid request error");
    assert!(err.message.contains("too large"), "got: {}", err.message);
}

async fn build_minimal_state() -> Arc<HttpState> {
    grants::init();

    let workspace_root = std::env::current_dir().unwrap_or_default();
    let registry_raw = GuildRegistry::new(workspace_root.clone(), 5, TimeoutsConfig::default(), 5);
    let registry_arc = Arc::new(RwLock::new(registry_raw));
    let (_registry_actor, registry_handle) = RegistryActor::new(registry_arc.clone());
    // Don't spawn the actor to avoid guild startup issues

    let memory = Arc::new(HybridMemory::in_memory().await.unwrap());
    let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
    silva.init().await.unwrap();
    let mailbox = Arc::new(Mailbox::in_memory().await.unwrap());
    mailbox.init().await.unwrap();
    let coloquio = Arc::new(ColoquioDb::new(":memory:").unwrap());
    let curriculum = Arc::new(std::sync::Mutex::new(
        tylluan_kernel::curriculum::CurriculumLearner::new_in_memory(1).unwrap()
    ));
    let doctor = Arc::new(Doctor::new(registry_arc.clone(), memory.clone(), silva.clone(), curriculum));
    let matcher = Arc::new(GuildMatcher::new(
        tylluan_kernel::router::catalog::builtin_catalog()
    ));

    let (broadcast_tx, _) = tokio::sync::broadcast::channel(10);
    let (download_tx, _) = tokio::sync::broadcast::channel(10);
    let node_router = tylluan_kernel::memory::agent_nodes::AgentNodeRouter::new(
        tokio::sync::broadcast::channel(1).0
    );

    let server = TylluanServer::new(
        registry_arc.clone(),
        matcher.clone(),
        memory.clone(),
        silva.clone(),
        mailbox.clone(),
        doctor.clone(),
        node_router.clone(),
    );

    let mut config = TylluanConfig::default();
    config.security.capabilities_enforce = true;
    let config = Arc::new(RwLock::new(config));

    let cwd = std::env::current_dir().unwrap_or_default();
    let repo_map = tylluan_kernel::repo_map::RepoMap::build(&cwd);
    let a2a_task_manager = Arc::new(a2a::A2aTaskManager::new(silva.clone()));

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
        ip_rate_limiter: Arc::new(
            tylluan_kernel::security::rate_limiter::RateLimiter::new(Some(300))
        ),
        config,
        matcher,
        tunnel_wsl_url: None,
        oauth: Arc::new(
            tylluan_kernel::transport::http::oauth::OAuthState::new(
                "http://localhost:3030".to_string()
            )
        ),
        metrics_ring: Arc::new(RwLock::new(
            tylluan_kernel::metrics_ring::MetricsRingBuffer::new()
        )),
        jobs: Arc::new(
            tylluan_kernel::memory::jobs::JobQueue::open(std::path::Path::new(":memory:")).unwrap()
        ),
        agents_contract: Arc::new(tylluan_kernel::security::agents_contract::AgentsContract::empty()),
        cancel_token: tokio_util::sync::CancellationToken::new(),
        node_router,
        health_ready: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        journal: Arc::new(
            tylluan_kernel::transport::http::api_v1::api_journal::JournalDb::open(":memory:").unwrap()
        ),
        agent_registry: tylluan_kernel::transport::http::api_v1::api_agents::AgentRegistry::new(7200),
        contract_registry: tylluan_kernel::transport::http::api_v1::api_contracts::ContractRegistry::new(),
        contract_db: Arc::new(
            tylluan_kernel::transport::http::api_v1::api_contracts::ContractDb::open(":memory:").unwrap()
        ),
        peer_db: Arc::new(
            tylluan_kernel::federation::PeerDb::open(":memory:").unwrap()
        ),
        node_identity: Arc::new(
            tylluan_link::identity::NodeIdentity::load_or_create(
                &std::env::temp_dir().join(format!(
                    "tylluan_id_a2a_{}",
                    TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
                )),
            ).unwrap()
        ),
        nat_cache: Arc::new(tokio::sync::RwLock::new(None)),
        dht_routing_table: Arc::new(tokio::sync::RwLock::new(
            tylluan_link::dht::RoutingTable::new("test-node".to_string())
        )),
        p2p_pool: Arc::new(tokio::sync::Mutex::new(
            tylluan_link::p2p::P2pSessionPool::new(16, 300)
        )),
        gossip_engine: Arc::new(tokio::sync::RwLock::new(
            tylluan_link::gossip::GossipEngine::new(
                "test-node".to_string(),
                tylluan_link::gossip::GossipConfig::default(),
            )
        )),
        capability_registry: Arc::new(std::sync::Mutex::new(
            tylluan_link::capability::CapabilityRegistry::new(
                std::time::Duration::from_secs(300)
            )
        )),
        dispatch_router: Arc::new(std::sync::Mutex::new(
            tylluan_link::dispatch::DispatchRouter::new(
                Arc::new(std::sync::Mutex::new(
                    tylluan_link::capability::CapabilityRegistry::new(
                        std::time::Duration::from_secs(300)
                    )
                )),
                std::time::Duration::from_secs(60),
            )
        )),
        dispatch_queue: Arc::new(std::sync::Mutex::new(
            tylluan_link::dispatch::DispatchQueue::new(1000)
        )),
        repo_map,
        a2a_task_manager,
        a2a_agents: Arc::new(tylluan_kernel::transport::http::a2a_client::A2aAgentStore::new(silva.clone())),
        a2a_client: Arc::new(tylluan_kernel::transport::http::a2a_client::A2aClient::new().unwrap()),
    })
}
