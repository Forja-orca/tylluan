use axum::{
    Json,
    extract::{State, Path, Query},
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;
//
use crate::transport::http::{HttpState, MemorySearchQuery};

pub async fn memory_write_handler(State(state): State<Arc<HttpState>>, Json(req): Json<serde_json::Value>) -> impl IntoResponse {
    let content = match req.get("content").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "missing content" }))).into_response(),
    };
    match state.silva.upsert_node("manual", "entity", content, "{}").await {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({ "status": "ok" }))).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "write failed" }))).into_response(),
    }
}

pub async fn memory_search_handler(State(state): State<Arc<HttpState>>, Query(p): Query<MemorySearchQuery>) -> impl IntoResponse {
    let query = p.q.as_deref().unwrap_or("");
    let limit = p.limit.unwrap_or(20);
    let query_embedding = state.matcher.engine().and_then(|e| e.embed(query).ok());
    Json(state.silva.search_hybrid(query, query_embedding.as_deref(), limit).await.unwrap_or_default())
}

pub async fn memory_retention_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let silva = state.silva.clone();
    let memory = state.memory.clone();
    let node_count = silva.node_count().await.unwrap_or(0) as i64;
    let edge_count = silva.edge_count().await.unwrap_or(0) as i64;
    let memory_stats = memory.stats().await.ok();
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
        },
        "hybrid_memory": {
            "documents": memory_stats.as_ref().map(|s| s.document_count).unwrap_or(0),
            "disk_bytes": memory_stats.as_ref().map(|s| s.total_bytes).unwrap_or(0)
        }
    }))).into_response()
}

pub async fn reindex_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let silva = state.silva.clone();
    let broadcast = state.broadcast_tx.clone();
    
    tokio::spawn(async move {
        let _ = broadcast.send(serde_json::json!({
            "type": "maintenance_started",
            "task": "reindex",
            "ts": chrono::Utc::now().timestamp_millis()
        }));
        
        let _ = silva.detect_communities().await;
        let _ = silva.checkpoint().await;
        
        let _ = broadcast.send(serde_json::json!({
            "type": "maintenance_finished",
            "task": "reindex",
            "ok": true,
            "ts": chrono::Utc::now().timestamp_millis()
        }));
    });
    
    (StatusCode::ACCEPTED, Json(serde_json::json!({"status": "started", "task": "reindex"})))
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
