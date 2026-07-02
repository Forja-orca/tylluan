use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tylluan_link::dispatch::{DispatchDecision, DispatchRouter, GuildDispatchRequest, GuildDispatchResponse};
use tylluan_link::capability::CapabilityRegistry;
use tylluan_link::gossip::HardwareCaps;
use tylluan_link::p2p::start_p2p_listener_noise;
use tylluan_link::identity::NodeIdentity;
use std::time::Duration;

fn make_registry() -> Arc<std::sync::Mutex<CapabilityRegistry>> {
    Arc::new(std::sync::Mutex::new(CapabilityRegistry::new(Duration::from_secs(300))))
}

fn inject_peer_with_p2p(registry: &Arc<std::sync::Mutex<CapabilityRegistry>>, node_id: &str, addr: &str, tcp_port: u16, capabilities: &[&str]) {
    let hw = HardwareCaps {
        ram_mb: 8192,
        has_gpu: false,
        load_avg: 0.1,
        supports_p2p: true,
        tcp_port: Some(tcp_port),
    };
    let caps: Vec<String> = capabilities.iter().map(|s| s.to_string()).collect();
    registry.lock().unwrap().ingest(node_id, addr, &hw, &caps, 1);
}

fn make_identity(tmp_dir: &std::path::Path, name: &str) -> NodeIdentity {
    let path = tmp_dir.join(format!("identity_{}.json", name));
    NodeIdentity::load_or_create(&path).unwrap()
}

/// Test: listener receives GuildDispatchRequest, handler returns GuildDispatchResponse, initiator verifies success
#[tokio::test]
async fn test_p2p_noise_roundtrip() {
    let tmp_dir = std::env::temp_dir().join("tylluan_p2p_test").join("roundtrip");
    let _ = std::fs::create_dir_all(&tmp_dir);
    
    let identity = make_identity(&tmp_dir, "server");
    let server_pubkey = identity.public_key_hex().to_string();
    let identity_arc = Arc::new(identity);

    // Handler stub: always returns success
    let handler: tylluan_link::p2p::P2pHandlerFn = Arc::new(|req: GuildDispatchRequest| {
        Box::pin(async move {
            GuildDispatchResponse {
                request_id: req.request_id,
                success: true,
                result: serde_json::json!({"echo": req.args}),
                error: None,
                executor_id: "test-listener".to_string(),
                duration_ms: 1,
            }
        })
    });

    // Start listener on port 0 (dynamic)
    let (handle, bound_addr) = start_p2p_listener_noise(
        "127.0.0.1:0".parse().unwrap(),
        identity_arc.clone(),
        handler,
    ).await.unwrap();

    // Give listener time to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Connect as client and send request
    let client_identity = make_identity(&tmp_dir, "client");
    let mut pool = tylluan_link::p2p::P2pSessionPool::new(4, 300);
    let request = GuildDispatchRequest {
        guild: "bash".to_string(),
        tool: "execute".to_string(),
        args: serde_json::json!({"cmd": "echo hello"}),
        request_id: "test-1".to_string(),
        sender_id: "client".to_string(),
        timeout_secs: Some(5),
    };

    let response = tylluan_link::p2p::execute_remote_tcp(
        &mut pool,
        request.clone(),
        bound_addr,
        &server_pubkey,
        &client_identity,
    ).await.unwrap();

    assert!(response.success);
    assert_eq!(response.request_id, "test-1");

    handle.abort();
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

/// Test: handler returns success=false, initiator receives error correctly
#[tokio::test]
async fn test_p2p_error_response() {
    let tmp_dir = std::env::temp_dir().join("tylluan_p2p_test").join("error");
    let _ = std::fs::create_dir_all(&tmp_dir);
    
    let identity = make_identity(&tmp_dir, "server");
    let server_pubkey = identity.public_key_hex().to_string();
    let identity_arc = Arc::new(identity);

    // Handler stub: always returns error
    let handler: tylluan_link::p2p::P2pHandlerFn = Arc::new(|req: GuildDispatchRequest| {
        Box::pin(async move {
            GuildDispatchResponse {
                request_id: req.request_id,
                success: false,
                result: serde_json::Value::Null,
                error: Some("simulated error".to_string()),
                executor_id: "test-listener".to_string(),
                duration_ms: 1,
            }
        })
    });

    // Start listener
    let (handle, bound_addr) = start_p2p_listener_noise(
        "127.0.0.1:0".parse().unwrap(),
        identity_arc.clone(),
        handler,
    ).await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Client request
    let client_identity = make_identity(&tmp_dir, "client2");
    let mut pool = tylluan_link::p2p::P2pSessionPool::new(4, 300);
    let request = GuildDispatchRequest {
        guild: "bash".to_string(),
        tool: "execute".to_string(),
        args: serde_json::json!({"cmd": "invalid"}),
        request_id: "test-err".to_string(),
        sender_id: "client".to_string(),
        timeout_secs: Some(5),
    };

    let response = tylluan_link::p2p::execute_remote_tcp(
        &mut pool,
        request.clone(),
        bound_addr,
        &server_pubkey,
        &client_identity,
    ).await.unwrap();

    assert!(!response.success);
    assert_eq!(response.error, Some("simulated error".to_string()));
    assert_eq!(response.request_id, "test-err");

    handle.abort();
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

/// Test: route() with peer.supports_p2p=true + tcp_port returns RemoteTcp
#[test]
fn test_route_prefers_tcp() {
    let registry = make_registry();
    inject_peer_with_p2p(&registry, "p2p-peer", "10.0.0.5:9000", 9001, &["bash"]);

    let router = DispatchRouter::new(registry, Duration::from_secs(60));
    router.record_latency("p2p-peer", 5.0);

    let local_caps = HardwareCaps {
        ram_mb: 4096,
        has_gpu: false,
        load_avg: 0.5,
        supports_p2p: false,
        tcp_port: None,
    };

    let decision = router.route("bash", &local_caps, 10.0);
    assert_eq!(
        decision,
        DispatchDecision::RemoteTcp {
            node_id: "p2p-peer".to_string(),
            addr: "10.0.0.5:9000".to_string(),
            tcp_port: 9001,
        },
        "route() should prefer RemoteTcp when peer supports P2P"
    );
}