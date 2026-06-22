use rmcp::model::CallToolResult;
use tracing::{info, warn};
use chrono;

use crate::registry::proxy::error_result;
use crate::router::matcher::GuildContext;
use crate::memory::mailbox::BlackboardMessage;
use rmcp::model::CallToolRequestParam;
use super::TylluanServer;

pub(super) async fn resolve_guild_name(
    server: &TylluanServer,
    intent: &str,
    guild_hint: Option<String>,
    agent_id: Option<&str>,
) -> Result<(String, Vec<String>), CallToolResult> {
    let mut trace = Vec::new();
    if let Some(hint) = guild_hint {
        // Known to the semantic matcher OR live in the registry — external MCPs
        // activated at runtime (M25-B) exist only in the registry until reboot.
        let known = server.matcher.available_guilds().iter().any(|g| g.name == hint)
            || server.registry.read().await.guilds.contains_key(&hint);
        if !known {
            trace.push(format!("Guild hint '{}' rejected (unknown guild)", hint));
            return Err(error_result(&format!(
                "Unknown guild '{}'. Use list_available_guilds to see valid options.",
                hint,
            )));
        }
        info!("🎯 tylluan_do: guild hint '{}' bypasses router", hint);
        trace.push(format!("guild_hint='{}' bypasses router", hint));
        Ok((hint, trace))
    } else {
        let intent_for_matching = if intent.chars().count() > 120 {
            intent.chars().take(120).collect::<String>()
        } else {
            intent.to_string()
        };

        let query_embedding = server.matcher.engine()
            .and_then(|engine| engine.embed(&intent_for_matching).ok());

        // Build agent context from role identifier if present
        let ctx = agent_id.map(GuildContext::from_agent_id);
        let ctx_ref = ctx.as_ref();

        if let Some(ref c) = ctx {
            tracing::debug!(
                agent_role = c.agent_role.as_deref().unwrap_or("none"),
                preferred_category = ?c.preferred_category,
                "tylluan_do: routing with agent context"
            );
        }

        // R14-3: Lesson prior — check SilvaDB for past successful routing before falling through to matcher
        let lesson_key = format!("lesson:intent:{}",
            intent_for_matching.to_lowercase()
                .split_whitespace()
                .take(3)
                .collect::<Vec<_>>()
                .join("_"));
        if let Ok(Some(node)) = server.silva.get_node(&lesson_key).await {
            let now_unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
            if node.weight >= 0.6 {
                // Verificar que la lección no es demasiado antigua (30 días)
                let age_days = (now_unix - node.last_touched.timestamp()) as f64 / 86400.0;
                if age_days > 30.0 {
                    // Lección expirada — reducir weight y no usarla
                    let silva_c = server.silva.clone();
                    let lk = lesson_key.clone();
                    tokio::spawn(async move {
                         let _ = silva_c.apply_node_decay(&lk).await;
                    });
                    // Fall through to normal matcher
                } else {
                    // R16-2: Success-rate check — deprecate if too many rejections
                    let window = now_unix - (7 * 86400);
                    let total = server.silva.get_trace_count_since(&lesson_key, window).await.unwrap_or(0);
                    let rejected = server.silva.get_trace_count_by_type(&lesson_key, "rejected", window).await.unwrap_or(0);
                    if total >= 5 && rejected as f64 / total as f64 > 0.5 {
                        info!("🎯 Lesson prior: '{}' deprecated (rejected={}/{})", intent, rejected, total);
                        let silva_c = server.silva.clone();
                        let lk = lesson_key.clone();
                        tokio::spawn(async move {
                            let _ = silva_c.touch_node(&lk, "system", "deprecated").await;
                            let _ = silva_c.apply_node_decay(&lk).await;
                        });
                        // Fall through to normal matcher
                    } else {
                        if let Some(guild) = node.content.split_whitespace()
                            .find_map(|w| w.strip_prefix("guild:"))
                            && server.matcher.available_guilds().iter().any(|g| g.name == guild) {
                                // Trigger override: if a catalog trigger phrase matches a DIFFERENT guild,
                                // it means the lesson is stale (guild was added after the lesson was written).
                                if let Some(trigger) = server.matcher.trigger_match_pub(&intent_for_matching)
                                    && trigger.score >= 0.7 && trigger.guild_name != guild {
                                        info!("⚡ Trigger overrides stale lesson: '{}' → {} (was: {})", intent, trigger.guild_name, guild);
                                        trace.push(format!("lesson overridden by trigger={}", trigger.guild_name));
                                        return Ok((trigger.guild_name, trace));
                                    }
                                info!("🎯 Lesson prior: '{}' → guild='{}' (weight={})", intent, guild, node.weight);
                                trace.push(format!("lesson_prior match='{}' (weight={})", guild, node.weight));
                                return Ok((guild.to_string(), trace));
                            }
                    }
                }
            }
        }

        // Trigger fast-path: fires AFTER lesson falls through — catches IQE/RFL-contaminated intents
        // Uses original `intent` (pre-IQE) so injected context can't poison trigger matching
        if let Some(trigger) = server.matcher.trigger_match_pub(&intent_for_matching)
            && trigger.score >= 0.85 {
                info!("⚡ Trigger fast-path (post-lesson): '{}' → {} (score={:.2})", intent, trigger.guild_name, trigger.score);
                trace.push(format!("trigger_fast_path='{}' score={:.2}", trigger.guild_name, trigger.score));
                return Ok((trigger.guild_name, trace));
            }

        // Anchor fast-path: SilvaDB routing_anchor nodes beat semantic matcher when confident
        if let Some(ref emb) = query_embedding
            && let Ok(anchor_results) = server.silva.match_by_anchors(emb, 3).await
                && let Some((best_guild, best_score)) = anchor_results.first() {
                    let second_score = anchor_results.get(1).map(|(_, s)| *s).unwrap_or(0.0);
                    let gap = best_score - second_score;
                    if (*best_score >= 0.88 || (*best_score >= 0.70 && gap >= 0.05))
                        && server.matcher.available_guilds().iter().any(|g| &g.name == best_guild) {
                            info!("⚓ Anchor fast-path: '{}' → {} (score={:.3}, gap={:.3})", intent, best_guild, best_score, gap);
                            trace.push(format!("anchor_fast_path='{}' score={:.3} gap={:.3}", best_guild, best_score, gap));
                            return Ok((best_guild.clone(), trace));
                        }
                }

        match server.matcher.match_guild(&intent_for_matching, query_embedding.as_deref(), 0.25, ctx_ref) {
            Some(m) => {
                // RFL Guard (R20-1): check for recorded routing failures
                let failure_id = super::routing_failure_id(&intent_for_matching);
                if let Ok(Some(node)) = server.silva.get_node(&failure_id).await
                    && node.weight >= 0.4
                        && let Some(blocked_guild) = node.content.split_whitespace()
                            .find_map(|w| w.strip_prefix("guild=").map(|g| g.to_string()))
                            && blocked_guild == m.guild_name {
                                // Try trigger_match as fallback before giving up
                                if let Some(trigger) = server.matcher.trigger_match_pub(&intent_for_matching)
                                    && trigger.guild_name != blocked_guild && trigger.score >= 0.7 {
                                        info!("🔀 RFL fallback: '{}' blocked → trigger → {}", blocked_guild, trigger.guild_name);
                                        trace.push(format!("rfl_fallback trigger='{}'", trigger.guild_name));
                                        return Ok((trigger.guild_name, trace));
                                    }
                                return Err(error_result(&format!(
                                    "Routing BLOCKED: guild '{}' previously failed for this intent (weight={:.0}%). Try a different phrasing.",
                                    m.guild_name, node.weight * 100.0
                                )));
                            }

                // Confidence gate: reject low-confidence routing to prevent silent misfires
                const MIN_CONFIDENCE: f32 = 0.20;
                if m.score < MIN_CONFIDENCE {
                    return Err(error_result(&format!(
                        "Error: intent '{}' unclear. Closest guild was '{}' \
                         but confidence too low ({:.0}%). \
                         Try being more specific, e.g.: \
                         'run command X', 'search files Y', 'show git status'.",
                        intent, m.guild_name, m.score * 100.0
                    )));
                }

                // Log routing decision with agent context for observability
                info!(
                    "🎯 Routing: '{}' → guild='{}' method={:?} score={:.3} agent_role={}",
                    intent, m.guild_name, m.method, m.score,
                    agent_id.unwrap_or("anonymous")
                );
                trace.push(format!("semantic_match='{}' score={:.3} method={:?}", m.guild_name, m.score, m.method));
                Ok((m.guild_name, trace))
            }
            None => {
                let msg = format!(
                    "NO_GUILD_MATCH: No guild found for intent: '{}'. \
                     Tried guild-context routing (agent_role={}). \
                     Register a guild or use a clearer description.",
                    intent,
                    agent_id.unwrap_or("anonymous")
                );
                server.notify("routing_failure", serde_json::json!({
                    "intent": intent, "error": msg,
                    "agent_id": agent_id.unwrap_or("anonymous"),
                    "ts": chrono::Utc::now().timestamp_millis()
                }));
                Err(error_result(&msg))
            }
        }
    }
}

pub(super) async fn run_agent_handshake(server: &TylluanServer, aid: &str) {
    let node_id = format!("agent_identity_{}", aid);
    let meta = serde_json::json!({
        "agent_id": aid,
        "registered_at": chrono::Utc::now().to_rfc3339(),
        "role": "generalist"
    }).to_string();
    let silva = server.silva.clone();
    let aid_c = aid.to_string();
    tokio::spawn(async move {
        let _ = silva.upsert_node(&node_id, "agent_identity", &format!("Agente {} registrado en el colectivo", aid_c), &meta).await;
        let _ = silva.touch_node(&node_id, &aid_c, "handshake").await;
    });
    if let Ok(mut h) = server.hormones.lock() { h.emit_novelty(0.6); }

    let msg = BlackboardMessage {
        msg_type: "welcome".into(),
        body: format!("Bienvenido al colectivo, {}. Tu identidad ha sido registrada.", aid),
        to: aid.to_string(),
        from: "kernel".into(),
        thread_id: None,
        priority: 3,
    };
    let _ = server.mailbox.send_mail("kernel", aid, &msg.to_payload()).await;
}

pub(super) fn record_activity_trace(server: &TylluanServer, aid: &str, guild_name: &str, tool_name: &str, result_text_len: usize) {
    if result_text_len <= 100 { return; }
    let edge_meta = serde_json::json!({
        "tool": tool_name,
        "timestamp": chrono::Utc::now().to_rfc3339()
    }).to_string();
    let silva = server.silva.clone();
    let aid_s = aid.to_string();
    let gn = guild_name.to_string();
    let aid_s2 = aid_s.clone();
    let gn2 = gn.clone();
    tokio::spawn(async move {
        match silva.strengthen_edge(&aid_s2, &gn2, "executed", 0.15).await {
            Ok(true) => info!("🌲 tylluan_do: strengthened edge: {} -[executed]-> {} (+0.15)", aid_s2, gn2),
            _ => {
                if let Err(e) = silva.add_edge(&aid_s2, &gn2, "executed", 1.0, &edge_meta).await {
                    warn!("⚠️ tylluan_do: failed to create edge in SilvaDB: {}", e);
                } else {
                    info!("🌲 tylluan_do: edge created: {} -[executed]-> {}", aid_s2, gn2);
                }
            }
        }
    });
    server.edge_added(&aid_s, &gn, "executed", 1.0);
}

pub(crate) fn maybe_auto_extract_triples(server: &TylluanServer, agent_id: Option<&str>, guild_name: &str, result_text: &str) {
    if result_text.len() <= 200 { return; }
    let skip = guild_name == "knowledge" || guild_name == "bash" || guild_name == "docker";
    if skip { return; }
    let snippet: String = result_text.chars().take(500).collect();
    let reg_c = std::sync::Arc::clone(&server.registry);
    let silva_c = std::sync::Arc::clone(&server.silva);
    let aid_c = agent_id.map(|s| s.to_string());
    tokio::spawn(async move {
        let extract_args = serde_json::json!({ "text": snippet, "max_triples": 5, "intent": snippet, "query": snippet });
        let call_params = CallToolRequestParam {
            name: "extract_triples".into(),
            arguments: Some(extract_args.as_object().cloned().unwrap_or_default()),
        };
        let triple_json = {
            let mut reg = reg_c.write().await;
            if let Some(guild) = reg.guilds.get_mut("knowledge") {
                // Skip if guild is not alive — avoids "disconnected" error spam
                if !guild.is_running() {
                    return;
                }
                guild.call_tool(call_params).await.content.into_iter()
                    .filter_map(|c| c.as_text().map(|t| t.text.clone()))
                    .next().unwrap_or_default()
            } else { String::new() }
        };
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&triple_json)
            && let Some(triples) = parsed["triples"].as_array() {
                for triple in triples {
                    let conf = triple["confidence"].as_f64().unwrap_or(0.0);
                    if conf >= 0.45 {
                        let subj = triple["subject"].as_str().unwrap_or("");
                        let pred = triple["predicate"].as_str().unwrap_or("relates_to");
                        let obj = triple["object"].as_str().unwrap_or("");
                        if !subj.is_empty() && !obj.is_empty() {
                            let meta = serde_json::json!({"source": "auto_extract", "confidence": conf}).to_string();
                            if silva_c.add_edge(subj, obj, pred, conf, &meta).await.is_ok() {
                                let aid = aid_c.as_deref().unwrap_or("anonymous");
                                let _ = silva_c.touch_node(subj, aid, "auto-triple").await;
                                let _ = silva_c.touch_node(obj, aid, "auto-triple").await;
                                info!("🌿 auto-triple: {} -[{}]-> {}", subj, pred, obj);
                            }
                        }
                    }
                }
            }
    });
}
