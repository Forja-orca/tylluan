use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::transport::http::HttpState;

/// An autonomous agent session — one per watcher.
#[derive(Debug, Clone, Serialize)]
pub struct AgentSession {
    pub agent_id: String,
    pub started_at_unix: u64,
    pub last_heartbeat_unix: u64,
    pub responses_sent: u32,
    pub max_responses: u32,
    pub status: String,
}

/// Global registry for autonomous agent watchers.
#[derive(Clone)]
pub struct AgentRegistry {
    pub active: Arc<AtomicBool>,
    pub sessions: Arc<dashmap::DashMap<String, AgentSession>>,
    pub max_responses: Arc<dashmap::DashMap<String, u32>>,
    pub ttl_secs: u64,
}

impl AgentRegistry {
    pub fn new(ttl_secs: u64) -> Self {
        Self {
            active: Arc::new(AtomicBool::new(false)),
            sessions: Arc::new(dashmap::DashMap::new()),
            max_responses: Arc::new(dashmap::DashMap::new()),
            ttl_secs,
        }
    }

    pub fn is_session_valid(&self, agent_id: &str) -> bool {
        if !self.active.load(Ordering::Acquire) {
            return false;
        }
        let Some(entry) = self.sessions.get(agent_id) else {
            return false;
        };
        if entry.status != "active" {
            return false;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if now > entry.started_at_unix + self.ttl_secs {
            return false;
        }
        if entry.responses_sent >= entry.max_responses {
            return false;
        }
        true
    }
}

#[derive(Deserialize)]
pub struct StartSessionRequest {
    pub agent_id: String,
    #[serde(default = "default_max_responses")]
    pub max_responses: u32,
}

fn default_max_responses() -> u32 { 50 }

#[derive(Deserialize)]
pub struct StopSessionRequest {
    pub agent_id: String,
}

#[derive(Deserialize)]
pub struct HeartbeatRequest {
    pub agent_id: String,
}

#[derive(Serialize)]
pub struct StatsResponse {
    pub active: bool,
    pub ttl_secs: u64,
    pub session_count: usize,
    pub sessions: Vec<AgentSummary>,
}

#[derive(Serialize)]
pub struct AgentSummary {
    pub agent_id: String,
    pub status: String,
    pub responses_sent: u32,
    pub max_responses: u32,
    pub uptime_secs: u64,
}

/// POST /api/v1/agents/session/start
pub async fn agent_session_start_handler(
    State(state): State<Arc<HttpState>>,
    Json(req): Json<StartSessionRequest>,
) -> axum::response::Response {
    if req.agent_id.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "agent_id is required"
        }))).into_response();
    }

    if state.agent_registry.sessions.contains_key(&req.agent_id) {
        return (StatusCode::CONFLICT, Json(serde_json::json!({
            "error": format!("session already exists for agent '{}'", req.agent_id)
        }))).into_response();
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let session = AgentSession {
        agent_id: req.agent_id.clone(),
        started_at_unix: now,
        last_heartbeat_unix: now,
        responses_sent: 0,
        max_responses: req.max_responses,
        status: "active".to_string(),
    };

    state.agent_registry.max_responses.insert(req.agent_id.clone(), req.max_responses);
    state.agent_registry.sessions.insert(req.agent_id.clone(), session);
    state.agent_registry.active.store(true, Ordering::Release);

    (StatusCode::OK, Json(serde_json::json!({
        "session_id": req.agent_id.clone(),
        "agent_id": req.agent_id,
        "status": "active",
        "max_responses": req.max_responses,
        "ttl_secs": state.agent_registry.ttl_secs,
    }))).into_response()
}

/// POST /api/v1/agents/session/stop
pub async fn agent_session_stop_handler(
    State(state): State<Arc<HttpState>>,
    Json(req): Json<StopSessionRequest>,
) -> axum::response::Response {
    let mut entry = match state.agent_registry.sessions.get_mut(&req.agent_id) {
        Some(e) => e,
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "error": format!("no session found for agent '{}'", req.agent_id)
        }))).into_response(),
    };

    entry.status = "stopped".to_string();

    let has_active = state.agent_registry.sessions.iter().any(|s| s.status == "active");
    if !has_active {
        state.agent_registry.active.store(false, Ordering::Release);
    }

    (StatusCode::OK, Json(serde_json::json!({
        "status": "stopped",
        "agent_id": req.agent_id
    }))).into_response()
}

/// GET /api/v1/agents/session
pub async fn agent_session_list_handler(
    State(state): State<Arc<HttpState>>,
) -> impl IntoResponse {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let agents: Vec<AgentSession> = state.agent_registry.sessions.iter()
        .map(|entry| {
            let mut s = entry.clone();
            if s.status == "active" && now > s.started_at_unix + state.agent_registry.ttl_secs {
                s.status = "expired".to_string();
            }
            s
        })
        .collect();

    (StatusCode::OK, Json(serde_json::json!({ "sessions": agents })))
}

/// GET /api/v1/agents/stats
pub async fn agent_stats_handler(
    State(state): State<Arc<HttpState>>,
) -> impl IntoResponse {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let sessions: Vec<AgentSummary> = state.agent_registry.sessions.iter()
        .map(|entry| {
            let uptime = now.saturating_sub(entry.started_at_unix);
            AgentSummary {
                agent_id: entry.agent_id.clone(),
                status: entry.status.clone(),
                responses_sent: entry.responses_sent,
                max_responses: entry.max_responses,
                uptime_secs: uptime,
            }
        })
        .collect();

    (StatusCode::OK, Json(serde_json::json!(StatsResponse {
        active: state.agent_registry.active.load(Ordering::Acquire),
        ttl_secs: state.agent_registry.ttl_secs,
        session_count: sessions.len(),
        sessions,
    })))
}

/// POST /api/v1/agents/heartbeat
pub async fn agent_heartbeat_handler(
    State(state): State<Arc<HttpState>>,
    Json(req): Json<HeartbeatRequest>,
) -> axum::response::Response {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut entry = match state.agent_registry.sessions.get_mut(&req.agent_id) {
        Some(e) => e,
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "error": format!("no session found for agent '{}'", req.agent_id)
        }))).into_response(),
    };

    if entry.status != "active" {
        return (StatusCode::CONFLICT, Json(serde_json::json!({
            "error": format!("session '{}' is not active (status={})", req.agent_id, entry.status)
        }))).into_response();
    }

    if now > entry.started_at_unix + state.agent_registry.ttl_secs {
        entry.status = "expired".to_string();
        return (StatusCode::GONE, Json(serde_json::json!({
            "error": "session TTL expired",
            "status": "expired",
            "agent_id": req.agent_id,
        }))).into_response();
    }

    if entry.responses_sent >= entry.max_responses {
        entry.status = "stopped".to_string();
        return (StatusCode::GONE, Json(serde_json::json!({
            "error": "max_responses budget exhausted",
            "status": "stopped",
            "agent_id": req.agent_id,
        }))).into_response();
    }

    entry.last_heartbeat_unix = now;

    (StatusCode::OK, Json(serde_json::json!({
        "status": "ok",
        "active": state.agent_registry.active.load(Ordering::Acquire),
        "agent_id": req.agent_id,
        "responses_sent": entry.responses_sent,
        "max_responses": entry.max_responses,
        "ttl_remaining_secs": (entry.started_at_unix + state.agent_registry.ttl_secs).saturating_sub(now),
    }))).into_response()
}
