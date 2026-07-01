use super::*;
use rusqlite::params;

async fn test_silva() -> SilvaDB {
        SilvaDB::in_memory().await.unwrap()
    }

    #[test]
    fn test_cosine_similarity_basic() {
        // (1.0, 0.0) vector represented as little-endian f32 bytes (4 bytes per f32)
        // 1.0_f32 = [0, 0, 128, 63]
        // 0.0_f32 = [0, 0, 0, 0]
        let v1: Vec<u8> = vec![0, 0, 128, 63, 0, 0, 0, 0]; // [1.0, 0.0]
        let v2: Vec<u8> = vec![0, 0, 0, 0, 0, 0, 128, 63]; // [0.0, 1.0]
        let v3: Vec<u8> = vec![0, 0, 128, 63, 0, 0, 0, 0]; // [1.0, 0.0]

        // Similarity of orthogonal vectors should be 0.0
        let sim_12 = cosine_similarity(&v1, &v2);
        assert!((sim_12 - 0.0).abs() < 1e-6, "Orthogonal similarity should be 0.0, got {}", sim_12);

        // Similarity of identical vectors should be 1.0
        let sim_13 = cosine_similarity(&v1, &v3);
        assert!((sim_13 - 1.0).abs() < 1e-6, "Identical similarity should be 1.0, got {}", sim_13);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_upsert_and_get() {
        let db = test_silva().await;
        db.upsert_node("rust-lang", "concept", "Rust programming language", "{}").await.unwrap();

        let node = db.get_node("rust-lang").await.unwrap().unwrap();
        assert_eq!(node.id, "rust-lang");
        assert_eq!(node.node_type, "concept");
        assert!((node.weight - 1.0).abs() < f64::EPSILON);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_personalized_pagerank_local_basic() {
        let db = test_silva().await;
        
        // Add nodes
        db.upsert_node("seed", "concept", "Seed node", "{}").await.unwrap();
        db.upsert_node("a", "concept", "Node A", "{}").await.unwrap();
        db.upsert_node("b", "concept", "Node B", "{}").await.unwrap();
        db.upsert_node("c", "concept", "Node C", "{}").await.unwrap();
        db.upsert_node("orphan", "concept", "Isolated Node", "{}").await.unwrap();
        
        // Add edges: seed -> a, a -> b, b -> c
        db.add_edge("seed", "a", "links_to", 1.0, "{}").await.unwrap();
        db.add_edge("a", "b", "links_to", 1.0, "{}").await.unwrap();
        db.add_edge("b", "c", "links_to", 1.0, "{}").await.unwrap();
        
        let seeds = vec!["seed".to_string()];
        let results = db.personalized_pagerank_local(&seeds, 0.85, 20, 10).await.unwrap();
        
        // Output should not contain "seed", and should contain "a", "b" (since they are within 2 hops).
        // "c" is 3 hops away, and "orphan" is isolated, so neither should be included.
        assert!(!results.iter().any(|(id, _)| id == "seed"));
        assert!(!results.iter().any(|(id, _)| id == "c"));
        assert!(!results.iter().any(|(id, _)| id == "orphan"));
        
        let result_ids: Vec<String> = results.iter().map(|(id, _)| id.clone()).collect();
        assert!(result_ids.contains(&"a".to_string()));
        assert!(result_ids.contains(&"b".to_string()));
        
        // Score validation: "a" should have higher PageRank than "b" because it's directly connected from the seed.
        let pr_a = results.iter().find(|(id, _)| id == "a").map(|(_, score)| *score).unwrap();
        let pr_b = results.iter().find(|(id, _)| id == "b").map(|(_, score)| *score).unwrap();
        assert!(pr_a > pr_b);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_weight_reinforcement() {
        let db = test_silva().await;
        db.upsert_node("test", "concept", "initial", "{}").await.unwrap();
        db.upsert_node("test", "concept", "updated", "{}").await.unwrap();
        db.upsert_node("test", "concept", "updated again", "{}").await.unwrap();

        let node = db.get_node("test").await.unwrap().unwrap();
        // Concurrent-safe upsert: weight = MAX(existing, new) — no blind increment
        assert!((node.weight - 1.0).abs() < 0.01);
        assert_eq!(node.content, "updated again");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_add_edge_and_count() {
        let db = test_silva().await;
        db.upsert_node("a", "entity", "Node A", "{}").await.unwrap();
        db.upsert_node("b", "entity", "Node B", "{}").await.unwrap();
        db.add_edge("a", "b", "relates_to", 1.0, "{}").await.unwrap();

        assert_eq!(db.edge_count().await.unwrap(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_search_by_content() {
        let db = test_silva().await;
        db.upsert_node("n1", "concept", "Machine Learning algorithms", "{}").await.unwrap();
        db.upsert_node("n2", "lesson", "Always write tests", "{}").await.unwrap();
        db.upsert_node("n3", "concept", "Deep Learning neural nets", "{}").await.unwrap();

        let results = db.search("learning", 10, None).await.unwrap();
        assert_eq!(results.len(), 2); // ML and DL
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_search_with_type_filter() {
        let db = test_silva().await;
        db.upsert_node("n1", "concept", "Rust language", "{}").await.unwrap();
        db.upsert_node("n2", "lesson", "Rust lesson learned", "{}").await.unwrap();

        let results = db.search("rust", 10, Some(&["lesson"])).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].node_type, "lesson");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_bfs_context() {
        let db = test_silva().await;
        db.upsert_node("a", "entity", "Root", "{}").await.unwrap();
        db.upsert_node("b", "entity", "Neighbor", "{}").await.unwrap();
        db.upsert_node("c", "entity", "Two hops away", "{}").await.unwrap();

        db.add_edge("a", "b", "connects", 1.0, "{}").await.unwrap();
        db.add_edge("b", "c", "connects", 1.0, "{}").await.unwrap();

        // Depth 0: only a
        let ctx0 = db.get_context("a", 0).await.unwrap();
        assert_eq!(ctx0.len(), 1);

        // Depth 1: a + b
        let ctx1 = db.get_context("a", 1).await.unwrap();
        assert_eq!(ctx1.len(), 2);

        // Depth 2: a + b + c
        let ctx2 = db.get_context("a", 2).await.unwrap();
        assert_eq!(ctx2.len(), 3);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_shortest_path_returns_ordered_nodes() {
        let db = test_silva().await;
        db.upsert_node("a", "entity", "A", "{}").await.unwrap();
        db.upsert_node("b", "entity", "B", "{}").await.unwrap();
        db.upsert_node("c", "entity", "C", "{}").await.unwrap();
        db.upsert_node("d", "entity", "D", "{}").await.unwrap();

        db.add_edge("a", "b", "connects", 1.0, "{}").await.unwrap();
        db.add_edge("b", "c", "connects", 1.0, "{}").await.unwrap();

        let path = db.shortest_path("a", "c", 3).await.unwrap();
        assert_eq!(path, Some(vec!["a".to_string(), "b".to_string(), "c".to_string()]));

        let too_shallow = db.shortest_path("a", "c", 1).await.unwrap();
        assert_eq!(too_shallow, None);

        let disconnected = db.shortest_path("a", "d", 3).await.unwrap();
        assert_eq!(disconnected, None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_decay() {
        let db = test_silva().await;
        db.upsert_node("old", "concept", "Old node", "{}").await.unwrap();
        // Force old timestamp
        {
            let conn = db.conn.lock().await;
            conn.execute(
                "UPDATE nodes SET updated_at = datetime('now', '-10 days') WHERE id = 'old'",
                [],
            ).unwrap();
        }

        let _changes = db.apply_decay(336).await.unwrap();
        assert!(_changes > 0);

        let node = db.get_node("old").await.unwrap().unwrap();
        assert!(node.weight < 1.0); // Should have decayed
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_node_not_found() {
        let db = test_silva().await;
        assert!(db.get_node("nonexistent").await.unwrap().is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_identity_node_is_protected_from_decay() {
        let db = test_silva().await;
        
        // Create an identity node (should be protected automatically)
        db.upsert_node("agent:megaingeniero", "identity", "MegaIngeniero - Senior Engineer", "{}").await.unwrap();
        
        // Force old timestamp to trigger decay
        {
            let conn = db.conn.lock().await;
            conn.execute(
                "UPDATE nodes SET updated_at = datetime('now', '-10 days') WHERE id = 'agent:megaingeniero'",
                [],
            ).unwrap();
        }
        
        // Apply decay
        let _changes = db.apply_decay(336).await.unwrap();
        
        // Identity node should NOT be affected (protected flag should be auto-set for identity type)
        let node = db.get_node("agent:megaingeniero").await.unwrap().unwrap();
        assert!((node.weight - 1.0).abs() < 0.01, "Identity node weight should remain 1.0, got {}", node.weight);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_protected_nodes_immune_to_decay() {
        let db = test_silva().await;
        
        // Create a regular node and mark it protected
        db.upsert_node("secret-plan", "concept", "Secret strategy", "{}").await.unwrap();
        db.set_protected("secret-plan", true).await.unwrap();
        
        // Force old timestamp
        {
            let conn = db.conn.lock().await;
            conn.execute(
                "UPDATE nodes SET updated_at = datetime('now', '-10 days') WHERE id = 'secret-plan'",
                [],
            ).unwrap();
        }
        
        // Apply decay - should not affect protected node
        db.apply_decay(336).await.unwrap();
        
        let node = db.get_node("secret-plan").await.unwrap().unwrap();
        assert!((node.weight - 1.0).abs() < 0.01, "Protected node should remain 1.0, got {}", node.weight);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_right_to_be_forgotten() {
        let db = test_silva().await;
        
        // Create nodes with agent metadata
        db.upsert_node("n1", "concept", "Thought from Agent", r#"{"agent":"agent-1"}"#).await.unwrap();
        db.upsert_node("n2", "lesson", "Lesson from Agent", r#"{"agent":"agent-1"}"#).await.unwrap();
        db.upsert_node("n3", "concept", "Thought from Other", r#"{"agent":"other-agent"}"#).await.unwrap();
        
        // Forget agent agent-1
        let deleted = db.forget_agent("agent-1").await.unwrap();
        
        assert_eq!(deleted, 2, "Should have deleted 2 nodes for agent-1");
        
        // Verify other agent's node remains
        let remaining = db.get_node("n3").await.unwrap();
        assert!(remaining.is_some(), "Other agent's node should remain");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_semantic_search_silva() {
        let db = test_silva().await;
        
        // Create nodes
        db.upsert_node("n1", "concept", "Rust programming", "{}").await.unwrap();
        db.upsert_node("n2", "concept", "Python scripting", "{}").await.unwrap();
        
        // Add fake embeddings (3-dim)
        let emb1 = vec![1.0_f32, 0.0, 0.0];
        let emb2 = vec![0.0_f32, 1.0, 0.0];
        db.save_embedding("n1", &emb1, "test-model", None).await.unwrap();
        db.save_embedding("n2", &emb2, "test-model", None).await.unwrap();
        
        // Search with query similar to emb1
        let query_emb = vec![0.9_f32, 0.1, 0.0];
        let results = db.search_vector(&query_emb, 5).await.unwrap();
        
        assert!(!results.is_empty());
        assert_eq!(results[0].0.id, "n1");
        assert!(results[0].1 > 0.8);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_hybrid_search_silva() {
        let db = test_silva().await;
        
        db.upsert_node("n1", "lesson", "Consensus is key", "{}").await.unwrap();
        let emb1 = vec![1.0_f32, 0.0, 0.0];
        db.save_embedding("n1", &emb1, "test-model", None).await.unwrap();
        
        // Hybrid search: query "consensus" (text match) + semantic match
        let query_emb = vec![0.95_f32, 0.05, 0.0];
        let results = db.search_hybrid("consensus", Some(&query_emb), 5, None, false).await.unwrap();
        
        assert!(!results.is_empty());
        assert_eq!(results[0].0.id, "n1");
        // RRF scores are small (max ~0.033) but n1 must rank first — both text and vector match
        assert!(results[0].1 > 0.0);
        assert!(results[0].1 >= results.get(1).map(|r| r.1).unwrap_or(0.0));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_triples_by_entity() {
        let db = test_silva().await;
        db.upsert_node("s", "entity", "Subj", "{}").await.unwrap();
        db.upsert_node("o", "entity", "Obj", "{}").await.unwrap();
        db.add_edge("s", "o", "pred", 1.0, "{}").await.unwrap();
        
        let triples = db.get_triples_by_entity("s").await.unwrap();
        assert_eq!(triples.len(), 1);
        assert_eq!(triples[0]["predicate"], "pred");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_stigmergy_heat() {
        let db = test_silva().await;
        db.upsert_node("n1", "concept", "Node", "{}").await.unwrap();
        db.touch_node("n1", "agent1", "read").await.unwrap();
        db.touch_node("n1", "agent2", "read").await.unwrap();
        
        let heat = db.get_stigmergy_heat("n1", 1).await.unwrap();
        assert!(heat > 0.0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_prune_cold_nodes() {
        let db = test_silva().await;
        db.upsert_node("hot", "concept", "Hot", "{}").await.unwrap();
        db.upsert_node("cold", "concept", "Cold", "{}").await.unwrap();
        
        // Lower weight of cold node
        {
            let conn = db.conn.lock().await;
            conn.execute("UPDATE nodes SET weight = 0.05 WHERE id = 'cold'", []).unwrap();
        }
        
        let pruned = db.prune_cold_nodes(0.1).await.unwrap();
        assert_eq!(pruned, 1);
        
        assert!(db.get_node("hot").await.unwrap().is_some());
        assert!(db.get_node("cold").await.unwrap().is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_triples_for_entity() {
        let db = test_silva().await;
        db.upsert_node("entity1", "concept", "Entity One", "{}").await.unwrap();
        db.upsert_node("entity2", "concept", "Entity Two", "{}").await.unwrap();
        db.add_edge("entity1", "entity2", "relates_to", 1.0, "{}").await.unwrap();
        
        let triples = db.get_triples_for_entity("entity1").await.unwrap();
        assert!(!triples.is_empty());
        assert!(triples.iter().any(|(s, p, _o)| s == "entity1" && p == "relates_to"));
        
        let triples2 = db.get_triples_for_entity("entity2").await.unwrap();
        assert!(!triples2.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_top_hot_nodes() {
        let db = test_silva().await;
        db.upsert_node("hot1", "concept", "Hot Node 1", "{}").await.unwrap();
        db.upsert_node("hot2", "concept", "Hot Node 2", "{}").await.unwrap();
        
        db.touch_node("hot1", "agent1", "read").await.unwrap();
        db.touch_node("hot1", "agent2", "read").await.unwrap();
        db.touch_node("hot1", "agent3", "read").await.unwrap();
        
        db.touch_node("hot2", "agent1", "write").await.unwrap();
        
        let hot = db.top_hot_nodes(10).await.unwrap();
        assert!(!hot.is_empty());
        let hot1_count = hot.iter().find(|(id, _)| id == "hot1").map(|(_, c)| *c).unwrap_or(0);
        assert!(hot1_count >= 3);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_search_content() {
        let db = test_silva().await;
        db.upsert_node("n1", "concept", "Rust programming language", "{}").await.unwrap();
        db.upsert_node("n2", "concept", "Python is great", "{}").await.unwrap();
        db.upsert_node("n3", "concept", "Rust tokio async runtime", "{}").await.unwrap();
        
        let results = db.search_content("Rust", 10).await.unwrap();
        assert_eq!(results.len(), 2);
        
        let results2 = db.search_content("Python", 10).await.unwrap();
        assert_eq!(results2.len(), 1);
        
        let results3 = db.search_content("nonexistent", 10).await.unwrap();
        assert!(results3.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_ebbinghaus_decay_faster_than_linear() {
        let db = test_silva().await;
        db.upsert_node("n1", "concept", "Ebbinghaus Node", "{}").await.unwrap();
        
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        
        // Manually set last_touched to 24 hours ago
        {
            let conn = db.conn.lock().await;
            conn.execute(
                "UPDATE nodes SET last_touched = ?1, weight = 1.0 WHERE id = 'n1'",
                params![now - 86400],
            ).unwrap();
        }
        
        let new_weight = db.apply_node_decay("n1").await.unwrap();
        
        // Ebbinghaus base: lambda = 0.05
        // hours = 24
        // new_weight = 1.0 * exp(-0.05 * 24) = exp(-1.2) ≈ 0.30119
        assert!((new_weight - 0.30119).abs() < 0.01);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_hub_node_decays_slower() {
        let db = test_silva().await;
        db.upsert_node("hub", "concept", "Hub Node", "{}").await.unwrap();
        db.upsert_node("n1", "concept", "Node 1", "{}").await.unwrap();
        db.upsert_node("n2", "concept", "Node 2", "{}").await.unwrap();
        db.upsert_node("n3", "concept", "Node 3", "{}").await.unwrap();
        
        // Add 3 incoming edges to 'hub'
        db.add_edge("n1", "hub", "ref", 1.0, "{}").await.unwrap();
        db.add_edge("n2", "hub", "ref", 1.0, "{}").await.unwrap();
        db.add_edge("n3", "hub", "ref", 1.0, "{}").await.unwrap();
        
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        
        // Manually set last_touched to 24 hours ago
        {
            let conn = db.conn.lock().await;
            conn.execute(
                "UPDATE nodes SET last_touched = ?1, weight = 1.0 WHERE id = 'hub'",
                params![now - 86400],
            ).unwrap();
        }
        
        let new_weight = db.apply_node_decay("hub").await.unwrap();
        
        // in_degree = 3
        // protection = 1.0 / (1.0 + ln(3).max(0)) = 1.0 / (1.0 + 1.0986) ≈ 0.4765
        // lambda_eff = 0.05 * 0.4765 ≈ 0.0238
        // new_weight = 1.0 * exp(-0.0238 * 24) = exp(-0.5718) ≈ 0.5645
        // This is significantly higher than 0.301! Let's assert it decays slower.
        assert!(new_weight > 0.50);
        assert!(new_weight < 0.65);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_consolidate_episodes_merges_similar() {
        let db = test_silva().await;

        db.upsert_node("ep1", "episode", "the quick brown fox jumps", "{}").await.unwrap();
        db.upsert_node("ep2", "episode", "the quick brown fox leaps", "{}").await.unwrap();
        db.upsert_node("ep3", "episode", "docker container orchestration", "{}").await.unwrap();

        let merged = db.consolidate_episodes(0.5, 10).await.unwrap();
        assert_eq!(merged, 1, "ep1 and ep2 should merge");

        let n1 = db.get_node("ep1").await.unwrap().unwrap();
        assert!((n1.weight - 0.02).abs() < f64::EPSILON, "ep1 should be soft-deprecated to 0.02");

        let n2 = db.get_node("ep2").await.unwrap().unwrap();
        assert!((n2.weight - 0.02).abs() < f64::EPSILON, "ep2 should be soft-deprecated to 0.02");

        let combined = "ep1:ep2";
        let hash: u64 = combined.bytes().fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
        let concept_id = format!("concept:merged:{:x}", hash);
        let concept = db.get_node(&concept_id).await.unwrap().unwrap();
        assert_eq!(concept.node_type, "concept");
        assert!((concept.weight - 1.15).abs() < 0.01, "concept weight should be (1.0+1.0)/2*1.15 = 1.15");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_consolidate_episodes_empty_batch() {
        let db = test_silva().await;
        let merged = db.consolidate_episodes(0.9, 0).await.unwrap();
        assert_eq!(merged, 0, "empty batch should return 0");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_trace_count_by_type_basic() {
        let db = test_silva().await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Insert traces via conn_lock direct SQL
        {
            let conn = db.conn_lock();
            let c = conn.lock().await;
            c.execute(
                "INSERT INTO node_traces (node_id, agent_id, touched_at, trace_type) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params!["test_node", "agent_a", now - 100, "executed"],
            ).unwrap();
            c.execute(
                "INSERT INTO node_traces (node_id, agent_id, touched_at, trace_type) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params!["test_node", "agent_a", now - 200, "executed"],
            ).unwrap();
            c.execute(
                "INSERT INTO node_traces (node_id, agent_id, touched_at, trace_type) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params!["test_node", "agent_b", now - 300, "rejected"],
            ).unwrap();
            c.execute(
                "INSERT INTO node_traces (node_id, agent_id, touched_at, trace_type) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params!["other_node", "agent_a", now - 50, "executed"],
            ).unwrap();
        }

        let window = now - (7 * 86400);
        let total = db.get_trace_count_since("test_node", window).await.unwrap();
        assert_eq!(total, 3, "test_node should have 3 traces total");

        let rejected = db.get_trace_count_by_type("test_node", "rejected", window).await.unwrap();
        assert_eq!(rejected, 1, "test_node should have 1 rejected trace");

        let executed = db.get_trace_count_by_type("test_node", "executed", window).await.unwrap();
        assert_eq!(executed, 2, "test_node should have 2 executed traces");

        let zero = db.get_trace_count_by_type("test_node", "nonexistent", window).await.unwrap();
        assert_eq!(zero, 0, "nonexistent type should be 0");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_diffuse_heat_proportional() {
        let db = test_silva().await;

        // Create base node
        db.upsert_node("n_base", "concept", "Base Node", "{}").await.unwrap();
        // Create 2 candidate nodes for semantic similar propagation
        db.upsert_node("n_sim_1", "concept", "Similar Node 1", "{}").await.unwrap();
        db.upsert_node("n_sim_2", "concept", "Similar Node 2", "{}").await.unwrap();

        // 3-dim vectors:
        // base vector: [1.0, 0.0, 0.0]
        let base_emb = vec![1.0_f32, 0.0, 0.0];
        db.save_embedding("n_base", &base_emb, "test-model", None).await.unwrap();

        // sim = 1.0 (exact match) -> should yield 3 traces
        let sim_1_emb = vec![1.0_f32, 0.0, 0.0];
        db.save_embedding("n_sim_1", &sim_1_emb, "test-model", None).await.unwrap();

        // sim = 0.86 (very close to 0.85 floor)
        // Cosine similarity = cos(theta) = 0.86.
        // We can construct a vector: [0.86, sqrt(1 - 0.86^2), 0]
        // sqrt(1 - 0.7396) = sqrt(0.2604) ≈ 0.510294
        let sim_2_emb = vec![0.86_f32, 0.510294_f32, 0.0];
        db.save_embedding("n_sim_2", &sim_2_emb, "test-model", None).await.unwrap();

        // Call touch_node on n_base, which propagates to similar nodes
        db.touch_node("n_base", "agent-test", "user-touch").await.unwrap();

        // Wait a short moment
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Query counts for diffuse_heat traces
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let window = now - 3600;

        let count_1 = db.get_trace_count_by_type("n_sim_1", "diffuse_heat", window).await.unwrap();
        let count_2 = db.get_trace_count_by_type("n_sim_2", "diffuse_heat", window).await.unwrap();

        // Check proportional heat counts
        assert_eq!(count_1, 3, "sim=1.0 should produce exactly 3 diffuse_heat traces");
        assert_eq!(count_2, 1, "sim=0.86 should produce exactly 1 diffuse_heat trace");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_meta_cognitive_prune_archives_isolated() {
        let db = test_silva().await;
        // Insert a very old, low-weight, isolated node
        {
            let conn = db.conn_lock();
            let c = conn.lock().await;
            c.execute(
                "INSERT INTO nodes (id, content, weight, type, created_at, last_accessed)
                 VALUES ('orphan:old:node', 'stale content', 0.05, 'episode',
                         datetime(unixepoch() - 1000000, 'unixepoch'), unixepoch() - 500000)",
                [],
            ).unwrap();
        }
        let archived = db.meta_cognitive_prune(0.10, 24, 48).unwrap();
        assert!(archived >= 1, "Should archive isolated low-weight old node");

        let node = db.get_node("orphan:old:node").await.unwrap().unwrap();
        assert_eq!(node.node_type, "archived");
        assert!(node.weight < 0.001);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_tms_deprecates_contradiction() {
        let db = test_silva().await;
        let emb: Vec<f32> = (0..128).map(|i| if i < 64 { 1.0f32 } else { 0.0f32 }).collect();
        db.upsert_node("lesson:intent:foo", "lesson", "old answer", "{}").await.unwrap();

        // Insert embedding directly using block_in_place
        let emb_clone = emb.clone();
        tokio::task::block_in_place(|| {
            let conn = db.conn.blocking_lock();
            let bytes: Vec<u8> = emb_clone.iter()
                .flat_map(|f| f.to_le_bytes().to_vec())
                .collect();
            let _ = conn.execute(
                "INSERT OR IGNORE INTO node_embeddings(node_id, embedding) VALUES(?1,?2)",
                rusqlite::params!["lesson:intent:foo", bytes]);
        });

        let deprecated = db.deprecate_contradictions("lesson:intent:foo:v2", &emb, "new answer").await.unwrap();
        assert!(deprecated >= 1, "TMS must deprecate old node with same prefix and high cosine");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_temporal_validity_field_persists() {
        let db = test_silva().await;
        let past = 1_000_000i64;
        db.upsert_node_with_validity("test:expired", "lesson", "old info", "{}", Some(past), false).await.unwrap();
        let node = db.get_node("test:expired").await.unwrap().unwrap();
        assert_eq!(node.valid_until, Some(past));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_deprecated_nodes_works() {
        let db = test_silva().await;
        db.upsert_node("test:normal", "lesson", "this is normal content", "{}").await.unwrap();
        db.upsert_node("test:dep", "lesson", "[DEPRECATED by test:new] this is old content", "{}").await.unwrap();

        let deprecated = db.get_deprecated_nodes(10).await.unwrap();
        assert_eq!(deprecated.len(), 1);
        assert_eq!(deprecated[0].id, "test:dep");
    }

    #[test]
    fn test_remember_with_expiry() {
        let days = 30u32;
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        let valid_until = now + (days as i64 * 86400);
        let expected_days_from_now = (valid_until - now) / 86400;
        assert_eq!(expected_days_from_now, 30);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_adaptive_lesson_decay_faster() {
        let db = test_silva().await;
        // Insert a lesson:intent node and a concept node with same initial weight
        db.upsert_node("lesson:intent:test", "lesson", "intent content", "{}").await.unwrap();
        db.upsert_node("concept:test", "concept", "concept content", "{}").await.unwrap();

        // Set both to same initial weight and touch time (7 days ago)
        tokio::task::block_in_place(|| {
            let conn = db.conn.blocking_lock();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            let past_time = now - (7 * 24 * 3600); // 7 days ago

            let _ = conn.execute(
                "UPDATE nodes SET weight = 1.0, last_touched = ?1 WHERE id IN ('lesson:intent:test', 'concept:test')",
                rusqlite::params![past_time],
            );
        });

        // Apply decay — should reduce both weights but lesson:intent should decay faster
        let _ = db.apply_node_decay("lesson:intent:test").await;
        let _ = db.apply_node_decay("concept:test").await;

        let intent_node = db.get_node("lesson:intent:test").await.unwrap();
        let concept_node = db.get_node("concept:test").await.unwrap();

        if let (Some(intent), Some(concept)) = (intent_node, concept_node) {
            assert!(intent.weight <= concept.weight,
                "lesson:intent should decay faster than concept: {} vs {}",
                intent.weight, concept.weight);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_delete_node() {
        let db = SilvaDB::in_memory().await.unwrap();
        db.upsert_node("del:test", "concept", "deletable node", "{}").await.unwrap();
        assert!(db.get_node("del:test").await.unwrap().is_some());

        let deleted = db.delete_node("del:test").await.unwrap();
        assert!(deleted, "delete_node should return true for existing node");
        assert!(db.get_node("del:test").await.unwrap().is_none(), "node should be gone");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_delete_protected_node_is_rejected() {
        let db = SilvaDB::in_memory().await.unwrap();
        db.upsert_node("prot:test", "concept", "protected node", "{}").await.unwrap();
        db.protect_node("prot:test").await.unwrap();

        let deleted = db.delete_node("prot:test").await.unwrap();
        assert!(!deleted, "delete_node should return false for protected node");
        assert!(db.get_node("prot:test").await.unwrap().is_some(), "protected node must survive");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_dream_cycle_dedup_and_contradictions() {
        let db = std::sync::Arc::new(SilvaDB::in_memory().await.unwrap());
        // Two nearly-identical nodes (no embeddings in test — nodes_are_similar returns false without embeddings)
        db.upsert_node("a", "concept", "rust async programming", "{}").await.unwrap();
        db.upsert_node("b", "concept", "rust async programming", "{}").await.unwrap();
        // Contradiction: same source+predicate, two targets — nodes must exist for FK
        db.upsert_node("alice", "person", "Alice", "{}").await.unwrap();
        db.upsert_node("acme", "org", "Acme Corp", "{}").await.unwrap();
        db.upsert_node("betacorp", "org", "BetaCorp", "{}").await.unwrap();
        db.add_edge("alice", "acme", "works_at", 1.0, "{}").await.unwrap();
        db.add_edge("alice", "betacorp", "works_at", 1.0, "{}").await.unwrap();

        let cycle = crate::memory::dream_cycle::DreamCycle::new(db.clone());
        let report = cycle.run().await;
        // contradictions should be >= 1 (alice has works_at conflict)
        assert!(report.contradictions_flagged >= 1, "expected contradiction flag");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_flag_contradiction_direct() {
        let db = SilvaDB::in_memory().await.unwrap();
        db.add_edge("s", "t1", "rel", 1.0, "{}").await.unwrap();
        db.add_edge("s", "t2", "rel", 1.0, "{}").await.unwrap();
        let count = db.flag_contradiction_nodes().await.unwrap();
        assert_eq!(count, 1, "expected 1 contradiction source");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_active_agents_batch() {
        let db = SilvaDB::in_memory().await.unwrap();
        db.upsert_node("node1", "concept", "Node 1", "{}").await.unwrap();
        db.upsert_node("node2", "concept", "Node 2", "{}").await.unwrap();

        // Touch node1 with agent1, then with agent2
        db.touch_node("node1", "agent1", "read").await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        db.touch_node("node1", "agent2", "write").await.unwrap();

        // Touch node2 with anonymous (should be ignored/filtered out)
        db.touch_node("node2", "anonymous", "read").await.unwrap();

        let active_agents = db.get_active_agents_batch(&["node1".to_string(), "node2".to_string()], 24).await.unwrap();
        assert_eq!(active_agents.get("node1"), Some(&"agent2".to_string()));
        assert_eq!(active_agents.get("node2"), None);
    }

#[tokio::test(flavor = "multi_thread")]
async fn test_merge_node_into_with_colliding_edges() {
    // Regression M24: src and dst sharing an identical edge (PK source,target,type)
    // must not abort the merge — root cause of dedup never merging duplicates.
    let db = test_silva().await;
    db.upsert_node("keep", "concept", "dup content", "{}").await.unwrap();
    db.upsert_node("drop", "concept", "dup content", "{}").await.unwrap();
    db.upsert_node("other", "concept", "neighbor", "{}").await.unwrap();
    db.add_edge("keep", "other", "related_to", 1.0, "{}").await.unwrap();
    db.add_edge("drop", "other", "related_to", 1.0, "{}").await.unwrap(); // collides post-redirect

    db.merge_node_into("drop", "keep").await.expect("merge must survive edge collision");

    assert!(db.get_node("drop").await.unwrap().is_none(), "src deleted");
    assert!(db.get_node("keep").await.unwrap().is_some(), "dst kept");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_temporal_edges_validity() {
    let db = test_silva().await;
    db.upsert_node("s", "concept", "Source", "{}").await.unwrap();
    db.upsert_node("t1", "concept", "Target 1", "{}").await.unwrap();
    db.upsert_node("t2", "concept", "Target 2", "{}").await.unwrap();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    db.add_edge_with_validity("s", "t1", "rel", 1.0, "{}", Some(now - 10), Some(now + 100)).await.unwrap();
    db.add_edge_with_validity("s", "t2", "rel", 1.0, "{}", Some(now - 20), Some(now - 10)).await.unwrap();

    let context = db.get_context("s", 1).await.unwrap();
    let has_t1 = context.iter().any(|n| n.id == "t1");
    let has_t2 = context.iter().any(|n| n.id == "t2");
    assert!(has_t1, "t1 should be present in context");
    assert!(!has_t2, "t2 should NOT be present (expired)");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_cleanup_orphan_nodes() {
    let db = test_silva().await;
    // Create nodes
    db.upsert_node("connected_src", "concept", "Connected Source", "{}").await.unwrap();
    db.upsert_node("connected_dst", "concept", "Connected Target", "{}").await.unwrap();
    db.upsert_node("orphan_node", "concept", "Orphan Node", "{}").await.unwrap();
    db.upsert_node("protected_orphan", "concept", "Protected Orphan", "{}").await.unwrap();
    db.upsert_node("identity_orphan", "identity", "Identity Orphan", "{}").await.unwrap();
    
    // Protect protected_orphan
    {
        let conn = db.conn.lock().await;
        conn.execute("UPDATE nodes SET protected = 1 WHERE id = 'protected_orphan'", []).unwrap();
    }
    
    // Connect source and target
    db.add_edge("connected_src", "connected_dst", "rel", 1.0, "{}").await.unwrap();
    
    // Verify count
    let orphans = db.orphan_node_count().await.unwrap();
    assert_eq!(orphans, 1); // Only orphan_node (type=concept, protected=0, no edges)
    
    // Clean up
    let cleaned = db.cleanup_orphan_nodes().await.unwrap();
    assert_eq!(cleaned, 1); // Only orphan_node is cleaned (filter: protected=0 AND type!='identity')
    
    // Verify remaining count
    let remaining_orphans = db.orphan_node_count().await.unwrap();
    assert_eq!(remaining_orphans, 0); // All orphan-like nodes are filtered (protected + identity)
    
    assert!(db.get_node("orphan_node").await.unwrap().is_none());
    assert!(db.get_node("protected_orphan").await.unwrap().is_some());
    assert!(db.get_node("identity_orphan").await.unwrap().is_some());
}

#[test]
fn test_build_contextual_text_with_source_and_heading() {
    use crate::memory::silva::nodes::build_contextual_text;
    let meta = r#"{"source_file":"manual.md","heading_path":"Setup > Install"}"#;
    let result = build_contextual_text(meta, "Run tylluan-cli start");
    assert_eq!(result, "[manual.md > Setup > Install]\nRun tylluan-cli start");
}

#[test]
fn test_build_contextual_text_source_only() {
    use crate::memory::silva::nodes::build_contextual_text;
    let meta = r#"{"source":"README.md"}"#;
    let result = build_contextual_text(meta, "One binary. No dependencies.");
    assert_eq!(result, "[README.md]\nOne binary. No dependencies.");
}

#[test]
fn test_build_contextual_text_no_metadata() {
    use crate::memory::silva::nodes::build_contextual_text;
    let result = build_contextual_text("{}", "plain content");
    assert_eq!(result, "plain content");
}

#[test]
fn test_exponential_decay_formula() {
    let weight = 1.0_f64;
    let half_life = 336.0_f64; // 14 days in hours
    let hours_elapsed = 336.0_f64; // exactly one half-life
    let decayed = weight * 0.5_f64.powf(hours_elapsed / half_life);
    // After exactly one half-life, weight should be 0.5
    assert!((decayed - 0.5).abs() < 0.001, "Expected ~0.5, got {}", decayed);
}

#[test]
fn test_exponential_decay_two_halflives() {
    let weight = 1.0_f64;
    let half_life = 336.0_f64;
    let hours_elapsed = 672.0_f64; // 28 days = 2 half-lives
    let decayed = weight * 0.5_f64.powf(hours_elapsed / half_life);
    assert!((decayed - 0.25).abs() < 0.001, "Expected ~0.25, got {}", decayed);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_bm25_search_returns_relevant_nodes() {
    let db = SilvaDB::in_memory().await.unwrap();
    db.upsert_node("n1", "lesson", "Rust ownership and borrowing rules are strict", "{}").await.unwrap();
    db.upsert_node("n2", "concept", "Python uses garbage collection for memory management", "{}").await.unwrap();
    db.upsert_node("n3", "lesson", "Rust lifetimes ensure memory safety without GC", "{}").await.unwrap();

    let results = db.search("Rust memory safety", 5, None).await.unwrap();
    assert!(!results.is_empty(), "BM25 search must return results for 'Rust memory safety'");
    let ids: Vec<&str> = results.iter().map(|n| n.id.as_str()).collect();
    assert!(ids.contains(&"n1") || ids.contains(&"n3"), "Rust nodes must appear in results");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_entity_boost_in_hybrid_search() {
    let db = SilvaDB::in_memory().await.unwrap();
    db.upsert_node("ent1", "entity", "Tokio async runtime for Rust", "{}").await.unwrap();
    db.upsert_node("lesson1", "lesson", "Tokio is the async runtime we use in this project", "{}").await.unwrap();

        let results = db.search_hybrid("Tokio runtime", None, 5, None, false).await.unwrap();
    assert!(!results.is_empty(), "hybrid search must return results for 'Tokio runtime'");
    let ids: Vec<&str> = results.iter().map(|(n, _)| n.id.as_str()).collect();
    assert!(ids.contains(&"ent1") || ids.contains(&"lesson1"), "at least one matching node must appear");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_hnsw_not_built_below_threshold() {
    let db = SilvaDB::in_memory().await.unwrap();
    db.upsert_node("n1", "lesson", "test content", "{}").await.unwrap();
    db.rebuild_hnsw_if_needed().await.unwrap();
    let guard = db.hnsw.read().await;
    assert!(guard.is_none(), "HNSW must not build below threshold");
}

#[test]
fn test_hnsw_serialize_roundtrip() {
    use crate::memory::silva::hnsw::{EmbPoint, HnswIndex, serialize_hnsw_data, deserialize_hnsw_rebuild};
    use instant_distance::Builder;
    let points = vec![EmbPoint(vec![1.0f32, 0.0, 0.0]), EmbPoint(vec![0.0, 1.0, 0.0])];
    let values = vec!["n1".to_string(), "n2".to_string()];
    let map = Builder::default().build(points.clone(), values);
    let index = HnswIndex { map, node_ids: vec!["n1".into(), "n2".into()], points };
    let bytes = serialize_hnsw_data(&index).unwrap();
    assert!(!bytes.is_empty());
    let restored = deserialize_hnsw_rebuild(&bytes).unwrap();
    assert_eq!(restored.node_ids.len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_degree_centrality() {
    let db = SilvaDB::in_memory().await.unwrap();
    db.upsert_node("n1", "entity", "Entity 1", "{}").await.unwrap();
    db.upsert_node("n2", "entity", "Entity 2", "{}").await.unwrap();
    db.upsert_node("n3", "entity", "Entity 3", "{}").await.unwrap();

    // n1 <-> n2 y n2 <-> n3
    db.add_edge("n1", "n2", "link", 1.0, "{}").await.unwrap();
    db.add_edge("n2", "n3", "link", 1.0, "{}").await.unwrap();

    let candidate_ids = vec!["n1".to_string(), "n2".to_string(), "n3".to_string()];
    let degree_map = db.degree_centrality(&candidate_ids).await.unwrap();

    // n1 tiene 1 (con n2)
    // n2 tiene 2 (con n1 y n3)
    // n3 tiene 1 (con n2)
    assert_eq!(*degree_map.get("n1").unwrap(), 1);
    assert_eq!(*degree_map.get("n2").unwrap(), 2);
    assert_eq!(*degree_map.get("n3").unwrap(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_local_query_graph() {
    let db = SilvaDB::in_memory().await.unwrap();
    db.upsert_node("n1", "entity", "Entity 1", "{}").await.unwrap();
    db.upsert_node("n2", "entity", "Entity 2", "{}").await.unwrap();
    db.upsert_node("n3", "entity", "Entity 3", "{}").await.unwrap();

    db.add_edge("n1", "n2", "link", 1.0, "{}").await.unwrap();
    db.add_edge("n2", "n3", "link", 1.0, "{}").await.unwrap();

    let results = db.local_query_graph(&["n1".to_string()], 5).await.unwrap();
    assert!(!results.is_empty());
    
    // n2 es el vecino directo de n1, debe ser retornado
    let ids: Vec<&str> = results.iter().map(|(n, _)| n.id.as_str()).collect();
    assert!(ids.contains(&"n2"));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_local_query_graph_degree_penalty() {
    let db = SilvaDB::in_memory().await.unwrap();
    db.upsert_node("seed", "entity", "Seed node", "{}").await.unwrap();
    db.upsert_node("low_degree", "entity", "Low degree node", "{}").await.unwrap();
    db.upsert_node("high_degree", "entity", "High degree hub node", "{}").await.unwrap();

    // low_degree has 1 edge (to seed)
    db.add_edge("seed", "low_degree", "link", 1.0, "{}").await.unwrap();

    // high_degree has 5 edges (to seed + 4 dummy nodes)
    db.add_edge("seed", "high_degree", "link", 1.0, "{}").await.unwrap();
    for i in 0..4 {
        let dummy = format!("dummy_{i}");
        db.upsert_node(&dummy, "entity", "Dummy", "{}").await.unwrap();
        db.add_edge("high_degree", &dummy, "link", 1.0, "{}").await.unwrap();
    }

    let results = db.local_query_graph(&["seed".to_string()], 5).await.unwrap();
    assert!(!results.is_empty(), "local_query_graph should return results");

    // Both neighbors must be present
    let ids: Vec<&str> = results.iter().map(|(n, _)| n.id.as_str()).collect();
    assert!(ids.contains(&"low_degree"), "low_degree should be in results");
    assert!(ids.contains(&"high_degree"), "high_degree should be in results");

    // The degree penalty inverts the bias: low-degree node should rank above high-degree node
    let rank_low = ids.iter().position(|&id| id == "low_degree").unwrap();
    let rank_high = ids.iter().position(|&id| id == "high_degree").unwrap();
    assert!(
        rank_low < rank_high,
        "low_degree (degree=1) should rank above high_degree (degree=5) — got low={rank_low}, high={rank_high}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_search_hybrid_with_graph_traversal() {
    let db = SilvaDB::in_memory().await.unwrap();
    db.upsert_node("n1", "entity", "Entity 1 Tokio async", "{}").await.unwrap();
    db.upsert_node("n2", "entity", "Entity 2 Tokio runner", "{}").await.unwrap();

    db.add_edge("n1", "n2", "link", 1.0, "{}").await.unwrap();

    // Insertar un embedding falso de 1024-dim
    let mut emb = vec![0.0f32; 1024];
    emb[0] = 1.0;
    let emb_bytes: Vec<u8> = emb.iter().flat_map(|v| v.to_le_bytes()).collect();
    tokio::task::block_in_place(|| {
        let conn = db.conn.blocking_lock();
        conn.execute(
            "INSERT OR REPLACE INTO node_embeddings (node_id, embedding, model_id) VALUES (?1, ?2, ?3)",
            rusqlite::params!["n1", emb_bytes, "test"],
        ).unwrap();
    });

    // Búsqueda híbrida con embedding
    let results = db.search_hybrid("Tokio async", Some(&emb), 5, None, false).await.unwrap();
    assert!(!results.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_search_hybrid_with_type_filter() {
    let db = SilvaDB::in_memory().await.unwrap();
    db.upsert_node("n1", "episodic", "Mensaje de coloquio episódico de testeo", "{}").await.unwrap();
    db.upsert_node("n2", "lesson", "Lección de testeo general", "{}").await.unwrap();

    // 1. Filtrar por 'episodic'
    let results_epi = db.search_hybrid("testeo", None, 5, Some("episodic"), false).await.unwrap();
    assert!(!results_epi.is_empty());
    let ids: Vec<&str> = results_epi.iter().map(|(n, _)| n.id.as_str()).collect();
    assert!(ids.contains(&"n1"));
    assert!(!ids.contains(&"n2"));

    // 2. Filtrar por 'lesson'
    let results_les = db.search_hybrid("testeo", None, 5, Some("lesson"), false).await.unwrap();
    assert!(!results_les.is_empty());
    let ids_les: Vec<&str> = results_les.iter().map(|(n, _)| n.id.as_str()).collect();
    assert!(!ids_les.contains(&"n1"));
    assert!(ids_les.contains(&"n2"));
}
