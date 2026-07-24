use crate::registry::proxy::error_result;
use crate::transport::server::handler_recall;
use crate::transport::server::handler_remember;
use crate::transport::server::TylluanServer;
use rmcp::{Error as McpError, model::*};

/// Deterministic `@coloquio` prefix dispatch — bypasses the semantic router entirely.
/// Returns `None` if `intent` doesn't start with `@coloquio` (caller should fall through).
pub(crate) async fn handle_coloquio_prefix(
    server: &TylluanServer,
    intent: &str,
    agent_id: &Option<String>,
) -> Option<Result<CallToolResult, McpError>> {
    if !intent.trim().starts_with("@coloquio") {
        return None;
    }
    let rest = intent.trim().strip_prefix("@coloquio").unwrap_or("").trim();
    if rest.is_empty() || rest == ":list" {
        // list channels via recall
        let mut args = serde_json::Map::new();
        args.insert("query".to_string(), serde_json::Value::String("@coloquio".to_string()));
        args.insert("limit".to_string(), serde_json::Value::Number(serde_json::Number::from(20)));
        return Some(Box::pin(handler_recall::handle_tylluan_recall(server, Some(args))).await);
    }
    if let Some(create_name) = rest.strip_prefix(":create:") {
        let channel_name = create_name.trim().to_string();
        if channel_name.is_empty() {
            return Some(Ok(error_result("Usage: @coloquio:create:<channel_name>")));
        }
        return Some(if let Some(ref coloquio) = server.coloquio {
            match coloquio.create_channel(&channel_name, &channel_name).await {
                Ok(ch) => Ok(CallToolResult {
                    content: vec![Content::text(format!("Channel #{} created.", ch.channel_id))],
                    is_error: Some(false),
                }),
                Err(e) => Ok(error_result(&format!("Failed to create channel: {e}"))),
            }
        } else {
            Ok(error_result("Coloquio is not available."))
        });
    }
    if let Some(channel_part) = rest.strip_prefix(':') {
        let (channel_id, message) = if let Some(idx) = channel_part.find(':') {
            let (cid, msg) = channel_part.split_at(idx);
            (cid.trim().to_string(), Some(msg[1..].trim().to_string()))
        } else {
            (channel_part.trim().to_string(), None)
        };
        if channel_id.is_empty() {
            return Some(Ok(error_result("Usage: @coloquio:<channel_id> (read) or @coloquio:<channel_id>:<message> (post)")));
        }
        if let Some(msg) = message {
            if msg.is_empty() {
                return Some(Ok(error_result("Message cannot be empty. Usage: @coloquio:<channel_id>:<message>")));
            }
            // Post to channel via remember
            let mut args = serde_json::Map::new();
            args.insert("content".to_string(), serde_json::Value::String(
                format!("@coloquio:{channel_id}:{msg}")
            ));
            if let Some(aid) = agent_id {
                args.insert("agent_id".to_string(), serde_json::Value::String(aid.clone()));
            }
            return Some(Box::pin(handler_remember::handle_tylluan_remember(server, Some(args))).await);
        } else {
            // Read channel via recall
            let mut args = serde_json::Map::new();
            args.insert("query".to_string(), serde_json::Value::String(
                format!("@coloquio:{channel_id}")
            ));
            return Some(Box::pin(handler_recall::handle_tylluan_recall(server, Some(args))).await);
        }
    }
    None
}