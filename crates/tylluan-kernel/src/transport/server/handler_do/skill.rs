use crate::registry::proxy::error_result;
use crate::transport::server::TylluanServer;
use rmcp::{Error as McpError, model::*};

/// M31-P5: @skill: prefix — project-scoped reusable skill context in SilvaDB.
/// Bypasses the semantic router entirely.
///
/// Syntax:
///   @skill:save:<name>: <content>   — save a skill with the given name
///   @skill:get:<name>               — retrieve a skill by name
///   @skill:list                     — list all skill names for this project
///   @skill:delete:<name>            — delete a skill by name
pub(crate) async fn handle_skill_prefix(
    server: &TylluanServer,
    intent: &str,
    _agent_id: &Option<String>,
) -> Option<Result<CallToolResult, McpError>> {
    let trimmed = intent.trim();
    if !trimmed.starts_with("@skill") {
        return None;
    }

    let workspace_root = std::env::current_dir().unwrap_or_default();
    let workspace_hash: u64 = workspace_root.to_string_lossy().as_bytes()
        .iter()
        .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(*b as u64));

    let after_prefix = trimmed.strip_prefix("@skill").unwrap_or("").trim();

    // @skill:list (also bare @skill)
    if after_prefix.is_empty() || after_prefix == ":list" || after_prefix == "list" {
        let results = server.silva.get_nodes_by_types(&["project_skill"], 100).await.unwrap_or_default();
        let escaped_root = workspace_root.to_string_lossy().replace('\\', "\\\\");
        let project_skills: Vec<String> = results.iter()
            .filter(|n| {
                n.metadata.contains(&format!("\"project_root\":\"{escaped_root}\""))
            })
            .filter_map(|n| {
                let name = n.id.rsplit(':').next()?;
                Some(format!("  - {name}"))
            })
            .collect();
        if project_skills.is_empty() {
            return Some(Ok(CallToolResult {
                content: vec![Content::text("No skills saved for this project. Use @skill:save:<name>: <content> to create one.")],
                is_error: Some(false),
            }));
        }
        return Some(Ok(CallToolResult {
            content: vec![Content::text(format!("Project skills:\n{}", project_skills.join("\n")))],
            is_error: Some(false),
        }));
    }

    // @skill:delete:<name>
    if after_prefix.starts_with(":delete:") || after_prefix.starts_with("delete:") {
        let name = after_prefix.split_once(':')
            .and_then(|(_, rest)| if rest.starts_with("delete:") { rest.strip_prefix("delete:") } else { None })
            .or_else(|| after_prefix.strip_prefix(":delete:"))
            .unwrap_or("")
            .trim();
        if name.is_empty() {
            return Some(Ok(error_result("@skill:delete:<name> requires a non-empty skill name.")));
        }
        let skill_id = format!("skill:project:{workspace_hash:x}:{name}");
        match server.silva.delete_node(&skill_id).await {
            Ok(true) => return Some(Ok(CallToolResult {
                content: vec![Content::text(format!("Skill '{name}' deleted."))],
                is_error: Some(false),
            })),
            Ok(false) => return Some(Ok(error_result(&format!(
                "Skill '{name}' not found or is protected."
            )))),
            Err(e) => return Some(Ok(error_result(&format!("delete failed: {e}")))),
        }
    }

    // @skill:get:<name>
    if after_prefix.starts_with(":get:") || after_prefix.starts_with("get:") {
        let name = after_prefix.split_once(':')
            .and_then(|(_, rest)| if rest.starts_with("get:") { rest.strip_prefix("get:") } else { None })
            .or_else(|| after_prefix.strip_prefix(":get:"))
            .unwrap_or("")
            .trim();
        if name.is_empty() {
            return Some(Ok(error_result("@skill:get:<name> requires a non-empty skill name.")));
        }
        let skill_id = format!("skill:project:{workspace_hash:x}:{name}");
        match server.silva.get_node(&skill_id).await {
            Ok(Some(node)) => return Some(Ok(CallToolResult {
                content: vec![Content::text(format!(
                    "Skill '{name}':\n{}",
                    node.content
                ))],
                is_error: Some(false),
            })),
            Ok(None) => return Some(Ok(error_result(&format!(
                "Skill '{name}' not found in this project."
            )))),
            Err(e) => return Some(Ok(error_result(&format!("read failed: {e}")))),
        }
    }

    // @skill:save:<name>: <content>
    if after_prefix.starts_with(":save:") || after_prefix.starts_with("save:") {
        let remainder = after_prefix.split_once(':')
            .and_then(|(_, rest)| if rest.starts_with("save:") { rest.strip_prefix("save:") } else { None })
            .or_else(|| after_prefix.strip_prefix(":save:"))
            .unwrap_or("")
            .trim();
        let (name, content) = match remainder.split_once(':') {
            Some((n, c)) => (n.trim(), c.trim()),
            None => return Some(Ok(error_result(
                "@skill:save:<name>: <content> requires a name and content separated by ':'."
            ))),
        };
        if name.is_empty() || content.is_empty() {
            return Some(Ok(error_result(
                "@skill:save:<name>: <content> requires a non-empty name and content."
            )));
        }
        let skill_id = format!("skill:project:{workspace_hash:x}:{name}");
        let meta = serde_json::json!({
            "project_root": workspace_root.to_string_lossy(),
            "name": name,
        }).to_string();

        match server.silva.upsert_node(&skill_id, "project_skill", content, &meta).await {
            Ok(()) => return Some(Ok(CallToolResult {
                content: vec![Content::text(format!("Skill '{name}' saved."))],
                is_error: Some(false),
            })),
            Err(e) => return Some(Ok(error_result(&format!("save failed: {e}")))),
        }
    }

    // Fallback: unknown @skill subcommand
    Some(Ok(error_result(&format!(
        "Unknown @skill command: '{after_prefix}'. Available: :save:<name>: <content>, :get:<name>, :list, :delete:<name>"
    ))))
}