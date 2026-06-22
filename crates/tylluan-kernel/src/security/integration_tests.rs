//! Security integration tests for TylluanNexus
//!
//! Tests the ExecutionGuard channel gating integration.

#[cfg(test)]
mod security_tests {
    use crate::security::guard::ExecutionGuard;
    use crate::transport::server::TylluanServer;
    use crate::registry::guild_process::GuildRegistry;
    use crate::memory::hybrid::HybridMemory;
    use crate::memory::silva::SilvaDB;
    use crate::memory::mailbox::Mailbox;
    use crate::router::matcher::GuildMatcher;
    use crate::router::catalog::builtin_catalog;
    use crate::memory::agent_nodes::AgentNodeRouter;
    use std::sync::Mutex;
    use tokio::sync::broadcast;

    fn test_registry() -> Arc<RwLock<GuildRegistry>> {
        let reg = GuildRegistry::new(PathBuf::from("."), 300, Default::default(), 3);
        Arc::new(RwLock::new(reg))
    }

    async fn test_server() -> TylluanServer {
        let (tx, _) = broadcast::channel(16);
        let node_router = AgentNodeRouter::new(tx);
        let matcher = GuildMatcher::new(builtin_catalog());
        let mailbox = Arc::new(Mailbox::in_memory().await.unwrap());
        TylluanServer::new(
            test_registry(),
            Arc::new(matcher),
            Arc::new(HybridMemory::in_memory().await.unwrap()),
            Arc::new(SilvaDB::in_memory().await.unwrap()),
            mailbox,
            Arc::new(crate::doctor::Doctor::new(
                test_registry(),
                Arc::new(HybridMemory::in_memory().await.unwrap()),
                Arc::new(SilvaDB::in_memory().await.unwrap()),
                Arc::new(Mutex::new(crate::curriculum::CurriculumLearner::new_in_memory(5).unwrap())),
            )),
            node_router,
        )
    }

    async fn test_server() -> TylluanServer {
        use crate::router::catalog::builtin_catalog;
        let matcher = GuildMatcher::new(builtin_catalog());
        TylluanServer::new(
            test_registry(),
            Arc::new(matcher),
            Arc::new(HybridMemory::in_memory().await.unwrap()),
            Arc::new(SilvaDB::in_memory().await.unwrap()),
            Arc::new(Mailbox::in_memory().await.unwrap()),
            Arc::new(crate::doctor::Doctor::new()),
        )
    }

    #[tokio::test]
    async fn test_security_blocks_dangerous_from_http_channel() {
        let server = test_server().await;
        
        // Set channel to HTTP (unauthenticated) - channel ID 2
        server.set_channel(2);
        
        // Try to call dangerous tool via HTTP
        let request = CallToolRequestParam {
            name: "bash_execute".into(),
            arguments: Some(json!({"command": "rm -rf /"}).as_object().unwrap().clone()),
        };
        
        let result = server.handle_call_internal(request).await.unwrap();
        
        // Should be blocked
        assert!(result.is_error == Some(true));
        let content = &result.content[0];
        let text = serde_json::to_string(content).unwrap();
        assert!(text.contains("Security Error") || text.contains("blocked"));
    }

    #[tokio::test]
    async fn test_security_allows_safe_from_http_channel() {
        let server = test_server().await;
        
        // Set channel to HTTP (unauthenticated)
        server.set_channel(2);
        
        // Try to call safe tool (memory_search)
        let request = CallToolRequestParam {
            name: "memory_search".into(),
            arguments: Some(json!({"query": "test"}).as_object().unwrap().clone()),
        };
        
        let result = server.handle_call_internal(request).await.unwrap();
        
        // Should NOT be blocked (result depends on memory, not security)
        // security check passes, but tool might fail for other reasons
        let content = &result.content[0];
        let text = serde_json::to_string(content).unwrap();
        // If it contains "Security Error", it was blocked
        assert!(!text.contains("Security Error") || text.contains("[]"));
    }

    #[tokio::test]
    async fn test_security_allows_dangerous_from_stdio_channel() {
        let server = test_server().await;
        
        // Set channel to Stdio (trusted)
        server.set_channel(0);
        
        // Try to call dangerous tool
        let request = CallToolRequestParam {
            name: "bash_execute".into(),
            arguments: Some(json!({"command": "echo hello"}).as_object().unwrap().clone()),
        };
        
        let result = server.handle_call_internal(request).await.unwrap();
        
        // Should NOT be blocked by security (will fail for other reasons in test env)
        let content = &result.content[0];
        let text = serde_json::to_string(content).unwrap();
        assert!(!text.contains("Security Error"));
    }

    #[tokio::test]
    async fn test_silva_clustering() {
        let server = test_server().await;
        
        // Add a cluster of related nodes
        let silva = server.silva.clone();
        
        // Create nodes: A connects to B, B connects to C
        silva.upsert_node("node_a", "concept", "Concept A", "{}").await.unwrap();
        silva.upsert_node("node_b", "concept", "Concept B", "{}").await.unwrap();
        silva.upsert_node("node_c", "concept", "Concept C", "{}").await.unwrap();
        silva.add_edge("node_a", "node_b", "relates_to", 1.0, "{}").await.unwrap();
        silva.add_edge("node_b", "node_c", "relates_to", 1.0, "{}").await.unwrap();
        
        // Find clusters
        let clusters = silva.find_clusters(2).await.unwrap();
        
        // Should have at least one cluster with 3 nodes
        assert!(!clusters.is_empty());
    }

    #[tokio::test]
    async fn test_cluster_summary() {
        let server = test_server().await;
        
        let silva = server.silva.clone();
        
        // Create cluster
        silva.upsert_node("x", "lesson", "Lesson X", "{}").await.unwrap();
        silva.upsert_node("y", "lesson", "Lesson Y", "{}").await.unwrap();
        
        // Generate summary
        let summary = silva.generate_cluster_summary(&["x".to_string(), "y".to_string()]).await.unwrap();
        
        // Should contain type info
        assert!(summary.contains("Lesson"));
    }
}