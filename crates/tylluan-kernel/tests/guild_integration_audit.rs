use tylluan_kernel::memory::silva::SilvaDB;
use tylluan_kernel::memory::mailbox::Mailbox;
use tylluan_kernel::memory::consensus::ConsensusEngine;
use std::sync::Arc;

#[tokio::test(flavor = "multi_thread")]
async fn test_guild_to_consensus_flow() {
    let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
    let mailbox = Arc::new(Mailbox::in_memory().await.unwrap());
    let engine = ConsensusEngine::new(silva.clone());
    let _node_b = "test".to_string();

    // 1. Simular Guild enviando propuesta
    let proposal = serde_json::json!({
        "type": "lesson_proposal",
        "topic": "rust_safety",
        "content": "Rust is memory safe",
        "evidence": {"source": "official_docs"}
    }).to_string();

    mailbox.send_mail("agent_rust", "hub", &proposal).await.unwrap();

    // 2. Simular el Hub Listener (lo que hace main.rs)
    let messages = mailbox.check_mail("hub", true, 1000).await.unwrap();
    assert_eq!(messages.len(), 1);

    for msg in messages {
        let p: serde_json::Value = serde_json::from_str(&msg.payload).unwrap();
        let p_id = format!("proposal_{}", msg.sender_id);
        silva.upsert_node(&p_id, "lesson", p["content"].as_str().unwrap(), &msg.payload).await.unwrap();
        // Force conflict and topic key for test resolution
        {
let conn_ref = silva.conn_lock();
        let conn = conn_ref.lock().await;
            conn.execute("UPDATE nodes SET topic_key = 'rust_safety', conflicted = 1 WHERE id = ?1", [p_id]).unwrap();
        }
    }

    // 3. Simular conflicto con otro agente
    silva.upsert_node("prop_b", "lesson", "C++ is safer than Rust", "{\"topic\":\"rust_safety\"}").await.unwrap();
    {
        let conn_ref = silva.conn_lock();
        let conn = conn_ref.lock().await;
        conn.execute("UPDATE nodes SET topic_key = 'rust_safety', conflicted = 1 WHERE id IN ('proposal_agent_rust', 'prop_b')", []).unwrap();
    }

    // 4. Reinforce Agent Rust to ensure clear victory (>10% margin)
    silva.reinforce_node("proposal_agent_rust", 1.5).await.unwrap();

    // 5. Ejecutar Consenso
    let resolved = engine.consolidate(Some("rust_safety")).await.unwrap();
    assert!(resolved > 0, "Consensus should resolve the guild proposal with clear margin");

    let winner = silva.get_node("proposal_agent_rust").await.unwrap().unwrap();
    assert!(!winner.conflicted, "Guild lesson should be resolved");
}
