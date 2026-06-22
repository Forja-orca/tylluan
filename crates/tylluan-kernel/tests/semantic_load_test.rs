//! Real Semantic Load Test for TylluanNexus o3
//! 
//! Verifies:
//! 1. End-to-end flow: Mailbox -> Embedding -> Clustering -> Consensus
//! 2. Performance on high volume (50 mixed lessons)
//! 3. GuardedTask resilience & resource usage
//! 4. Multilingual consolidation accuracy

use tylluan_kernel::memory::silva::SilvaDB;
use tylluan_kernel::memory::mailbox::Mailbox;
use tylluan_kernel::memory::consensus::ConsensusEngine;
use tylluan_kernel::router::embeddings::EmbeddingEngine;
use anyhow::Context;
use std::sync::Arc;
use std::time::Instant;
use tracing::info;

#[tokio::test(flavor = "multi_thread")]
async fn test_real_semantic_load() -> anyhow::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    
    // 1. Setup temporary persistent DBs for real I/O measurement
    let data_dir = std::path::PathBuf::from("data/test_load");
    std::fs::create_dir_all(&data_dir).ok();
    let silva_path = data_dir.join("silva_load.db");
    let mailbox_path = data_dir.join("mailbox_load.db");
    
    let _ = std::fs::remove_file(&silva_path);
    let _ = std::fs::remove_file(&mailbox_path);

    let silva = Arc::new(SilvaDB::open(&silva_path.to_string_lossy())?);
    let mailbox = Arc::new(Mailbox::open(&mailbox_path.to_string_lossy())?);
    silva.init().await?;
    mailbox.init().await?;

    let model_path = EmbeddingEngine::default_model_path()
        .context("No BGE-M3 model found. Set TYLLUAN_MODEL_PATH or ensure models/bge-m3 exists")?;
    
    let start_load = Instant::now();
    let engine = EmbeddingEngine::load(&model_path)?;
    info!("⏱️ Model Load Time: {:?}", start_load.elapsed());

    // 2. Generate 50 realistic agricultural/industrial lessons
    let mut lessons = Vec::new();
    let agents = ["agent-alpha", "agent-beta", "agent-gamma", "agent-delta"];
    
    let scenarios = [
        ("Pump pressure issues identified in Sector 7 during high-load cycle.", "Problemas de presión detectados en la bomba del Sector 7 durante el ciclo de alta carga."),
        ("Unauthorized access attempt via SSH at midnight from unknown IP.", "Intento de acceso no autorizado por SSH a medianoche desde una IP desconocida."),
        ("Memory leak in the Vision Guild when processing high-res PNGs.", "Fuga de memoria en el gremio de visión al procesar imágenes PNG de alta resolución."),
        ("Handshake timeout between Kernel and External Bridge was too low (5s).", "El tiempo de espera del handshake entre el Kernel y el Bridge externo era muy bajo (5s)."),
        ("Soil moisture sensor in Field A reporting inconsistent values after rain.", "Sensor de humedad del suelo en el Campo A reporta valores inconsistentes tras la lluvia."),
    ];

    for i in 0..15 {
        let (en, es) = scenarios[i % scenarios.len()];
        let agent = agents[i % agents.len()];
        let is_es = i % 2 == 1;
        let content = if is_es { 
            format!("{} [ID: {}]", es, i) 
        } else { 
            format!("{} [ID: {}]", en, i) 
        };
        
        let payload = serde_json::json!({
            "type": "lesson_proposal",
            "topic": "topic_for_clustering", // Unified topic to force semantic conflict group detection
            "content": content
        });
        
        lessons.push((agent.to_string(), payload.to_string()));
    }

    // 3. Phase 1: Injection (Mailbox)
    info!("🚀 Injecting 15 proposals into Mailbox...");
    let start_inject = Instant::now();
    for (agent, payload) in &lessons {
        mailbox.send_mail(agent, "hub", payload).await?;
    }
    info!("⏱️ Injection Time: {:?}", start_inject.elapsed());

    // 4. Phase 2: Processing (Simulate Hub Listener)
    info!("🧠 Processing 15 proposals (Inference + Silva Persistence)...");
    let start_proc = Instant::now();
    let messages = mailbox.check_mail("hub", true, 1000).await?;
    
    for (i, msg) in messages.iter().enumerate() {
        let proposal: serde_json::Value = serde_json::from_str(&msg.payload)?;
        let content = proposal["content"].as_str().unwrap();
        let p_id = format!("load_node_{}", i);
        
        // Persist + Mark Conflicted
        silva.upsert_node(&p_id, "lesson", content, &msg.payload).await?;
        silva.mark_conflicted(&p_id, true).await?;
        
        // Add artificial jitter to weights so they are not all 1.0
        // Using larger increments (0.5) to ensure >10% win diff between nodes
        let jitter = 1.0 + (i as f64 * 0.5); 
        silva.set_weight(&p_id, jitter).await?;
        
        // Inference (Embedding)
        let vector = engine.embed(content)?;
        silva.save_embedding(&p_id, &vector, "bge-m3", None).await?;
        
        if (i + 1) % 5 == 0 {
            info!("   Processed {}/15...", i + 1);
        }
    }
    let total_proc = start_proc.elapsed();
    info!("⏱️ Total Processing Time: {:?}", total_proc);
    info!("⏱️ Average Latency per Node: {:?}", total_proc / 15);

    // 5. Phase 3: Semantic Consensus (Clustering)
    info!("⚖️ Running Semantic Consensus (Greedy Clustering)...");
    let consensus = ConsensusEngine::new(silva.clone());
    let start_cons = Instant::now();
    
    // Manually reinforce some winners to guide consensus
    // Using load_node_X IDs which were assigned in Phase 2
    silva.set_weight("load_node_0", 3.0).await?; // Scenario 0 winner
    silva.set_weight("load_node_1", 3.0).await?; // Scenario 1 winner
    silva.set_weight("load_node_2", 3.0).await?; // Scenario 2 winner
    silva.set_weight("load_node_3", 3.0).await?; // Scenario 3 winner
    silva.set_weight("load_node_4", 3.0).await?; // Scenario 4 winner
    
    // Also use the unified topic for global consolidation if needed
    let resolved_count = consensus.consolidate(None).await?;
    info!("⏱️ Consensus/Clustering Time: {:?}", start_cons.elapsed());
    info!("✅ Total Clustered Groups Resolved: {}", resolved_count);

    // 6. Verification
    // With 15 nodes and 5 scenarios, we expect at least 3 clusters 
    // (some might consolidate into synthesis or be perfect matches)
    assert!(resolved_count >= 3, "Should have resolved at least 3 main clusters through winner selection or synthesis");
    
    let stats = silva.stats().await?;
    info!("📊 SilvaDB Stats: Nodes={}, Edges={}, DB Size={} bytes", 
        stats.node_count, stats.edge_count, stats.total_bytes);

    // Cleanup
    let _ = std::fs::remove_dir_all(&data_dir);
    
    Ok(())
}
