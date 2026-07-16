use rmcp::model::*;
use tracing::info;
use crate::registry::proxy::error_result;
use crate::config::TylluanConfig;
use super::TylluanServer;
use crate::router::matcher::{tokenize, keyword_score};
use super::routing::record_activity_trace;
use super::log_audit_entry;

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
            return dispatch_to_external(server, name, &tokens, intent, agent_id).await;
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
        return Some(do_external_call(server, &server_name, &tool_name, intent, agent_id).await);
    }

    None
}

async fn dispatch_to_external(
    server: &TylluanServer,
    server_name: &str,
    tokens: &[String],
    intent: &str,
    agent_id: Option<&str>,
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
        Some(do_external_call(server, server_name, &tool_name, intent, agent_id).await)
    } else {
        Some(error_result(&format!(
            "External MCP server '{server_name}' is registered in config but not in the guild registry."
        )))
    }
}

async fn do_external_call(
    server: &TylluanServer,
    server_name: &str,
    tool_name: &str,
    intent: &str,
    agent_id: Option<&str>,
) -> CallToolResult {
    let tool_args = serde_json::json!({
        "command": intent, "intent": intent,
        "query": intent, "text": intent, "content": intent,
        "prompt": intent, "message": intent, "input": intent,
        "timeout_secs": 30,
    });

    let call_params = CallToolRequestParam {
        name: tool_name.to_string().into(),
        arguments: Some(tool_args.as_object().cloned().unwrap_or_default()),
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
        let _ = log_audit_entry(&audit_intent, &audit_guild, &audit_tool, &audit_agent, audit_success, &audit_preview);
    });

    // Notify dashboard about external routing
    server.notify("external_mcp_call", serde_json::json!({
        "status": if is_error { "error" } else { "ok" },
        "server": server_name,
        "tool": tool_name,
        "agent_id": agent_id.unwrap_or("anonymous"),
        "intent": intent,
        "ts": chrono::Utc::now().timestamp_millis()
    }));

    result
}

#[cfg(test)]
mod tests {
    use super::*;

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
