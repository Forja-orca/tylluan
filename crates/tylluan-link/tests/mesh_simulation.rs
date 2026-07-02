//! Mesh topology simulation tests — Phase 1 (Padawan handoff).
//!
//! Uses InMemoryTransport (mpsc channels) to simulate full-mesh, star, and
//! split-brain topologies without spawning real TCP sockets.
//!
//! Patterns follow gossip_dst.rs: make_engine, respond_to_sync, in_memory_pair.

use tylluan_link::gossip::{GossipEngine, GossipConfig, GossipEntry, GossipMessage, HardwareCaps};
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

/// Full mesh (3 nodes, all-pairs): each pair synchronizes directly.
/// A has entry_a, B has entry_b, C has entry_c.
/// After A↔B, B↔C, C↔A syncs, all three nodes have all three entries.
#[tokio::test]
async fn test_full_mesh_3node_all_pairs() {
    let mut engine_a = make_engine("node-a");
    let mut engine_b = make_engine("node-b");
    let mut engine_c = make_engine("node-c");

    engine_a.advance_clock();
    let entry_a = engine_a.local_entry("10.0.0.1:9000", vec!["bash".into()], HardwareCaps::default());
    engine_a.store_entries(&[entry_a]);
    engine_b.advance_clock();
    let entry_b = engine_b.local_entry("10.0.0.2:9000", vec!["git".into()], HardwareCaps::default());
    engine_b.store_entries(&[entry_b]);
    engine_c.advance_clock();
    let entry_c = engine_c.local_entry("10.0.0.3:9000", vec!["vision".into()], HardwareCaps::default());
    engine_c.store_entries(&[entry_c]);

    // A ↔ B
    {
        let (mut t_a, mut t_b) = in_memory_pair();
        let sync = engine_b.perform_sync(&mut t_b, "node-a");
        let handle = respond_to_sync(&mut engine_a, &mut t_a, "node-b");
        let (result, _) = tokio::join!(sync, handle);
        result.unwrap();
    }

    // B ↔ C
    {
        let (mut t_b, mut t_c) = in_memory_pair();
        let sync = engine_c.perform_sync(&mut t_c, "node-b");
        let handle = respond_to_sync(&mut engine_b, &mut t_b, "node-c");
        let (result, _) = tokio::join!(sync, handle);
        result.unwrap();
    }

    // C ↔ A
    {
        let (mut t_c, mut t_a) = in_memory_pair();
        let sync = engine_a.perform_sync(&mut t_a, "node-c");
        let handle = respond_to_sync(&mut engine_c, &mut t_c, "node-a");
        let (result, _) = tokio::join!(sync, handle);
        result.unwrap();
    }

    let all_a = engine_a.entries_since(0);
    let all_b = engine_b.entries_since(0);
    let all_c = engine_c.entries_since(0);

    assert!(all_a.iter().any(|e| e.node_id == "node-a"), "A must have self");
    assert!(all_a.iter().any(|e| e.node_id == "node-b"), "A must have B's entry");
    assert!(all_a.iter().any(|e| e.node_id == "node-c"), "A must have C's entry");
    assert!(all_b.iter().any(|e| e.node_id == "node-a"), "B must have A's entry");
    assert!(all_b.iter().any(|e| e.node_id == "node-b"), "B must have self");
    assert!(all_b.iter().any(|e| e.node_id == "node-c"), "B must have C's entry");
    assert!(all_c.iter().any(|e| e.node_id == "node-a"), "C must have A's entry");
    assert!(all_c.iter().any(|e| e.node_id == "node-b"), "C must have B's entry");
    assert!(all_c.iter().any(|e| e.node_id == "node-c"), "C must have self");
}

/// Star topology: B is the hub, A and C only sync with B.
/// A holds entry_a, C holds entry_c (neither knows the other directly).
/// B holds its own entry. After A→B sync, B knows entry_a, A knows entry_b.
/// After C→B sync, B knows entry_c, C knows entry_b.
#[tokio::test]
async fn test_star_topology_hub_propagation() {
    let mut engine_a = make_engine("node-a");
    let mut engine_b = make_engine("node-b");
    let mut engine_c = make_engine("node-c");

    // All three have their own entries stored
    engine_a.advance_clock();
    let entry_a = engine_a.local_entry("10.0.0.1:9000", vec!["bash".into()], HardwareCaps::default());
    engine_a.store_entries(&[entry_a]);
    engine_b.advance_clock();
    let entry_b = engine_b.local_entry("10.0.0.2:9000", vec!["hub".into()], HardwareCaps::default());
    engine_b.store_entries(&[entry_b]);
    engine_c.advance_clock();
    let entry_c = engine_c.local_entry("10.0.0.3:9000", vec!["vision".into()], HardwareCaps::default());
    engine_c.store_entries(&[entry_c]);

    // A syncs with B (hub)
    {
        let (mut t_a, mut t_b) = in_memory_pair();
        let sync = engine_b.perform_sync(&mut t_b, "node-a");
        let handle = respond_to_sync(&mut engine_a, &mut t_a, "node-b");
        let (result, _) = tokio::join!(sync, handle);
        result.unwrap();
    }

    // C syncs with B (hub)
    {
        let (mut t_c, mut t_b) = in_memory_pair();
        let sync = engine_b.perform_sync(&mut t_b, "node-c");
        let handle = respond_to_sync(&mut engine_c, &mut t_c, "node-b");
        let (result, _) = tokio::join!(sync, handle);
        result.unwrap();
    }

    let b_entries = engine_b.entries_since(0);
    assert!(b_entries.iter().any(|e| e.node_id == "node-a"), "Hub B must have A's entry");
    assert!(b_entries.iter().any(|e| e.node_id == "node-c"), "Hub B must have C's entry");

    let a_entries = engine_a.entries_since(0);
    assert!(a_entries.iter().any(|e| e.node_id == "node-b"), "Leaf A must have hub B's entry");

    let c_entries = engine_c.entries_since(0);
    assert!(c_entries.iter().any(|e| e.node_id == "node-b"), "Leaf C must have hub B's entry");
}

/// Split-brain partition then heal.
/// A and B diverge: A has entry_a at clock 5, B has entry_a at clock 10 (fresh).
/// After partition A→B sync fails. After heal (successful sync via hub H),
/// the fresher clock=10 entry must survive on both A and B.
#[tokio::test]
async fn test_split_brain_partition_then_heal() {
    let mut engine_h = make_engine("node-h");
    let mut engine_a = make_engine("node-a");
    let mut engine_b = make_engine("node-b");

    // Both A and B have divergent entries for node-x
    let entry_x_stale = GossipEntry {
        node_id: "node-x".to_string(),
        addr: "10.0.0.1:9000".to_string(),
        capabilities: vec!["stale".into()],
        clock: 5,
        hardware: HardwareCaps::default(),
    };
    let entry_x_fresh = GossipEntry {
        node_id: "node-x".to_string(),
        addr: "10.0.0.2:9001".to_string(),
        capabilities: vec!["fresh".into()],
        clock: 10,
        hardware: HardwareCaps::default(),
    };

    engine_a.store_entries(&[entry_x_stale]);
    engine_b.store_entries(&[entry_x_fresh]);

    // Partition: A→B sync fails
    {
        let (mut t_a, mut t_b) = in_memory_pair();
        let handle = tokio::spawn(async move {
            t_a.receive().await.ok();
            drop(t_a); // close without responding — simulates partition
        });
        let result = engine_b.perform_sync(&mut t_b, "node-a").await;
        handle.await.unwrap();
        assert!(result.is_err(), "partitioned sync must fail");
    }

    // Heal: B syncs with hub H (H has no node-x yet, receives B's fresh entry)
    engine_h.advance_clock();
    let entry_h = engine_h.local_entry("10.0.0.99:9000", vec!["hub".into()], HardwareCaps::default());
    engine_h.store_entries(&[entry_h]);

    {
        let (mut t_b, mut t_h) = in_memory_pair();
        let sync = engine_h.perform_sync(&mut t_h, "node-b");
        let handle = respond_to_sync(&mut engine_b, &mut t_b, "node-h");
        let (result, _) = tokio::join!(sync, handle);
        result.unwrap();
    }

    // H must have B's fresh entry
    let h_entries = engine_h.entries_since(0);
    let h_node_x = h_entries.iter().find(|e| e.node_id == "node-x").expect("H must know node-x after B sync");
    assert_eq!(h_node_x.clock, 10, "H must adopt B's fresh entry (LWW)");

    // Now A syncs with H — A must get the fresh entry (clock=10)
    {
        let (mut t_a, mut t_h) = in_memory_pair();
        let sync = engine_h.perform_sync(&mut t_h, "node-a");
        let handle = respond_to_sync(&mut engine_a, &mut t_a, "node-h");
        let (result, _) = tokio::join!(sync, handle);
        result.unwrap();
    }

    let a_entries = engine_a.entries_since(0);
    let a_node_x = a_entries.iter().find(|e| e.node_id == "node-x").expect("A must know node-x after heal");
    assert_eq!(a_node_x.clock, 10, "A must adopt clock=10 after healing via hub (LWW)");
    assert!(a_node_x.capabilities.contains(&"fresh".to_string()), "A must adopt fresh capabilities after heal");
}
