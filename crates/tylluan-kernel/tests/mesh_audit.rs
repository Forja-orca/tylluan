//! Integration Tests for Tylluan Mesh Identity (M12-A)
//! Tests Ed25519 keypair generation, persistence, signature verification,
//! and the /api/v1/federation/identity endpoint.

use tylluan_link::identity::NodeIdentity;
use std::path::PathBuf;

fn tmp_identity_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("tylluan_mesh_audit_{}_{}.key", tag, std::process::id()))
}

// ─── Unit-level identity tests ───────────────────────────────────────────────

#[test]
fn test_identity_generate_and_load_roundtrip() {
    let path = tmp_identity_path("roundtrip");
    let _ = std::fs::remove_file(&path);

    let id1 = NodeIdentity::load_or_create(&path).expect("should generate on first call");
    assert_eq!(id1.public_key_hex().len(), 64);
    assert!(id1.public_key_hex().chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(id1.node_id().len(), 32);

    // Second call must load — not regenerate
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
    assert!(result.is_err(), "corrupted identity.key must return Err — never silently regenerate");

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

// ─── HTTP endpoint test ───────────────────────────────────────────────────────

use tylluan_kernel::transport::http::HttpState;
use tylluan_kernel::transport::http::api_v1::api_v1_routes;
use tylluan_kernel::registry::guild_process::GuildRegistry;
use tylluan_kernel::config::{TimeoutsConfig, TylluanConfig};
use tylluan_kernel::router::matcher::GuildMatcher;
use tylluan_kernel::memory::hybrid::HybridMemory;
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

    let memory = Arc::new(HybridMemory::in_memory().await.unwrap());
    let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
    silva.init().await.unwrap();
    let mailbox = Arc::new(Mailbox::in_memory().await.unwrap());
    mailbox.init().await.unwrap();
    let coloquio = Arc::new(ColoquioDb::new(":memory:").unwrap());
    let curriculum = Arc::new(std::sync::Mutex::new(
        tylluan_kernel::curriculum::CurriculumLearner::new_in_memory(1).unwrap(),
    ));
    let doctor = Arc::new(Doctor::new(registry_arc, memory.clone(), silva.clone(), curriculum));
    let matcher = Arc::new(GuildMatcher::new(vec![]));
    let node_router = tylluan_kernel::memory::agent_nodes::AgentNodeRouter::new(tokio::sync::broadcast::channel(1).0);
    let (broadcast_tx, _) = tokio::sync::broadcast::channel(10);
    let (download_tx, _) = tokio::sync::broadcast::channel(10);
    let config = Arc::new(RwLock::new(TylluanConfig::default()));

    // Use a temp file so load_or_create works on Windows too
    let identity_path = tmp_identity_path("httptest");
    let _ = std::fs::remove_file(&identity_path);
    let node_identity = Arc::new(NodeIdentity::load_or_create(&identity_path).unwrap());

    Arc::new(HttpState {
        version: "test".to_string(),
        auth_token: None,
        dev_mode: Some(true),
        start_time: Instant::now(),
        server: None,
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
        config,
        matcher,
        tunnel_wsl_url: None,
        oauth: Arc::new(tylluan_kernel::transport::http::oauth::OAuthState::new("http://localhost:3000".to_string())),
        metrics_ring: Arc::new(RwLock::new(tylluan_kernel::metrics_ring::MetricsRingBuffer::new())),
        jobs: Arc::new(tylluan_kernel::memory::jobs::JobQueue::open(std::path::Path::new(":memory:")).unwrap()),
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
    })
}

// ─── mDNS Discovery test ─────────────────────────────────────────────────────

#[test]
fn test_mdns_discovery_startup_does_not_panic() {
    let mut config = TylluanConfig::default();
    config.mdns.discover = true;
    let config_arc = Arc::new(RwLock::new(config));

    // start_mdns_discovery spawns a thread; if the mDNS daemon fails to bind
    // (UDP multicast unavailable in CI/test), it logs an error and returns —
    // it never panics.
    tylluan_kernel::transport::mdns::start_mdns_discovery(0, config_arc, None);

    // Give the thread a moment to attempt daemon creation
    std::thread::sleep(std::time::Duration::from_millis(500));
    // If we reach here without panic, the test passes.
}

// ─── NatConfig accessibility test ─────────────────────────────────────────────

#[test]
fn test_nat_config_is_accessible_from_tylluan_config() {
    let config = TylluanConfig::default();
    // Verify NatConfig exists and has default values via TylluanConfig.nat
    assert!(!config.nat.stun_servers.is_empty(), "stun_servers should have defaults");
    let _ = config.nat.stun_timeout_secs;
    let _ = config.nat.stun_retries;
}

// ─── HTTP endpoint test ───────────────────────────────────────────────────────

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
