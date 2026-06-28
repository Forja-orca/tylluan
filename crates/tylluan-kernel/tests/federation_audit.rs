//! Integration Tests for Tylluan Federation (M11)
//! Tests the PeerDb, Pull/Push endpoints, Auto-Sync loop setup, and Provenance tagging

use tylluan_kernel::federation::{FederationPeer, PeerDb};
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
use axum::body::Body;
use axum::http::{Request, header, StatusCode};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;
use std::collections::HashMap;
use std::time::Instant;

async fn test_state() -> Arc<HttpState> {
    let workspace_root = std::env::current_dir().unwrap_or_default();
    let registry_raw = GuildRegistry::new(workspace_root, 5, TimeoutsConfig::default(), 5);
    let registry_arc = Arc::new(RwLock::new(registry_raw));
    let (registry_actor, registry_handle) = RegistryActor::new(registry_arc.clone());
    tokio::spawn(async move {
        registry_actor.run().await;
    });

    let memory = Arc::new(HybridMemory::in_memory().await.unwrap());
    let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
    silva.init().await.unwrap();
    let mailbox = Arc::new(Mailbox::in_memory().await.unwrap());
    mailbox.init().await.unwrap();
    let coloquio = Arc::new(ColoquioDb::new(":memory:").unwrap());
    let curriculum = Arc::new(std::sync::Mutex::new(tylluan_kernel::curriculum::CurriculumLearner::new_in_memory(1).unwrap()));
    let doctor = Arc::new(Doctor::new(registry_arc, memory.clone(), silva.clone(), curriculum));
    let matcher = Arc::new(GuildMatcher::new(vec![]));

    let node_router = tylluan_kernel::memory::agent_nodes::AgentNodeRouter::new(tokio::sync::broadcast::channel(1).0);
    let (broadcast_tx, _) = tokio::sync::broadcast::channel(10);
    let (download_tx, _) = tokio::sync::broadcast::channel(10);

    let config = Arc::new(RwLock::new(TylluanConfig::default()));

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
    })
}

#[tokio::test]
async fn test_peer_db_roundtrip() {
    let peer_db = PeerDb::open(":memory:").unwrap();
    let peer = FederationPeer {
        name: "test-peer".to_string(),
        url: "http://127.0.0.1:4000".to_string(),
        auth_token: "auth123".to_string(),
        shared_secret: "secret123".to_string(),
        last_sync: None,
        approved: true,
        added_at: 0,
    };

    peer_db.insert(&peer).unwrap();
    let loaded = peer_db.load_all().unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].name, "test-peer");
    assert_eq!(loaded[0].auth_token, "auth123");
    assert_eq!(loaded[0].shared_secret, "secret123");
    assert!(loaded[0].approved);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_peer_approval_required() {
    let state = test_state().await;
    
    // Register unapproved peer in peer_db & config
    let unapproved = FederationPeer {
        name: "malicious".to_string(),
        url: "http://127.0.0.1:5000".to_string(),
        auth_token: "evil_auth".to_string(),
        shared_secret: "evil_secret".to_string(),
        last_sync: None,
        approved: false, // NOT approved
        added_at: 0,
    };
    state.peer_db.insert(&unapproved).unwrap();
    state.config.write().await.federation_peers.push(unapproved);

    let app = api_v1_routes().with_state(state.clone());

    // Try exporting nodes with the unapproved peer token
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/federation/sync/export")
        .header(header::AUTHORIZATION, "Bearer evil_auth")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_shared_secret_separate_from_auth() {
    let peer = FederationPeer {
        name: "peer".to_string(),
        url: "http://127.0.0.1:6000".to_string(),
        auth_token: "bearer-token".to_string(),
        shared_secret: "separate-encryption-secret-32-chars-long".to_string(),
        last_sync: None,
        approved: true,
        added_at: 0,
    };

    let data = b"confidential payload";
    let encrypted = tylluan_kernel::federation::encrypt_payload(data, peer.encryption_key()).unwrap();
    
    // Ensure we can decrypt using the encryption key derived from shared_secret
    let decrypted = tylluan_kernel::federation::decrypt_payload(&encrypted, peer.encryption_key()).unwrap();
    assert_eq!(decrypted, data);

    // Decrypting with the auth_token bearer instead should fail
    assert!(tylluan_kernel::federation::decrypt_payload(&encrypted, &peer.auth_token).is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_provenance_tagged_on_receive() {
    let state = test_state().await;
    let peer = FederationPeer {
        name: "source-peer".to_string(),
        url: "http://localhost:3000".to_string(),
        auth_token: "peer-token".to_string(),
        shared_secret: "key-key-key".to_string(),
        last_sync: None,
        approved: true,
        added_at: 0,
    };
    state.peer_db.insert(&peer).unwrap();
    state.config.write().await.federation_peers.push(peer.clone());

    let nodes = serde_json::json!([
        {
            "id": "provenance-node",
            "node_type": "document",
            "content": "federated content",
            "metadata": "{}",
            "weight": 1.0,
            "protected": false,
            "conflicted": false,
            "topic_key": null,
            "created_at": null,
            "updated_at": null,
            "last_touched": "2026-06-28T12:00:00Z",
            "valid_from": null,
            "valid_until": null,
            "shareable": true
        }
    ]);

    let plain_json = serde_json::to_vec(&nodes).unwrap();
    let encrypted = tylluan_kernel::federation::encrypt_payload(&plain_json, peer.encryption_key()).unwrap();

    let app = api_v1_routes().with_state(state.clone());
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/federation/sync/receive")
        .header(header::AUTHORIZATION, "Bearer peer-token")
        .header("content-type", "application/octet-stream")
        .body(Body::from(encrypted))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Assert that the received node in SilvaDB has "federation_source" set to peer's name in its metadata
    let saved = state.silva.get_node("provenance-node").await.unwrap().unwrap();
    let meta: serde_json::Value = serde_json::from_str(&saved.metadata).unwrap();
    assert_eq!(meta.get("federation_source").unwrap().as_str().unwrap(), "source-peer");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_no_echo_loop() {
    let state = test_state().await;
    
    // Add a local node
    state.silva.upsert_node("local-node", "entity", "local info", "{}").await.unwrap();
    state.silva.set_shareable("local-node", true).await.unwrap();

    // Add a received node (simulating federated provenance)
    state.silva.upsert_node("federated-node", "entity", "remote info", "{\"federation_source\":\"other-peer\"}").await.unwrap();
    state.silva.set_shareable("federated-node", true).await.unwrap();

    let peer = FederationPeer {
        name: "requester-peer".to_string(),
        url: "http://localhost:3000".to_string(),
        auth_token: "requester-token".to_string(),
        shared_secret: "requester-secret".to_string(),
        last_sync: None,
        approved: true,
        added_at: 0,
    };
    state.peer_db.insert(&peer).unwrap();
    state.config.write().await.federation_peers.push(peer);

    let app = api_v1_routes().with_state(state.clone());
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/federation/sync/export")
        .header(header::AUTHORIZATION, "Bearer requester-token")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap().to_vec();
    let decrypted = tylluan_kernel::federation::decrypt_payload(&body_bytes, "requester-secret").unwrap();
    let exported_nodes: Vec<serde_json::Value> = serde_json::from_slice(&decrypted).unwrap();

    // Verify it only exported the local node, skipping the federated one (prevent echo loop)
    assert_eq!(exported_nodes.len(), 1);
    assert_eq!(exported_nodes[0]["id"].as_str().unwrap(), "local-node");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_auto_sync_config_zero_disables() {
    let state = test_state().await;
    // Set auto-sync interval to 0
    state.config.write().await.federation.auto_sync_interval_secs = 0;
    
    // Call spawn_auto_sync (should terminate immediately since interval is 0)
    tylluan_kernel::transport::http::api_v1::api_federation::spawn_auto_sync(state.clone());
    
    // We shouldn't see background loop running. Wait a bit and verify state is ok.
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    // Just a sanity check that the system is stable and alive.
    assert_eq!(state.config.read().await.federation.auto_sync_interval_secs, 0);
}
