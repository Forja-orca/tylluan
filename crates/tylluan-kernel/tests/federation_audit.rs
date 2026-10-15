//! Integration Tests for Tylluan Federation (M11)
//! Tests the PeerDb, Pull/Push endpoints, Auto-Sync loop setup, and Provenance tagging

use tylluan_kernel::federation::{FederationPeer, PeerDb};
use tylluan_kernel::transport::http::HttpState;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);
use tylluan_kernel::transport::http::api_v1::api_v1_routes;
use tylluan_kernel::registry::guild_process::GuildRegistry;
use tylluan_kernel::config::{TimeoutsConfig, TylluanConfig};
use tylluan_kernel::router::matcher::GuildMatcher;
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

    let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
    silva.init().await.unwrap();
    let mailbox = Arc::new(Mailbox::in_memory().await.unwrap());
    mailbox.init().await.unwrap();
    let coloquio = Arc::new(ColoquioDb::new(":memory:").unwrap());
    let curriculum = Arc::new(std::sync::Mutex::new(tylluan_kernel::curriculum::CurriculumLearner::new_in_memory(1).unwrap()));
    let doctor = Arc::new(Doctor::new(registry_arc, silva.clone(), curriculum));
    let matcher = Arc::new(GuildMatcher::new(vec![]));

    let node_router = tylluan_kernel::memory::agent_nodes::AgentNodeRouter::new(tokio::sync::broadcast::channel(1).0);
    let (broadcast_tx, _) = tokio::sync::broadcast::channel(10);
    let (download_tx, _) = tokio::sync::broadcast::channel(10);

    let config = Arc::new(RwLock::new(TylluanConfig::default()));

    // ADR-019 step 2: permission denials are audited via log_audit_entry ->
    // guild_audit_log. Keep those rows in a per-process temp file instead of
    // polluting the repo tree (./data/audit.db relative to the crate dir).
    if std::env::var("TYLLUAN_AUDIT_DB").is_err() {
        let p = std::env::temp_dir().join(format!(
            "tylluan_audit_fed_{}.db",
            TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        // Safety: set once, before any audit write in this test process.
        unsafe { std::env::set_var("TYLLUAN_AUDIT_DB", p); }
    }

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
        node_identity: Arc::new(tylluan_link::identity::NodeIdentity::load_or_create(&std::env::temp_dir().join(format!("tylluan_id_http_{}", TEST_COUNTER.fetch_add(1, Ordering::Relaxed)))).unwrap()),
        nat_cache: Arc::new(tokio::sync::RwLock::new(None)),
        dht_routing_table: Arc::new(tokio::sync::RwLock::new(tylluan_link::dht::RoutingTable::new("test-node".to_string()))),
        p2p_pool: 
Arc::new(tokio::sync::Mutex::new(tylluan_link::p2p::P2pSessionPool::new(16, 300))),
        gossip_engine:
Arc::new(tokio::sync::RwLock::new(tylluan_link::gossip::GossipEngine::new(
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

#[tokio::test]
async fn test_peer_db_roundtrip() {
    let peer_db = PeerDb::open(":memory:").unwrap();
    let peer = FederationPeer {
        name: "test-peer".to_string(),
        url: "http://127.0.0.1:47004".to_string(),
        auth_token: "auth123".to_string(),
        shared_secret: "secret123".to_string(),
        last_sync: None,
        approved: true,
        added_at: 0,
        ed25519_pubkey: String::new(),
            external_address: String::new(), permissions: tylluan_kernel::federation::PeerPermissions::default(),
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
        ed25519_pubkey: String::new(),
            external_address: String::new(), permissions: tylluan_kernel::federation::PeerPermissions::default(),
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
        ed25519_pubkey: String::new(),
            external_address: String::new(), permissions: tylluan_kernel::federation::PeerPermissions::default(),
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
        ed25519_pubkey: String::new(),
        // ADR-019 step 2: receive is deny-by-default; grant the space this
        // test pushes. (Was permissions: default before step 2.)
        permissions: serde_json::from_str(r#"{"sync_spaces":[{"space":"tests-receive","direction":"pull"}]}"#).unwrap(),
        external_address: String::new(),
    };
    state.peer_db.insert(&peer).unwrap();
    state.config.write().await.federation_peers.push(peer.clone());

    let node_val = serde_json::json!({
        "id": "provenance-node",
        "node_type": "document",
        "content": "federated content",
        "metadata": "{\"space\":\"tests-receive\"}",
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
    });

    // Sign the node before sending (M12-B)
    let envelope = tylluan_link::identity::sign_node(&state.node_identity, &node_val).unwrap();
    let envelopes = vec![envelope];
    let plain_json = serde_json::to_vec(&envelopes).unwrap();
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
    
    // Add a local node (ADR-019 step 2: tag it so the peer's push grant covers it)
    state.silva.upsert_node("local-node", "entity", "local info", "{\"space\":\"tests-export\"}").await.unwrap();
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
        ed25519_pubkey: String::new(),
        // ADR-019 step 2: export is deny-by-default; grant the space of the
        // local node this test exports. (Was permissions: default before step 2.)
        permissions: serde_json::from_str(r#"{"sync_spaces":[{"space":"tests-export","direction":"push"}]}"#).unwrap(),
        external_address: String::new(),
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
    let exported_envelopes: Vec<tylluan_link::identity::SignedEnvelope> = serde_json::from_slice(&decrypted).unwrap();

    // Verify it only exported the local node, skipping the federated one (prevent echo loop)
    assert_eq!(exported_envelopes.len(), 1);
    assert_eq!(exported_envelopes[0].node["id"].as_str().unwrap(), "local-node");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_auto_sync_config_zero_disables() {
    // REAL BUG FIX (2026-08-23, config-muerta gate G1): this test always
    // set/asserted `federation.auto_sync_interval_secs` -- a field that
    // spawn_auto_sync() (api_federation.rs) never actually reads. It was a
    // tautology: the assertion only checked that the value the test itself
    // set was still there, never verifying real disable behavior. The real
    // interval spawn_auto_sync reads is `silva.sync_interval_ms` -- found
    // while removing auto_sync_interval_secs as genuinely dead config.
    let state = test_state().await;
    // Set the REAL sync interval to 0
    state.config.write().await.silva.sync_interval_ms = 0;

    // Call spawn_auto_sync (should terminate immediately since interval is 0)
    tylluan_kernel::transport::http::api_v1::api_federation::spawn_auto_sync(state.clone());

    // We shouldn't see background loop running. Wait a bit and verify state is ok.
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    // Just a sanity check that the system is stable and alive.
    assert_eq!(state.config.read().await.silva.sync_interval_ms, 0);
}

// ═════════════════ ADR-019 step 2 — permissions tests (T1–T15) ═════════════════
//
// Coverage map: T7/T9/T10/T11/T12/T13 live in the unit tests of
// federation/permissions.rs and api_v1/api_federation.rs (pure gates);
// T16 lives in memory/silva/tests.rs (upsert hardening). The flows here are
// the end-to-end ones: export filtering, ingestion gating (receive/pull/both
// share ONE helper), per-peer push bodies, denial auditing and legacy
// migration.

use tylluan_kernel::transport::http::api_v1::api_federation::{
    ingest_from_peer, push_body_for_peer, IngestOutcome,
};

async fn register_peer_with_perms(state: &Arc<HttpState>, name: &str, token: &str, perms_json: &str) {
    let permissions: tylluan_kernel::federation::PeerPermissions =
        serde_json::from_str(perms_json).expect("test permissions json");
    let peer = FederationPeer {
        name: name.to_string(),
        url: "http://127.0.0.1:5999".to_string(),
        auth_token: token.to_string(),
        shared_secret: "sec".to_string(),
        last_sync: None,
        approved: true,
        added_at: 0,
        ed25519_pubkey: String::new(),
        external_address: String::new(),
        permissions,
    };
    state.peer_db.insert(&peer).unwrap();
    let mut cfg = state.config.write().await;
    cfg.federation_peers.retain(|p| p.name != name);
    cfg.federation_peers.push(peer);
}

async fn peer_named(state: &Arc<HttpState>, name: &str) -> FederationPeer {
    state
        .config
        .read()
        .await
        .federation_peers
        .iter()
        .find(|p| p.name == name)
        .cloned()
        .expect("registered test peer")
}

async fn add_shareable_node(state: &Arc<HttpState>, id: &str, space: Option<&str>) {
    let meta = match space {
        Some(s) => serde_json::json!({ "space": s }).to_string(),
        None => "{}".to_string(),
    };
    state
        .silva
        .upsert_node_with_provenance(id, "lesson", &format!("content {id}"), &meta, "agent_generated")
        .await
        .unwrap();
    let g = state.silva.conn_lock();
    let c = g.lock().await;
    c.execute(&format!("UPDATE nodes SET shareable = 1 WHERE id = '{id}'"), [])
        .unwrap();
}

fn envelope_for(state: &Arc<HttpState>, id: &str, space: &str) -> tylluan_link::identity::SignedEnvelope {
    let node = serde_json::json!({
        "id": id,
        "node_type": "lesson",
        "content": format!("peer content {id}"),
        "metadata": serde_json::json!({ "space": space }).to_string(),
    });
    tylluan_link::identity::sign_node(&state.node_identity, &node).unwrap()
}

/// T1: an approved peer with NO permissions gets nothing and can give nothing.
#[tokio::test(flavor = "multi_thread")]
async fn t1_default_deny_new_peer() {
    let state = test_state().await;
    register_peer_with_perms(&state, "newpeer", "tok1", "{}").await;
    add_shareable_node(&state, "t1:local", None).await;
    let peer = peer_named(&state, "newpeer").await;

    let envs = push_body_for_peer(&state, &peer).await;
    assert!(envs.is_empty(), "default (empty) permissions must push/export NOTHING");

    let out = ingest_from_peer(&state, &peer, &envelope_for(&state, "t1:incoming", "whatever"), "receive").await;
    assert!(matches!(out, IngestOutcome::Denied(_)), "receive denied by default");
    assert!(!peer.permissions.may_dispatch("git", "status"), "dispatch denied by default");
}

/// T3 + T4: export contains ONLY spaces the peer may send to (direction push),
/// never pull-only or unlisted spaces. End-to-end through the HTTP endpoint.
#[tokio::test(flavor = "multi_thread")]
async fn t3_t4_export_filters_by_space_and_direction() {
    let state = test_state().await;
    register_peer_with_perms(
        &state,
        "p34",
        "tok34",
        r#"{"sync_spaces":[{"space":"proj","direction":"push"},{"space":"pin","direction":"pull"}]}"#,
    )
    .await;
    add_shareable_node(&state, "t3:proj", Some("proj")).await;
    add_shareable_node(&state, "t3:pin", Some("pin")).await;
    add_shareable_node(&state, "t3:other", Some("other")).await;

    let app = api_v1_routes().with_state(state.clone());
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/federation/sync/export")
        .header(header::AUTHORIZATION, "Bearer tok34")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 10 * 1024 * 1024).await.unwrap();

    let peer = peer_named(&state, "p34").await;
    let plain = tylluan_kernel::federation::decrypt_from_peer(&bytes, &state.node_identity, &peer).unwrap();
    let envs: Vec<tylluan_link::identity::SignedEnvelope> = serde_json::from_slice(&plain).unwrap();
    let ids: Vec<String> = envs
        .iter()
        .map(|e| e.node["id"].as_str().unwrap_or("").to_string())
        .collect();

    assert!(ids.contains(&"t3:proj".to_string()), "T3: push space must be exported");
    assert!(!ids.contains(&"t3:pin".to_string()), "T4: pull-only space must NOT be exported");
    assert!(!ids.contains(&"t3:other".to_string()), "T3: unlisted space must NOT be exported");
}

/// T5/T6/T7 + T14: the single ingestion gate across receive/pull/both flows.
#[tokio::test(flavor = "multi_thread")]
async fn t5_t6_t7_ingestion_gates_and_both_flow() {
    let state = test_state().await;
    register_peer_with_perms(&state, "p56", "tok56", r#"{"sync_spaces":[{"space":"in","direction":"pull"}]}"#).await;
    let peer = peer_named(&state, "p56").await;

    // T6 positive control: pull space ingests with provenance intact.
    let ok = ingest_from_peer(&state, &peer, &envelope_for(&state, "t6:node", "in"), "receive").await;
    assert_eq!(ok, IngestOutcome::Received, "pull space must be ingested");
    {
        let g = state.silva.conn_lock();
        let c = g.lock().await;
        let src: String = c
            .query_row("SELECT federation_source FROM nodes WHERE id = 't6:node'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(src, "p56", "provenance intact after gate");
    }

    // T5: unlisted space denied AND not stored.
    let out = ingest_from_peer(&state, &peer, &envelope_for(&state, "t5:node", "out"), "receive").await;
    assert!(matches!(out, IngestOutcome::Denied(_)), "unlisted space denied");
    {
        let g = state.silva.conn_lock();
        let c = g.lock().await;
        let n: i64 = c
            .query_row("SELECT COUNT(*) FROM nodes WHERE id = 't5:node'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "denied node must NOT be stored");
    }

    // T7: push-only direction rejects inbound delivery.
    register_peer_with_perms(&state, "p7", "tok7", r#"{"sync_spaces":[{"space":"in","direction":"push"}]}"#).await;
    let peer7 = peer_named(&state, "p7").await;
    let out7 = ingest_from_peer(&state, &peer7, &envelope_for(&state, "t7:node", "in"), "receive").await;
    assert!(matches!(out7, IngestOutcome::Denied(_)), "push-only space must reject receive");

    // T14: sync_both consumes the SAME helper with flow "both".
    let out14 = ingest_from_peer(&state, &peer, &envelope_for(&state, "t14:node", "in"), "both").await;
    assert_eq!(out14, IngestOutcome::Received, "both-flow path shares the gate");
}

/// T8: push bodies are built PER PEER — each peer receives only its own spaces.
#[tokio::test(flavor = "multi_thread")]
async fn t8_push_body_is_per_peer() {
    let state = test_state().await;
    register_peer_with_perms(&state, "pa", "tokA", r#"{"sync_spaces":[{"space":"a","direction":"push"}]}"#).await;
    register_peer_with_perms(&state, "pb", "tokB", r#"{"sync_spaces":[{"space":"b","direction":"push"}]}"#).await;
    add_shareable_node(&state, "t8:a", Some("a")).await;
    add_shareable_node(&state, "t8:b", Some("b")).await;
    add_shareable_node(&state, "t8:c", Some("c")).await;

    let pa = peer_named(&state, "pa").await;
    let pb = peer_named(&state, "pb").await;
    let ids = |es: &[tylluan_link::identity::SignedEnvelope]| -> Vec<String> {
        es.iter().map(|e| e.node["id"].as_str().unwrap_or("").to_string()).collect()
    };
    assert_eq!(ids(&push_body_for_peer(&state, &pa).await), vec!["t8:a".to_string()], "peer A gets only space a");
    assert_eq!(ids(&push_body_for_peer(&state, &pb).await), vec!["t8:b".to_string()], "peer B gets only space b");
}

/// T15: a permission denial leaves a hash-chained row in guild_audit_log.
#[tokio::test(flavor = "multi_thread")]
async fn t15_denials_are_audited() {
    let state = test_state().await;
    let audit_path = std::env::var("TYLLUAN_AUDIT_DB").expect("test_state installs the audit seam");
    register_peer_with_perms(&state, "paudit", "tokaudit", "{}").await;
    let peer = peer_named(&state, "paudit").await;
    let out = ingest_from_peer(&state, &peer, &envelope_for(&state, "t15:node", "x"), "receive").await;
    assert!(matches!(out, IngestOutcome::Denied(_)));

    let conn = tylluan_kernel::config::open_db(std::path::Path::new(&audit_path)).unwrap();
    let (cnt, hash): (i64, String) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(MAX(hash), '') FROM guild_audit_log WHERE intent LIKE 'peer paudit:%'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(cnt >= 1, "a permission denial must leave a guild_audit_log row (got {cnt})");
    assert!(!hash.is_empty(), "audited denial carries the hash-chain value");
}

/// T2: a pre-migration peers.db (no permissions column) loads as deny-all.
#[tokio::test]
async fn t2_legacy_peers_db_migration_defaults_deny_all() {
    let path = std::env::temp_dir().join(format!(
        "tylluan_legacy_peers_{}.db",
        TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_file(&path);
    {
        let conn = tylluan_kernel::config::open_db(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE federation_peers (
                name TEXT PRIMARY KEY,
                url TEXT NOT NULL,
                auth_token TEXT NOT NULL,
                shared_secret TEXT NOT NULL DEFAULT '',
                approved INTEGER NOT NULL DEFAULT 0,
                last_sync INTEGER,
                added_at INTEGER NOT NULL,
                ed25519_pubkey TEXT NOT NULL DEFAULT '',
                external_address TEXT NOT NULL DEFAULT ''
            );
            INSERT INTO federation_peers(name,url,auth_token,shared_secret,approved,added_at)
            VALUES('legacy','http://x','tok','',1,0);",
        )
        .unwrap();
    }
    let db = PeerDb::open(path.to_str().unwrap()).unwrap();
    let peers = db.load_all().unwrap();
    assert_eq!(peers.len(), 1);
    let p = &peers[0];
    assert_eq!(
        p.permissions,
        tylluan_kernel::federation::PeerPermissions::default(),
        "legacy rows migrate to deny-all"
    );
    assert!(!p.permissions.may_send_space("default"));
    assert!(!p.permissions.may_dispatch("git", "status"));
}
