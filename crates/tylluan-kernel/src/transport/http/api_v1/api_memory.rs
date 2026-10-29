use axum::{
    Json,
    extract::{State, Path, Query},
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;
//
use crate::memory::silva::nodes::build_contextual_text;
use crate::transport::http::{HttpState, MemorySearchQuery};

/// T-DSH-134: id unico por escritura de memoria.
fn new_memory_write_node_id() -> String {
    format!("memory_write:{}", uuid::Uuid::new_v4())
}

pub async fn memory_write_handler(State(state): State<Arc<HttpState>>, Json(req): Json<serde_json::Value>) -> impl IntoResponse {
    let content = match req.get("content").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "missing content" }))).into_response(),
    };
    // D-17/H5: esta ruta es una escritura de AGENTE, igual que
    // `tylluan_remember`, pero hasta ahora escribía sin capa 1 ni juez: B-9
    // midió 8/8 muestras de inyección aceptadas aquí sin ninguna marca. Misma
    // severidad que `remember`: rechazo duro, antes de tocar la BD.
    if crate::security::poison_patterns::matches_injection_pattern(content) {
        tracing::warn!("memory/write rejected: content matches a known injection pattern (ASI06 Layer 1)");
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "ACCESS_DENIED",
                "reason": "content matches a known prompt-injection pattern and was rejected before being written to memory."
            })),
        ).into_response();
    }
    // Previously hardcoded to node_id "manual" -- every single call to this
    // endpoint (e.g. guilds/core/coloquio_digest.py's _store()) silently
    // overwrote the same node instead of creating a new one, losing all
    // prior content on each write. Generate a unique id per call, same
    // pattern as handler_remember.rs's task_id/nid generation.
    // T-DSH-134: antes `memory_write:{millis}`; dos escrituras concurrentes en
    // el mismo milisegundo compartian id y la segunda sobrescribia a la primera
    // (ambas devolvian 200). uuid v4 elimina la colision.
    let node_id = new_memory_write_node_id();
    // Accept optional agent_id/owner_scope/node_type so callers can attribute
    // what they write instead of it landing as an anonymous "entity" node --
    // closes the gap where coloquio_digest.py summaries had no structured
    // link back to the contributing agent's identity. owner_scope goes into
    // the real indexed `owner_scope` column (schema.rs v16), not buried in
    // the metadata JSON blob -- agent_id stays in metadata since there's no
    // dedicated column for it.
    let agent_id = req.get("agent_id").and_then(|v| v.as_str());
    let owner_scope = req.get("owner_scope").and_then(|v| v.as_str());
    let node_type = req.get("node_type").and_then(|v| v.as_str()).unwrap_or("entity");
    let metadata = serde_json::json!({ "agent_id": agent_id }).to_string();
    let opts = crate::memory::silva::NodeWriteOptions::new("agent_generated").owner_scope(owner_scope);
    match state.silva.upsert_node_with_validity(&node_id, node_type, content, &metadata, opts).await {
        Ok(_) => {
            // D-17: capa 2 (juez LLM, fire-and-forget) también aquí — B-9 midió
            // que esta ruta no llegaba nunca al juez. Mismo patrón que
            // `handler_remember.rs`: no bloquea la respuesta ni falla la escritura.
            crate::security::write_gate::spawn_write_gate_judge(
                state.silva.clone(),
                node_id.clone(),
                content.to_string(),
            );
            (StatusCode::OK, Json(serde_json::json!({ "status": "ok", "node_id": node_id }))).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "write failed" }))).into_response(),
    }
}

pub async fn memory_search_handler(State(state): State<Arc<HttpState>>, Query(p): Query<MemorySearchQuery>) -> impl IntoResponse {
    let query = p.q.as_deref().unwrap_or("");
    let limit = p.limit.unwrap_or(20);
    let query_embedding = state.matcher.engine().and_then(|e| {
        tokio::task::block_in_place(|| {
            state.silva.query_embed_cache.get_or_embed(query, |q| e.embed_batch_coalesced(q))
        })
        .ok()
    });
    Json(state.silva.search_hybrid(query, query_embedding.as_deref(), limit, None, false).await.unwrap_or_default())
}

pub async fn memory_retention_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let silva = state.silva.clone();
    let node_count = silva.node_count().await.unwrap_or(0) as i64;
    let edge_count = silva.edge_count().await.unwrap_or(0);
    let conn_guard = silva.conn_lock();
    let conn = conn_guard.lock().await;
    let fresh_24h: i64 = conn.query_row("SELECT COUNT(*) FROM nodes WHERE created_at > datetime('now', '-24 hours')", [], |r| r.get(0)).unwrap_or(0);
    let stale_7d: i64 = conn.query_row("SELECT COUNT(*) FROM nodes WHERE created_at BETWEEN datetime('now', '-7 days') AND datetime('now', '-24 hours')", [], |r| r.get(0)).unwrap_or(0);
    let cold_30d: i64 = conn.query_row("SELECT COUNT(*) FROM nodes WHERE created_at <= datetime('now', '-7 days')", [], |r| r.get(0)).unwrap_or(0);
    let protected_count: i64 = conn.query_row("SELECT COUNT(*) FROM nodes WHERE protected = 1", [], |r| r.get(0)).unwrap_or(0);
    drop(conn);
    (StatusCode::OK, Json(serde_json::json!({
        "silva": {
            "total_nodes": node_count, "total_edges": edge_count,
            "fresh_24h": fresh_24h, "stale_7d": stale_7d, "cold_30d": cold_30d,
            "protected": protected_count,
            "retention_rate_percent": if node_count > 0 { ((fresh_24h as f64 / node_count as f64) * 100.0).round() as i64 } else { 0 }
        }
    }))).into_response()
}

pub async fn reindex_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let silva = state.silva.clone();
    let broadcast = state.broadcast_tx.clone();

    let maybe_engine = state.matcher.engine_arc();
    let engine = match maybe_engine {
        Some(e) => e,
        None => {
            // Check if model is configured but not loaded
            let config = state.config.read().await;
            if config.memory.embedding_model == "none" || config.memory.embedding_model.is_empty() {
                return (StatusCode::OK, Json(serde_json::json!({
                    "status": "skipped",
                    "reason": "BM25-only mode, no embeddings to reindex"
                }))).into_response();
            }
            return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({
                "error": "embedding engine not loaded yet"
            }))).into_response();
        }
    };

    let model_id = engine.engine_id();
    let model_hash = engine.engine_hash();
    // T-DSH-136: pausa por nodo configurable ([memory] reindex_node_delay_ms,
    // default 200 ms = comportamiento historico). T-DSH-138: tipos excluidos.
    let (reindex_delay_ms, reindex_skip_types) = {
        let cfg = state.config.read().await;
        (cfg.memory.reindex_node_delay_ms, cfg.memory.reindex_skip_types.clone())
    };
    let total = silva.node_count().await.unwrap_or(0);
    let stale_nodes = silva
        .get_stale_embeddings(&model_id, model_hash.as_deref(), &reindex_skip_types)
        .await
        .unwrap_or_default();
    let stale_count = stale_nodes.len();

    tokio::spawn(async move {
        let _ = broadcast.send(serde_json::json!({
            "type": "reindex_started",
            "stale": stale_count,
            "total": total,
            "ts": chrono::Utc::now().timestamp_millis()
        }));

        let mut done = 0usize;
        for node_id in &stale_nodes {
            if let Ok(Some(node)) = silva.get_node(node_id).await {
                let contextual = build_contextual_text(&node.metadata, &node.content);
                let _ = tokio::task::block_in_place(|| engine.embed_batch_coalesced(&contextual)).map(|vector| {
                    let sid = silva.clone();
                    let nid = node_id.clone();
                    let mid = model_id.clone();
                    let mhash = model_hash.clone();
                    tokio::spawn(async move {
                        let _ = sid.save_embedding(&nid, &vector, &mid, mhash.as_deref()).await;
                    });
                });
            }
            done += 1;
            if done.is_multiple_of(10) {
                let _ = broadcast.send(serde_json::json!({
                    "type": "reindex_progress",
                    "done": done,
                    "stale": stale_count,
                    "total": total,
                    "ts": chrono::Utc::now().timestamp_millis()
                }));
            }
            tokio::time::sleep(std::time::Duration::from_millis(reindex_delay_ms)).await;
        }

        let _ = broadcast.send(serde_json::json!({
            "type": "reindex_finished",
            "indexed": done,
            "stale": stale_count,
            "total": total,
            "ok": true,
            "ts": chrono::Utc::now().timestamp_millis()
        }));
    });

    (StatusCode::ACCEPTED, Json(serde_json::json!({
        "status": "started",
        "stale": stale_count,
        "total": total
    }))).into_response()
}

pub async fn agent_memories_handler(
    State(state): State<Arc<HttpState>>,
    Path(agent_id): Path<String>,
) -> impl IntoResponse {
    let mgr = crate::memory::agent_memory::AgentMemoryManager::new(state.silva.clone(), 20);
    let memories = mgr.get_memories(&agent_id, 50).await;
    let list: Vec<serde_json::Value> = memories.into_iter().map(|n| {
        serde_json::json!({
            "id": n.id,
            "node_type": n.node_type,
            "content": n.content.chars().take(200).collect::<String>(),
            "weight": n.weight,
            "created_at": n.created_at,
            "metadata": n.metadata,
        })
    }).collect();
    (StatusCode::OK, Json(serde_json::json!({ "agent_id": agent_id, "memories": list }))).into_response()
}

pub async fn agent_memories_summary_handler(
    State(state): State<Arc<HttpState>>,
    Path(agent_id): Path<String>,
) -> impl IntoResponse {
    let mgr = crate::memory::agent_memory::AgentMemoryManager::new(state.silva.clone(), 20);
    match mgr.get_summary(&agent_id).await {
        Some(node) => (StatusCode::OK, Json(serde_json::json!({
            "agent_id": agent_id,
            "summary": {
                "id": node.id,
                "content": node.content,
                "created_at": node.created_at,
                "updated_at": node.updated_at,
                "metadata": node.metadata,
            }
        }))).into_response(),
        None => (StatusCode::OK, Json(serde_json::json!({ "agent_id": agent_id, "summary": null }))).into_response(),
    }
}

pub async fn agent_memories_delete_handler(
    State(state): State<Arc<HttpState>>,
    Path(agent_id): Path<String>,
) -> impl IntoResponse {
    match state.silva.forget_agent(&agent_id).await {
        Ok(deleted) => (StatusCode::OK, Json(serde_json::json!({ "ok": true, "deleted": deleted, "agent_id": agent_id }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "ok": false, "error": e.to_string(), "agent_id": agent_id }))).into_response(),
    }
}

// ── D-12: ASI06 quarantine (operator class) ─────────────────────────────────
//
// Contract frozen with Antigravity (lote AE). Field names are consumed by the
// dashboard panel; do not rename without telling the TL.
//
//   GET  /api/v1/memory/quarantine?limit=50&offset=0
//        -> {"total":N,"nodes":[{"id","content_preview","quarantine_reason",
//            "provenance","agent_id"|null,"created_at"(int)}]}
//   POST /api/v1/memory/quarantine/{id}/release
//        -> {"released":true,"id"} | 404 {"error":"not_found"}
//
// Both are operator class (master token by header): release undoes a security
// decision, and the list exposes text the gate decided was hostile.

#[derive(serde::Deserialize)]
pub struct QuarantineListQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

/// Contract: `limit` max 200. Clamped instead of rejected — a panel asking for
/// 500 gets 200, not an error it has to special-case.
const QUARANTINE_MAX_LIMIT: i64 = 200;
const QUARANTINE_DEFAULT_LIMIT: i64 = 50;
const QUARANTINE_PREVIEW_CHARS: usize = 200;

pub async fn quarantine_list_handler(
    State(state): State<Arc<HttpState>>,
    Query(q): Query<QuarantineListQuery>,
) -> impl IntoResponse {
    let limit = q
        .limit
        .unwrap_or(QUARANTINE_DEFAULT_LIMIT)
        .clamp(1, QUARANTINE_MAX_LIMIT);
    let offset = q.offset.unwrap_or(0).max(0);
    match state.silva.list_quarantined(limit, offset).await {
        Ok((total, rows)) => {
            let nodes: Vec<serde_json::Value> = rows
                .into_iter()
                .map(|r| {
                    serde_json::json!({
                        "id": r.id,
                        "content_preview": r.content.chars().take(QUARANTINE_PREVIEW_CHARS).collect::<String>(),
                        "quarantine_reason": r.quarantine_reason,
                        "provenance": r.provenance,
                        "agent_id": r.agent_id,
                        "created_at": r.created_at,
                    })
                })
                .collect();
            (StatusCode::OK, Json(serde_json::json!({ "total": total, "nodes": nodes }))).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn quarantine_release_handler(
    State(state): State<Arc<HttpState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.silva.release_quarantine(&id).await {
        Ok(true) => {
            // Audit with the operator's real identity. `human_intervention` is
            // true only when the credential really was the master token: D-11
            // flagged that this field must not claim a human when it wasn't one.
            let actor = crate::transport::http::auth::current_bound_agent_id()
                .or_else(|| {
                    crate::transport::http::auth::current_acl_role()
                        .map(|r| format!("operator:{r}"))
                })
                .unwrap_or_else(|| "operator".to_string());
            let by_human = matches!(
                crate::transport::http::auth::current_auth_kind(),
                Some(crate::transport::http::auth::AclAuthKind::MasterToken)
            );
            // Fire-and-forget on purpose: `audit_or_warn` escribe la auditoría
            // (un SQLite aparte) y no debe sentarse en el request path (a
            // blocking write there deadlocks the test harness and stalls the
            // caller).
            let node_id = id.clone();
            tokio::task::spawn_blocking(move || {
                crate::transport::server::handler_do::audit_or_warn(
                    "quarantine:release",
                    "memory",
                    "quarantine_release",
                    &actor,
                    true,
                    &format!("released {node_id} from ASI06 quarantine"),
                    0,
                    by_human,
                )
            });
            (StatusCode::OK, Json(serde_json::json!({ "released": true, "id": id }))).into_response()
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "not_found" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}


#[cfg(test)]
mod t134_memory_write_id_tests {
    use super::new_memory_write_node_id;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique_in_a_tight_loop() {
        // Con el formato anterior (timestamp_millis) 1000 llamadas seguidas
        // colisionaban en el mismo milisegundo; con uuid no debe repetirse ninguna.
        let ids: HashSet<String> = (0..1000).map(|_| new_memory_write_node_id()).collect();
        assert_eq!(ids.len(), 1000, "los node_id de memory_write deben ser unicos");
    }

    #[test]
    fn ids_are_unique_under_concurrent_calls() {
        let handles: Vec<_> = (0..8)
            .map(|_| {
                std::thread::spawn(|| {
                    (0..200).map(|_| new_memory_write_node_id()).collect::<Vec<_>>()
                })
            })
            .collect();
        let mut all = HashSet::new();
        for h in handles {
            for id in h.join().expect("thread") {
                all.insert(id);
            }
        }
        assert_eq!(all.len(), 1600, "8x200 node_id concurrentes deben ser unicos");
    }
}
