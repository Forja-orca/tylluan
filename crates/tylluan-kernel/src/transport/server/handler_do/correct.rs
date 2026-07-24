use crate::registry::proxy::error_result;
use crate::transport::server::TylluanServer;
use rmcp::{Error as McpError, model::*};

/// M36: @correct:<node_id>:<content> — explicit self-correction of a memory node.
/// Supersedes the old node via valid_until (M35 pattern), creates a new corrected
/// node with provenance=agent_generated, and links via "corrects" edge.
pub(crate) async fn handle_correct_prefix(
    server: &TylluanServer,
    intent: &str,
) -> Option<Result<CallToolResult, McpError>> {
    let trimmed = intent.trim();
    if !trimmed.starts_with("@correct") {
        return None;
    }

    // Parse @correct:<node_id>:<content> — node_id may contain colons,
    // so we use rsplit_once(':') to find the last colon as content boundary.
    let after = trimmed.strip_prefix("@correct").unwrap_or("").trim();
    let after = match after.strip_prefix(':') {
        Some(s) => s,
        None => return Some(Ok(error_result(
            "Usage: @correct:<node_id>:<contenido corregido>"
        ))),
    };
    let (node_id, new_content) = match after.rsplit_once(':') {
        Some((id, rest)) if !id.is_empty() && !rest.trim().is_empty() => (id.trim(), rest.trim()),
        _ => return Some(Ok(error_result(
            "Usage: @correct:<node_id>:<contenido corregido>"
        ))),
    };

    let silva = &server.silva;

    let node = match silva.get_node(node_id).await {
        Ok(Some(n)) => n,
        Ok(None) => return Some(Ok(error_result(&format!("Node '{node_id}' not found.")))),
        Err(e) => return Some(Ok(error_result(&format!("Error reading node '{node_id}': {e}")))),
    };

    if node.protected {
        return Some(Ok(error_result(
            &format!("Cannot correct protected node '{node_id}': protected nodes are intentionally immutable.")
        )));
    }

    if node.node_type == "identity" {
        return Some(Ok(error_result(
            &format!("Cannot correct identity node '{node_id}': identity nodes are intentionally immutable.")
        )));
    }

    if node.valid_until.is_some() {
        return Some(Ok(error_result(&format!(
            "Node '{node_id}' already has a superseding correction (valid_until is set). Only the current version can be corrected."
        ))));
    }

    let now_timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    // Supersede old node via spawn_blocking to avoid blocking the async runtime
    let silva_clone = silva.clone();
    let nid = node_id.to_string();
    if let Err(e) = tokio::task::spawn_blocking(move || {
        let conn = silva_clone.conn.blocking_lock();
        if let Err(e) = conn.execute(
            "UPDATE nodes SET valid_until = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2 AND valid_until IS NULL",
            rusqlite::params![now_timestamp, nid],
        ) {
            tracing::warn!("@correct: failed to set valid_until on '{}': {}", nid, e);
        }
    }).await {
        tracing::warn!("@correct: spawn_blocking panicked: {}", e);
    }

    let new_id = format!("{node_id}:corrected:{now_timestamp}");

    let mut meta: serde_json::Value = serde_json::from_str(&node.metadata).unwrap_or(serde_json::Value::Object(Default::default()));
    if let (Some(tk), Some(obj)) = (node.topic_key.as_ref(), meta.as_object_mut()) {
        obj.insert("topic".to_string(), serde_json::Value::String(tk.clone()));
    }

    if let Err(e) = silva.upsert_node_with_provenance(
        &new_id, &node.node_type, new_content, &meta.to_string(), "agent_generated",
    ).await {
        return Some(Ok(error_result(&format!("Failed to create corrected node: {e}"))));
    }

    let _ = silva.set_weight(&new_id, node.weight).await;

    if let Err(e) = silva.add_edge(&new_id, node_id, "corrects", 1.0, "").await {
        tracing::warn!("@correct: failed to add edge {} -> {}: {}", new_id, node_id, e);
    }

    Some(Ok(CallToolResult {
        content: vec![Content::text(format!(
            "Corrected node '{node_id}'. New version created as '{new_id}'. Old version superseded (valid_until set)."
        ))],
        is_error: Some(false),
    }))
}