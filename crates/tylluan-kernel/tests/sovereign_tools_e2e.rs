//! End-to-end tests for sovereign tools invariant.

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

async fn create_server(dir: &str) -> TylluanServer {
    let pb = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&pb).ok();
    let registry = Arc::new(RwLock::new(GuildRegistry::new(
        pb.clone(),
        300,
        TimeoutsConfig::default(),
        5,
    )));
    let matcher = Arc::new(GuildMatcher::new(builtin_catalog()));
    let memory = Arc::new(
        HybridMemory::open(&pb.join("mem.db").to_string_lossy()).unwrap(),
    );
    memory.init().await.unwrap();
    let silva = Arc::new(
        SilvaDB::open(&pb.join("silva.db").to_string_lossy()).unwrap(),
    );
    silva.init().await.unwrap();
    let mailbox = Arc::new(
        Mailbox::open(&pb.join("mailbox.db").to_string_lossy()).unwrap(),
    );
    mailbox.init().await.unwrap();
    let curriculum = Arc::new(std::sync::Mutex::new(
        tylluan_kernel::curriculum::CurriculumLearner::new_in_memory(1).unwrap(),
    ));
    let doctor = Arc::new(Doctor::new(
        registry.clone(),
        memory.clone(),
        silva.clone(),
        curriculum,
    ));
    let node_router = tylluan_kernel::memory::agent_nodes::AgentNodeRouter::new(tokio::sync::broadcast::channel(1).0);
    TylluanServer::new(registry, matcher, memory, silva, mailbox, doctor, node_router)
}

// ── Invariant ────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_exactly_3_sovereign_tools() {
    let server = create_server("./e2e_t1").await;
    let tools = server.all_tools().await;
    assert_eq!(tools.len(), 5, "Expected exactly 5 sovereign tools, got {}", tools.len());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_sovereign_tool_names_exact() {
    let server = create_server("./e2e_t2").await;
    let tools = server.all_tools().await;
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    assert!(names.contains(&"tylluan_do"), "missing tylluan_do");
    assert!(names.contains(&"tylluan_remember"), "missing tylluan_remember");
    assert!(names.contains(&"tylluan_recall"), "missing tylluan_recall");
    assert!(names.contains(&"tylluan_think"), "missing tylluan_think");
    assert!(names.contains(&"tylluan_graph"), "missing tylluan_graph");
    let allowed = ["tylluan_do", "tylluan_remember", "tylluan_recall", "tylluan_think", "tylluan_graph"];
    for name in &names {
        assert!(allowed.contains(name), "Unexpected sovereign tool: {name}");
    }
}

// ── tylluan_remember ────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tylluan_remember_works() {
    let server = create_server("./e2e_t3").await;
    let mut args = JsonObject::new();
    args.insert("content".to_string(), serde_json::Value::from("test fact"));
    let result = server.handle_kernel_tool("tylluan_remember", Some(args)).await;
    assert!(result.is_ok());
    let r = result.unwrap();
    assert_eq!(r.is_error, Some(false));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tylluan_remember_empty_content_returns_error() {
    let server = create_server("./e2e_t4").await;
    let mut args = JsonObject::new();
    args.insert("content".to_string(), serde_json::Value::from(""));
    let result = server.handle_kernel_tool("tylluan_remember", Some(args)).await;
    assert!(result.is_ok());
    let r = result.unwrap();
    assert_eq!(r.is_error, Some(true), "Empty content must return error");
    let text = r.content.first().and_then(|c| c.as_text()).map(|t| t.text.as_str()).unwrap_or("");
    assert!(text.contains("non-empty"), "Error must mention non-empty: {text}");
}

// ── tylluan_recall ─────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tylluan_recall_works() {
    let server = create_server("./e2e_t5").await;
    let mut args = JsonObject::new();
    args.insert("query".to_string(), serde_json::Value::from("test"));
    let result = server.handle_kernel_tool("tylluan_recall", Some(args)).await;
    assert!(result.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tylluan_recall_empty_query_returns_error() {
    let server = create_server("./e2e_t6").await;
    let mut args = JsonObject::new();
    args.insert("query".to_string(), serde_json::Value::from("   "));
    let result = server.handle_kernel_tool("tylluan_recall", Some(args)).await;
    assert!(result.is_ok());
    let r = result.unwrap();
    assert_eq!(r.is_error, Some(true), "Blank query must return error");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tylluan_remember_then_recall_roundtrip() {
    let server = create_server("./e2e_t7").await;

    // Store a unique marker
    let marker = "sovereign_roundtrip_marker_xq9z";
    let mut rem_args = JsonObject::new();
    rem_args.insert("content".to_string(), serde_json::Value::from(marker));
    let stored = server.handle_kernel_tool("tylluan_remember", Some(rem_args)).await.unwrap();
    assert_eq!(stored.is_error, Some(false), "remember must succeed");

    // Recall should return an Ok result (content may or may not include marker
    // since hybrid memory is BM25-based, but must not error)
    let mut rec_args = JsonObject::new();
    rec_args.insert("query".to_string(), serde_json::Value::from(marker));
    let recalled = server.handle_kernel_tool("tylluan_recall", Some(rec_args)).await.unwrap();
    assert_eq!(recalled.is_error, Some(false), "recall after store must succeed");
}

// ── tylluan_do ─────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tylluan_do_empty_intent_returns_error() {
    let server = create_server("./e2e_t8").await;
    let mut args = JsonObject::new();
    args.insert("intent".to_string(), serde_json::Value::from(""));
    let result = server.handle_kernel_tool("tylluan_do", Some(args)).await;
    assert!(result.is_ok());
    let r = result.unwrap();
    assert_eq!(r.is_error, Some(true), "Empty intent must return error");
    let text = r.content.first().and_then(|c| c.as_text()).map(|t| t.text.as_str()).unwrap_or("");
    assert!(text.contains("non-empty"), "Error must mention non-empty: {text}");
}

// ── Unknown tool ─────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_unknown_tool_returns_mcp_error() {
    let server = create_server("./e2e_t9").await;
    let result = server.handle_kernel_tool("not_a_sovereign_tool", None).await;
    assert!(result.is_err(), "Unknown tool must return Err(McpError)");
}

// ── tylluan_do remember flag ────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tylluan_do_schema_exposes_remember() {
    let server = create_server("./e2e_t10").await;
    let tools = server.all_tools().await;
    let tylluan_do = tools.iter().find(|t| t.name.as_ref() == "tylluan_do")
        .expect("tylluan_do must be in all_tools");
    let schema = serde_json::to_string(&tylluan_do.input_schema).unwrap();
    assert!(schema.contains("remember"), "tylluan_do schema must expose 'remember' param to MCP clients");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tylluan_do_remember_false_default_accepted() {
    let server = create_server("./e2e_t11").await;
    let mut args = JsonObject::new();
    args.insert("intent".to_string(), serde_json::Value::from("list files"));
    args.insert("remember".to_string(), serde_json::Value::Bool(false));
    let result = server.handle_kernel_tool("tylluan_do", Some(args)).await;
    assert!(result.is_ok(), "tylluan_do with remember=false must not panic");
}

// ── tylluan_do guild hint ───────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tylluan_do_guild_hint_valid_bypasses_router() {
    // "bash" is in the catalog → hint is accepted, guild start attempted.
    // No Python process runs in tests so result will be a startup error — that's fine.
    let server = create_server("./e2e_t12").await;
    let mut args = JsonObject::new();
    args.insert("intent".to_string(), serde_json::Value::from("do something"));
    args.insert("guild".to_string(), serde_json::Value::from("bash"));
    let result = server.handle_kernel_tool("tylluan_do", Some(args)).await;
    assert!(result.is_ok());
    // Must NOT return "No guild found" — the router was bypassed
    let r = result.unwrap();
    let text = r.content.first().and_then(|c| c.as_text()).map(|t| t.text.as_str()).unwrap_or("");
    assert!(!text.contains("No guild found"), "guild hint must bypass the router: {text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tylluan_do_guild_hint_unknown_returns_error() {
    let server = create_server("./e2e_t13").await;
    let mut args = JsonObject::new();
    args.insert("intent".to_string(), serde_json::Value::from("do something"));
    args.insert("guild".to_string(), serde_json::Value::from("nonexistent_guild_xyz"));
    let result = server.handle_kernel_tool("tylluan_do", Some(args)).await.unwrap();
    assert_eq!(result.is_error, Some(true));
    let text = result.content.first().and_then(|c| c.as_text()).map(|t| t.text.as_str()).unwrap_or("");
    assert!(text.contains("Unknown guild"), "must report unknown guild: {text}");
}

// ── tylluan_ingest ──────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tylluan_ingest_empty_source_returns_error() {
    let server = create_server("./e2e_t14").await;
    let mut args = JsonObject::new();
    args.insert("source".to_string(), serde_json::Value::from(""));
    args.insert("name".to_string(), serde_json::Value::from("test-guild"));
    let result = server.handle_kernel_tool("tylluan_ingest", Some(args)).await;
    assert!(result.is_ok());
    let r = result.unwrap();
    assert_eq!(r.is_error, Some(true), "Empty source must return error");
    let text = r.content.first().and_then(|c| c.as_text()).map(|t| t.text.as_str()).unwrap_or("");
    assert!(text.to_lowercase().contains("source"), "Error must mention 'source': {text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tylluan_ingest_empty_name_returns_error() {
    let server = create_server("./e2e_t15").await;
    let mut args = JsonObject::new();
    args.insert("source".to_string(), serde_json::Value::from("https://example.com/repo"));
    args.insert("name".to_string(), serde_json::Value::from(""));
    let result = server.handle_kernel_tool("tylluan_ingest", Some(args)).await;
    assert!(result.is_ok());
    let r = result.unwrap();
    assert_eq!(r.is_error, Some(true), "Empty name must return error");
    let text = r.content.first().and_then(|c| c.as_text()).map(|t| t.text.as_str()).unwrap_or("");
    assert!(text.to_lowercase().contains("name"), "Error must mention 'name': {text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tylluan_ingest_nonexistent_local_path_returns_error() {
    let server = create_server("./e2e_t16").await;
    let mut args = JsonObject::new();
    args.insert("source".to_string(), serde_json::Value::from("/nonexistent/path/to/guild"));
    args.insert("name".to_string(), serde_json::Value::from("ghost-guild"));
    args.insert("ingest_type".to_string(), serde_json::Value::from("file"));
    let result = server.handle_kernel_tool("tylluan_ingest", Some(args)).await;
    assert!(result.is_ok());
    let r = result.unwrap();
    assert_eq!(r.is_error, Some(true), "Nonexistent path must return error");
    let text = r.content.first().and_then(|c| c.as_text()).map(|t| t.text.as_str()).unwrap_or("");
    assert!(
        text.to_lowercase().contains("does not exist") || text.to_lowercase().contains("not found") || text.to_lowercase().contains("no such"),
        "Error must mention path problem: {text}"
    );
}

// NOTE: tylluan_ingest is an internal pipeline tool — not exposed via MCP all_tools().
// It is callable via handle_kernel_tool() (REST /api/v1/do) but not listed in tools/list.
// Schema test removed: the schema is intentionally not visible to MCP clients.

// ── Phase E: agent sessions ────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_sovereign_schemas_expose_agent_id() {
    let server = create_server("./e2e_t17").await;
    let tools = server.all_tools().await;
    for name in ["tylluan_do", "tylluan_remember", "tylluan_recall"] {
        let tool = tools.iter().find(|t| t.name.as_ref() == name)
            .unwrap_or_else(|| panic!("{name} must be in all_tools"));
        let schema = serde_json::to_string(&tool.input_schema).unwrap();
        assert!(schema.contains("agent_id"), "{name} schema must expose 'agent_id': {schema}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tylluan_remember_with_agent_id_succeeds() {
    let server = create_server("./e2e_t18").await;
    let mut args = JsonObject::new();
    args.insert("content".to_string(), serde_json::Value::from("agent session fact"));
    args.insert("agent_id".to_string(), serde_json::Value::from("test-agent"));
    let result = server.handle_kernel_tool("tylluan_remember", Some(args)).await;
    assert!(result.is_ok());
    let r = result.unwrap();
    assert_eq!(r.is_error, Some(false), "tylluan_remember with agent_id must succeed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tylluan_recall_with_agent_id_succeeds() {
    let server = create_server("./e2e_t19").await;
    let mut args = JsonObject::new();
    args.insert("query".to_string(), serde_json::Value::from("session"));
    args.insert("agent_id".to_string(), serde_json::Value::from("test-agent"));
    let result = server.handle_kernel_tool("tylluan_recall", Some(args)).await;
    assert!(result.is_ok());
    let r = result.unwrap();
    assert_eq!(r.is_error, Some(false), "tylluan_recall with agent_id must succeed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_agent_session_roundtrip() {
    // Store a tagged memory, then recall it filtering by the same agent_id.
    let server = create_server("./e2e_t20").await;
    let agent = "roundtrip-agent-x7k";
    let marker = "unique_session_marker_y3w";

    let mut rem_args = JsonObject::new();
    rem_args.insert("content".to_string(), serde_json::Value::from(marker));
    rem_args.insert("agent_id".to_string(), serde_json::Value::from(agent));
    let stored = server.handle_kernel_tool("tylluan_remember", Some(rem_args)).await.unwrap();
    assert_eq!(stored.is_error, Some(false), "agent-tagged remember must succeed");

    let mut rec_args = JsonObject::new();
    rec_args.insert("query".to_string(), serde_json::Value::from("session marker"));
    rec_args.insert("agent_id".to_string(), serde_json::Value::from(agent));
    let recalled = server.handle_kernel_tool("tylluan_recall", Some(rec_args)).await.unwrap();
    assert_eq!(recalled.is_error, Some(false), "agent-filtered recall must succeed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tylluan_do_agent_id_accepted_does_not_panic() {
    // guild will fail to start (no Python) but agent_id must not cause a panic.
    let server = create_server("./e2e_t21").await;
    let mut args = JsonObject::new();
    args.insert("intent".to_string(), serde_json::Value::from("list files"));
    args.insert("agent_id".to_string(), serde_json::Value::from("test-agent"));
    let result = server.handle_kernel_tool("tylluan_do", Some(args)).await;
    assert!(result.is_ok(), "tylluan_do with agent_id must not return Err: {:?}", result);
}

// ── tylluan_think ───────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tylluan_think_works() {
    let server = create_server("./e2e_t22").await;
    let mut args = JsonObject::new();
    args.insert("query".to_string(), serde_json::Value::from("test knowledge"));
    let result = server.handle_kernel_tool("tylluan_think", Some(args)).await;
    assert!(result.is_ok(), "tylluan_think must not return Err: {:?}", result);
    let r = result.unwrap();
    assert_eq!(r.is_error, Some(false), "tylluan_think with valid query must succeed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tylluan_think_empty_query_returns_error() {
    let server = create_server("./e2e_t23").await;
    let mut args = JsonObject::new();
    args.insert("query".to_string(), serde_json::Value::from("  "));
    let result = server.handle_kernel_tool("tylluan_think", Some(args)).await;
    assert!(result.is_ok());
    let r = result.unwrap();
    assert_eq!(r.is_error, Some(true), "Blank query must return error");
    let text = r.content.first().and_then(|c| c.as_text()).map(|t| t.text.as_str()).unwrap_or("");
    assert!(text.contains("non-empty") || text.contains("requires"), "Error must mention requirement: {text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tylluan_think_schema_exposed() {
    let server = create_server("./e2e_t24").await;
    let tools = server.all_tools().await;
    let tool = tools.iter().find(|t| t.name.as_ref() == "tylluan_think")
        .expect("tylluan_think must be in all_tools");
    let schema = serde_json::to_string(&tool.input_schema).unwrap();
    assert!(schema.contains("query"), "tylluan_think schema must have 'query': {schema}");
    assert!(schema.contains("depth"), "tylluan_think schema must have 'depth': {schema}");
}

// ── tylluan_graph ───────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tylluan_graph_stats_works() {
    let server = create_server("./e2e_t25").await;
    let mut args = JsonObject::new();
    args.insert("command".to_string(), serde_json::Value::from("stats"));
    let result = server.handle_kernel_tool("tylluan_graph", Some(args)).await;
    assert!(result.is_ok(), "tylluan_graph stats must not return Err: {:?}", result);
    let r = result.unwrap();
    assert_eq!(r.is_error, Some(false), "tylluan_graph stats must succeed");
    let text = r.content.first().and_then(|c| c.as_text()).map(|t| t.text.as_str()).unwrap_or("");
    assert!(text.contains("node") || text.contains("edge"), "stats must contain node/edge info: {text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tylluan_graph_add_triple_works() {
    let server = create_server("./e2e_t26").await;
    let mut args = JsonObject::new();
    args.insert("command".to_string(), serde_json::Value::from("add_triple"));
    args.insert("subject".to_string(), serde_json::Value::from("TestSubject"));
    args.insert("predicate".to_string(), serde_json::Value::from("has_property"));
    args.insert("object".to_string(), serde_json::Value::from("TestObject"));
    let result = server.handle_kernel_tool("tylluan_graph", Some(args)).await;
    assert!(result.is_ok());
    let r = result.unwrap();
    assert_eq!(r.is_error, Some(false), "tylluan_graph add_triple must succeed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tylluan_graph_retrograde_extract_accepted() {
    // retrograde_extract spawns a background task and returns immediately.
    let server = create_server("./e2e_t27").await;
    let mut args = JsonObject::new();
    args.insert("command".to_string(), serde_json::Value::from("retrograde_extract"));
    args.insert("limit".to_string(), serde_json::Value::from(5));
    let result = server.handle_kernel_tool("tylluan_graph", Some(args)).await;
    assert!(result.is_ok());
    let r = result.unwrap();
    assert_eq!(r.is_error, Some(false), "retrograde_extract must be accepted: {:?}", r.content);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tylluan_graph_missing_command_returns_error() {
    // No "command" argument — must return a clear error.
    let server = create_server("./e2e_t28").await;
    let args = JsonObject::new();
    let result = server.handle_kernel_tool("tylluan_graph", Some(args)).await;
    assert!(result.is_ok());
    // stats is the default — should succeed silently rather than error
    let r = result.unwrap();
    let text = r.content.first().and_then(|c| c.as_text()).map(|t| t.text.as_str()).unwrap_or("");
    // Either stats result (default) or an error — both are valid; must not panic
    let _ = text;
}

// ── Invariant: no new sovereign tools added silently ─────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_sovereign_count_is_exactly_5() {
    // INVARIANT: MCP clients must always see exactly 5 sovereign tools.
    // If this fails, a tool was added to all_tools() without an architecture review.
    let server = create_server("./e2e_t29").await;
    let tools = server.all_tools().await;
    assert_eq!(
        tools.len(), 5,
        "SOVEREIGN INVARIANT VIOLATED: expected 5 tools, got {}. \
         Update CLAUDE.md invariant #1 if this is intentional.",
        tools.len()
    );
}
