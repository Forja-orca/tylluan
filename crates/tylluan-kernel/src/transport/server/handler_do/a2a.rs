use rmcp::{Error as McpError, model::*};
use std::time::Duration;

use crate::registry::proxy::error_result;
use crate::transport::http::a2a_client::{A2aAgentStore, A2aClient};
use super::TylluanServer;

/// Parse `@agent <name>: <message>` -> (name, message).
pub(crate) fn parse_agent_intent(intent: &str) -> Option<(String, String)> {
    let rest = intent.trim().strip_prefix("@agent")?.trim_start();
    if !rest.starts_with(char::is_alphanumeric) {
        return None;
    }
    let (name, message) = rest.split_once(':')?;
    let name = name.trim();
    let message = message.trim();
    if name.is_empty() || message.is_empty() {
        return None;
    }
    Some((name.to_string(), message.to_string()))
}

/// Deterministic `@agent <name>: <message>` prefix — dispatches a message to a
/// configured external A2A agent, bypassing the semantic router (same pattern
/// as `@coloquio`/nodo prefixes). F2.
///
/// ACL: mirrors the guild ACL gate — when `[security.acl]` defines roles, an
/// explicit role must be resolvable (fail-closed) and it must be `admin`, `*`,
/// or list the virtual guild `"a2a"` to reach external agents.
pub(crate) async fn handle_a2a_agent_prefix(
    server: &TylluanServer,
    intent: &str,
    agent_id: &Option<String>,
) -> Option<Result<CallToolResult, McpError>> {
    let trimmed = intent.trim();
    if !trimmed.starts_with("@agent") {
        return None;
    }
    let Some((name, message)) = parse_agent_intent(trimmed) else {
        return Some(Ok(error_result(
            "Syntax: `@agent <name>: <message>` — e.g. `@agent sdk-echo: ping`.\n\
             Configure agents with POST /api/v1/a2a/agents.",
        )));
    };
    // Guard against dialog runaway: a tiny accept-loop is enough, but require
    // a real dispatch — one agent, fail fast.
    if message.len() > 4000 {
        return Some(Ok(error_result("Message too long (max 4000 chars).")));
    }

    // ACL gate (same semantics as guild dispatch, virtual guild name "a2a").
    if let Ok(config_lock) = crate::config::TylluanConfig::load_cached() {
        let cfg = config_lock.read().await;
        let acl = &cfg.security.acl;
        if !acl.roles.is_empty() || !acl.tokens.is_empty() {
            let Some(role) = crate::transport::http::auth::current_acl_role() else {
                return Some(Err(McpError::internal_error(
                    "ACCESS_DENIED: ACL context unavailable for this invocation",
                    None,
                )));
            };
            if !crate::transport::http::auth::acl_can_access(&role, "a2a", acl) {
                let msg = format!(
                    "ACCESS_DENIED: role '{role}' does not have access to 'a2a' external agents. \
                     Contact your administrator to update [security.acl] in tylluan.toml."
                );
                return Some(Ok(error_result(&msg)));
            }
        }
    }

    // Resolve agent: match by id first, then case-insensitive name.
    let store = A2aAgentStore::new(server.silva.clone());
    let agents = match store.load_all().await {
        Ok(a) => a,
        Err(e) => return Some(Ok(error_result(&format!("A2A store error: {e}")))),
    };
    let lower = name.to_lowercase();
    let agent = agents.iter().find(|a| a.id == name || a.name.to_lowercase() == lower);
    if agent.is_none() {
        let names: Vec<&str> = agents.iter().map(|a| a.name.as_str()).collect();
        let who = agent_id.clone().unwrap_or_else(|| "unknown".into());
        return Some(Ok(error_result(&format!(
            "External agent '{name}' not found (caller: {who}). Configured: {}",
            if names.is_empty() { "(none)".to_string() } else { names.join(", ") }
        ))));
    }
    let agent = agent.unwrap().clone();
    if !agent.enabled {
        return Some(Ok(error_result(&format!("External agent '{}' is disabled.", agent.name))));
    }

    let client = match A2aClient::new() {
        Ok(c) => c,
        Err(e) => return Some(Ok(error_result(&format!("A2A client init failed: {e}")))),
    };

    let started = std::time::Instant::now();
    let run = async {
        let card = client.fetch_card(&agent).await?;
        let endpoint = A2aClient::resolve_endpoint(&card, &agent.url)
            .map_err(|m| anyhow::anyhow!(m))?;
        client.message_send(&agent, &endpoint, &message).await
    };
    match tokio::time::timeout(Duration::from_secs(30), run).await {
        Ok(Ok(task)) => {
            let text = A2aClient::task_text(&task);
            let report = format!(
                "External agent '{}' replied (state: {}):\n{}",
                agent.name,
                task.resolved_state().as_str(),
                text
            );
            Some(Ok(CallToolResult {
                content: vec![Content::text(report)],
                is_error: Some(false),
            }))
        }
        Ok(Err(e)) => Some(Ok(error_result(&format!(
            "External agent '{}' failed after {}ms: {e}",
            agent.name,
            started.elapsed().as_millis()
        )))),
        Err(_) => Some(Ok(error_result(&format!(
            "External agent '{}' timed out after 30s.",
            agent.name
        )))),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_agent_intent;

    #[test]
    fn parses_valid_agent_intent() {
        let (name, msg) = parse_agent_intent("@agent sdk-echo: ping").unwrap();
        assert_eq!(name, "sdk-echo");
        assert_eq!(msg, "ping");
    }

    #[test]
    fn parses_with_extra_whitespace_and_colons() {
        let (name, msg) = parse_agent_intent("  @agent  my-agent :  hello: world  ").unwrap();
        assert_eq!(name, "my-agent");
        assert_eq!(msg, "hello: world");
    }

    #[test]
    fn rejects_malformed_intents() {
        assert!(parse_agent_intent("@agent: no name").is_none());
        assert!(parse_agent_intent("@agent name: ").is_none());
        assert!(parse_agent_intent("@agent name").is_none());
        assert!(parse_agent_intent("@agent  :x").is_none());
        assert!(parse_agent_intent("@coloquio general: hi").is_none());
        assert!(parse_agent_intent("random text").is_none());
        assert!(parse_agent_intent("friendlier: for no agent").is_none());
    }
}