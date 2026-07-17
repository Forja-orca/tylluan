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
                let reg = self.registry.read().await;
                let statuses = reg.status_all();
                let list = statuses.into_iter().map(|s| {
                    serde_json::json!({ "name": s.name, "running": s.running, "tools": s.tools_count })
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
                let pending = self.pending_approvals.read().await;
                let ids: Vec<String> = pending.keys().cloned().collect();
                let grants = crate::security::grants::list_pending().await;
                let mut entries: Vec<serde_json::Value> = ids.iter().map(|id| {
                    serde_json::json!({ "id": id, "origin": "hitl" })
                }).collect();
                entries.extend(grants);
                Ok(CallToolResult {
                    content: vec![Content::text(
                        if entries.is_empty() {
                            "No pending actions.".to_string()
                        } else {
                            serde_json::to_string_pretty(&entries).unwrap_or_default()
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
                let mut resolved = false;
                // First try: pending_approvals (HITL path)
                {
                    let mut pending = self.pending_approvals.write().await;
                    if let Some(action) = pending.remove(request_id) {
                        let _ = action.tx.send(Ok(CallToolResult {
                            content: vec![Content::text(if approved { "Approved" } else { "Rejected" }.to_string())],
                            is_error: Some(!approved),
                        }));
                        resolved = true;
                    }
                }
                if !resolved && approved {
                    // Try grant registry — approve with level
                    let level = match grant_level {
                        "this_session" => crate::security::grants::GrantLevel::ThisSession,
                        "always_for_guild" => crate::security::grants::GrantLevel::AlwaysForGuild,
                        _ => crate::security::grants::GrantLevel::ThisTime,
                    };
                    resolved = crate::security::grants::resolve(request_id, level).await;
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