//! D-22b — efectos posteriores a la llamada de `tylluan_do`.
//!
//! Movido tal cual desde `handler_do/mod.rs` (mismo texto, salvo `use` y
//! visibilidad): `post_process_outcome` (Stage 3), `persist_remember` (Stage 4)
//! y sus helpers de friccion y leccion. `mod.rs` los llama por `outcome::`.

use rmcp::model::*;
use tracing::{info, warn};

use super::TylluanServer;
use super::audit::{audit_or_warn, routing_failure_id};
use super::distill_for_embedding;
use super::handle_tylluan_do;
use super::routing::{maybe_auto_extract_triples, record_activity_trace, run_agent_handshake};
use crate::transport::server::utils::extract_path_from_intent;

/// Auto-capture friction events from guild tool call results.
/// Parses common error patterns and logs them to the friction tracking system.
fn capture_friction_from_result(intent: &str, guild: &str, tool: &str, result_text: &str) {
    let lower = result_text.to_lowercase();
    let preview: String = result_text.chars().take(100).collect();

    if result_text.contains("BLOCKED:") {
        // Extract the quoted command, e.g. "BLOCKED: 'list' is not in..." -> "list"
        let command = result_text
            .split("BLOCKED:").nth(1)
            .and_then(|rest| rest.split('\'').nth(1))
            .unwrap_or(guild);
        crate::security::friction_log::log_blocklist_rejection(intent, command);
    } else if lower.contains("error:") && (lower.contains("guild") || lower.contains("tool")) {
        crate::security::friction_log::log_routing_mismatch(intent, guild, tool, &preview);
    }
}

pub(super) fn penalize_lesson_fn(intent: &str, silva: std::sync::Arc<crate::memory::silva::SilvaDB>) {
    let intent_lower = intent.to_lowercase();
    let words: Vec<_> = intent_lower.split_whitespace().take(3).collect();
    let lesson_key = format!("lesson:intent:{}", words.join("_"));
    tokio::spawn(async move {
        if let Ok(Some(_node)) = silva.get_node(&lesson_key).await {
            let _ = silva.touch_node(&lesson_key, "system", "rejected").await;
        }
    });
}

/// Stage 3: post-process the outcome — reactive cascade, agent profiles,
/// audit logging, anchor learning, lesson drain, routing feedback, reputation.
/// Returns the (possibly cascade-replaced) result and success flag.
/// 8 params is over clippy's default threshold; this is a private, single-caller
/// helper extracted straight out of handle_tylluan_do's body, not a public API --
/// bundling into a params struct isn't worth the churn for the marginal benefit.
#[allow(clippy::too_many_arguments)]
pub(super) async fn post_process_outcome(
    server: &TylluanServer,
    intent: &str,
    guild_name: &str,
    tool_name: &str,
    agent_id: &Option<String>,
    remember: bool,
    // WS3: the Scheduler's verdict carried unused from Stage 1 (`None` on
    // the plan path — plans never execute, nothing to pair). Consumed only
    // by the observation-only confusion collector below; never by dispatch.
    scheduler_decision: Option<crate::router::scheduler::types::SchedulingDecision>,
    mut result: CallToolResult,
    latency_ms: u64,
) -> (CallToolResult, bool) {
    let mut is_success = result.is_error != Some(true)
        && !result.content.iter().filter_map(|c| c.as_text())
            .any(|t| t.text.contains("Exit code:") && !t.text.contains("Exit code: 0"));

    let mut cascade_fired = false;
    if !is_success && guild_name != "coordinator" {
        let c_score = crate::router::complexity::score_complexity(intent);
        let registry_has_coordinator = server.registry.read().await.guilds.contains_key("coordinator");
        // WS2 delegation gate: same rule as the Proactive Cascade — a failed
        // complex intent only escalates to coordinator if it actually asked
        // for delegation/orchestration (or the caller is a coordinator worker).
        let is_coordinator_worker = agent_id.as_deref().is_some_and(|a| a.starts_with("coordinator"));
        let cascade_eligible = crate::router::complexity::coordinator_eligible(
            c_score,
            intent,
            false,
            is_coordinator_worker,
        );
        if cascade_eligible && registry_has_coordinator {
            info!("🔄 Reactive Cascade (score={:.2}): '{}' failed on '{}' → fallback to coordinator", c_score, intent, guild_name);
            let mut new_args = serde_json::Map::new();
            new_args.insert("intent".to_string(), serde_json::Value::String(intent.to_string()));
            new_args.insert("guild".to_string(), serde_json::Value::String("coordinator".to_string()));
            if let Some(aid) = agent_id {
                new_args.insert("agent_id".to_string(), serde_json::Value::String(aid.clone()));
            }
            new_args.insert("remember".to_string(), serde_json::Value::Bool(remember));

            match Box::pin(handle_tylluan_do(server, Some(new_args))).await {
                Ok(cascade_res) => {
                    info!("🔄 Reactive Cascade successful for '{}'", intent);
                    result = cascade_res;
                    cascade_fired = true;
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

    // WS3 confusion collector — OBSERVATION ONLY. Pairs the Scheduler's
    // verdict (carried unused from Stage 1; None on the plan path and on
    // cascade re-entry, where the inner dispatch records its own row) with
    // what actually happened here: routed guild/tool, final guild after any
    // reactive cascade, cascade flag, success. One complete row per
    // completed dispatch, fire-and-forget; failures are logged and never
    // propagate. NOTHING reads the store back into dispatch — cutover is a
    // separate Tech Lead decision on accumulated tallies, not this code.
    if let Some(decision) = scheduler_decision {
        let final_guild_str: &str = if cascade_fired { "coordinator" } else { guild_name };
        let rec = crate::router::scheduler::confusion::ConfusionRecord {
            intent,
            agent_id: agent_id.as_deref(),
            decision: &decision,
            routed_guild: guild_name,
            routed_tool: tool_name,
            final_guild: final_guild_str,
            cascade_fired,
            success: is_success,
        };
        if let Err(e) = crate::router::scheduler::confusion::record_dispatch_pair(&rec) {
            warn!("scheduler confusion record failed (non-fatal): {e}");
        }
    }

    server.matcher.record_outcome(intent, guild_name, is_success, latency_ms);

    if !is_success {
        penalize_lesson_fn(intent, server.silva.clone());
    }

    let is_new = if let Some(profiles) = &server.agent_profiles {
        if let Some(aid) = agent_id {
            if let Ok(p_store) = profiles.lock() {
                let _ = p_store.upsert_activity(aid, guild_name, is_success, Some(intent));
                if !is_success
                    && let Ok(Some(best)) = p_store.get_best_agent_for_domain(guild_name) {
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

    if is_new && let Some(aid) = agent_id { run_agent_handshake(server, aid).await; }

    server.notify("tool_call", serde_json::json!({
        "status": "finished", "tool": tool_name,
        "agent_id": agent_id.as_deref().unwrap_or("anonymous"),
        "intent": intent, "ok": is_success, "ts": chrono::Utc::now().timestamp_millis()
    }));

    if !is_success && let Ok(mut h) = server.hormones.lock() { h.emit_stress(agent_id.as_deref().unwrap_or("unknown")); }

    let result_text = result.content.iter().filter_map(|c| c.as_text())
        .map(|t| t.text.clone()).next().unwrap_or_default();

    maybe_auto_extract_triples(server, agent_id.as_deref(), guild_name, &result_text);

    if let Some(aid) = agent_id {
        record_activity_trace(server, aid, guild_name, tool_name, result_text.len());
    }

    let audit_intent = intent.to_string();
    let audit_guild = guild_name.to_string();
    let audit_tool = tool_name.to_string();
    // D-28: una llamada sin `agent_id` deja `anonymous` en el audit (igual que
    // las demas rutas de entrada), no una cadena vacia. Sin canonicalizar nada.
    let audit_agent = agent_id.clone().unwrap_or_else(|| "anonymous".to_string());
    let audit_success = is_success;
    let audit_preview = result_text.chars().take(200).collect::<String>();
    // Autonomous Success Rate prerequisite: full intent->result cycle latency
    // (t0 at fn start, latency_ms at :1029) + HITL flag. human_intervention is
    // always false on this direct path — the HITL approve_action path logs its
    // own audit entry with human_intervention=true (see handlers.rs approve_action).
    let audit_latency_ms = latency_ms;
    let audit_hitl = false;
    tokio::task::spawn_blocking(move || {
        audit_or_warn(&audit_intent, &audit_guild, &audit_tool, &audit_agent, audit_success, &audit_preview, audit_latency_ms, audit_hitl);
        capture_friction_from_result(&audit_intent, &audit_guild, &audit_tool, &audit_preview);
    });

    if is_success && !intent.trim().is_empty() {
        let path = extract_path_from_intent(intent);
        if path != "." {
            let silva_p = server.silva.clone();
            let aid_p = agent_id.as_deref().unwrap_or("unknown").to_string();
            tokio::spawn(async move {
                let _ = silva_p.record_work_trace(&path, "file", &aid_p, "tool_call", 1.0).await;
            });
        }

        let silva_anchor = server.silva.clone();
        let engine_anchor = server.matcher.engine_arc();
        let intent_anchor = intent.to_string();
        let guild_anchor = guild_name.to_string();
        tokio::spawn(async move {
            let embedding = engine_anchor.as_ref()
                .and_then(|e| tokio::task::block_in_place(|| e.embed_batch_coalesced_routing(&intent_anchor)).ok()
                    .map(|emb| (emb, e.engine_id())));
            let _ = silva_anchor.upsert_routing_anchor(
                &guild_anchor, &intent_anchor, "learned",
                embedding.as_ref().map(|(v, m)| (v.as_slice(), m.as_str())),
            ).await;
        });
    }

    if let Some(aid) = agent_id
        && let Some(profiles) = &server.agent_profiles {
            let p_store = profiles.clone();
            let silva_reput = server.silva.clone();
            let aid_clone = aid.clone();
            tokio::spawn(async move {
                let profile_opt = {
                    if let Ok(store) = p_store.lock() {
                        store.get_profile(&aid_clone).unwrap_or(None)
                    } else { None }
                };
                if let Some(prof) = profile_opt {
                    crate::memory::agent_profile::sync_agent_reputation_to_silva(
                        &silva_reput, &[prof]
                    ).await;
                }
            });
        }

    if !is_success || result_text.trim().is_empty() {
        let err_msg = if !is_success {
            let t = result_text.clone();
            if t.chars().count() > 100 { format!("{}...", t.chars().take(100).collect::<String>()) } else { t }
        } else { "EMPTY_RESULT".to_string() };
        let failure_id = routing_failure_id(intent);
        let failure_content = format!(
            "ROUTING_FAILURE guild={} intent={} error={}",
            guild_name, &intent[..intent.len().min(100)], err_msg
        );
        let _ = server.silva.upsert_node(
            &failure_id, "lesson", &failure_content, "{}"
        ).await;
        let _ = server.silva.touch_node(&failure_id, "system", "routing_failure").await;
    }

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

    if is_success {
        let lesson_key = format!("lesson:intent:{}",
            intent.to_lowercase()
                .split_whitespace().take(3).collect::<Vec<_>>().join("_"));

        let should_write_lesson = match server.silva.get_node(&lesson_key).await {
            Ok(Some(existing)) => existing.weight < 0.5,
            Ok(None) => true,
            Err(_) => true,
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
                "source": "routing_lesson", "guild": guild_name,
                "tool": tool_name, "intent": intent
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

    (result, is_success)
}

/// Stage 4: persist remember — write episode to memory and SilvaDB graph.
pub(super) async fn persist_remember(
    server: &TylluanServer,
    result: &CallToolResult,
    agent_id: &Option<String>,
    intent: &str,
    guild_name: &str,
    tool_name: &str,
    remember: bool,
) {
    let should_remember = remember || agent_id.is_some();
    if !should_remember || result.is_error == Some(true) {
        return;
    }

    let output_preview = result.content.first()
        .and_then(|c| c.as_text()).map(|t| t.text.chars().take(300).collect::<String>())
        .unwrap_or_default();
    let trace = match agent_id {
        Some(aid) => format!("tylluan_do episode | agent: {aid} | intent: {intent} | guild: {guild_name} | tool: {tool_name} | result: {output_preview}"),
        None => format!("tylluan_do episode | intent: {intent} | guild: {guild_name} | tool: {tool_name} | result: {output_preview}"),
    };
    let meta = serde_json::json!({ "source": "tylluan_do", "guild": guild_name, "tool": tool_name, "agent_id": agent_id.as_deref().unwrap_or("anonymous") }).to_string();
    let embedding_target = distill_for_embedding(intent, &output_preview);
    let embedded = server.matcher.engine()
        .and_then(|e| tokio::task::block_in_place(|| e.embed(&embedding_target)).ok()
            .map(|emb| (emb, e.engine_id())));
    // F1 ROADMAP_O3:58: dual-write híbrido cerrado — el episodio se persiste solo
    // en SilvaDB (upsert + embedding, más abajo).
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
    if let Some((emb, model_id)) = embedded.as_ref()
        && let Err(e) = server.silva.save_embedding(&node_id, emb, model_id, None).await {
            warn!("⚠️ tylluan_do remember: embedding save failed for {}: {}", node_id, e);
        }
    server.notify("memory_added", serde_json::json!({
        "node_id": node_id, "type": "episode",
        "label": trace.chars().take(100).collect::<String>(),
        "ts": chrono::Utc::now().timestamp_millis()
    }));
}
