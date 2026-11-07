use rmcp::{Error as McpError, model::*};
use serde_json;
use chrono;

use crate::registry::proxy::error_result;
use super::TylluanServer;
use crate::memory::mailbox::BlackboardMessage;
use rusqlite::params;

/// T-DSH-95: tope de tamaño del contenido fusionado por DCR. Si la fusión
/// supera este tamaño se mantiene el comportamiento anterior (no se fusiona)
/// para no crecer sin límite.
pub(crate) const DCR_MERGE_MAX_CHARS: usize = 16_000;

/// T-DSH-95: el DCR (umbral 0.87) fusionaba una nota nueva en la vieja
/// reforzando el nodo existente, pero **descartaba el texto entrante**: la
/// información nueva no se guardaba en ningún sitio. Esta función construye el
/// contenido fusionado (append con marca) o `None` si el texto ya está
/// contenido o el resultado excede el tope.
pub(crate) fn merge_near_duplicate_content(existing: &str, incoming: &str) -> Option<String> {
    let incoming = incoming.trim();
    if incoming.is_empty() || existing.contains(incoming) {
        return None;
    }
    let merged = format!("{existing}\n\n[actualización] {incoming}");
    if merged.chars().count() > DCR_MERGE_MAX_CHARS {
        return None;
    }
    Some(merged)
}

/// T-DSH-154: qué hacer con un candidato del DCR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DcrAction {
    /// Fusionar el texto entrante en el nodo existente (append).
    Merge,
    /// No fusionar: crear nodo propio y enlazarlo al parecido.
    Link,
    /// No es un near-duplicate: flujo normal de escritura.
    Skip,
}

/// Decisión pura y testeable de la política DCR.
///
/// **Regla dura de MEM-3 (por delante de cualquier política):** nunca se fusiona
/// ni se sustituye entre `type` distinto ni entre `metadata.space` distinto. Esas
/// dos cosas son identidad, no configuración: el DCR legacy fusionaba por
/// similitud a secas y metió una nota de estado dentro de un nodo ajeno.
///
/// Después, una nota de estado (`kind=state` explícito) **nunca** se decide por
/// similitud: se enlaza. El reemplazo va por clave exacta y no destructivo, antes
/// de esta función (ver `dcr_state_key`).
/// - `legacy` (default): fusiona con similitud >= 0.87.
/// - `strict`: fusiona solo con MISMO agent_id y similitud >= 0.95; si hay
///   parecido >= 0.87 sin cumplir, se enlaza sin fusionar.
pub(crate) fn dcr_action(
    policy: &str,
    sim: f32,
    same_agent: bool,
    same_type: bool,
    same_space: bool,
    is_state_note: bool,
) -> DcrAction {
    const LEGACY_THRESHOLD: f32 = 0.87;
    const STRICT_MERGE_THRESHOLD: f32 = 0.95;

    // MEM-3 (1): identidad primero. Ni fusionar ni sustituir fuera de su tipo/espacio.
    if !same_type || !same_space {
        return if sim >= LEGACY_THRESHOLD {
            DcrAction::Link
        } else {
            DcrAction::Skip
        };
    }
    // MEM-3 v2 (T3031): una nota de estado NUNCA se decide por similitud. El
    // reemplazo va por clave exacta antes de llegar aquí (ver `dcr_state_key`);
    // si el parecido llega a este punto, se enlaza y se conservan las dos.
    if is_state_note {
        return if sim >= LEGACY_THRESHOLD {
            DcrAction::Link
        } else {
            DcrAction::Skip
        };
    }

    match policy {
        "strict" => {
            if sim >= STRICT_MERGE_THRESHOLD && same_agent {
                DcrAction::Merge
            } else if sim >= LEGACY_THRESHOLD {
                DcrAction::Link
            } else {
                DcrAction::Skip
            }
        }
        // Cualquier valor desconocido se comporta como legacy (fail-safe).
        _ => {
            if sim >= LEGACY_THRESHOLD {
                DcrAction::Merge
            } else {
                DcrAction::Skip
            }
        }
    }
}

/// T-DSH-154: ¿el nodo existente pertenece al mismo agente? Los nodos de
/// `agent_memory` guardan `agent_id` en metadata; los anónimos no lo llevan.
pub(crate) fn dcr_same_agent(existing_metadata: &str, aid: &str) -> bool {
    match serde_json::from_str::<serde_json::Value>(existing_metadata)
        .ok()
        .as_ref()
        .and_then(|m| m.get("agent_id"))
        .and_then(|v| v.as_str())
    {
        Some(existing_aid) => existing_aid == aid,
        None => aid == "anonymous",
    }
}

/// MEM-3: `space` declarado en la metadata del nodo (`metadata.space`).
///
/// Es lo que separa "mi mapa global" de "mi estado del turno": dos notas con el
/// mismo texto pero espacios distintos NO son la misma memoria.
pub(crate) fn dcr_space(metadata: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(metadata)
        .ok()
        .as_ref()
        .and_then(|m| m.get("space"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

/// MEM-3 v2 (T3031): una nota es de estado **solo** si lo dice explícitamente
/// (`kind = "state"`). Llevar `space` NO basta: con `space` y sin `kind`, una nota
/// normal que se parezca a otra del mismo espacio se enlaza, no se reemplaza.
pub(crate) fn dcr_is_state_note(metadata: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(metadata)
        .ok()
        .as_ref()
        .and_then(|v| v.get("kind"))
        .and_then(|v| v.as_str())
        .map(|k| k.eq_ignore_ascii_case("state"))
        .unwrap_or(false)
}

/// MEM-3 v2: clave de identidad de una nota de estado.
///
/// `topic_key` explícito (`metadata.topic`) si lo hay; si no, `state:<space>`.
/// Es la MISMA clave que `upsert_node` guarda en la columna `topic_key`, así que
/// la sustitución se resuelve por clave exacta e índice, no por parecido.
pub(crate) fn dcr_state_key(metadata: &str) -> Option<String> {
    if !dcr_is_state_note(metadata) {
        return None;
    }
    let valor: serde_json::Value = serde_json::from_str(metadata).ok()?;
    if let Some(topic) = valor.get("topic").and_then(|v| v.as_str())
        && !topic.trim().is_empty()
    {
        return Some(topic.trim().replace('"', "").replace("\\\"", ""));
    }
    let space = valor.get("space").and_then(|v| v.as_str())?.trim();
    if space.is_empty() {
        return None;
    }
    Some(format!("state:{}", space.replace('"', "")))
}

/// T-DSH-94: contador visible de embeddings que no se pudieron persistir.
/// El nodo YA está guardado (la memoria no se pierde); solo el recall semántico
/// de ese nodo queda degradado, y eso ya no puede ser silencioso.
pub(crate) static EMBEDDING_SAVE_FAILURES: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Nº de `save_embedding` fallidos desde el arranque (visible para tests y para
/// telemetría interna; no cambia el contrato de `tylluan_remember`).
pub(crate) fn embedding_save_failures() -> u64 {
    EMBEDDING_SAVE_FAILURES.load(std::sync::atomic::Ordering::Relaxed)
}

/// Registra el fallo de persistencia del embedding de un recuerdo.
pub(crate) fn record_embedding_save_failure(node_id: &str, agent_id: &str, error: &str) {
    EMBEDDING_SAVE_FAILURES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    tracing::warn!(
        node_id = %node_id,
        agent_id = %agent_id,
        error = %error,
        total_failures = embedding_save_failures(),
        "tylluan_remember: el embedding no se pudo persistir; el nodo queda guardado y el recall BM25 no se ve afectado"
    );
}

/// T-DSH-94: persiste el embedding best-effort sin perder el fallo.
/// Devuelve `true` si se guardó; ante error avisa y cuenta.
pub(crate) async fn persist_memory_embedding(
    silva: &crate::memory::silva::SilvaDB,
    node_id: &str,
    agent_id: &str,
    embedding: &[f32],
    model_id: &str,
    model_hash: Option<&str>,
) -> bool {
    match silva
        .save_embedding(node_id, embedding, model_id, model_hash)
        .await
    {
        Ok(()) => true,
        Err(error) => {
            record_embedding_save_failure(node_id, agent_id, &error.to_string());
            false
        }
    }
}

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
                clean.push_str(&format!("[Agente: {a}] "));
            }
            if let Some(i) = intent {
                clean.push_str(&format!("Acción: {i}\n"));
            }
            if let Some(r) = result {
                clean.push_str(&format!("Resultado: {r}"));
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
        .and_then(|a| a.get("content").or_else(|| a.get("intent")))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if raw_content.trim().is_empty() {
        return Ok(error_result("tylluan_remember requires a non-empty 'content' argument."));
    }

    // ASI06 Layer 1: reject known injection patterns before ANYTHING else --
    // before Coloquio post, before DCR, before any write. Same pattern list
    // `tylluan_recall` already uses to eliminate results silently (never
    // penalized, never returned) -- there is no legitimate use case for
    // content matching these patterns, so ingestion gets the same severity
    // as recall: hard rejection, not quarantine. Checked against raw_content
    // (pre-wrapper-cleaning) so a wrapper can't be used to smuggle a pattern
    // past a post-clean check.
    if crate::security::poison_patterns::matches_injection_pattern(&raw_content) {
        tracing::warn!("🛡️ tylluan_remember rejected: content matches a known injection pattern (ASI06 Layer 1)");
        return Ok(error_result(
            "ACCESS_DENIED: content matches a known prompt-injection pattern and was rejected before being written to memory."
        ));
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

    // MEM-3 v2 (T3031): la identidad de una nota de estado es una CLAVE, no la
    // similitud: aqui solo se CALCULA. La caducidad de la version anterior se hace
    // despues de escribir la nueva (v4, T3038): si se hiciera antes, un fallo de
    // escritura dejaria el estado sin version viva.
    let clave_estado: Option<String> = dcr_state_key(&metadata);
    let mut estado_anterior: Option<String> = None;

    // DCR — Dedup Check Remember (R21-3)
    // T-DSH-154: política configurable `[silva] dcr_policy = legacy|strict`.
    // legacy (default): fusiona con sim >= 0.87 (comportamiento histórico).
    // strict: fusiona solo con mismo agent_id, mismo tipo y sim >= 0.95; si hay
    // parecido >= 0.87 sin cumplir, crea nodo propio y lo enlaza (Link).
    const DCR_THRESHOLD: f32 = 0.87;
    let dcr_policy = server.dcr_policy.clone();
    let mut dcr_link_to: Option<String> = None;
    let early_embedding = server.matcher.engine()
        .and_then(|e| tokio::task::block_in_place(|| e.embed_batch_coalesced_routing(content.trim())).ok()
            .map(|emb| (emb, e.engine_id())));
    if let Some((ref emb, _)) = early_embedding
        && let Ok(candidates) = server.silva.search_vector(emb, 3).await
        && let Some((existing_node, sim)) = candidates.into_iter().find(|(_, s)| *s >= DCR_THRESHOLD)
    {
        let aid = rem_agent_id.as_deref().unwrap_or("anonymous");
        let same_agent = dcr_same_agent(&existing_node.metadata, aid);
        let new_type = if is_pending_task {
            "task"
        } else if rem_agent_id.is_some() {
            "agent_memory"
        } else {
            "memory"
        };
        let same_type = existing_node.node_type == new_type;
        // MEM-3: el espacio forma parte de la identidad del recuerdo; y las notas
        // de estado se sustituyen en vez de acumularse.
        let same_space = dcr_space(&existing_node.metadata) == dcr_space(&metadata);
        let is_state_note = dcr_is_state_note(&metadata);
        match dcr_action(&dcr_policy, sim, same_agent, same_type, same_space, is_state_note) {
            DcrAction::Merge => {
                // T-DSH-95: si el texto entrante aporta información nueva, se
                // fusiona en el nodo existente en vez de descartarlo (bug: el
                // DCR reforzaba el nodo y perdía lo nuevo).
                if let Some(merged) = merge_near_duplicate_content(&existing_node.content, &content) {
                    let merged_embedding = server.matcher.engine().and_then(|e| {
                        tokio::task::block_in_place(|| e.embed_batch_coalesced_routing(&merged))
                            .ok()
                            .map(|emb| (emb, e.engine_id()))
                    });
                    match server.silva.update_node_content(&existing_node.id, &merged).await {
                        Ok(()) => {
                            if let Some((emb2, model_id)) = merged_embedding {
                                persist_memory_embedding(
                                    &server.silva,
                                    &existing_node.id,
                                    aid,
                                    &emb2,
                                    &model_id,
                                    None,
                                )
                                .await;
                            }
                            let _ = server.silva.reinforce_node(&existing_node.id, 1.05).await;
                            if rem_agent_id.is_some() {
                                let _ = server
                                    .silva
                                    .record_agent_access(
                                        &existing_node.id,
                                        chrono::Utc::now().timestamp(),
                                    )
                                    .await;
                            }
                            crate::security::write_gate::spawn_write_gate_judge(
                                server.silva.clone(),
                                existing_node.id.clone(),
                                merged.clone(),
                            );
                            tracing::info!(
                                "DCR: near-duplicate merged (sim={:.3}) existing={} added_chars={} policy={}",
                                sim,
                                existing_node.id,
                                content.trim().chars().count(),
                                dcr_policy
                            );
                            let preview = merged.chars().take(120).collect::<String>();
                            return Ok(CallToolResult {
                                content: vec![Content::text(format!(
                                    "DCR: near-duplicate merged (similarity={:.0}%) into node '{}' — new content appended ({} chars) instead of being discarded.\nMerged: \"{}\"",
                                    sim * 100.0,
                                    existing_node.id,
                                    content.trim().chars().count(),
                                    preview
                                ))],
                                is_error: Some(false),
                            });
                        }
                        Err(error) => {
                            tracing::warn!(
                                node_id = %existing_node.id,
                                error = %error,
                                "DCR: fallo al fusionar contenido nuevo; se mantiene el nodo existente"
                            );
                        }
                    }
                }
                tracing::info!(
                    "🔄 DCR: near-duplicate blocked (sim={:.3}) existing={} agent={} policy={}",
                    sim, existing_node.id,
                    rem_agent_id.as_deref().unwrap_or("anonymous"),
                    dcr_policy
                );
                let _ = server.silva.reinforce_node(&existing_node.id, 1.05).await;
                if rem_agent_id.is_some() {
                    let _ = server.silva.record_agent_access(
                        &existing_node.id,
                        chrono::Utc::now().timestamp(),
                    ).await;
                }
                // D-17: cuarto camino de escritura. Este retorno temprano no crea
                // nodo, así que se quedaba fuera del juez (hallazgo abierto de D-9):
                // el texto entrante refuerza un nodo existente sin que nadie mire
                // si ese nodo es contenido no confiable. Se juzga el nodo que SE
                // REFUERZA (es el que queda en memoria), no el texto descartado.
                crate::security::write_gate::spawn_write_gate_judge(
                    server.silva.clone(),
                    existing_node.id.clone(),
                    existing_node.content.clone(),
                );
                let preview = existing_node.content.chars().take(100).collect::<String>();
                return Ok(CallToolResult {
                    content: vec![Content::text(format!(
                        "DCR: near-duplicate detected (similarity={:.0}%). Reinforced existing node '{}' instead of creating duplicate.\nExisting: \"{}\"",
                        sim * 100.0, existing_node.id, preview
                    ))],
                    is_error: Some(false),
                });
            }
            DcrAction::Link => {
                tracing::info!(
                    "DCR strict: parecido sin fusión (sim={:.3}, same_agent={}, same_type={}) existing={} — se crea nodo propio enlazado",
                    sim, same_agent, same_type, existing_node.id
                );
                dcr_link_to = Some(existing_node.id.clone());
            }
            DcrAction::Skip => {}
        }
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
        Some(aid) => format!("memory | agent: {aid} | {content}"),
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

            // M31 audit fix: these three calls each hit SilvaDB's own connection
            // mutex independently (upsert_node/reinforce_node/touch_node_with_rating
            // aren't composable into one SQL transaction without risking deadlock
            // re-entering the same tokio::sync::Mutex<Connection>). Genuinely
            // atomic composition would need a lower-level "run these 3 statements
            // under one lock" primitive on SilvaDB that doesn't exist yet. The
            // audit correctly rates this as low-severity (worst case: task node
            // exists with weight=1.0 instead of 1.1, or missing its creation
            // trace) -- not data corruption. Until this is silently swallowed,
            // log failures so a partial write is at least visible/debuggable.
            if let Err(e) = server.silva.upsert_node_with_provenance(&task_id, "task", &content, &task_meta.to_string(), "agent_generated").await {
                tracing::error!("Task node upsert failed for {}: {}", task_id, e);
            }
            if let Err(e) = server.silva.reinforce_node(&task_id, 1.1).await {
                tracing::error!("Task node reinforce failed for {}: {}", task_id, e);
            }
            if let Err(e) = server.silva.touch_node(&task_id, aid, "task_created").await {
                tracing::error!("Task node touch failed for {}: {}", task_id, e);
            }
            tracing::info!("🧠 Memory reinforced (task): node={} agent={}", task_id, aid);
            task_id
        } else {
            let mgr = crate::memory::agent_memory::AgentMemoryManager::new(server.silva.clone(), 20);
            mgr.record_memory(aid, &content, importance).await
        };

        // ASI06 Layer 2 runs once for every remember, after the node exists --
        // see the single call site below the node_id block (D-9/H2: it used to
        // live in here, so anonymous writes skipped the judge entirely).

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
        let agent_node_id = format!("agent:{aid}");
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
        if let Err(e) = server.silva.upsert_node_with_validity(&nid, "memory", &tagged_content, &metadata, crate::memory::silva::NodeWriteOptions::new("agent_generated").valid_until(valid_until)).await {
            tracing::warn!("⚠️ tylluan_remember: silva graph write failed: {}", e);
        }
        nid
    };
    // MEM-3 v4 (T3038): ahora que la nota nueva existe, se caduca la version viva
    // anterior de la MISMA clave (se conserva: `valid_until`, no se borra). Solo se
    // hace si el nodo nuevo se llego a escribir: si la escritura fallo, el estado
    // anterior sigue vivo y no se pierde nada.
    if let Some(clave) = clave_estado.as_deref()
        && server.silva.get_node(&node_id).await.ok().flatten().is_some()
        && let Ok(Some(previo)) = server
            .silva
            .expire_live_state_by_key_excluding(clave, chrono::Utc::now().timestamp(), &node_id)
            .await
    {
        tracing::info!(
            "DCR: nota de estado '{}' sustituye a {} (caducada y conservada)",
            clave, previo
        );
        estado_anterior = Some(previo);
    }

    // T-DSH-154 (strict): si el DCR no fusionó por política pero había un
    // parecido >= 0.87, el nodo nuevo queda enlazado al existente para no
    // perder la relación semántica (el texto entrante se conserva íntegro).
    if let Some(target) = dcr_link_to
        && let Err(e) = server
            .silva
            .add_edge(&node_id, &target, "near_duplicate", 0.9, "{}")
            .await
    {
        tracing::warn!("DCR strict: no se pudo enlazar {node_id} -> {target}: {e}");
    }
    // ASI06 Layer 2: fire-and-forget LLM judge, runs after the node already
    // exists. Does not block this response and never fails the write -- Layer 1
    // (above, before any write) already rejected known injection patterns hard;
    // this is the softer, slower net for content that isn't a known pattern but
    // still reads as manipulative to a real judge.
    //
    // D-9/H2: one call site for BOTH paths. Before this, the call lived inside
    // the `if let Some(agent_id)` branch, so every anonymous write -- no
    // agent_id -- was stored with no Layer 2 at all.
    crate::security::write_gate::spawn_write_gate_judge(
        server.silva.clone(),
        node_id.clone(),
        content.clone(),
    );
    let aid = rem_agent_id.as_deref().unwrap_or("anonymous");
    if let Some(ref profiles) = server.agent_profiles {
        let is_new = {
            if let Ok(p_store) = profiles.lock() {
                p_store.is_new_agent(aid)
            } else { false }
        };

        if is_new {
            let node_id = format!("agent_identity_{aid}");
            let meta = serde_json::json!({
                "agent_id": aid,
                "registered_at": chrono::Utc::now().to_rfc3339(),
                "role": "generalist"
            });
            let _ = server.silva.upsert_node_with_provenance(&node_id, "agent_identity", &format!("Agente {aid} registrado en el colectivo"), &meta.to_string(), "agent_generated").await;
            let _ = server.silva.touch_node(&node_id, aid, "handshake").await;
            if let Ok(mut h) = server.hormones.lock() {
                h.emit_novelty(0.6);
            }
            let msg = BlackboardMessage {
                msg_type: "welcome".into(),
                body: format!("Bienvenido al colectivo, {aid}. Tu identidad ha sido registrada."),
                to: aid.to_string(),
                from: "kernel".into(),
                thread_id: None,
                priority: 3,
                untrusted: false,
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
    let embedding = early_embedding.or_else(|| server.matcher.engine()
        .and_then(|e| tokio::task::block_in_place(|| e.embed_batch_coalesced_routing(&tagged_content)).ok()
            .map(|emb| (emb, e.engine_id()))));
    if let Some((emb, model_id)) = embedding.as_ref() {
        // T-DSH-94: best-effort, pero un fallo ya no es invisible (warn + contador).
        persist_memory_embedding(server.silva.as_ref(), &node_id, aid, emb, model_id, None).await;

        // TMS: deprecate contradictions after storing
        if let Ok(deprecated_count) = server.silva.deprecate_contradictions(&node_id, emb, &tagged_content).await
            && deprecated_count > 0 {
                tracing::info!("🧠 TMS: deprecated {} contradictions for node={}", deprecated_count, node_id);
            }
    }

    // F1 ROADMAP_O3:58: dual-write híbrido cerrado — la escritura híbrida se retiró;
    // SilvaDB es la única escritura de este handler (rutas de arriba). Los side
    // effects antes condicionados al éxito de la segunda escritura (invalidar
    // cachés, auto-link, respuesta al caller) se ejecutan siempre.
    // Invalidate caches so new memories are immediately visible
    {
        let mut cache = server.recall_cache.lock().await;
        cache.invalidate_all();
    }
    server.silva.query_embed_cache.invalidate();

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
    // ADR-012 Fase 1: actualizar last_agent_access si hay agent_id
    if rem_agent_id.is_some() {
        let now = chrono::Utc::now().timestamp();
        let _ = server.silva.record_agent_access(&node_id, now).await;
    }
    let supersesion = match estado_anterior {
        Some(previo) => format!(
            " [state key: previous version {previo} expired (kept, not deleted)]"
        ),
        None => String::new(),
    };
    Ok(CallToolResult {
        content: vec![Content::text(format!("Stored node {node_id} (importance={importance:.2}): \"{preview}\"{supersesion}"))],
        is_error: Some(false),
    })
}

#[cfg(test)]
mod embedding_failure_tests {
    use super::*;

    /// T-DSH-94: el fallo se avisa (con node_id y agent_id) y suma al contador.
    #[test]
    fn embedding_save_failure_warns_and_counts() {
        let before = embedding_save_failures();
        let captured = crate::test_support::capture_warns(|| {
            record_embedding_save_failure("node-42", "agent-7", "disk full");
        });
        assert!(captured.contains("node-42"), "warn debe nombrar el nodo: {captured}");
        assert!(captured.contains("agent-7"), "warn debe nombrar el agente: {captured}");
        assert!(captured.contains("embedding"), "warn debe explicar el embedding: {captured}");
        assert!(embedding_save_failures() > before, "el contador global (compartido entre tests en paralelo) debe subir");
    }

    /// T-DSH-94: aunque el embedding falle, el nodo sigue guardado (BM25).
    #[tokio::test(flavor = "multi_thread")]
    async fn embedding_save_failure_keeps_the_node_stored() {
        let silva = crate::memory::silva::SilvaDB::in_memory().await.unwrap();
        silva
            .upsert_node("n-embed-94", "fact", "contenido persistente", "{}")
            .await
            .unwrap();
        // Fuerza el error de escritura sin tocar el disco del usuario.
        {
            let conn = silva.conn.lock().await;
            conn.execute_batch("PRAGMA query_only=ON;").unwrap();
        }
        let before = embedding_save_failures();
        let saved =
            persist_memory_embedding(&silva, "n-embed-94", "agent-7", &[0.1f32; 1024], "test-model", None)
                .await;
        assert!(!saved, "con la DB en solo-lectura el embedding no se persiste");
        assert!(embedding_save_failures() > before, "el contador global (compartido entre tests en paralelo) debe subir");
        assert!(
            silva.get_node("n-embed-94").await.unwrap().is_some(),
            "el nodo debe seguir guardado aunque falle el embedding (recall BM25 intacto)"
        );
    }

    /// T-DSH-95: el DCR (umbral 0.87) ya no descarta el texto nuevo; lo añade.
    #[test]
    fn dcr_merge_keeps_new_information() {
        let old = "estado: x=1; sin novedades";
        let incoming = "estado: x=1; sin novedades; y=2 (nota nueva del 2026-10-26)";
        let merged = merge_near_duplicate_content(old, incoming)
            .expect("con similitud >= 0.87 el DCR debe fusionar, no descartar");
        assert!(merged.contains("x=1"), "el contenido viejo se conserva: {merged}");
        assert!(merged.contains("y=2"), "la información nueva NO se pierde: {merged}");
        assert!(merged.contains("[actualización]"));
    }

    #[test]
    fn dcr_merge_does_not_duplicate_exact_text() {
        assert!(merge_near_duplicate_content("misma nota", " misma nota ").is_none());
        assert!(merge_near_duplicate_content("vieja", "   ").is_none());
    }

    /// T-DSH-154 + MEM-3: `legacy` fusiona con >= 0.87 **pero ya no "sin mirar
    /// tipo ni espacio"** — esa parte era el fallo. El agente sigue sin importar
    /// en legacy; la identidad (tipo y espacio) sí, en todas las políticas.
    #[test]
    fn dcr_policy_legacy_merges_similar_only_inside_the_same_identity() {
        // Mismo tipo y mismo espacio (sin space en ninguno de los dos).
        assert_eq!(
            dcr_action("legacy", 0.87, false, true, true, false),
            DcrAction::Merge
        );
        assert_eq!(
            dcr_action("legacy", 0.99, true, true, true, false),
            DcrAction::Merge
        );
        // Por debajo del umbral: flujo normal.
        assert_eq!(
            dcr_action("legacy", 0.86, true, true, true, false),
            DcrAction::Skip
        );
        // Politica desconocida = legacy (fail-safe).
        assert_eq!(
            dcr_action("desconocida", 0.90, false, true, true, false),
            DcrAction::Merge
        );
        // MEM-3: fuera de su tipo o de su espacio NO se fusiona, ni en legacy.
        assert_eq!(
            dcr_action("legacy", 0.99, true, false, true, false),
            DcrAction::Link,
            "distinto type: se enlaza, no se fusiona"
        );
        assert_eq!(
            dcr_action("legacy", 0.99, true, true, false, false),
            DcrAction::Link,
            "distinto space: se enlaza, no se fusiona"
        );
    }

    /// T-DSH-154: `strict` solo fusiona con MISMO agente, MISMO tipo y >= 0.95;
    /// con parecido >= 0.87 pero sin cumplir, enlaza sin fusionar.
    #[test]
    fn dcr_policy_strict_merges_only_exact_context() {
        assert_eq!(
            dcr_action("strict", 0.95, true, true, true, false),
            DcrAction::Merge
        );
        assert_eq!(
            dcr_action("strict", 0.99, true, true, true, false),
            DcrAction::Merge
        );
        // mismo agente/tipo pero por debajo de 0.95 -> enlaza
        assert_eq!(
            dcr_action("strict", 0.94, true, true, true, false),
            DcrAction::Link
        );
        // >= 0.95 pero otro agente o tipo -> enlaza
        assert_eq!(
            dcr_action("strict", 0.99, false, true, true, false),
            DcrAction::Link
        );
        assert_eq!(
            dcr_action("strict", 0.99, true, false, true, false),
            DcrAction::Link
        );
        // por debajo del umbral legacy -> flujo normal
        assert_eq!(
            dcr_action("strict", 0.86, true, true, true, false),
            DcrAction::Skip
        );
    }

    /// MEM-3 (1) — el caso del TL: una nota de estado NO puede acabar fusionada en
    /// un nodo ajeno. Mismo texto, similitud altisima, pero distinto tipo o espacio.
    #[test]
    fn mem3_no_fusiona_entre_types_ni_entre_spaces() {
        // Distinto tipo: enlaza (para no perder el texto), nunca fusiona.
        assert_eq!(
            dcr_action("legacy", 0.99, true, false, true, true),
            DcrAction::Link,
            "una nota de estado no puede fusionarse en otro type"
        );
        // Distinto espacio: idem. Es el caso real (space=tl-global-map vs sin space).
        assert_eq!(
            dcr_action("legacy", 0.99, true, true, false, true),
            DcrAction::Link,
            "una nota de estado no puede fusionarse en otro space"
        );
        // Las dos diferencias a la vez.
        assert_eq!(
            dcr_action("strict", 0.99, true, false, false, true),
            DcrAction::Link
        );
        // Y por debajo del umbral, ni enlace: flujo normal.
        assert_eq!(
            dcr_action("legacy", 0.50, true, false, false, true),
            DcrAction::Skip
        );
        // Estos cuatro son los que discriminan de verdad: una nota NORMAL (sin
        // kind=state) tampoco puede cruzar tipo ni espacio. Sin estos casos, la
        // rama de estado pondria todo en Link y la regla de identidad quedaria sin
        // cubrir (lo destapo la mutacion).
        assert_eq!(
            dcr_action("legacy", 0.99, true, true, false, false),
            DcrAction::Link,
            "nota normal en distinto space: enlaza, no fusiona"
        );
        assert_eq!(
            dcr_action("legacy", 0.99, true, false, true, false),
            DcrAction::Link,
            "nota normal en distinto type: enlaza, no fusiona"
        );
        assert_eq!(
            dcr_action("strict", 0.99, true, true, false, false),
            DcrAction::Link
        );
        assert_eq!(
            dcr_action("desconocida", 0.99, true, false, false, false),
            DcrAction::Link,
            "la politica desconocida (fail-safe legacy) tampoco cruza identidad"
        );
    }

    /// MEM-3 v2 (T3031): la similitud NUNCA decide un reemplazo. Una nota de estado
    /// en su mismo tipo y espacio se ENLAZA; quien sustituye es la clave exacta.
    #[test]
    fn mem3_la_similitud_no_decide_un_reemplazo() {
        // legacy y strict: con parecido alto, una nota de estado se enlaza igual.
        assert_eq!(
            dcr_action("legacy", 0.99, true, true, true, true),
            DcrAction::Link,
            "una nota de estado no puede fusionarse ni reemplazarse por parecido"
        );
        assert_eq!(
            dcr_action("strict", 0.99, false, true, true, true),
            DcrAction::Link
        );
        // Y sin llegar al umbral, ni enlace.
        assert_eq!(
            dcr_action("legacy", 0.50, true, true, true, true),
            DcrAction::Skip
        );
        // Una nota normal sigue el camino de siempre (fusion en legacy).
        assert_eq!(
            dcr_action("legacy", 0.99, true, true, true, false),
            DcrAction::Merge,
            "una nota normal (sin kind=state) sigue fusionando en legacy"
        );
    }

    /// MEM-3 v2: `space` y `kind` de la metadata, y la clave de identidad.
    #[test]
    fn mem3_space_kind_y_clave_de_estado() {
        assert_eq!(dcr_space(r#"{"space":"tl-global-map"}"#).as_deref(), Some("tl-global-map"));
        assert_eq!(dcr_space(r#"{"space":""}"#), None, "space vacio no es espacio");
        assert_eq!(dcr_space(r#"{"agent_id":"deep"}"#), None);
        assert_eq!(dcr_space("no-es-json"), None);

        assert!(dcr_is_state_note(r#"{"kind":"state"}"#));
        assert!(dcr_is_state_note(r#"{"kind":"STATE"}"#));
        // T3031 (a): un `space` suelto NO convierte una nota en nota de estado.
        assert!(
            !dcr_is_state_note(r#"{"space":"tl-global-map"}"#),
            "con space y SIN kind=state no es una nota de estado"
        );
        assert!(!dcr_is_state_note(r#"{"kind":"note"}"#));
        assert!(!dcr_is_state_note(r#"{}"#));
        assert!(!dcr_is_state_note("no-es-json"));

        // T3031 (b): la clave. `topic` manda; si no hay, `state:<space>`.
        assert_eq!(
            dcr_state_key(r#"{"kind":"state","topic":"mapa global"}"#).as_deref(),
            Some("mapa global")
        );
        assert_eq!(
            dcr_state_key(r#"{"kind":"state","space":"tl-global-map"}"#).as_deref(),
            Some("state:tl-global-map")
        );
        assert_eq!(
            dcr_state_key(r#"{"space":"tl-global-map"}"#),
            None,
            "sin kind=state no hay clave de estado"
        );
        assert_eq!(dcr_state_key(r#"{"kind":"state"}"#), None, "sin topic ni space, sin clave");
    }

    /// T-DSH-154: `same_agent` sale de `metadata.agent_id`; los anónimos no
    /// llevan la clave y solo casan con `anonymous`.
    #[test]
    fn dcr_same_agent_reads_metadata() {
        assert!(dcr_same_agent(r#"{"agent_id":"dsh"}"#, "dsh"));
        assert!(!dcr_same_agent(r#"{"agent_id":"otro"}"#, "dsh"));
        assert!(dcr_same_agent("{}", "anonymous"));
        assert!(!dcr_same_agent("{}", "dsh"));
        assert!(!dcr_same_agent("no-json", "dsh"));
    }

    /// T-DSH-154: la config nueva tiene default `legacy` y parsea `strict`.
    #[test]
    fn dcr_policy_defaults_to_legacy_and_parses_strict() {
        assert_eq!(crate::config::SilvaConfig::default().dcr_policy, "legacy");
        let empty: crate::config::TylluanConfig = toml::from_str("").expect("empty config parses");
        assert_eq!(empty.silva.dcr_policy, "legacy");
        let strict: crate::config::TylluanConfig =
            toml::from_str("[silva]\ndcr_policy = \"strict\"\n").expect("strict parses");
        assert_eq!(strict.silva.dcr_policy, "strict");
    }

    /// T-DSH-95: `update_node_content` persiste la fusión y recalcula el hash.
    #[tokio::test(flavor = "multi_thread")]
    async fn update_node_content_persists_merged_text() {
        let silva = crate::memory::silva::SilvaDB::in_memory().await.unwrap();
        silva.upsert_node("n-dcr", "fact", "estado: x=1", "{}").await.unwrap();
        silva
            .update_node_content("n-dcr", "estado: x=1\n\n[actualización] y=2")
            .await
            .unwrap();
        let node = silva.get_node("n-dcr").await.unwrap().expect("el nodo debe existir");
        assert!(node.content.contains("y=2"), "el contenido fusionado debe persistir");
        assert_ne!(node.content_hash, "", "el hash debe recalcularse");
    }

    /// MEM-3 v4 (T3038): punta a punta por `handle_tylluan_remember`, no solo unitarios.
    ///
    /// Dos notas `kind=state, space=tl-global-map` seguidas: la primera queda
    /// **caducada y conservada**, la segunda **viva y recuperable**. Y un nodo normal
    /// cuyo `metadata.topic` coincide con la clave del estado **no** se caduca.
    #[tokio::test(flavor = "multi_thread")]
    async fn mem3_e2e_dos_notas_de_estado_seguidas() {
        use crate::memory::agent_nodes::AgentNodeRouter;
        use crate::memory::mailbox::Mailbox;
        use crate::registry::guild_process::GuildRegistry;
        use crate::router::catalog::builtin_catalog;
        use crate::router::matcher::GuildMatcher;
        use std::path::PathBuf;
        use std::sync::Arc;
        use tokio::sync::RwLock;
        use tokio::sync::broadcast;

        let reg = GuildRegistry::new(PathBuf::from("."), 300, Default::default(), 3);
        let test_reg = Arc::new(RwLock::new(reg));
        let matcher = GuildMatcher::new(builtin_catalog());
        let (tx, _) = broadcast::channel(16);
        let node_router = AgentNodeRouter::new(tx);
        let silva = Arc::new(crate::memory::silva::SilvaDB::in_memory().await.unwrap());
        let doctor = Arc::new(crate::doctor::Doctor::new(
            test_reg.clone(),
            Arc::new(crate::memory::silva::SilvaDB::in_memory().await.unwrap()),
            Arc::new(std::sync::Mutex::new(
                crate::curriculum::CurriculumLearner::new_in_memory(5).unwrap(),
            )),
        ));
        let server = TylluanServer::new(
            test_reg,
            Arc::new(matcher),
            silva.clone(),
            Arc::new(Mailbox::in_memory().await.unwrap()),
            doctor,
            node_router,
        );

        // Un nodo NORMAL con el mismo `topic` que la clave del estado: no debe caducarse.
        silva
            .upsert_node(
                "normal-con-mismo-topic",
                "memory",
                "nota normal",
                r#"{"topic":"state:tl-global-map"}"#,
            )
            .await
            .unwrap();

        let mut args = serde_json::Map::new();
        args.insert(
            "content".to_string(),
            serde_json::Value::String("estado: fase 1 del mapa".to_string()),
        );
        args.insert(
            "metadata".to_string(),
            serde_json::json!({"kind": "state", "space": "tl-global-map"}),
        );
        let r1 = handle_tylluan_remember(&server, Some(args.clone()))
            .await
            .expect("primera nota");
        assert!(!r1.is_error.unwrap_or(false));
        let t1 = r1.content[0].as_text().unwrap().text.clone();
        let id1 = t1
            .split("Stored node ")
            .nth(1)
            .and_then(|s| s.split(' ').next())
            .expect("id de la primera nota")
            .to_string();

        // Que no caigan en el mismo milisegundo (el id anonimo es memory:<millis>).
        tokio::time::sleep(std::time::Duration::from_millis(3)).await;

        let mut args2 = args.clone();
        args2.insert(
            "content".to_string(),
            serde_json::Value::String("estado: fase 2 del mapa".to_string()),
        );
        let r2 = handle_tylluan_remember(&server, Some(args2))
            .await
            .expect("segunda nota");
        assert!(!r2.is_error.unwrap_or(false));
        let t2 = r2.content[0].as_text().unwrap().text.clone();
        let id2 = t2
            .split("Stored node ")
            .nth(1)
            .and_then(|s| s.split(' ').next())
            .expect("id de la segunda nota")
            .to_string();
        assert_ne!(id1, id2, "las dos notas son nodos distintos");

        // La primera: caducada y conservada (el texto sigue ahi).
        let n1 = silva.get_node(&id1).await.unwrap().expect("la 1a sigue existiendo");
        assert!(n1.valid_until.is_some(), "la 1a queda caducada: {n1:?}");
        assert_eq!(n1.content, "estado: fase 1 del mapa", "el texto anterior NO se borra");

        // La segunda: viva, con la clave de estado en topic_key.
        let n2 = silva.get_node(&id2).await.unwrap().expect("la 2a existe");
        assert!(n2.valid_until.is_none(), "la 2a es la version viva");
        assert_eq!(
            n2.topic_key.as_deref(),
            Some("state:tl-global-map"),
            "la clave de estado se guarda en topic_key"
        );
        assert!(
            t2.contains("expired (kept, not deleted)"),
            "la respuesta debe decir que la anterior se conserva: {t2}"
        );

        // Y el nodo normal con el mismo topic sigue vivo.
        let normal = silva
            .get_node("normal-con-mismo-topic")
            .await
            .unwrap()
            .expect("el normal sigue");
        assert!(
            normal.valid_until.is_none(),
            "un nodo normal no se caduca por tener el mismo topic"
        );
    }

    /// MEM-3 v4 (T3038, punto 2): si la escritura de la nota nueva FALLA, el estado
    /// anterior no puede quedarse caducado. La caducidad va despues de comprobar que
    /// el nodo nuevo existe de verdad.
    #[tokio::test(flavor = "multi_thread")]
    async fn mem3_e2e_si_la_escritura_falla_el_estado_anterior_sigue_vivo() {
        use crate::memory::agent_nodes::AgentNodeRouter;
        use crate::memory::mailbox::Mailbox;
        use crate::registry::guild_process::GuildRegistry;
        use crate::router::catalog::builtin_catalog;
        use crate::router::matcher::GuildMatcher;
        use std::path::PathBuf;
        use std::sync::Arc;
        use tokio::sync::RwLock;
        use tokio::sync::broadcast;

        let reg = GuildRegistry::new(PathBuf::from("."), 300, Default::default(), 3);
        let test_reg = Arc::new(RwLock::new(reg));
        let (tx, _) = broadcast::channel(16);
        let silva = Arc::new(crate::memory::silva::SilvaDB::in_memory().await.unwrap());
        let doctor = Arc::new(crate::doctor::Doctor::new(
            test_reg.clone(),
            Arc::new(crate::memory::silva::SilvaDB::in_memory().await.unwrap()),
            Arc::new(std::sync::Mutex::new(
                crate::curriculum::CurriculumLearner::new_in_memory(5).unwrap(),
            )),
        ));
        let server = TylluanServer::new(
            test_reg,
            Arc::new(GuildMatcher::new(builtin_catalog())),
            silva.clone(),
            Arc::new(Mailbox::in_memory().await.unwrap()),
            doctor,
            AgentNodeRouter::new(tx),
        );

        // 1) Estado v1, escrito sin trucos.
        let mut args = serde_json::Map::new();
        args.insert(
            "content".to_string(),
            serde_json::Value::String("estado: v1 intacto".to_string()),
        );
        args.insert(
            "metadata".to_string(),
            serde_json::json!({"kind": "state", "space": "tl-global-map"}),
        );
        let r1 = handle_tylluan_remember(&server, Some(args.clone())).await.expect("v1");
        let t1 = r1.content[0].as_text().unwrap().text.clone();
        let id1 = t1
            .split("Stored node ")
            .nth(1)
            .and_then(|s| s.split(' ').next())
            .expect("id v1")
            .to_string();

        // 2) La escritura de la v2 va a fallar: la conexion pasa a solo lectura.
        {
            let conn = silva.conn.lock().await;
            conn.execute_batch("PRAGMA query_only=ON;").unwrap();
        }

        tokio::time::sleep(std::time::Duration::from_millis(3)).await;
        let mut args2 = args.clone();
        args2.insert(
            "content".to_string(),
            serde_json::Value::String("estado: v2 que no se llega a escribir".to_string()),
        );
        // El handler no propaga el error de escritura (solo avisa); lo que importa es
        // que NO caduque la v1.
        let _ = handle_tylluan_remember(&server, Some(args2)).await;

        // 3) La v1 sigue viva: no se caduco nada porque no hubo escritura.
        let n1 = silva.get_node(&id1).await.unwrap().expect("la v1 sigue existiendo");
        assert!(
            n1.valid_until.is_none(),
            "si la escritura falla, el estado anterior NO puede quedar caducado: {n1:?}"
        );
        assert_eq!(n1.content, "estado: v1 intacto");

        // Y deja la conexion como estaba, para no contaminar el resto de tests.
        {
            let conn = silva.conn.lock().await;
            conn.execute_batch("PRAGMA query_only=OFF;").unwrap();
        }
    }

    /// AG-6: tylluan_remember accepts 'intent' as an ergonomic fallback alias when 'content' is omitted.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_remember_accepts_intent_alias() {
        use crate::registry::guild_process::GuildRegistry;
        use crate::router::catalog::builtin_catalog;
        use crate::router::matcher::GuildMatcher;
        use crate::memory::mailbox::Mailbox;
        use crate::memory::agent_nodes::AgentNodeRouter;
        use std::path::PathBuf;
        use std::sync::Arc;
        use tokio::sync::RwLock;
        use tokio::sync::broadcast;

        let reg = GuildRegistry::new(PathBuf::from("."), 300, Default::default(), 3);
        let test_reg = Arc::new(RwLock::new(reg));
        let matcher = GuildMatcher::new(builtin_catalog());
        let (tx, _) = broadcast::channel(16);
        let node_router = AgentNodeRouter::new(tx);
        let doctor = Arc::new(crate::doctor::Doctor::new(
            test_reg.clone(),
            Arc::new(crate::memory::silva::SilvaDB::in_memory().await.unwrap()),
            Arc::new(std::sync::Mutex::new(crate::curriculum::CurriculumLearner::new_in_memory(5).unwrap())),
        ));
        let server = TylluanServer::new(
            test_reg,
            Arc::new(matcher),
            Arc::new(crate::memory::silva::SilvaDB::in_memory().await.unwrap()),
            Arc::new(Mailbox::in_memory().await.unwrap()),
            doctor,
            node_router,
        );

        let mut args = serde_json::Map::new();
        args.insert("intent".to_string(), serde_json::Value::String("guardar aprendizaje via intent".to_string()));
        let res = handle_tylluan_remember(&server, Some(args)).await.unwrap();
        assert!(!res.is_error.unwrap_or(false));
        let text = res.content[0].as_text().unwrap();
        assert!(text.text.contains("Stored node"), "Debe guardar con éxito pasando 'intent': {text:?}");
    }
}