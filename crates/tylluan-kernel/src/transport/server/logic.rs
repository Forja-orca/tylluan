use rmcp::{Error as McpError, model::*};
use tracing::{info, warn};
use crate::registry::proxy::error_result;
use crate::registry::tools::RiskLevel;
use crate::security::guard::ExecutionGuard;
use super::types::PendingAction;
use uuid;
use chrono;


impl super::TylluanServer {
    /// Check if a tool requires human approval based on its registered RiskLevel.
    pub async fn check_tool_risk(&self, tool_name: &str) -> RiskLevel {
        if let Some(tool) = Self::kernel_tools().iter().find(|t| t.name == tool_name) {
            return tool.risk.clone();
        }
        if let Some(meta) = crate::registry::tools::TOOL_METADATA.get(tool_name) {
            return meta.risk_level.clone();
        }
        {
            let reg = self.registry.read().await;
            if let Some(guild_name) = reg.find_guild_for_tool(tool_name)
                && let Some(guild) = reg.guilds.get(guild_name)
                    && let Some(tool) = guild.tools.iter().find(|t| t.name == tool_name) {
                        let desc = &tool.description;
                        if desc.contains("[RISK: HIGH]") || desc.contains("approval=\"always\"") {
                            return RiskLevel::High;
                        } else if desc.contains("[RISK: MEDIUM]") {
                            return RiskLevel::Medium;
                        }
                    }
        }
        warn!("⚠️ No risk level found for tool '{}', defaulting to Medium.", tool_name);
        RiskLevel::Medium
    }

    /// Synthesize context from SilvaDB and HybridMemory.
    pub async fn synthesize_context(&self, query: &str) -> String {
        let mut synthesis = String::from("SINTESIS DE CONCURRENCIA SOBERANA (v3.1)\n");
        synthesis.push_str("-------------------------------------------\n");
        let embedding = self.matcher.engine().and_then(|engine| engine.embed(query).ok());

        synthesis.push_str("\nESTATUTO SOBERANO:\n");
        if let Ok(nodes) = self.silva.get_identity_nodes().await {
            for node in nodes { synthesis.push_str(&format!("  > {}\n", node.content)); }
        }

        synthesis.push_str("\nLECCIONES HISTORICAS:\n");
        if let Ok(results) = self.silva.search_hybrid(query, embedding.as_deref(), 5, None).await {
            for (node, score) in results { synthesis.push_str(&format!("  * {} (score: {:.2})\n", node.content, score)); }
        }

        synthesis.push_str("\nTRAYECTORIA ACTUAL:\n");
        if let Ok(lessons) = self.memory.search(query, embedding.as_deref(), 3).await {
            for lesson in lessons { synthesis.push_str(&format!("  ~ {}\n", lesson.content)); }
        }
        synthesis.push_str("\n-------------------------------------------");
        synthesis
    }
    
    /// Handle tool call directly (used by HTTP transport)
    pub async fn handle_call_internal(
        &self, 
        request: CallToolRequestParam, 
        channel: tylluan_common::types::Channel,
        _session_id: &str
    ) -> Result<CallToolResult, McpError> {
        let tool_name = request.name.to_string();
        let risk_level = self.check_tool_risk(&tool_name).await;
        
        let guard_result = ExecutionGuard::check(&tool_name, &channel, &risk_level);
        if !guard_result.allowed {
            return Ok(error_result(&guard_result.reason.unwrap_or_else(|| "Acceso denegado por política de seguridad.".to_string())));
        }

        if guard_result.requires_hitl {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let request_id = uuid::Uuid::new_v4().simple().to_string()[..8].to_string();
            {
                let mut pending = self.pending_approvals.write().await;
                pending.insert(request_id.clone(), PendingAction {
                    name: tool_name.clone(), arguments: request.arguments.clone(), tx,
                });
            }
            self.notify("approval_required", serde_json::json!({ "id": request_id, "tool": tool_name, "arguments": request.arguments, "risk_level": format!("{:?}", risk_level), "ts": chrono::Utc::now().timestamp_millis() }));
            match rx.await {
                Ok(Ok(_)) => info!("✅ [HITL] Approved."),
                Ok(Err(e)) => return Err(e),
                Err(_) => return Ok(error_result("Approval request cancelled.")),
            }
        }

        let is_kernel_tool = Self::kernel_tools().iter().any(|t| t.name == tool_name);
        if is_kernel_tool {
            let result = self.handle_kernel_tool(&tool_name, request.arguments).await?;
            if let Ok(mut h) = self.hormones.lock() {
                h.tick();
                if result.is_error.unwrap_or(false) {
                    h.emit_stress(&tool_name);
                }
                let signals = h.active_signals();
                if !signals.is_empty() {
                    self.notify("hormone_signal", serde_json::json!({
                        "signals": signals,
                        "ts": chrono::Utc::now().timestamp_millis()
                    }));
                }
            }
            return Ok(result);
        }

        let guild_name = self.registry.read().await.find_guild_for_tool(&tool_name).map(|s| s.to_string());
        match guild_name {
            Some(gname) => {
                let breaker_res = self.breaker.check(&gname);
                if breaker_res.open { return Ok(CallToolResult { content: vec![Content::text("[Circuit Breaker OPEN]")], is_error: Some(true) }); }

                let _tool_timeout = self.registry.read().await.guilds.get(&gname).and_then(|g| g.tool_timeout);
                let mut result = error_result("Execution failed");
                let mut reg = self.registry.write().await;
                if let Some(guild) = reg.guilds.get_mut(&gname) {
                    let res = guild.call_tool(request.clone()).await;
                    result = res;
                }

                if result.is_error.unwrap_or(false) { self.breaker.record_error(&gname); }
                else { self.breaker.record_success(&gname); }

                {
                    if let Ok(mut h) = self.hormones.lock() {
                        h.tick();
                        if result.is_error.unwrap_or(false) {
                            h.emit_stress(&tool_name);
                        }
                        let signals = h.active_signals();
                        if !signals.is_empty() {
                            self.notify("hormone_signal", serde_json::json!({
                                "signals": signals,
                                "ts": chrono::Utc::now().timestamp_millis()
                            }));
                        }
                    }
                }

                Ok(result)
            }
            None => Ok(error_result("Tool not found")),
        }
    }
}
