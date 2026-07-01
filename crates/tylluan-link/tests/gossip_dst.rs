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
