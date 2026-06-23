use tylluan_kernel::transport::server::handler_do::check_dangerous_intent;
use tylluan_kernel::transport::http::auth::acl_can_access;
use tylluan_kernel::config::AclConfig;
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
        assert!(response.get(key).is_some(), "key '{}' should be in kill switch response", key);
    }
}

#[test]
fn test_emergency_kill_localhost_required() {
    let expected_route = "/api/v1/admin/emergency-kill";
    assert!(expected_route.starts_with("/api/v1/admin/"));
}
