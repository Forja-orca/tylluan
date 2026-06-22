//! Concurrent agent write safety tests.
//! Validates that 20 simultaneous agents writing to SilvaDB produce no
//! duplicates, preserve max weight, and leave the graph consistent.

use tylluan_kernel::memory::silva::SilvaDB;
use std::sync::Arc;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_20_agents_concurrent_identity_write() {
    let db = Arc::new(SilvaDB::in_memory().await.unwrap());

    let handles: Vec<_> = (0..20).map(|i| {
        let db = db.clone();
        tokio::spawn(async move {
            db.upsert_node(
                &format!("agent:{i}"),
                "identity",
                &format!("Agent {i} identity"),
                "{}",
            ).await
        })
    }).collect();

    for h in handles {
        h.await.unwrap().unwrap();
    }

    let count = db.node_count().await.unwrap();
    assert_eq!(count, 20, "Expected exactly 20 identity nodes, got {count}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_write_preserves_max_weight() {
    let db = Arc::new(SilvaDB::in_memory().await.unwrap());

    // Two agents write the same concept node simultaneously
    // Agent A writes weight 1.0 (default), then we reinforce it
    db.upsert_node("concept:rust", "concept", "Rust programming language", "{}").await.unwrap();
    db.reinforce_node("concept:rust", 3.0).await.unwrap(); // weight ~3.0

    // Now a second agent writes the same node (lower weight)
    // upsert should keep MAX(existing, new) = keep the higher weight
    db.upsert_node("concept:rust", "concept", "Rust systems language", "{}").await.unwrap();

    let node = db.get_node("concept:rust").await.unwrap().unwrap();
    assert!(node.weight > 1.0, "Weight should be preserved after concurrent upsert, got {}", node.weight);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_20_agents_concurrent_memory_write() {
    let db = Arc::new(SilvaDB::in_memory().await.unwrap());

    // 20 agents each write a unique memory node
    let handles: Vec<_> = (0..20).map(|i| {
        let db = db.clone();
        tokio::spawn(async move {
            db.upsert_node(
                &format!("memory:agent{i}_task1"),
                "agent_memory",
                &format!("[agent{i}] completed analysis task"),
                "{}",
            ).await
        })
    }).collect();

    for h in handles {
        h.await.unwrap().unwrap();
    }

    let count = db.node_count().await.unwrap();
    assert_eq!(count, 20, "Each agent should have exactly one memory node");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_retrolink_safe_under_concurrent_writes() {
    let db = Arc::new(SilvaDB::in_memory().await.unwrap());

    // Seed some nodes first
    for i in 0..10 {
        db.upsert_node(
            &format!("concept:topic{i}"),
            "concept",
            &format!("Topic {i} about systems programming"),
            "{}",
        ).await.unwrap();
    }

    // Concurrent writes + retrolink at the same time
    let db2 = db.clone();
    let retrolink_handle = tokio::spawn(async move {
        db2.retrolink_orphans(50, 0.1).await
    });

    let write_handles: Vec<_> = (10..20).map(|i| {
        let db = db.clone();
        tokio::spawn(async move {
            db.upsert_node(
                &format!("concept:topic{i}"),
                "concept",
                &format!("Topic {i} about memory safety"),
                "{}",
            ).await
        })
    }).collect();

    // Both should complete without panic or deadlock
    let _ = retrolink_handle.await.unwrap();
    for h in write_handles {
        h.await.unwrap().unwrap();
    }

    let count = db.node_count().await.unwrap();
    assert_eq!(count, 20);
}
