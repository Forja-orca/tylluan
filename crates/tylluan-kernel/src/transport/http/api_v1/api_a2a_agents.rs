use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

use crate::transport::http::a2a_client::{A2aClient, ExternalAgent};
use crate::transport::http::{HttpState, Utf8Json};

#[derive(Deserialize)]
pub struct UpsertA2aAgentBody {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub auth_token: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

/// Wire-safe view of an external agent: never echoes the stored auth token.
#[derive(serde::Serialize)]
pub struct ExternalAgentView {
    pub id: String,
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<&ExternalAgent> for ExternalAgentView {
    fn from(a: &ExternalAgent) -> Self {
        Self {
            id: a.id.clone(),
            name: a.name.clone(),
            url: a.url.clone(),
            enabled: a.enabled,
            created_at: a.created_at,
            updated_at: a.updated_at,
        }
    }
}

fn validate_url(url: &str) -> Result<(), String> {
    if url.starts_with("http://") || url.starts_with("https://") {
        Ok(())
    } else {
        Err("url must start with http:// or https://".to_string())
    }
}

/// GET /api/v1/a2a/agents — list configured external agents (no secrets).
pub async fn a2a_agents_list_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    match state.a2a_agents.load_all().await {
        Ok(agents) => {
            let view: Vec<ExternalAgentView> = agents.iter().map(ExternalAgentView::from).collect();
            (StatusCode::OK, Utf8Json(serde_json::json!({"ok": true, "agents": view})))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Utf8Json(serde_json::json!({"ok": false, "error": e})),
        ),
    }
}

/// POST /api/v1/a2a/agents — create a new external agent entry.
pub async fn a2a_agents_create_handler(
    State(state): State<Arc<HttpState>>,
    Json(body): Json<UpsertA2aAgentBody>,
) -> impl IntoResponse {
    if let Err(msg) = validate_url(&body.url) {
        return (StatusCode::BAD_REQUEST, Utf8Json(serde_json::json!({"ok": false, "error": msg})));
    }
    let mut agent = ExternalAgent::new(&body.name, &body.url, &body.auth_token);
    agent.enabled = body.enabled;
    match state.a2a_agents.upsert(&agent).await {
        Ok(()) => (
            StatusCode::CREATED,
            Utf8Json(serde_json::json!({"ok": true, "agent": ExternalAgentView::from(&agent)})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Utf8Json(serde_json::json!({"ok": false, "error": e})),
        ),
    }
}

/// PUT /api/v1/a2a/agents/{id} — update an existing external agent.
pub async fn a2a_agents_update_handler(
    State(state): State<Arc<HttpState>>,
    Path(id): Path<String>,
    Json(body): Json<UpsertA2aAgentBody>,
) -> impl IntoResponse {
    if let Err(msg) = validate_url(&body.url) {
        return (StatusCode::BAD_REQUEST, Utf8Json(serde_json::json!({"ok": false, "error": msg})));
    }
    let existing = match state.a2a_agents.get(&id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Utf8Json(serde_json::json!({"ok": false, "error": format!("agent '{id}' not found")})),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Utf8Json(serde_json::json!({"ok": false, "error": e})),
            );
        }
    };
    let mut agent = existing;
    agent.name = body.name;
    agent.url = body.url;
    agent.auth_token = body.auth_token;
    agent.enabled = body.enabled;
    agent.updated_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    match state.a2a_agents.upsert(&agent).await {
        Ok(()) => (
            StatusCode::OK,
            Utf8Json(serde_json::json!({"ok": true, "agent": ExternalAgentView::from(&agent)})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Utf8Json(serde_json::json!({"ok": false, "error": e})),
        ),
    }
}

/// DELETE /api/v1/a2a/agents/{id} — remove an external agent.
pub async fn a2a_agents_delete_handler(
    State(state): State<Arc<HttpState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.a2a_agents.remove(&id).await {
        Ok(true) => (StatusCode::OK, Utf8Json(serde_json::json!({"ok": true}))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Utf8Json(serde_json::json!({"ok": false, "error": format!("agent '{id}' not found")})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Utf8Json(serde_json::json!({"ok": false, "error": e})),
        ),
    }
}

/// POST /api/v1/a2a/agents/{id}/test — card discovery + one message roundtrip.
pub async fn a2a_agents_test_handler(
    State(state): State<Arc<HttpState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let agent = match state.a2a_agents.get(&id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Utf8Json(serde_json::json!({"ok": false, "error": format!("agent '{id}' not found")})),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Utf8Json(serde_json::json!({"ok": false, "error": e})),
            );
        }
    };
    if !agent.enabled {
        return (
            StatusCode::CONFLICT,
            Utf8Json(serde_json::json!({"ok": false, "error": format!("agent '{}' is disabled", agent.name)})),
        );
    }

    let started = std::time::Instant::now();
    let card = match tokio::time::timeout(
        Duration::from_secs(10),
        state.a2a_client.fetch_card(&agent),
    )
    .await
    {
        Ok(Ok(card)) => card,
        Ok(Err(e)) => {
            return (
                StatusCode::BAD_GATEWAY,
                Utf8Json(serde_json::json!({
                    "ok": false,
                    "error": format!("card discovery failed: {e}"),
                    "latency_ms": started.elapsed().as_millis() as u64,
                })),
            );
        }
        Err(_) => {
            return (
                StatusCode::GATEWAY_TIMEOUT,
                Utf8Json(serde_json::json!({
                    "ok": false,
                    "error": "card discovery timed out after 10s",
                    "latency_ms": started.elapsed().as_millis() as u64,
                })),
            );
        }
    };

    let endpoint = match A2aClient::resolve_endpoint(&card, &agent.url) {
        Ok(e) => e,
        Err(msg) => {
            return (
                StatusCode::BAD_GATEWAY,
                Utf8Json(serde_json::json!({"ok": false, "error": msg})),
            );
        }
    };

    let task = match tokio::time::timeout(Duration::from_secs(20), state.a2a_client.message_send(&agent, &endpoint, "ping")).await {
        Ok(Ok(task)) => task,
        Ok(Err(e)) => {
            return (
                StatusCode::BAD_GATEWAY,
                Utf8Json(serde_json::json!({
                    "ok": false,
                    "error": format!("message/send failed: {e}"),
                    "latency_ms": started.elapsed().as_millis() as u64,
                })),
            );
        }
        Err(_) => {
            return (
                StatusCode::GATEWAY_TIMEOUT,
                Utf8Json(serde_json::json!({
                    "ok": false,
                    "error": "message roundtrip timed out after 20s",
                    "latency_ms": started.elapsed().as_millis() as u64,
                })),
            );
        }
    };

    let reply = A2aClient::task_text(&task);
    (
        StatusCode::OK,
        Utf8Json(serde_json::json!({
            "ok": true,
            "agent": agent.name,
            "card_name": card.name,
            "protocol_version": card.protocol_version,
            "endpoint": endpoint,
            "task_state": task.resolved_state().as_str(),
            "reply": reply.chars().take(200).collect::<String>(),
            "latency_ms": started.elapsed().as_millis() as u64,
        })),
    )
}
