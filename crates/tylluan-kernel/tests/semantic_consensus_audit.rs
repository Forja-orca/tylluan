use tylluan_kernel::memory::silva::SilvaDB;
use tylluan_kernel::memory::consensus::ConsensusEngine;
use std::sync::Arc;

#[tokio::test(flavor = "multi_thread")]
async fn test_semantic_clustering_clash() {
    let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
    let engine = ConsensusEngine::new(silva.clone());

    // Nodos semánticamente similares pero con topic_key diferente (o nulo)
    // Usamos contenidos que generarán embeddings cercanos en un sistema real,
    // pero aquí simulamos la inyección de embeddings manualmente para el test de integración del motor.
    
    silva.upsert_node("node_en", "lesson", "The server port is 3000", "{\"agent\":\"agent_en\"}").await.unwrap();
    silva.upsert_node("node_es", "lesson", "El puerto del servidor es 8080", "{\"agent\":\"agent_es\"}").await.unwrap();

    // Inyectamos embeddings manualmente (simulando que BGE-M3 detectó similitud o forzando colisión)
    // Para simplificar el test, inyectamos vectores idénticos para forzar el clustering > 0.85
    let mock_emb = vec![1.0; 1024]; 
    silva.save_embedding("node_en", &mock_emb, "test", None).await.unwrap();
    silva.save_embedding("node_es", &mock_emb, "test", None).await.unwrap();

    // Marcamos ambos como conflicted para que el motor los recoja
    silva.mark_conflicted("node_en", true).await.unwrap();
    silva.mark_conflicted("node_es", true).await.unwrap();

    // Run consolidation - it will reinforce the winner automatically based on weight
    // Set weight directly to make EN clearly win (weight * trust > runner_up * 1.1)
    silva.set_weight("node_en", 5.0).await.unwrap();

    // Ejecutamos consolidación global (sin topic_key)
    let resolved = engine.consolidate(None).await.unwrap();
    assert!(resolved > 0, "Should have resolved the semantic cluster");

    let win = silva.get_node("node_en").await.unwrap().unwrap();
    let lose = silva.get_node("node_es").await.unwrap().unwrap();

    assert!(win.weight > 1.0, "Winner should be reinforced");
    assert!(lose.weight < 1.0, "Loser in semantic cluster should decay");
    assert!(!win.conflicted, "Winner should no longer be conflicted");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_semantic_multilingual_resolution() {
    let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
    let engine = ConsensusEngine::new(silva.clone());

    // Escenario: "User was deleted" vs "Usuario fue borrado"
    silva.upsert_node("task_en", "lesson", "User account deleted", "{\"agent\":\"admin_en\"}").await.unwrap();
    silva.upsert_node("task_es", "lesson", "Cuenta de usuario eliminada", "{\"agent\":\"admin_es\"}").await.unwrap();

    // Forzamos embedding muy similar (0.99)
    let emb_a = vec![0.5; 1024];
    let mut emb_b = emb_a.clone();
    emb_b[0] = 0.51; // Ligerísima variación

    silva.save_embedding("task_en", &emb_a, "test", None).await.unwrap();
    silva.save_embedding("task_es", &emb_b, "test", None).await.unwrap();
    
    silva.mark_conflicted("task_en", true).await.unwrap();
    silva.mark_conflicted("task_es", true).await.unwrap();

    // Ejecutamos consolidación
    let _ = engine.consolidate(None).await.unwrap();

    let _node_a = silva.get_node("task_en").await.unwrap().unwrap();
    let _node_b = silva.get_node("task_es").await.unwrap().unwrap();

    // Uno debe haber ganado y otro perdido (o ambos Ambiguous si empatan)
    // Como ambos tienen peso 1.0 inicial, deberían quedar como Ambiguous
    let meta_a: serde_json::Value = serde_json::from_str(&_node_a.metadata).unwrap();
    assert_eq!(meta_a.get("status").and_then(|v| v.as_str()), Some("Ambiguous"), "Close scores should trigger Ambiguity");
}
