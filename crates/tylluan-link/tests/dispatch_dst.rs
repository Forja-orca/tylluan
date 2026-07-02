//! DST (Deterministic Simulation Testing) for DispatchRouter + DispatchQueue.
//!
//! Phase 2: Router multi-peer routing decisions.
//! Phase 3: DispatchQueue FIFO, overflow, and TTL semantics.

use std::sync::{Arc, Mutex};
use std::time::Duration;
use tylluan_link::capability::CapabilityRegistry;
use tylluan_link::dispatch::{DispatchDecision, DispatchQueue, DispatchRouter};
use tylluan_link::gossip::HardwareCaps;

fn make_registry() -> Arc<Mutex<CapabilityRegistry>> {
    Arc::new(Mutex::new(CapabilityRegistry::new(Duration::from_secs(300))))
}

fn cpu_light() -> HardwareCaps {
    HardwareCaps { ram_mb: 4096, has_gpu: false, load_avg: 0.1 }
}

fn cpu_medium() -> HardwareCaps {
    HardwareCaps { ram_mb: 8192, has_gpu: false, load_avg: 0.3 }
}

fn cpu_heavy() -> HardwareCaps {
    HardwareCaps { ram_mb: 4096, has_gpu: false, load_avg: 0.9 }
}

fn gpu_light() -> HardwareCaps {
    HardwareCaps { ram_mb: 16384, has_gpu: true, load_avg: 0.1 }
}

// ─── Phase 2: Router multi-peer ────────────────────────────────────────

#[test]
fn test_router_selects_gpu_peer_over_two_cpu_peers() {
    let registry = make_registry();
    let router = DispatchRouter::new(registry.clone(), Duration::from_secs(60));

    {
        let mut reg = registry.lock().unwrap();
        reg.ingest("cpu-1", "10.0.0.2:9000", &cpu_light(), &["vision".into()], 1);
        reg.ingest("cpu-2", "10.0.0.3:9000", &cpu_medium(), &["vision".into()], 2);
        reg.ingest("gpu-1", "10.0.0.4:9000", &gpu_light(), &["vision".into()], 3);
    }

    // CPU peers have slow latency (100ms), GPU peer is fast (5ms)
    router.record_latency("cpu-1", 100.0);
    router.record_latency("cpu-2", 100.0);
    router.record_latency("gpu-1", 5.0);

    let local = HardwareCaps { ram_mb: 2048, has_gpu: false, load_avg: 0.5 };
    let decision = router.route("vision", &local, 30.0);

    assert_eq!(
        decision,
        DispatchDecision::Remote { node_id: "gpu-1".into(), addr: "10.0.0.4:9000".into() },
        "GPU peer must win with lower latency and GPU multiplier"
    );
}

#[test]
fn test_router_capability_filter_excludes_wrong_guild() {
    let registry = make_registry();
    let router = DispatchRouter::new(registry.clone(), Duration::from_secs(60));

    {
        let mut reg = registry.lock().unwrap();
        reg.ingest("peer-bash", "10.0.0.2:9000", &gpu_light(), &["bash".into()], 1);
        reg.ingest("peer-git", "10.0.0.3:9000", &gpu_light(), &["git".into()], 2);
    }

    router.record_latency("peer-bash", 5.0);
    router.record_latency("peer-git", 5.0);

    let local = cpu_heavy();
    let decision = router.route("vision", &local, 10.0);

    assert_eq!(
        decision,
        DispatchDecision::Local,
        "Must route local when no peer supports the requested guild"
    );
}

#[test]
fn test_router_falls_back_to_second_peer_when_first_circuit_open() {
    let registry = make_registry();
    let router = DispatchRouter::new(registry.clone(), Duration::from_millis(200));

    {
        let mut reg = registry.lock().unwrap();
        reg.ingest("peer-best", "10.0.0.2:9000", &gpu_light(), &["bash".into()], 1);
        reg.ingest("peer-fallback", "10.0.0.3:9000", &cpu_light(), &["bash".into()], 2);
    }

    router.record_latency("peer-best", 5.0);
    router.record_latency("peer-fallback", 50.0);

    let local = cpu_heavy();

    // Initially peer-best wins (GPU + low latency)
    assert_eq!(
        router.route("bash", &local, 100.0),
        DispatchDecision::Remote { node_id: "peer-best".into(), addr: "10.0.0.2:9000".into() }
    );

    // Trip circuit breaker for peer-best
    router.record_failure("peer-best");
    router.record_failure("peer-best");
    router.record_failure("peer-best");

    // Falls back to peer-fallback
    assert_eq!(
        router.route("bash", &local, 100.0),
        DispatchDecision::Remote { node_id: "peer-fallback".into(), addr: "10.0.0.3:9000".into() }
    );

    // After cooldown, peer-best recovers
    std::thread::sleep(Duration::from_millis(250));
    assert_eq!(
        router.route("bash", &local, 100.0),
        DispatchDecision::Remote { node_id: "peer-best".into(), addr: "10.0.0.2:9000".into() }
    );
}

// ─── Phase 3: DispatchQueue ────────────────────────────────────────────

#[test]
fn test_dispatch_queue_enqueue_dequeue_fifo() {
    let mut q = DispatchQueue::new(10);

    assert!(q.enqueue(serde_json::json!("first")));
    assert!(q.enqueue(serde_json::json!("second")));
    assert!(q.enqueue(serde_json::json!("third")));

    assert_eq!(q.dequeue(), Some(serde_json::json!("first")));
    assert_eq!(q.dequeue(), Some(serde_json::json!("second")));
    assert_eq!(q.dequeue(), Some(serde_json::json!("third")));
    assert_eq!(q.dequeue(), None);
    assert!(q.is_empty());
}

#[test]
fn test_dispatch_queue_max_size_rejects_overflow() {
    let mut q = DispatchQueue::new(2);

    assert!(q.enqueue(serde_json::json!("a")));
    assert!(q.enqueue(serde_json::json!("b")));
    assert!(!q.enqueue(serde_json::json!("c")), "Third enqueue must be rejected");

    assert_eq!(q.len(), 2);
    assert_eq!(q.dequeue(), Some(serde_json::json!("a")));
}

#[test]
fn test_dispatch_queue_ttl_expiry() {
    let mut q = DispatchQueue::new(10);

    q.enqueue(serde_json::json!("fresh"));
    q.enqueue(serde_json::json!("stale"));
    q.enqueue(serde_json::json!("another-fresh"));

    // Manually age the second entry by rewriting internal state
    // The only way to control Instant in tests is to modify the queue internals.
    // We test TTL via remove_timed_out with a very short timeout.
    // We wait and then verify that the stale entries are removed.

    // Sleep so "stale" entry ages relative to the first enqueue
    std::thread::sleep(Duration::from_millis(50));

    // Enqueue more fresh items after the sleep
    q.enqueue(serde_json::json!("late-entry"));

    // Remove items older than 25ms — should remove the first 3 entries
    q.remove_timed_out(Duration::from_millis(25));

    assert_eq!(q.len(), 1, "Only the late entry should survive");
    assert_eq!(q.dequeue(), Some(serde_json::json!("late-entry")));
}

#[test]
fn test_dispatch_queue_ttl_keeps_fresh_entries() {
    let mut q = DispatchQueue::new(10);

    q.enqueue(serde_json::json!("alpha"));
    q.enqueue(serde_json::json!("beta"));

    // Very short sleep ensures entries are still fresh relative to a generous TTL
    std::thread::sleep(Duration::from_millis(10));

    let before = q.len();
    q.remove_timed_out(Duration::from_secs(60));

    assert_eq!(q.len(), before, "No entries should be removed with 60s TTL");
    assert_eq!(q.dequeue(), Some(serde_json::json!("alpha")));
    assert_eq!(q.dequeue(), Some(serde_json::json!("beta")));
}
