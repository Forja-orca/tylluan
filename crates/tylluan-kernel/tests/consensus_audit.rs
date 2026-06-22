use tylluan_kernel::memory::silva::SilvaDB;
use tylluan_kernel::memory::consensus::ConsensusEngine;
use std::sync::Arc;

#[tokio::test(flavor = "multi_thread")]
async fn test_consensus_simple_conflict() {
    let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
    let engine = ConsensusEngine::new(silva.clone());

    // Agent A (Trust 1.0)
    silva.upsert_node("msg_a", "lesson", "The server uses port 3030", "{\"agent\":\"agent_a\", \"topic\":\"server_port\"}").await.unwrap();
    
    // Agent B (Trust 1.0, but we'll simulate higher weight for A)
    silva.upsert_node("msg_b", "lesson", "The server uses port 8080", "{\"agent\":\"agent_b\", \"topic\":\"server_port\"}").await.unwrap();
    
    // Explicitly mark as conflicted and set topic for the engine to pick them up
    {
        let conn_ref = silva.conn_lock();
        let conn = conn_ref.lock().await;
        conn.execute("UPDATE nodes SET topic_key = 'server_port', conflicted = 1 WHERE id IN ('msg_a', 'msg_b')", []).unwrap();
    }

    // Reinforce A slightly to simulate trust/preference
    silva.reinforce_node("msg_a", 1.2).await.unwrap();

    let resolved = engine.consolidate(Some("server_port")).await.unwrap();
    assert!(resolved > 0, "Should have resolved the conflict for server_port");

    let node_a = silva.get_node("msg_a").await.unwrap().unwrap();
    let node_b = silva.get_node("msg_b").await.unwrap().unwrap();

    assert!(node_a.weight > 1.2, "Winner should be reinforced");
    assert!(node_b.weight < 1.0, "Loser should have decay");
    assert!(!node_a.conflicted, "Winner should be resolved");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_consensus_evidence_wins_over_trust() {
    let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
    let engine = ConsensusEngine::new(silva.clone());

    // Agent A: Senior but wrong
    silva.upsert_node("senior_node", "lesson", "DB is slow", "{\"agent\":\"senior\"}").await.unwrap();
    
    // Agent B: Junior but WITH EVIDENCE
    silva.upsert_node("junior_evidence", "lesson", "DB is fast (see logs)", "{\"agent\":\"junior\", \"file_ref\":\"logs.txt\", \"verified\":true}").await.unwrap();

    {
        let conn_ref = silva.conn_lock();
        let conn = conn_ref.lock().await;
        conn.execute("UPDATE nodes SET topic_key = 'db_perf', conflicted = 1 WHERE id IN ('senior_node', 'junior_evidence')", []).unwrap();
    }

    // Even if senior has slightly higher base weight, junior has +2.0 bonus
    let _ = engine.consolidate(Some("db_perf")).await.unwrap();

    let winner = silva.get_node("junior_evidence").await.unwrap().unwrap();
    let loser = silva.get_node("senior_node").await.unwrap().unwrap();

    assert!(winner.weight > 1.0, "Evidence node should win");
    assert!(loser.weight < 1.0, "Senior without evidence should lose");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_consensus_protected_immunity() {
    let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
    let engine = ConsensusEngine::new(silva.clone());

    silva.upsert_node("identity_god", "identity", "I am the OS", "{}").await.unwrap();
    silva.set_protected("identity_god", true).await.unwrap();
    
    silva.upsert_node("fake_identity", "identity", "I am a toaster", "{}").await.unwrap();

    {
        let conn_ref = silva.conn_lock();
        let conn = conn_ref.lock().await;
        conn.execute("UPDATE nodes SET topic_key = 'identity_check', conflicted = 1 WHERE id IN ('identity_god', 'fake_identity')", []).unwrap();
    }

    // Attempt consensus
    let _ = engine.consolidate(Some("identity_check")).await.unwrap();

    let god = silva.get_node("identity_god").await.unwrap().unwrap();
    assert!(god.weight >= 1.0, "Protected identity should never lose weight in consensus");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_consensus_ambiguous_zone() {
    let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
    let engine = ConsensusEngine::new(silva.clone());

    // Two nodes with almost identical weight - tight race
    silva.upsert_node("node_a", "lesson", "Option A is correct", "{\"agent\":\"a\"}").await.unwrap();
    silva.upsert_node("node_b", "lesson", "Option B is correct", "{\"agent\":\"b\"}").await.unwrap();

    {
        let conn_ref = silva.conn_lock();
        let conn = conn_ref.lock().await;
        conn.execute("UPDATE nodes SET topic_key = 'ambiguous_topic', conflicted = 1, weight = 1.0 WHERE id IN ('node_a', 'node_b')", []).unwrap();
    }

    // Run consensus - should NOT resolve, should mark as Ambiguous
    let resolved = engine.consolidate(Some("ambiguous_topic")).await.unwrap();
    assert_eq!(resolved, 0, "Ambiguous conflicts should not resolve");

    // Check that both nodes are marked as Ambiguous in metadata
    let node_a = silva.get_node("node_a").await.unwrap().unwrap();
    let meta: serde_json::Value = serde_json::from_str(&node_a.metadata).unwrap_or(serde_json::json!({}));
    assert_eq!(meta.get("status").and_then(|v| v.as_str()), Some("Ambiguous"), "Nodes should be marked Ambiguous");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_consensus_hallucination_rejected() {
    let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
    let engine = ConsensusEngine::new(silva.clone());

    // Legitimate node (will have decay applied by time passing)
    silva.upsert_node("truth_node", "lesson", "2+2=4", "{\"verified\":true}").await.unwrap();
    
    // Hallucination - low weight, no evidence
    silva.upsert_node("hallucination", "lesson", "2+2=5", "{}").await.unwrap();

    {
        let conn_ref = silva.conn_lock();
        let conn = conn_ref.lock().await;
        conn.execute("UPDATE nodes SET topic_key = 'math', conflicted = 1, weight = 1.0 WHERE id IN ('truth_node', 'hallucination')", []).unwrap();
    }

    // Apply decay to both (simulate time passing)
    silva.decay_node("truth_node", 0.8).await.unwrap();
    silva.decay_node("hallucination", 0.8).await.unwrap();

    // Run consensus - hallucination should lose badly
    let _ = engine.consolidate(Some("math")).await.unwrap();

    let hallucination = silva.get_node("hallucination").await.unwrap().unwrap();
    assert!(hallucination.weight < 0.8, "Hallucination should be penalized");
}
