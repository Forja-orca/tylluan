//! DST (Deterministic Simulation Testing) harness for GossipEngine.
//! Uses InMemoryTransport (mpsc channels) to simulate network interactions
//! without spawning real TCP sockets. Each test is fully deterministic.
//!
//! All sync roundtrips use tokio::join! to avoid deadlocks (spawn + sequential
//! can hang because the responder task must be polled concurrently).

use tylluan_link::gossip::{GossipEngine, GossipConfig, GossipMessage};
use tylluan_link::transport::{MeshTransport, in_memory_pair};

fn make_engine(id: &str) -> GossipEngine {
    GossipEngine::new(id.to_string(), GossipConfig::default())
}

async fn respond_to_sync(
    responder: &mut GossipEngine,
    transport: &mut impl MeshTransport,
    initiator_id: &str,
) {
    let raw = transport.receive().await.expect("responder: receive Pull");
    let msg: GossipMessage = serde_json::from_slice(&raw).unwrap();
    responder.handle_incoming_message(transport, initiator_id, &msg).await.unwrap();
    if let Ok(raw2) = transport.receive().await {
        let msg2: GossipMessage = serde_json::from_slice(&raw2).unwrap();
        responder.handle_incoming_message(transport, initiator_id, &msg2).await.ok();
    }
}

/// Normal push-pull sync between two engines.
/// Engine A has an entry. B performs sync. B must receive A's entry.
#[tokio::test]
async fn test_gossip_dst_normal_sync() {
    let mut engine_a = make_engine("node-a");
    let mut engine_b = make_engine("node-b");

    engine_a.advance_clock();
    let entry = engine_a.local_entry("127.0.0.1:9000", vec!["bash".into(), "git".into()]);
    engine_a.store_entries(&[entry.clone()]);

    let (mut t_a, mut t_b) = in_memory_pair();

    let sync = engine_b.perform_sync(&mut t_b, "node-a");
    let handle = respond_to_sync(&mut engine_a, &mut t_a, "node-b");

    let (result, _) = tokio::join!(sync, handle);
    result.unwrap();

    let b_entries = engine_b.entries_since(0);
    assert!(!b_entries.is_empty(), "B must receive at least one entry from A");
    assert!(
        b_entries.iter().any(|e| e.node_id == engine_a.local_node_id()),
        "B must have A's node entry"
    );
}

/// Partition simulation — receive but then drop the responder transport.
/// B's sync must fail gracefully.
#[tokio::test]
async fn test_gossip_dst_partition_graceful_failure() {
    let mut engine_b = make_engine("node-b");
    let (mut t_a, mut t_b) = in_memory_pair();

    let handle = tokio::spawn(async move {
        t_a.receive().await.ok();
        drop(t_a);
    });

    let result = engine_b.perform_sync(&mut t_b, "node-a").await;
    handle.await.unwrap();
    assert!(result.is_err(), "sync must fail when peer is unreachable");
    assert_eq!(engine_b.entries_since(0).len(), 0, "B state must be clean after failed sync");
}

/// Bidirectional convergence — two engines sync, both converge.
/// A has entry_a, B has entry_b. After one sync round, both have both entries.
#[tokio::test]
async fn test_gossip_dst_bidirectional_convergence() {
    let mut engine_a = make_engine("node-a");
    let mut engine_b = make_engine("node-b");

    engine_a.advance_clock();
    let entry_a = engine_a.local_entry("10.0.0.1:9000", vec!["bash".into()]);
    engine_a.store_entries(&[entry_a]);
    engine_b.advance_clock();
    let entry_b = engine_b.local_entry("10.0.0.2:9000", vec!["git".into()]);
    engine_b.store_entries(&[entry_b]);

    let (mut t_a, mut t_b) = in_memory_pair();

    let sync = engine_b.perform_sync(&mut t_b, "node-a");
    let handle = respond_to_sync(&mut engine_a, &mut t_a, "node-b");

    let (result, _) = tokio::join!(sync, handle);
    result.unwrap();

    let b_entries = engine_b.entries_since(0);
    let a_entries = engine_a.entries_since(0);
    assert!(b_entries.iter().any(|e| e.node_id == engine_a.local_node_id()), "B must have A's entry");
    assert!(a_entries.iter().any(|e| e.node_id == "node-b"), "A must have B's entry");
}

// ── T427: 3 nuevos tests DST ─────────────────────────────────────────────────

/// 3-node ring convergence.
/// Topology: A ─sync→ B ─sync→ C
/// A holds entry_a. After A→B sync B has entry_a.
/// After B→C sync C must also have entry_a (transitive propagation).
#[tokio::test]
async fn test_gossip_dst_3node_convergence() {
    let mut engine_a = make_engine("node-a");
    let mut engine_b = make_engine("node-b");
    let mut engine_c = make_engine("node-c");

    engine_a.advance_clock();
    let entry_a = engine_a.local_entry("10.0.0.1:9000", vec!["rust".into()]);
    engine_a.store_entries(&[entry_a]);

    // Round 1: B pulls from A
    {
        let (mut t_a, mut t_b) = in_memory_pair();
        let sync = engine_b.perform_sync(&mut t_b, "node-a");
        let handle = respond_to_sync(&mut engine_a, &mut t_a, "node-b");
        let (result, _) = tokio::join!(sync, handle);
        result.unwrap();
    }

    assert!(
        engine_b.entries_since(0).iter().any(|e| e.node_id == "node-a"),
        "B must have A's entry after A->B sync"
    );

    // Round 2: C pulls from B — transitive propagation
    {
        let (mut t_b, mut t_c) = in_memory_pair();
        let sync = engine_c.perform_sync(&mut t_c, "node-b");
        let handle = respond_to_sync(&mut engine_b, &mut t_b, "node-c");
        let (result, _) = tokio::join!(sync, handle);
        result.unwrap();
    }

    assert!(
        engine_c.entries_since(0).iter().any(|e| e.node_id == "node-a"),
        "C must have A's entry after B->C sync (transitive propagation)"
    );
}

/// Message loss resilience.
/// First sync attempt: the responder drops connection immediately (simulated loss).
/// The initiator receives an Err. A second sync succeeds and delivers the entry.
#[tokio::test]
async fn test_gossip_dst_message_loss_resilience() {
    let mut engine_a = make_engine("node-a");
    let mut engine_b = make_engine("node-b");

    engine_a.advance_clock();
    let entry_a = engine_a.local_entry("10.0.0.1:9000", vec!["resilience".into()]);
    engine_a.store_entries(&[entry_a]);

    // Attempt 1: packet loss — responder closes channel without responding
    {
        let (mut t_a, mut t_b) = in_memory_pair();
        let handle = tokio::spawn(async move {
            t_a.receive().await.ok(); // absorb Pull
            drop(t_a);               // close without responding
        });
        let result = engine_b.perform_sync(&mut t_b, "node-a").await;
        handle.await.unwrap();
        assert!(result.is_err(), "first sync must fail on message loss");
        assert!(
            engine_b.entries_since(0).is_empty(),
            "B state must stay clean after failed sync"
        );
    }

    // Attempt 2: retry — succeeds
    {
        let (mut t_a2, mut t_b2) = in_memory_pair();
        let sync = engine_b.perform_sync(&mut t_b2, "node-a");
        let handle = respond_to_sync(&mut engine_a, &mut t_a2, "node-b");
        let (result, _) = tokio::join!(sync, handle);
        result.unwrap();
    }

    assert!(
        engine_b.entries_since(0).iter().any(|e| e.node_id == "node-a"),
        "B must converge with A after successful retry"
    );
}

/// Concurrent conflicting updates — last-write-wins (LWW) by clock.
/// A holds node-x at clock=5 (stale), B holds node-x at clock=10 (fresh).
/// After sync, the fresh entry must survive on both sides.
#[tokio::test]
async fn test_gossip_dst_concurrent_conflicting_updates() {
    use tylluan_link::gossip::GossipEntry;

    let mut engine_a = make_engine("node-a");
    let mut engine_b = make_engine("node-b");

    let entry_x_stale = GossipEntry {
        node_id: "node-x".to_string(),
        addr: "10.0.0.10:9000".to_string(),
        capabilities: vec!["stale".into()],
        clock: 5,
    };
    let entry_x_fresh = GossipEntry {
        node_id: "node-x".to_string(),
        addr: "10.0.0.10:9001".to_string(),
        capabilities: vec!["fresh".into()],
        clock: 10,
    };

    engine_a.store_entries(&[entry_x_stale]);
    engine_b.store_entries(&[entry_x_fresh]);

    // A syncs with B (B is the responder — B will send its fresh entry to A)
    {
        let (mut t_b, mut t_a) = in_memory_pair();
        let sync = engine_a.perform_sync(&mut t_a, "node-b");
        let handle = respond_to_sync(&mut engine_b, &mut t_b, "node-a");
        let (result, _) = tokio::join!(sync, handle);
        result.unwrap();
    }

    // A must now carry the fresher entry (clock=10) received from B
    let a_node_x = engine_a
        .entries_since(0)
        .into_iter()
        .find(|e| e.node_id == "node-x")
        .expect("A must know node-x");
    assert_eq!(a_node_x.clock, 10, "A must adopt B's higher-clock entry (LWW)");
    assert!(
        a_node_x.capabilities.contains(&"fresh".to_string()),
        "A must adopt fresh capabilities"
    );

    // B must retain its own fresh entry (not degraded by A's stale copy)
    let b_node_x = engine_b
        .entries_since(0)
        .into_iter()
        .find(|e| e.node_id == "node-x")
        .expect("B must know node-x");
    assert_eq!(b_node_x.clock, 10, "B must retain fresh entry (LWW)");
}
