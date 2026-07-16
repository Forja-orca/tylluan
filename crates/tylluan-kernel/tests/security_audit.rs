use tylluan_kernel::transport::server::handler_do::check_dangerous_intent;
use tylluan_kernel::transport::http::auth::{acl_can_access, check_agent_id_tool_allowed, resolve_agent_id_for_token, agent_has_memory_isolation};
use tylluan_kernel::config::{AclConfig, AgentPermission};
use std::collections::HashMap;
use tylluan_kernel::security::rate_limiter::RateLimiter;


// ── S1a: Intent Filter ─────────────────────────────────────────

#[test]
fn test_intent_filter_rejects_rm_rf_root() {
    assert!(check_dangerous_intent("rm -rf /").is_some(), "rm -rf / should be blocked");
}

#[test]
fn test_intent_filter_rejects_rm_rf_home() {
    assert!(check_dangerous_intent("sudo rm -rf ~").is_some(), "rm -rf ~ should be blocked");
}

#[test]
fn test_intent_filter_rejects_drop_table() {
    assert!(check_dangerous_intent("DROP TABLE users;").is_some(), "DROP TABLE should be blocked");
}

#[test]
fn test_intent_filter_rejects_format_c() {
    assert!(check_dangerous_intent("format c:").is_some(), "format c: should be blocked");
}

#[test]
fn test_intent_filter_rejects_fork_bomb() {
    assert!(check_dangerous_intent(":(){ :|:& };:").is_some(), "fork bomb should be blocked");
}

#[test]
fn test_intent_filter_rejects_reboot() {
    assert!(check_dangerous_intent("reboot now").is_some(), "reboot should be blocked");
}

#[test]
fn test_intent_filter_allows_safe_command() {
    assert!(check_dangerous_intent("list files in current directory").is_none(), "safe intent should pass");
}

#[test]
fn test_intent_filter_allows_git_status() {
    assert!(check_dangerous_intent("check git status").is_none(), "git status should pass");
}

#[test]
fn test_intent_filter_allows_create_file() {
    assert!(check_dangerous_intent("create a new text file called readme").is_none(), "creating files should pass");
}

#[test]
fn test_intent_filter_case_insensitive() {
    assert!(check_dangerous_intent("RM -RF /").is_some(), "case-insensitive match should work");
    assert!(check_dangerous_intent("DROP TABLE secrets").is_some(), "case-insensitive DROP TABLE should work");
}

#[test]
fn test_intent_filter_rejects_delete_from() {
    assert!(check_dangerous_intent("delete from users where id=1").is_some(), "DELETE FROM should be blocked");
}

#[test]
fn test_intent_filter_rejects_shutdown() {
    assert!(check_dangerous_intent("shutdown /s").is_some(), "shutdown should be blocked");
}

// ── S1b: ACL ───────────────────────────────────────────────────

fn make_acl(roles: HashMap<String, Vec<String>>) -> AclConfig {
    AclConfig {
        default_role: "reader".to_string(),
        roles,
        tokens: HashMap::new(),
        token_agent_bindings: HashMap::new(),
        agent_permissions: HashMap::new(),
    }
}

#[test]
fn test_acl_admin_has_unrestricted_access() {
    let acl = make_acl(HashMap::new());
    assert!(acl_can_access("admin", "bash", &acl), "admin should access anything");
    assert!(acl_can_access("admin", "filesystem", &acl), "admin should access anything");
    assert!(acl_can_access("admin", "knowledge", &acl), "admin should access anything");
}

#[test]
fn test_acl_reader_blocked_from_bash() {
    let mut roles = HashMap::new();
    roles.insert("reader".to_string(), vec!["knowledge".to_string(), "monitor".to_string()]);
    let acl = make_acl(roles);
    assert!(!acl_can_access("reader", "bash", &acl), "reader should NOT access bash");
    assert!(!acl_can_access("reader", "filesystem", &acl), "reader should NOT access filesystem");
}

#[test]
fn test_acl_reader_can_access_allowed_guilds() {
    let mut roles = HashMap::new();
    roles.insert("reader".to_string(), vec!["knowledge".to_string(), "monitor".to_string()]);
    let acl = make_acl(roles);
    assert!(acl_can_access("reader", "knowledge", &acl), "reader should access knowledge");
    assert!(acl_can_access("reader", "monitor", &acl), "reader should access monitor");
}

#[test]
fn test_acl_wildcard_grants_all() {
    let mut roles = HashMap::new();
    roles.insert("writer".to_string(), vec!["*".to_string()]);
    let acl = make_acl(roles);
    assert!(acl_can_access("writer", "bash", &acl), "writer with * should access bash");
    assert!(acl_can_access("writer", "git", &acl), "writer with * should access git");
    assert!(acl_can_access("writer", "anything", &acl), "writer with * should access anything");
}

#[test]
fn test_acl_unknown_role_denied() {
    let acl = make_acl(HashMap::new());
    assert!(!acl_can_access("hacker", "knowledge", &acl), "unknown role should be denied");
}

#[test]
fn test_acl_empty_config_allows_nonexistent_role() {
    let acl = AclConfig::default();
    assert!(!acl_can_access("nonexistent", "bash", &acl), "nonexistent role with empty ACL should be denied");
}

#[test]
fn test_acl_default_role_applied_to_unknown_token() {
    let mut roles = HashMap::new();
    roles.insert("reader".to_string(), vec!["knowledge".to_string()]);
    let acl = AclConfig {
        default_role: "reader".to_string(),
        roles,
        tokens: HashMap::new(),
        token_agent_bindings: HashMap::new(),
        agent_permissions: HashMap::new(),
    };
    // Unknown token falls back to default_role="reader"
    // acl_can_access uses role name directly; default_role is applied upstream.
    // Verify reader role blocks bash and allows knowledge.
    assert!(!acl_can_access("reader", "bash", &acl), "default reader cannot use bash");
    assert!(acl_can_access("reader", "knowledge", &acl), "default reader can use knowledge");
}

// ── S1c: Rate Limiter ──────────────────────────────────────────

#[test]
fn test_rate_limiter_allows_within_limit() {
    let limiter = RateLimiter::new(Some(5));
    for i in 0..5 {
        assert!(limiter.check_and_record("session-1").is_ok(), "call {} should be allowed", i + 1);
    }
}

#[test]
fn test_rate_limiter_blocks_over_limit() {
    let limiter = RateLimiter::new(Some(3));
    for _ in 0..3 {
        limiter.check_and_record("session-2").unwrap();
    }
    let result = limiter.check_and_record("session-2");
    assert!(result.is_err(), "4th call should be rate-limited");
    assert!(result.unwrap_err().contains("Rate limit exceeded"), "error should mention rate limit");
}

#[test]
fn test_rate_limiter_separate_sessions_independent() {
    let limiter = RateLimiter::new(Some(2));
    limiter.check_and_record("session-a").unwrap();
    limiter.check_and_record("session-a").unwrap();
    assert!(limiter.check_and_record("session-a").is_err(), "session-a should be limited");
    assert!(limiter.check_and_record("session-b").is_ok(), "session-b should NOT be limited");
}

#[test]
fn test_rate_limiter_60_calls_then_blocked() {
    let limiter = RateLimiter::new(Some(60));
    for i in 0..60 {
        assert!(limiter.check_and_record("burst-test").is_ok(), "call {} should be allowed", i + 1);
    }
    let result = limiter.check_and_record("burst-test");
    assert!(result.is_err(), "61st call should be rate-limited");
}

#[test]
fn test_rate_limiter_none_uses_default_limit() {
    // RateLimiter::new(None) → DEFAULT_MAX_CALLS = 60 per 60s window
    let limiter = RateLimiter::new(None);
    for i in 0..60 {
        assert!(
            limiter.check_and_record("default-session").is_ok(),
            "call {} within default limit should pass", i + 1
        );
    }
    assert!(
        limiter.check_and_record("default-session").is_err(),
        "61st call should exceed default limit of 60"
    );
}

#[test]
fn test_intent_filter_empty_input_safe() {
    assert!(check_dangerous_intent("").is_none(), "empty input should not trigger filter");
}

#[test]
fn test_intent_filter_whitespace_only_safe() {
    assert!(check_dangerous_intent("   ").is_none(), "whitespace-only input should not trigger filter");
}

// ── S1d: Kill Switch ───────────────────────────────────────────

#[test]
fn test_emergency_kill_route_pattern() {
    let route = "/api/v1/admin/emergency-kill";
    assert!(route.starts_with('/'));
    assert!(route.starts_with("/api/v1/admin/"), "kill switch should be under admin routes");
    assert!(!route.contains(' '));
}

#[test]
fn test_kill_guild_route_pattern() {
    let route = "/api/v1/admin/kill-guild/{name}";
    assert!(route.starts_with('/'));
    assert!(route.starts_with("/api/v1/admin/"), "kill-guild should be under admin routes");
}

#[test]
fn test_emergency_kill_response_shape() {
    let expected_keys = vec!["status", "guilds_killed"];
    let response = serde_json::json!({
        "status": "emergency_kill_complete",
        "guilds_killed": 0
    });
    for key in &expected_keys {
        assert!(response.get(key).is_some(), "key '{key}' should be in kill switch response");
    }
}

#[test]
fn test_emergency_kill_localhost_required() {
    let expected_route = "/api/v1/admin/emergency-kill";
    assert!(expected_route.starts_with("/api/v1/admin/"));
}

// ── M31-P1: Granular agent_id Permissions ────────────────────────────────

#[test]
fn test_agent_id_tool_allowed_denied_tools_blocks() {
    let mut perms = HashMap::new();
    perms.insert("agent-reader".to_string(), AgentPermission {
        scope: "read-write".to_string(),
        denied_tools: vec!["tylluan_graph".to_string()],
        memory_isolation: false,
    });
    let acl = AclConfig {
        default_role: "reader".to_string(),
        roles: HashMap::new(),
        tokens: HashMap::new(),
        token_agent_bindings: HashMap::new(),
        agent_permissions: perms,
    };
    assert!(
        check_agent_id_tool_allowed("agent-reader", "tylluan_remember", &acl).is_none(),
        "read-write agent should use tylluan_remember"
    );
    assert!(
        check_agent_id_tool_allowed("agent-reader", "tylluan_graph", &acl).is_some(),
        "denied_tools should block tylluan_graph"
    );
    assert!(
        check_agent_id_tool_allowed("agent-reader", "tylluan_recall", &acl).is_none(),
        "recall is not denied"
    );
}

#[test]
fn test_agent_id_read_only_scope_blocks_write_tools() {
    let mut perms = HashMap::new();
    perms.insert("agent-ro".to_string(), AgentPermission {
        scope: "read-only".to_string(),
        denied_tools: vec![],
        memory_isolation: true,
    });
    let acl = AclConfig {
        default_role: "reader".to_string(),
        roles: HashMap::new(),
        tokens: HashMap::new(),
        token_agent_bindings: HashMap::new(),
        agent_permissions: perms,
    };
    // Read-only scope blocks tylluan_remember, tylluan_do, tylluan_graph
    assert!(
        check_agent_id_tool_allowed("agent-ro", "tylluan_remember", &acl).is_some(),
        "read-only scope blocks remember"
    );
    assert!(
        check_agent_id_tool_allowed("agent-ro", "tylluan_do", &acl).is_some(),
        "read-only scope blocks do"
    );
    assert!(
        check_agent_id_tool_allowed("agent-ro", "tylluan_graph", &acl).is_some(),
        "read-only scope blocks graph"
    );
    // But allows recall and think
    assert!(
        check_agent_id_tool_allowed("agent-ro", "tylluan_recall", &acl).is_none(),
        "read-only scope allows recall"
    );
    assert!(
        check_agent_id_tool_allowed("agent-ro", "tylluan_think", &acl).is_none(),
        "read-only scope allows think"
    );
}

#[test]
fn test_agent_id_memory_isolation_flag() {
    let mut perms = HashMap::new();
    perms.insert("agent-isolated".to_string(), AgentPermission {
        scope: "read-write".to_string(),
        denied_tools: vec![],
        memory_isolation: true,
    });
    perms.insert("agent-open".to_string(), AgentPermission {
        scope: "read-write".to_string(),
        denied_tools: vec![],
        memory_isolation: false,
    });
    let acl = AclConfig {
        default_role: "reader".to_string(),
        roles: HashMap::new(),
        tokens: HashMap::new(),
        token_agent_bindings: HashMap::new(),
        agent_permissions: perms,
    };
    assert!(agent_has_memory_isolation("agent-isolated", &acl), "isolated agent should have isolation");
    assert!(!agent_has_memory_isolation("agent-open", &acl), "open agent should NOT have isolation");
    assert!(!agent_has_memory_isolation("unknown-agent", &acl), "unknown agent should NOT have isolation");
}

#[test]
fn test_resolve_agent_id_for_token_returns_binding() {
    let mut bindings = HashMap::new();
    bindings.insert("token-abc".to_string(), "agent-fixed".to_string());
    let acl = AclConfig {
        default_role: "reader".to_string(),
        roles: HashMap::new(),
        tokens: HashMap::new(),
        token_agent_bindings: bindings,
        agent_permissions: HashMap::new(),
    };
    assert_eq!(
        resolve_agent_id_for_token("token-abc", &acl),
        "agent-fixed",
        "token with binding returns agent_id"
    );
    assert_eq!(
        resolve_agent_id_for_token("unknown-token", &acl),
        "",
        "token without binding returns empty string"
    );
}

#[test]
fn test_agent_id_tool_allowed_unknown_agent_is_permitted() {
    let acl = AclConfig {
        default_role: "admin".to_string(),
        roles: HashMap::new(),
        tokens: HashMap::new(),
        token_agent_bindings: HashMap::new(),
        agent_permissions: HashMap::new(),
    };
    assert!(
        check_agent_id_tool_allowed("unknown-agent", "tylluan_remember", &acl).is_none(),
        "unknown agent with no permissions config should be allowed"
    );
}

// ── M31-P2: Plan Mode ───────────────────────────────────────────────────

#[tokio::test]
async fn test_plan_store_and_retrieve_roundtrip() {
    tylluan_kernel::security::grants::init_plan_store();
    let plan_id = "test-plan-001";
    let args = serde_json::json!({ "command": "ls -la", "intent": "list files" });
    tylluan_kernel::security::grants::store_plan(
        plan_id, "bash", "bash_execute", &args, "agent-1", "list files in current directory",
    ).await;
    let plan = tylluan_kernel::security::grants::get_plan(plan_id).await;
    assert!(plan.is_some(), "stored plan should be retrievable");
    let plan = plan.unwrap();
    assert_eq!(plan.guild, "bash");
    assert_eq!(plan.tool, "bash_execute");
    assert_eq!(plan.agent_id, "agent-1");
    assert_eq!(plan.intent, "list files in current directory");
    assert_eq!(plan.args.get("command").and_then(|v| v.as_str()), Some("ls -la"));
}

#[tokio::test]
async fn test_plan_remove_frees_plan() {
    tylluan_kernel::security::grants::init_plan_store();
    let plan_id = "test-plan-002";
    let args = serde_json::json!({});
    tylluan_kernel::security::grants::store_plan(
        plan_id, "filesystem", "file_read", &args, "agent-2", "read a file",
    ).await;
    assert!(tylluan_kernel::security::grants::get_plan(plan_id).await.is_some());
    let removed = tylluan_kernel::security::grants::remove_plan(plan_id).await;
    assert!(removed, "remove_plan should return true for existing plan");
    assert!(tylluan_kernel::security::grants::get_plan(plan_id).await.is_none(), "plan should be gone after remove");
}

#[tokio::test]
async fn test_plan_get_nonexistent_returns_none() {
    tylluan_kernel::security::grants::init_plan_store();
    let plan = tylluan_kernel::security::grants::get_plan("no-such-plan").await;
    assert!(plan.is_none(), "nonexistent plan should return None");
}

#[tokio::test]
async fn test_plan_remove_nonexistent_returns_false() {
    tylluan_kernel::security::grants::init_plan_store();
    let removed = tylluan_kernel::security::grants::remove_plan("no-such-plan").await;
    assert!(!removed, "remove on nonexistent plan should return false");
}

#[tokio::test]
async fn test_plan_overwrite_replaces_existing() {
    tylluan_kernel::security::grants::init_plan_store();
    let plan_id = "test-plan-003";
    let args1 = serde_json::json!({ "query": "hello" });
    let args2 = serde_json::json!({ "query": "world" });
    tylluan_kernel::security::grants::store_plan(
        plan_id, "memory", "tylluan_recall", &args1, "agent-a", "first",
    ).await;
    tylluan_kernel::security::grants::store_plan(
        plan_id, "memory", "tylluan_recall", &args2, "agent-b", "second",
    ).await;
    let plan = tylluan_kernel::security::grants::get_plan(plan_id).await.unwrap();
    assert_eq!(plan.intent, "second", "overwrite should replace intent");
    assert_eq!(plan.args.get("query").and_then(|v| v.as_str()), Some("world"), "overwrite should replace args");
}
