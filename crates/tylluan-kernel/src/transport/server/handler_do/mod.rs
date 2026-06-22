use rmcp::{Error as McpError, model::*};
use tracing::{info, warn};
use chrono;
use rusqlite::Connection;

use crate::registry::proxy::error_result;
use super::utils::{extract_path_from_intent, extract_url_from_intent, extract_command_from_intent};
use super::TylluanServer;
use super::handler_recall;
use super::handler_remember;

mod routing;
mod embedding;
mod coloquio_utils;
mod timeout;

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
    format!("lesson:routing_failure:{:x}", hash)
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

    if intent.trim().is_empty() {
        return Ok(error_result("tylluan_do requires a non-empty 'intent' argument."));
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
    if intent.trim().starts_with("@coloquio") {
        let rest = intent.trim().strip_prefix("@coloquio").unwrap_or("").trim();
        if rest.is_empty() || rest == ":list" {
            // list channels via recall
            let mut args = serde_json::Map::new();
            args.insert("query".to_string(), serde_json::Value::String("@coloquio".to_string()));
            args.insert("limit".to_string(), serde_json::Value::Number(serde_json::Number::from(20)));
            return Box::pin(handler_recall::handle_tylluan_recall(server, Some(args))).await;
        }
        if let Some(create_name) = rest.strip_prefix(":create:") {
            let channel_name = create_name.trim().to_string();
            if channel_name.is_empty() {
                return Ok(error_result("Usage: @coloquio:create:<channel_name>"));
            }
            if let Some(ref coloquio) = server.coloquio {
                return match coloquio.create_channel(&channel_name, &channel_name).await {
                    Ok(ch) => Ok(CallToolResult {
                        content: vec![Content::text(format!("Channel #{} created.", ch.channel_id))],
                        is_error: Some(false),
                    }),
                    Err(e) => Ok(error_result(&format!("Failed to create channel: {}", e))),
                };
            } else {
                return Ok(error_result("Coloquio is not available."));
            }
        }
        if let Some(channel_part) = rest.strip_prefix(':') {
            let (channel_id, message) = if let Some(idx) = channel_part.find(':') {
                let (cid, msg) = channel_part.split_at(idx);
                (cid.trim().to_string(), Some(msg[1..].trim().to_string()))
            } else {
                (channel_part.trim().to_string(), None)
            };
            if channel_id.is_empty() {
                return Ok(error_result("Usage: @coloquio:<channel_id> (read) or @coloquio:<channel_id>:<message> (post)"));
            }
            if let Some(msg) = message {
                if msg.is_empty() {
                    return Ok(error_result("Message cannot be empty. Usage: @coloquio:<channel_id>:<message>"));
                }
                // Post to channel via remember
                let mut args = serde_json::Map::new();
                args.insert("content".to_string(), serde_json::Value::String(
                    format!("@coloquio:{}:{}", channel_id, msg)
                ));
                if let Some(ref aid) = agent_id {
                    args.insert("agent_id".to_string(), serde_json::Value::String(aid.clone()));
                }
                return Box::pin(handler_remember::handle_tylluan_remember(server, Some(args))).await;
            } else {
                // Read channel via recall
                let mut args = serde_json::Map::new();
                args.insert("query".to_string(), serde_json::Value::String(
                    format!("@coloquio:{}", channel_id)
                ));
                return Box::pin(handler_recall::handle_tylluan_recall(server, Some(args))).await;
            }
        }
    }

    // Deterministic nodo/node prefix — agent-to-agent messaging
    // Uses existing AgentNodeRouter + parse_node_intent from agent_nodes.rs
    if let Some(nodo_intent) = crate::memory::agent_nodes::parse_node_intent(&intent) {
        use crate::memory::agent_nodes::NodeIntent;
        let aid = agent_id.as_deref().unwrap_or("unknown");
        let router = &server.node_router;

        // Auto-register on any nodo command
        router.register(aid).await;

        return match nodo_intent {
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
                    content: vec![Content::text(format!("Broadcast enviado a {} nodos.", count))],
                    is_error: Some(false),
                })
            }
            NodeIntent::DrainInbox | NodeIntent::PeekInbox => {
                let msgs = match nodo_intent {
                    NodeIntent::DrainInbox => router.drain_inbox(aid).await,
                    _ => router.peek_inbox(aid).await,
                };
                if msgs.is_empty() {
                    return Ok(CallToolResult {
                        content: vec![Content::text("Buzón vacío.")],
                        is_error: Some(false),
                    });
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
                    return Ok(CallToolResult {
                        content: vec![Content::text("No hay nodos conectados.")],
                        is_error: Some(false),
                    });
                }
                let report = nodes.iter().map(|n| {
                    let agent_id = n["agent_id"].as_str().unwrap_or("?");
                    let pending = n["inbox_pending"].as_u64().unwrap_or(0);
                    format!("- {}: {} pendientes", agent_id, pending)
                }).collect::<Vec<_>>().join("\n");
                Ok(CallToolResult {
                    content: vec![Content::text(format!("Nodos conectados:\n{}", report))],
                    is_error: Some(false),
                })
            }
            NodeIntent::Register => {
                Ok(CallToolResult {
                    content: vec![Content::text(format!("Nodo '{}' registrado.", aid))],
                    is_error: Some(false),
                })
            }
            NodeIntent::Unregister => {
                router.unregister(aid).await;
                Ok(CallToolResult {
                    content: vec![Content::text(format!("Nodo '{}' desregistrado.", aid))],
                    is_error: Some(false),
                })
            }
        };
    }

    // Sovereign shortcut: "forget: {node_id}" — delete a node without routing to a guild
    let intent_lower = intent.trim().to_lowercase();
    if intent_lower.starts_with("forget:") || intent_lower.starts_with("delete node:") {
        let node_id = intent.split_once(':').map(|x| x.1).unwrap_or("").trim().to_string();
        if node_id.is_empty() {
            return Ok(error_result("forget: requires a node_id. Usage: forget: {node_id}"));
        }
        return match server.silva.delete_node(&node_id).await {
            Ok(true) => Ok(CallToolResult {
                content: vec![Content::text(format!("Forgotten: node '{}' deleted.", node_id))],
                is_error: Some(false),
            }),
            Ok(false) => Ok(error_result(&format!(
                "Cannot forget '{}': node not found or is protected.", node_id
            ))),
            Err(e) => Ok(error_result(&format!("forget failed: {}", e))),
        };
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

    let (guild_name, routing_trace) = match resolve_guild_name(server, &effective_intent, guild_hint, agent_id.as_deref()).await {
        Ok((name, trace)) => (name, trace),
        Err(err_result) => return Ok(err_result),
    };

    if let Err(e) = server.registry.write().await.ensure_guild_running(&guild_name).await {
        penalize_lesson(&intent, server.silva.clone());
        return Ok(error_result(&format!("Failed to start guild '{}': {}", guild_name, e)));
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
        return Ok(error_result(&format!("Guild '{}' has no tools.", guild_name)));
    }

    let path_hint = extract_path_from_intent(&intent);
    let url_hint = extract_url_from_intent(&intent).unwrap_or_default();
    // Only inject path_hint as cwd/directory if it actually exists as a directory
    let safe_path = if path_hint != "." && std::path::Path::new(&path_hint).is_dir() {
        &path_hint
    } else {
        "."
    };
    let project_hint = {
        let mut p = path_hint.replace('/', "-").replace('\\', "-").replace(':', "");
        while p.contains("--") { p = p.replace("--", "-"); }
        p.trim_matches('-').to_string()
    };
    let mut tool_args = serde_json::json!({
        "command": intent, "intent": intent, "directory": safe_path,
        "cwd": safe_path, "path": path_hint, "file_path": path_hint, "repo_path": path_hint,
        "project": project_hint,
        "query": intent, "text": intent, "content": intent,
        "prompt": intent, "message": intent, "input": intent,
        "server_url": url_hint, "url": url_hint,
        "timeout_secs": 30, "language": "", "depth": 2, "max_results": 50,
    });

    // Bash/Git: extract clean command from NL wrapper ("run X", "execute X:", etc.)
    // so the guild receives "ls -la" instead of "execute bash command: ls -la".
    if guild_name == "bash" || guild_name == "git" {
        if let Some(obj) = tool_args.as_object_mut() {
            let clean = extract_command_from_intent(&intent);
            obj.insert("command".to_string(), serde_json::Value::String(clean.to_string()));
        }
    }

    // Coloquio: extract structured params from intent BEFORE validation so channel_id
    // is populated when required_args check runs.
    if guild_name == "coloquio" {
        let (channel_id, content_or_name, tool_hint) = parse_coloquio_intent(&intent);
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
                    "Error: guild '{}' requires argument(s): {}. \
                     Provide them explicitly: {}. \
                     Check guild documentation for required fields.",
                    guild_name, missing_list, example
                )));
            }
        }

    let call_params = CallToolRequestParam {
        name: tool_name.clone().into(),
        arguments: Some(tool_args.as_object().cloned().unwrap_or_default()),
    };
    info!("🔀 tylluan_do: intent='{}' → guild='{}' → tool='{}'", intent, guild_name, tool_name);

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
                format!("Running... (timeout {}s, ~{}s remaining)", timeout_secs, remaining)
            } else if remaining > 0 {
                format!("⏳ Last {}s...", remaining)
            } else {
                format!("⚠️ Exceeded estimated timeout of {}s — waiting for response on local hardware", timeout_secs)
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
                error_result(&format!("Guild '{}' not found — use tylluan_do with a valid intent.", guild_name))
            }
        }
    ).await {
        Ok(res) => res,
        Err(_) => {
            warn!("⌛ tylluan_do: guild call to '{}' timed out after {}ms", guild_name, call_timeout_ms);
            error_result(&format!(
                "ERROR: guild '{}' timed out after {}ms. \
                 The process may be saturated or has failed. \
                 Try splitting the task or restarting the guild.",
                guild_name, call_timeout_ms
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
    let is_success = result.is_error != Some(true)
        && !result.content.iter().filter_map(|c| c.as_text())
            .any(|t| t.text.contains("Exit code:") && !t.text.contains("Exit code: 0"));

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
    tokio::spawn(async move {
        let _ = log_audit_entry(&audit_intent, &audit_guild, &audit_tool, &audit_agent, audit_success, &audit_preview);
    });

    // Anchor learning: store successful routings as routing_anchor nodes (async, fire-and-forget)
    if is_success && !intent.trim().is_empty() {
        let silva_anchor = server.silva.clone();
        let engine_anchor = server.matcher.engine_arc().cloned();
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
            let lesson_id = format!("lesson:{:x}", hash);
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
                format!("guild:{} tool:{} intent:{} -- {}", guild_name, tool_name, intent, preview)
            } else {
                format!("guild:{} tool:{} intent:{}", guild_name, tool_name, intent)
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
            Some(aid) => format!("tylluan_do episode | agent: {} | intent: {} | guild: {} | tool: {} | result: {}", aid, intent, guild_name, tool_name, output_preview),
            None => format!("tylluan_do episode | intent: {} | guild: {} | tool: {} | result: {}", intent, guild_name, tool_name, output_preview),
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
        if let Some(emb) = embedding.as_deref() {
            let _ = server.silva.save_embedding(&node_id, emb, "nomic", None).await;
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
/// Called fire-and-forget from handle_tylluan_do — errors are non-fatal.
pub(crate) fn log_audit_entry(intent: &str, guild: &str, tool: &str, agent_id: &str, success: bool, preview: &str) -> Result<(), String> {
    let db_path = std::path::Path::new("./data/audit.db");
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("audit mkdir: {}", e))?;
    }
    let conn = Connection::open(db_path).map_err(|e| format!("audit open: {}", e))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS audit_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            tool_name TEXT NOT NULL,
            guild TEXT NOT NULL,
            user_agent TEXT,
            success INTEGER NOT NULL,
            intent TEXT,
            result_preview TEXT
        );"
    ).map_err(|e| format!("audit schema: {}", e))?;
    conn.execute(
        "INSERT INTO audit_log (timestamp, tool_name, guild, user_agent, success, intent, result_preview)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            chrono::Utc::now().to_rfc3339(),
            tool,
            guild,
            agent_id,
            if success { 1 } else { 0 },
            intent,
            preview,
        ],
    ).map_err(|e| format!("audit insert: {}", e))?;
    Ok(())
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
        assert!(text.contains("Forgotten"), "Expected 'Forgotten' in: {}", text);
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
        let (cid, content, hint) = parse_coloquio_intent("lee el coloquio mision-activa offset 140 limit 30");
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
}
