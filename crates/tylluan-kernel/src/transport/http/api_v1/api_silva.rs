use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;
use crate::transport::http::HttpState;

pub async fn tylluan_graph_get_handler(
    State(state): State<Arc<HttpState>>,
) -> impl IntoResponse {
    let server_arc = match state.server.as_ref() {
        Some(s) => s,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"error": "Sovereign server not initialized"}))).into_response(),
    };
    let server = server_arc.read().await;
    match crate::transport::server::handler_graph::handle_tylluan_graph(&server, None).await {
        Ok(res) => (StatusCode::OK, Json(res)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

pub async fn tylluan_graph_handler(
    State(state): State<Arc<HttpState>>,
    Json(args): Json<serde_json::Value>,
) -> impl IntoResponse {
    let server_arc = match state.server.as_ref() {
        Some(s) => s,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"error": "Sovereign server not initialized"}))).into_response(),
    };
    let server = server_arc.read().await;
    match crate::transport::server::handler_graph::handle_tylluan_graph(&server, args.as_object().cloned()).await {
        Ok(res) => (StatusCode::OK, Json(res)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

pub async fn agents_list_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let profiles = if let Some(ref srv_arc) = state.server {
        let srv = srv_arc.read().await;
        if let Some(ref ap) = srv.agent_profiles {
            if let Ok(p) = ap.lock() {
                p.list_profiles().unwrap_or_default()
            } else { vec![] }
        } else { vec![] }
    } else { vec![] };

    let reputation: Vec<serde_json::Value> = if let Some(ref srv_arc) = state.server {
        let srv = srv_arc.read().await;
        if let Some(ref ap) = srv.agent_profiles {
            match ap.lock() {
                Ok(g) => match g.get_domain_reputation() { Ok(v) => v, Err(_) => vec![] },
                Err(_) => vec![],
            }
        } else { vec![] }
    } else { vec![] };

    let mut domain_map: std::collections::HashMap<String, Vec<serde_json::Value>> =
        std::collections::HashMap::new();
    for rep in &reputation {
        if let Some(aid) = rep.get("agent_id").and_then(|v: &serde_json::Value| v.as_str()) {
            domain_map.entry(aid.to_string()).or_default().push(rep.clone());
        }
    }

    let agents: Vec<serde_json::Value> = profiles.iter().map(|p| {
        let domains = domain_map.get(&p.agent_id).cloned().unwrap_or_default();
        let identity_node = format!("agent_identity_{}", p.agent_id);
        serde_json::json!({
            "agent_id": p.agent_id,
            "role": p.role,
            "total_calls": p.total_calls,
            "first_seen": p.first_seen,
            "last_intent": p.last_intent,
            "competencies": p.competencies,
            "identity_node": identity_node,
            "domains": domains
        })
    }).collect();

    (StatusCode::OK, Json(serde_json::json!({ "agents": agents, "count": agents.len() }))).into_response()
}
