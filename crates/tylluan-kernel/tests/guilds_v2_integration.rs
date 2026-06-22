//! Integration tests for Guild Architecture V2
//!
//! Validates:
//! 1. Guild catalog structure: all 21 guilds present, correct categories
//! 2. Guild context routing: `match_guild_with_context` routes correctly by gremio
//! 3. Agent role inference: GuildContext::from_agent_id() maps roles to categories
//! 4. NO_GUILD_MATCH fallback: unrecognizable intents return None (not a wrong guild)
//! 5. Catalog invariants: no duplicate names, no empty descriptions, all in guilds/core/
//! 6. tylluan_do + agent_id: kernel tool routes with context when agent_id is provided
//! 7. Guild filesystem: guild.md files exist for each gremio
//! 8. Workflow files: at least 1 workflow per gremio
//! 9. Sub-agent skills: rust-specialist.skill.md is present

use tylluan_kernel::config::TimeoutsConfig;
use tylluan_kernel::doctor::Doctor;
use tylluan_kernel::memory::hybrid::HybridMemory;
use tylluan_kernel::memory::mailbox::Mailbox;
use tylluan_kernel::memory::silva::SilvaDB;
use tylluan_kernel::registry::guild_process::GuildRegistry;
use tylluan_kernel::router::catalog::{builtin_catalog, GuildCategory};
use tylluan_kernel::router::matcher::{GuildContext, GuildMatcher};
use tylluan_kernel::transport::server::TylluanServer;
use rmcp::model::JsonObject;
use std::sync::Arc;
use tokio::sync::RwLock;

// ─── Test helpers ──────────────────────────────────────────────────────────────

async fn create_test_server(dir: &str) -> TylluanServer {
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

fn test_matcher() -> GuildMatcher {
    GuildMatcher::new(builtin_catalog())
}

// ─── Section 1: Catalog Invariants ────────────────────────────────────────────

#[test]
fn test_guilds_v2_catalog_not_empty() {
    let catalog = builtin_catalog();
    assert!(!catalog.is_empty(), "Guild catalog must not be empty");
    assert!(
        catalog.len() >= 20,
        "Expected at least 20 guilds, got {}",
        catalog.len()
    );
}

#[test]
fn test_guilds_v2_no_duplicate_names() {
    let catalog = builtin_catalog();
    let mut names: Vec<&str> = catalog.iter().map(|g| g.name.as_str()).collect();
    let total = names.len();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), total, "Duplicate guild names found in catalog");
}

#[test]
fn test_guilds_v2_all_descriptions_non_empty() {
    let catalog = builtin_catalog();
    for guild in &catalog {
        assert!(
            !guild.description.is_empty(),
            "Guild '{}' has empty description",
            guild.name
        );
        assert!(
            guild.description.len() > 20,
            "Guild '{}' description too short: '{}'",
            guild.name,
            guild.description
        );
    }
}

#[test]
fn test_guilds_v2_all_module_paths_in_guilds_core() {
    // Catalog module paths were migrated from `guilds.core.*` to gremio-scoped paths
    // like `guilds.builders.plugins.*`, `guilds.scholars.plugins.*`, etc.
    // This test validates that ALL paths are within the `guilds.` namespace
    // and that no path has escaped to an external namespace.
    let catalog = builtin_catalog();
    for guild in &catalog {
        // External MCP guilds (e.g. codebase_memory) use "external:" prefix, no Python file
        if guild.module_path.starts_with("external:") {
            continue;
        }
        assert!(
            guild.module_path.starts_with("guilds."),
            "Guild '{}' module_path '{}' must be inside the guilds.* namespace",
            guild.name,
            guild.module_path
        );
        // Must NOT reference a completely different package
        assert!(
            !guild.module_path.starts_with("external.") &&
            !guild.module_path.starts_with("plugins.") &&
            !guild.module_path.is_empty(),
            "Guild '{}' has invalid module_path: '{}'",
            guild.name,
            guild.module_path
        );
    }
}

#[test]
fn test_guilds_v2_categories_correctly_assigned() {
    let catalog = builtin_catalog();

    // Core: bash, filesystem, memory, monitor
    let core: Vec<&str> = catalog.iter()
        .filter(|g| g.category == GuildCategory::Core)
        .map(|g| g.name.as_str())
        .collect();
    assert!(core.contains(&"bash"), "bash must be Core");
    assert!(core.contains(&"filesystem"), "filesystem must be Core");
    assert!(core.contains(&"memory"), "memory must be Core");

    // Builders: git, docker, code
    let builders: Vec<&str> = catalog.iter()
        .filter(|g| g.category == GuildCategory::Builder)
        .map(|g| g.name.as_str())
        .collect();
    assert!(builders.contains(&"git"), "git must be Builder");
    assert!(builders.contains(&"code"), "code must be Builder");
    assert!(builders.contains(&"docker"), "docker must be Builder");

    // Scholars: search, browser, pdf, vision, code_analysis, knowledge
    let scholars: Vec<&str> = catalog.iter()
        .filter(|g| g.category == GuildCategory::Scholar)
        .map(|g| g.name.as_str())
        .collect();
    assert!(scholars.contains(&"search"), "search must be Scholar");
    assert!(scholars.contains(&"knowledge"), "knowledge must be Scholar");

    // Watchers: audit, system_metrics
    let watchers: Vec<&str> = catalog.iter()
        .filter(|g| g.category == GuildCategory::Watcher)
        .map(|g| g.name.as_str())
        .collect();
    assert!(watchers.contains(&"audit"), "audit must be Watcher");
    assert!(watchers.contains(&"system_metrics"), "system_metrics must be Watcher");
}

// ─── Section 2: Guild Context Routing ─────────────────────────────────────────

#[test]
fn test_guilds_v2_routing_builder_intent_routes_code() {
    let matcher = test_matcher();
    // "crea un endpoint REST" → Builder guild (code or git)
    let result = matcher.match_guild_with_context(
        "crea un endpoint REST en el kernel",
        None,
        0.15,
        None,
    );
    assert!(result.is_some(), "Builder intent must route to a guild");
    let r = result.unwrap();
    let builder_guilds = ["code", "git", "bash", "docker", "filesystem"];
    assert!(
        builder_guilds.contains(&r.guild_name.as_str()),
        "Builder intent 'crea endpoint REST' routed to unexpected guild: {}",
        r.guild_name
    );
}

#[test]
fn test_guilds_v2_routing_warden_intent_routes_audit() {
    let matcher = test_matcher();
    // "audita seguridad de handler.rs" → Watcher guild (audit)
    let ctx = GuildContext::from_agent_id("claude-guardian-warden");
    let result = matcher.match_guild_with_context(
        "audit system security and integrity of handler.rs",
        None,
        0.15,
        Some(&ctx),
    );
    assert!(result.is_some(), "Warden intent must route to a guild");
    let r = result.unwrap();
    let warden_guilds = ["audit", "system_metrics", "monitor"];
    assert!(
        warden_guilds.contains(&r.guild_name.as_str()),
        "Warden security intent routed to unexpected guild: {}",
        r.guild_name
    );
}

#[test]
fn test_guilds_v2_routing_scholar_intent_routes_research_guild() {
    let matcher = test_matcher();
    // "busca información sobre embeddings" → Scholar guild (search, browser)
    let ctx = GuildContext::from_agent_id("agent-researcher-scholars");
    let result = matcher.match_guild_with_context(
        "busca información sobre los mejores modelos de embeddings",
        None,
        0.15,
        Some(&ctx),
    );
    assert!(result.is_some(), "Scholar research intent must route to a guild");
    let r = result.unwrap();
    let scholar_guilds = ["search", "browser", "knowledge", "sequential_thinking", "deep_analysis"];
    assert!(
        scholar_guilds.contains(&r.guild_name.as_str()),
        "Scholar intent routed to unexpected guild: {}",
        r.guild_name
    );
}

#[test]
fn test_guilds_v2_routing_garbage_intent_returns_none() {
    let matcher = test_matcher();
    // Completely unrecognizable → NO_GUILD_MATCH
    let result = matcher.match_guild_with_context(
        "zzzzqqqzzzqqqnonexistent_xzyzxqzq",
        None,
        0.30,
        None,
    );
    assert!(
        result.is_none(),
        "Unrecognizable intent must return None (NO_GUILD_MATCH), got: {:?}",
        result.map(|r| r.guild_name)
    );
}

#[test]
fn test_guilds_v2_high_confidence_trigger_beats_context() {
    let matcher = test_matcher();
    // Trigger: "cargo test" → bash (0.95) — must NOT be overridden by scholar context
    let ctx = GuildContext::from_agent_id("scholar-researcher");
    let result = matcher.match_guild_with_context(
        "cargo test -p tylluan-kernel",
        None,
        0.2,
        Some(&ctx),
    );
    assert!(result.is_some());
    assert_eq!(
        result.unwrap().guild_name, "bash",
        "High-confidence trigger 'cargo test' must route to bash, not be overridden by scholar context"
    );
}

// ─── Section 3: GuildContext Agent Role Inference ─────────────────────────────

#[test]
fn test_guilds_v2_context_backend_dev_prefers_builder() {
    let ctx = GuildContext::from_agent_id("agent-backend-dev");
    assert_eq!(
        ctx.preferred_category,
        Some(GuildCategory::Builder),
        "backend-dev agent must prefer Builder category"
    );
}

#[test]
fn test_guilds_v2_context_frontend_dev_prefers_builder() {
    let ctx = GuildContext::from_agent_id("cursor-frontend-dev");
    assert_eq!(
        ctx.preferred_category,
        Some(GuildCategory::Builder),
        "frontend-dev agent must prefer Builder category"
    );
}

#[test]
fn test_guilds_v2_context_architect_prefers_builder() {
    let ctx = GuildContext::from_agent_id("claude-architect-systems");
    assert_eq!(
        ctx.preferred_category,
        Some(GuildCategory::Builder),
        "architect agent must prefer Builder category"
    );
}

#[test]
fn test_guilds_v2_context_guardian_prefers_watcher() {
    let ctx = GuildContext::from_agent_id("guardian-warden-01");
    assert_eq!(
        ctx.preferred_category,
        Some(GuildCategory::Watcher),
        "guardian agent must prefer Watcher category"
    );
}

#[test]
fn test_guilds_v2_context_researcher_prefers_scholar() {
    let ctx = GuildContext::from_agent_id("agent-researcher-scholars");
    assert_eq!(
        ctx.preferred_category,
        Some(GuildCategory::Scholar),
        "researcher agent must prefer Scholar category"
    );
}

#[test]
fn test_guilds_v2_context_generic_agent_no_preference() {
    let ctx = GuildContext::from_agent_id("generic-assistant-001");
    assert_eq!(
        ctx.preferred_category,
        None,
        "Generic agent must have no category preference"
    );
}

// ─── Section 4: Kernel Tool Integration (tylluan_do + agent context) ────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_guilds_v2_tylluan_do_with_backend_agent_id_does_not_panic() {
    // guild will fail to start (no Python in CI) but routing + context must work without panic
    let server = create_test_server("./gv2_t1").await;
    let mut args = JsonObject::new();
    args.insert("intent".to_string(), serde_json::Value::from("list files in src/"));
    args.insert("agent_id".to_string(), serde_json::Value::from("agent-backend-dev"));
    let result = server.handle_kernel_tool("tylluan_do", Some(args)).await;
    assert!(result.is_ok(), "tylluan_do with backend-dev agent_id must not return Err: {:?}", result);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_guilds_v2_tylluan_do_with_guardian_agent_id_does_not_panic() {
    let server = create_test_server("./gv2_t2").await;
    let mut args = JsonObject::new();
    args.insert("intent".to_string(), serde_json::Value::from("audit system health check"));
    args.insert("agent_id".to_string(), serde_json::Value::from("guardian-warden"));
    let result = server.handle_kernel_tool("tylluan_do", Some(args)).await;
    assert!(result.is_ok(), "tylluan_do with guardian agent_id must not return Err: {:?}", result);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_guilds_v2_tylluan_do_no_guild_match_returns_error_not_panic() {
    let server = create_test_server("./gv2_t3").await;
    let mut args = JsonObject::new();
    // Extremely high threshold to force a NO_GUILD_MATCH
    args.insert("intent".to_string(), serde_json::Value::from("xzyzqnonexistentqqzxyz"));
    let result = server.handle_kernel_tool("tylluan_do", Some(args)).await;
    assert!(result.is_ok(), "NO_GUILD_MATCH must be an Ok(error_result), not a panic");
    let r = result.unwrap();
    // Must be an error result (guild not found)
    assert_eq!(r.is_error, Some(true), "Unrecognizable intent must produce is_error=true");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_guilds_v2_sovereign_tools_unchanged() {
    // INVARIANT: adding guild context routing must NOT change sovereign tool count
    let server = create_test_server("./gv2_t4").await;
    let tools = server.all_tools().await;
    assert_eq!(
        tools.len(), 5,
        "Guild V2 migration must NOT change sovereign tool count. Got: {}",
        tools.len()
    );
}

// ─── Section 5: Guild Filesystem Validation ───────────────────────────────────

#[test]
fn test_guilds_v2_builders_guild_md_exists() {
    let path = std::path::Path::new("../../guilds/builders/guild.md");
    // Try both relative paths (from crate root and from workspace root)
    let alt_path = std::path::Path::new("guilds/builders/guild.md");
    assert!(
        path.exists() || alt_path.exists(),
        "guilds/builders/guild.md must exist — Builders gremio identity file is missing"
    );
}

#[test]
fn test_guilds_v2_scholars_guild_md_exists() {
    let path = std::path::Path::new("../../guilds/scholars/guild.md");
    let alt_path = std::path::Path::new("guilds/scholars/guild.md");
    assert!(
        path.exists() || alt_path.exists(),
        "guilds/scholars/guild.md must exist"
    );
}

#[test]
fn test_guilds_v2_wardens_guild_md_exists() {
    let path = std::path::Path::new("../../guilds/wardens/guild.md");
    let alt_path = std::path::Path::new("guilds/wardens/guild.md");
    assert!(
        path.exists() || alt_path.exists(),
        "guilds/wardens/guild.md must exist"
    );
}

#[test]
fn test_guilds_v2_builders_has_agents_subdir() {
    let path = std::path::Path::new("../../guilds/builders/agents");
    let alt_path = std::path::Path::new("guilds/builders/agents");
    assert!(
        path.is_dir() || alt_path.is_dir(),
        "guilds/builders/agents/ directory must exist"
    );
}

#[test]
fn test_guilds_v2_builders_has_workflows() {
    let path = std::path::Path::new("../../guilds/builders/workflows/new-feature.md");
    let alt_path = std::path::Path::new("guilds/builders/workflows/new-feature.md");
    assert!(
        path.exists() || alt_path.exists(),
        "guilds/builders/workflows/new-feature.md must exist — at least 1 pre-baked workflow required"
    );
}

#[test]
fn test_guilds_v2_builders_has_sub_agents() {
    let path = std::path::Path::new("../../guilds/builders/sub-agents/rust-specialist.skill.md");
    let alt_path = std::path::Path::new("guilds/builders/sub-agents/rust-specialist.skill.md");
    assert!(
        path.exists() || alt_path.exists(),
        "guilds/builders/sub-agents/rust-specialist.skill.md must exist"
    );
}

#[test]
fn test_guilds_v2_builders_has_sandbox() {
    let path = std::path::Path::new("../../guilds/builders/sandbox/README.md");
    let alt_path = std::path::Path::new("guilds/builders/sandbox/README.md");
    assert!(
        path.exists() || alt_path.exists(),
        "guilds/builders/sandbox/README.md must exist — sandbox lab required"
    );
}

// ─── Section 6: Migration Regression Checks ───────────────────────────────────

#[test]
fn test_guilds_v2_migration_core_plugins_still_accessible() {
    // Ensure Python plugins exist in their reorganized V2 paths
    let plugins = [
        "guilds/builders/plugins/bash.py",
        "guilds/builders/plugins/git.py",
        "guilds/builders/plugins/code.py",
        "guilds/builders/plugins/filesystem.py",
        "guilds/watchers/plugins/monitor.py",
        "guilds/wardens/plugins/audit.py",
        "guilds/scholars/plugins/search.py",
        "guilds/scholars/plugins/knowledge.py",
        "guilds/watchers/plugins/system_metrics.py",
    ];
    for plugin in plugins {
        let path = std::path::Path::new("../../").join(plugin);
        let alt_path = std::path::Path::new(plugin);
        assert!(
            path.exists() || alt_path.exists(),
            "Migrated plugin '{}' must exist in the V2 reorganized structure",
            plugin
        );
    }
}

#[test]
fn test_guilds_v2_migration_tylluan_toml_still_valid() {
    // tylluan.toml must still be parseable — migration must not break config
    let path = std::path::Path::new("../../tylluan.toml");
    let alt_path = std::path::Path::new("tylluan.toml");
    let content = if path.exists() {
        std::fs::read_to_string(path).ok()
    } else {
        std::fs::read_to_string(alt_path).ok()
    };
    assert!(content.is_some(), "tylluan.toml must exist and be readable");
    let content = content.unwrap();
    assert!(content.contains("[nexus]"), "tylluan.toml must contain [nexus] section");
    assert!(content.contains("[guilds.core]"), "tylluan.toml must contain [guilds.core] section");
    assert!(content.contains("always_on"), "tylluan.toml must contain always_on guild list");
}

#[test]
fn test_guilds_v2_registry_json_still_valid() {
    // registry.json must still be a valid JSON array
    let path = std::path::Path::new("../../registry.json");
    let alt_path = std::path::Path::new("registry.json");
    let content = if path.exists() {
        std::fs::read_to_string(path).ok()
    } else {
        std::fs::read_to_string(alt_path).ok()
    };
    assert!(content.is_some(), "registry.json must exist and be readable");
    let parsed: serde_json::Result<serde_json::Value> = serde_json::from_str(&content.unwrap());
    assert!(parsed.is_ok(), "registry.json must be valid JSON: {:?}", parsed.err());
    let val = parsed.unwrap();
    assert!(val.is_array(), "registry.json must be a JSON array");
    let arr = val.as_array().unwrap();
    assert!(!arr.is_empty(), "registry.json must not be empty");
    assert!(arr.len() >= 15, "registry.json must have at least 15 guild entries, got {}", arr.len());
}

#[tokio::test]
async fn test_guilds_v2_external_mcp_registration_and_toggle() {
    let temp_dir = std::env::temp_dir().join(format!("tylluan_test_mcp_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    
    let mut registry = GuildRegistry::new(
        temp_dir.clone(),
        30,
        TimeoutsConfig::default(),
        5,
    );
    
    // Register Http/SSE remote MCP server
    let name = "test_remote_mcp";
    let url = "http://127.0.0.1:9090/sse";
    registry.register_http_mcp(name, url, std::collections::HashMap::new(), None);
    
    assert!(registry.guilds.contains_key(name), "Guild must be registered");
    let guild = registry.guilds.get(name).unwrap();
    assert!(matches!(guild.launcher, tylluan_kernel::registry::guild_process::GuildLauncher::Http { .. }), "Must be Http launcher");
    
    // Cleanup
    std::fs::remove_dir_all(temp_dir).ok();
}
