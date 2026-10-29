//! D-22c — resolucion y ejecucion de la llamada de `tylluan_do`.
//!
//! Movido tal cual desde `handler_do/mod.rs` (mismo texto, salvo `use` y
//! visibilidad): `ResolvedToolCall`, `resolve_and_prepare_tool_call` (Stage 1) y
//! `execute_guild_call_with_progress` (Stage 2).

use rmcp::model::*;
use tracing::{info, warn};

use super::TylluanServer;
use super::error_prefixes;
use super::external_mcp;
use super::coloquio_utils::{_parse_coloquio_pagination, parse_coloquio_intent};
use super::is_bash_state_intent;
use super::outcome::penalize_lesson_fn;
use super::routing::resolve_guild_name;
use crate::transport::server::utils::{extract_command_from_intent, extract_path_from_intent, extract_url_from_intent};
use crate::registry::proxy::error_result;

/// Resolved tool call returned by `resolve_and_prepare_tool_call`.
pub(super) struct ResolvedToolCall {
    pub(super) guild_name: String,
    pub(super) tool_name: String,
    pub(super) tool_args: serde_json::Value,
    pub(super) routing_trace: Vec<String>,
    pub(super) call_params: CallToolRequestParam,
}

/// Stage 1: resolve guild, apply ACL/rate-limit, select tool, build call params.
/// Returns early-`Err(CallToolResult)` for soft failures (ACL, rate-limit,
/// missing guild/tool). On `Ok` all downstream code can assume the call is
/// safe and prepared.
pub(super) async fn resolve_and_prepare_tool_call(
    server: &TylluanServer,
    routing_intent: &str,
    intent: &str,
    effective_intent: &str,
    guild_hint: &Option<String>,
    agent_id: &Option<String>,
    arguments: &Option<serde_json::Map<String, serde_json::Value>>,
) -> Result<ResolvedToolCall, CallToolResult> {
    let (guild_name, routing_trace) = match resolve_guild_name(server, routing_intent, guild_hint.clone(), agent_id.as_deref()).await {
        Ok((name, trace)) => (name, trace),
        Err(initial_err) => {
            if guild_hint.is_none()
                && let Some(result) = external_mcp::try_external_mcp_dispatch(
                    server, intent, agent_id.as_deref()
                ).await {
                    info!("tylluan_do: dispatched to external MCP server");
                    return Err(result);
                }
            return Err(initial_err);
        }
    };

    if let Err(msg) = server.guild_rate_limiter.check_and_record(&guild_name) {
        warn!("Guild rate limit exceeded for '{}': {}", guild_name, msg);
        return Err(error_result(&format!(
            "{} Rate limit for guild '{guild_name}' exceeded. Try again later.",
            error_prefixes::RATE_LIMITED
        )));
    }

    if let Ok(config_lock) = crate::config::TylluanConfig::load_cached() {
        let cfg = config_lock.read().await;
        let acl = &cfg.security.acl;
        // D-68: the gate runs whenever an HTTP role is present, not only when
        // [security.acl.roles] is non-empty. No role means a direct/internal
        // invocation, not an HTTP caller with the default role.
        if let Some(role) = crate::transport::http::auth::current_acl_role()
            && !crate::transport::http::auth::acl_can_access(&role, &guild_name, acl)
        {
            let msg = format!(
                "ACCESS_DENIED: role '{role}' does not have access to guild '{guild_name}'. \
                 Contact your administrator to update [security.acl] in tylluan.toml."
            );
            warn!("{}", msg);
            return Err(error_result(&msg));
        }
        if !acl.agent_permissions.is_empty() {
            let aid = agent_id.as_deref().unwrap_or("anonymous");
            if aid != "anonymous"
                && let Some(msg) = crate::transport::http::auth::check_agent_id_tool_allowed(aid, "tylluan_do", acl) {
                    return Err(error_result(&msg));
                }
        }
    }

    if let Err(e) = server.registry.write().await.ensure_guild_running(&guild_name).await {
        penalize_lesson_fn(intent, server.silva.clone());
        return Err(error_result(&format!(
            "{} Failed to start guild '{guild_name}': {e}",
            error_prefixes::START_FAILED
        )));
    }

    let mut tool_name = {
        let reg = server.registry.read().await;
        if let Some(guild) = reg.guilds.get(&guild_name) {
            use crate::router::matcher::{tokenize, keyword_score};
            let tokens = tokenize(effective_intent);
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
        penalize_lesson_fn(intent, server.silva.clone());
        return Err(error_result(&format!(
            "{} Guild '{guild_name}' has no tools.",
            error_prefixes::NO_TOOLS
        )));
    }

    let path_hint = extract_path_from_intent(intent);
    let url_hint = extract_url_from_intent(intent).unwrap_or_default();
    let mut tool_args = serde_json::json!({
        "command": intent, "intent": intent, "task": intent,
        "query": intent, "text": intent, "content": intent,
        "prompt": intent, "message": intent, "input": intent,
        "server_url": url_hint, "url": url_hint,
        "timeout_secs": 30, "language": "", "depth": 2, "max_results": 50,
    });
    if let Some(aid) = agent_id {
        tool_args["agent_id"] = serde_json::Value::String(aid.clone());
    }
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

    if (guild_name == "bash" || guild_name == "git")
        && let Some(obj) = tool_args.as_object_mut() {
            // Same root cause as the limit/offset fix above: `tool_args` is
            // built fresh from the intent string and a fixed field set, so a
            // structured `command` argument the caller passed directly to
            // tylluan_do was silently overwritten by the text-extracted one.
            // Prefer the explicit argument; fall back to text parsing.
            let explicit_command = arguments.as_ref()
                .and_then(|a| a.get("command"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty());
            let clean = explicit_command
                .map(str::to_string)
                .unwrap_or_else(|| extract_command_from_intent(intent).to_string());
            obj.insert("command".to_string(), serde_json::Value::String(clean));
        }

    if guild_name == "bash" && tool_name != "bash_execute" && !is_bash_state_intent(intent) {
        let old_tool = tool_name.clone();
        tool_name = "bash_execute".to_string();
        tracing::debug!(
            "bash: overriding tool '{}' -> 'bash_execute' (intent '{}' is a command, not a state op)",
            old_tool, intent
        );
    }

    if guild_name == "coloquio" {
        let (mut channel_id, content_or_name, tool_hint) = parse_coloquio_intent(intent);

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
                    || lower.contains("lee") || lower.contains("leer")
                    || lower.contains("read") || lower.contains("ver ")
                    || lower.contains("mostrar") || lower.contains("lista")
                {
                    "read_channel"
                } else {
                    "post_to_channel"
                }
            },
        }.to_string();
        if let Some(obj) = tool_args.as_object_mut() {
            // Same root cause as the limit/offset fix: prefer structured
            // arguments the caller passed to tylluan_do; fall back to
            // parsing the intent text (keeps natural-language phrasing
            // like "post to coloquio general: ..." working unchanged).
            let explicit_channel = arguments.as_ref()
                .and_then(|a| a.get("channel_id"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty());
            if let Some(cid) = explicit_channel.or(channel_id.as_deref()) {
                obj.insert("channel_id".to_string(), serde_json::Value::String(cid.to_string()));
            }
            let explicit_content = arguments.as_ref()
                .and_then(|a| a.get("content"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty());
            if let Some(cn) = explicit_content.or(content_or_name.as_deref()) {
                obj.insert("content".to_string(), serde_json::Value::String(cn.to_string()));
                obj.insert("message".to_string(), serde_json::Value::String(cn.to_string()));
                obj.insert("intent".to_string(), serde_json::Value::String(cn.to_string()));
            }
            if tool_hint == "read" {
                // REAL BUG FIX: pagination used to come ONLY from regex-parsing
                // the `intent` text (e.g. "offset 150" written in the sentence).
                // Any real, structured `limit`/`offset` argument the caller
                // passed directly to tylluan_do -- exactly what a normal MCP
                // tool call looks like -- was silently discarded, because
                // `tool_args` above is built fresh from the intent string and
                // a fixed field set, never consulting the original `arguments`
                // map. Found live: an offset argument passed structurally to
                // tylluan_do(guild="coloquio", command=<channel>, offset=150)
                // never took effect, always falling back to offset=0. Now
                // prefer an explicit numeric argument when the caller supplied
                // one; only fall back to text-parsing the intent when they
                // didn't (keeps the natural-language "offset 150" / "desde
                // turno 150" phrasing working exactly as before).
                let explicit_limit = arguments.as_ref().and_then(|a| a.get("limit")).and_then(|v| v.as_i64());
                let explicit_offset = arguments.as_ref().and_then(|a| a.get("offset")).and_then(|v| v.as_i64());
                let (text_limit, text_offset) = _parse_coloquio_pagination(intent);
                let limit = explicit_limit.unwrap_or(text_limit);
                let offset = explicit_offset.unwrap_or(text_offset);
                if limit > 0 { obj.insert("limit".to_string(), serde_json::Value::Number(limit.into())); }
                if offset > 0 { obj.insert("offset".to_string(), serde_json::Value::Number(offset.into())); }
                // Same root cause again: `turn` and `full` are new params on
                // read_channel() (guilds/core/coloquio.py) added to fix a real
                // truncation gap (every message body hard-capped at 400 chars
                // with no way to request the full text via MCP). Found live
                // 2026-08-21: turn=141 passed to tylluan_do silently had no
                // effect for the exact same reason offset/limit didn't before
                // this file's earlier fix -- forward them the same way.
                let explicit_turn = arguments.as_ref().and_then(|a| a.get("turn")).and_then(|v| v.as_i64());
                if let Some(t) = explicit_turn.filter(|t| *t > 0) {
                    obj.insert("turn".to_string(), serde_json::Value::Number(t.into()));
                }
                let explicit_full = arguments.as_ref().and_then(|a| a.get("full")).and_then(|v| v.as_bool());
                if explicit_full == Some(true) {
                    obj.insert("full".to_string(), serde_json::Value::Bool(true));
                }
            }
            if !obj.contains_key("author_id") {
                // Same root cause: prefer an explicit `author_id` argument;
                // fall back to "author X" in the intent text, then to the
                // caller's agent_id.
                let explicit_author = arguments.as_ref()
                    .and_then(|a| a.get("author_id"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty());
                if let Some(aid) = explicit_author {
                    obj.insert("author_id".to_string(), serde_json::Value::String(aid.to_string()));
                } else if !intent.to_lowercase().contains("author ")
                    && let Some(aid) = agent_id.as_deref().filter(|a| !a.trim().is_empty()) {
                        obj.insert("author_id".to_string(), serde_json::Value::String(aid.to_string()));
                    }
            }
        }
    }

    // Pre-execution required_args hard-fail gate REMOVED for text-backfilled args
    // (2026-10-08, architectural fix for simple-harness MCP clients -- Hermes/Qwen
    // Desktop could not recover from this; see Coloquio thread of that date).
    //
    // Root cause: the old gate used ITS OWN rigid prefix-matching (coloquio_utils's
    // READ_PREFIXES et al.) to decide whether an arg was "provided" -- far more
    // rigid than the natural-language fallback parsing each guild's own Python
    // tool already has (confirmed: bash.py `command or intent`, coloquio.py
    // `_extract_channel_and_rest`, websearch.py, deep_web_research.py,
    // code_reviewer.py). A client phrasing a request in a way the gate's narrow
    // prefix list didn't recognize got a hard MISSING_ARGS failure even though the
    // guild tool it never reached would have handled it fine.
    //
    // Audited before removal: of the ~30 guilds declaring required_args, every
    // value EXCEPT `path` and `channel_id` is unconditionally backfilled above
    // with the raw intent text (`"command": intent, "query": intent, "prompt":
    // intent, "url": url_hint, ...`) -- never empty, so the old gate was already
    // a near-no-op for those. `channel_id` guilds (coloquio family) return a
    // clean in-band error on empty input (confirmed in coloquio.py), so no gate
    // is needed there either.
    //
    // `path` is different and kept gated below: when `extract_path_from_intent`
    // finds nothing, `path_hint == "."` and the block above DELIBERATELY skips
    // setting "path"/"directory"/"cwd" at all -- which would let several guilds'
    // OWN Python defaults (`path: str = "."`, confirmed in git.py/code.py) silently
    // execute against the kernel's own working directory instead of failing loudly.
    // That silent-wrong-location risk is real (git_add, code_graph, audit, ingest,
    // etc. all require "path"), so it keeps an explicit, narrow refusal.
    if let Some(guild_desc) = server.matcher.available_guilds()
        .iter().find(|g| g.name == guild_name)
        && guild_desc.required_args.iter().any(|a| a == "path")
        && path_hint == "."
    {
        return Err(error_result(&format!(
            "{} Error: guild '{guild_name}' needs a file/directory path, and none could be \
             extracted from the intent text. Provide one explicitly: \
             tylluan_do(intent='...', path='<path>'). Refusing rather than silently \
             operating on the kernel's own working directory.",
            error_prefixes::MISSING_ARGS
        )));
    }

    let call_params = CallToolRequestParam {
        name: tool_name.clone().into(),
        arguments: Some(tool_args.as_object().cloned().unwrap_or_default()),
    };
    info!("🔀 tylluan_do: intent={} → guild='{}' → tool='{}'", crate::logging::redacted("do_prepare", intent), guild_name, tool_name);

    Ok(ResolvedToolCall { guild_name, tool_name, tool_args, routing_trace, call_params })
}

/// Stage 2: execute the guild tool call with a progress ticker and safety timeout.
pub(super) async fn execute_guild_call_with_progress(
    server: &TylluanServer,
    guild_name: &str,
    tool_name: &str,
    call_params: CallToolRequestParam,
    intent: &str,
    agent_id: &Option<String>,
) -> (CallToolResult, u64) {
    let progress_notifier = server.notifier.clone();
    let progress_guild = guild_name.to_string();
    let progress_intent = intent.chars().take(60).collect::<String>();

    let heartbeat_ms = if let Ok(c_lock) = crate::config::TylluanConfig::load_cached() {
        let c = c_lock.read().await;
        c.timeouts.mcp_client_heartbeat_ms
    } else { 8_000 };
    let heartbeat_secs = (heartbeat_ms / 1000).max(1);

    let progress_handle = tokio::spawn(async move {
        let effective_timeout = crate::transport::server::handler_do::guild_effective_timeout(
            &progress_guild, false
        );
        let timeout_secs = effective_timeout / 1000;
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(heartbeat_secs));
        let _ = interval.tick().await;
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

    server.notify("tool_call", serde_json::json!({
        "status": "started", "tool": tool_name,
        "agent_id": agent_id.as_deref().unwrap_or("anonymous"),
        "intent": intent, "ts": chrono::Utc::now().timestamp_millis()
    }));

    let t0 = std::time::Instant::now();
    let effective_timeout = crate::transport::server::handler_do::guild_effective_timeout(
        guild_name, server.low_memory_mode
    );
    let original_timeout = {
        let mut reg = server.registry.write().await;
        let orig = reg.guilds.get(guild_name).and_then(|g| g.tool_timeout);
        if let Some(guild) = reg.guilds.get_mut(guild_name) {
            guild.touch();
            guild.tool_timeout = Some(std::time::Duration::from_millis(effective_timeout));
        }
        orig
    };

    let call_timeout_ms = effective_timeout + 10_000;
    let result = match tokio::time::timeout(
        std::time::Duration::from_millis(call_timeout_ms),
        async {
            let reg = server.registry.read().await;
            if let Some(guild) = reg.guilds.get(guild_name) {
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

    {
        let mut reg = server.registry.write().await;
        if let Some(guild) = reg.guilds.get_mut(guild_name) {
            guild.tool_timeout = original_timeout;
        }
    }
    progress_handle.abort();
    let latency_ms = t0.elapsed().as_millis() as u64;

    if latency_ms > 3000 {
        server.notify("guild_progress", serde_json::json!({
            "type": "guild_progress",
            "guild": guild_name,
            "status": "done",
            "latency_ms": latency_ms,
            "ts": chrono::Utc::now().timestamp_millis()
        }));
    }

    (result, latency_ms)
}
