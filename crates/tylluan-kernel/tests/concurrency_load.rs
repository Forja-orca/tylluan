//! M40-P7: real concurrency suite — N parallel agents exercising the full
//! kernel stack (HTTP router -> fractal gate -> deterministic routing ->
//! kernel subtool -> journal -> sessions) simultaneously, NOT isolated DB
//! writes like concurrent_agents.rs. Each agent uses a unique agent_id and
//! verifies its own cross-client continuity (P5 resume context) under load.
//!
//! Contention exercised on purpose:
//!   - POST /api/v1/do (tylluan_do whoami)       -> record_tool_call (journal + friction audit.db)
//!   - POST /api/v1/do (tylluan_remember)        -> silva + hybrid memory + journal
//!   - POST /api/v1/do (tylluan_recall)          -> hybrid search + recall cache + journal
//!   - GET  /api/v1/sessions/resume              -> build_resume_context under load
//!   - POST /api/v1/sessions/resume              -> sessions RwLock under contention

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
    // Replicate real kernel startup (main.rs:869) — build_agent_bootstrap
    // (bootstrap.rs:36) calls grants::list_pending() which panics with
    // "GrantRegistry not initialized" unless grants::init() ran first.
    tylluan_kernel::security::grants::init();
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
    let mut server = TylluanServer::new(
        registry_arc, matcher.clone(), memory.clone(), silva.clone(),
        mailbox.clone(), doctor.clone(), node_router.clone(),
    );
    // Replicate production wiring (main.rs:902-907): the same JournalDb must be
    // reachable from BOTH HttpState (resume context) and TylluanServer
    // (auto-checkin on every handle_kernel_tool call, handlers.rs:29).
    let journal = Arc::new(tylluan_kernel::transport::http::api_v1::api_journal::JournalDb::open(":memory:").unwrap());
    server.journal = Some(journal.clone());
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
        config: tylluan_kernel::config::TylluanConfig::load_cached().unwrap_or_else(|_| Arc::new(RwLock::new(tylluan_kernel::config::TylluanConfig::default()))),
        matcher,
        tunnel_wsl_url: None,
        oauth: Arc::new(tylluan_kernel::transport::http::oauth::OAuthState::new("http://localhost:3030".to_string())),
        metrics_ring: Arc::new(RwLock::new(tylluan_kernel::metrics_ring::MetricsRingBuffer::new())),
        jobs: Arc::new(tylluan_kernel::memory::jobs::JobQueue::open(std::path::Path::new(":memory:")).unwrap()),
        agents_contract: Arc::new(tylluan_kernel::security::agents_contract::AgentsContract::empty()),
        cancel_token: tokio_util::sync::CancellationToken::new(),
        node_router,
        health_ready: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        journal,
        agent_registry: tylluan_kernel::transport::http::api_v1::api_agents::AgentRegistry::new(7200),
        contract_registry: tylluan_kernel::transport::http::api_v1::api_contracts::ContractRegistry::new(),
        contract_db: Arc::new(tylluan_kernel::transport::http::api_v1::api_contracts::ContractDb::open(":memory:").unwrap()),
        peer_db: Arc::new(tylluan_kernel::federation::PeerDb::open(":memory:").unwrap()),
        node_identity: Arc::new(tylluan_link::identity::NodeIdentity::load_or_create(&std::env::temp_dir().join(format!("tylluan_id_cl_{}", TEST_COUNTER.fetch_add(1, Ordering::Relaxed)))).unwrap()),
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
        a2a_task_manager: Arc::new(tylluan_kernel::transport::http::a2a::A2aTaskManager::new(silva)),
    })
}

fn build_test_app(state: Arc<HttpState>) -> axum::Router {
    axum::Router::new()
        .merge(api_v1_routes())
        .with_state(state)
}

async fn post_json(app: &axum::Router, uri: &str, body: serde_json::Value) -> (StatusCode, serde_json::Value) {
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_else(|_| serde_json::json!({"raw": String::from_utf8_lossy(&bytes).to_string()}));
    (status, json)
}

async fn get_json(app: &axum::Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_else(|_| serde_json::json!({"raw": String::from_utf8_lossy(&bytes).to_string()}));
    (status, json)
}

const NUM_AGENTS: usize = 8;
const ITERATIONS: usize = 3;

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_n_parallel_agents_full_stack_no_races() {
    let state = test_state().await;
    let app = build_test_app(state);

    let barrier = Arc::new(tokio::sync::Barrier::new(NUM_AGENTS));

    let mut handles = Vec::new();
    for i in 0..NUM_AGENTS {
        let app = app.clone();
        let barrier = barrier.clone();
        handles.push(tokio::spawn(async move {
            let agent_id = format!("load-agent-{i}");
            barrier.wait().await;
            let mut last_intents = Vec::new();
            let mut last_statuses = Vec::new();

            for iter in 0..ITERATIONS {
                // 1) tylluan_do — deterministic kernel subtool (identity)
                let (status, body) = post_json(&app, "/api/v1/do", serde_json::json!({
                    "intent": "whoami",
                    "agent_id": agent_id,
                    "session_id": agent_id,
                })).await;
                last_statuses.push((format!("do-whoami-{iter}"), status, body.clone()));
                assert_eq!(status, StatusCode::OK, "agent {agent_id} whoami: {body}");
                assert_ne!(body.get("status").and_then(|v| v.as_str()), Some("ambiguous"),
                    "agent {agent_id} whoami should route deterministically, got ambiguous: {body}");

                // 2) tylluan_remember — real memory write (silva + hybrid)
                let (status, body) = post_json(&app, "/api/v1/do", serde_json::json!({
                    "tool": "tylluan_remember",
                    "content": format!("[load-agent-{i}] iteration {iter} finding: concurrent memory write is safe"),
                    "agent_id": agent_id,
                    "session_id": agent_id,
                })).await;
                last_statuses.push((format!("do-remember-{iter}"), status, body.clone()));
                assert_eq!(status, StatusCode::OK, "agent {agent_id} remember: {body}");
                let content_arr = body.get("content").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                let text = content_arr.iter().filter_map(|c| c.as_str()).collect::<String>();
                assert!(text.contains("Stored node"),
                    "agent {agent_id} remember should store a node: {body}");

                // 3) tylluan_recall — hybrid search under load
                let (status, body) = post_json(&app, "/api/v1/do", serde_json::json!({
                    "tool": "tylluan_recall",
                    "query": format!("concurrent memory write agent {i}"),
                    "limit": 5,
                    "agent_id": agent_id,
                    "session_id": agent_id,
                })).await;
                last_statuses.push((format!("do-recall-{iter}"), status, body.clone()));
                assert_eq!(status, StatusCode::OK, "agent {agent_id} recall: {body}");

                // 4) explicit journal checkin (real endpoint, api_v1.rs:622) —
                //    last writer wins, so the resume below must see THIS task
                let (status, body) = post_json(&app, &format!("/api/v1/journal/{agent_id}/checkin"), serde_json::json!({
                    "task": format!("[load-agent-{i}] completed iteration {iter}"),
                })).await;
                last_statuses.push((format!("checkin-{iter}"), status, body.clone()));
                assert_eq!(status, StatusCode::OK, "agent {agent_id} checkin: {body}");

                // 5) cross-client resume — each agent must see ITS OWN last task
                let (status, body) = get_json(&app, &format!("/api/v1/sessions/resume?agent_id={agent_id}")).await;
                last_statuses.push((format!("resume-{iter}"), status, body.clone()));
                assert_eq!(status, StatusCode::OK, "agent {agent_id} resume: {body}");
                if let Some(intent) = body.get("last_task").and_then(|t| t.get("task")).and_then(|v| v.as_str()) {
                    last_intents.push(intent.to_string());
                }

                // 6) POST resume — sessions RwLock under contention
                let (status, body) = post_json(&app, "/api/v1/sessions/resume", serde_json::json!({
                    "session_id": agent_id,
                    "agent_id": agent_id,
                    "client_name": format!("load-client-{i}"),
                })).await;
                last_statuses.push((format!("post-resume-{iter}"), status, body.clone()));
                assert_eq!(status, StatusCode::OK, "agent {agent_id} post-resume: {body}");
                assert_eq!(body.get("success").and_then(|v| v.as_bool()), Some(true),
                    "agent {agent_id} post-resume success: {body}");
            }

            (agent_id, last_intents, last_statuses)
        }));
    }

    let mut errors = Vec::new();
    for h in handles {
        let (agent_id, last_intents, statuses) = h.await.unwrap();
        // Every request must have been 200
        for (label, status, body) in &statuses {
            if *status != StatusCode::OK {
                errors.push(format!("{agent_id} {label}: {status} {body}"));
            }
        }
        // Cross-agent isolation: each agent's resume must return ITS OWN last
        // checkin task (journal is keyed per agent_id — no cross-talk allowed)
        for intent in &last_intents {
            assert!(intent.starts_with("[load-agent-"),
                "agent {agent_id} saw another agent's or empty last_task: {intent:?}");
        }
        assert_eq!(last_intents.len(), ITERATIONS, "agent {agent_id} should see its last task each iteration, got: {last_intents:?}");
    }
    assert!(errors.is_empty(), "non-200 responses under concurrency:\n{}", errors.join("\n"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_parallel_resume_isolation_per_agent() {
    let state = test_state().await;
    let app = build_test_app(state);

    // Interleave tool calls so each agent's journal gets distinct last_task
    let mut handles = Vec::new();
    for i in 0..4 {
        let app = app.clone();
        handles.push(tokio::spawn(async move {
            let agent_id = format!("iso-agent-{i}");
            for iter in 0..3 {
                let _ = post_json(&app, "/api/v1/do", serde_json::json!({
                    "intent": "whoami",
                    "agent_id": agent_id,
                    "session_id": agent_id,
                })).await;
                let _ = post_json(&app, "/api/v1/do", serde_json::json!({
                    "tool": "tylluan_remember",
                    "content": format!("[iso-agent-{i}] unique finding number {iter}"),
                    "agent_id": agent_id,
                    "session_id": agent_id,
                })).await;
                let _ = post_json(&app, &format!("/api/v1/journal/{agent_id}/checkin"), serde_json::json!({
                    "task": format!("[iso-agent-{i}] wrapped up iteration {iter}"),
                })).await;
            }
            let (status, body) = get_json(&app, &format!("/api/v1/sessions/resume?agent_id={agent_id}")).await;
            (agent_id, status, body)
        }));
    }

    let mut seen_agents = std::collections::HashSet::new();
    for h in handles {
        let (agent_id, status, body) = h.await.unwrap();
        assert_eq!(status, StatusCode::OK, "{agent_id}: {body}");
        assert!(seen_agents.insert(agent_id.clone()), "duplicate agent in results");
        let task = body.get("last_task").and_then(|t| t.get("task")).and_then(|v| v.as_str()).unwrap_or("");
        // journal keeps the last checkin per agent_id — must be this agent's own
        assert!(task.starts_with("[iso-agent-"),
            "{agent_id} resume leaked another agent's or empty task: {body}");
        assert!(task.contains(&agent_id),
            "{agent_id} resume leaked another agent's task: {task}");
    }
    assert_eq!(seen_agents.len(), 4, "expected exactly 4 isolated agents");
}
