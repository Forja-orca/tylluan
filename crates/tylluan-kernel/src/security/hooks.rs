//! M31-P0 — deterministic pre/post hooks around the sovereign tools.
//!
//! Every MCP client (Claude Desktop, Claude Code, LM Studio, Qwen, whatever
//! connects next) goes through the same `handle_kernel_tool` dispatch point.
//! Hooks configured here run once, there, for all of them -- not per-client
//! logic bolted onto each integration separately. Deterministic on purpose:
//! no LLM call in this path (matches the project's existing "no LLM in
//! guild tools" sovereignty rule) -- regex pattern matching against tool
//! name + args/result, nothing more clever than that by design.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;
use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HookPhase {
    Pre,
    Post,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookAction {
    /// Block the call entirely. `message` is returned as the tool error.
    Deny,
    /// Replace every regex match with `replacement` (default: "[REDACTED]")
    /// in the matched field before the call proceeds (pre) or before the
    /// result is returned to the client (post).
    Redact,
    /// Prepend `inject` as a text block ahead of the tool's own output.
    /// Pre-phase: prepended to `intent`/`content` before dispatch.
    /// Post-phase: prepended to the result content.
    InjectContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookRule {
    /// Sovereign tool name to match ("tylluan_remember", ...), or "*" for all.
    pub tool: String,
    pub phase: HookPhase,
    /// Regex applied against the relevant text field (see field selection
    /// below). Required for Redact, optional for Deny (absent = always
    /// matches, i.e. an unconditional block) and InjectContext.
    #[serde(default)]
    pub pattern: Option<String>,
    pub action: HookAction,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub replacement: Option<String>,
    #[serde(default)]
    pub inject: Option<String>,
}

/// Compiled regex cache -- rules are static for the process lifetime (only
/// change via a full config reload, which clears this), so compile once
/// instead of on every tool call.
static REGEX_CACHE: OnceLock<std::sync::RwLock<HashMap<String, Regex>>> = OnceLock::new();

fn compiled(pattern: &str) -> Option<Regex> {
    let cache = REGEX_CACHE.get_or_init(|| std::sync::RwLock::new(HashMap::new()));
    if let Some(re) = cache.read().ok().and_then(|c| c.get(pattern).cloned()) {
        return Some(re);
    }
    let re = Regex::new(pattern).ok()?;
    if let Ok(mut w) = cache.write() {
        w.insert(pattern.to_string(), re.clone());
    }
    Some(re)
}

/// Clear the compiled-regex cache. Call after a config reload in case hook
/// patterns changed -- otherwise stale compiled regexes would keep running.
pub fn clear_regex_cache() {
    if let Some(cache) = REGEX_CACHE.get() {
        if let Ok(mut w) = cache.write() {
            w.clear();
        }
    }
}

fn rule_matches_tool(rule: &HookRule, tool_name: &str) -> bool {
    rule.tool == "*" || rule.tool == tool_name
}

/// The text field each hook inspects/mutates, per tool. Kept simple and
/// explicit rather than a generic "walk every string field" -- these are
/// the fields users actually write free text into.
fn text_field_name(tool_name: &str) -> &'static str {
    match tool_name {
        "tylluan_remember" | "tylluan_ingest" => "content",
        "tylluan_recall" | "tylluan_think" => "query",
        _ => "intent", // tylluan_do, tylluan_graph, and anything else
    }
}

pub enum PreHookOutcome {
    Continue,
    Deny(String),
}

/// Run all matching `pre` hooks against the outgoing call. Redact/InjectContext
/// mutate `args` in place; Deny short-circuits with the configured message.
pub fn run_pre_hooks(rules: &[HookRule], tool_name: &str, args: &mut serde_json::Map<String, Value>) -> PreHookOutcome {
    let field = text_field_name(tool_name);
    for rule in rules {
        if rule.phase != HookPhase::Pre || !rule_matches_tool(rule, tool_name) {
            continue;
        }
        let text = args.get(field).and_then(|v| v.as_str()).unwrap_or("").to_string();
        match rule.action {
            HookAction::Deny => {
                let matched = match &rule.pattern {
                    None => true,
                    Some(p) => compiled(p).map(|re| re.is_match(&text)).unwrap_or(false),
                };
                if matched {
                    return PreHookOutcome::Deny(
                        rule.message.clone().unwrap_or_else(|| format!("Blocked by pre-hook on '{}'", tool_name))
                    );
                }
            }
            HookAction::Redact => {
                if let Some(re) = rule.pattern.as_deref().and_then(compiled) {
                    let replacement = rule.replacement.as_deref().unwrap_or("[REDACTED]");
                    let redacted = re.replace_all(&text, replacement).to_string();
                    if redacted != text {
                        args.insert(field.to_string(), Value::String(redacted));
                    }
                }
            }
            HookAction::InjectContext => {
                if let Some(inject) = &rule.inject {
                    let combined = format!("{}\n{}", inject, text);
                    args.insert(field.to_string(), Value::String(combined));
                }
            }
        }
    }
    PreHookOutcome::Continue
}

/// Run all matching `post` hooks against the result about to be returned.
/// Only Redact/InjectContext apply here -- a Deny rule on `post` is a no-op
/// by design (the call already happened; denying the response back to the
/// client without denying the call itself would be a confusing half-measure,
/// not a real security boundary. Block on `pre` instead).
pub fn run_post_hooks(rules: &[HookRule], tool_name: &str, content_texts: &mut [String]) {
    for rule in rules {
        if rule.phase != HookPhase::Post || !rule_matches_tool(rule, tool_name) {
            continue;
        }
        match rule.action {
            HookAction::Redact => {
                if let Some(re) = rule.pattern.as_deref().and_then(compiled) {
                    let replacement = rule.replacement.as_deref().unwrap_or("[REDACTED]");
                    for text in content_texts.iter_mut() {
                        let redacted = re.replace_all(text, replacement).to_string();
                        *text = redacted;
                    }
                }
            }
            HookAction::InjectContext => {
                if let Some(inject) = &rule.inject {
                    for text in content_texts.iter_mut() {
                        *text = format!("{}\n{}", inject, text);
                    }
                }
            }
            HookAction::Deny => {} // no-op on post, see doc comment above
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args_with(field: &str, value: &str) -> serde_json::Map<String, Value> {
        let mut m = serde_json::Map::new();
        m.insert(field.to_string(), Value::String(value.to_string()));
        m
    }

    #[test]
    fn test_deny_unconditional() {
        let rules = vec![HookRule {
            tool: "tylluan_remember".into(), phase: HookPhase::Pre, pattern: None,
            action: HookAction::Deny, message: Some("nope".into()), replacement: None, inject: None,
        }];
        let mut args = args_with("content", "anything");
        match run_pre_hooks(&rules, "tylluan_remember", &mut args) {
            PreHookOutcome::Deny(msg) => assert_eq!(msg, "nope"),
            _ => panic!("expected deny"),
        }
    }

    #[test]
    fn test_deny_with_pattern_only_blocks_match() {
        let rules = vec![HookRule {
            tool: "*".into(), phase: HookPhase::Pre,
            pattern: Some(r"\b\d{3}-\d{2}-\d{4}\b".into()),
            action: HookAction::Deny, message: Some("PII detected".into()), replacement: None, inject: None,
        }];
        let mut safe = args_with("intent", "hello world");
        assert!(matches!(run_pre_hooks(&rules, "tylluan_do", &mut safe), PreHookOutcome::Continue));

        let mut unsafe_args = args_with("intent", "my ssn is 123-45-6789");
        match run_pre_hooks(&rules, "tylluan_do", &mut unsafe_args) {
            PreHookOutcome::Deny(msg) => assert_eq!(msg, "PII detected"),
            _ => panic!("expected deny"),
        }
    }

    #[test]
    fn test_redact_pre() {
        let rules = vec![HookRule {
            tool: "tylluan_remember".into(), phase: HookPhase::Pre,
            pattern: Some(r"\b\d{3}-\d{2}-\d{4}\b".into()),
            action: HookAction::Redact, message: None, replacement: Some("[SSN]".into()), inject: None,
        }];
        let mut args = args_with("content", "call 123-45-6789 back");
        run_pre_hooks(&rules, "tylluan_remember", &mut args);
        assert_eq!(args.get("content").unwrap().as_str().unwrap(), "call [SSN] back");
    }

    #[test]
    fn test_inject_context_pre() {
        let rules = vec![HookRule {
            tool: "*".into(), phase: HookPhase::Pre, pattern: None,
            action: HookAction::InjectContext, message: None, replacement: None,
            inject: Some("[audited-session]".into()),
        }];
        let mut args = args_with("intent", "do the thing");
        run_pre_hooks(&rules, "tylluan_do", &mut args);
        assert_eq!(args.get("intent").unwrap().as_str().unwrap(), "[audited-session]\ndo the thing");
    }

    #[test]
    fn test_wildcard_tool_matches_all() {
        let rules = vec![HookRule {
            tool: "*".into(), phase: HookPhase::Pre, pattern: None,
            action: HookAction::Deny, message: Some("blocked".into()), replacement: None, inject: None,
        }];
        for tool in ["tylluan_do", "tylluan_remember", "tylluan_recall", "tylluan_think", "tylluan_graph"] {
            let mut args = args_with("intent", "x");
            assert!(matches!(run_pre_hooks(&rules, tool, &mut args), PreHookOutcome::Deny(_)));
        }
    }

    #[test]
    fn test_non_matching_tool_is_ignored() {
        let rules = vec![HookRule {
            tool: "tylluan_recall".into(), phase: HookPhase::Pre, pattern: None,
            action: HookAction::Deny, message: Some("blocked".into()), replacement: None, inject: None,
        }];
        let mut args = args_with("intent", "x");
        assert!(matches!(run_pre_hooks(&rules, "tylluan_do", &mut args), PreHookOutcome::Continue));
    }

    #[test]
    fn test_post_redact() {
        let rules = vec![HookRule {
            tool: "*".into(), phase: HookPhase::Post,
            pattern: Some(r"secret-\w+".into()),
            action: HookAction::Redact, message: None, replacement: None, inject: None,
        }];
        let mut texts = vec!["token=secret-abc123".to_string()];
        run_post_hooks(&rules, "tylluan_recall", &mut texts);
        assert_eq!(texts[0], "token=[REDACTED]");
    }

    #[test]
    fn test_post_deny_is_noop() {
        let rules = vec![HookRule {
            tool: "*".into(), phase: HookPhase::Post, pattern: None,
            action: HookAction::Deny, message: Some("should not apply".into()), replacement: None, inject: None,
        }];
        let mut texts = vec!["unchanged".to_string()];
        run_post_hooks(&rules, "tylluan_do", &mut texts);
        assert_eq!(texts[0], "unchanged");
    }

    #[test]
    fn test_wrong_phase_ignored() {
        let rules = vec![HookRule {
            tool: "*".into(), phase: HookPhase::Post, pattern: None,
            action: HookAction::Deny, message: Some("x".into()), replacement: None, inject: None,
        }];
        let mut args = args_with("intent", "x");
        assert!(matches!(run_pre_hooks(&rules, "tylluan_do", &mut args), PreHookOutcome::Continue));
    }
}
