use std::sync::{Arc, Mutex};
use std::time::Duration;
use tylluan_link::capability::CapabilityRegistry;
use tylluan_link::dispatch::{DispatchRouter, DispatchDecision, DispatchQueue};
use tylluan_link::gossip::HardwareCaps;

fn make_registry() -> Arc<Mutex<CapabilityRegistry>> {
    Arc::new(Mutex::new(CapabilityRegistry::new(Duration::from_secs(300))))
}

fn inject_peer(registry: &Arc<Mutex<CapabilityRegistry>>, node_id: &str, addr: &str, ram_mb: u32, has_gpu: bool, load_avg: f32, capabilities: &[&str]) {
    let hw = HardwareCaps { ram_mb, has_gpu, load_avg };
    let caps: Vec<String> = capabilities.iter().map(|s| s.to_string()).collect();
    registry.lock().unwrap().ingest(node_id, addr, &hw, &caps, 1);
}

fn make_local_light() -> HardwareCaps {
    HardwareCaps { ram_mb: 4096, has_gpu: false, load_avg: 0.5 }
}

fn make_local_loaded() -> HardwareCaps {
    HardwareCaps { ram_mb: 4096, has_gpu: false, load_avg: 0.9 }
}

/// 3 peers with different capabilities. Router should pick the peer with GPU + low load for "vision".
/// Other guilds ("bash") should stay local if local is good enough.
#[test]
fn test_router_multi_peer_picks_best() {
    let registry = make_registry();
    inject_peer(&registry, "rpi", "10.0.0.2:9000", 4096, false, 0.9, &["bash", "vision"]);
    inject_peer(&registry, "workstation", "10.0.0.3:9000", 32768, true, 0.1, &["vision", "comfy_ui"]);
    inject_peer(&registry, "server", "10.0.0.4:9000", 16384, false, 0.3, &["bash", "git"]);

    let router = DispatchRouter::new(registry, Duration::from_secs(60));
    router.record_latency("rpi", 5.0);
    router.record_latency("workstation", 2.0);
    router.record_latency("server", 3.0);

    // Vision should route to workstation (GPU + low load + good latency)
    let local = make_local_light();
    let decision = router.route("vision", &local, 10.0);
    assert_eq!(
        decision,
        DispatchDecision::Remote { node_id: "workstation".into(), addr: "10.0.0.3:9000".into() },
        "vision must route to workstation (GPU peer)"
    );

    // Bash should stay local (local is good enough, no GPU needed)
    let local_loaded = make_local_loaded();
    let decision_bash = router.route("bash", &local_loaded, 10.0);
    assert_eq!(
        decision_bash,
        DispatchDecision::Remote { node_id: "server".into(), addr: "10.0.0.4:9000".into() },
        "bash should route to server when local is overloaded"
    );
}

/// Router correctly prefers GPU-capable peers over loaded local.
#[test]
fn test_router_gpu_preference() {
    let registry = make_registry();
    inject_peer(&registry, "gpu-peer", "10.0.0.5:9000", 16384, true, 0.1, &["vision"]);
    let router = DispatchRouter::new(registry, Duration::from_secs(60));
    router.record_latency("gpu-peer", 5.0);

    let local = HardwareCaps { ram_mb: 8192, has_gpu: false, load_avg: 0.7 };
    let decision = router.route("vision", &local, 20.0);
    assert_eq!(
        decision,
        DispatchDecision::Remote { node_id: "gpu-peer".into(), addr: "10.0.0.5:9000".into() },
        "GPU peer should win over loaded local even with moderate latency"
    );
}

/// Circuit breaker: 3 consecutive failures → peer degraded → routes local.
#[test]
fn test_router_circuit_breaker_trips() {
    let registry = make_registry();
    inject_peer(&registry, "flakey-peer", "10.0.0.6:9000", 8192, false, 0.1, &["bash"]);

    let router = DispatchRouter::new(registry, Duration::from_secs(60));
    router.record_latency("flakey-peer", 2.0);

    let local = make_local_loaded();

    // First call routes remote (peer looks good)
    assert_eq!(
        router.route("bash", &local, 10.0),
        DispatchDecision::Remote { node_id: "flakey-peer".into(), addr: "10.0.0.6:9000".into() },
    );

    // 3 failures → circuit open → routes local
    router.record_failure("flakey-peer");
    router.record_failure("flakey-peer");
    router.record_failure("flakey-peer");
    assert_eq!(
        router.route("bash", &local, 10.0),
        DispatchDecision::Local,
        "circuit breaker must degrade flakey-peer after 3 failures"
    );
}

/// Circuit breaker recovers after cooldown.
#[test]
fn test_router_circuit_breaker_recovers() {
    let registry = make_registry();
    inject_peer(&registry, "recovering-peer", "10.0.0.7:9000", 8192, false, 0.1, &["bash"]);

    let router = DispatchRouter::new(registry, Duration::from_millis(50));
    router.record_latency("recovering-peer", 2.0);
    router.record_failure("recovering-peer");
    router.record_failure("recovering-peer");
    router.record_failure("recovering-peer");

    let local = make_local_loaded();
    assert_eq!(router.route("bash", &local, 10.0), DispatchDecision::Local);

    std::thread::sleep(Duration::from_millis(60));
    assert_eq!(
        router.route("bash", &local, 10.0),
        DispatchDecision::Remote { node_id: "recovering-peer".into(), addr: "10.0.0.7:9000".into() },
        "circuit breaker must recover after cooldown"
    );
}

/// DispatchQueue enqueue and dequeue basic FIFO order.
#[test]
fn test_queue_fifo_order() {
    let mut q = DispatchQueue::new(100);
    q.enqueue(serde_json::json!({"id": 1}));
    q.enqueue(serde_json::json!({"id": 2}));
    q.enqueue(serde_json::json!({"id": 3}));

    assert_eq!(q.dequeue(), Some(serde_json::json!({"id": 1})));
    assert_eq!(q.dequeue(), Some(serde_json::json!({"id": 2})));
    assert_eq!(q.dequeue(), Some(serde_json::json!({"id": 3})));
    assert_eq!(q.dequeue(), None);
}

/// DispatchQueue enqueue rejects when full.
#[test]
fn test_queue_full_eviction() {
    let mut q = DispatchQueue::new(2);
    assert!(q.enqueue(serde_json::json!({"id": 1})));
    assert!(q.enqueue(serde_json::json!({"id": 2})));
    assert!(!q.enqueue(serde_json::json!({"id": 3})), "queue full must reject");

    assert_eq!(q.len(), 2);
}

/// DispatchQueue TTL: items older than timeout are reported by peek_timed_out
/// and removed by remove_timed_out.
#[test]
fn test_queue_ttl_expiry() {
    let mut q = DispatchQueue::new(100);
    q.enqueue(serde_json::json!({"id": "fresh"}));

    std::thread::sleep(Duration::from_millis(10));
    let timed_out = q.peek_timed_out(Duration::from_millis(5));
    assert!(!timed_out.is_empty(), "item older than TTL must appear in peek");

    q.remove_timed_out(Duration::from_millis(5));
    assert!(q.is_empty(), "queue must be empty after TTL removal");
}