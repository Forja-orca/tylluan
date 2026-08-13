//! # A2A Outbound Client (A2A spec v0.3.0)
//!
//! Talks to *external* A2A agents over JSON-RPC 2.0, mirroring the wire shape
//! of the official `a2a-sdk` (Linux Foundation / Google) so Tylluan can
//! federate with any conformant agent, not just its own kernel peers.
//!
//! ## Wire compatibility notes (verified against a2a-sdk)
//!
//! - Task state lives under `status.state` (v0.3.0 wrapper). The older flat
//!   `state` shape is accepted as a fallback so v0.2.x peers still work.
//! - Text parts use `parts: [{"kind": "text", "text": ...}]` (v0.3.0).
//! - JSON-RPC endpoint is taken from the remote Agent Card's `url` field,
//!   falling back to the configured base URL. Discovery always hits
//!   `{base}/.well-known/agent-card.json` per spec.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const A2A_PROTOCOL_VERSION: &str = "0.3.0";

// ─── External agent configuration (persisted at runtime) ─────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalAgent {
    pub id: String,
    pub name: String,
    /// Base URL of the agent. Discovery appends `/.well-known/agent-card.json`;
    /// task dispatch uses the card's `url` endpoint when present.
    pub url: String,
    #[serde(default)]
    pub auth_token: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

fn default_enabled() -> bool {
    true
}

impl ExternalAgent {
    pub fn new(name: &str, url: &str, auth_token: &str) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        ExternalAgent {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            url: url.to_string(),
            auth_token: auth_token.to_string(),
            enabled: true,
            created_at: now,
            updated_at: now,
        }
    }
}

// ─── Agent store (persisted in SilvaDB, table a2a_agents) ────────────────────────

/// CRUD over the `a2a_agents` table. Runtime config: written and read via
/// REST, persisted in SilvaDB so the roster survives kernel restarts.
pub struct A2aAgentStore {
    silva: Arc<crate::memory::silva::SilvaDB>,
}

impl A2aAgentStore {
    pub fn new(silva: Arc<crate::memory::silva::SilvaDB>) -> Self {
        Self { silva }
    }

    pub async fn upsert(&self, agent: &ExternalAgent) -> Result<(), String> {
        let r = tokio::task::block_in_place(|| {
            let conn = self.silva.conn_lock();
            let conn = conn.blocking_lock();
            conn.execute(
                "INSERT INTO a2a_agents(id,name,url,auth_token,enabled,created_at,updated_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7)
                 ON CONFLICT(id) DO UPDATE SET
                     name=excluded.name,
                     url=excluded.url,
                     auth_token=excluded.auth_token,
                     enabled=excluded.enabled,
                     updated_at=excluded.updated_at",
                rusqlite::params![
                    agent.id, agent.name, agent.url, agent.auth_token,
                    agent.enabled as i64, agent.created_at, agent.updated_at
                ],
            )
        });
        match r {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("a2a_agents upsert failed: {e}")),
        }
    }

    pub async fn remove(&self, id: &str) -> Result<bool, String> {
        let n = tokio::task::block_in_place(|| {
            let conn = self.silva.conn_lock();
            let conn = conn.blocking_lock();
            conn.execute("DELETE FROM a2a_agents WHERE id=?1", rusqlite::params![id])
        });
        match n {
            Ok(n) => Ok(n > 0),
            Err(e) => Err(format!("a2a_agents delete failed: {e}")),
        }
    }

    pub async fn get(&self, id: &str) -> Result<Option<ExternalAgent>, String> {
        tokio::task::block_in_place(|| {
            let conn = self.silva.conn_lock();
            let conn = conn.blocking_lock();
            match conn.prepare(
                "SELECT id,name,url,auth_token,enabled,created_at,updated_at
                 FROM a2a_agents WHERE id=?1",
            ) {
                Ok(mut stmt) => stmt
                    .query_row(rusqlite::params![id], row_to_agent)
                    .optional()
                    .map_err(|e| format!("a2a_agents select failed: {e}")),
                Err(e) => Err(format!("a2a_agents select failed: {e}")),
            }
        })
    }

    pub async fn load_all(&self) -> Result<Vec<ExternalAgent>, String> {
        tokio::task::block_in_place(|| {
            let conn = self.silva.conn_lock();
            let conn = conn.blocking_lock();
            match conn.prepare(
                "SELECT id,name,url,auth_token,enabled,created_at,updated_at
                 FROM a2a_agents ORDER BY created_at ASC",
            ) {
                Ok(mut stmt) => stmt
                    .query_map([], row_to_agent)
                    .map_err(|e| format!("a2a_agents list failed: {e}"))
                    .and_then(|rows| {
                        rows.collect::<rusqlite::Result<Vec<_>>>()
                            .map_err(|e| format!("a2a_agents list failed: {e}"))
                    }),
                Err(e) => Err(format!("a2a_agents list failed: {e}")),
            }
        })
    }
}

fn row_to_agent(row: &rusqlite::Row<'_>) -> rusqlite::Result<ExternalAgent> {
    Ok(ExternalAgent {
        id: row.get(0)?,
        name: row.get(1)?,
        url: row.get(2)?,
        auth_token: row.get(3)?,
        enabled: row.get::<_, i64>(4)? != 0,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

use rusqlite::OptionalExtension;

// ─── Remote Agent Card (wire subset, deserialize-only) ───────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteAgentCard {
    #[serde(rename = "protocolVersion", default)]
    pub protocol_version: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// JSON-RPC endpoint to dispatch tasks to (spec v0.3.0).
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub capabilities: Option<serde_json::Value>,
    #[serde(rename = "defaultInputModes", default)]
    pub default_input_modes: Vec<String>,
    #[serde(rename = "defaultOutputModes", default)]
    pub default_output_modes: Vec<String>,
    #[serde(rename = "securitySchemes", default)]
    pub security_schemes: Option<serde_json::Value>,
    #[serde(default)]
    pub skills: Vec<RemoteAgentSkill>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteAgentSkill {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
}

// ─── Remote task (wire shape) ────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteTaskState {
    Submitted,
    Working,
    #[serde(rename = "input-required")]
    InputRequired,
    Completed,
    Canceled,
    Failed,
    Rejected,
    #[serde(rename = "auth-required")]
    AuthRequired,
    Unknown,
}

impl RemoteTaskState {
    pub fn as_str(&self) -> &'static str {
        match self {
            RemoteTaskState::Submitted => "submitted",
            RemoteTaskState::Working => "working",
            RemoteTaskState::InputRequired => "input-required",
            RemoteTaskState::Completed => "completed",
            RemoteTaskState::Canceled => "canceled",
            RemoteTaskState::Failed => "failed",
            RemoteTaskState::Rejected => "rejected",
            RemoteTaskState::AuthRequired => "auth-required",
            RemoteTaskState::Unknown => "unknown",
        }
    }
}

/// v0.3.0 `TaskStatus` wrapper: `{state, message?, timestamp?}`.
#[derive(Debug, Clone, Deserialize)]
pub struct RemoteTaskStatus {
    pub state: RemoteTaskState,
    #[serde(default)]
    pub message: Option<serde_json::Value>,
    #[serde(default)]
    pub timestamp: Option<String>,
}

/// Tolerant `Task` shape: accepts both `status.state` (v0.3.0) and the flat
/// `state`/`message` layout (v0.2.x) used by some peers.
#[derive(Debug, Clone, Deserialize)]
pub struct RemoteTask {
    pub id: String,
    #[serde(default)]
    pub status: Option<RemoteTaskStatus>,
    #[serde(default)]
    pub state: Option<RemoteTaskState>,
    #[serde(default)]
    pub message: Option<serde_json::Value>,
    #[serde(default)]
    pub artifacts: Vec<serde_json::Value>,
}

impl RemoteTask {
    pub fn resolved_state(&self) -> RemoteTaskState {
        self.status
            .as_ref()
            .map(|s| s.state.clone())
            .or_else(|| self.state.clone())
            .unwrap_or(RemoteTaskState::Unknown)
    }

    pub fn resolved_message(&self) -> Option<&serde_json::Value> {
        self.status
            .as_ref()
            .and_then(|s| s.message.as_ref())
            .or(self.message.as_ref())
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self.resolved_state(),
            RemoteTaskState::Completed
                | RemoteTaskState::Canceled
                | RemoteTaskState::Failed
                | RemoteTaskState::Rejected
                | RemoteTaskState::InputRequired
                | RemoteTaskState::AuthRequired
        )
    }
}

// ─── A2A client ──────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct JsonRpcEnvelope {
    #[serde(default)]
    result: Option<serde_json::Value>,
    #[serde(default)]
    error: Option<JsonRpcErrorBody>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcErrorBody {
    code: i64,
    message: String,
}

#[derive(Clone)]
pub struct A2aClient {
    http: reqwest::Client,
}

impl A2aClient {
    pub fn new() -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .user_agent(concat!("tylluan/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self { http })
    }

    /// `GET {base}/.well-known/agent-card.json` per A2A spec.
    pub fn card_url(base_url: &str) -> String {
        format!("{base_url}/.well-known/agent-card.json", base_url = base_url.trim_end_matches('/'))
    }

    pub async fn fetch_card(&self, agent: &ExternalAgent) -> anyhow::Result<RemoteAgentCard> {
        let url = Self::card_url(&agent.url);
        let mut req = self.http.get(&url);
        if !agent.auth_token.is_empty() {
            req = req.bearer_auth(&agent.auth_token);
        }
        let resp = req.send().await?;
        let status = resp.status();
        if status.is_client_error() || status.is_server_error() {
            anyhow::bail!("agent card fetch for '{}' failed: HTTP {}", agent.name, status);
        }
        resp.json().await.map_err(|e| {
            anyhow::anyhow!("agent card from '{url}' is not valid JSON ({e})")
        })
    }

    /// Resolve the dispatch endpoint and validate the card (F4 hardening hooks
    /// into this). Returns the endpoint URL for task dispatch.
    pub fn resolve_endpoint(
        card: &RemoteAgentCard,
        base_url: &str,
    ) -> Result<String, String> {
        if card.name.trim().is_empty() {
            return Err("agent card has no name".into());
        }
        if !card.protocol_version.is_empty() && !card.protocol_version.starts_with("0.3") {
            return Err(format!(
                "unsupported A2A protocolVersion '{v}' (need 0.3.x)",
                v = card.protocol_version
            ));
        }
        let endpoint = card
            .url
            .as_deref()
            .filter(|u| !u.trim().is_empty())
            .map(|u| u.to_string())
            .unwrap_or_else(|| {
                // Card without a url (a2a-sdk v1.1.x dropped the field from the
                // proto): the JSON-RPC endpoint lives at {base}/a2a. Found live
                // against the official SDK echo agent 2026-08-13.
                format!("{}/a2a", base_url.trim_end_matches('/'))
            });
        if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
            return Err(format!("agent endpoint '{endpoint}' is not an http(s) URL"));
        }
        Ok(endpoint)
    }

    async fn jsonrpc(
        &self,
        agent: &ExternalAgent,
        endpoint: &str,
        method: &str,
        params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });
        let mut req = self
            .http
            .post(endpoint)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json");
        if !agent.auth_token.is_empty() {
            req = req.bearer_auth(&agent.auth_token);
        }
        let resp = req.json(&body).send().await?;
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            anyhow::bail!("A2A {method} to '{}' rejected auth (HTTP {status})", agent.name);
        }
        if status.is_server_error() {
            anyhow::bail!("A2A {method} to '{}' failed: HTTP {status}", agent.name);
        }
        let envelope: JsonRpcEnvelope = resp.json().await.map_err(|e| {
            anyhow::anyhow!("A2A {method} to '{}': non-JSON-RPC response ({e})", agent.name)
        })?;
        if let Some(err) = envelope.error {
            anyhow::bail!(
                "A2A {method} to '{}': JSON-RPC error {}: {}",
                agent.name,
                err.code,
                err.message
            );
        }
        envelope
            .result
            .ok_or_else(|| anyhow::anyhow!("A2A {method} to '{}': empty result", agent.name))
    }

    /// Send a text intent (`message/send`). Task usually returns in a
    /// non-terminal state; poll with [`Self::tasks_get`] / [`Self::run_task`].
    ///
    /// Wire shape follows the official a2a-sdk v0.3 models exactly: `message`
    /// requires `kind: "message"` and a sender-generated `message_id` (UUID),
    /// plus `role` and `parts[].kind` discriminators. Verified live against
    /// a a2a-sdk v1.1.2 server in v0.3-compat mode 2026-08-13; missing any of
    /// these gets `-32600 Invalid Request` from `model_validate`.
    pub async fn message_send(
        &self,
        agent: &ExternalAgent,
        endpoint: &str,
        text: &str,
    ) -> anyhow::Result<RemoteTask> {
        let result = self
            .jsonrpc(
                agent,
                endpoint,
                "message/send",
                serde_json::json!({
                    "message": {
                        "kind": "message",
                        "message_id": uuid::Uuid::new_v4().to_string(),
                        "role": "user",
                        "parts": [{"kind": "text", "text": text}],
                    }
                }),
            )
            .await?;
        serde_json::from_value(result).map_err(|e| anyhow::anyhow!("A2A message/send: bad task response ({e})"))
    }

    pub async fn tasks_get(
        &self,
        agent: &ExternalAgent,
        endpoint: &str,
        task_id: &str,
    ) -> anyhow::Result<RemoteTask> {
        let result = self
            .jsonrpc(agent, endpoint, "tasks/get", serde_json::json!({ "id": task_id }))
            .await?;
        serde_json::from_value(result).map_err(|e| anyhow::anyhow!("A2A tasks/get: bad task response ({e})"))
    }

    pub async fn tasks_cancel(
        &self,
        agent: &ExternalAgent,
        endpoint: &str,
        task_id: &str,
    ) -> anyhow::Result<RemoteTask> {
        let result = self
            .jsonrpc(agent, endpoint, "tasks/cancel", serde_json::json!({ "id": task_id }))
            .await?;
        serde_json::from_value(result).map_err(|e| anyhow::anyhow!("A2A tasks/cancel: bad task response ({e})"))
    }

    /// Send + poll `tasks/get` until the task is terminal or `timeout` elapses.
    pub async fn run_task(
        &self,
        agent: &ExternalAgent,
        endpoint: &str,
        text: &str,
        timeout: Duration,
    ) -> anyhow::Result<RemoteTask> {
        let mut task = self.message_send(agent, endpoint, text).await?;
        let deadline = Instant::now() + timeout;
        while !task.is_terminal() {
            if Instant::now() >= deadline {
                anyhow::bail!(
                    "A2A task {} on '{}' still {:?} after {}s",
                    task.id,
                    agent.name,
                    task.resolved_state(),
                    timeout.as_secs()
                );
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
            task = self.tasks_get(agent, endpoint, &task.id).await?;
        }
        Ok(task)
    }

    /// Extract the agent's final text from a terminal task (best effort).
    pub fn task_text(task: &RemoteTask) -> String {
        let msg = task.resolved_message();
        let text = msg
            .and_then(|m| m.get("parts"))
            .and_then(|p| p.as_array())
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(|part| part.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .or_else(|| msg.and_then(|m| m.get("text")).and_then(|t| t.as_str()).map(String::from));
        text.unwrap_or_else(|| match task.resolved_state() {
            RemoteTaskState::Completed => String::new(),
            other => format!("task state: {other:?}"),
        })
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Json, Router, extract::State, http::StatusCode, routing::{get, post}};
    use std::sync::{Arc, Mutex};

    /// Scripted JSON-RPC reply: (method, params_json, auth_header) -> result.
    type Scripted = Arc<dyn Fn(&str, &str, Option<&str>) -> serde_json::Value + Send + Sync>;

    fn mock_agent(url: &str) -> ExternalAgent {
        ExternalAgent::new("mock", url, "")
    }

    /// Spawn an axum server impersonating a remote A2A agent. Logs every
    /// request body into the returned `Arc<Mutex<Vec<String>>>` and replies
    /// per the scripted closure.
    async fn spawn_agent(
        scripted: Scripted,
    ) -> (String, Arc<Mutex<Vec<String>>>) {
        let log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let log2 = log.clone();

        async fn card(headers: axum::http::HeaderMap) -> impl axum::response::IntoResponse {
            let host = headers
                .get(axum::http::header::HOST)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("127.0.0.1");
            Json(serde_json::json!({
                "protocolVersion": "0.3.0",
                "name": "remote-mock",
                "description": "mock external agent",
                "url": format!("http://{host}/a2a"),
                "skills": [{"id": "mock.skill", "name": "mock", "description": "d"}],
                "capabilities": {"streaming": false},
            }))
        }

        let router = Router::new()
            .route("/.well-known/agent-card.json", get(card))
            .route(
                "/a2a",
                post(
                    |State(state): State<Arc<Mutex<Vec<String>>>>,
                     headers: axum::http::HeaderMap,
                     body: axum::body::Bytes| async move {
                        let body_str = String::from_utf8_lossy(&body).to_string();
                        state.lock().unwrap().push(body_str.clone());
                        let parsed: serde_json::Value = serde_json::from_str(&body_str).unwrap_or_default();
                        let method = parsed.get("method").and_then(|m| m.as_str()).unwrap_or("").to_string();
                        let params = parsed
                            .get("params")
                            .map(|p| p.to_string())
                            .unwrap_or_default();
                        let auth = headers
                            .get(axum::http::header::AUTHORIZATION)
                            .and_then(|v| v.to_str().ok())
                            .map(String::from);
                        let result = scripted(&method, &params, auth.as_deref());
                        if let Some(e) = result.get("__error") {
                            // JSON-RPC errors ride on a 200 transport status
                            // (spec); only a malformed/absent envelope is a 500.
                            return (
                                StatusCode::OK,
                                Json(e.clone()),
                            );
                        }
                        (StatusCode::OK, Json(serde_json::json!({"jsonrpc": "2.0", "id": 1, "result": result})))
                    },
                ),
            )
            .with_state(log2);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        (format!("http://127.0.0.1:{}", addr.port()), log)
    }

    async fn mock_endpoint(base: &str) -> (A2aClient, ExternalAgent, String) {
        let agent = mock_agent(base);
        let client = A2aClient::new().unwrap();
        let card = client.fetch_card(&agent).await.unwrap();
        let endpoint = A2aClient::resolve_endpoint(&card, &agent.url).unwrap();
        (client, agent, endpoint)
    }

    #[tokio::test]
    async fn test_fetch_card_and_resolve_endpoint() {
        let scripted: Scripted = Arc::new(|_, _, _| serde_json::json!({}));
        let (base, _log) = spawn_agent(scripted).await;
        let (client, agent, endpoint) = mock_endpoint(&base).await;

        let card = client.fetch_card(&agent).await.unwrap();
        assert_eq!(card.name, "remote-mock");
        assert_eq!(card.skills.len(), 1);
        assert_eq!(card.skills[0].id, "mock.skill");
        // Spec: task dispatch goes to the card's own `url` endpoint.
        assert!(endpoint.ends_with("/a2a"));
    }

    #[tokio::test]
    async fn test_message_send_uses_v03_wire_shape() {
        let scripted: Scripted = Arc::new(|method, _params, _auth| {
            assert_eq!(method, "message/send");
            serde_json::json!({
                "id": "task-1",
                "status": {"state": "working", "timestamp": "t"},
            })
        });
        let (base, log) = spawn_agent(scripted).await;
        let (client, agent, endpoint) = mock_endpoint(&base).await;

        let task = client.message_send(&agent, &endpoint, "hello").await.unwrap();
        assert_eq!(task.id, "task-1");
        assert_eq!(task.resolved_state(), RemoteTaskState::Working);

        let bodies = log.lock().unwrap();
        let sent: serde_json::Value = serde_json::from_str(&bodies[0]).unwrap();
        assert_eq!(sent["method"], "message/send");
        assert_eq!(sent["jsonrpc"], "2.0");
        assert_eq!(sent["params"]["message"]["kind"], "message");
        let message_id = sent["params"]["message"]["message_id"].as_str().unwrap();
        assert!(!message_id.is_empty(), "sender-generated message_id required");
        assert_eq!(sent["params"]["message"]["role"], "user");
        assert_eq!(sent["params"]["message"]["parts"][0]["kind"], "text");
        assert_eq!(sent["params"]["message"]["parts"][0]["text"], "hello");
    }

    #[tokio::test]
    async fn test_run_task_polls_until_terminal() {
        let polls = Arc::new(Mutex::new(0usize));
        let polls2 = polls.clone();
        let scripted: Scripted = Arc::new(move |method, _params, _auth| match method {
            "message/send" => serde_json::json!({
                "id": "task-9",
                "status": {"state": "working"},
            }),
            "tasks/get" => {
                let mut n = polls2.lock().unwrap();
                *n += 1;
                if *n >= 3 {
                    serde_json::json!({
                        "id": "task-9",
                        "status": {
                            "state": "completed",
                            "message": {
                                "role": "agent",
                                "parts": [{"kind": "text", "text": "done!"}],
                            },
                        },
                    })
                } else {
                    serde_json::json!({
                        "id": "task-9",
                        "status": {"state": "working"},
                    })
                }
            }
            other => panic!("unexpected method {other}"),
        });
        let (base, _log) = spawn_agent(scripted).await;
        let (client, agent, endpoint) = mock_endpoint(&base).await;

        let task = client
            .run_task(&agent, &endpoint, "do it", Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(task.resolved_state(), RemoteTaskState::Completed);
        assert_eq!(A2aClient::task_text(&task), "done!");
        assert_eq!(*polls.lock().unwrap(), 3);
    }

    #[tokio::test]
    async fn test_run_task_times_out() {
        let scripted: Scripted = Arc::new(|_, _params, _auth| {
            serde_json::json!({
                "id": "task-slow",
                "status": {"state": "working"},
            })
        });
        let (base, _log) = spawn_agent(scripted).await;
        let (client, agent, endpoint) = mock_endpoint(&base).await;

        let err = client
            .run_task(&agent, &endpoint, "wait", Duration::from_millis(350))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("task-slow"), "{err}");
    }

    #[tokio::test]
    async fn test_jsonrpc_error_envelope_surfaces() {
        let scripted: Scripted = Arc::new(|_, _, _| {
            serde_json::json!({
                "__error": {"jsonrpc": "2.0", "id": 1,
                    "error": {"code": -32601, "message": "Method not found"}}
            })
        });
        let (base, _log) = spawn_agent(scripted).await;
        let (client, agent, endpoint) = mock_endpoint(&base).await;

        let err = client
            .message_send(&agent, &endpoint, "hi")
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("-32601"), "{msg}");
        assert!(msg.contains("Method not found"), "{msg}");
    }

    #[tokio::test]
    async fn test_bearer_token_is_sent_when_configured() {
        let scripted: Scripted = Arc::new(|_, _, auth| {
            assert_eq!(auth, Some("Bearer sekret-token"));
            serde_json::json!({
                "id": "task-auth",
                "status": {"state": "completed"},
            })
        });
        let (base, _log) = spawn_agent(scripted).await;
        let mut agent = mock_agent(&base);
        agent.auth_token = "sekret-token".into();
        let client = A2aClient::new().unwrap();
        let card = client.fetch_card(&agent).await.unwrap();
        let endpoint = A2aClient::resolve_endpoint(&card, &agent.url).unwrap();

        let task = client.message_send(&agent, &endpoint, "hi").await.unwrap();
        assert_eq!(task.resolved_state(), RemoteTaskState::Completed);
    }

    #[tokio::test]
    async fn test_legacy_flat_state_shape_is_accepted() {
        let scripted: Scripted = Arc::new(|_, _, _| {
            serde_json::json!({
                "id": "task-old",
                "state": "completed",
                "message": {"role": "agent", "parts": [{"kind": "text", "text": "legacy ok"}]},
            })
        });
        let (base, _log) = spawn_agent(scripted).await;
        let (client, agent, endpoint) = mock_endpoint(&base).await;

        let task = client.message_send(&agent, &endpoint, "hi").await.unwrap();
        assert_eq!(task.resolved_state(), RemoteTaskState::Completed);
        assert_eq!(A2aClient::task_text(&task), "legacy ok");
    }

    #[tokio::test]
    async fn test_card_validation_rejects_bad_cards() {
        let bad = RemoteAgentCard {
            protocol_version: "0.4.0".into(),
            name: "x".into(),
            description: None,
            url: Some("ftp://nope".into()),
            version: None,
            capabilities: None,
            default_input_modes: vec![],
            default_output_modes: vec![],
            security_schemes: None,
            skills: vec![],
        };
        let err = A2aClient::resolve_endpoint(&bad, "http://127.0.0.1").unwrap_err();
        assert!(err.contains("0.4.0"), "{err}");

        let no_name = RemoteAgentCard {
            name: "  ".into(),
            ..bad
        };
        assert!(A2aClient::resolve_endpoint(&no_name, "http://x").unwrap_err().contains("no name"));
    }

    #[test]
    fn test_card_without_url_falls_back_to_base_a2a() {
        // Regression for the interop bug found live against the official
        // a2a-sdk v1.1.2 echo agent 2026-08-13: v1.1.x cards have no `url`
        // field, so the endpoint must resolve to {base}/a2a.
        let card = RemoteAgentCard {
            protocol_version: "0.3.0".into(),
            name: "sdk-echo".into(),
            description: None,
            url: None,
            version: None,
            capabilities: None,
            default_input_modes: vec![],
            default_output_modes: vec![],
            security_schemes: None,
            skills: vec![],
        };
        let endpoint = A2aClient::resolve_endpoint(&card, "http://127.0.0.1:8901").unwrap();
        assert_eq!(endpoint, "http://127.0.0.1:8901/a2a");
    }

    // ── A2aAgentStore (SilvaDB persistence) ──────────────────────────────

    async fn test_store() -> A2aAgentStore {
        let silva = Arc::new(crate::memory::silva::SilvaDB::in_memory().await.unwrap());
        silva.init().await.unwrap();
        A2aAgentStore::new(silva)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_store_upsert_get_remove_roundtrip() {
        let store = test_store().await;
        let agent = ExternalAgent::new("sdk-echo", "http://127.0.0.1:8901", "");

        store.upsert(&agent).await.unwrap();
        let loaded = store.get(&agent.id).await.unwrap().unwrap();
        assert_eq!(loaded.name, "sdk-echo");
        assert_eq!(loaded.url, "http://127.0.0.1:8901");
        assert!(loaded.enabled);

        // Upsert updates in place (same id, new url).
        let mut updated = agent.clone();
        updated.url = "http://127.0.0.1:8902".into();
        updated.auth_token = "sekret".into();
        store.upsert(&updated).await.unwrap();
        let reloaded = store.get(&agent.id).await.unwrap().unwrap();
        assert_eq!(reloaded.url, "http://127.0.0.1:8902");
        assert_eq!(reloaded.auth_token, "sekret");

        assert_eq!(store.load_all().await.unwrap().len(), 1);
        assert!(store.remove(&agent.id).await.unwrap());
        assert!(store.get(&agent.id).await.unwrap().is_none());
        assert_eq!(store.load_all().await.unwrap().len(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_store_persists_enabled_flag() {
        let store = test_store().await;
        let mut agent = ExternalAgent::new("offline-agent", "http://127.0.0.1:9999", "");
        agent.enabled = false;
        store.upsert(&agent).await.unwrap();
        let loaded = store.get(&agent.id).await.unwrap().unwrap();
        assert!(!loaded.enabled, "enabled=false must survive the roundtrip");
    }
}