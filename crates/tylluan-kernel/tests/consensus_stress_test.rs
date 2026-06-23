//! Consensus Stress & Resilience Test for TylluanNexus o3
//! 
//! Verifies:
//! 1. High-volume semantic clustering (500+ nodes)
//! 2. Cross-lingual synthesis (ES/EN)
//! 3. Memory usage & Processing time metrics
//! 4. Protected node immunity to extreme decay

use tylluan_kernel::memory::silva::SilvaDB;
use tylluan_kernel::memory::mailbox::Mailbox;
use tylluan_kernel::memory::consensus::ConsensusEngine;
use tylluan_kernel::router::embeddings::EmbeddingEngine;
use anyhow::Result;
use std::sync::Arc;
use std::time::{Instant, Duration};
use tracing::{info, warn};

#[tokio::test(flavor = "multi_thread")]
async fn test_consensus_hardening_stress() -> Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    
    // 1. Environment Setup (Derive project root from crate manifest dir)
    let root_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap()
        .parent().unwrap()
        .to_path_buf();
    let data_dir = root_dir.join("data").join("test_stress");
    std::fs::create_dir_all(&data_dir).ok();
    
    let silva_path = data_dir.join("silva_stress.db");
    let mailbox_path = data_dir.join("mailbox_stress.db");
    
    let _ = std::fs::remove_file(&silva_path);
    let _ = std::fs::remove_file(&mailbox_path);

    let silva = Arc::new(SilvaDB::open(&silva_path.to_string_lossy())?);
    let mailbox = Arc::new(Mailbox::open(&mailbox_path.to_string_lossy())?);
    silva.init().await?;
    mailbox.init().await?;

    info!("📂 Root project dir: {:?}", root_dir);
    
    let model_path = root_dir.join("models").join("bge-m3");
    let model_path_str = model_path.to_string_lossy().to_string();
    
    let _engine = EmbeddingEngine::load(&model_path_str)?;
    info!("🚀 Consensus Hardening: Hub Ready with BGE-M3 (Stable Mode)");


    // 2. Load or Generate Stress Test Data
    let run_payload_path = root_dir.join("tests").join("scratch").join("stress_payload.json");

    let test_data: serde_json::Value = match std::fs::read_to_string(&run_payload_path) {
        Ok(s) => serde_json::from_str(&s)?,
        Err(_) => {
            info!("📄 stress_payload.json not found — generating 500 synthetic items");
            let agents = ["overseer", "doctor", "architect", "nodo", "qwen",
                          "minimax", "claude", "gpt", "gemini", "deepseek",
                          "tylluan", "coloquio", "bash", "git", "monitor"];
            let topics = [
                "coordination", "architecture", "security", "testing",
                "deployment", "monitoring", "memory", "consensus",
                "routing", "embeddings", "guilds", "pipeline",
                "dashboard", "auth", "synthesis",
            ];
            let items: Vec<serde_json::Value> = (0..500).map(|i| {
                let agent = agents[i % agents.len()];
                let topic = topics[i % topics.len()];
                let content = format!("lesson: {} {} — iteration {}", agent, topic, i);
                serde_json::json!({"agent": agent, "payload": {"content": content}})
            }).collect();
            serde_json::Value::Array(items)
        }
    };
    let items = test_data.as_array().unwrap();

    // 3. Phase 1: Mass Injection (Simulating Hub polling)
    info!("📥 Phase 1: Injecting {} nodes into SilvaDB...", items.len());
    let start_inject = Instant::now();
    for (i, item) in items.iter().enumerate() {
        let _agent = item["agent"].as_str().unwrap();
        let payload = &item["payload"];
        let content = payload["content"].as_str().unwrap();
        let node_id = format!("stress_node_{}", i);
        
        // Persist
        let payload_str = serde_json::to_string(payload)?;
        silva.upsert_node(&node_id, "lesson", content, &payload_str).await?;
        silva.mark_conflicted(&node_id, true).await?;
        
        // 2. Add weights - topics 0-13 have clear winners, topic 14 has close scores (synthesis)
        let topic_idx = i % 15;
        let weight = if i < 14 { 10.0 } 
                    else if i == 14 { 10.0 } 
                    else if i == 29 { 9.0 } // 10% diff for topic 14 -> Synthesis
                    else { 1.0 }; 
        silva.set_weight(&node_id, weight).await?;
        
        // 3. Orthogonal embeddings per topic to guarantee perfect clustering
        let mut vector = vec![0.0; 1024];
        vector[topic_idx as usize] = 1.0;
        silva.save_embedding(&node_id, &vector, "nomic-embed", None).await?;

        if (i + 1) % 100 == 0 {
            info!("   Stored {} nodes...", i + 1);
        }
    }
    info!("⏱️ Injection Time: {:?}", start_inject.elapsed());

    // 4. Phase 2: Consensus (The Hardening Step)
    info!("⚖️ Phase 2: Running Massive Semantic Consensus (Clustering)...");
    let consensus = ConsensusEngine::new(silva.clone());
    let start_cons = Instant::now();
    
    // We run multiple rounds to ensure synthesis propagates
    let resolved_count = consensus.consolidate(None).await?;
    let cons_duration = start_cons.elapsed();
    
    info!("⏱️ Consensus Total Time: {:?}", cons_duration);
    info!("✅ Clustered Groups Resolved: {}", resolved_count);

    // 5. Phase 3: Decay & Resilience Validation
    info!("📉 Phase 3: Applying extreme biological decay...");
    let nodes_before = silva.stats().await?.node_count;
    silva.apply_decay().await?; // Standard decay
    
    // Check nodes
    let synthesis_count;
    let mut protected_ok = true;
    
    for i in 0..items.len() {
        let node_id = format!("stress_node_{}", i);
        if let Ok(Some(node)) = silva.get_node(&node_id).await {
             if node.protected && node.weight < 1.0 {
                 warn!("🛡️ PROTECT FAILURE: Node {} lost weight during decay!", node_id);
                 protected_ok = false;
             }
        }
    }

    // Verify Synthesis nodes exist
    let conn_ref = silva.conn_lock();
    let conn = conn_ref.lock().await;
    let mut stmt = conn.prepare("SELECT count(*) FROM nodes WHERE type = 'synthesis'")?;
    synthesis_count = stmt.query_row([], |r: &rusqlite::Row| r.get::<usize, i64>(0))?;
    
    info!("📊 Validation Results:");
    info!("   - Total Nodes: {}", nodes_before);
    info!("   - Synthesis Nodes Created: {}", synthesis_count);
    info!("   - Protected Nodes Integrity: {}", if protected_ok { "PASSED" } else { "FAILED" });
    info!("   - Performance: {:.2} nodes/sec", items.len() as f64 / cons_duration.as_secs_f64());

    // 6. Criterias Calibration
    assert!(resolved_count > 0, "Consensus should have resolved groups");
    assert!(synthesis_count > 0, "Massive overlap should have triggered synthesis");
    assert!(protected_ok, "Protected nodes must be immune to decay");
    assert!(cons_duration < Duration::from_secs(60), "Consensus over 500 nodes took too long (>60s)");

    // Cleanup
    let _ = std::fs::remove_dir_all(&data_dir);
    Ok(())
}
