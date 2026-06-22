use rmcp::{Error as McpError, model::{CallToolResult, Content, JsonObject}};
use crate::registry::proxy::error_result;
use serde_json;
use super::{handler_do, handler_remember, handler_recall, handler_think, handler_graph, handler_ingest, TylluanServer};

impl TylluanServer {
    /// Handle a kernel built-in tool call.
    pub async fn handle_kernel_tool(
        &self,
        name: &str,
        arguments: Option<JsonObject>,
    ) -> Result<CallToolResult, McpError> {
        // Auto-checkin to crash-safe journal
        let agent_id: String = arguments.as_ref()
            .and_then(|a| a.get("agent_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("anonymous")
            .to_string();
        if let Some(ref journal) = self.journal {
            let _ = journal.checkin(&agent_id, &format!("tool:{}", name));
        }

        let result = match name {
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
                    Ok(_) => Ok(CallToolResult { content: vec![Content::text(format!("✅ Guild '{}' is now running.", query))], is_error: Some(false) }),
                    Err(e) => Ok(error_result(&format!("Failed to load guild: {}", e))),
                }
            }
            "unload_guild" => {
                let name = arguments.as_ref().and_then(|a| a.get("guildName")).and_then(|v| v.as_str()).unwrap_or("");
                if name.is_empty() { return Ok(error_result("guildName required.")); }
                if let Some(guild) = self.registry.write().await.guilds.get_mut(name) {
                    if guild.always_on { return Ok(error_result("Always-on guild cannot be unloaded.")); }
                    guild.kill().await.ok();
                    self.notify("notifications/tool/list_changed", serde_json::Value::Null);
                    Ok(CallToolResult { content: vec![Content::text(format!("✅ Guild '{}' unloaded.", name))], is_error: Some(false) })
                } else { Ok(error_result("Unknown guild.")) }
            }
            "doctor_diagnose" => {
                let diag = self.doctor.diagnose().await;
                Ok(CallToolResult { content: vec![Content::text(serde_json::to_string_pretty(&diag).unwrap_or_default())], is_error: Some(false) })
            }
            "list_pending_actions" => {
                let pending = self.pending_approvals.read().await;
                let ids: Vec<String> = pending.keys().cloned().collect();
                Ok(CallToolResult { content: vec![Content::text(format!("Pending approvals: {:?}", ids))], is_error: Some(false) })
            }
            "approve_action" => {
                let request_id = arguments.as_ref().and_then(|a| a.get("requestId")).and_then(|v| v.as_str()).unwrap_or("");
                let approved = arguments.as_ref().and_then(|a| a.get("approved")).and_then(|v| v.as_bool()).unwrap_or(false);
                let mut pending = self.pending_approvals.write().await;
                if let Some(action) = pending.remove(request_id) {
                    let _ = action.tx.send(Ok(CallToolResult { content: vec![Content::text(if approved { "Approved" } else { "Rejected" }.to_string())], is_error: Some(!approved) }));
                    Ok(CallToolResult { content: vec![Content::text("✅ Action resolved".to_string())], is_error: Some(false) })
                } else { Ok(error_result("Action not found.")) }
            }
            "ponder" => {
                let thought = arguments.as_ref().and_then(|a| a.get("thought")).and_then(|v| v.as_str()).unwrap_or("");
                self.thought(thought, 1.0);
                Ok(CallToolResult { content: vec![Content::text("Pondering...")], is_error: Some(false) })
            }
            _ => Err(McpError::invalid_params(format!("Unknown kernel tool: {}", name), None)),
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

        result
    }
}