use crate::registry::proxy::error_result;
use crate::transport::server::TylluanServer;
use rmcp::{Error as McpError, model::*};

/// Sovereign shortcut: "forget: {node_id}" / "delete node: {node_id}" — deletes a
/// node directly without routing to a guild. Returns `None` if `intent` doesn't match.
pub(crate) async fn handle_forget_shortcut(
    server: &TylluanServer,
    intent: &str,
) -> Option<Result<CallToolResult, McpError>> {
    let intent_lower = intent.trim().to_lowercase();
    if !(intent_lower.starts_with("forget:") || intent_lower.starts_with("delete node:")) {
        return None;
    }
    let node_id = intent.split_once(':').map(|x| x.1).unwrap_or("").trim().to_string();
    if node_id.is_empty() {
        return Some(Ok(error_result("forget: requires a node_id. Usage: forget: {node_id}")));
    }
    Some(match server.silva.delete_node(&node_id).await {
        Ok(true) => Ok(CallToolResult {
            content: vec![Content::text(format!("Forgotten: node '{node_id}' deleted."))],
            is_error: Some(false),
        }),
        Ok(false) => Ok(error_result(&format!(
            "Cannot forget '{node_id}': node not found or is protected."
        ))),
        Err(e) => Ok(error_result(&format!("forget failed: {e}"))),
    })
}