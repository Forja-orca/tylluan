use rmcp::{Error as McpError, model::*};
use serde_json;
use chrono;

use crate::registry::proxy::error_result;
use super::TylluanServer;
use crate::memory::mailbox::BlackboardMessage;
use rusqlite::params;

fn clean_operational_wrapper(content: &str) -> String {
    let trimmed = content.trim();
    if trimmed.starts_with("tylluan_do episode") {
        let parts: Vec<&str> = trimmed.split(" | ").collect();
        let mut agent = None;
        let mut intent = None;
        let mut result = None;
        for part in &parts {
            if let Some(val) = part.strip_prefix("agent: ") {
                agent = Some(val.trim());
            } else if let Some(val) = part.strip_prefix("intent: ") {
                intent = Some(val.trim());
            } else if let Some(val) = part.strip_prefix("result: ") {
                result = Some(val.trim());
            }
        }
        if intent.is_some() || result.is_some() {
            let mut clean = String::new();
            if let Some(a) = agent {
                clean.push_str(&format!("[Agente: {}] ", a));
            }
            if let Some(i) = intent {
                clean.push_str(&format!("Acción: {}\n", i));
            }
            if let Some(r) = result {
                clean.push_str(&format!("Resultado: {}", r));
            }
            if !clean.is_empty() {
                return clean;
            }
        }
    }
    content.to_string()
}

pub async fn handle_tylluan_remember(
    server: &TylluanServer,
    arguments: Option<serde_json::Map<String, serde_json::Value>>,
) -> Result<CallToolResult, McpError> {
    let raw_content = arguments.as_ref()
        .and_then(|a| a.get("content")).and_then(|v| v.as_str()).unwrap_or("").to_string();
    if raw_content.trim().is_empty() {
        return Ok(error_result("tylluan_remember requires a non-empty 'content' argument."));
    }
    let content = clean_operational_wrapper(&raw_content);
    let coloquio_post = if content.starts_with("@coloquio:") {
        let rest = content.strip_prefix("@coloquio:").unwrap_or("");
        let parts: Vec<&str> = rest.splitn(2, ':').collect();
        if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            Some((parts[0].trim().to_string(), parts[1].trim().to_string()))
        } else {
            None
        }
    } else {
        None
    };
    let (content, coloquio_channel) = if let Some((channel_id, msg)) = coloquio_post {
        (msg, Some(channel_id))
    } else {
        (content, None)
    };
    let rem_agent_id = arguments.as_ref()
        .and_then(|a| a.get("agent_id")).and_then(|v| v.as_str())
        .map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let metadata = {
        let mut m: serde_json::Value = arguments.as_ref()
            .and_then(|a| a.get("metadata"))
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        if let Some(aid) = &rem_agent_id
            && let Some(obj) = m.as_object_mut() {
                obj.insert("agent_id".to_string(), serde_json::Value::String(aid.clone()));
            }
        m.to_string()
    };

    let metadata_val = arguments.as_ref()
        .and_then(|a| a.get("metadata"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    let is_pending_task = metadata_val.get("status")
        .and_then(|v| v.as_str())
        .map(|s| s == "pending")
        .unwrap_or(false);

    let task_to_complete = metadata_val.get("completes_task")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Calculate valid_until from expires_in_days parameter
    let expires_in_days: Option<u32> = arguments
        .as_ref()
        .and_then(|a| a.get("expires_in_days"))
        .and_then(|v| v.as_u64())
        .map(|d| d as u32);

    let valid_until: Option<i64> = expires_in_days.map(|days| {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
        now + (days as i64 * 86400)
    });

    // Post to coloquio if @coloquio: prefix was used (before DCR — post even if duplicate)
    if let Some(ref channel_id) = coloquio_channel {
        if let Some(ref coloquio) = server.coloquio {
            let author = rem_agent_id.as_deref().unwrap_or("agent");
            if let Err(e) = coloquio.post_message(channel_id, author, "agent", &content, "{}").await {
                tracing::warn!("⚠️ tylluan_remember: coloquio post to #{} failed: {}", channel_id, e);
            }
        } else {
            tracing::warn!("⚠️ tylluan_remember: coloquio not available, cannot post to #{}", channel_id);
        }
    }

    // DCR — Dedup Check Remember (R21-3)
    // Compute embedding early to detect near-duplicates before writing to silva.
    // Threshold 0.87: blocks content that is semantically similar (broader dedup than 0.93).
    // On hit: reinforces the existing node and returns it instead of creating a duplicate.
    const DCR_THRESHOLD: f32 = 0.87;
    let early_embedding = server.matcher.engine()
        .and_then(|e| e.embed(content.trim()).ok());
    if let Some(ref emb) = early_embedding
        && let Ok(candidates) = server.silva.search_vector(emb, 3).await
            && let Some((existing_node, sim)) = candidates.into_iter().find(|(_, s)| *s >= DCR_THRESHOLD) {
                tracing::info!(
                    "🔄 DCR: near-duplicate blocked (sim={:.3}) existing={} agent={}",
                    sim, existing_node.id,
                    rem_agent_id.as_deref().unwrap_or("anonymous")
                );
                let _ = server.silva.reinforce_node(&existing_node.id, 1.05).await;
                let preview = existing_node.content.chars().take(100).collect::<String>();
                return Ok(CallToolResult {
                    content: vec![Content::text(format!(
                        "DCR: near-duplicate detected (similarity={:.0}%). Reinforced existing node '{}' instead of creating duplicate.\nExisting: \"{}\"",
                        sim * 100.0, existing_node.id, preview
                    ))],
                    is_error: Some(false),
                });
            }

    if let Some(ref completes_id) = task_to_complete {
        let aid = rem_agent_id.as_deref().unwrap_or("anonymous");
        let conn_guard = server.silva.conn_lock();
        if let Ok(conn) = conn_guard.try_lock() {
            let now = chrono::Utc::now().to_rfc3339();
            let _ = conn.execute(
                "UPDATE nodes SET metadata = json_set(COALESCE(metadata, '{}'), '$.status', 'completed', '$.completed_by', ?1, '$.completed_at', ?2) WHERE id = ?3",
                params![aid, now, completes_id],
            );
        }
    }

    let tagged_content = match &rem_agent_id {
        Some(aid) => format!("memory | agent: {} | {}", aid, content),
        None => content.clone(),
    };
    let node_id = if let Some(ref aid) = rem_agent_id {
        let importance = arguments.as_ref()
            .and_then(|a| a.get("metadata")).and_then(|m| m.get("importance"))
            .and_then(|v| v.as_f64()).unwrap_or(0.7);

        let nid = if is_pending_task {
            let task_id = format!("task:{}", chrono::Utc::now().timestamp_millis());
            let assigned_to = metadata_val.get("assigned_to")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let priority = metadata_val.get("priority")
                .and_then(|v| v.as_i64())
                .unwrap_or(5);
            let task_type = metadata_val.get("task_type")
                .and_then(|v| v.as_str())
                .unwrap_or("general");

            let mut task_meta = serde_json::json!({
                "status": "pending",
                "created_by": aid,
                "priority": priority,
                "task_type": task_type,
            });
            if let Some(assign) = assigned_to {
                task_meta["assigned_to"] = serde_json::Value::String(assign);
            }

            let _ = server.silva.upsert_node(&task_id, "task", &content, &task_meta.to_string()).await;
            let _ = server.silva.reinforce_node(&task_id, 1.1).await;
            let _ = server.silva.touch_node(&task_id, aid, "task_created").await;
            tracing::info!("🧠 Memory reinforced (task): node={} agent={}", task_id, aid);
            task_id
        } else {
            let mgr = crate::memory::agent_memory::AgentMemoryManager::new(server.silva.clone(), 20);
            mgr.record_memory(aid, &content, importance).await
        };

        if importance > 0.7
            && let Ok(mut h) = server.hormones.lock() {
                h.emit_novelty(0.4);
            }

        if importance >= 0.9 {
            let _ = server.silva.protect_node(&nid).await;
        }

        if importance >= 0.85 {
            let mailbox_clone = server.mailbox.clone();
            let aid_clone = aid.clone();
            let content_clone = content.chars().take(300).collect::<String>();
            let imp_clone = importance;
            let known_agents: Vec<String> = server.agent_profiles.as_ref()
                .and_then(|ap| ap.lock().ok())
                .and_then(|p| p.list_profiles().ok())
                .unwrap_or_default()
                .into_iter()
                .map(|p| p.agent_id)
                .filter(|id| id != aid)
                .collect();
            tokio::spawn(async move {
                let snippet = serde_json::json!({
                    "node_id": format!("memory:{}", chrono::Utc::now().timestamp_millis()),
                    "content": content_clone,
                    "importance": imp_clone,
                }).to_string();
                let _ = mailbox_clone.broadcast(&aid_clone, "knowledge_share", &snippet, &known_agents).await;
            });
        }

        let (real_calls, first_seen) = server.agent_profiles.as_ref()
            .and_then(|ap| ap.lock().ok())
            .and_then(|p| p.get_profile(aid).ok().flatten())
            .map(|prof| (prof.total_calls, prof.first_seen))
            .unwrap_or((0, String::new()));
        let _ = server.silva.update_agent_identity(aid, aid, real_calls as usize, &first_seen).await;
        let agent_node_id = format!("agent:{}", aid);
        let _ = server.silva.add_edge(&agent_node_id, &nid, "remembers", 0.9, "{}").await;
        let _ = server.silva.touch_node(&agent_node_id, aid, "identity-refresh").await;

        let silva_clone = server.silva.clone();
        let aid_clone = aid.clone();
        tokio::spawn(async move {
            let mgr2 = crate::memory::agent_memory::AgentMemoryManager::new(silva_clone, 20);
            let _ = mgr2.consolidate_if_needed(&aid_clone).await;
        });
        nid
    } else {
        let nid = format!("memory:{}", chrono::Utc::now().timestamp_millis());
        if let Err(e) = server.silva.upsert_node_with_validity(&nid, "memory", &tagged_content, &metadata, valid_until, false).await {
            tracing::warn!("⚠️ tylluan_remember: silva graph write failed: {}", e);
        }
        nid
    };
    let aid = rem_agent_id.as_deref().unwrap_or("anonymous");
    if let Some(ref profiles) = server.agent_profiles {
        let is_new = {
            if let Ok(p_store) = profiles.lock() {
                p_store.is_new_agent(aid)
            } else { false }
        };

        if is_new {
            let node_id = format!("agent_identity_{}", aid);
            let meta = serde_json::json!({
                "agent_id": aid,
                "registered_at": chrono::Utc::now().to_rfc3339(),
                "role": "generalist"
            });
            let _ = server.silva.upsert_node(&node_id, "agent_identity", &format!("Agente {} registrado en el colectivo", aid), &meta.to_string()).await;
            let _ = server.silva.touch_node(&node_id, aid, "handshake").await;
            if let Ok(mut h) = server.hormones.lock() {
                h.emit_novelty(0.6);
            }
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
    }
    let _ = server.silva.reinforce_node(&node_id, 1.1).await;
    let _ = server.silva.touch_node(&node_id, aid, "remember").await;
    tracing::info!("🧠 Memory reinforced: node={} agent={}", node_id, aid);
    server.notify("memory_added", serde_json::json!({
        "node_id": node_id,
        "type": "memory",
        "label": tagged_content.chars().take(100).collect::<String>(),
        "ts": chrono::Utc::now().timestamp_millis()
    }));

    // Reuse early_embedding computed for DCR check (avoids double embedding call)
    let embedding = early_embedding.or_else(|| server.matcher.engine().and_then(|e| e.embed(&tagged_content).ok()));
    if let Some(emb) = embedding.as_deref() {
        let _ = server.silva.save_embedding(&node_id, emb, "nomic", None).await;

        // TMS: deprecate contradictions after storing
        if let Ok(deprecated_count) = server.silva.deprecate_contradictions(&node_id, emb, &tagged_content).await
            && deprecated_count > 0 {
                tracing::info!("🧠 TMS: deprecated {} contradictions for node={}", deprecated_count, node_id);
            }
    }

    match server.memory.add_document(&tagged_content, &metadata, embedding.as_deref()).await {
        Ok(_) => {
            // Invalidate recall cache so new memories are immediately visible
            {
                let mut cache = server.recall_cache.lock().await;
                cache.invalidate_all();
            }

            let importance = arguments.as_ref()
                .and_then(|a| a.get("metadata")).and_then(|m| m.get("importance"))
                .and_then(|v| v.as_f64()).unwrap_or(0.7);
            if content.len() > 200 {
                let server_clone = server.clone();
                let aid_clone = rem_agent_id.clone();
                let content_clone = content.clone();
                let nid_clone = node_id.clone();
                let tagged_clone = tagged_content.clone();
                tokio::spawn(async move {
                    crate::transport::server::handler_do::maybe_auto_extract_triples(
                        &server_clone,
                        aid_clone.as_deref(),
                        "remember",
                        &content_clone,
                    );
                    
                    // Auto-link to semantically similar nodes (grows graph density)
                    let _ = server_clone.silva.auto_link_similar(&nid_clone, &tagged_clone, 3, 0.25).await;
                });
            } else {
                // Still auto-link even if content is short and no triples extracted
                let silva_clone = server.silva.clone();
                let nid_clone = node_id.clone();
                let tagged_clone = tagged_content.clone();
                tokio::spawn(async move {
                    let _ = silva_clone.auto_link_similar(&nid_clone, &tagged_clone, 3, 0.25).await;
                });
            }
            let preview = if content.chars().count() > 80 { format!("{}...", content.chars().take(80).collect::<String>()) } else { content.clone() };
            Ok(CallToolResult {
                content: vec![Content::text(format!("Stored node {} (importance={:.2}): \"{}\"", node_id, importance, preview))],
                is_error: Some(false),
            })
        },
        Err(e) => Ok(error_result(&format!("Memory write failed: {}", e))),
    }
}