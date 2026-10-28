use rmcp::{Error as McpError, model::*};
use chrono;

/// MD-3 (MEASUREMENT_DEBT_REGISTER): prefijos estables y parseables por máquina
/// para los fallos de Stage-1 de `tylluan_do`. Convención estrictamente aditiva:
/// el prefijo se antepone y el cuerpo legible del mensaje se preserva verbatim,
/// de modo que consumidores existentes (tests, dashboard, triaje de benchmarks)
/// sigan haciendo match con el texto anterior. Las denegaciones ACL ya llevan su
/// propio prefijo estable (`ACCESS_DENIED:`, `transport/http/auth.rs`) y los
/// intents sin guild ya llevan `NO_GUILD_MATCH:`; ambos se dejan intactos a
/// propósito.
///
/// NOTA de parsing para consumidores: todo mensaje que sale por `error_result()`
/// lleva el wrapper uniforme `"❌ Error: "` (`registry/proxy.rs`), así que el
/// texto final que ve un cliente es `"❌ Error: {PREFIX} {cuerpo}"`. El
/// clasificador debe quitar ese wrapper antes de matchear el prefijo (o buscar
/// el token del prefijo directamente).
pub mod error_prefixes {
    pub const ROUTING_FAILED: &str = "ROUTING_FAILED:";
    pub const RATE_LIMITED: &str = "RATE_LIMITED:";
    pub const START_FAILED: &str = "START_FAILED:";
    pub const NO_TOOLS: &str = "NO_TOOLS:";
    pub const MISSING_ARGS: &str = "MISSING_ARGS:";
}

use crate::registry::proxy::error_result;
use super::TylluanServer;

pub(crate) mod routing;
mod embedding;
mod coloquio_utils;
mod coloquio_wait;
mod timeout;
mod external_mcp;
pub(crate) mod audit;
mod coloquio;
mod forget;
mod skill;
mod correct;
mod a2a;
mod outcome;
mod prepare;
mod sentinel_cmd;
mod llama_cmd;

use prepare::{execute_guild_call_with_progress, resolve_and_prepare_tool_call};

use outcome::{persist_remember, post_process_outcome};

pub use embedding::re_embed_legacy_nodes;
pub(crate) use embedding::distill_for_embedding;
pub(crate) use routing::maybe_auto_extract_triples;
pub use timeout::guild_effective_timeout;
pub(crate) use audit::verify_audit_chain;
pub use audit::audit_open_readonly;
pub use audit::audit_db_path;
pub use audit::check_dangerous_intent;
pub(crate) use audit::routing_failure_id;
pub(crate) use audit::log_audit_entry;
pub(crate) use audit::audit_or_warn;
pub(crate) use skill::handle_skill_prefix;
pub(crate) use correct::handle_correct_prefix;
pub(crate) use coloquio::handle_coloquio_prefix;
pub(crate) use forget::handle_forget_shortcut;
pub(crate) use a2a::handle_a2a_agent_prefix;


/// Returns true if the intent explicitly asks for a bash state restore/checkpoint
/// operation (not a shell command). Used to disambiguate bash guild routing.
pub(crate) fn is_bash_state_intent(intent: &str) -> bool {
    let lower = intent.trim().to_lowercase();
    lower == "state_restore" || lower == "restore state"
        || lower == "state_checkpoint" || lower == "checkpoint state"
        || lower == "state save" || lower == "save state"
        || lower.starts_with("state_restore ") || lower.starts_with("state_checkpoint ")
        || (lower.starts_with("restore ") && lower.contains("checkpoint"))
        || (lower.starts_with("save ") && lower.contains("checkpoint"))
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
    let mut guild_hint = arguments.as_ref()
        .and_then(|a| a.get("guild")).and_then(|v| v.as_str())
        .map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let plan_mode = arguments.as_ref()
        .and_then(|a| a.get("plan")).and_then(|v| v.as_bool())
        .unwrap_or(false);

    tracing::info!(
        gen_ai.operation.name = "tylluan_do",
        gen_ai.request.model = %guild_hint.as_deref().unwrap_or("semantic_router"),
        intent = %intent,
        "Handling tylluan_do"
    );

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

    // T-DSH-114: interruptor del centinela por MCP (deterministico, sin guild,
    // sin herramienta nueva: CONTRACT-01 sigue con 6 sovereign tools).
    if let Some(result) = sentinel_cmd::handle_sentinel_intent(&intent) {
        let is_err = result.get("ok") == Some(&serde_json::json!(false));
        return Ok(CallToolResult {
            content: vec![Content::text(serde_json::to_string_pretty(&result).unwrap_or_default())],
            is_error: Some(is_err),
        });
    }

    // T-DSH-116: lectura del registro de puntos de inferencia (sin claves).
    if let Some(result) = llama_cmd::handle_points_intent(&intent).await {
        return Ok(CallToolResult {
            content: vec![Content::text(serde_json::to_string_pretty(&result).unwrap_or_default())],
            is_error: Some(false),
        });
    }

    // T-DSH-115: intents de LECTURA del llama-server del usuario (sin arrancar).
    if let Some(result) = llama_cmd::handle_llama_intent(server, &intent).await {
        let is_err = result.get("ok") == Some(&serde_json::json!(false));
        return Ok(CallToolResult {
            content: vec![Content::text(serde_json::to_string_pretty(&result).unwrap_or_default())],
            is_error: Some(is_err),
        });
    }

    // Coloquio long-poll wait (turn 498/500 design): blocks this MCP call
    // until a new message arrives in the channel or the timeout expires.
    // Must run BEFORE handle_coloquio_prefix so @coloquio:wait is not treated as a channel named 'wait'.
    if let Some(result) = coloquio_wait::handle_coloquio_wait_prefix(server, &intent, &agent_id).await {
        return Ok(result);
    }

    // Deterministic @coloquio: prefix — bypass semantic router entirely
    if let Some(result) = handle_coloquio_prefix(server, &intent, &agent_id).await {
        return result;
    }

    // Pre-IQE coloquio forcing: the deterministic gate inside
    // resolve_guild_name runs on the ENRICHED+stripped intent ([ctx: ...]
    // prepended by intent_enhancer when the session has recent intents).
    // That enrichment can shift the prefix check intermittently, letting a
    // "publica en coloquio X: ..." fall into the semantic router (observed
    // live 2026-09-16: routed to bash). Forcing the guild hint here, on the
    // ORIGINAL intent, makes the coloquio route immune to enrichment — the
    // same discipline as the pre-IQE trigger fast-path in routing.rs.
    if guild_hint.is_none() && coloquio_utils::is_coloquio_dispatch_intent(&intent) {
        guild_hint = Some("coloquio".to_string());
    }

    // Deterministic nodo/node prefix — agent-to-agent messaging
    // Uses existing AgentNodeRouter + parse_node_intent from agent_nodes.rs
    if let Some(result) = handle_nodo_prefix(server, &intent, &agent_id).await {
        return result;
    }

    // Deterministic @agent prefix — dispatch to configured external A2A agents
    // (roster persisted in SilvaDB, managed via REST /api/v1/a2a/agents). F2.
    if let Some(result) = handle_a2a_agent_prefix(server, &intent, &agent_id).await {
        return result;
    }

    // Deterministic whoami/register_identity routing — these are real kernel
    // tools (see handlers.rs) but are NOT in the 6-sovereign tools/list surface,
    // by design (CONTRACT-01). A client that only ever discovers tools via
    // tools/list (LM Studio, Qwen, generic MCP clients -- anything that isn't
    // told the literal tool name out of band) could never reach them otherwise.
    // Routing through tylluan_do's intent text keeps the 6-tool contract intact
    // while making identity actually usable by the whole fleet, not just
    // clients that happen to know the exact tool name.
    {
        let lower = intent.trim().to_lowercase();
        let is_whoami = lower == "whoami" || lower.starts_with("whoami ")
            || lower.contains("who am i") || lower.contains("quien soy") || lower.contains("quién soy")
            || lower.contains("mi identidad") || lower.contains("my identity");
        let is_register = lower == "register_identity" || lower.starts_with("register_identity ")
            || lower.contains("register my identity") || lower.contains("registra mi identidad")
            || lower.contains("register identity");
        if is_whoami || is_register {
            let mut kernel_args = arguments.clone().unwrap_or_default();
            if let Some(aid) = &agent_id {
                kernel_args.entry("agent_id".to_string()).or_insert_with(|| serde_json::Value::String(aid.clone()));
            }
            let tool_name = if is_register { "register_identity" } else { "whoami" };
            // Box::pin: handle_kernel_tool -> handle_tylluan_do -> handle_kernel_tool
            // is a real call cycle (different tool name each time, so it terminates
            // at runtime), but the compiler needs explicit indirection to size the
            // recursive async fn.
            return Box::pin(server.handle_kernel_tool(tool_name, Some(kernel_args))).await;
        }
    }

    // Deterministic list_available_guilds / agent_bootstrap routing -- same
    // rationale as whoami above (CONTRACT-01 keeps these off tools/list, so
    // intent text is the only discovery path for clients that don't already
    // know the literal tool name). Found live, 2026-08-09: neither was
    // reachable from natural language at all before this -- the semantic
    // router misrouted "list available guilds" to bash and "agent bootstrap"
    // to coloquio. Discoverability debt (P2), logged in Coloquio, fixed here.
    {
        let lower = intent.trim().to_lowercase();
        // SHORT_INTENT_MAX: a real command phrase ("agent bootstrap", "orientame")
        // is short. A long intent that happens to CONTAIN one of these phrases
        // deep inside its text (e.g. posting a Coloquio message that quotes
        // "qué estaba haciendo") must NOT be hijacked by this matcher -- found
        // live, 2026-08-09: this exact bug swallowed a real Coloquio announcement
        // whose body contained the phrase, routing it to agent_bootstrap instead
        // of letting it reach the coloquio guild. Anchoring to short intents only
        // keeps the deterministic shortcut without shadowing real guild calls.
        const SHORT_INTENT_MAX: usize = 60;
        let is_short = lower.len() <= SHORT_INTENT_MAX;
        let is_list_guilds = is_short && (
            lower.contains("list_available_guilds") || lower.contains("list available guild")
            || lower.contains("list guilds") || lower.contains("lista de guilds")
            || lower.contains("qué guilds") || lower.contains("que guilds")
            || lower.contains("available guilds") || lower.contains("guild contracts")
            || lower.contains("guild capabilities"));
        let is_bootstrap = is_short && (
            lower.contains("agent_bootstrap") || lower.contains("agent bootstrap")
            || lower.contains("bootstrap context") || lower.contains("bootstrap me")
            || lower.contains("orient me") || lower.contains("orientame") || lower.contains("oriéntame")
            || lower.contains("contexto de sesión") || lower.contains("contexto de sesion")
            || lower.contains("qué estaba haciendo") || lower.contains("que estaba haciendo"));
        // M40-P3: deterministic undo routing. Same guard rationale as the other
        // matchers here — anchored to short intents only so a long message that
        // merely quotes "deshacer la última acción" is never hijacked.
        let is_undo = is_short && (
            lower.contains("undo_last_action") || lower.contains("undo last action")
            || lower.contains("undo my last action") || lower.contains("deshacer última acción")
            || lower.contains("deshacer ultima accion") || lower.contains("deshacer la última")
            || lower.contains("deshacer la ultima"));
        // Found live by an external audit, 2026-08-09: 'health' and
        // 'list_pending_actions' are both real kernel subtools with real MCP
        // schemas, but had zero deterministic routing -- "health" alone was
        // misrouted to the bash guild (returned isError=false, making the
        // failure look like a legitimate result instead of a routing miss).
        let is_health = is_short && (lower == "health" || lower.contains("kernel health")
            || lower.contains("salud del kernel") || lower.contains("estado del kernel"));
        let is_list_pending = is_short && (
            lower.contains("list_pending_actions") || lower.contains("list pending action")
            || lower.contains("pending approvals") || lower.contains("acciones pendientes")
            || lower.contains("aprobaciones pendientes"));
        // Found live by Qwen Desktop's diagnostic report, 2026-08-11: "doctor_diagnose"
        // had zero deterministic routing and fell through to the semantic router,
        // which anchor-matched it to sequential_thinking's compare_options (score
        // 0.923) instead -- a real kernel subtool made unreachable by natural
        // language, same class of bug as health/list_pending_actions above.
        // doctor_diagnose takes no args, safe to route on the bare phrase alone.
        // doctor_repair requires 'target' via structured arguments (not parsed from
        // free text here) -- routing it deterministically just means a missing
        // target now returns handlers.rs's clear guidance message instead of a
        // wrong-guild misroute.
        let is_doctor_diagnose = is_short && (
            lower.contains("doctor_diagnose") || lower.contains("doctor diagnose")
            || lower.contains("diagnóstico del kernel") || lower.contains("diagnostico del kernel"));
        let is_doctor_repair = is_short && (
            lower.contains("doctor_repair") || lower.contains("doctor repair")
            || lower.contains("reparar kernel") || lower.contains("repair kernel"));
        // D-16: la instrucción que el PROPIO kernel emite en modo plan
        // (`tylluan_do(intent='approve action for plan <id>')`) no tenía ruta
        // determinista: caía en el router semántico y moría con NO_GUILD_MATCH,
        // así que la aprobación por intent era imposible y la compuerta de
        // D-13 sólo existía en `approve_action`. Se enruta a esa misma
        // herramienta - la compuerta y el audit son UNO, no dos copias.
        let approve_plan_id = if is_short {
            plan_id_from_approve_intent(&lower)
        } else {
            None
        };
        if is_list_guilds || is_bootstrap || is_undo || is_health || is_list_pending
            || is_doctor_diagnose || is_doctor_repair || approve_plan_id.is_some() {
            let mut kernel_args = arguments.clone().unwrap_or_default();
            if let Some(aid) = &agent_id {
                kernel_args.entry("agent_id".to_string()).or_insert_with(|| serde_json::Value::String(aid.clone()));
            }
            let tool_name = if let Some(plan_id) = &approve_plan_id {
                kernel_args.insert("requestId".to_string(), serde_json::Value::String(plan_id.clone()));
                kernel_args.insert("approved".to_string(), serde_json::Value::Bool(true));
                "approve_action"
            } else if is_bootstrap {
                "agent_bootstrap"
            } else if is_undo {
                "undo_last_action"
            } else if is_health {
                "health"
            } else if is_list_pending {
                "list_pending_actions"
            } else if is_doctor_diagnose {
                "doctor_diagnose"
            } else if is_doctor_repair {
                "doctor_repair"
            } else {
                "list_available_guilds"
            };
            return Box::pin(server.handle_kernel_tool(tool_name, Some(kernel_args))).await;
        }

        let explicit_explore = arguments.as_ref()
            .and_then(|a| a.get("explore"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let is_explore_intent = is_short && (
            lower.starts_with("explore") || lower.starts_with("explora")
            || lower.contains("subtools") || lower.contains("discover tools")
            || lower.contains("descubrir herramientas") || lower.contains("ver herramientas")
        );

        if explicit_explore.is_some() || is_explore_intent {
            let target_domain = explicit_explore.unwrap_or_else(|| {
                let cleaned = lower
                    .replace("explore", "")
                    .replace("explora", "")
                    .replace("subtools", "")
                    .replace("discover tools", "")
                    .replace("descubrir herramientas", "")
                    .replace("ver herramientas", "")
                    .replace("herramientas", "");
                cleaned.trim().to_string()
            });
            let available_guilds = server.matcher.available_guilds();
            let result_json = TylluanServer::explore_actionable_tools(&target_domain, &available_guilds);
            return Ok(CallToolResult {
                content: vec![Content::text(serde_json::to_string_pretty(&result_json).unwrap_or_default())],
                is_error: Some(false),
            });
        }
    }

    // Ouroboros Loop — record half. Deterministic intent match so any client
    // can persist a self-critique of an action's outcome via natural language,
    // per-agent, without a new tool. The agent (the LLM) does the reflection and
    // passes action/outcome/verdict/lesson as arguments; the kernel only stores.
    {
        let lower = intent.trim().to_lowercase();
        let is_record_exp = lower.contains("record_experience")
            || lower.contains("record experience") || lower.contains("registra experiencia")
            || lower.contains("registrar experiencia") || lower.contains("aprendí de esto")
            || lower.contains("log outcome");
        if is_record_exp {
            let aid = agent_id.as_deref().unwrap_or("anonymous");
            if aid == "anonymous" {
                return Ok(error_result("record_experience requires an agent_id (experiences are stored per-agent)."));
            }
            let arg = |k: &str| arguments.as_ref().and_then(|a| a.get(k)).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let action = arg("action");
            let outcome = arg("outcome");
            let verdict = { let v = arg("verdict"); if v.is_empty() { "partial".to_string() } else { v } };
            let lesson = arg("lesson");
            if action.is_empty() || outcome.is_empty() {
                return Ok(error_result(
                    "record_experience needs `action` and `outcome` as arguments (and optionally `verdict`=worked|failed|partial, `lesson`). \
                     The agent reflects; the kernel only stores.",
                ));
            }
            if let Some(ref amm) = server.agent_memory {
                let id = amm.record_experience(aid, &action, &outcome, &verdict, &lesson).await;
                return Ok(CallToolResult {
                    content: vec![Content::text(format!("✅ Experience recorded ({verdict}) as {id} — will surface in your future tylluan_think on related topics."))],
                    is_error: Some(false),
                });
            }
            return Ok(error_result("Agent memory subsystem not available."));
        }
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

    // Stage 1: Resolve guild + ACL/rate-limit + tool selection + args + call_params
    let routing_intent = intent_enhancer::strip_ctx_prefix(&effective_intent);
    let resolved = match resolve_and_prepare_tool_call(
        server, routing_intent, &intent, &effective_intent,
        &guild_hint, &agent_id, &arguments,
    ).await {
        Ok(r) => r,
        Err(call_tool_result) => return Ok(call_tool_result),
    };

    // Cognitive Scheduler — Phase 3, OBSERVATION ONLY (same pattern as
    // CoherenceGate Layer 4 at first): build a TaskContext from the fully
    // resolved Stage-1 data and log decide()'s verdict next to the routing
    // trace. Dispatch still never READS the verdict — guild_name and tool
    // selection flow exactly as before this call existed. WS3: the verdict is
    // now also CARRIED (not consumed) to Stage 3, where the outcome half of
    // the pair is known and the confusion collector persists it. Plan mode
    // drops it here: plans never execute, so there is no outcome to pair.
    let scheduler_decision = crate::router::scheduler::observe::observe_scheduling(
        server,
        &crate::router::scheduler::observe::SchedulingObservation {
            intent: &intent,
            routing_intent,
            agent_id: agent_id.as_deref(),
            guild_name: &resolved.guild_name,
            tool_name: &resolved.tool_name,
            routing_trace: &resolved.routing_trace,
            plan_mode,
        },
    ).await;
    let scheduler_decision = (!plan_mode).then_some(scheduler_decision);

    // M31-P2: Plan mode — return resolved guild+tool+args for approval before executing
    if plan_mode {
        let plan_id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let risk_level = server.check_tool_risk(&resolved.tool_name).await;
        let plan_info = serde_json::json!({
            "status": "plan",
            "plan_id": plan_id,
            "guild": resolved.guild_name,
            "tool": resolved.tool_name,
            "risk_level": format!("{:?}", risk_level),
            "intent": intent,
            "arguments": resolved.tool_args,
            "message": format!(
                "Plan mode: would execute '{}' via guild '{}' tool '{}' (risk: {:?}). \
                 To approve, call: tylluan_do(intent='approve action for plan {plan_id}') or \
                 approve_action(requestId='{plan_id}', approved=true).",
                intent, resolved.guild_name, resolved.tool_name, risk_level
            ),
        });
        crate::security::grants::store_plan(
            &plan_id, &resolved.guild_name, &resolved.tool_name, &resolved.tool_args,
            agent_id.as_deref().unwrap_or("anonymous"), &intent,
        ).await;
        let result_text = serde_json::to_string_pretty(&plan_info).unwrap_or_default();
        return Ok(CallToolResult {
            content: vec![Content::text(result_text)],
            is_error: Some(false),
        });
    }

    // Stage 2: Execute guild call with progress ticker and safety timeout
    let (result, latency_ms) = execute_guild_call_with_progress(
        server, &resolved.guild_name, &resolved.tool_name,
        resolved.call_params, &intent, &agent_id,
    ).await;

    // Stage 3: Post-process — reactive cascade, agent profiles, audit, anchor learning, lesson drain, etc.
    let (mut result, _) = post_process_outcome(
        server, &intent, &resolved.guild_name, &resolved.tool_name,
        &agent_id, remember, scheduler_decision, result, latency_ms,
    ).await;

    // Stage 4: Persist to memory and SilvaDB if remember flag or agent_id is set
    persist_remember(
        server, &result, &agent_id, &intent,
        &resolved.guild_name, &resolved.tool_name, remember,
    ).await;

    // Stage 5 (M40-P3): structured post-execution report for risk >= Medium.
    // Wrapped only when the action actually modified state (risk gate per
    // approved design — read-only/Light guilds skip the wrapper, YAGNI).
    // Report is minimal by default: intended/performed/changed. The
    // `verified`/`rollback` fields are added ONLY when the guild declares
    // real metadata (GuildDescriptor.verification / .rollback) — never
    // fabricated nulls for performative completeness.
    {
        let risk_level = server.check_tool_risk(&resolved.tool_name).await;
        if risk_level != crate::registry::tools::RiskLevel::Low {
            let desc = server.matcher.available_guilds()
                .iter()
                .find(|d| d.name == resolved.guild_name)
                .cloned();
            let performed_ok = result.is_error != Some(true);
            let result_text: String = result.content.iter()
                .filter_map(|c| c.as_text())
                .map(|t| t.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let changed: String = if performed_ok {
                let preview: String = result_text.chars().take(400).collect();
                if preview.is_empty() { "ok".to_string() } else { preview }
            } else {
                let preview: String = result_text.chars().take(200).collect();
                format!("call failed: {}", if preview.is_empty() { "unknown error" } else { &preview })
            };

            let mut report = serde_json::json!({
                "intended": intent,
                "performed": format!("guild='{}' tool='{}' ({})", resolved.guild_name, resolved.tool_name, if performed_ok { "ok" } else { "error" }),
                "changed": changed,
            });
            if let Some(d) = &desc {
                if let Some(verification) = &d.verification {
                    report["verified"] = serde_json::json!({
                        "method": verification,
                        "status": if performed_ok { "passed" } else { "failed" },
                    });
                }
                // Reversible action: register in undo_log for undo_last_action.
                // Only when the guild declares a real rollback contract AND the
                // call succeeded — you can only undo something that happened.
                if let Some(rollback_spec) = &d.rollback
                    && performed_ok
                    && let Ok(mut log) = server.undo_log.lock() {
                        log.insert(
                            agent_id.clone().unwrap_or_else(|| "anonymous".to_string()),
                            super::types::ReversibleAction {
                                guild: resolved.guild_name.clone(),
                                tool: resolved.tool_name.clone(),
                                rollback_spec: rollback_spec.clone(),
                                intent: intent.clone(),
                                result_text: result_text.clone(),
                                ts_unix: chrono::Utc::now().timestamp(),
                            },
                        );
                        report["rollback"] = serde_json::json!({
                            "spec": rollback_spec,
                            "recorded": true,
                            "note": "Call undo_last_action (or 'undo last action') to revert.",
                        });
                    }
            }
            result.content.push(Content::text(format!(
                "\n\n---\nACTION_REPORT: {}",
                serde_json::to_string_pretty(&report).unwrap_or_default()
            )));
        }
    }

    let footer = format!("\n\n---\nRouting: guild={} tool={}\nRouting Trace:\n - {}", resolved.guild_name, resolved.tool_name, resolved.routing_trace.join("\n - "));
    result.content.push(rmcp::model::Content::text(footer));
    Ok(result)
}

/// D-16: extrae el `plan_id` de la instrucción que el modo plan emite al
/// cliente. La única forma documentada es
/// `approve action for plan <id>` y nada más (T-DSH-9 F3: la variante corta
/// `approve plan <id>` se retiró, el kernel solo documenta la larga); la frase debe EMPEZAR así (un `approve plan x` incrustado en
/// otra frase no aprueba nada y cae al router semántico como siempre).
fn plan_id_from_approve_intent(lower_intent: &str) -> Option<String> {
    const PREFIXES: [&str; 1] = ["approve action for plan "];
    let rest = PREFIXES
        .iter()
        .find_map(|prefix| lower_intent.trim_start().strip_prefix(prefix))?;
    let id: String = rest
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    (!id.is_empty()).then_some(id)
}

/// Test helper: minimal TylluanServer with in-memory stores, no guilds.
/// Used by both handler_do's own tests and background_jobs tests.
#[cfg(test)]
pub(crate) async fn base_test_server(silva: std::sync::Arc<crate::memory::silva::SilvaDB>) -> TylluanServer {
    use tokio::sync::broadcast;
    use crate::router::matcher::GuildMatcher;
    use crate::router::catalog::builtin_catalog;
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
        silva.clone(),
        Arc::new(std::sync::Mutex::new(crate::curriculum::CurriculumLearner::new_in_memory(5).unwrap())),
    ));
    let mut server = TylluanServer::new(
        registry,
        Arc::new(matcher),
        silva,
        Arc::new(Mailbox::in_memory().await.unwrap()),
        doctor,
        node_router,
    );
    server.set_notifier(tx);
    server
}

#[cfg(test)]
mod tests;
