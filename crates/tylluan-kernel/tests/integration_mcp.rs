//! End-to-end MCP protocol integration tests.
//!
//! Verifies the full MCP tool lifecycle without real guilds or Python:
//!  1. TylluanServer boots in test mode (SilvaDB in-memory)
//!  2. all_tools() returns exactly 5 sovereign tools
//!  3. tylluan_remember stores a node in SilvaDB
//!  4. tylluan_recall finds the stored node
//!
//! All tests run in CI without any external dependencies.

use tylluan_kernel::config::TimeoutsConfig;
use tylluan_kernel::doctor::Doctor;
use tylluan_kernel::memory::hybrid::HybridMemory;
use tylluan_kernel::memory::mailbox::Mailbox;
use tylluan_kernel::memory::silva::SilvaDB;
use tylluan_kernel::registry::guild_process::GuildRegistry;
use tylluan_kernel::router::catalog::builtin_catalog;
use tylluan_kernel::router::matcher::GuildMatcher;
use tylluan_kernel::transport::server::TylluanServer;
use rmcp::model::JsonObject;
use std::sync::Arc;
use tokio::sync::RwLock;

fn test_dir(name: &str) -> String {
    format!("./target/mcp_int_{}", name)
}

async fn create_server(label: &str) -> TylluanServer {
    let dir = test_dir(label);
    let pb = std::path::PathBuf::from(&dir);
    std::fs::create_dir_all(&pb).ok();

    let registry = Arc::new(RwLock::new(GuildRegistry::new(
        pb.clone(), 300, TimeoutsConfig::default(), 5,
    )));
    let matcher = Arc::new(GuildMatcher::new(builtin_catalog()));
    let memory = Arc::new(
        HybridMemory::open(&pb.join("mem.db").to_string_lossy()).unwrap(),
    );
    memory.init().await.unwrap();
    let silva = Arc::new(
        SilvaDB::open(":memory:").unwrap(),
    );
    silva.init().await.unwrap();
    let mailbox = Arc::new(
        Mailbox::open(":memory:").unwrap(),
    );
    mailbox.init().await.unwrap();
    let curriculum = Arc::new(std::sync::Mutex::new(
        tylluan_kernel::curriculum::CurriculumLearner::new_in_memory(1).unwrap(),
    ));
    let doctor = Arc::new(Doctor::new(
        registry.clone(), memory.clone(), silva.clone(), curriculum,
    ));
    let node_router = tylluan_kernel::memory::agent_nodes::AgentNodeRouter::new(tokio::sync::broadcast::channel(1).0);
    TylluanServer::new(registry, matcher, memory, silva, mailbox, doctor, node_router)
}

/// Server boots and reports exactly 5 sovereign tools.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_integration_tools_list_exactly_5() {
    let server = create_server("t1").await;
    let tools = server.all_tools().await;
    assert_eq!(tools.len(), 5, "Expected exactly 5 sovereign tools");
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    for expected in &["tylluan_do", "tylluan_remember", "tylluan_recall", "tylluan_think", "tylluan_graph"] {
        assert!(names.contains(expected), "Missing tool: {}", expected);
    }
}

/// tylluan_remember stores a node in SilvaDB, tylluan_recall retrieves it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_integration_remember_recall_roundtrip() {
    let server = create_server("t2").await;
    let marker = "integration_marker_7k9m";

    // Remember
    let mut rem_args = JsonObject::new();
    rem_args.insert("content".to_string(), serde_json::Value::from(marker));
    let stored = server.handle_kernel_tool("tylluan_remember", Some(rem_args)).await.unwrap();
    assert_eq!(stored.is_error, Some(false), "tylluan_remember must succeed");

    // Verify node exists in SilvaDB directly
    let silva = server.silva();
    let results = silva.search(marker, 5, None).await.unwrap();
    assert!(!results.is_empty(), "Stored memory must exist as a node in SilvaDB");
    assert!(
        results.iter().any(|n| n.content.contains(marker)),
        "Node content must contain the stored marker"
    );

    // Recall
    let mut rec_args = JsonObject::new();
    rec_args.insert("query".to_string(), serde_json::Value::from(marker));
    let recalled = server.handle_kernel_tool("tylluan_recall", Some(rec_args)).await.unwrap();
    assert_eq!(recalled.is_error, Some(false), "tylluan_recall after store must succeed");
    let text = recalled.content.first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.as_str())
        .unwrap_or("");
    assert!(
        text.contains(marker) || text.contains("integration_marker"),
        "Recall output must contain the remembered content, got: {text}"
    );
}
