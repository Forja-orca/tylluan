use rmcp::{Error as McpError, model::{CallToolResult, Content, JsonObject}};
use crate::registry::proxy::error_result;
use serde_json;
use super::{handler_do, handler_remember, handler_recall, handler_think, handler_graph, handler_ingest, TylluanServer};

/// The 5 sovereign tools plus tylluan_ingest -- the single dispatch point
/// M31-P0's hooks apply to, so every MCP client (Claude Desktop, Claude
/// Code, LM Studio, Qwen, ...) gets the same pre/post behavior regardless
/// of which one it is, rather than per-client logic bolted on separately.
const SOVEREIGN_TOOLS: [&str; 6] = [
    "tylluan_do", "tylluan_remember", "tylluan_recall",
    "tylluan_think", "tylluan_graph", "tylluan_ingest",
];

impl TylluanServer {
    /// Handle a kernel built-in tool call.
    pub async fn handle_kernel_tool(
        &self,
        name: &str,
        mut arguments: Option<JsonObject>,
    ) -> Result<CallToolResult, McpError> {
        // Auto-checkin to crash-safe journal
        let agent_id: String = arguments.as_ref()
            .and_then(|a| a.get("agent_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("anonymous")
            .to_string();
        if let Some(ref journal) = self.journal {
            let _ = journal.checkin(&agent_id, &format!("tool:{name}"));
        }

        // M31-P1: Enforce agent_id matches the bearer token binding (if any)
        if let Some(bound_agent) = crate::transport::http::auth::current_bound_agent_id()
            && agent_id != bound_agent && agent_id != "anonymous" {
                return Ok(error_result(&format!(
                    "ACCESS_DENIED: this token is bound to agent '{bound_agent}', \
                     cannot impersonate agent '{agent_id}'. Set agent_id='{bound_agent}' in your tool call."
                )));
            }
        // M31-P1: Enforce per-agent tool permissions
        if let Ok(config_lock) = crate::config::TylluanConfig::load_cached() {
            let cfg = config_lock.read().await;
            let acl = &cfg.security.acl;
            if !acl.agent_permissions.is_empty() && agent_id != "anonymous"
                && let Some(msg) = crate::transport::http::auth::check_agent_id_tool_allowed(&agent_id, name, acl) {
                    return Ok(error_result(&msg));
                }
        }

        // Identity auto-bootstrap: an agent should never have to hand-register
        // itself before it "exists" to the kernel. First contact from a real,
        // already-authenticated agent_id (past the impersonation/ACL checks
        // above) creates a minimal identity node so whoami never comes back
        // empty; register_identity later just fills it in with real biography.
        if agent_id != "anonymous" && name != "whoami" && name != "register_identity" {
            let silva = self.silva.clone();
            let bootstrap_id = agent_id.clone();
            tokio::spawn(async move {
                let identity_mgr = crate::memory::identity::IdentityManager::new(silva);
                if !identity_mgr.has_identity(&bootstrap_id).await {
                    let identity = crate::memory::identity::AgentIdentity::new(
                        &bootstrap_id, &bootstrap_id, "unregistered",
                        "Auto-bootstrapped on first contact -- call register_identity to fill in your real biography.",
                    );
                    let _ = identity_mgr.register_agent(&identity).await;
                }
            });
        }

        let is_sovereign_tool = SOVEREIGN_TOOLS.contains(&name);
        let hook_rules: Vec<crate::security::hooks::HookRule> = if is_sovereign_tool {
            match crate::config::TylluanConfig::load_cached() {
                Ok(cfg) => cfg.read().await.hooks.clone(),
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };

        if is_sovereign_tool && !hook_rules.is_empty()
            && let Some(ref mut args) = arguments
                && let crate::security::hooks::PreHookOutcome::Deny(msg) =
                    crate::security::hooks::run_pre_hooks(&hook_rules, name, args)
                {
                    return Ok(error_result(&msg));
                }

        let mut result = match name {
            "tylluan_do" => handler_do::handle_tylluan_do(self, arguments).await,
            "tylluan_remember" => handler_remember::handle_tylluan_remember(self, arguments).await,
            "tylluan_recall" => handler_recall::handle_tylluan_recall(self, arguments).await,
            "tylluan_think" => handler_think::handle_tylluan_think(self, arguments).await,
            "tylluan_graph" => handler_graph::handle_tylluan_graph(self, arguments).await,
            "tylluan_ingest" => handler_ingest::handle_tylluan_ingest(self, arguments).await,

            "health" => {
                let reg = self.registry.read().await;
                let statuses = reg.status_all();
                let mut report = String::from("TylluanNexus Kernel Health:\n");
                for s in statuses {
                    report.push_str(&format!("- {}: {}\n", s.name, if s.running { "OK" } else { "STOPPED" }));
                }
                Ok(CallToolResult { content: vec![Content::text(report)], is_error: Some(false) })
            }
            "list_available_guilds" => {
                // M40-P1: self-documenting guild contracts. Before this, a caller had no
                // way to discover a guild's required_args ahead of time -- e.g. the
                // 'audit' guild rejects a tylluan_do call for missing 'path' with no
                // hint of where that requirement was documented (found live, 2026-08-09).
                // required_args and capabilities already existed on GuildDescriptor
                // (catalog.rs) but were never surfaced here.
                let reg = self.registry.read().await;
                let statuses = reg.status_all();
                let descriptors = self.matcher.available_guilds();
                let list = statuses.into_iter().map(|s| {
                    let desc = descriptors.iter().find(|d| d.name == s.name);
                    serde_json::json!({
                        "name": s.name,
                        "running": s.running,
                        "tools": s.tools_count,
                        "required_args": desc.map(|d| &d.required_args).cloned().unwrap_or_default(),
                        "capabilities": desc.and_then(|d| d.capabilities.clone()),
                        "permissions": desc.map(|d| &d.permissions).cloned().unwrap_or_default(),
                        "estimated_cost": desc.and_then(|d| d.estimated_cost.clone()),
                        "side_effects": desc.map(|d| &d.side_effects).cloned().unwrap_or_default(),
                        "examples": desc.map(|d| &d.examples).cloned().unwrap_or_default(),
                        "preconditions": desc.map(|d| &d.preconditions).cloned().unwrap_or_default(),
                        "verification": desc.and_then(|d| d.verification.clone()),
                        "rollback": desc.and_then(|d| d.rollback.clone()),
                    })
                }).collect::<Vec<_>>();
                Ok(CallToolResult { content: vec![Content::text(serde_json::to_string_pretty(&list).unwrap_or_default())], is_error: Some(false) })
            }
            "request_guild" => {
                let query = arguments.as_ref().and_then(|a| a.get("query")).and_then(|v| v.as_str()).unwrap_or("");
                if query.is_empty() { return Ok(error_result("Query required.")); }
                match self.registry.write().await.ensure_guild_running(query).await {
                    Ok(_) => Ok(CallToolResult { content: vec![Content::text(format!("✅ Guild '{query}' is now running."))], is_error: Some(false) }),
                    Err(e) => Ok(error_result(&format!("Failed to load guild: {e}"))),
                }
            }
            "unload_guild" => {
                let name = arguments.as_ref().and_then(|a| a.get("guildName")).and_then(|v| v.as_str()).unwrap_or("");
                if name.is_empty() { return Ok(error_result("guildName required.")); }
                if let Some(guild) = self.registry.write().await.guilds.get_mut(name) {
                    if guild.always_on { return Ok(error_result("Always-on guild cannot be unloaded.")); }
                    guild.kill().await.ok();
                    self.notify("notifications/tool/list_changed", serde_json::Value::Null);
                    Ok(CallToolResult { content: vec![Content::text(format!("✅ Guild '{name}' unloaded."))], is_error: Some(false) })
                } else { Ok(error_result("Unknown guild.")) }
            }
            "doctor_diagnose" => {
                let diag = self.doctor.diagnose().await;
                Ok(CallToolResult { content: vec![Content::text(serde_json::to_string_pretty(&diag).unwrap_or_default())], is_error: Some(false) })
            }
            "doctor_repair" => {
                let target = arguments.as_ref().and_then(|a| a.get("target")).and_then(|v| v.as_str()).unwrap_or("");
                if target.is_empty() {
                    return Ok(error_result("doctor_repair requires a 'target' argument. Targets: 'guild', 'storage', 'benchmark'."));
                }
                let name = arguments.as_ref().and_then(|a| a.get("name")).and_then(|v| v.as_str());
                match self.doctor.repair(target, name).await {
                    Ok(msg) => Ok(CallToolResult { content: vec![Content::text(msg)], is_error: Some(false) }),
                    Err(e) => Ok(error_result(&e)),
                }
            }
            "list_pending_actions" => {
                let grants = crate::security::grants::list_pending().await;
                Ok(CallToolResult {
                    content: vec![Content::text(
                        if grants.is_empty() {
                            "No pending actions.".to_string()
                        } else {
                            serde_json::to_string_pretty(&grants).unwrap_or_default()
                        }
                    )],
                    is_error: Some(false),
                })
            }
            "approve_action" => {
                let request_id = arguments.as_ref().and_then(|a| a.get("requestId")).and_then(|v| v.as_str()).unwrap_or("");
                let approved = arguments.as_ref().and_then(|a| a.get("approved")).and_then(|v| v.as_bool()).unwrap_or(false);
                let grant_level = arguments.as_ref()
                    .and_then(|a| a.get("grant_level"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("this_time");
                let mut resolved;
                if approved {
                    let level = match grant_level {
                        "this_session" => crate::security::grants::GrantLevel::ThisSession,
                        "always_for_guild" => crate::security::grants::GrantLevel::AlwaysForGuild,
                        _ => crate::security::grants::GrantLevel::ThisTime,
                    };
                    resolved = crate::security::grants::resolve(request_id, level).await;
                } else {
                    resolved = crate::security::grants::remove(request_id).await;
                }
                // M31-P2: Try plan store — execute the stored plan directly
                if !resolved && approved
                    && let Some(plan) = crate::security::grants::get_plan(request_id).await
                {
                    let call_params = rmcp::model::CallToolRequestParam {
                        name: plan.tool.clone().into(),
                        arguments: Some(plan.args.clone()),
                    };
                    let guild_name = plan.guild.clone();
                    let result = {
                        let reg = self.registry.read().await;
                        match reg.guilds.get(&guild_name) {
                            Some(guild) => guild.call_tool_readonly(call_params).await,
                            None => error_result(&format!("Plan guild '{guild_name}' not found")),
                        }
                    };
                    crate::security::grants::remove_plan(request_id).await;
                    if !plan.agent_id.is_empty() && plan.agent_id != "anonymous" {
                        let _ = self.journal.as_ref().map(|j| j.checkin(&plan.agent_id, "plan:executed"));
                    }
                    return Ok(result);
                }
                if !resolved && !approved {
                    // Reject by removing the grant without sending (receiver gets Canceled)
                    resolved = crate::security::grants::remove(request_id).await;
                }
                if resolved {
                    Ok(CallToolResult { content: vec![Content::text("✅ Action resolved".to_string())], is_error: Some(false) })
                } else {
                    Ok(error_result("Action not found."))
                }
            }
            "ponder" => {
                let thought = arguments.as_ref().and_then(|a| a.get("thought")).and_then(|v| v.as_str()).unwrap_or("");
                self.thought(thought, 1.0);
                Ok(CallToolResult { content: vec![Content::text("Pondering...")], is_error: Some(false) })
            }
            "agent_get_persona" => {
                let profile = if let Some(ref profiles) = self.agent_profiles {
                    profiles.lock().ok().and_then(|store| store.get_profile(&agent_id).ok()).flatten()
                } else { None };
                let result = match profile {
                    Some(p) => serde_json::json!({ "agent_id": p.agent_id, "persona": p.persona, "preferences": p.preferences }),
                    None    => serde_json::json!({ "agent_id": agent_id, "persona": "", "preferences": {} }),
                };
                Ok(CallToolResult { content: vec![Content::text(serde_json::to_string_pretty(&result).unwrap_or_default())], is_error: Some(false) })
            }
            "agent_set_persona" => {
                let persona = arguments.as_ref().and_then(|a| a.get("persona")).and_then(|v| v.as_str()).unwrap_or("");
                let preferences = arguments.as_ref().and_then(|a| a.get("preferences"));
                if let Some(ref profiles) = self.agent_profiles
                    && let Ok(store) = profiles.lock() {
                        let _ = store.upsert_activity(&agent_id, "kernel", true, Some("set_persona"));
                        store.set_persona(&agent_id, persona).ok();
                        if let Some(prefs) = preferences {
                            store.set_preferences(&agent_id, prefs).ok();
                        }
                    }
                Ok(CallToolResult { content: vec![Content::text("✅ Persona updated")], is_error: Some(false) })
            }
            "whoami" => {
                let target_id = arguments.as_ref()
                    .and_then(|a| a.get("agent_id"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or(&agent_id);

                let identity_mgr = crate::memory::identity::IdentityManager::new(self.silva.clone());
                let bio_context = identity_mgr.get_agent_context(target_id).await;
                // Self-documenting: if what's registered is still the auto-bootstrap
                // placeholder (or nothing at all), tell the agent exactly how to fix
                // it -- so the human never has to relay the calling convention by hand.
                let raw_identity = identity_mgr.get_identity(target_id).await;
                let needs_real_bio = raw_identity.as_ref().map(|i| i.role == "unregistered").unwrap_or(true);
                let register_hint = if needs_real_bio {
                    Some(serde_json::json!({
                        "message": "Your biography is a placeholder. Call register_identity with real values to fix it.",
                        "example_call": {
                            "name": "register_identity",
                            "arguments": {
                                "agent_id": target_id,
                                "human_name": "<your display name>",
                                "role": "<your role, e.g. 'Builder Backend'>",
                                "purpose": "<your current focus, one sentence>",
                                "philosophy": "<optional>"
                            }
                        },
                        "note": "These must be separate JSON arguments in the tool call -- text embedded inside `intent` is not parsed.",
                    }))
                } else { None };

                let profile = if let Some(ref profiles) = self.agent_profiles {
                    profiles.lock().ok().and_then(|store| store.get_profile(target_id).ok()).flatten()
                } else { None };

                let now = chrono::Utc::now();
                let requested_tz = arguments.as_ref()
                    .and_then(|a| a.get("timezone"))
                    .and_then(|v| v.as_str());
                let local = requested_tz.and_then(|tz_name| {
                    tz_name.parse::<chrono_tz::Tz>().ok().map(|tz| {
                        let local_time = now.with_timezone(&tz);
                        serde_json::json!({
                            "timezone": tz_name,
                            "local_time": local_time.to_rfc3339(),
                            "weekday": local_time.format("%A").to_string(),
                        })
                    })
                });
                let tz_error = requested_tz
                    .filter(|tz_name| tz_name.parse::<chrono_tz::Tz>().is_err())
                    .map(|tz_name| format!("Unknown IANA timezone '{tz_name}' -- use names like 'Asia/Tokyo', 'Europe/Madrid', 'America/New_York'."));

                // Last in-progress task: JournalDb.recover() already tracks this on every
                // tool call, but until now it only existed as a REST endpoint no agent
                // ever called. An agent reconnecting should get its own "what was I doing"
                // back without a separate round trip.
                let last_task = self.journal.as_ref().and_then(|j| j.recover(target_id).ok().flatten());

                let result = serde_json::json!({
                    "agent_id": target_id,
                    "registered": bio_context.is_some(),
                    "biography": bio_context,
                    "activity": profile.map(|p| serde_json::json!({
                        "first_seen": p.first_seen,
                        "total_calls": p.total_calls,
                        "reputation_score": p.reputation_score,
                        "role": p.role,
                        "persona": p.persona,
                    })),
                    "last_task": last_task.map(|t| {
                        let (stale, stale_secs) = crate::transport::http::api_v1::api_journal::is_stale(t.updated_at);
                        serde_json::json!({
                            "task": t.task,
                            "updated_at_unix": t.updated_at,
                            "stale": stale,
                            "stale_secs": stale_secs,
                        })
                    }),
                    "now": {
                        "utc": now.to_rfc3339(),
                        "unix_epoch": now.timestamp(),
                        "weekday": now.format("%A").to_string(),
                        "local": local,
                        "timezone_error": tz_error,
                    },
                    "register_identity_hint": register_hint,
                    "world_grounding_hint": "This kernel deliberately does not auto-fetch world news at connect time (no LLM/network call in the critical path, by design). For current events or real-world context, call tylluan_do with guild='websearch' (request_guild first if not already loaded).",
                });
                Ok(CallToolResult { content: vec![Content::text(serde_json::to_string_pretty(&result).unwrap_or_default())], is_error: Some(false) })
            }
            "register_identity" => {
                let target_id = arguments.as_ref()
                    .and_then(|a| a.get("agent_id"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or(&agent_id);
                let human_name_arg = arguments.as_ref().and_then(|a| a.get("human_name")).and_then(|v| v.as_str());
                let role_arg = arguments.as_ref().and_then(|a| a.get("role")).and_then(|v| v.as_str());
                let purpose_arg = arguments.as_ref().and_then(|a| a.get("purpose")).and_then(|v| v.as_str());
                // If none of the biographical fields were passed as real JSON arguments,
                // this is almost certainly a caller that put the info inside the `intent`
                // text instead (e.g. via tylluan_do) -- reject with the exact shape
                // needed instead of silently persisting a placeholder identity.
                if human_name_arg.is_none() && role_arg.is_none() && purpose_arg.is_none() {
                    return Ok(error_result(&format!(
                        "register_identity needs human_name, role, and purpose as separate JSON arguments -- \
                         they are not parsed out of the `intent` text. Call it like: \
                         {{\"name\": \"register_identity\", \"arguments\": {{\"agent_id\": \"{target_id}\", \
                         \"human_name\": \"<your display name>\", \"role\": \"<your role>\", \
                         \"purpose\": \"<your current focus, one sentence>\"}}}}"
                    )));
                }
                let human_name = human_name_arg.unwrap_or(target_id);
                let role = role_arg.unwrap_or("Assistant");
                let purpose = purpose_arg.unwrap_or("General assistance");
                let philosophy = arguments.as_ref().and_then(|a| a.get("philosophy")).and_then(|v| v.as_str());

                let identity_mgr = crate::memory::identity::IdentityManager::new(self.silva.clone());
                let mut identity = crate::memory::identity::AgentIdentity::new(target_id, human_name, role, purpose);
                // Preserve the original born_at on re-registration; only a first-time
                // registration should set "active since" to today.
                if let Some(existing) = identity_mgr.get_identity(target_id).await {
                    identity.born_at = existing.born_at;
                }
                identity.philosophy = philosophy.map(|s| s.to_string());

                match identity_mgr.register_agent(&identity).await {
                    Ok(()) => Ok(CallToolResult { content: vec![Content::text(format!("✅ Identity registered for '{target_id}' — persisted, survives kernel restarts."))], is_error: Some(false) }),
                    Err(e) => Ok(error_result(&format!("Failed to register identity: {e}"))),
                }
            }
            _ => Err(McpError::invalid_params(format!("Unknown kernel tool: {name}"), None)),
        };

        // Audit log: record every sovereign tool call fire-and-forget (tylluan_do has its own audit)
        if name != "tylluan_do" {
            let audit_tool = name.to_string();
            let audit_agent = agent_id.to_string();
            let audit_success = result.as_ref().map(|r| !r.is_error.unwrap_or(false)).unwrap_or(false);
            tokio::spawn(async move {
                let _ = handler_do::log_audit_entry("", "kernel", &audit_tool, &audit_agent, audit_success, "");
            });
        }

        if is_sovereign_tool && !hook_rules.is_empty()
            && let Ok(ref mut res) = result {
                let mut texts: Vec<String> = res.content.iter()
                    .filter_map(|c| c.as_text().map(|t| t.text.clone()))
                    .collect();
                if !texts.is_empty() {
                    crate::security::hooks::run_post_hooks(&hook_rules, name, &mut texts);
                    res.content = texts.into_iter().map(Content::text).collect();
                }
            }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use crate::memory::silva::SilvaDB;

    async fn test_server() -> TylluanServer {
        let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
        crate::transport::server::handler_do::base_test_server(silva).await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_doctor_diagnose_returns_report() {
        let server = test_server().await;
        let result = server.handle_kernel_tool("doctor_diagnose", None).await;
        assert!(result.is_ok(), "diagnose should succeed");
        let r = result.unwrap();
        assert_eq!(r.is_error, Some(false));
        let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(text.contains("status"), "should contain 'status' field");
        assert!(text.contains("guilds"), "should contain 'guilds' field");
        assert!(text.contains("storage"), "should contain 'storage' field");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_available_guilds_exposes_required_args() {
        // M40-P1: a caller must be able to discover a guild's required_args before
        // calling it, not just find out via a runtime error. Regression guard for
        // the real gap found 2026-08-09: 'audit' required 'path' with zero schema
        // hint, and 'coloquio_digest'/'whats_new' have the same pattern for
        // 'channel_id' -- the exact bug that already bit an agent live in Coloquio.
        let server = test_server().await;
        let result = server.handle_kernel_tool("list_available_guilds", None).await;
        assert!(result.is_ok());
        let r = result.unwrap();
        assert_eq!(r.is_error, Some(false));
        let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        let list: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
        let entries = list.as_array().expect("list is an array");
        let audit = entries.iter().find(|e| e["name"] == "audit");
        if let Some(audit) = audit {
            let required = audit["required_args"].as_array().expect("required_args is an array");
            assert!(
                required.iter().any(|v| v == "path"),
                "audit guild must expose 'path' as a required_args entry, got: {required:?}"
            );
        }
        // Every entry must at least carry the required_args key, even when empty,
        // so a caller never has to guess whether the field is simply absent.
        for entry in entries {
            assert!(entry.get("required_args").is_some(), "every guild entry must expose required_args");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_doctor_repair_benchmark_returns_results() {
        let server = test_server().await;
        let args = Some(serde_json::json!({"target": "benchmark"}).as_object().unwrap().clone());
        let result = server.handle_kernel_tool("doctor_repair", args).await;
        assert!(result.is_ok(), "repair benchmark should succeed");
        let r = result.unwrap();
        assert_eq!(r.is_error, Some(false));
        let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(text.contains("Benchmark"), "should contain benchmark results");
        assert!(text.contains("String alloc"), "should contain string benchmark");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_doctor_repair_empty_target_returns_error() {
        let server = test_server().await;
        let args = Some(serde_json::json!({"target": ""}).as_object().unwrap().clone());
        let result = server.handle_kernel_tool("doctor_repair", args).await;
        assert!(result.is_ok());
        let r = result.unwrap();
        assert_eq!(r.is_error, Some(true), "empty target should error");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_doctor_repair_storage_returns_result() {
        let server = test_server().await;
        let args = Some(serde_json::json!({"target": "storage"}).as_object().unwrap().clone());
        let result = server.handle_kernel_tool("doctor_repair", args).await;
        assert!(result.is_ok(), "repair storage should succeed");
        let r = result.unwrap();
        assert_eq!(r.is_error, Some(false));
        let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(text.contains("✅"), "should show success indicator");
    }

    // ── M34-P0: Provenance Tests ──────────────────────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn test_provenance_default_is_unverified() {
        let server = test_server().await;
        server.silva.upsert_node("p_test:default", "test", "default provenance", "{}").await.unwrap();
        let node = server.silva.get_node("p_test:default").await.unwrap().unwrap();
        assert_eq!(node.provenance, "unverified", "nodes written via upsert_node should default to 'unverified'");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_provenance_agent_generated_roundtrip() {
        let server = test_server().await;
        server.silva.upsert_node_with_provenance("p_test:agent", "test", "agent memory", "{}", "agent_generated").await.unwrap();
        let node = server.silva.get_node("p_test:agent").await.unwrap().unwrap();
        assert_eq!(node.provenance, "agent_generated");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_provenance_federation_peer_roundtrip() {
        let server = test_server().await;
        server.silva.upsert_node_with_provenance("p_test:fed", "test", "federated content", "{}", "federation_peer").await.unwrap();
        let node = server.silva.get_node("p_test:fed").await.unwrap().unwrap();
        assert_eq!(node.provenance, "federation_peer");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_provenance_fts_search_preserves_provenance() {
        let server = test_server().await;
        server.silva.upsert_node_with_provenance("p_test:fts", "lesson", "unique FTS token for provenance test 987654", "{}", "agent_generated").await.unwrap();
        let results = server.silva.search("987654", 5, None).await.unwrap();
        let found = results.into_iter().find(|n| n.id == "p_test:fts");
        assert!(found.is_some(), "FTS should find the node");
        assert_eq!(found.unwrap().provenance, "agent_generated", "FTS search should preserve provenance");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_provenance_migration_default_is_unverified() {
        let server = test_server().await;
        // Nodes inserted via standard upsert_node (legacy path) get 'unverified'
        server.silva.upsert_node("p_test:migrated", "test", "old node", "{}").await.unwrap();
        let node = server.silva.get_node("p_test:migrated").await.unwrap().unwrap();
        assert_eq!(node.provenance, "unverified");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_doctor_repair_unknown_target_returns_error() {
        let server = test_server().await;
        let args = Some(serde_json::json!({"target": "nonexistent"}).as_object().unwrap().clone());
        let result = server.handle_kernel_tool("doctor_repair", args).await;
        assert!(result.is_ok());
        let r = result.unwrap();
        assert_eq!(r.is_error, Some(true), "unknown target should error");
    }
}