use tylluan_link::gossip::{GossipEngine, GossipConfig, GossipEntry, GossipMessage};
use tylluan_link::transport::{MeshTransport, in_memory_pair};

fn make_entry(node_id: &str, clock: u64) -> GossipEntry {
    GossipEntry {
        node_id: node_id.to_string(),
        addr: format!("127.0.0.1:{}", 3000 + clock),
        capabilities: vec!["mesh".into()],
        clock,
    }
}

fn make_engine(node_id: &str) -> GossipEngine {
    GossipEngine::new(node_id.to_string(), GossipConfig::default())
}

async fn roundtrip(
    initiator: &mut GossipEngine,
    responder: &mut GossipEngine,
    initiator_id: &str,
    responder_id: &str,
) {
    let (mut to_responder, mut from_initiator) = in_memory_pair();

    let initiator_fut = initiator.perform_sync(&mut to_responder, responder_id);
    let responder_fut = async {
        let data = from_initiator.receive().await.unwrap();
        let msg: GossipMessage = serde_json::from_slice(&data).unwrap();
        responder.handle_incoming_message(&mut from_initiator, initiator_id, &msg).await.unwrap();
        let push_data = from_initiator.receive().await;
        if let Ok(data) = push_data {
            let msg: GossipMessage = serde_json::from_slice(&data).unwrap();
            responder.handle_incoming_message(&mut from_initiator, initiator_id, &msg).await.unwrap();
        }
    };

    let (result, _) = tokio::join!(initiator_fut, responder_fut);
    result.unwrap();
}

#[tokio::test]
async fn test_one_way_sync() {
    let mut alice = make_engine("alice");
    let mut bob = make_engine("bob");
    let (mut a_t, mut b_t) = in_memory_pair();

    alice.advance_clock();
    alice.store_entries(&[make_entry("alice", 1)]);

    let sync = alice.perform_sync(&mut a_t, "bob");
    let handle = async {
        let data = b_t.receive().await.unwrap();
        let msg: GossipMessage = serde_json::from_slice(&data).unwrap();
        bob.handle_incoming_message(&mut b_t, "alice", &msg).await.unwrap();
        // Also handle potential Push
        if let Ok(push_data) = b_t.receive().await {
            let msg: GossipMessage = serde_json::from_slice(&push_data).unwrap();
            bob.handle_incoming_message(&mut b_t, "alice", &msg).await.unwrap();
        }
    };
    let (result, _) = tokio::join!(sync, handle);
    result.unwrap();

    assert!(bob.entries_since(0).iter().any(|e| e.node_id == "alice"));
}

#[tokio::test]
async fn test_bidirectional_sync() {
    let mut alice = make_engine("alice");
    let mut bob = make_engine("bob");

    alice.advance_clock();
    alice.store_entries(&[make_entry("alice", 1)]);
    bob.advance_clock();
    bob.store_entries(&[make_entry("bob", 1)]);

    // Alice → Bob
    roundtrip(&mut alice, &mut bob, "alice", "bob").await;
    // Bob → Alice
    roundtrip(&mut bob, &mut alice, "bob", "alice").await;

    let alice_entries = alice.entries_since(0);
    let bob_entries = bob.entries_since(0);
    assert_eq!(alice_entries.len(), 2, "Alice should have 2 entries");
    assert_eq!(bob_entries.len(), 2, "Bob should have 2 entries");
}

#[tokio::test]
async fn test_entry_propagation_chain() {
    let mut alice = make_engine("alice");
    let mut bob = make_engine("bob");
    let mut charlie = make_engine("charlie");

    alice.advance_clock();
    alice.store_entries(&[make_entry("alice", 1)]);

    // Alice → Bob
    roundtrip(&mut alice, &mut bob, "alice", "bob").await;
    assert!(bob.entries_since(0).iter().any(|e| e.node_id == "alice"));

    // Bob → Charlie
    roundtrip(&mut bob, &mut charlie, "bob", "charlie").await;
    assert!(charlie.entries_since(0).iter().any(|e| e.node_id == "alice"),
        "Charlie should have Alice's entry via Bob");
}

#[tokio::test]
async fn test_concurrent_sync() {
    let mut alice = make_engine("alice");
    let mut bob = make_engine("bob");

    alice.advance_clock();
    alice.store_entries(&[make_entry("alice", 1)]);
    bob.advance_clock();
    bob.store_entries(&[make_entry("bob", 1)]);

    roundtrip(&mut alice, &mut bob, "alice", "bob").await;

    // Both should have both entries after a single roundtrip
    assert_eq!(alice.entries_since(0).len(), 2, "Alice received Bob's entry via PullResponse");
    assert_eq!(bob.entries_since(0).len(), 2, "Bob received Alice's entry via Push");
}
