use rmcp::model::*;
use tracing::info;
use crate::registry::proxy::error_result;
use crate::config::TylluanConfig;
use super::TylluanServer;
use crate::router::matcher::{tokenize, keyword_score};
use super::routing::record_activity_trace;
use super::audit_or_warn;

/// After resolve_guild_name fails, attempt dispatch to a registered external MCP server.
///
/// Strategy (in order):
/// 1. If intent explicitly names a server (or a substring matches a server name), use it.
/// 2. Otherwise, keyword-score all tools across all active external servers, pick the best
///    match above MIN_SCORE.
///
/// Returns None when no external server matches — the caller should fall through to
/// the normal "no guild found" error.
pub(super) async fn try_external_mcp_dispatch(
    server: &TylluanServer,
    intent: &str,
    agent_id: Option<&str>,
    explicit_args: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Option<CallToolResult> {
    if intent.trim().is_empty() {
        return None;
    }

    let external_servers = {
        let config = TylluanConfig::load_cached().ok()?;
        let cfg = config.read().await;
        cfg.external_mcp.iter()
            .filter(|s| s.active)
            .map(|s| s.name.clone())
            .collect::<Vec<_>>()
    };

    if external_servers.is_empty() {
        return None;
    }

    let intent_lower = intent.to_lowercase();
    let tokens = tokenize(intent);

    // Phase 1: explicit server name in intent
    for name in &external_servers {
        let name_lower = name.to_lowercase();
        if intent_lower.contains(&name_lower) || intent_lower.contains(&name_lower.replace('_', " ")) {
            info!("external_mcp: explicit server name '{}' matched in intent", name);
            return dispatch_to_external(server, name, &tokens, intent, agent_id, explicit_args).await;
        }
    }

    // Phase 2: find the best tool match across all external servers
    let reg = server.registry.read().await;
    let mut best_score = 0.40_f32;
    let mut best: Option<(String, String)> = None; // (server_name, tool_name)

    for name in &external_servers {
        if let Some(guild) = reg.guilds.get(name) {
            if !guild.is_running() {
                continue;
            }
            for tool in &guild.tools {
                let score = keyword_score(&tokens, tool.description.as_ref(), tool.name.as_ref());
                if score > best_score {
                    best_score = score;
                    best = Some((name.clone(), tool.name.to_string()));
                }
            }
        }
    }

    if let Some((server_name, tool_name)) = best {
        info!(
            "external_mcp: routing '{}' → server='{}' tool='{}' score={:.2}",
            intent, server_name, tool_name, best_score
        );
        return Some(do_external_call(server, &server_name, &tool_name, intent, agent_id, explicit_args).await);
    }

    None
}

async fn dispatch_to_external(
    server: &TylluanServer,
    server_name: &str,
    tokens: &[String],
    intent: &str,
    agent_id: Option<&str>,
    explicit_args: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Option<CallToolResult> {
    let reg = server.registry.read().await;
    if let Some(guild) = reg.guilds.get(server_name) {
        if !guild.is_running() {
            return Some(error_result(&format!(
                "External MCP server '{server_name}' is registered but not running. Use the MCP API to restart it."
            )));
        }
        if guild.tools.is_empty() {
            return Some(error_result(&format!(
                "External MCP server '{server_name}' has no tools available."
            )));
        }

        // Pick best tool within this server
        let tool_name = guild.tools.iter()
            .max_by(|a, b| {
                let sa = keyword_score(tokens, a.description.as_ref(), a.name.as_ref());
                let sb = keyword_score(tokens, b.description.as_ref(), b.name.as_ref());
                sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|t| t.name.to_string())
            .unwrap_or_else(|| guild.tools[0].name.to_string());

        info!(
            "external_mcp: explicit server '{}' → tool='{}'",
            server_name, tool_name
        );
        Some(do_external_call(server, server_name, &tool_name, intent, agent_id, explicit_args).await)
    } else {
        Some(error_result(&format!(
            "External MCP server '{server_name}' is registered in config but not in the guild registry."
        )))
    }
}

/// D-48: el evento `external_mcp_call` del dashboard no debe llevar el intent
/// completo (puede contener secretos o datos personales). La auditoría ya
/// recorta su preview; el dashboard usa el mismo límite de 200 caracteres.
const DASHBOARD_INTENT_MAX_CHARS: usize = 200;

fn dashboard_intent_preview(intent: &str) -> String {
    intent.chars().take(DASHBOARD_INTENT_MAX_CHARS).collect()
}

/// D-93: `tylluan_do(..., args={...})` carries explicit arguments for an
/// external tool. Optional: when absent the intent-only path applies.
pub(super) fn explicit_args_from(
    arguments: &Option<serde_json::Map<String, serde_json::Value>>,
) -> Option<&serde_json::Map<String, serde_json::Value>> {
    arguments
        .as_ref()
        .and_then(|a| a.get("args"))
        .and_then(|v| v.as_object())
}

/// D-93: `args` must be a JSON object. Silently ignoring a malformed value
/// would fall back to the intent-only path and send a guessed call.
pub(super) fn explicit_args_error(
    arguments: &Option<serde_json::Map<String, serde_json::Value>>,
) -> Option<String> {
    match arguments.as_ref().and_then(|a| a.get("args")) {
        None => None,
        Some(serde_json::Value::Object(_)) => None,
        Some(other) => Some(format!(
            "args must be a JSON object with the external tool's arguments, got: {}",
            match other {
                serde_json::Value::String(_) => "a string",
                serde_json::Value::Array(_) => "an array",
                serde_json::Value::Number(_) => "a number",
                serde_json::Value::Bool(_) => "a boolean",
                serde_json::Value::Null => "null",
                serde_json::Value::Object(_) => "an object",
            }
        )),
    }
}

/// D-93 (ADR-023 P-1): the old `do_external_call` filled eight fields with the
/// same text (`command`, `query`, `text`…) without looking at the tool schema.
/// Arguments are now built from the declared `inputSchema`: explicit args are
/// validated, and an intent-only call fills only the fields the schema declares
/// and the intent actually names. If the required fields cannot be filled, the
/// caller gets an actionable error instead of a silent guess.
pub(super) fn build_external_tool_args(
    schema: &serde_json::Value,
    intent: &str,
    explicit: Option<&serde_json::Map<String, serde_json::Value>>,
    timeout_secs: u64,
) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let empty = serde_json::Map::new();
    let properties = schema
        .get("properties")
        .and_then(|v| v.as_object())
        .unwrap_or(&empty);
    let required: Vec<String> = schema
        .get("required")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    // Undeclared schema: nothing to validate against, pass the args through.
    if properties.is_empty() && required.is_empty() {
        if let Some(explicit) = explicit {
            return Ok(explicit.clone());
        }
        return Err(
            "this external tool declares no inputSchema: call it again with args={...} and the arguments it expects".to_string(),
        );
    }

    if let Some(explicit) = explicit {
        let unknown: Vec<&str> = explicit
            .keys()
            .filter(|k| !properties.contains_key(*k))
            .map(String::as_str)
            .collect();
        let missing: Vec<&str> = required
            .iter()
            .filter(|k| {
                !explicit.contains_key(*k) && !(*k == "timeout_secs" && properties.contains_key("timeout_secs"))
            })
            .map(String::as_str)
            .collect();
        if !unknown.is_empty() || !missing.is_empty() {
            let mut msg = String::from("args do not match the tool inputSchema");
            if !unknown.is_empty() {
                msg.push_str(&format!("; unknown field(s): {}", unknown.join(", ")));
            }
            if !missing.is_empty() {
                msg.push_str(&format!("; missing required field(s): {}", missing.join(", ")));
            }
            msg.push_str(&format!("; declared fields: {}", declared_fields(properties)));
            return Err(msg);
        }
        // A declared timeout still comes from the server config, never from the
        // caller: one place decides how long an external call may take.
        let mut args = explicit.clone();
        if properties.contains_key("timeout_secs") && !args.contains_key("timeout_secs") {
            args.insert("timeout_secs".to_string(), serde_json::json!(timeout_secs));
        }
        return Ok(args);
    }

    // Intent-only call: fill a declared string field only when the intent names
    // it. No shotgun, no invented fields.
    let intent_lower = intent.to_lowercase();
    let mut derived = serde_json::Map::new();
    for (name, prop) in properties {
        if name == "timeout_secs" {
            continue;
        }
        let kind = prop.get("type").and_then(|v| v.as_str()).unwrap_or("string");
        if kind != "string" {
            continue;
        }
        let name_lower = name.to_lowercase();
        let name_spaced = name_lower.replace('_', " ");
        if intent_lower.contains(&name_lower) || intent_lower.contains(&name_spaced) {
            derived.insert(name.clone(), serde_json::Value::String(intent.to_string()));
        }
    }

    if properties.contains_key("timeout_secs") && !derived.contains_key("timeout_secs") {
        derived.insert("timeout_secs".to_string(), serde_json::json!(timeout_secs));
    }

    let missing: Vec<&str> = required
        .iter()
        .filter(|k| !derived.contains_key(*k))
        .map(String::as_str)
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "cannot fill the required field(s) {} from the intent: call it again with args={{...}}; declared fields: {}",
            missing.join(", "),
            declared_fields(properties)
        ));
    }

    Ok(derived)
}

fn declared_fields(properties: &serde_json::Map<String, serde_json::Value>) -> String {
    let mut names: Vec<&str> = properties.keys().map(String::as_str).collect();
    names.sort();
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join(", ")
    }
}

/// D-93: the timeout comes from the server's configured `timeout_ms` (stored as
/// the guild's tool timeout) and never drops below the historical 30 s.
pub(super) fn external_timeout_secs(tool_timeout: Option<std::time::Duration>) -> u64 {
    tool_timeout
        .map(|d| d.as_secs().max(1))
        .unwrap_or(30)
        .max(30)
}

async fn do_external_call(
    server: &TylluanServer,
    server_name: &str,
    tool_name: &str,
    intent: &str,
    agent_id: Option<&str>,
    explicit_args: Option<&serde_json::Map<String, serde_json::Value>>,
) -> CallToolResult {
    // D-93: the schema was already fetched by `tools/list`; use it instead of
    // guessing field names.
    let (schema, tool_timeout) = {
        let reg = server.registry.read().await;
        match reg.guilds.get(server_name) {
            Some(guild) => (
                guild
                    .tools
                    .iter()
                    .find(|t| t.name.as_ref() == tool_name)
                    .map(|t| serde_json::Value::Object(t.input_schema.as_ref().clone()))
                    .unwrap_or_else(|| serde_json::json!({})),
                guild.tool_timeout,
            ),
            None => {
                return error_result(&format!(
                    "External MCP server '{server_name}' was removed before the call completed."
                ));
            }
        }
    };

    let tool_args = match build_external_tool_args(
        &schema,
        intent,
        explicit_args,
        external_timeout_secs(tool_timeout),
    ) {
        Ok(args) => serde_json::Value::Object(args),
        Err(msg) => return error_result(&format!("{server_name}/{tool_name}: {msg}")),
    };

    let call_params = CallToolRequestParam {
        name: tool_name.to_string().into(),
        arguments: tool_args.as_object().cloned(),
    };

    let result = {
        let reg = server.registry.read().await;
        match reg.guilds.get(server_name) {
            Some(guild) => {
                info!("external_mcp: calling '{}' tool='{}'", server_name, tool_name);
                guild.call_tool_readonly(call_params).await
            }
            None => return error_result(&format!(
                "External MCP server '{server_name}' was removed before the call completed."
            )),
        }
    };

    // ─── M32-P1: Audit trail for external MCP calls ────────────────
    let is_error = result.is_error.unwrap_or(false);
    let result_text = result.content.iter()
        .filter_map(|c| c.as_text())
        .map(|t| t.text.clone())
        .next().unwrap_or_default();
    let guild_label = format!("external_mcp:{server_name}");

    // Activity trace in SilvaDB (agent → external server edge)
    if let Some(aid) = agent_id {
        record_activity_trace(server, aid, &guild_label, tool_name, result_text.len());
    }

    // Audit log entry (fire-and-forget via spawn_blocking)
    let audit_intent = intent.to_string();
    let audit_guild = guild_label.clone();
    let audit_tool = tool_name.to_string();
    let audit_agent = agent_id.unwrap_or("anonymous").to_string();
    let audit_success = !is_error;
    let audit_preview = result_text.chars().take(200).collect::<String>();
    tokio::task::spawn_blocking(move || {
        audit_or_warn(&audit_intent, &audit_guild, &audit_tool, &audit_agent, audit_success, &audit_preview, 0, false);
    });

    // Notify dashboard about external routing
    server.notify("external_mcp_call", serde_json::json!({
        "status": if is_error { "error" } else { "ok" },
        "server": server_name,
        "tool": tool_name,
        "agent_id": agent_id.unwrap_or("anonymous"),
        "intent": dashboard_intent_preview(intent),
        "ts": chrono::Utc::now().timestamp_millis()
    }));

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_intent_preview_truncates_to_200_chars() {
        let long = "a".repeat(500);
        let preview = dashboard_intent_preview(&long);
        assert_eq!(preview.chars().count(), DASHBOARD_INTENT_MAX_CHARS);
        assert_eq!(preview.len(), DASHBOARD_INTENT_MAX_CHARS);
    }

    #[test]
    fn dashboard_intent_preview_keeps_short_intent() {
        assert_eq!(
            dashboard_intent_preview("search the web"),
            "search the web"
        );
    }

    #[test]
    fn dashboard_intent_preview_counts_chars_not_bytes() {
        let long = "é".repeat(250);
        let preview = dashboard_intent_preview(&long);
        assert_eq!(preview.chars().count(), DASHBOARD_INTENT_MAX_CHARS);
        assert!(
            preview.len() > DASHBOARD_INTENT_MAX_CHARS,
            "no debe cortar por bytes ni romper UTF-8"
        );
    }

    #[test]
    fn test_tokenize_intent_for_external_mcp() {
        let tokens = tokenize("search the web for rust lang");
        assert!(tokens.contains(&"search".to_string()));
        assert!(tokens.contains(&"web".to_string()));
        assert!(tokens.contains(&"rust".to_string()));
        assert!(tokens.contains(&"lang".to_string()));
    }

    #[test]
    fn test_keyword_score_reuses_existing_logic() {
        let tokens = tokenize("search the web");
        let score = keyword_score(&tokens, "Search the web for information", "web_search");
        assert!(score > 0.4, "score should be > 0.4, got {score}");
    }

    #[test]
    fn test_keyword_score_low_for_unrelated() {
        let tokens = tokenize("compile rust project");
        let score = keyword_score(&tokens, "Search the web for information", "web_search");
        assert!(score < 0.3, "score should be < 0.3, got {score}");
    }
}

#[cfg(test)]
mod d93_tests {
    use super::*;

    fn schema() -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "query": {"type": "string"}
            },
            "required": ["path"]
        })
    }

    fn args(pairs: &[(&str, &str)]) -> serde_json::Map<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), serde_json::json!(v)))
            .collect()
    }

    #[test]
    fn d93_explicit_args_are_validated_against_the_schema() {
        let ok = build_external_tool_args(&schema(), "whatever", Some(&args(&[("path", "/tmp")])), 30)
            .expect("a declared required field must pass");
        assert_eq!(ok.get("path").unwrap(), "/tmp");

        let err = build_external_tool_args(&schema(), "whatever", Some(&args(&[("q", "x")])), 30)
            .unwrap_err();
        assert!(err.contains("unknown field(s): q"), "{err}");
        assert!(err.contains("missing required field(s): path"), "{err}");
        assert!(err.contains("declared fields: path, query"), "{err}");
    }

    /// The old `do_external_call` map (command/intent/query/text/content/prompt/
    /// message/input) must be rejected now, not silently forwarded.
    #[test]
    fn d93_legacy_shotgun_args_are_rejected() {
        let legacy = args(&[
            ("command", "x"),
            ("intent", "x"),
            ("query", "x"),
            ("text", "x"),
            ("content", "x"),
            ("prompt", "x"),
            ("message", "x"),
            ("input", "x"),
        ]);
        let err = build_external_tool_args(&schema(), "x", Some(&legacy), 30).unwrap_err();
        assert!(err.contains("unknown field(s):"), "{err}");
    }

    /// Intent-only: only declared fields the intent names are filled, never the
    /// eight-field shotgun.
    #[test]
    fn d93_intent_only_fills_declared_fields() {
        let out = build_external_tool_args(&schema(), "read the path now", None, 30)
            .expect("path is declared and named in the intent");
        assert_eq!(out.len(), 1, "no invented fields: {out:?}");
        assert_eq!(out.get("path").unwrap(), "read the path now");
    }

    #[test]
    fn d93_underivable_required_field_asks_for_args() {
        let err = build_external_tool_args(&schema(), "hello there", None, 30).unwrap_err();
        assert!(err.contains("cannot fill the required field(s) path"), "{err}");
        assert!(err.contains("args={...}"), "{err}");
    }

    #[test]
    fn d93_schema_less_tool_requires_explicit_args() {
        let empty = serde_json::json!({});
        assert!(build_external_tool_args(&empty, "do it", None, 30).is_err());
        let ok = build_external_tool_args(&empty, "do it", Some(&args(&[("anything", "1")])), 30)
            .expect("undeclared schema passes args through");
        assert_eq!(ok.get("anything").unwrap(), "1");
    }

    /// D-93: with explicit args, a declared `timeout_secs` is still filled from
    /// the server config instead of being demanded from the caller.
    #[test]
    fn d93_explicit_args_get_the_configured_timeout() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {"path": {"type": "string"}, "timeout_secs": {"type": "integer"}},
            "required": ["path", "timeout_secs"]
        });
        let out = build_external_tool_args(&schema, "x", Some(&args(&[("path", "/tmp")])), 45)
            .expect("the configured timeout fills the declared field");
        assert_eq!(out.get("timeout_secs").unwrap(), 45);
        assert_eq!(out.get("path").unwrap(), "/tmp");
    }

    /// D-93: a malformed `args` value is an error, not a silent fall back to the
    /// intent-only path.
    #[test]
    fn d93_malformed_args_are_reported() {
        let mut arguments = serde_json::Map::new();
        arguments.insert("args".into(), serde_json::json!("path=/tmp"));
        let msg = explicit_args_error(&Some(arguments)).expect("a string is not a valid args");
        assert!(msg.contains("JSON object"), "{msg}");

        let mut ok = serde_json::Map::new();
        ok.insert("args".into(), serde_json::json!({"path": "/tmp"}));
        assert!(explicit_args_error(&Some(ok)).is_none());
        assert!(explicit_args_error(&None).is_none());
    }

    #[test]
    fn d93_timeout_comes_from_config_and_never_drops_below_30() {
        assert_eq!(external_timeout_secs(None), 30);
        assert_eq!(
            external_timeout_secs(Some(std::time::Duration::from_millis(5_000))),
            30
        );
        assert_eq!(
            external_timeout_secs(Some(std::time::Duration::from_millis(45_000))),
            45
        );

        let with_timeout = serde_json::json!({
            "type": "object",
            "properties": {"path": {"type": "string"}, "timeout_secs": {"type": "integer"}},
            "required": ["timeout_secs"]
        });
        let out = build_external_tool_args(
            &with_timeout,
            "read the path",
            None,
            external_timeout_secs(Some(std::time::Duration::from_millis(45_000))),
        )
        .expect("timeout_secs is declared, so it can be filled");
        assert_eq!(out.get("timeout_secs").unwrap(), 45);
    }
}
