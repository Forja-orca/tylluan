use rmcp::{Error as McpError, model::{CallToolResult, Content, JsonObject}};
use crate::registry::proxy::error_result;
use serde_json;
use super::{handler_do, handler_remember, handler_recall, handler_think, handler_graph, handler_ingest, handler_decide, TylluanServer};

/// The 6 sovereign tools plus tylluan_ingest -- the single dispatch point
/// M31-P0's hooks apply to, so every MCP client (Claude Desktop, Claude
/// Code, LM Studio, Qwen, ...) gets the same pre/post behavior regardless
/// of which one it is, rather than per-client logic bolted on separately.
const SOVEREIGN_TOOLS: [&str; 7] = [
    "tylluan_do", "tylluan_remember", "tylluan_recall",
    "tylluan_think", "tylluan_graph", "tylluan_decide", "tylluan_ingest",
];

impl TylluanServer {
    /// Handle a kernel built-in tool call.
    pub async fn handle_kernel_tool(
        &self,
        name: &str,
        mut arguments: Option<JsonObject>,
    ) -> Result<CallToolResult, McpError> {
        // Auto-checkin to crash-safe journal
        let mut agent_id: String = arguments.as_ref()
            .and_then(|a| a.get("agent_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("anonymous")
            .to_string();

        // D5033b: identity of a tool call comes from the credential when the
        // transport carries one. `current_bound_agent_id()` is the task-local the
        // HTTP bearer middleware installs from `[security.acl.token_agent_bindings]`
        // (see `bearer_auth_middleware` / `resolve_agent_id_for_token` in
        // `transport/http/auth.rs`). A caller that declares no `agent_id`
        // landed on `"anonymous"`, and BOTH gates below explicitly except
        // `"anonymous"` — so a bound credential gained freedom by staying silent:
        // it skipped the impersonation guard and the per-agent tool permissions of
        // its own agent. When the transport knows who is calling, the credential
        // decides; the payload can only narrow, never rename. A path with no
        // middleware (stdio, internal) has no binding and is unchanged — that
        // boundary is pinned by `d5033b_sin_binding_del_middleware_gana_el_payload`.
        if agent_id == "anonymous"
            && let Some(bound) = crate::transport::http::auth::current_bound_agent_id()
        {
            agent_id = bound;
            // Los handlers vuelven a leer `agent_id` del payload (`whoami` y
            // `register_identity` en este mismo fichero, `handler_recall`,
            // `handler_remember`, `handler_graph`, `handler_do/prepare.rs`). Sin
            // escribir la identidad resuelta de vuelta en los argumentos, la
            // resolución de arriba sería decorativa para ellos.
            arguments
                .get_or_insert_with(JsonObject::new)
                .insert("agent_id".to_string(), serde_json::Value::String(agent_id.clone()));
        }

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
            "tylluan_decide" => handler_decide::handle_tylluan_decide(self, arguments).await,
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
                        "subtools": desc.map(|d| &d.subtools).cloned().unwrap_or_default(),
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
                {
                    let mut reg = self.registry.write().await;
                    match reg.guilds.get_mut(name) {
                        None => return Ok(error_result("Unknown guild.")),
                        Some(guild) => {
                            if guild.always_on { return Ok(error_result("Always-on guild cannot be unloaded.")); }
                            guild.kill().await.ok();
                        }
                    }
                }
                // Leave the registry consistent: kill() cleared the guild's
                // tools, so drop the stale entries from the shared tool index.
                // ensure_guild_running rebuilds it again on the next respawn.
                self.registry.write().await.rebuild_tool_index();
                // MCP 2025-06-18 (server/tools, "List Changed Notification"): the method is
                // `notifications/tools/list_changed`, plural. The singular form was ignored by clients.
                self.notify("notifications/tools/list_changed", serde_json::Value::Null);
                Ok(CallToolResult { content: vec![Content::text(format!("✅ Guild '{name}' unloaded."))], is_error: Some(false) })
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
                // Jose 2026-11-15: tercera salida. `decision="hold"` deja el plan
                // PENDIENTE marcado con el motivo: ni se aprueba ni se rechaza.
                // Aprobar deja de ser la única forma de cerrar la conversación.
                let decision = arguments
                    .as_ref()
                    .and_then(|a| a.get("decision"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if decision.eq_ignore_ascii_case("hold") {
                    let reason = arguments
                        .as_ref()
                        .and_then(|a| a.get("reason"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if reason.trim().is_empty() {
                        return Ok(error_result(
                            "hold requires a non-empty 'reason': what is missing to judge this action?",
                        ));
                    }
                    let held = crate::security::grants::hold_plan(request_id, reason).await;
                    return Ok(CallToolResult {
                        content: vec![Content::text(
                            serde_json::json!({
                                "status": if held { "held" } else { "not_found" },
                                "request_id": request_id,
                                "reason": reason.trim(),
                                "note": if held {
                                    "el plan sigue pendiente: no se ha ejecutado ni rechazado"
                                } else {
                                    "no hay ningún plan pendiente con ese id"
                                },
                            })
                            .to_string(),
                        )],
                        // `hold` no es un error: es una decisión válida.
                        is_error: Some(!held),
                    });
                }
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
                    // H-20261116 APPROVE-ACTION-REJECT-SIN-COMPUERTA (P1): rechazar
                    // llamaba a `remove` a secas, así que cualquier autenticado
                    // cancelaba el grant o el plan de otro y no quedaba rastro.
                    // `reject_checked` compara con el autor (o exige clase operador) y
                    // devuelve los datos para la fila de auditoría.
                    let operator = crate::transport::http::auth::operator_gate_allows(
                        crate::transport::http::auth::current_auth_kind(),
                    );
                    match crate::security::grants::reject_checked(request_id, &agent_id, operator).await {
                        crate::security::grants::JudgementOutcome::Removed { guild, tool, owner } => {
                            resolved = true;
                            // Fila de auditoría del rechazo: quién decidió, qué se
                            // descartó y quién lo había pedido.
                            handler_do::audit::audit_or_warn(
                                &format!("approve_action:reject {}", request_id),
                                &guild,
                                &tool,
                                if agent_id.is_empty() { "anonymous" } else { &agent_id },
                                true,
                                &format!("rechazado por {agent_id:?}; autor del plan/grant: {owner:?}"),
                                0,
                                operator,
                            );
                        }
                        crate::security::grants::JudgementOutcome::NotFound => resolved = false,
                        crate::security::grants::JudgementOutcome::NotOwner { owner } => {
                            return Ok(CallToolResult {
                                content: vec![Content::text(
                                    serde_json::json!({
                                        "error": "not_author",
                                        "request_id": request_id,
                                        "author": owner,
                                        "decider": agent_id,
                                        "hint": "solo su autor (o el token del operador) puede rechazarlo; el plan sigue pendiente",
                                    })
                                    .to_string(),
                                )],
                                is_error: Some(true),
                            });
                        }
                    }
                }
                // M31-P2: Try plan store — execute the stored plan directly
                if !resolved && approved
                    && let Some(plan) = crate::security::grants::get_plan(request_id).await
                {
                    // D-13: an agent must not approve its own plan. Before this
                    // check the approver was never compared with the author, so
                    // `tylluan_do(record_experience)`-style loops could plan and
                    // approve themselves with no human in the loop. The plan is
                    // NOT consumed on refusal: it stays pending for a real
                    // approver. Operator credentials (master token / dev bypass)
                    // are exempt — a human can always approve anything.
                    let operator = crate::transport::http::auth::operator_gate_allows(
                        crate::transport::http::auth::current_auth_kind(),
                    );
                    let is_anonymous = |who: &str| who.is_empty() || who == "anonymous";
                    let is_self = plan.agent_id == agent_id
                        || (is_anonymous(&plan.agent_id) && is_anonymous(&agent_id));
                    if is_self && !operator {
                        return Ok(CallToolResult {
                            content: vec![Content::text(
                                serde_json::json!({
                                    "error": "self_approval",
                                    "request_id": request_id,
                                    "author": plan.agent_id,
                                    "approver": agent_id,
                                    "hint": "another agent, or the operator token, must approve this plan",
                                })
                                .to_string(),
                            )],
                            is_error: Some(true),
                        });
                    }
                    let approval_t0 = std::time::Instant::now();
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
                    // Measures guild execution time only, not intent->approval wait (unbounded
                    // human response time, not meaningful as a system-latency signal).
                    let approval_latency_ms = approval_t0.elapsed().as_millis() as u64;
                    crate::security::grants::remove_plan(request_id).await;
                    if !plan.agent_id.is_empty() && plan.agent_id != "anonymous" {
                        let _ = self.journal.as_ref().map(|j| j.checkin(&plan.agent_id, "plan:executed"));
                    }
                    // HITL audit: this execution passed through human approval.
                    // log the same way as the direct path but with human_intervention=true
                    // so ASR / Operational Friction metrics see it correctly.
                    let is_success = result.is_error != Some(true)
                        && !result.content.iter().filter_map(|c| c.as_text())
                            .any(|t| t.text.contains("Exit code:") && !t.text.contains("Exit code: 0"));
                    let preview = result.content.iter().filter_map(|c| c.as_text())
                        .map(|t| t.text.chars().take(200).collect::<String>())
                        .next().unwrap_or_default();
                    let audit_intent = plan.intent.clone();
                    let audit_guild = plan.guild.clone();
                    let audit_tool = plan.tool.clone();
                    // D-13: audit the APPROVER, not the plan author — the author
                    // was the only id recorded before, so the log attributed the
                    // action to whoever wrote the plan. The author is kept in the
                    // preview so the trail is not lost.
                    //
                    // D-26 (O1 de T-DSH-21): si aprueba el operador y el cliente no
                    // declaró `agent_id`, la fila decía `anonymous` con
                    // `human_intervention=1`: el `agent_id` no explicaba nada y el
                    // operador no aparecía por ningún lado. Se materializa
                    // `operator` SOLO en el audit: quién puede aprobar y la
                    // compuerta de autoaprobación no cambian. Un `agent_id`
                    // declarado (aunque sea con el token maestro) se respeta tal
                    // cual, que es lo que ya verifica
                    // `approve_action_audit_names_the_approver_and_human_only_for_operator`.
                    let audit_agent = if operator && is_anonymous(&agent_id) {
                        "operator".to_string()
                    } else {
                        agent_id.clone()
                    };
                    let audit_preview = if plan.agent_id != audit_agent {
                        format!("[plan author: {}] {preview}", plan.agent_id)
                    } else {
                        preview
                    };
                    // `human_intervention` is only true with an operator
                    // credential: it used to be hardcoded `true`, which claimed a
                    // human approved even when another agent had.
                    let human_intervention = operator;
                    tokio::task::spawn_blocking(move || {
                        handler_do::audit::audit_or_warn(
                            &audit_intent, &audit_guild, &audit_tool, &audit_agent,
                            is_success, &audit_preview, approval_latency_ms, human_intervention,
                        );
                    });
                    return Ok(result);
                }
                if !resolved && !approved {
                    // Reject by removing the grant without sending (receiver gets Canceled)
                    resolved = crate::security::grants::remove(request_id).await;
                    // Jose 2026-11-16: un RECHAZO de un plan no lo borraba nadie --
                    // `remove()` solo mira el store de grants, así que el plan seguía
                    // pendiente y la tool respondía «Action not found». El plan
                    // rechazado se retira aquí.
                    //
                    // P1 (H-20261116): también aquí se comprueba la autoría; antes
                    // cualquier autenticado retiraba el plan de otro. Solo se llega a
                    // este bloque con `approved=false` o con un grant ya resuelto, así
                    // que `reject_checked` es el camino correcto (el plan sigue
                    // pendiente si no es suyo).
                    if !resolved {
                        let operator = crate::transport::http::auth::operator_gate_allows(
                            crate::transport::http::auth::current_auth_kind(),
                        );
                        match crate::security::grants::reject_checked(request_id, &agent_id, operator).await {
                            crate::security::grants::JudgementOutcome::Removed { guild, tool, owner } => {
                                resolved = true;
                                handler_do::audit::audit_or_warn(
                                    &format!("approve_action:reject {} (plan)", request_id),
                                    &guild,
                                    &tool,
                                    if agent_id.is_empty() { "anonymous" } else { &agent_id },
                                    true,
                                    &format!("plan rechazado por {agent_id:?}; autor: {owner:?}"),
                                    0,
                                    operator,
                                );
                            }
                            crate::security::grants::JudgementOutcome::NotFound => {}
                            crate::security::grants::JudgementOutcome::NotOwner { owner } => {
                                return Ok(CallToolResult {
                                    content: vec![Content::text(
                                        serde_json::json!({
                                            "error": "not_author",
                                            "request_id": request_id,
                                            "author": owner,
                                            "decider": agent_id,
                                            "hint": "solo su autor (o el token del operador) puede rechazarlo; el plan sigue pendiente",
                                        })
                                        .to_string(),
                                    )],
                                    is_error: Some(true),
                                });
                            }
                        }
                    }
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
            "agent_bootstrap" => {
                // M40-P2: one call instead of whoami + tylluan_recall + list_pending_actions
                // stitched together by hand. See transport::server::bootstrap.
                let target_id = arguments.as_ref()
                    .and_then(|a| a.get("agent_id"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or(&agent_id);
                let ctx = super::bootstrap::build_agent_bootstrap(self.silva.clone(), target_id).await;
                Ok(CallToolResult { content: vec![Content::text(serde_json::to_string_pretty(&ctx).unwrap_or_default())], is_error: Some(false) })
            }
            "undo_last_action" => {
                // M40-P3: revert the agent's last reversible action. Only actions
                // recorded in undo_log (risk>=medium, guild with declared rollback
                // contract, call succeeded) can be undone. The rollback_spec format
                // is "undo_tool(param_from_result)" — for scheduler the result of
                // schedule() IS the schedule_id, so the captured result_text drives
                // the cancel call. Never invents an undo when nothing is on record.
                //
                // Consumption semantics: a broken spec or an empty/error result is
                // consumed (cleared) because it can never be undone; a guild that is
                // momentarily unloaded KEEPS the entry so the agent can request_guild
                // and retry; a real rollback call consumes the entry whether it
                // succeeded or failed (the attempt happened).
                let target_id = arguments.as_ref()
                    .and_then(|a| a.get("agent_id"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or(&agent_id)
                    .to_string();
                let action = {
                    let log = self.undo_log.lock().map_err(|_| McpError::internal_error("undo_log poisoned", None))?;
                    log.get(&target_id).cloned()
                };
                let Some(action) = action else {
                    return Ok(error_result(&format!(
                        "Nothing reversible on record for agent '{target_id}'. \
                         Only risk>=medium calls to guilds with a declared rollback contract \
                         (e.g. scheduler) are recorded — and only when they succeeded."
                    )));
                };

                let spec = action.rollback_spec.as_str();
                let parse = spec.split_once('(').and_then(|(tool, rest)| {
                    let param = rest.trim_end_matches(')').trim();
                    if tool.is_empty() || param.is_empty() { None } else { Some((tool, param)) }
                });
                let Some((undo_tool, undo_param)) = parse else {
                    // Kernel bug or broken contract: the entry can never be undone,
                    // so consume it instead of blocking future undos with a zombie.
                    {
                        let mut log = self.undo_log.lock().map_err(|_| McpError::internal_error("undo_log poisoned", None))?;
                        log.remove(&target_id);
                    }
                    return Ok(error_result(&format!(
                        "Guild '{}' declares rollback spec '{}' without a callable tool(param) shape — kernel bug, report it. \
                         Record cleared.", action.guild, spec
                    )));
                };

                let result_text = action.result_text.trim();
                if result_text.to_lowercase().starts_with("error:") || result_text.is_empty() {
                    {
                        let mut log = self.undo_log.lock().map_err(|_| McpError::internal_error("undo_log poisoned", None))?;
                        log.remove(&target_id);
                    }
                    return Ok(error_result(&format!(
                        "Cannot undo: the recorded result of the original action looks like an error ('{}'), \
                         so there is nothing to revert. Record cleared.", action.result_text
                    )));
                }

                let mut args = serde_json::Map::new();
                args.insert(undo_param.to_string(), serde_json::Value::String(result_text.to_string()));
                let call_params = rmcp::model::CallToolRequestParam {
                    name: undo_tool.to_string().into(),
                    arguments: Some(args),
                };
                let undo_result = {
                    let reg = self.registry.read().await;
                    match reg.guilds.get(&action.guild) {
                        Some(guild) => guild.call_tool_readonly(call_params).await,
                        None => {
                            // Entry KEPT: the guild may come back (request_guild).
                            return Ok(error_result(&format!(
                                "Guild '{}' is not loaded, so the rollback '{}' could not run. \
                                 Request the guild (request_guild) and call undo_last_action again — \
                                 your recorded action is preserved.", action.guild, undo_tool
                            )));
                        }
                    }
                };

                let is_err = undo_result.is_error == Some(true);
                {
                    let mut log = self.undo_log.lock().map_err(|_| McpError::internal_error("undo_log poisoned", None))?;
                    log.remove(&target_id);
                }
                let undo_text: String = undo_result.content.iter()
                    .filter_map(|c| c.as_text())
                    .map(|t| t.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                let summary = serde_json::json!({
                    "undo": "executed",
                    "agent_id": target_id,
                    "guild": action.guild,
                    "original_tool": action.tool,
                    "original_intent": action.intent,
                    "rollback_tool": undo_tool,
                    "rollback_param": undo_param,
                    "rollback_value": result_text,
                    "rollback_success": !is_err,
                    "rollback_output": undo_text.chars().take(300).collect::<String>(),
                });
                Ok(CallToolResult {
                    content: vec![Content::text(serde_json::to_string_pretty(&summary).unwrap_or_default())],
                    is_error: Some(is_err),
                })
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
                handler_do::audit_or_warn("", "kernel", &audit_tool, &audit_agent, audit_success, "", 0, false);
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
    async fn anonymous_remember_reaches_layer2_judge() {
        // D-9/H2 (B-4 H2): the Layer 2 judge used to be spawned inside the
        // `if let Some(agent_id)` branch of handler_remember, so an anonymous
        // write (no agent_id) was stored with no judge at all. The counter is
        // incremented at spawn time, so this asserts the invocation exists
        // without needing a live LLM or a particular verdict.
        let server = test_server().await;
        let before = crate::security::write_gate::judge_spawn_count();

        let mut args = serde_json::Map::new();
        args.insert(
            "content".to_string(),
            serde_json::json!("D-9/H2 probe: an anonymous write must still reach the Layer 2 judge"),
        );
        let result = server
            .handle_kernel_tool("tylluan_remember", Some(args))
            .await
            .expect("anonymous remember must not error at the transport level");
        assert_ne!(result.is_error, Some(true), "anonymous remember must succeed");

        assert!(
            crate::security::write_gate::judge_spawn_count() > before,
            "a write WITHOUT agent_id must still spawn the Layer 2 judge (H2)"
        );
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

    // ── D-13: approve_action must not let an agent approve its own plan ──────

    async fn call_approve(server: &TylluanServer, request_id: &str, agent: Option<&str>) -> CallToolResult {
        let mut args = serde_json::Map::new();
        args.insert("requestId".to_string(), serde_json::json!(request_id));
        args.insert("approved".to_string(), serde_json::json!(true));
        if let Some(a) = agent {
            args.insert("agent_id".to_string(), serde_json::json!(a));
        }
        server
            .handle_kernel_tool("approve_action", Some(args))
            .await
            .expect("approve_action must not fail at the transport level")
    }

    fn tool_text(result: &CallToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|c| c.as_text())
            .map(|t| t.text.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }

    async fn seed_plan(id: &str, author: Option<&str>, intent: &str) {
        // `approve_action` calls `resolve()` before the plan lookup, and the
        // registry is only initialized by the real bootstrap. Both inits are
        // OnceLock::set, so calling them from every test is safe.
        crate::security::grants::init();
        crate::security::grants::init_plan_store();
        let args = serde_json::json!({ "command": "echo d13" });
        crate::security::grants::store_plan(
            id,
            "d13-no-such-guild",
            "d13_noop",
            &args,
            author.unwrap_or("anonymous"),
            intent,
        )
        .await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn approve_action_refuses_self_approval_and_keeps_plan_pending() {
        let server = test_server().await;
        seed_plan("d13-self", Some("agent-a"), "d13:test:self").await;

        let result = call_approve(&server, "d13-self", Some("agent-a")).await;
        assert_eq!(result.is_error, Some(true), "self-approval must be an error");
        let text = tool_text(&result);
        assert!(text.contains("self_approval"), "must name the reason: {text}");

        // The plan must NOT be consumed: a real approver can still take it.
        assert!(
            crate::security::grants::get_plan("d13-self").await.is_some(),
            "a refused self-approval must leave the plan pending"
        );
        crate::security::grants::remove_plan("d13-self").await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn approve_action_accepts_another_agent() {
        let server = test_server().await;
        seed_plan("d13-other", Some("agent-a"), "d13:test:other").await;

        let result = call_approve(&server, "d13-other", Some("agent-b")).await;
        let text = tool_text(&result);
        assert!(!text.contains("self_approval"), "another agent is a valid approver: {text}");
        assert!(
            crate::security::grants::get_plan("d13-other").await.is_none(),
            "an accepted approval consumes the plan"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn approve_action_blocks_anonymous_self_approval() {
        // No agent_id at all on both sides: previously indistinguishable, and
        // exactly the shape of an unattended loop.
        let server = test_server().await;
        seed_plan("d13-anon", None, "d13:test:anon").await;

        let blocked = call_approve(&server, "d13-anon", None).await;
        assert_eq!(blocked.is_error, Some(true));
        assert!(tool_text(&blocked).contains("self_approval"));
        assert!(crate::security::grants::get_plan("d13-anon").await.is_some());

        // A named agent breaks the tie and the plan goes through.
        let ok = call_approve(&server, "d13-anon", Some("agent-b")).await;
        assert!(!tool_text(&ok).contains("self_approval"));
        assert!(crate::security::grants::get_plan("d13-anon").await.is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn approve_action_operator_may_self_approve() {
        use crate::transport::http::auth::{ACL_AUTH_KIND, AclAuthKind};

        let server = test_server().await;
        seed_plan("d13-operator", Some("agent-a"), "d13:test:operator").await;

        let result = ACL_AUTH_KIND
            .scope(AclAuthKind::MasterToken, async {
                call_approve(&server, "d13-operator", Some("agent-a")).await
            })
            .await;
        assert!(
            !tool_text(&result).contains("self_approval"),
            "the operator token must be able to approve anything"
        );
        assert!(
            crate::security::grants::get_plan("d13-operator").await.is_none(),
            "the operator's approval executes the plan"
        );
    }

    /// Poll the audit store for the row of a given intent. The write is
    /// fire-and-forget and other tests write to the same file, so this waits
    /// rather than assuming a fixed position.
    async fn audit_row(intent: &str) -> Option<(String, i64)> {
        use crate::transport::server::handler_do::audit::guild_audit_db_path;
        let path = guild_audit_db_path();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let found = tokio::task::spawn_blocking({
                let path = path.clone();
                let intent = intent.to_string();
                move || -> Option<(String, i64)> {
                    let conn = crate::config::open_db(&path).ok()?;
                    conn.query_row(
                        "SELECT agent_id, human_intervention FROM guild_audit_log
                         WHERE intent = ?1 ORDER BY id DESC LIMIT 1",
                        rusqlite::params![intent],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .ok()
                }
            })
            .await
            .ok()
            .flatten();
            if found.is_some() {
                return found;
            }
            if std::time::Instant::now() >= deadline {
                return None;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn approve_action_audit_names_the_approver_and_human_only_for_operator() {
        use crate::transport::http::auth::{ACL_AUTH_KIND, AclAuthKind};

        let server = test_server().await;

        // Agent-to-agent approval: the log must name the approver, and it must
        // NOT claim a human intervened (that was hardcoded `true` before D-13).
        //
        // The audit path is process-global (`TYLLUAN_AUDIT_DB`): while
        // `audit_store_seam_write_and_read_land_on_same_file` has the seam set,
        // a write from this test lands in that test's temp file and is deleted
        // with it (flaky in CI on 043eb71). Retry with a fresh plan so one
        // attempt falls outside the seam window.
        let mut agent_row = None;
        for attempt in 0..4 {
            let (id, intent) = (format!("d13-audit-agent-{attempt}"), format!("d13:audit:agent:{attempt}"));
            seed_plan(&id, Some("agent-a"), &intent).await;
            let _ = call_approve(&server, &id, Some("agent-b")).await;
            agent_row = audit_row(&intent).await;
            if agent_row.is_some() {
                break;
            }
        }
        let (who, human) = agent_row.expect("the agent approval must be audited");
        assert_eq!(who, "agent-b", "the audit must name the approver, not the author");
        assert_eq!(human, 0, "an agent approval is not human intervention");

        // Operator approval: same, but human_intervention must be true.
        let mut operator_row = None;
        for attempt in 0..4 {
            let (id, intent) = (format!("d13-audit-operator-{attempt}"), format!("d13:audit:operator:{attempt}"));
            seed_plan(&id, Some("agent-a"), &intent).await;
            let _ = ACL_AUTH_KIND
                .scope(AclAuthKind::MasterToken, async {
                    call_approve(&server, &id, Some("agent-a")).await
                })
                .await;
            operator_row = audit_row(&intent).await;
            if operator_row.is_some() {
                break;
            }
        }
        let (who, human) = operator_row.expect("the operator approval must be audited");
        assert_eq!(who, "agent-a");
        assert_eq!(human, 1, "the operator credential IS human intervention");
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

    // ── M40-P3: Plan → Verify → Rollback cycle ────────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn test_undo_last_action_no_record_returns_clear_message() {
        // Nothing reversible on record must NOT invent an undo — honest minimal
        // contract (approved design decision 1: no fabricated metadata).
        let server = test_server().await;
        let args = Some(serde_json::json!({"agent_id": "test-agent"}).as_object().unwrap().clone());
        let result = server.handle_kernel_tool("undo_last_action", args).await;
        assert!(result.is_ok());
        let r = result.unwrap();
        assert_eq!(r.is_error, Some(true), "no record must error clearly");
        let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(text.contains("Nothing reversible"), "must say nothing is on record, got: {text}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_undo_last_action_reads_recorded_action() {
        // Record a reversible action directly (as the Stage-5 wrapper would)
        // and check the undo handler parses the rollback spec and attempts
        // the real rollback call against the guild.
        let server = test_server().await;
        server.undo_log.lock().unwrap().insert(
            "test-agent".to_string(),
            crate::transport::server::types::ReversibleAction {
                guild: "scheduler".to_string(),
                tool: "schedule".to_string(),
                rollback_spec: "cancel_schedule(schedule_id)".to_string(),
                intent: "schedule a wake-up".to_string(),
                result_text: "abc-123-schedule-id".to_string(),
                ts_unix: chrono::Utc::now().timestamp(),
            },
        );
        let args = Some(serde_json::json!({"agent_id": "test-agent"}).as_object().unwrap().clone());
        let result = server.handle_kernel_tool("undo_last_action", args).await;
        // Guild isn't loaded in the test server, so the rollback call itself
        // can't run — the contract surface must still be exercised: spec
        // parsed, param bound, and the entry KEPT for a retry after
        // request_guild (unloaded guild ≠ consumed action).
        assert!(result.is_ok());
        let r = result.unwrap();
        let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(
            text.contains("not loaded"),
            "expected guild-not-loaded path for the real rollback call, got: {text}"
        );
        assert!(
            text.contains("preserved"),
            "message must say the recorded action is preserved, got: {text}"
        );
        assert!(
            server.undo_log.lock().unwrap().get("test-agent").is_some(),
            "entry must be KEPT when the guild is unloaded so the agent can retry"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_undo_last_action_removes_entry_on_parse_failure() {
        // A malformed rollback spec (no callable tool) is a kernel bug; the
        // handler must report it and still clear the record instead of
        // leaving a poisoned entry that would block future undos.
        let server = test_server().await;
        server.undo_log.lock().unwrap().insert(
            "test-agent".to_string(),
            crate::transport::server::types::ReversibleAction {
                guild: "scheduler".to_string(),
                tool: "schedule".to_string(),
                rollback_spec: "no_parens_spec".to_string(),
                intent: "schedule a wake-up".to_string(),
                result_text: "abc-123".to_string(),
                ts_unix: chrono::Utc::now().timestamp(),
            },
        );
        let args = Some(serde_json::json!({"agent_id": "test-agent"}).as_object().unwrap().clone());
        let result = server.handle_kernel_tool("undo_last_action", args).await;
        assert!(result.is_ok());
        let r = result.unwrap();
        let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(text.contains("without a callable tool"), "must flag the broken spec, got: {text}");
        assert!(
            server.undo_log.lock().unwrap().get("test-agent").is_none(),
            "poisoned entry must be cleared"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_scheduler_descriptor_declares_rollback_contract() {
        // M40-P3: the rollback contract is real catalog data, so the Stage-5
        // wrapper (handler_do) and the undo handler both read the same source
        // of truth. Regression guard: scheduler must declare its cancel tool.
        let descriptors = crate::router::catalog::builtin_catalog();
        let scheduler = descriptors.iter().find(|d| d.name == "scheduler");
        assert!(scheduler.is_some(), "scheduler must be in the catalog");
        let rollback = scheduler.and_then(|d| d.rollback.clone());
        assert_eq!(
            rollback.as_deref(),
            Some("cancel_schedule(schedule_id)"),
            "scheduler must declare its rollback contract"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn approve_action_hold_deja_el_plan_pendiente_y_cuenta_criterio() {
        let server = test_server().await;
        seed_plan("hold-d184", Some("agent-a"), "d184:hold").await;

        let antes = crate::security::grants::judgement_counters();
        let mut args = serde_json::Map::new();
        args.insert("requestId".to_string(), serde_json::json!("hold-d184"));
        args.insert("decision".to_string(), serde_json::json!("hold"));
        args.insert("reason".to_string(), serde_json::json!("falta saber si es produccion"));
        args.insert("agent_id".to_string(), serde_json::json!("agent-b"));
        let result = server
            .handle_kernel_tool("approve_action", Some(args))
            .await
            .expect("hold no debe fallar a nivel de transporte");

        assert_eq!(result.is_error, Some(false), "hold no es un error");
        let text = tool_text(&result);
        assert!(text.contains("held"), "debe decir held: {text}");
        assert!(
            text.contains("sigue pendiente"),
            "debe dejar claro que no se ejecutó: {text}"
        );

        // El plan sigue ahí, con el motivo.
        let plan = crate::security::grants::get_plan("hold-d184")
            .await
            .expect("hold NO consume el plan");
        let nota = plan.held.expect("hold debe dejar nota");
        assert_eq!(nota.reason, "falta saber si es produccion");

        // Y cuenta como criterio, no como aprobación.
        let despues = crate::security::grants::judgement_counters();
        assert_eq!(
            despues["held"].as_u64().unwrap(),
            antes["held"].as_u64().unwrap() + 1
        );
        crate::security::grants::remove_plan("hold-d184").await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn approve_action_hold_sin_motivo_es_error() {
        let server = test_server().await;
        seed_plan("hold-d184-sinmotivo", Some("agent-a"), "d184:hold:sinmotivo").await;

        let mut args = serde_json::Map::new();
        args.insert("requestId".to_string(), serde_json::json!("hold-d184-sinmotivo"));
        args.insert("decision".to_string(), serde_json::json!("hold"));
        let result = server
            .handle_kernel_tool("approve_action", Some(args))
            .await
            .expect("no debe fallar a nivel de transporte");

        assert_eq!(result.is_error, Some(true));
        assert!(tool_text(&result).contains("reason"), "debe pedir el motivo");
        // Y no marca nada: el plan queda intacto y sin nota.
        let plan = crate::security::grants::get_plan("hold-d184-sinmotivo").await;
        assert!(plan.map(|p| p.held.is_none()).unwrap_or(true));
        crate::security::grants::remove_plan("hold-d184-sinmotivo").await;
    }

    /// Jose 2026-11-16: rechazar un PLAN por `approve_action` tiene que borrarlo.
    /// Antes `remove()` solo miraba grants, el plan seguía pendiente y la tool
    /// respondía «Action not found» — el rechazo no rechazaba nada.
    ///
    /// P1 H-20261116: quien rechaza tiene que ser **su autor** o el operador, así que
    /// aquí el autor y el que decide son el mismo (antes este test rechazaba un plan
    /// de otro y daba la holgura por buena).
    #[tokio::test(flavor = "multi_thread")]
    async fn approve_action_rechazar_un_plan_lo_borra() {
        let server = test_server().await;
        seed_plan("reject-plan-d189", Some("agent-b"), "d189:reject:plan").await;

        let mut args = serde_json::Map::new();
        args.insert("requestId".to_string(), serde_json::json!("reject-plan-d189"));
        args.insert("approved".to_string(), serde_json::json!(false));
        args.insert("agent_id".to_string(), serde_json::json!("agent-b"));
        let result = server
            .handle_kernel_tool("approve_action", Some(args))
            .await
            .expect("rechazar no debe fallar a nivel de transporte");

        assert_eq!(result.is_error, Some(false), "rechazar es una acción válida: {}", tool_text(&result));
        assert!(
            crate::security::grants::get_plan("reject-plan-d189").await.is_none(),
            "el plan rechazado debe desaparecer, no quedarse pendiente"
        );
    }

    /// P1 H-20261116 APPROVE-ACTION-REJECT-SIN-COMPUERTA: un tercero NO puede
    /// descartar el plan de otro, y el plan sigue pendiente para su autor.
    /// MUTACION: sin la compuerta, el plan desaparece y la tool responde «ok».
    #[tokio::test(flavor = "multi_thread")]
    async fn approve_action_no_deja_que_un_tercero_rechace_el_plan_de_otro() {
        let server = test_server().await;
        seed_plan("reject-plan-ajeno", Some("agent-a"), "p1:reject:ajeno").await;

        let mut args = serde_json::Map::new();
        args.insert("requestId".to_string(), serde_json::json!("reject-plan-ajeno"));
        args.insert("approved".to_string(), serde_json::json!(false));
        args.insert("agent_id".to_string(), serde_json::json!("agent-b"));
        let result = server
            .handle_kernel_tool("approve_action", Some(args))
            .await
            .expect("no debe fallar a nivel de transporte");

        assert_eq!(result.is_error, Some(true), "rechazar lo ajeno es un error: {}", tool_text(&result));
        assert!(
            tool_text(&result).contains("not_author"),
            "tiene que decir por qué: {}",
            tool_text(&result)
        );
        assert!(
            crate::security::grants::get_plan("reject-plan-ajeno").await.is_some(),
            "el plan del otro tiene que seguir pendiente"
        );
        crate::security::grants::remove_plan("reject-plan-ajeno").await;
    }

    // ── D5033b: identidad de la tool call MCP ligada a la credencial ─────────
    // El TL pidió (T5370) el camino real del lane MCP. `resolve_acl_role` NUNCA se
    // llama desde `transport/server/` (0 callers medidos en 8dea5e12): lo único que
    // llega del lane HTTP es el task-local `ACL_AGENT_ID` que instala
    // `bearer_auth_middleware` desde `[security.acl.token_agent_bindings]`. Aquí se
    // prueba ese hilo de identidad en la entrada real (`handle_kernel_tool`), con
    // `whoami` como observable: devuelve en `agent_id` la identidad con la que se
    // resolvió la llamada (read-only, sin efectos en disco).

    /// Utilidad: qué identidad resolvió el dispatch para esta llamada.
    async fn resolucion_de_identidad(server: &TylluanServer) -> String {
        let result = server
            .handle_kernel_tool("whoami", None)
            .await
            .expect("whoami no debe fallar a nivel de transporte");
        let text = tool_text(&result);
        let value: serde_json::Value =
            serde_json::from_str(&text).expect("whoami responde JSON");
        value["agent_id"].as_str().unwrap_or_default().to_string()
    }

    /// Caso 1 de dsh (T5372): token vinculado + `agent_id` de OTRO agente.
    /// La guarda ya existía; se fija como regresión.
    /// MUTACION: sin la guarda, `whoami` responde la identidad suplantada.
    #[tokio::test(flavor = "multi_thread")]
    async fn d5033b_token_vinculado_no_deja_suplantar_a_otro_agente() {
        let server = test_server().await;
        let mut args = serde_json::Map::new();
        args.insert("agent_id".to_string(), serde_json::json!("otro-agente"));
        let result = crate::transport::http::auth::ACL_AGENT_ID
            .scope(
                "agente-vinculado".to_string(),
                server.handle_kernel_tool("whoami", Some(args)),
            )
            .await
            .expect("no debe fallar a nivel de transporte");
        assert_eq!(result.is_error, Some(true), "suplantar debe rechazarse: {}", tool_text(&result));
        assert!(
            tool_text(&result).contains("this token is bound to agent 'agente-vinculado'"),
            "el rechazo debe decir que la credencial está vinculada: {}",
            tool_text(&result)
        );
    }

    /// Caso 2 de dsh (T5372): token vinculado SIN `agent_id` en el payload.
    /// Antes de D5033b esto caía en `"anonymous"`, que está exceptuado de las dos
    /// puertas (`handlers.rs:34` y `:44`): el token silencioso se quitaba las
    /// restricciones en vez de aplicarse las de su agente.
    /// MUTACION: sin la resolución por credencial, `whoami` responde `anonymous`.
    #[tokio::test(flavor = "multi_thread")]
    async fn d5033b_token_vinculado_sin_agent_id_resuelve_por_la_credencial() {
        let server = test_server().await;
        let identity = crate::transport::http::auth::ACL_AGENT_ID
            .scope(
                "agente-vinculado".to_string(),
                resolucion_de_identidad(&server),
            )
            .await;
        assert_eq!(
            identity.as_str(),
            "agente-vinculado",
            "sin agent_id en el payload, la identidad la pone la credencial vinculada"
        );
    }

    /// Caso 2b: declararse explícitamente `"anonymous"` no puede usarse para
    /// evadir las puertas del propio token.
    /// MUTACION: con la excepción `agent_id != "anonymous"` sola, sale `anonymous`.
    #[tokio::test(flavor = "multi_thread")]
    async fn d5033b_declarse_anonymous_no_evade_la_credencial_vinculada() {
        let server = test_server().await;
        let mut args = serde_json::Map::new();
        args.insert("agent_id".to_string(), serde_json::json!("anonymous"));
        let result = crate::transport::http::auth::ACL_AGENT_ID
            .scope(
                "agente-vinculado".to_string(),
                server.handle_kernel_tool("whoami", Some(args)),
            )
            .await
            .expect("no debe fallar a nivel de transporte");
        assert_eq!(result.is_error, Some(false), "declararse anonymous no es suplantar: {}", tool_text(&result));
        let value: serde_json::Value =
            serde_json::from_str(&tool_text(&result)).expect("whoami responde JSON");
        assert_eq!(
            value["agent_id"].as_str(),
            Some("agente-vinculado"),
            "`anonymous` no puede borrar la identidad de la credencial"
        );
    }

    /// Caso 3 de dsh (T5372): un camino que NO pasa por el middleware de auth
    /// (stdio, dispatch interno) no tiene binding y `current_bound_agent_id()`
    /// devuelve `None`. Este test fija el límite REAL del parche: ahí la identidad
    /// sigue siendo la del payload, y cerrarlo exige decisión de José (binding
    /// token->agente para stdio/dev_mode), no un parche en esta capa.
    #[tokio::test(flavor = "multi_thread")]
    async fn d5033b_sin_binding_del_middleware_gana_el_payload() {
        let server = test_server().await;
        let identity = resolucion_de_identidad(&server).await;
        assert_eq!(
            identity.as_str(),
            "anonymous",
            "sin credencial ni binding, el dispatch no tiene identidad autenticada"
        );
        let mut args = serde_json::Map::new();
        args.insert("agent_id".to_string(), serde_json::json!("cualquiera"));
        let result = server
            .handle_kernel_tool("whoami", Some(args))
            .await
            .expect("no debe fallar a nivel de transporte");
        assert_eq!(result.is_error, Some(false), "sin binding la guarda no se aplica: {}", tool_text(&result));
    }

}