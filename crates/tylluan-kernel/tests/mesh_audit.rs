//! Integration Tests for Tylluan Mesh Identity (M12-A)
//! Tests Ed25519 keypair generation, persistence, signature verification,
//! and the /api/v1/federation/identity endpoint.

use tylluan_link::identity::NodeIdentity;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn tmp_identity_path(tag: &str) -> PathBuf {
    let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("tylluan_mesh_{tag}_{id}.key"))
}

// â”€â”€â”€ Unit-level identity tests â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn test_identity_generate_and_load_roundtrip() {
    let path = tmp_identity_path("roundtrip");
    let _ = std::fs::remove_file(&path);

    let id1 = NodeIdentity::load_or_create(&path).expect("should generate on first call");
    assert_eq!(id1.public_key_hex().len(), 64);
    assert!(id1.public_key_hex().chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(id1.node_id().len(), 32);

    // Second call must load â€” not regenerate
    let id2 = NodeIdentity::load_or_create(&path).expect("should load on second call");
    assert_eq!(id1.public_key_hex(), id2.public_key_hex(), "public key must be stable across loads");
    assert_eq!(id1.node_id(), id2.node_id(), "node_id must be stable across loads");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_identity_file_is_68_bytes() {
    let path = tmp_identity_path("filesize");
    let _ = std::fs::remove_file(&path);

    NodeIdentity::load_or_create(&path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(bytes.len(), 68, "identity file must be exactly 68 bytes (4 magic + 32 secret + 32 public)");
    assert_eq!(&bytes[..4], b"TLID", "file must start with TLID magic");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_identity_sign_and_verify() {
    let path = tmp_identity_path("sign");
    let _ = std::fs::remove_file(&path);

    let identity = NodeIdentity::load_or_create(&path).unwrap();
    let message = b"tylluan:federation:handshake:v1";
    let sig = identity.sign(message);

    // Valid signature verifies
    {
        use ed25519_dalek::Verifier;
        identity.verifying_key().verify(message, &sig)
            .expect("valid signature must verify against own public key");

        // Tampered message must not verify
        let bad_result = identity.verifying_key().verify(b"tampered", &sig);
        assert!(bad_result.is_err(), "signature must not verify against different message");
    }

    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_identity_different_nodes_have_different_keypairs() {
    let path_a = tmp_identity_path("nodeA");
    let path_b = tmp_identity_path("nodeB");
    let _ = std::fs::remove_file(&path_a);
    let _ = std::fs::remove_file(&path_b);

    let id_a = NodeIdentity::load_or_create(&path_a).unwrap();
    let id_b = NodeIdentity::load_or_create(&path_b).unwrap();

    assert_ne!(id_a.public_key_hex(), id_b.public_key_hex(), "two independent nodes must have unique keypairs");
    assert_ne!(id_a.node_id(), id_b.node_id());

    let _ = std::fs::remove_file(&path_a);
    let _ = std::fs::remove_file(&path_b);
}

#[test]
fn test_corrupted_identity_file_returns_error() {
    let path = tmp_identity_path("corrupt");
    std::fs::write(&path, b"not a tylluan identity file").unwrap();

    let result = NodeIdentity::load_or_create(&path);
    assert!(result.is_err(), "corrupted identity.key must return Err â€” never silently regenerate");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_wrong_magic_header_returns_error() {
    let path = tmp_identity_path("wrongmagic");
    // 68 bytes but wrong magic
    let mut buf = vec![0u8; 68];
    buf[..4].copy_from_slice(b"XXXX");
    std::fs::write(&path, &buf).unwrap();

    let result = NodeIdentity::load_or_create(&path);
    assert!(result.is_err(), "wrong magic header must return Err");

    let _ = std::fs::remove_file(&path);
}

// â”€â”€â”€ HTTP endpoint test â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

use tylluan_kernel::transport::http::HttpState;
use tylluan_kernel::transport::http::api_v1::api_v1_routes;
use tylluan_kernel::registry::guild_process::GuildRegistry;
use tylluan_kernel::config::{TimeoutsConfig, TylluanConfig};
use tylluan_kernel::router::matcher::GuildMatcher;
use tylluan_kernel::memory::silva::SilvaDB;
use tylluan_kernel::memory::mailbox::Mailbox;
use tylluan_kernel::memory::coloquio::ColoquioDb;
use tylluan_kernel::doctor::Doctor;
use tylluan_kernel::registry::actor::RegistryActor;
use tylluan_kernel::federation::PeerDb;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;
use std::collections::HashMap;
use std::time::Instant;

async fn mesh_test_state() -> Arc<HttpState> {
    let workspace_root = std::env::current_dir().unwrap_or_default();
    let registry_raw = GuildRegistry::new(workspace_root, 5, TimeoutsConfig::default(), 5);
    let registry_arc = Arc::new(RwLock::new(registry_raw));
    let (registry_actor, registry_handle) = RegistryActor::new(registry_arc.clone());
    tokio::spawn(async move { registry_actor.run().await; });

    let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
    silva.init().await.unwrap();
    let mailbox = Arc::new(Mailbox::in_memory().await.unwrap());
    mailbox.init().await.unwrap();
    let coloquio = Arc::new(ColoquioDb::new(":memory:").unwrap());
    let curriculum = Arc::new(std::sync::Mutex::new(
        tylluan_kernel::curriculum::CurriculumLearner::new_in_memory(1).unwrap(),
    ));
    let doctor = Arc::new(Doctor::new(registry_arc, silva.clone(), curriculum));
    let matcher = Arc::new(GuildMatcher::new(vec![]));
    let node_router = tylluan_kernel::memory::agent_nodes::AgentNodeRouter::new(tokio::sync::broadcast::channel(1).0);
    let (broadcast_tx, _) = tokio::sync::broadcast::channel(10);
    let (download_tx, _) = tokio::sync::broadcast::channel(10);
    let config = Arc::new(RwLock::new(TylluanConfig::default()));

    // Use a temp file so load_or_create works on Windows too
    let identity_path = tmp_identity_path("httptest");
    let _ = std::fs::remove_file(&identity_path);
    let node_identity = Arc::new(NodeIdentity::load_or_create(&identity_path).unwrap());

    // Build default repo map for test
    let cwd = std::env::current_dir().unwrap_or_default();
    let repo_map = tylluan_kernel::repo_map::RepoMap::build(&cwd);
    Arc::new(HttpState {
        task_context: Arc::new(tylluan_kernel::memory::task_context::TaskContextStore::in_memory().expect("test task_context store")),
        version: "test".to_string(),
        auth_token: None,
        dev_mode: Some(true),
        start_time: Instant::now(),
        server: None,
        registry: registry_handle,
        doctor,
        silva: silva.clone(),
        mailbox,
        coloquio,
        broadcast_tx,
        download_progress_tx: download_tx,
        sessions: Arc::new(RwLock::new(HashMap::new())),
        guild_status_cache: Arc::new(std::sync::Mutex::new(None)),
        agent_rate_limiter: Arc::new(dashmap::DashMap::new()),
        ip_rate_limiter: Arc::new(tylluan_kernel::security::rate_limiter::RateLimiter::new(Some(300))),
        config,
        matcher,
        tunnel_wsl_url: None,
        oauth: Arc::new(tylluan_kernel::transport::http::oauth::OAuthState::new("http://localhost:3000".to_string())),
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
        peer_db: Arc::new(PeerDb::open(":memory:").unwrap()),
        node_identity,
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
        repo_map,
        a2a_task_manager: Arc::new(tylluan_kernel::transport::http::a2a::A2aTaskManager::new(silva.clone())),
        a2a_agents: Arc::new(tylluan_kernel::transport::http::a2a_client::A2aAgentStore::new(silva.clone())),
        a2a_client: Arc::new(tylluan_kernel::transport::http::a2a_client::A2aClient::new().unwrap()),
    })
}

// â”€â”€â”€ mDNS Discovery test â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn test_mdns_discovery_startup_does_not_panic() {
    let mut config = TylluanConfig::default();
    config.mdns.discover = true;
    let config_arc = Arc::new(RwLock::new(config));

    // start_mdns_discovery spawns a thread; if the mDNS daemon fails to bind
    // (UDP multicast unavailable in CI/test), it logs an error and returns â€”
    // it never panics.
    tylluan_kernel::transport::mdns::start_mdns_discovery(0, config_arc, None);

    // Give the thread a moment to attempt daemon creation
    std::thread::sleep(std::time::Duration::from_millis(500));
    // If we reach here without panic, the test passes.
}

// â”€â”€â”€ NatConfig accessibility test â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn test_nat_config_is_accessible_from_tylluan_config() {
    let config = TylluanConfig::default();
    // Verify NatConfig exists and has default values via TylluanConfig.nat
    assert!(!config.nat.stun_servers.is_empty(), "stun_servers should have defaults");
    let _ = config.nat.stun_timeout_secs;
    let _ = config.nat.stun_retries;
}

// â”€â”€â”€ HTTP endpoint test â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[tokio::test(flavor = "multi_thread")]
async fn test_federation_identity_endpoint() {
    let state = mesh_test_state().await;
    let expected_pubkey = state.node_identity.public_key_hex().to_string();
    let expected_node_id = state.node_identity.node_id().to_string();

    let app = api_v1_routes().with_state(state);

    let request = Request::builder()
        .uri("/api/v1/federation/identity")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["public_key"].as_str().unwrap(), expected_pubkey);
    assert_eq!(json["node_id"].as_str().unwrap(), expected_node_id);
    assert!(json["tylluan_version"].as_str().is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_nat_external_address_http_endpoint() {
    let state = mesh_test_state().await;

    // Prime the nat_cache with a known value
    {
        let mut cache = state.nat_cache.write().await;
        *cache = Some(tylluan_link::nat::ExternalAddr {
            ip: "203.0.113.42".parse().unwrap(),
            port: 12345,
            stun_server: "test-stun:3478".to_string(),
        });
    }

    let app = api_v1_routes().with_state(state);

    // Test with cache hit
    let request = Request::builder()
        .uri("/api/v1/nat/external-address")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["external_ip"], "203.0.113.42");
    assert_eq!(json["external_port"], 12345);
    assert_eq!(json["stun_server"], "test-stun:3478");
    assert_eq!(json["cached"], true);
}

// â”€â”€â”€ M21-P4: P2P kernel-level DST test â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
//
// tylluan-link/tests/p2p_dst.rs already proves the Noise XK protocol works
// between two real listeners with a stub handler. What was still untested:
// the kernel's own guild_dispatch_remote_handler (POST
// /api/v1/guilds/dispatch/remote) actually reaching DispatchDecision::RemoteTcp
// and calling execute_remote_tcp with the real p2p_pool from HttpState --
// i.e. the kernel wiring found already-complete during the M14-F Phase 3 doc
// audit (2026-07-12), now backed by a real integration test instead of just
// a manual code read.
//
// mesh_test_state() builds capability_registry and dispatch_router with two
// SEPARATE CapabilityRegistry instances (matches how existing tests in this
// file use it) -- for this test we need them to share one registry, the way
// main.rs actually wires them (transport/http/mod.rs:379-386, verified:
// `DispatchRouter::new(capability_registry.clone(), ...)`), so a peer
// injected into the registry is visible to the router's routing decision.

async fn dst_test_state(
    shared_registry: Arc<std::sync::Mutex<tylluan_link::capability::CapabilityRegistry>>,
    node_identity: Arc<NodeIdentity>,
) -> Arc<HttpState> {
    let workspace_root = std::env::current_dir().unwrap_or_default();
    let registry_raw = GuildRegistry::new(workspace_root, 5, TimeoutsConfig::default(), 5);
    let registry_arc = Arc::new(RwLock::new(registry_raw));
    let (registry_actor, registry_handle) = RegistryActor::new(registry_arc.clone());
    tokio::spawn(async move { registry_actor.run().await; });

    let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
    silva.init().await.unwrap();
    let mailbox = Arc::new(Mailbox::in_memory().await.unwrap());
    mailbox.init().await.unwrap();
    let coloquio = Arc::new(ColoquioDb::new(":memory:").unwrap());
    let curriculum = Arc::new(std::sync::Mutex::new(
        tylluan_kernel::curriculum::CurriculumLearner::new_in_memory(1).unwrap(),
    ));
    let doctor = Arc::new(Doctor::new(registry_arc, silva.clone(), curriculum));
    let matcher = Arc::new(GuildMatcher::new(vec![]));
    let node_router = tylluan_kernel::memory::agent_nodes::AgentNodeRouter::new(tokio::sync::broadcast::channel(1).0);
    let (broadcast_tx, _) = tokio::sync::broadcast::channel(10);
    let (download_tx, _) = tokio::sync::broadcast::channel(10);
    let config = Arc::new(RwLock::new(TylluanConfig::default()));

    // Build default repo map for test
    let cwd = std::env::current_dir().unwrap_or_default();
    let repo_map = tylluan_kernel::repo_map::RepoMap::build(&cwd);
    Arc::new(HttpState {
        task_context: Arc::new(tylluan_kernel::memory::task_context::TaskContextStore::in_memory().expect("test task_context store")),
        version: "test".to_string(),
        auth_token: None,
        dev_mode: Some(true),
        start_time: Instant::now(),
        server: None,
        registry: registry_handle,
        doctor,
        silva: silva.clone(),
        mailbox,
        coloquio,
        broadcast_tx,
        download_progress_tx: download_tx,
        sessions: Arc::new(RwLock::new(HashMap::new())),
        guild_status_cache: Arc::new(std::sync::Mutex::new(None)),
        agent_rate_limiter: Arc::new(dashmap::DashMap::new()),
        ip_rate_limiter: Arc::new(tylluan_kernel::security::rate_limiter::RateLimiter::new(Some(300))),
        config,
        matcher,
        tunnel_wsl_url: None,
        oauth: Arc::new(tylluan_kernel::transport::http::oauth::OAuthState::new("http://localhost:3000".to_string())),
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
        peer_db: Arc::new(PeerDb::open(":memory:").unwrap()),
        node_identity,
        nat_cache: Arc::new(tokio::sync::RwLock::new(None)),
        dht_routing_table: Arc::new(tokio::sync::RwLock::new(tylluan_link::dht::RoutingTable::new("test-node".to_string()))),
        p2p_pool: Arc::new(tokio::sync::Mutex::new(tylluan_link::p2p::P2pSessionPool::new(16, 300))),
        gossip_engine: Arc::new(tokio::sync::RwLock::new(tylluan_link::gossip::GossipEngine::new(
            "test-node".to_string(),
            tylluan_link::gossip::GossipConfig::default(),
        ))),
        capability_registry: shared_registry.clone(),
        dispatch_router: Arc::new(std::sync::Mutex::new(tylluan_link::dispatch::DispatchRouter::new(
            shared_registry,
            std::time::Duration::from_secs(60),
        ))),
        dispatch_queue: Arc::new(std::sync::Mutex::new(tylluan_link::dispatch::DispatchQueue::new(1000))),
        repo_map,
        a2a_task_manager: Arc::new(tylluan_kernel::transport::http::a2a::A2aTaskManager::new(silva.clone())),
        a2a_agents: Arc::new(tylluan_kernel::transport::http::a2a_client::A2aAgentStore::new(silva.clone())),
        a2a_client: Arc::new(tylluan_kernel::transport::http::a2a_client::A2aClient::new().unwrap()),
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn test_kernel_remote_dispatch_routes_via_real_noise_xk_p2p() {
    use tylluan_link::gossip::HardwareCaps;
    use tylluan_link::dispatch::{GuildDispatchRequest, GuildDispatchResponse};
    use tylluan_link::p2p::start_p2p_listener_noise;

    // "Remote" node: a real Noise XK listener with a stub guild handler,
    // exactly like tylluan-link/tests/p2p_dst.rs's own roundtrip test --
    // the difference here is the *initiator* is a full kernel HttpState
    // reached over a real HTTP request, not a direct execute_remote_tcp call.
    let tmp_dir = std::env::temp_dir().join("tylluan_kernel_p2p_dst_test");
    let _ = std::fs::create_dir_all(&tmp_dir);
    let remote_identity = Arc::new(NodeIdentity::load_or_create(&tmp_dir.join("remote.json")).unwrap());
    let remote_pubkey = remote_identity.public_key_hex().to_string();

    let handler: tylluan_link::p2p::P2pHandlerFn = Arc::new(|req: GuildDispatchRequest, _caller_x25519: String| {
        Box::pin(async move {
            GuildDispatchResponse {
                request_id: req.request_id,
                success: true,
                result: serde_json::json!({"executed_by": "remote-stub", "args": req.args}),
                error: None,
                executor_id: "remote-stub".to_string(),
                duration_ms: 1,
            }
        })
    });

    let local_identity = Arc::new(NodeIdentity::load_or_create(&tmp_dir.join("local.json")).unwrap());
    let local_pubkey_hex = local_identity.public_key_hex().to_string();
    let approved_x25519: tylluan_link::p2p::ApprovedPeersFn = Arc::new(move || {
        tylluan_link::noise::x25519_from_ed25519_hex(&local_pubkey_hex)
            .map(|k| vec![k])
            .unwrap_or_default()
    });

    let (listener_handle, bound_addr) = start_p2p_listener_noise(
        "127.0.0.1:0".parse().unwrap(),
        remote_identity.clone(),
        handler,
        approved_x25519,
    ).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Local kernel state: register the remote node in the SAME registry the
    // dispatch_router consults, with supports_p2p=true and a guild capability
    // so route() picks DispatchDecision::RemoteTcp instead of Local.
    let shared_registry = Arc::new(std::sync::Mutex::new(
        tylluan_link::capability::CapabilityRegistry::new(std::time::Duration::from_secs(300))
    ));
    let hw = HardwareCaps {
        ram_mb: 8192,
        has_gpu: false,
        load_avg: 0.1,
        supports_p2p: true,
        tcp_port: Some(bound_addr.port()),
    };
    // node_id MUST be the remote's real public key hex: api_mesh.rs passes
    // DispatchDecision::RemoteTcp's node_id straight to execute_remote_tcp as
    // peer_pubkey_hex, which Noise XK uses to authenticate the responder.
    // An arbitrary label here would make the handshake fail against the real
    // listener (which really is identified by remote_identity's pubkey).
    shared_registry.lock().unwrap().ingest(
        &remote_pubkey, &bound_addr.to_string(), &hw, &["test-guild".to_string()], 1,
    );

    let state = dst_test_state(shared_registry, local_identity).await;
    let app = api_v1_routes().with_state(state);

    let body = serde_json::json!({
        "guild": "test-guild",
        "tool": "execute",
        "args": {"cmd": "echo hello"},
        "peer_addr": bound_addr.to_string(),
    });
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/guilds/dispatch/remote")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let resp_body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();

    assert_eq!(json["success"], true, "remote dispatch should succeed: {json}");
    let executor = json["executor"].as_str().unwrap_or("");
    assert!(
        executor.starts_with(&format!("p2p://{remote_pubkey}:")),
        "must route via the real p2p RemoteTcp arm, not local execution: executor={executor}"
    );
    assert_eq!(json["result"]["executed_by"], "remote-stub", "response must come from the remote listener, not a local stub");

    listener_handle.abort();
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

// ─── T1277-C: P2P dispatch gate end-to-end (ADR-019 step 2 debt) ────────────
//
// ADR-019 declared the debt itself: "sin test end-to-end del dispatch P2P"
// (docs/reference/adr/ADR019_team_scoped_networks.md). The Noise listener
// test above proves the transport with a STUB handler — the production gate
// (dispatch_gate_decision over the responder's peers.db, plus the
// approval-checked handshake) was never driven end-to-end.
//
// These three tests wire the PRODUCTION closures
// (transport::http::build_p2p_dispatch_gate_handler /
// build_p2p_approved_peers — the exact code the listener spawn uses) into a
// real Noise XK listener on an ephemeral port, then drive it through
// POST /api/v1/guilds/dispatch/remote with a real peer identity. Production
// config is untouched: `[p2p] enabled` stays at its default (off); the
// listener is started directly, exactly like the M21-P4 test above.
//
// Topology mirrors production: the responder owns identity + peers.db +
// registry + gate; the initiator owns the HTTP entry point + p2p_pool. One
// test = one fresh state = one fresh session pool, so every request performs
// a fresh handshake (no pooled-session reuse across approval changes).

/// Point `TYLLUAN_AUDIT_DB` at a per-process temp file before the first
/// denial audit write, so the crate's real `./data/audit.db` is never touched
/// (same pattern as federation_audit.rs).
fn ensure_p2p_audit_env() {
    if std::env::var("TYLLUAN_AUDIT_DB").is_err() {
        let p = std::env::temp_dir().join(format!(
            "tylluan_audit_p2p_gate_{}.db",
            std::process::id()
        ));
        // Safety: set once, before any audit write in this test process.
        unsafe { std::env::set_var("TYLLUAN_AUDIT_DB", p); }
    }
}

/// `(result_preview, intent)` of audit rows written for `agent_id`.
/// Missing DB/table (nothing audited yet) => no rows.
fn p2p_audit_rows(agent: &str) -> Vec<(String, String)> {
    let Ok(path) = std::env::var("TYLLUAN_AUDIT_DB") else {
        return Vec::new();
    };
    let Ok(conn) = rusqlite::Connection::open(&path) else {
        return Vec::new();
    };
    let Ok(mut stmt) = conn.prepare(
        "SELECT result_preview, intent FROM guild_audit_log WHERE agent_id = ?1",
    ) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map(rusqlite::params![agent], |r| {
        Ok((
            r.get::<_, Option<String>>(0)
                .unwrap_or_default()
                .unwrap_or_default(),
            r.get::<_, Option<String>>(1)
                .unwrap_or_default()
                .unwrap_or_default(),
        ))
    }) else {
        return Vec::new();
    };
    rows.flatten().collect()
}

/// Count of denial rows whose peer could NOT be resolved by key (gate label
/// `x25519:<key>`): only possible if the handler ran for an unapproved caller.
fn p2p_audit_x25519_rows() -> usize {
    let Ok(path) = std::env::var("TYLLUAN_AUDIT_DB") else {
        return 0;
    };
    let Ok(conn) = rusqlite::Connection::open(&path) else {
        return 0;
    };
    conn.query_row(
        "SELECT COUNT(*) FROM guild_audit_log WHERE agent_id LIKE 'x25519:%'",
        [],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0) as usize
}

/// Responder-side peer row: the initiator identity as the responder's
/// peers.db knows it (Ed25519 pubkey + approval + ADR-019 permissions).
fn gate_peer_row(
    name: &str,
    initiator_pubkey: &str,
    perms_json: &str,
) -> tylluan_kernel::federation::FederationPeer {
    tylluan_kernel::federation::FederationPeer {
        name: name.to_string(),
        url: "http://127.0.0.1:0".to_string(),
        auth_token: "test-token".to_string(),
        shared_secret: String::new(),
        last_sync: None,
        approved: true,
        added_at: 0,
        ed25519_pubkey: initiator_pubkey.to_string(),
        external_address: String::new(),
        permissions: tylluan_kernel::federation::permissions::parse_permissions(perms_json),
    }
}

/// The responder half of the topology: identity + peers.db + registry actor,
/// wired through the PRODUCTION gate closures, listening on an ephemeral port.
async fn start_gate_listener(
    responder_identity: Arc<NodeIdentity>,
    peer_db: Arc<PeerDb>,
) -> (tokio::task::JoinHandle<()>, std::net::SocketAddr) {
    use tylluan_link::p2p::start_p2p_listener_noise;
    let registry_raw = GuildRegistry::new(
        std::env::current_dir().unwrap_or_default(),
        5,
        TimeoutsConfig::default(),
        5,
    );
    let registry_arc = Arc::new(RwLock::new(registry_raw));
    let (registry_actor, registry_handle) = RegistryActor::new(registry_arc);
    tokio::spawn(async move { registry_actor.run().await; });
    let handler = tylluan_kernel::transport::http::build_p2p_dispatch_gate_handler(
        registry_handle,
        peer_db.clone(),
    );
    let approved = tylluan_kernel::transport::http::build_p2p_approved_peers(peer_db);
    let (handle, bound) = start_p2p_listener_noise(
        "127.0.0.1:0".parse().unwrap(),
        responder_identity,
        handler,
        approved,
    )
    .await
    .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    (handle, bound)
}

/// Initiator kernel state whose dispatch_router routes `test-guild` to the
/// responder's bound address over RemoteTcp (same ingestion as the M21-P4
/// test: node_id MUST be the responder's real pubkey for Noise auth).
async fn gate_initiator_state(
    responder_pubkey: &str,
    bound: std::net::SocketAddr,
    local_identity: Arc<NodeIdentity>,
) -> Arc<HttpState> {
    use tylluan_link::gossip::HardwareCaps;
    let shared_registry = Arc::new(std::sync::Mutex::new(
        tylluan_link::capability::CapabilityRegistry::new(std::time::Duration::from_secs(300)),
    ));
    let hw = HardwareCaps {
        ram_mb: 8192,
        has_gpu: false,
        load_avg: 0.1,
        supports_p2p: true,
        tcp_port: Some(bound.port()),
    };
    shared_registry.lock().unwrap().ingest(
        responder_pubkey,
        &bound.to_string(),
        &hw,
        &["test-guild".to_string()],
        1,
    );
    dst_test_state(shared_registry, local_identity).await
}

/// One dispatch through the real router; status is asserted inside (these
/// endpoints answer 200 with success=false for both denial and failure).
async fn post_remote_dispatch(
    state: &Arc<HttpState>,
    bound: std::net::SocketAddr,
    tool: &str,
) -> serde_json::Value {
    let app = api_v1_routes().with_state(state.clone());
    let body = serde_json::json!({
        "guild": "test-guild",
        "tool": tool,
        "args": {"cmd": "echo hi"},
        "peer_addr": bound.to_string(),
    });
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/guilds/dispatch/remote")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let resp_body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
    serde_json::from_slice(&resp_body).unwrap()
}

/// T1277-C case 1 — approved peer, guild.tool IN the allow-list: the gate
/// lets the request through and the REAL registry answers ("Guild ... not
/// found": the test registry is empty; spawning real guilds in tests is out
/// of scope, same reasoning as audit_latency_endpoint_test.rs).
#[tokio::test(flavor = "multi_thread")]
async fn test_t1277_c1_gate_allows_listed_guild_tool() {
    ensure_p2p_audit_env();
    let local_path = tmp_identity_path("gate_allow_a");
    let responder_path = tmp_identity_path("gate_allow_r");
    let local_identity = Arc::new(NodeIdentity::load_or_create(&local_path).unwrap());
    let responder_identity = Arc::new(NodeIdentity::load_or_create(&responder_path).unwrap());
    let responder_pubkey = responder_identity.public_key_hex().to_string();
    let local_pubkey = local_identity.public_key_hex().to_string();

    let peer_db = Arc::new(PeerDb::open(":memory:").unwrap());
    peer_db
        .insert(&gate_peer_row(
            "remote-initiator-allow",
            &local_pubkey,
            r#"{"dispatch_allow":["test-guild.execute"]}"#,
        ))
        .unwrap();

    let (listener, bound) = start_gate_listener(responder_identity, peer_db).await;
    let state = gate_initiator_state(&responder_pubkey, bound, local_identity).await;
    let json = post_remote_dispatch(&state, bound, "execute").await;

    let err = json["error"].as_str().unwrap_or("");
    assert!(
        err.contains("Guild 'test-guild' not found"),
        "allowed request must reach the real registry past the gate: {json}"
    );
    assert!(
        !err.contains("tool not allowed"),
        "gate must allow the exact guild.tool entry: {json}"
    );
    assert_eq!(json["success"], false, "registry has no such guild: {json}");
    let executor = json["executor"].as_str().unwrap_or("");
    assert!(
        executor.starts_with(&format!("p2p://{responder_pubkey}:")),
        "must route via RemoteTcp, never local: executor={executor}"
    );
    assert!(
        p2p_audit_rows("remote-initiator-allow").is_empty(),
        "the allow path must not audit a denial"
    );

    listener.abort();
    let _ = std::fs::remove_file(local_path);
    let _ = std::fs::remove_file(responder_path);
}

/// T1277-C case 2 — approved peer, empty allow-list (deny-by-default): the
/// gate denies BEFORE the registry and audits the denial with the peer's name
/// (resolved Ed25519→X25519→row). Relies on the `error` field added by the
/// T1277-C api_mesh change; without it a policy denial was indistinguishable
/// from an execution failure at the HTTP layer.
#[tokio::test(flavor = "multi_thread")]
async fn test_t1277_c2_gate_denies_unlisted_tool_and_audits() {
    ensure_p2p_audit_env();
    let local_path = tmp_identity_path("gate_deny_a");
    let responder_path = tmp_identity_path("gate_deny_r");
    let local_identity = Arc::new(NodeIdentity::load_or_create(&local_path).unwrap());
    let responder_identity = Arc::new(NodeIdentity::load_or_create(&responder_path).unwrap());
    let responder_pubkey = responder_identity.public_key_hex().to_string();
    let local_pubkey = local_identity.public_key_hex().to_string();

    let peer_db = Arc::new(PeerDb::open(":memory:").unwrap());
    peer_db
        .insert(&gate_peer_row(
            "remote-initiator-deny",
            &local_pubkey,
            "{}", // deny-all: no dispatch_allow entries
        ))
        .unwrap();

    let (listener, bound) = start_gate_listener(responder_identity, peer_db).await;
    let state = gate_initiator_state(&responder_pubkey, bound, local_identity).await;
    let json = post_remote_dispatch(&state, bound, "execute").await;

    assert_eq!(json["success"], false, "{json}");
    let err = json["error"].as_str().unwrap_or("");
    assert_eq!(
        err, "tool not allowed by peer policy",
        "the gate's own denial must reach the requester: {json}"
    );
    assert!(
        !err.contains("not found"),
        "denied request must never reach the registry: {json}"
    );
    let executor = json["executor"].as_str().unwrap_or("");
    assert!(
        executor.starts_with(&format!("p2p://{responder_pubkey}:")),
        "must route via RemoteTcp: executor={executor}"
    );

    let rows = p2p_audit_rows("remote-initiator-deny");
    assert_eq!(rows.len(), 1, "exactly one denial audited: {rows:?}");
    assert!(
        rows[0].0.contains("tool not allowed by peer policy"),
        "audit preview: {}",
        rows[0].0
    );
    assert!(
        rows[0].1.contains("dispatch denied"),
        "audit intent: {}",
        rows[0].1
    );

    listener.abort();
    let _ = std::fs::remove_file(local_path);
    let _ = std::fs::remove_file(responder_path);
}

/// T1277-C case 3 — NO approved peer at all (fail-closed on empty set): the
/// listener's approval check rejects the connection BEFORE any request is
/// read, so neither the gate handler nor the registry ever runs. The Noise
/// layer may surface this as an encrypted `peer not approved` response or as
/// a transport error depending on socket timing — both mean pre-handler
/// rejection, and both must be visible to the requester.
#[tokio::test(flavor = "multi_thread")]
async fn test_t1277_c3_unapproved_peer_rejected_before_handler() {
    ensure_p2p_audit_env();
    let local_path = tmp_identity_path("gate_unappr_a");
    let responder_path = tmp_identity_path("gate_unappr_r");
    let local_identity = Arc::new(NodeIdentity::load_or_create(&local_path).unwrap());
    let responder_identity = Arc::new(NodeIdentity::load_or_create(&responder_path).unwrap());
    let responder_pubkey = responder_identity.public_key_hex().to_string();

    // Empty peers.db — fail-closed: nothing is approved.
    let peer_db = Arc::new(PeerDb::open(":memory:").unwrap());

    let (listener, bound) = start_gate_listener(responder_identity, peer_db).await;
    let state = gate_initiator_state(&responder_pubkey, bound, local_identity).await;
    let json = post_remote_dispatch(&state, bound, "execute").await;

    assert_eq!(json["success"], false, "{json}");
    let err = json["error"].as_str().unwrap_or("");
    assert!(
        err.contains("peer not approved") || err.contains("P2P dispatch failed"),
        "must be rejected by the approval check before the handler: {json}"
    );
    assert!(
        !err.contains("tool not allowed") && !err.contains("not found"),
        "gate/registry must never run for an unapproved peer: {json}"
    );
    assert_eq!(
        p2p_audit_x25519_rows(),
        0,
        "no gate denial may be audited — the handshake was rejected first"
    );

    listener.abort();
    let _ = std::fs::remove_file(local_path);
    let _ = std::fs::remove_file(responder_path);
}
