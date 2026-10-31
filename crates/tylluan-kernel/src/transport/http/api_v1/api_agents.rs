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
use crate::memory::silva::SilvaDB;

/// An autonomous agent session — one per watcher.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// T-DSH-152: handle opcional a SilvaDB para persistir las sesiones en
    /// `silva.db` (tabla aditiva `agent_sessions`) y que sobrevivan al
    /// reinicio del kernel. `None` en los estados de test que no la necesitan.
    silva: Arc<std::sync::Mutex<Option<Arc<SilvaDB>>>>,
}

impl AgentRegistry {
    pub fn new(ttl_secs: u64) -> Self {
        Self {
            active: Arc::new(AtomicBool::new(false)),
            sessions: Arc::new(dashmap::DashMap::new()),
            max_responses: Arc::new(dashmap::DashMap::new()),
            ttl_secs,
            silva: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// T-DSH-152: enlaza el registro con SilvaDB (lo llama el builder de
    /// `HttpState` en produccion, y los tests que quieren persistencia).
    pub fn set_silva(&self, silva: Arc<SilvaDB>) {
        if let Ok(mut guard) = self.silva.lock() {
            *guard = Some(silva);
        }
    }

    fn silva_handle(&self) -> Option<Arc<SilvaDB>> {
        self.silva.lock().ok().and_then(|guard| guard.clone())
    }

    /// T-DSH-152: escribe (upsert) la sesion en `agent_sessions`. Un fallo se
    /// registra y no rompe la respuesta: la sesion sigue viva en memoria.
    pub async fn persist_session(&self, session: &AgentSession) {
        let Some(silva) = self.silva_handle() else {
            return;
        };
        let conn = silva.conn.lock().await;
        if let Err(e) = conn.execute(
            "INSERT INTO agent_sessions
                (agent_id, started_at_unix, last_heartbeat_unix, responses_sent, max_responses, status, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))
             ON CONFLICT(agent_id) DO UPDATE SET
                started_at_unix = excluded.started_at_unix,
                last_heartbeat_unix = excluded.last_heartbeat_unix,
                responses_sent = excluded.responses_sent,
                max_responses = excluded.max_responses,
                status = excluded.status,
                updated_at = datetime('now')",
            rusqlite::params![
                session.agent_id,
                session.started_at_unix as i64,
                session.last_heartbeat_unix as i64,
                session.responses_sent as i64,
                session.max_responses as i64,
                session.status,
            ],
        ) {
            tracing::warn!("T-DSH-152: no se pudo persistir la sesion de agente: {e}");
        }
    }

    /// T-DSH-152: recarga en memoria las sesiones persistidas. Lo llama el
    /// arranque del kernel (tras construir `HttpState`); devuelve cuantas
    /// sesiones se recuperaron. Sin SilvaDB enlazada es un no-op.
    pub async fn rehydrate(&self) -> anyhow::Result<usize> {
        let Some(silva) = self.silva_handle() else {
            return Ok(0);
        };
        let loaded: Vec<AgentSession> = {
            let conn = silva.conn.lock().await;
            let mut stmt = conn.prepare(
                "SELECT agent_id, started_at_unix, last_heartbeat_unix,
                        responses_sent, max_responses, status
                 FROM agent_sessions",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(AgentSession {
                    agent_id: row.get(0)?,
                    started_at_unix: row.get::<_, i64>(1)? as u64,
                    last_heartbeat_unix: row.get::<_, i64>(2)? as u64,
                    responses_sent: row.get::<_, i64>(3)? as u32,
                    max_responses: row.get::<_, i64>(4)? as u32,
                    status: row.get(5)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let count = loaded.len();
        for session in loaded {
            self.max_responses
                .insert(session.agent_id.clone(), session.max_responses);
            self.sessions.insert(session.agent_id.clone(), session);
        }
        if self.sessions.iter().any(|entry| entry.status == "active") {
            self.active.store(true, Ordering::Release);
        }
        Ok(count)
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
    state.agent_registry.sessions.insert(req.agent_id.clone(), session.clone());
    state.agent_registry.active.store(true, Ordering::Release);
    // T-DSH-152: persistir para sobrevivir al reinicio del kernel.
    state.agent_registry.persist_session(&session).await;

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
    let persisted = entry.clone();
    drop(entry);

    let has_active = state.agent_registry.sessions.iter().any(|s| s.status == "active");
    if !has_active {
        state.agent_registry.active.store(false, Ordering::Release);
    }
    // T-DSH-152: persistir el estado "stopped".
    state.agent_registry.persist_session(&persisted).await;

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
        let persisted = entry.clone();
        drop(entry);
        state.agent_registry.persist_session(&persisted).await;
        return (StatusCode::GONE, Json(serde_json::json!({
            "error": "session TTL expired",
            "status": "expired",
            "agent_id": req.agent_id,
        }))).into_response();
    }

    if entry.responses_sent >= entry.max_responses {
        entry.status = "stopped".to_string();
        let persisted = entry.clone();
        drop(entry);
        state.agent_registry.persist_session(&persisted).await;
        return (StatusCode::GONE, Json(serde_json::json!({
            "error": "max_responses budget exhausted",
            "status": "stopped",
            "agent_id": req.agent_id,
        }))).into_response();
    }

    entry.last_heartbeat_unix = now;
    let persisted = entry.clone();
    drop(entry);
    // T-DSH-152: persistir el heartbeat para sobrevivir al reinicio.
    state.agent_registry.persist_session(&persisted).await;

    (StatusCode::OK, Json(serde_json::json!({
        "status": "ok",
        "active": state.agent_registry.active.load(Ordering::Acquire),
        "agent_id": req.agent_id,
        "responses_sent": persisted.responses_sent,
        "max_responses": persisted.max_responses,
        "ttl_remaining_secs": (persisted.started_at_unix + state.agent_registry.ttl_secs).saturating_sub(now),
    }))).into_response()
}

#[derive(Deserialize, Default)]
pub struct WhoamiQuery {
    pub agent_id: Option<String>,
    pub timezone: Option<String>,
}

/// GET /api/v1/whoami (read-only identity inspection, symmetric with tylluan_do whoami).
pub async fn whoami_handler(
    State(state): State<Arc<HttpState>>,
    axum::extract::Query(q): axum::extract::Query<WhoamiQuery>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let agent_id = q.agent_id
        .or_else(|| {
            headers.get("X-Agent-Id")
                .or_else(|| headers.get("x-agent-id"))
                .and_then(|v| v.to_str().ok())
                .map(String::from)
        })
        .unwrap_or_else(|| "anonymous".to_string());

    let server_arc = match state.server.as_ref() {
        Some(s) => s,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({
            "error": "Sovereign server not initialized"
        }))).into_response(),
    };

    let server = server_arc.read().await;
    let mut args = serde_json::Map::new();
    args.insert("agent_id".to_string(), serde_json::Value::String(agent_id));
    if let Some(tz) = q.timezone {
        args.insert("timezone".to_string(), serde_json::Value::String(tz));
    }

    match server.handle_kernel_tool("whoami", Some(args)).await {
        Ok(res) => {
            let is_error = res.is_error.unwrap_or(false);
            let text = res.content.iter()
                .find_map(|c| c.as_text().map(|t| t.text.as_str()))
                .unwrap_or("{}");
            let val: serde_json::Value = serde_json::from_str(text)
                .unwrap_or_else(|_| serde_json::json!({ "raw": text }));
            if is_error {
                (StatusCode::BAD_REQUEST, Json(val)).into_response()
            } else {
                (StatusCode::OK, Json(val)).into_response()
            }
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "error": e.to_string()
        }))).into_response(),
    }
}

