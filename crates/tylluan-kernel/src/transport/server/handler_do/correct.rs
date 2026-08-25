use crate::registry::proxy::error_result;
use crate::transport::server::TylluanServer;
use rmcp::{Error as McpError, model::*};

/// Disambiguate `<node_id>:<content>` when both sides may contain colons.
/// The node_id is the longest colon-delimited prefix that names an existing
/// node; everything after it is the content. This fixes the real bug where
/// content with a literal ':' (e.g. an `arXiv:XXXX` citation) was parsed as
/// part of the node id under the old `rsplit_once(':')` last-colon rule.
///
/// Falls back to the legacy last-colon split when no prefix matches, so a
/// nonexistent node id still surfaces as a clear "Node not found" upstream
/// instead of a parse error.
async fn split_node_id_and_content(
    silva: &crate::memory::silva::SilvaDB,
    after: &str,
) -> Option<(String, String)> {
    // Collect byte offsets of every ':' (ascending).
    let mut colons: Vec<usize> = Vec::new();
    let mut from = 0usize;
    while let Some(pos) = after[from..].find(':') {
        colons.push(from + pos);
        from += pos + 1;
    }

    // Longest boundary first: `concept:x:corrected:123` must win over its
    // parent `concept:x` when both exist (correcting a corrected node).
    for &i in colons.iter().rev() {
        let candidate_id = after[..i].trim();
        let content = after[i + 1..].trim();
        if candidate_id.is_empty() || content.is_empty() {
            continue;
        }
        if silva.get_node(candidate_id).await.map(|n| n.is_some()).unwrap_or(false) {
            return Some((candidate_id.to_string(), content.to_string()));
        }
    }

    // No colon boundary named an existing node — legacy last-colon split.
    match after.rsplit_once(':') {
        Some((id, rest)) if !id.is_empty() && !rest.trim().is_empty() =>
            Some((id.trim().to_string(), rest.trim().to_string())),
        _ => None,
    }
}

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

    // Parse @correct:<node_id>:<content> — both sides may contain colons
    // (node ids like `concept:merged:*`, content like `arXiv:2606.24322`),
    // so the boundary is ambiguous. Disambiguate against the DB: the
    // node_id is the longest colon-delimited prefix naming an existing node.
    let after = trimmed.strip_prefix("@correct").unwrap_or("").trim();
    let after = match after.strip_prefix(':') {
        Some(s) => s,
        None => return Some(Ok(error_result(
            "Usage: @correct:<node_id>:<contenido corregido>"
        ))),
    };

    let silva = &server.silva;

    let (node_id, new_content) = match split_node_id_and_content(silva, after).await {
        Some(parsed) => parsed,
        None => return Some(Ok(error_result(
            "Usage: @correct:<node_id>:<contenido corregido>"
        ))),
    };

    let node = match silva.get_node(&node_id).await {
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

    // Supersede old node via spawn_blocking to avoid blocking the async runtime.
    // node_id is already owned (String) — the clone below feeds the closure
    // while the original is still used to build the corrected node id.
    let silva_clone = silva.clone();
    let nid = node_id.clone();
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
        &new_id, &node.node_type, &new_content, &meta.to_string(), "agent_generated",
    ).await {
        return Some(Ok(error_result(&format!("Failed to create corrected node: {e}"))));
    }

    let _ = silva.set_weight(&new_id, node.weight).await;

    if let Err(e) = silva.add_edge(&new_id, &node_id, "corrects", 1.0, "").await {
        tracing::warn!("@correct: failed to add edge {} -> {}: {}", new_id, node_id, e);
    }

    Some(Ok(CallToolResult {
        content: vec![Content::text(format!(
            "Corrected node '{node_id}'. New version created as '{new_id}'. Old version superseded (valid_until set)."
        ))],
        is_error: Some(false),
    }))
}