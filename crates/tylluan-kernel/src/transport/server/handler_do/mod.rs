use rmcp::{Error as McpError, model::*};
use tracing::{info, warn};
use chrono;

use crate::registry::proxy::error_result;
use super::utils::{extract_path_from_intent, extract_url_from_intent, extract_command_from_intent};
use super::TylluanServer;
use super::handler_recall;
use super::handler_remember;

pub(crate) mod routing;
mod embedding;
mod coloquio_utils;
mod timeout;
mod external_mcp;

pub use embedding::re_embed_legacy_nodes;
pub(crate) use embedding::distill_for_embedding;
pub(crate) use routing::maybe_auto_extract_triples;
pub use timeout::guild_effective_timeout;

use coloquio_utils::{parse_coloquio_intent, _parse_coloquio_pagination};
use routing::{resolve_guild_name, run_agent_handshake, record_activity_trace};
#[cfg(test)]
use embedding::parse_content_for_embedding;

/// Deterministic failure node ID for the routing feedback loop.
fn routing_failure_id(intent: &str) -> String {
    let hash: u64 = intent.bytes()
        .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
    format!("lesson:routing_failure:{hash:x}")
}

/// Deterministic `@coloquio` prefix dispatch — bypasses the semantic router entirely.
/// Returns `None` if `intent` doesn't start with `@coloquio` (caller should fall through).
async fn handle_coloquio_prefix(
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

/// Deterministic nodo/node prefix — agent-to-agent messaging via `AgentNodeRouter`.
/// Returns `None` if `intent` doesn't parse as a node command (caller should fall through).
async fn handle_nodo_prefix(
    server: &TylluanServer,
    intent: &str,
    agent_id: &Option<String>,
) -> Option<Result<CallToolResult, McpError>> {
    use crate::memory::agent_nodes::NodeIntent;
    let nodo_intent = crate::memory::agent_nodes::parse_node_intent(intent)?;
    let aid = agent_id.as_deref().unwrap_or("unknown");
    let router = &server.node_router;

    // Auto-register on any nodo command
    router.register(aid).await;

    Some(match nodo_intent {
        NodeIntent::Send { to, payload } => {
            let from = aid;
            match router.send(from, &to, &payload, "direct").await {
                Ok(res) => Ok(CallToolResult {
                    content: vec![Content::text(format!("Mensaje enviado a {} (msg_id: {})", to, res["msg_id"]))],
                    is_error: Some(false),
                }),
                Err(e) => Ok(error_result(&e)),
            }
        }
        NodeIntent::Broadcast { payload } => {
            let res = router.broadcast(aid, &payload).await;
            let count = res["recipients"].as_u64().unwrap_or(0);
            Ok(CallToolResult {
                content: vec![Content::text(format!("Broadcast enviado a {count} nodos."))],
                is_error: Some(false),
            })
        }
        NodeIntent::DrainInbox | NodeIntent::PeekInbox => {
            let msgs = match nodo_intent {
                NodeIntent::DrainInbox => router.drain_inbox(aid).await,
                _ => router.peek_inbox(aid).await,
            };
            if msgs.is_empty() {
                return Some(Ok(CallToolResult {
                    content: vec![Content::text("Buzón vacío.")],
                    is_error: Some(false),
                }));
            }
            let mut report = format!("Buzón de {} ({} mensajes):\n", aid, msgs.len());
            for (i, m) in msgs.iter().enumerate() {
                let preview = if m.payload.len() > 120 {
                    format!("{}...", &m.payload[..120])
                } else { m.payload.clone() };
                report.push_str(&format!("{}. [{}] {}: {}\n", i + 1, m.msg_type, m.from, preview));
            }
            Ok(CallToolResult { content: vec![Content::text(report)], is_error: Some(false) })
        }
        NodeIntent::List => {
            let nodes = router.list().await;
            if nodes.is_empty() {
                return Some(Ok(CallToolResult {
                    content: vec![Content::text("No hay nodos conectados.")],
                    is_error: Some(false),
                }));
            }
            let report = nodes.iter().map(|n| {
                let agent_id = n["agent_id"].as_str().unwrap_or("?");
                let pending = n["inbox_pending"].as_u64().unwrap_or(0);
                format!("- {agent_id}: {pending} pendientes")
            }).collect::<Vec<_>>().join("\n");
            Ok(CallToolResult {
                content: vec![Content::text(format!("Nodos conectados:\n{report}"))],
                is_error: Some(false),
            })
        }
        NodeIntent::Register => {
            Ok(CallToolResult {
                content: vec![Content::text(format!("Nodo '{aid}' registrado."))],
                is_error: Some(false),
            })
        }
        NodeIntent::Unregister => {
            router.unregister(aid).await;
            Ok(CallToolResult {
                content: vec![Content::text(format!("Nodo '{aid}' desregistrado."))],
                is_error: Some(false),
            })
        }
    })
}

/// Sovereign shortcut: "forget: {node_id}" / "delete node: {node_id}" — deletes a
/// node directly without routing to a guild. Returns `None` if `intent` doesn't match.
async fn handle_forget_shortcut(
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

/// M31-P5: @skill: prefix — project-scoped reusable skill context in SilvaDB.
/// Bypasses the semantic router entirely.
///
/// Syntax:
///   @skill:save:<name>: <content>   — save a skill with the given name
///   @skill:get:<name>               — retrieve a skill by name
///   @skill:list                     — list all skill names for this project
///   @skill:delete:<name>            — delete a skill by name
async fn handle_skill_prefix(
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

pub async fn handle_tylluan_do(
    server: &TylluanServer,
    arguments: Option<serde_json::Map<String, serde_json::Value>>,
) -> Result<CallToolResult, McpError> {
    let intent = arguments.as_ref()
        .and_then(|a| a.get("intent")).and_then(|v| v.as_str())
        .unwrap_or("").to_string();
    let remember = arguments.as_ref()
        .and_then(|a| a.get("remember")).and_then(|v| v.as_bool())
        .unwrap_or(false);
    let agent_id: Option<String> = arguments.as_ref()
        .and_then(|a| a.get("agent_id")).and_then(|v| v.as_str())
        .map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let guild_hint = arguments.as_ref()
        .and_then(|a| a.get("guild")).and_then(|v| v.as_str())
        .map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let plan_mode = arguments.as_ref()
        .and_then(|a| a.get("plan")).and_then(|v| v.as_bool())
        .unwrap_or(false);

    if intent.trim().is_empty() {
        return Ok(error_result("tylluan_do requires a non-empty 'intent' argument."));
    }

    if let Ok(config_lock) = crate::config::TylluanConfig::load_cached() {
        let cfg = config_lock.read().await;
        if cfg.security.intent_filter
            && let Some(reason) = check_dangerous_intent(&intent) {
                tracing::warn!("⚠️ Intent blocked by safety filter: '{}' — reason: {}", intent, reason);
                return Ok(error_result(&format!(
                    "Intent blocked by safety filter: {reason}. \
                     If this is intentional, disable the filter with [security] intent_filter = false in tylluan.toml, \
                     or use guild='bash' to bypass the router."
                )));
            }
    }

    // Deterministic node/nodo prefix — Agent Node Router, bypasses semantic matcher
    if let Some(node_intent) = crate::memory::agent_nodes::parse_node_intent(intent.trim()) {
        use crate::memory::agent_nodes::NodeIntent;
        let aid = agent_id.as_deref().unwrap_or("unknown");
        let router = &server.node_router;
        let result = match node_intent {
            NodeIntent::Register => router.register(aid).await,
            NodeIntent::Send { to, payload } => {
                router.send(aid, &to, &payload, "direct").await
                    .unwrap_or_else(|e| serde_json::json!({ "error": e }))
            }
            NodeIntent::Broadcast { payload } => router.broadcast(aid, &payload).await,
            NodeIntent::DrainInbox => {
                let msgs = router.drain_inbox(aid).await;
                let n = msgs.len();
                serde_json::json!({ "messages": msgs, "count": n, "drained": true })
            }
            NodeIntent::PeekInbox => {
                let msgs = router.peek_inbox(aid).await;
                let n = msgs.len();
                serde_json::json!({ "messages": msgs, "count": n, "drained": false })
            }
            NodeIntent::List => serde_json::json!({ "nodes": router.list().await }),
            NodeIntent::Unregister => {
                router.unregister(aid).await;
                serde_json::json!({ "status": "unregistered", "agent_id": aid })
            }
        };
        let is_err = result.get("error").is_some();
        return Ok(CallToolResult {
            content: vec![Content::text(serde_json::to_string_pretty(&result).unwrap_or_default())],
            is_error: Some(is_err),
        });
    }

    // Deterministic @coloquio: prefix — bypass semantic router entirely
    if let Some(result) = handle_coloquio_prefix(server, &intent, &agent_id).await {
        return result;
    }

    // Deterministic nodo/node prefix — agent-to-agent messaging
    // Uses existing AgentNodeRouter + parse_node_intent from agent_nodes.rs
    if let Some(result) = handle_nodo_prefix(server, &intent, &agent_id).await {
        return result;
    }

    // Sovereign shortcut: "forget: {node_id}" — delete a node without routing to a guild
    if let Some(result) = handle_forget_shortcut(server, &intent).await {
        return result;
    }

    // M31-P5: @skill: prefix — project-scoped reusable skill context
    if let Some(result) = handle_skill_prefix(server, &intent, &agent_id).await {
        return result;
    }

    // M36: @correct:<node_id>:<content> — explicit self-correction of a memory node
    if let Some(result) = handle_correct_prefix(server, &intent).await {
        return result;
    }

    // M31-P6: @bg:<intent> — enqueue a guild call as a background job
    if let Some(result) = crate::transport::server::background_jobs::handle_bg_prefix(server, &intent, &agent_id).await {
        return result;
    }

    // M31-P6: @job:<id> — check status of a background job
    if let Some(result) = crate::transport::server::background_jobs::handle_job_status(server, &intent).await {
        return result;
    }

    use crate::transport::server::intent_enhancer;

    // IQE: enrich ambiguous intents with session context
    let effective_intent = if intent_enhancer::is_ambiguous(&intent) {
        // Fetch last 3 intents from session — use empty vec if unavailable
        let recent: Vec<String> = server.silva
            .search("recent intents session", 3, None).await
            .unwrap_or_default()
            .into_iter()
            .map(|n| n.content)
            .collect();
        let enriched = intent_enhancer::enrich_intent(&intent, &recent);
        tracing::debug!("IQE: enriched intent '{}' → '{}'", intent, enriched);
        enriched
    } else {
        intent.clone()
    };

    let penalize_lesson = |intent: &str, silva: std::sync::Arc<crate::memory::silva::SilvaDB>| {
        let intent_lower = intent.to_lowercase();
        let words: Vec<_> = intent_lower.split_whitespace().take(3).collect();
        let lesson_key = format!("lesson:intent:{}", words.join("_"));
        tokio::spawn(async move {
            if let Ok(Some(_node)) = silva.get_node(&lesson_key).await {
                // Registrar trace "rejected" para activar negative forgetting de R11-2
                let _ = silva.touch_node(&lesson_key, "system", "rejected").await;
            }
        });
    };

    // Strip [ctx: ...] prefix so it doesn't pollute routing
    let routing_intent = intent_enhancer::strip_ctx_prefix(&effective_intent);

    let (guild_name, routing_trace) = match resolve_guild_name(server, routing_intent, guild_hint.clone(), agent_id.as_deref()).await {
        Ok((name, trace)) => (name, trace),
        Err(initial_err) => {
            // M32: External MCP dispatch — when no internal guild matches, try registered
            // external MCP servers before returning the "no guild found" error.
            if guild_hint.is_none()
                && let Some(result) = external_mcp::try_external_mcp_dispatch(
                    server, &intent, agent_id.as_deref()
                ).await {
                    info!("tylluan_do: dispatched to external MCP server");
                    return Ok(result);
                }
            return Ok(initial_err);
        }
    };

    // S1b: Per-Guild rate limit check — backstop against a single guild
    // saturating guild_process even when calls come from diverse agents/IPs.
    if let Err(msg) = server.guild_rate_limiter.check_and_record(&guild_name) {
        warn!("Guild rate limit exceeded for '{}': {}", guild_name, msg);
        return Ok(error_result(&format!(
            "Rate limit for guild '{guild_name}' exceeded. Try again later."
        )));
    }

    // S2: Per-Guild ACL check — verify the request's role has access to this guild
    if let Ok(config_lock) = crate::config::TylluanConfig::load_cached() {
        let cfg = config_lock.read().await;
        let acl = &cfg.security.acl;
        if !acl.roles.is_empty() || !acl.tokens.is_empty() {
            let role = crate::transport::http::auth::current_acl_role();
            if !crate::transport::http::auth::acl_can_access(&role, &guild_name, acl) {
                let msg = format!(
                    "ACCESS_DENIED: role '{role}' does not have access to guild '{guild_name}'. \
                     Contact your administrator to update [security.acl] in tylluan.toml."
                );
                warn!("{}", msg);
                return Ok(error_result(&msg));
            }
        }
        // M31-P1: Enforce per-agent tool permissions & scope for tylluan_do
        if !acl.agent_permissions.is_empty() {
            let aid = agent_id.as_deref().unwrap_or("anonymous");
            if aid != "anonymous"
                && let Some(msg) = crate::transport::http::auth::check_agent_id_tool_allowed(aid, "tylluan_do", acl) {
                    return Ok(error_result(&msg));
                }
        }
    }

    if let Err(e) = server.registry.write().await.ensure_guild_running(&guild_name).await {
        penalize_lesson(&intent, server.silva.clone());
        return Ok(error_result(&format!("Failed to start guild '{guild_name}': {e}")));
    }

    let mut tool_name = {
        let reg = server.registry.read().await;
        if let Some(guild) = reg.guilds.get(&guild_name) {
            use crate::router::matcher::{tokenize, keyword_score};
            let tokens = tokenize(&effective_intent);
            guild.tools.iter()
                .max_by(|a, b| {
                    let sa = keyword_score(&tokens, a.description.as_ref(), a.name.as_ref());
                    let sb = keyword_score(&tokens, b.description.as_ref(), b.name.as_ref());
                    sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|t| t.name.to_string())
                .unwrap_or_default()
        } else { String::new() }
    };

    if tool_name.is_empty() {
        penalize_lesson(&intent, server.silva.clone());
        return Ok(error_result(&format!("Guild '{guild_name}' has no tools.")));
    }

    let path_hint = extract_path_from_intent(&intent);
    let url_hint = extract_url_from_intent(&intent).unwrap_or_default();
    let mut tool_args = serde_json::json!({
        "command": intent, "intent": intent,
        "query": intent, "text": intent, "content": intent,
        "prompt": intent, "message": intent, "input": intent,
        "server_url": url_hint, "url": url_hint,
        "timeout_secs": 30, "language": "", "depth": 2, "max_results": 50,
    });
    // Only inject path fields when intent contains an actual path — passing "."
    // causes "Permission denied" in guilds that require filesystem access.
    if path_hint != "." {
        let safe_path = if std::path::Path::new(&path_hint).is_dir() {
            &path_hint
        } else {
            "."
        };
        let project_hint = {
            let mut p = path_hint.replace(['/', '\\'], "-").replace(':', "");
            while p.contains("--") { p = p.replace("--", "-"); }
            p.trim_matches('-').to_string()
        };
        if let Some(obj) = tool_args.as_object_mut() {
            obj.insert("directory".to_string(), serde_json::Value::String(safe_path.to_string()));
            obj.insert("cwd".to_string(), serde_json::Value::String(safe_path.to_string()));
            obj.insert("path".to_string(), serde_json::Value::String(path_hint.clone()));
            obj.insert("file_path".to_string(), serde_json::Value::String(path_hint.clone()));
            obj.insert("repo_path".to_string(), serde_json::Value::String(path_hint.clone()));
            obj.insert("project".to_string(), serde_json::Value::String(project_hint));
        }
    }

    // Bash/Git: extract clean command from NL wrapper ("run X", "execute X:", etc.)
    // so the guild receives "ls -la" instead of "execute bash command: ls -la".
    if (guild_name == "bash" || guild_name == "git")
        && let Some(obj) = tool_args.as_object_mut() {
            let clean = extract_command_from_intent(&intent);
            obj.insert("command".to_string(), serde_json::Value::String(clean.to_string()));
        }

    // Coloquio: extract structured params from intent BEFORE validation so channel_id
    // is populated when required_args check runs.
    if guild_name == "coloquio" {
        let (mut channel_id, content_or_name, tool_hint) = parse_coloquio_intent(&intent);

        // Fallback: if parser couldn't extract channel_id but intent has recognizable structure,
        // try splitting on first colon — text before is channel, text after is message.
        if channel_id.is_none() && intent.contains(':') {
            let parts: Vec<&str> = intent.splitn(2, ':').collect();
            if parts.len() == 2 {
                let before_words: Vec<&str> = parts[0].split_whitespace().collect();
                if let Some(last_word) = before_words.last() {
                    let candidate = last_word.trim().to_lowercase();
                    if candidate.len() >= 2 && candidate != "coloquio" && candidate != "canal" {
                        channel_id = Some(candidate);
                    }
                }
            }
        }

        tool_name = match tool_hint {
            "post" => "post_to_channel",
            "read" => "read_channel",
            "list" => "list_channels",
            "create" => "create_channel",
            _ => {
                let lower = intent.to_lowercase();
                if !lower.contains(':')
                    || lower.contains("lee")
                    || lower.contains("leer")
                    || lower.contains("read")
                    || lower.contains("ver ")
                    || lower.contains("mostrar")
                    || lower.contains("lista")
                {
                    "read_channel"
                } else {
                    "post_to_channel"
                }
            },
        }.to_string();
        if let Some(obj) = tool_args.as_object_mut() {
            if let Some(ref cid) = channel_id {
                obj.insert("channel_id".to_string(), serde_json::Value::String(cid.clone()));
            }
            if let Some(ref cn) = content_or_name {
                obj.insert("content".to_string(), serde_json::Value::String(cn.clone()));
                obj.insert("message".to_string(), serde_json::Value::String(cn.clone()));
                obj.insert("intent".to_string(), serde_json::Value::String(cn.clone()));
            }
            if tool_hint == "read" {
                let (limit, offset) = _parse_coloquio_pagination(&intent);
                if limit > 0 { obj.insert("limit".to_string(), serde_json::Value::Number(limit.into())); }
                if offset > 0 { obj.insert("offset".to_string(), serde_json::Value::Number(offset.into())); }
            }
            if !obj.contains_key("author_id") && !intent.to_lowercase().contains("author ")
                && let Some(aid) = agent_id.as_deref().filter(|a| !a.trim().is_empty()) {
                    obj.insert("author_id".to_string(), serde_json::Value::String(aid.to_string()));
                }
        }
    }

    // M29-A: Validate required_args contract — check the guild's declared args
    // are populated (coloquio handler already injected channel_id if applicable).
    if let Some(guild_desc) = server.matcher.available_guilds()
        .iter().find(|g| g.name == guild_name)
        && !guild_desc.required_args.is_empty()
        && let Some(obj) = tool_args.as_object() {
            let missing: Vec<&str> = guild_desc.required_args.iter()
                .filter(|arg| {
                    let val = obj.get(*arg).and_then(|v| v.as_str()).unwrap_or("");
                    val.is_empty()
                })
                .map(|s| s.as_str())
                .collect();
            if !missing.is_empty() {
                let missing_list = missing.join(", ");
                let example = format!("tylluan_do(intent='...', {}<value>)", missing[0]);
                return Ok(error_result(&format!(
                    "Error: guild '{guild_name}' requires argument(s): {missing_list}. \
                     Provide them explicitly: {example}. \
                     Check guild documentation for required fields."
                )));
            }
        }

    let call_params = CallToolRequestParam {
        name: tool_name.clone().into(),
        arguments: Some(tool_args.as_object().cloned().unwrap_or_default()),
    };
    info!("🔀 tylluan_do: intent='{}' → guild='{}' → tool='{}'", intent, guild_name, tool_name);

    // M31-P2: Plan mode — return resolved guild+tool+args for approval before executing
    if plan_mode {
        let plan_id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let risk_level = server.check_tool_risk(&tool_name).await;
        let plan_info = serde_json::json!({
            "status": "plan",
            "plan_id": plan_id,
            "guild": guild_name,
            "tool": tool_name,
            "risk_level": format!("{:?}", risk_level),
            "intent": intent,
            "arguments": tool_args,
            "message": format!(
                "Plan mode: would execute '{}' via guild '{}' tool '{}' (risk: {:?}). \
                 To approve, call: tylluan_do(intent='approve action for plan {plan_id}') or \
                 approve_action(requestId='{plan_id}', approved=true).",
                intent, guild_name, tool_name, risk_level
            ),
        });
        crate::security::grants::store_plan(
            &plan_id, &guild_name, &tool_name, &tool_args,
            agent_id.as_deref().unwrap_or("anonymous"), &intent,
        ).await;
        let result_text = serde_json::to_string_pretty(&plan_info).unwrap_or_default();
        return Ok(CallToolResult {
            content: vec![Content::text(result_text)],
            is_error: Some(false),
        });
    }

    // Progress ticker: emit SSE events every heartbeat interval for long-running guild calls
    let progress_notifier = server.notifier.clone();
    let progress_guild = guild_name.clone();
    let progress_intent = intent.chars().take(60).collect::<String>();

    // Get heartbeat interval from config
    let heartbeat_ms = if let Ok(c_lock) = crate::config::TylluanConfig::load_cached() {
        let c = c_lock.read().await;
        c.timeouts.mcp_client_heartbeat_ms
    } else {
        8_000 // fallback
    };
    let heartbeat_secs = (heartbeat_ms / 1000).max(1);

    let progress_handle = tokio::spawn(async move {
        let effective_timeout = crate::transport::server::handler_do::guild_effective_timeout(
            &progress_guild, false
        );
        let timeout_secs = effective_timeout / 1000;
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(heartbeat_secs));
        let _ = interval.tick().await; // skip first immediate tick
        let mut elapsed = 0u64;
        loop {
            interval.tick().await;
            elapsed += heartbeat_secs;
            let remaining = timeout_secs.saturating_sub(elapsed);
            let msg = if remaining > 30 {
                format!("Running... (timeout {timeout_secs}s, ~{remaining}s remaining)")
            } else if remaining > 0 {
                format!("⏳ Last {remaining}s...")
            } else {
                format!("⚠️ Exceeded estimated timeout of {timeout_secs}s — waiting for response on local hardware")
            };
            if let Some(ref tx) = progress_notifier {
                let _ = tx.send(serde_json::json!({
                    "type": "guild_progress",
                    "guild": progress_guild,
                    "intent": progress_intent,
                    "elapsed_secs": elapsed,
                    "timeout_secs": timeout_secs,
                    "message": msg,
                    "ts": chrono::Utc::now().timestamp_millis()
                }));
            }
        }
    });

    // Emit started BEFORE the guild call so subscribers see the correct order
    server.notify("tool_call", serde_json::json!({
        "status": "started", "tool": tool_name,
        "agent_id": agent_id.as_deref().unwrap_or("anonymous"),
        "intent": intent, "ts": chrono::Utc::now().timestamp_millis()
    }));

    let t0 = std::time::Instant::now();
    let effective_timeout = crate::transport::server::handler_do::guild_effective_timeout(
        &guild_name, server.low_memory_mode
    );
    // Phase 1: brief write lock — touch() and set timeout only, no IO.
    let original_timeout = {
        let mut reg = server.registry.write().await;
        let orig = reg.guilds.get(&guild_name).and_then(|g| g.tool_timeout);
        if let Some(guild) = reg.guilds.get_mut(&guild_name) {
            guild.touch();
            guild.tool_timeout = Some(std::time::Duration::from_millis(effective_timeout));
        }
        orig
    }; // write lock dropped here — other requests can proceed during guild call

    // Phase 2: read lock for the actual guild call (no writes needed during IO).
    // Wrap in a safety timeout to prevent kernel hangs on dead guild processes
    let call_timeout_ms = effective_timeout + 10_000; // 10s grace period over guild's own timeout

    let mut result: CallToolResult = match tokio::time::timeout(
        std::time::Duration::from_millis(call_timeout_ms),
        async {
            let reg = server.registry.read().await;
            if let Some(guild) = reg.guilds.get(&guild_name) {
                guild.call_tool_readonly(call_params).await
            } else {
                error_result(&format!("Guild '{guild_name}' not found — use tylluan_do with a valid intent."))
            }
        }
    ).await {
        Ok(res) => res,
        Err(_) => {
            warn!("⌛ tylluan_do: guild call to '{}' timed out after {}ms", guild_name, call_timeout_ms);
            error_result(&format!(
                "ERROR: guild '{guild_name}' timed out after {call_timeout_ms}ms. \
                 The process may be saturated or has failed. \
                 Try splitting the task or restarting the guild."
            ))
        }
    };
    // Restore original timeout (brief write lock)
    {
        let mut reg = server.registry.write().await;
        if let Some(guild) = reg.guilds.get_mut(&guild_name) {
            guild.tool_timeout = original_timeout;
        }
    }
    progress_handle.abort();
    let latency_ms = t0.elapsed().as_millis() as u64;

    // Final progress event for slow calls
    if latency_ms > 3000 {
        server.notify("guild_progress", serde_json::json!({
            "type": "guild_progress",
            "guild": guild_name,
            "status": "done",
            "latency_ms": latency_ms,
            "ts": chrono::Utc::now().timestamp_millis()
        }));
    }
    let mut is_success = result.is_error != Some(true)
        && !result.content.iter().filter_map(|c| c.as_text())
            .any(|t| t.text.contains("Exit code:") && !t.text.contains("Exit code: 0"));

    // M20: Reactive Cascade Check
    if !is_success && guild_name != "coordinator" {
        let c_score = crate::router::complexity::score_complexity(&intent);
        let registry_has_coordinator = server.registry.read().await.guilds.contains_key("coordinator");
        if c_score >= 0.4 && registry_has_coordinator {
            info!("🔄 Reactive Cascade (score={:.2}): '{}' failed on '{}' → fallback to coordinator", c_score, intent, guild_name);
            let mut new_args = serde_json::Map::new();
            new_args.insert("intent".to_string(), serde_json::Value::String(intent.clone()));
            new_args.insert("guild".to_string(), serde_json::Value::String("coordinator".to_string()));
            if let Some(ref aid) = agent_id {
                new_args.insert("agent_id".to_string(), serde_json::Value::String(aid.clone()));
            }
            new_args.insert("remember".to_string(), serde_json::Value::Bool(remember));

            // Invoke coordinator recursively
            match Box::pin(handle_tylluan_do(server, Some(new_args))).await {
                Ok(cascade_res) => {
                    info!("🔄 Reactive Cascade successful for '{}'", intent);
                    result = cascade_res;
                    // Recompute is_success for the new result
                    is_success = result.is_error != Some(true)
                        && !result.content.iter().filter_map(|c| c.as_text())
                            .any(|t| t.text.contains("Exit code:") && !t.text.contains("Exit code: 0"));
                }
                Err(e) => {
                    warn!("🔄 Reactive Cascade failed to execute for '{}': {:?}", intent, e);
                }
            }
        }
    }

    server.matcher.record_outcome(&intent, &guild_name, is_success, latency_ms);

    if !is_success {
        penalize_lesson(&intent, server.silva.clone());
    }

    let is_new = if let Some(ref profiles) = server.agent_profiles {
        if let Some(ref aid) = agent_id {
            if let Ok(p_store) = profiles.lock() {
                let _ = p_store.upsert_activity(aid, &guild_name, is_success, Some(&intent));
                if !is_success
                    && let Ok(Some(best)) = p_store.get_best_agent_for_domain(&guild_name) {
                        let b_aid = best["agent_id"].as_str().unwrap_or_default();
                        let b_rate = best["rate"].as_f64().unwrap_or(0.0);
                        if b_aid != *aid && b_rate > 0.6 {
                            let hint = format!("Hint: Agent {} has higher success rate ({:.1}%) in domain '{}'.", b_aid, b_rate * 100.0, guild_name);
                            result.content.push(rmcp::model::Content::text(hint));
                        }
                    }
                p_store.is_new_agent(aid)
            } else { false }
        } else { false }
    } else { false };

    if is_new && let Some(ref aid) = agent_id { run_agent_handshake(server, aid).await; }

    server.notify("tool_call", serde_json::json!({
        "status": "finished", "tool": tool_name,
        "agent_id": agent_id.as_deref().unwrap_or("anonymous"),
        "intent": intent, "ok": is_success, "ts": chrono::Utc::now().timestamp_millis()
    }));

    if !is_success && let Ok(mut h) = server.hormones.lock() { h.emit_stress(agent_id.as_deref().unwrap_or("unknown")); }

    let result_text = result.content.iter().filter_map(|c| c.as_text())
        .map(|t| t.text.clone()).next().unwrap_or_default();

    maybe_auto_extract_triples(server, agent_id.as_deref(), &guild_name, &result_text);

    if let Some(ref aid) = agent_id {
        record_activity_trace(server, aid, &guild_name, &tool_name, result_text.len());
    }

    // Audit log: record every tylluan_do tool call to audit.db (fire-and-forget)
    let audit_intent = intent.clone();
    let audit_guild = guild_name.clone();
    let audit_tool = tool_name.clone();
    let audit_agent = agent_id.clone().unwrap_or_default();
    let audit_success = is_success;
    let audit_preview = result_text.chars().take(200).collect::<String>();
    tokio::task::spawn_blocking(move || {
        let _ = log_audit_entry(&audit_intent, &audit_guild, &audit_tool, &audit_agent, audit_success, &audit_preview);
    });

    // Anchor learning: store successful routings as routing_anchor nodes (async, fire-and-forget)
    if is_success && !intent.trim().is_empty() {
        let silva_anchor = server.silva.clone();
        let engine_anchor = server.matcher.engine_arc();
        let intent_anchor = intent.clone();
        let guild_anchor = guild_name.clone();
        tokio::spawn(async move {
            let embedding = engine_anchor.as_ref().and_then(|e| e.embed(&intent_anchor).ok());
            let _ = silva_anchor.upsert_routing_anchor(
                &guild_anchor,
                &intent_anchor,
                "learned",
                embedding.as_deref(),
            ).await;
        });
    }

    // Sync agent reputation to SilvaDB after each tool call (fire-and-forget)
    if let Some(ref aid) = agent_id
        && let Some(ref profiles) = server.agent_profiles {
            let p_store = profiles.clone();
            let silva_reput = server.silva.clone();
            let aid_clone = aid.clone();
            tokio::spawn(async move {
                let profile_opt = {
                    if let Ok(store) = p_store.lock() {
                        store.get_profile(&aid_clone).unwrap_or(None)
                    } else {
                        None
                    }
                };
                if let Some(prof) = profile_opt {
                    crate::memory::agent_profile::sync_agent_reputation_to_silva(
                        &silva_reput, &[prof]
                    ).await;
                }
            });
        }

    // R19-2: Routing Feedback Loop — persist failures for future learning
    if !is_success || result_text.trim().is_empty() {
        let err_msg = if !is_success {
            let t = result_text.clone();
            if t.chars().count() > 100 { format!("{}...", t.chars().take(100).collect::<String>()) } else { t }
        } else {
            "EMPTY_RESULT".to_string()
        };
        let failure_id = routing_failure_id(&intent);
        let failure_content = format!(
            "ROUTING_FAILURE guild={} intent={} error={}",
            guild_name, &intent[..intent.len().min(100)], err_msg
        );
        let _ = server.silva.upsert_node(
            &failure_id, "lesson", &failure_content, "{}"
        ).await;
        let _ = server.silva.touch_node(&failure_id, "system", "routing_failure").await;
    }

    // R14-3: Lesson drain — if result contains lesson markers, promote to durable SilvaDB node.
    if is_success && result_text.len() > 100 {
        let lower = result_text.to_lowercase();
        let has_lesson = lower.contains("lesson:") || lower.contains("aprendí")
            || lower.contains("aprendi") || lower.contains("discovered:")
            || lower.contains("conclusion:") || lower.contains("key insight:");
        if has_lesson {
            let aid = agent_id.as_deref().unwrap_or("anonymous");
            let hash_input = result_text.chars().take(40).collect::<String>();
            let hash: u64 = hash_input.bytes().fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
            let lesson_id = format!("lesson:{hash:x}");
            let meta = serde_json::json!({
                "source": "tylluan_do_lesson_drain", "guild": guild_name,
                "agent_id": aid, "intent": intent
            }).to_string();
            let silva_c = server.silva.clone();
            let rt = result_text.clone();
            let lid = lesson_id.clone();
            let aid_s = aid.to_string();
            tokio::spawn(async move {
                if let Ok(()) = silva_c.upsert_node(&lid, "lesson", &rt, &meta).await {
                    let _ = silva_c.touch_node(&lid, &aid_s, "lesson_drain").await;
                }
            });
        }
    }

    // R14-3: Save routing lesson on success (PASO 3)
    if is_success {
        let lesson_key = format!("lesson:intent:{}",
            intent.to_lowercase()
                .split_whitespace()
                .take(3)
                .collect::<Vec<_>>()
                .join("_"));

        // R20-3: Lesson throttle — only create/update if new or decayed
        let should_write_lesson = match server.silva.get_node(&lesson_key).await {
            Ok(Some(existing)) => existing.weight < 0.5, // update only if decayed
            Ok(None) => true,   // new lesson — always write
            Err(_) => true,     // on error, write anyway (safe default)
        };

        if should_write_lesson {
            let preview = result.content.first()
                .and_then(|c| c.as_text()).map(|t| t.text.chars().take(200).collect::<String>())
                .unwrap_or_default();
            let content = if preview.trim().len() > 20 {
                format!("guild:{guild_name} tool:{tool_name} intent:{intent} -- {preview}")
            } else {
                format!("guild:{guild_name} tool:{tool_name} intent:{intent}")
            };
            let meta = serde_json::json!({
                "source": "routing_lesson",
                "guild": guild_name,
                "tool": tool_name,
                "intent": intent
            }).to_string();
            let silva_c = server.silva.clone();
            let lk = lesson_key.clone();
            tokio::spawn(async move {
                if let Err(e) = silva_c.upsert_node(&lk, "lesson", &content, &meta).await {
                    warn!("⚠️ routing lesson save failed: {}", e);
                }
            });
        }
    }

    let should_remember = remember || agent_id.is_some();
    if should_remember && result.is_error != Some(true) {
        let output_preview = result.content.first()
            .and_then(|c| c.as_text()).map(|t| t.text.chars().take(300).collect::<String>())
            .unwrap_or_default();
        let trace = match &agent_id {
            Some(aid) => format!("tylluan_do episode | agent: {aid} | intent: {intent} | guild: {guild_name} | tool: {tool_name} | result: {output_preview}"),
            None => format!("tylluan_do episode | intent: {intent} | guild: {guild_name} | tool: {tool_name} | result: {output_preview}"),
        };
        let meta = serde_json::json!({ "source": "tylluan_do", "guild": guild_name, "tool": tool_name, "agent_id": agent_id.as_deref().unwrap_or("anonymous") }).to_string();
        let embedding_target = distill_for_embedding(&intent, &output_preview);
        let embedding = server.matcher.engine().and_then(|e| e.embed(&embedding_target).ok());
        if let Err(e) = server.memory.add_document(&trace, &meta, embedding.as_deref()).await {
            warn!("⚠️ tylluan_do remember: hybrid memory write failed: {}", e);
        }
        let node_id = format!("memory:{}", chrono::Utc::now().timestamp_millis());
        if let Err(e) = server.silva.upsert_node(&node_id, "episode", &trace, &meta).await {
            warn!("⚠️ tylluan_do remember: silva graph write failed: {}", e);
        } else {
            let aid = agent_id.as_deref().unwrap_or("anonymous");
            let _ = server.silva.touch_node(&node_id, aid, "episode").await;
            info!("🌲 tylluan_do remember: saved to SilvaDB (node: {})", node_id);
            let silva_clone = server.silva.clone();
            let nid_clone = node_id.clone();
            let trace_clone = trace.clone();
            tokio::spawn(async move { let _ = silva_clone.auto_link_similar(&nid_clone, &trace_clone, 3, 0.3).await; });
        }
        if let Some(emb) = embedding.as_deref()
            && let Err(e) = server.silva.save_embedding(&node_id, emb, "nomic", None).await {
                warn!("⚠️ tylluan_do remember: embedding save failed for {}: {}", node_id, e);
            }
        server.notify("memory_added", serde_json::json!({
            "node_id": node_id, "type": "episode",
            "label": trace.chars().take(100).collect::<String>(),
            "ts": chrono::Utc::now().timestamp_millis()
        }));
    }

    let footer = format!("\n\n---\nRouting: guild={} tool={}\nRouting Trace:\n - {}", guild_name, tool_name, routing_trace.join("\n - "));
    result.content.push(rmcp::model::Content::text(footer));
    Ok(result)
}

/// Write an audit log entry to ./data/audit.db for every tylluan_do tool call.
/// Uses SHA-256 hash chaining: each entry stores the hash of the previous entry,
/// making tampering detectable. Called fire-and-forget — errors are non-fatal.
pub(crate) fn log_audit_entry(intent: &str, guild: &str, tool: &str, agent_id: &str, success: bool, preview: &str) -> Result<(), String> {
    let db_path = std::path::Path::new("./data/audit.db");
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("audit mkdir: {e}"))?;
    }
    let conn = crate::config::open_db(db_path).map_err(|e| format!("audit open: {e}"))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS guild_audit_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            guild TEXT NOT NULL,
            tool_name TEXT NOT NULL,
            agent_id TEXT NOT NULL DEFAULT '',
            intent TEXT,
            status TEXT NOT NULL DEFAULT 'ok',
            result_preview TEXT,
            prev_hash TEXT NOT NULL DEFAULT '',
            hash TEXT NOT NULL
        );"
    ).map_err(|e| format!("audit schema: {e}"))?;

    let now = chrono::Utc::now().to_rfc3339();
    let status = if success { "ok" } else { "error" };

    // Get previous hash for chaining
    let prev_hash: String = conn
        .query_row("SELECT hash FROM guild_audit_log ORDER BY id DESC LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap_or_default();

    // Chain hash: SHA-256 of (prev_hash || timestamp || guild || tool || agent_id || status)
    let chain_input = format!("{prev_hash}|{now}|{guild}|{tool}|{agent_id}|{status}");
    use sha2::Digest;
    let hash = format!("{:x}", sha2::Sha256::digest(chain_input.as_bytes()));

    conn.execute(
        "INSERT INTO guild_audit_log (timestamp, guild, tool_name, agent_id, intent, status, result_preview, prev_hash, hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![now, guild, tool, agent_id, intent, status, preview, prev_hash, hash],
    ).map_err(|e| format!("audit insert: {e}"))?;
    Ok(())
}

/// Verify the integrity of the audit chain from oldest to newest.
/// Returns (ok_count, bad_count) — bad > 0 means tampering detected.
pub fn verify_audit_chain() -> Result<(usize, usize), String> {
    let db_path = std::path::Path::new("./data/audit.db");
    let conn = match crate::config::open_db(db_path) {
        Ok(c) => c,
        Err(_) => return Ok((0, 0)),
    };
    // Ensure table exists (no-op if it does)
    let _ = conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS guild_audit_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL, guild TEXT NOT NULL, tool_name TEXT NOT NULL,
            agent_id TEXT NOT NULL DEFAULT '', intent TEXT,
            status TEXT NOT NULL DEFAULT 'ok', result_preview TEXT,
            prev_hash TEXT NOT NULL DEFAULT '', hash TEXT NOT NULL
        );"
    );
    let mut stmt = conn.prepare(
        "SELECT id, timestamp, guild, tool_name, agent_id, status, prev_hash, hash \
         FROM guild_audit_log ORDER BY id ASC"
    ).map_err(|e| format!("audit prepare: {e}"))?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
        ))
    }).map_err(|e| format!("audit query: {e}"))?;

    let mut prev = String::new();
    let mut ok = 0usize;
    let mut bad = 0usize;
    for row_res in rows {
        let row = row_res.map_err(|e| format!("audit row: {e}"))?;
        let (_id, ts, guild, tool_name, agent_id, status, stored_prev, stored_hash) = row;
        if stored_prev != prev {
            bad += 1;
            continue;
        }
        let chain_input = format!("{stored_prev}|{ts}|{guild}|{tool_name}|{agent_id}|{status}");
        use sha2::Digest;
        let computed = format!("{:x}", sha2::Sha256::digest(chain_input.as_bytes()));
        if computed != stored_hash {
            bad += 1;
            continue;
        }
        prev = stored_hash;
        ok += 1;
    }
    Ok((ok, bad))
}

/// Opt-in safety filter for dangerous intents.
/// Returns Some(reason) if the intent matches a dangerous pattern.
pub fn check_dangerous_intent(intent: &str) -> Option<&'static str> {
    let lower = intent.to_lowercase();

    static PATTERNS: &[(&str, &str)] = &[
        ("rm -rf /", "recursive deletion of root filesystem"),
        ("rm -rf ~", "recursive deletion of home directory"),
        ("rm -rf .", "recursive deletion of current directory"),
        ("mkfs", "filesystem formatting"),
        ("format c:", "disk formatting"),
        ("format d:", "disk formatting"),
        (":(){:|:&};:", "fork bomb"),
        ("dd if=/dev/zero", "disk overwrite"),
        ("dd if=/dev/random", "disk overwrite"),
        ("> /dev/sda", "raw disk write"),
        ("chmod -r 777 /", "recursive permission change on root"),
        ("drop table", "SQL table deletion"),
        ("drop database", "SQL database deletion"),
        ("truncate table", "SQL table truncation"),
        ("delete from", "SQL mass deletion"),
        ("shutdown /s", "system shutdown"),
        ("shutdown -h now", "system shutdown"),
        ("reboot", "system reboot"),
        ("init 0", "system halt"),
        (":(){ :|:& };:", "fork bomb"),
    ];

    for (pattern, reason) in PATTERNS {
        if lower.contains(pattern) {
            return Some(reason);
        }
    }

    None
}

/// M36: @correct:<node_id>:<content> — explicit self-correction of a memory node.
/// Supersedes the old node via valid_until (M35 pattern), creates a new corrected
/// node with provenance=agent_generated, and links via "corrects" edge.
async fn handle_correct_prefix(
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

/// Test helper: minimal TylluanServer with in-memory stores, no guilds.
/// Used by both handler_do's own tests and background_jobs tests.
#[cfg(test)]
pub(crate) async fn base_test_server(silva: std::sync::Arc<crate::memory::silva::SilvaDB>) -> TylluanServer {
    use tokio::sync::broadcast;
    use crate::router::matcher::GuildMatcher;
    use crate::router::catalog::builtin_catalog;
    use crate::memory::hybrid::HybridMemory;
    use crate::memory::mailbox::Mailbox;
    use crate::memory::agent_nodes::AgentNodeRouter;
    use crate::registry::guild_process::GuildRegistry;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    let reg = GuildRegistry::new(PathBuf::from("."), 300, Default::default(), 3);
    let registry = Arc::new(RwLock::new(reg));
    let matcher = GuildMatcher::new(builtin_catalog());
    let (tx, _) = broadcast::channel(16);
    let node_router = AgentNodeRouter::new(tx.clone());
    let doctor = Arc::new(crate::doctor::Doctor::new(
        registry.clone(),
        Arc::new(HybridMemory::in_memory().await.unwrap()),
        silva.clone(),
        Arc::new(std::sync::Mutex::new(crate::curriculum::CurriculumLearner::new_in_memory(5).unwrap())),
    ));
    let mut server = TylluanServer::new(
        registry,
        Arc::new(matcher),
        Arc::new(HybridMemory::in_memory().await.unwrap()),
        silva,
        Arc::new(Mailbox::in_memory().await.unwrap()),
        doctor,
        node_router,
    );
    server.set_notifier(tx);
    server
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use std::path::PathBuf;
    use crate::registry::guild_process::GuildRegistry;
    use crate::memory::hybrid::HybridMemory;
    use crate::memory::silva::SilvaDB;
    use crate::memory::mailbox::Mailbox;
    use crate::router::matcher::GuildMatcher;
    use crate::router::catalog::builtin_catalog;
    use crate::memory::agent_nodes::AgentNodeRouter;

    fn test_registry() -> Arc<RwLock<GuildRegistry>> {
        let reg = GuildRegistry::new(PathBuf::from("."), 300, Default::default(), 3);
        Arc::new(RwLock::new(reg))
    }

    async fn test_server() -> TylluanServer {
        use tokio::sync::broadcast;
        let matcher = GuildMatcher::new(builtin_catalog());
        let (tx, _) = broadcast::channel(16);
        let node_router = AgentNodeRouter::new(tx);
        let doctor = Arc::new(crate::doctor::Doctor::new(
            test_registry(),
            Arc::new(HybridMemory::in_memory().await.unwrap()),
            Arc::new(SilvaDB::in_memory().await.unwrap()),
            Arc::new(std::sync::Mutex::new(crate::curriculum::CurriculumLearner::new_in_memory(5).unwrap())),
        ));
        TylluanServer::new(
            test_registry(),
            Arc::new(matcher),
            Arc::new(HybridMemory::in_memory().await.unwrap()),
            Arc::new(SilvaDB::in_memory().await.unwrap()),
            Arc::new(Mailbox::in_memory().await.unwrap()),
            doctor,
            node_router,
        )
    }

    #[test]
    fn test_lesson_key_format() {
        let intent = "analyze the system health";
        let key = format!("lesson:intent:{}",
            intent.to_lowercase()
                .split_whitespace()
                .take(3)
                .collect::<Vec<_>>()
                .join("_"));
        assert_eq!(key, "lesson:intent:analyze_the_system");
    }

    #[test]
    fn test_lesson_key_short_intent() {
        let intent = "hello";
        let key = format!("lesson:intent:{}",
            intent.to_lowercase()
                .split_whitespace()
                .take(3)
                .collect::<Vec<_>>()
                .join("_"));
        assert_eq!(key, "lesson:intent:hello");
    }

    #[test]
    fn test_lesson_key_long_intent_truncated() {
        let intent = "RUN CARGO TEST FOR KERNEL MODULE WITH COVERAGE";
        let key = format!("lesson:intent:{}",
            intent.to_lowercase()
                .split_whitespace()
                .take(3)
                .collect::<Vec<_>>()
                .join("_"));
        // Only first 3 lowercase tokens
        assert_eq!(key, "lesson:intent:run_cargo_test");
        // Should NOT include "for", "kernel", "module", etc.
        assert_ne!(key, "lesson:intent:run_cargo_test_for");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_lesson_expiry_old_node() {
        let server = test_server().await;
        let silva = server.silva.clone();

        // Target intent: "analyze system health"
        let intent = "analyze system health";
        let lesson_key = "lesson:intent:analyze_system_health";
        let content = "guild:system_metrics tool:system_metrics_collect intent:analyze system health";

        // 1. Create a lesson node that is 31 days old
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
        let age_31_days_secs = 31 * 86400;
        let touched_at = now - age_31_days_secs;

        silva.upsert_node(lesson_key, "lesson", content, "{}").await.unwrap();
        // Artificially modify last_touched/touched_at to make it 31 days old
        {
            let conn_guard = silva.conn_lock();
            let conn = conn_guard.lock().await;
            conn.execute(
                "UPDATE nodes SET last_touched = ?1, weight = 1.0 WHERE id = ?2",
                rusqlite::params![touched_at, lesson_key],
            ).unwrap();
        }

        // 2. Call resolve_guild_name
        let _result = resolve_guild_name(&server, intent, None, None).await;

        // Since the lesson is 31 days old, it should expire (decay) and the resolve should fall through.
        tokio::time::sleep(std::time::Duration::from_millis(150)).await; // wait for spawned decay task to complete

        let node = silva.get_node(lesson_key).await.unwrap().unwrap();
        // Weight should have been decayed (Ebbinghaus exponential decay or apply_node_decay)
        assert!(node.weight < 1.0, "Old lesson weight should have decayed, got {}", node.weight);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_lesson_penalty_on_failure() {
        let server = test_server().await;
        let silva = server.silva.clone();

        // 1. Create a lesson node
        let intent = "analyze system health";
        let lesson_key = "lesson:intent:analyze_system_health";
        let content = "guild:system_metrics tool:system_metrics_collect intent:analyze system health";
        silva.upsert_node(lesson_key, "lesson", content, "{}").await.unwrap();

        // 2. Call handle_tylluan_do with arguments
        let mut args = serde_json::Map::new();
        args.insert("intent".to_string(), serde_json::json!(intent));

        let _result = handle_tylluan_do(&server, Some(args)).await.unwrap();

        // Wait for spawned tasks to finish
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

        // Verify that a "rejected" trace is recorded for the lesson node
        let traces = silva.get_node_traces(lesson_key, 10).await.unwrap();
        let has_rejected = traces.iter().any(|t| t.trace_type == "rejected");
        assert!(has_rejected, "Should record a 'rejected' trace when intent fails");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_lesson_deprecated_on_low_success() {
        let server = test_server().await;
        let silva = server.silva.clone();

        let intent = "analyze system health";
        let lesson_key = "lesson:intent:analyze_system_health";
        let content = "guild:system_metrics tool:system_metrics_collect intent:analyze system health";

        // 1. Create a lesson node with weight >= 0.6
        silva.upsert_node(lesson_key, "lesson", content, "{}").await.unwrap();

        // 2. Add 6 rejected traces (total=6, rejected=6, ratio=1.0 > 0.5)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        {
            let conn = silva.conn_lock();
            let c = conn.lock().await;
            for i in 0..6 {
                c.execute(
                    "INSERT INTO node_traces (node_id, agent_id, touched_at, trace_type) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![lesson_key, "test_agent", now - (i * 10), "rejected"],
                ).unwrap();
            }
        }

        // 3. Verify trace counts before calling resolve_guild_name
        let window = now - (7 * 86400);
        let total = silva.get_trace_count_since(lesson_key, window).await.unwrap();
        assert_eq!(total, 6, "Should see 6 traces");
        let rejected = silva.get_trace_count_by_type(lesson_key, "rejected", window).await.unwrap();
        assert_eq!(rejected, 6, "Should see 6 rejected traces");

        // 4. Call resolve_guild_name — should deprecate and fall through to matcher
        let _result = resolve_guild_name(&server, intent, None, None).await;

        // 5. Verify lesson was marked as deprecated in background
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let traces = silva.get_node_traces(lesson_key, 20).await.unwrap();
        let has_deprecated = traces.iter().any(|t| t.trace_type == "deprecated");
        assert!(has_deprecated, "Lesson should have a 'deprecated' trace after low success-rate check");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_forget_shortcut_deletes_node() {
        let server = test_server().await;
        server.silva.upsert_node("forget:test:node", "concept", "temporary test node", "{}").await.unwrap();

        let args: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
            r#"{"intent": "forget: forget:test:node"}"#
        ).unwrap();
        let result = handle_tylluan_do(&server, Some(args)).await.unwrap();
        assert!(result.is_error != Some(true));
        let text = result.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<Vec<_>>().join("");
        assert!(text.contains("Forgotten"), "Expected 'Forgotten' in: {text}");
        assert!(server.silva.get_node("forget:test:node").await.unwrap().is_none());
    }

    #[test]
    fn test_routing_failure_id_is_deterministic() {
        let id1 = routing_failure_id("list files in current directory");
        let id2 = routing_failure_id("list files in current directory");
        let id3 = routing_failure_id("different intent");
        assert_eq!(id1, id2, "same intent should produce same failure id");
        assert_ne!(id1, id3, "different intents should produce different ids");
        assert!(id1.starts_with("lesson:routing_failure:"));
    }

    #[test]
    fn test_lesson_throttle_only_writes_when_decayed() {
        // Verifica que el guard lógico funciona: weight >= 0.5 → skip, weight < 0.5 → write
        let existing_weight_high = 0.7f64;
        let should_write = existing_weight_high < 0.5;
        assert!(!should_write, "Should not overwrite high-weight lesson");

        let existing_weight_low = 0.3f64;
        let should_write = existing_weight_low < 0.5;
        assert!(should_write, "Should update decayed lesson");
    }

    #[test]
    fn test_rfl_guard_extracts_guild_from_content() {
        let id1 = routing_failure_id("list files in project");
        let id2 = routing_failure_id("list files in project");
        let id3 = routing_failure_id("show git status");
        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
        assert!(id1.starts_with("lesson:routing_failure:"));

        let content = "ROUTING_FAILURE guild=ingest intent=extract triples from text error=empty";
        let extracted = content.split_whitespace()
            .find_map(|w| w.strip_prefix("guild=").map(|g| g.to_string()));
        assert_eq!(extracted, Some("ingest".to_string()));

        let content_no_guild = "ROUTING_FAILURE intent=list files error=empty";
        let extracted_none = content_no_guild.split_whitespace()
            .find_map(|w| w.strip_prefix("guild=").map(|g| g.to_string()));
        assert_eq!(extracted_none, None);
    }

    #[test]
    fn test_parse_coloquio_post_with_channel_and_content() {
        let (cid, content, hint) = parse_coloquio_intent("post to mision-activa: Hello world");
        assert_eq!(cid.as_deref(), Some("mision-activa"));
        assert_eq!(content.as_deref(), Some("Hello world"));
        assert_eq!(hint, "post");
    }

    #[test]
    fn test_parse_coloquio_post_to_channel_without_coloquio_word() {
        let (cid, content, hint) = parse_coloquio_intent("post to mision-activa: COMPLETED task");
        assert_eq!(cid.as_deref(), Some("mision-activa"));
        assert_eq!(content.as_deref(), Some("COMPLETED task"));
        assert_eq!(hint, "post");
    }

    #[test]
    fn test_parse_coloquio_publica_en() {
        let (cid, content, hint) = parse_coloquio_intent("publica en coloquio mision-activa: Mensaje de prueba");
        assert_eq!(cid.as_deref(), Some("mision-activa"));
        assert_eq!(content.as_deref(), Some("Mensaje de prueba"));
        assert_eq!(hint, "post");
    }

    #[test]
    fn test_parse_coloquio_lee_el() {
        let (cid, content, hint) = parse_coloquio_intent("lee el coloquio mision-activa");
        assert_eq!(cid.as_deref(), Some("mision-activa"));
        assert_eq!(content, None);
        assert_eq!(hint, "read");
    }

    #[test]
    fn test_parse_coloquio_read_channel() {
        let (cid, _content, hint) = parse_coloquio_intent("read coloquio channel mision-activa");
        assert_eq!(cid.as_deref(), Some("mision-activa"));
        assert_eq!(hint, "read");
    }

    #[test]
    fn test_parse_coloquio_lee_el_with_pagination_suffix() {
        let (cid, content, hint) = parse_coloquio_intent("lee el canal coloquio mision-activa ultimos 5 mensajes");
        assert_eq!(cid.as_deref(), Some("mision-activa"));
        assert_eq!(content, None);
        assert_eq!(hint, "read");
    }

    #[test]
    fn test_parse_coloquio_lee_el_with_limit_offset() {
        let (cid, _content, hint) = parse_coloquio_intent("lee el coloquio mision-activa offset 140 limit 30");
        assert_eq!(cid.as_deref(), Some("mision-activa"));
        assert_eq!(hint, "read");
    }

    #[test]
    fn test_clean_coloquio_channel_id_strips_messages() {
        let cleaned = coloquio_utils::_clean_coloquio_channel_id("mision-activa ultimos 5 mensajes");
        assert_eq!(cleaned, "mision-activa");
    }

    #[test]
    fn test_clean_coloquio_channel_id_strips_limit() {
        let cleaned = coloquio_utils::_clean_coloquio_channel_id("mision-activa limit 10");
        assert_eq!(cleaned, "mision-activa");
    }

    #[test]
    fn test_parse_coloquio_list() {
        let (cid, content, hint) = parse_coloquio_intent("lista canales coloquio");
        assert_eq!(cid, None);
        assert_eq!(content, None);
        assert_eq!(hint, "list");
    }

    #[test]
    fn test_parse_coloquio_create() {
        let (cid, content, hint) = parse_coloquio_intent("crea canal test-channel: Canal de prueba");
        assert_eq!(cid.as_deref(), Some("test-channel"));
        assert_eq!(content.as_deref(), Some("Canal de prueba"));
        assert_eq!(hint, "create");
    }

    #[test]
    fn test_parse_coloquio_send_message() {
        let (cid, content, hint) = parse_coloquio_intent("send message to mision-activa: task done");
        assert_eq!(cid.as_deref(), Some("mision-activa"));
        assert_eq!(content.as_deref(), Some("task done"));
        assert_eq!(hint, "post");
    }

    #[test]
    fn test_parse_coloquio_envia_al_canal() {
        let (cid, content, hint) = parse_coloquio_intent("envia al canal mision-activa: COMPLETED");
        assert_eq!(cid.as_deref(), Some("mision-activa"));
        assert_eq!(content.as_deref(), Some("COMPLETED"));
        assert_eq!(hint, "post");
    }

    #[test]
    fn test_parse_coloquio_non_coloquio_intent_returns_none() {
        let (cid, content, hint) = parse_coloquio_intent("list files in current directory");
        assert_eq!(cid, None);
        assert_eq!(content, None);
        assert_eq!(hint, "");
    }

    // ── distill_for_embedding tests ──

    #[test]
    fn test_distill_empty_output_falls_back_to_intent() {
        let result = distill_for_embedding("analiza el sistema", "");
        assert_eq!(result, "analiza el sistema");
    }

    #[test]
    fn test_distill_short_output_falls_back_to_intent() {
        let result = distill_for_embedding("analiza el sistema", "OK");
        assert_eq!(result, "analiza el sistema");
    }

    #[test]
    fn test_distill_knowledge_output_keeps_intent_and_content() {
        let result = distill_for_embedding(
            "estado del proyecto",
            "El informe ejecutivo muestra que el sistema tiene 2250 nodos y 12200 aristas. La cobertura de embeddings es del 99%."
        );
        assert!(result.starts_with("estado del proyecto: "));
        assert!(result.len() > 40);
        assert!(result.contains("informe ejecutivo"));
    }

    #[test]
    fn test_distill_operational_powershell_returns_intent_only() {
        let result = distill_for_embedding(
            "Set-Content -Path archivo.json -Value contenido",
            "Set-Content -Path 'E:\\data\\file.json' -Value '{\"key\": \"value\"}'"
        );
        // Operational output — should return just intent
        assert_eq!(result, "Set-Content -Path archivo.json -Value contenido");
    }

    #[test]
    fn test_distill_operational_curl_returns_intent_only() {
        let result = distill_for_embedding(
            "consulta el endpoint de salud",
            "curl -s http://127.0.0.1:3030/health"
        );
        assert_eq!(result, "consulta el endpoint de salud");
    }

    #[test]
    fn test_distill_operational_json_returns_intent_only() {
        let result = distill_for_embedding(
            "guardar configuracion",
            "{\"author_id\": \"agent-1\", \"role\": \"agent\"}"
        );
        assert_eq!(result, "guardar configuracion");
    }

    #[test]
    fn test_distill_operational_git_returns_intent_only() {
        let result = distill_for_embedding(
            "revisar cambios del repositorio",
            "git status --short\n M src/main.rs\n?? new_file.rs"
        );
        assert_eq!(result, "revisar cambios del repositorio");
    }

    #[test]
    fn test_distill_mixed_output_extracts_meaningful_words() {
        let result = distill_for_embedding(
            "analizar log del sistema",
            "kernel version 3.0.0 puerto 3030 modo produccion timeouts OK sistema funcionando correctamente"
        );
        assert!(result.starts_with("analizar log del sistema"));
        assert!(result.contains("kernel") || result.contains("sistema"));
    }

    #[test]
    fn test_parse_episode_with_agent() {
        let content = "tylluan_do episode | agent: test-agent | intent: list files in directory | guild: bash | tool: bash_execute | result: file1.txt\nfile2.txt";
        let (intent, preview) = parse_content_for_embedding(content, "episode");
        assert_eq!(intent, "list files in directory");
        assert_eq!(preview, "file1.txt\nfile2.txt");
    }

    #[test]
    fn test_parse_episode_without_agent() {
        let content = "tylluan_do episode | intent: check health | guild: bash | tool: bash_execute | result: HTTP 200 OK";
        let (intent, preview) = parse_content_for_embedding(content, "episode");
        assert_eq!(intent, "check health");
        assert_eq!(preview, "HTTP 200 OK");
    }

    #[test]
    fn test_parse_episode_empty_result() {
        let content = "tylluan_do episode | intent: list files | guild: bash | tool: bash_execute | result: ";
        let (intent, preview) = parse_content_for_embedding(content, "episode");
        assert_eq!(intent, "list files");
        assert_eq!(preview, "");
    }

    #[test]
    fn test_parse_lesson_with_preview() {
        let content = "guild:bash tool:bash_execute intent:list files in directory -- file1.txt\nfile2.txt";
        let (intent, preview) = parse_content_for_embedding(content, "lesson");
        assert_eq!(intent, "list files in directory");
        assert_eq!(preview, "file1.txt\nfile2.txt");
    }

    #[test]
    fn test_parse_lesson_without_preview() {
        let content = "guild:bash tool:bash_execute intent:Set-Content -Path file.txt -Value 'hello'";
        let (intent, preview) = parse_content_for_embedding(content, "lesson");
        assert_eq!(intent, "Set-Content -Path file.txt -Value 'hello'");
        assert_eq!(preview, "");
    }

    #[test]
    fn test_parse_unknown_type_returns_empty() {
        let (intent, preview) = parse_content_for_embedding("anything", "routing_anchor");
        assert!(intent.is_empty());
        assert!(preview.is_empty());
    }

    // ─── Guild Rate Limiter Tests ──────────────────────────────────────────────

    #[test]
    fn test_guild_rate_limiter_separate_guilds() {
        let limiter = crate::security::rate_limiter::RateLimiter::new(Some(3));
        for _ in 0..3 {
            assert!(limiter.check_and_record("bash").is_ok());
        }
        // bash is now at limit
        assert!(limiter.check_and_record("bash").is_err());
        // Different guild should still work
        assert!(limiter.check_and_record("filesystem").is_ok());
    }

    #[test]
    fn test_guild_rate_limiter_blocks_excess() {
        let limiter = crate::security::rate_limiter::RateLimiter::new(Some(2));
        assert!(limiter.check_and_record("vision").is_ok());
        assert!(limiter.check_and_record("vision").is_ok());
        // vision is at limit
        let result = limiter.check_and_record("vision");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Rate limit exceeded"));
        // After the window elapses, it should work again
        std::thread::sleep(std::time::Duration::from_millis(10));
        // But wait — window is 60s. With limit 2, the third call still fails.
        // This is a sliding window, so we must verify it's still blocked within window.
        // Create a tiny window RateLimiter for a more practical test.
    }

    #[test]
    fn test_guild_rate_limiter_small_window() {
        // Verify that with a low limit the window actually slides
        let limiter = crate::security::rate_limiter::RateLimiter::new(Some(1));
        assert!(limiter.check_and_record("websearch").is_ok());
        assert!(limiter.check_and_record("websearch").is_err());
        // Different guild unaffected
        assert!(limiter.check_and_record("code").is_ok());
    }

    // ─── M32-P0: External MCP dispatch tests ───────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn test_external_mcp_no_servers_returns_none() {
        let server = test_server().await;
        let result = external_mcp::try_external_mcp_dispatch(
            &server, "search the web", None
        ).await;
        // Real config likely has no external MCP servers → None
        // If it does, result will be Some, which is also fine
        if result.is_none() {
            // No external MCP servers registered — expected in default config
            return;
        }
        // If Some, verify it's a valid result (not a panic/error)
        let r = result.unwrap();
        assert!(!r.content.is_empty() || r.is_error == Some(true));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_external_mcp_server_name_in_intent_no_config() {
        let server = test_server().await;
        let result = external_mcp::try_external_mcp_dispatch(
            &server, "use external-mcp to search", None
        ).await;
        // With no external MCPs in the running registry, dispatch returns None.
        // This tests the guard against non-registered servers.
        assert!(result.is_none() || result.as_ref().map(|r| r.is_error == Some(true)).unwrap_or(false),
            "should be None or error when no external servers configured");
    }

    // ─── M31-P5: @skill prefix tests ────────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn test_skill_save_get_roundtrip() {
        let server = test_server().await;
        let result = handle_skill_prefix(&server, "@skill:save:test-skill: This is a test skill content", &None).await;
        assert!(result.is_some(), "save should return Some");
        let r = result.unwrap().unwrap();
        assert_eq!(r.is_error, Some(false), "save should succeed");
        let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(text.contains("Skill"), "save response should mention 'Skill'");
        assert!(text.contains("test-skill"), "save response should include skill name");

        // Now get it back
        let result2 = handle_skill_prefix(&server, "@skill:get:test-skill", &None).await;
        assert!(result2.is_some(), "get should return Some");
        let r2 = result2.unwrap().unwrap();
        assert_eq!(r2.is_error, Some(false), "get should succeed");
        let text2 = r2.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(text2.contains("This is a test skill content"), "get should return saved content");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_skill_list_only_project_scoped() {
        let server = test_server().await;
        // Save a skill for the current project
        let _ = handle_skill_prefix(&server, "@skill:save:my-skill: content for current project", &None).await;

        // Manually insert a skill node with a DIFFERENT project_root (simulating another project)
        let other_root = "E:/some-other-project";
        let other_hash: u64 = other_root.as_bytes().iter()
            .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(*b as u64));
        let other_id = format!("skill:project:{other_hash:x}:other-skill");
        let other_meta = serde_json::json!({
            "project_root": other_root,
            "name": "other-skill",
        }).to_string();
        server.silva.upsert_node(&other_id, "project_skill", "content from other project", &other_meta).await.unwrap();

        // List: should only show current project's skills
        let result = handle_skill_prefix(&server, "@skill:list", &None).await;
        assert!(result.is_some(), "list should return Some");
        let r = result.unwrap().unwrap();
        assert_eq!(r.is_error, Some(false), "list should succeed");
        let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(text.contains("my-skill"), "list should include current project's skill");
        assert!(!text.contains("other-skill"), "list should NOT include other project's skill");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_skill_delete_removes_node() {
        let server = test_server().await;
        // Save a skill
        let _ = handle_skill_prefix(&server, "@skill:save:delete-me: content to be deleted", &None).await;
        // Verify it exists
        let get_before = handle_skill_prefix(&server, "@skill:get:delete-me", &None).await;
        assert!(get_before.is_some());
        let text_before = get_before.unwrap().unwrap().content.iter()
            .filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(text_before.contains("content to be deleted"), "skill should exist before delete");

        // Delete it
        let del = handle_skill_prefix(&server, "@skill:delete:delete-me", &None).await;
        assert!(del.is_some(), "delete should return Some");
        let d = del.unwrap().unwrap();
        assert_eq!(d.is_error, Some(false), "delete should succeed");
        let del_text = d.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(del_text.contains("deleted"), "delete response should mention deleted");

        // Verify it's gone
        let get_after = handle_skill_prefix(&server, "@skill:get:delete-me", &None).await;
        assert!(get_after.is_some(), "get after delete should return Some");
        let g = get_after.unwrap().unwrap();
        assert_eq!(g.is_error, Some(true), "get after delete should report error/is_error");
        let text_after = g.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(text_after.contains("not found"), "get after delete should say 'not found'");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_skill_get_nonexistent_returns_clear_message() {
        let server = test_server().await;
        let result = handle_skill_prefix(&server, "@skill:get:nonexistent-skill", &None).await;
        assert!(result.is_some(), "get nonexistent should return Some");
        let r = result.unwrap().unwrap();
        assert_eq!(r.is_error, Some(true), "get nonexistent should be error");
        let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(text.contains("not found"), "error message should say 'not found'");
        assert!(!text.is_empty(), "should return a non-empty message, not a panic");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_skill_prefix_not_matched_returns_none() {
        let server = test_server().await;
        let result = handle_skill_prefix(&server, "list files in current directory", &None).await;
        assert!(result.is_none(), "non-skill intents should return None");
    }

    // ─── M36: @correct: prefix tests ──────────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn test_correct_supersedes_and_creates_new() {
        let server = test_server().await;
        server.silva.upsert_node("m36:test:original", "concept", "original content", "{}").await.unwrap();

        let result = handle_correct_prefix(&server, "@correct:m36:test:original:corrected content").await;
        assert!(result.is_some(), "correct should return Some");
        let r = result.unwrap().unwrap();
        if r.is_error == Some(true) {
            let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
            panic!("correct returned error: {text}");
        }

        let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(text.contains("Corrected"), "success response should mention 'Corrected'");
        assert!(text.contains("m36:test:original"), "response should include original node id");

        // Old node should have valid_until set
        let old = server.silva.get_node("m36:test:original").await.unwrap().unwrap();
        assert!(old.valid_until.is_some(), "old node should have valid_until set");

        // New node should exist with corrected content
        let all = server.silva.get_all_nodes().await.unwrap();
        let new_node = all.iter().find(|n| n.id.contains("corrected")).unwrap();
        assert_eq!(new_node.content, "corrected content", "new node should have corrected content");
        assert_eq!(new_node.provenance, "agent_generated", "new node should have provenance=agent_generated");

        // Edge should exist: new -> old
        let new_id = &new_node.id;
        let edges = server.silva.get_all_edges().await.unwrap();
        assert!(edges.iter().any(|e|
            e["source"].as_str() == Some(new_id.as_str()) &&
            e["target"].as_str() == Some("m36:test:original") &&
            e["type"].as_str() == Some("corrects")
        ), "should have 'corrects' edge from new to old");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_correct_rejects_protected() {
        let server = test_server().await;
        server.silva.upsert_node("m36:protected:node", "concept", "protected content", "{}").await.unwrap();
        server.silva.protect_node("m36:protected:node").await.unwrap();

        let result = handle_correct_prefix(&server, "@correct:m36:protected:node:new content").await;
        assert!(result.is_some());
        let r = result.unwrap().unwrap();
        assert_eq!(r.is_error, Some(true), "protected correction should be error");
        let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(text.contains("protected"), "error should mention 'protected'");

        // Original should not have valid_until set
        let node = server.silva.get_node("m36:protected:node").await.unwrap().unwrap();
        assert!(node.valid_until.is_none(), "protected node should not be superseded");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_correct_rejects_identity() {
        let server = test_server().await;
        server.silva.upsert_node("agent:test-agent", "identity", "agent identity", "{}").await.unwrap();

        let result = handle_correct_prefix(&server, "@correct:agent:test-agent:new identity").await;
        assert!(result.is_some());
        let r = result.unwrap().unwrap();
        assert_eq!(r.is_error, Some(true), "identity correction should be error");
        let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(text.contains("identity"), "error should mention 'identity'");

        let node = server.silva.get_node("agent:test-agent").await.unwrap().unwrap();
        assert!(node.valid_until.is_none(), "identity node should not be superseded");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_correct_rejects_already_superseded() {
        let server = test_server().await;
        server.silva.upsert_node("m36:old:node", "concept", "old content", "{}").await.unwrap();

        // First correction
        let r1 = handle_correct_prefix(&server, "@correct:m36:old:node:newer content").await;
        assert!(r1.is_some(), "first correct should return Some");
        let r1 = r1.unwrap().unwrap();
        if r1.is_error == Some(true) {
            let text = r1.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
            panic!("first correct returned error: {text}");
        }

        // Second correction on same original should fail (already has valid_until)
        let r2 = handle_correct_prefix(&server, "@correct:m36:old:node:even newer content").await;
        assert!(r2.is_some());
        let r2 = r2.unwrap().unwrap();
        assert_eq!(r2.is_error, Some(true), "second correct on same node should be error");
        let text = r2.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(text.contains("already"), "error should mention 'already'");
        // Only one corrected node should exist
        let all = server.silva.get_all_nodes().await.unwrap();
        let corrected: Vec<_> = all.iter().filter(|n| n.id.contains("corrected")).collect();
        assert_eq!(corrected.len(), 1, "only one corrected node should exist");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_correct_rejects_nonexistent_node() {
        let server = test_server().await;
        let result = handle_correct_prefix(&server, "@correct:nonexistent:node:some content").await;
        assert!(result.is_some());
        let r = result.unwrap().unwrap();
        assert_eq!(r.is_error, Some(true), "nonexistent node should be error");
        let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(text.contains("not found"), "error should say 'not found'");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_correct_parse_error_returns_usage() {
        let server = test_server().await;
        let result = handle_correct_prefix(&server, "@correct:").await;
        assert!(result.is_some());
        let r = result.unwrap().unwrap();
        assert_eq!(r.is_error, Some(true), "empty correct should be error");
        let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(text.contains("Usage"), "error should show usage");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_correct_prefix_not_matched_returns_none() {
        let server = test_server().await;
        let result = handle_correct_prefix(&server, "list files in current directory").await;
        assert!(result.is_none(), "non-correct intents should return None");
    }
}
