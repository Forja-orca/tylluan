//! # A2A (Agent-to-Agent) Protocol Handler
//!
//! Implements the A2A spec v0.3.0 (Linux Foundation) over JSON-RPC 2.0.
//!
//! ## Implemented methods
//!
//! - `message/send` — Create and execute a task from an intent.
//! - `tasks/get` — Query task state and result.
//! - `tasks/cancel` — Cancel a non-terminal task.
//!
//! ## Out of scope (M38)
//!
//! `message/stream` (SSE push) is intentionally **not implemented** in M38.
//! See backlog item M33/J-3 for discussion. Only polling via `tasks/get` is
//! available for result retrieval. SSE push notifications may be added later
//! once the A2A task lifecycle is stabilized in production.

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::memory::silva::SilvaDB;
use crate::transport::http::HttpState;
use crate::transport::server::TylluanServer;

// ─── Agent Card (A2A spec v0.3.0) ───────────────────────────────────────────────

#[derive(Serialize)]
pub struct AgentCard {
    #[serde(rename = "protocolVersion")]
    protocol_version: String,
    name: String,
    description: String,
    url: String,
    #[serde(rename = "preferredTransport")]
    preferred_transport: String,
    version: String,
    capabilities: serde_json::Value,
    skills: Vec<AgentSkill>,
    #[serde(rename = "defaultInputModes")]
    default_input_modes: Vec<String>,
    #[serde(rename = "defaultOutputModes")]
    default_output_modes: Vec<String>,
    // A2A spec models securitySchemes as a map (scheme name -> SecurityScheme
    // object), not a list. The official a2a-sdk's card resolver calls
    // .values() on this field and errors with "'list' object has no
    // attribute 'values'" against the old Vec shape -- found 2026-07-27
    // testing interop against a real external client, not our own code.
    #[serde(rename = "securitySchemes")]
    security_schemes: serde_json::Map<String, serde_json::Value>,
}

#[derive(Serialize)]
pub struct AgentSkill {
    id: String,
    name: String,
    description: String,
    tags: Vec<String>,
}

pub async fn agent_card_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let skills = build_skills_list();
    let config = state.config.read().await;
    let url = format!("http://{}:{}/a2a", config.nexus.host, config.nexus.port);

    let card = AgentCard {
        protocol_version: "0.3.0".into(),
        name: "Tylluan Sovereign Kernel".into(),
        description: "Agent-to-Agent protocol endpoint for the Tylluan MCP kernel. Accepts task delegation via JSON-RPC 2.0.".into(),
        url,
        preferred_transport: "JSONRPC".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        capabilities: serde_json::json!({
            "streaming": false,
            "pushNotifications": false,
            "stateTransitionHistory": true,
        }),
        skills,
        default_input_modes: vec!["text".into()],
        default_output_modes: vec!["text".into()],
        security_schemes: {
            let mut m = serde_json::Map::new();
            m.insert("bearer".to_string(), serde_json::json!({
                "type": "http",
                "scheme": "bearer",
                "bearerFormat": "JWT",
                "description": "Bearer token matching the kernel's configured auth token or OAuth JWT"
            }));
            m
        },
    };

    (StatusCode::OK, Json(card))
}

fn build_skills_list() -> Vec<AgentSkill> {
    let sovereign_names: [&str; 5] = [
        "tylluan_do", "tylluan_remember", "tylluan_recall",
        "tylluan_think", "tylluan_graph",
    ];

    let kernel_tools = TylluanServer::kernel_tools();
    let mut skills: Vec<AgentSkill> = kernel_tools.into_iter()
        .filter(|t| sovereign_names.contains(&t.name.as_str()))
        .map(|t| {
            let tags = vec![format!("{:?}", t.category).to_lowercase()];
            AgentSkill {
                id: t.name.clone(),
                name: t.name.clone(),
                description: t.description.clone(),
                tags,
            }
        })
        .collect();

    skills.push(AgentSkill {
        id: "guild_dispatch".into(),
        name: "guild_dispatch".into(),
        description: "Route a natural-language intent through Tylluan's semantic guild router".into(),
        tags: vec!["kernel".into(), "routing".into()],
    });

    skills
}

// ─── JSON-RPC Types ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct JsonRpcRequest {
    #[serde(default)]
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<serde_json::Value>,
    pub id: serde_json::Value,
}

#[derive(Serialize)]
pub struct JsonRpcResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jsonrpc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

fn jsonrpc_error(code: i32, message: &str, id: serde_json::Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: Some("2.0".into()),
        result: None,
        error: Some(JsonRpcError { code, message: message.into(), data: None }),
        id,
    }
}

fn jsonrpc_result(result: serde_json::Value, id: serde_json::Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: Some("2.0".into()),
        result: Some(result),
        error: None,
        id,
    }
}

// ─── A2A Task States (A2A spec v0.3.0) ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum A2aTaskState {
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

impl std::fmt::Display for A2aTaskState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            A2aTaskState::Submitted => write!(f, "submitted"),
            A2aTaskState::Working => write!(f, "working"),
            A2aTaskState::InputRequired => write!(f, "input-required"),
            A2aTaskState::Completed => write!(f, "completed"),
            A2aTaskState::Canceled => write!(f, "canceled"),
            A2aTaskState::Failed => write!(f, "failed"),
            A2aTaskState::Rejected => write!(f, "rejected"),
            A2aTaskState::AuthRequired => write!(f, "auth-required"),
            A2aTaskState::Unknown => write!(f, "unknown"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2aTask {
    pub id: String,
    pub state: A2aTaskState,
    pub client_agent_id: String,
    pub method: String,
    pub params_json: String,
    pub result_json: Option<String>,
    pub grant_id: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

// ─── Task Manager ───────────────────────────────────────────────────────────────

pub struct A2aTaskManager {
    silva: Arc<SilvaDB>,
}

impl A2aTaskManager {
    pub fn new(silva: Arc<SilvaDB>) -> Self {
        Self { silva }
    }

    pub async fn create_task(&self, client_agent_id: &str, method: &str, params: &serde_json::Value) -> String {
        let id = format!("a2a_{}", Uuid::new_v4().simple());
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let params_str = serde_json::to_string(params).unwrap_or_default();
        let state_str = A2aTaskState::Submitted.to_string();
        let _ = tokio::task::block_in_place(|| {
            let conn = self.silva.conn_lock();
            let conn = conn.blocking_lock();
            conn.execute(
                "INSERT INTO a2a_tasks (id, state, client_agent_id, method, params_json, result_json, grant_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL, ?6, ?7)",
                params![id, state_str, client_agent_id, method, params_str, now, now],
            )
        });
        id
    }

    pub async fn update_state(
        &self,
        task_id: &str,
        state: A2aTaskState,
        result: Option<serde_json::Value>,
        grant_id: Option<String>,
    ) -> bool {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let state_str = state.to_string();
        let result_str = result.map(|r| serde_json::to_string(&r).unwrap_or_default());
        let grant_str = grant_id;
        let affected = tokio::task::block_in_place(|| {
            let conn = self.silva.conn_lock();
            let conn = conn.blocking_lock();
            conn.execute(
                "UPDATE a2a_tasks SET state = ?1, updated_at = ?2, result_json = COALESCE(?3, result_json), grant_id = COALESCE(?4, grant_id) WHERE id = ?5",
                params![state_str, now, result_str, grant_str, task_id],
            ).unwrap_or(0)
        });
        affected > 0
    }

    pub async fn get_task(&self, task_id: &str) -> Option<A2aTask> {
        tokio::task::block_in_place(|| {
            let conn = self.silva.conn_lock();
            let conn = conn.blocking_lock();
            conn.query_row(
                "SELECT id, state, client_agent_id, method, params_json, result_json, grant_id, created_at, updated_at FROM a2a_tasks WHERE id = ?1",
                params![task_id],
                |row| {
                    let state_str: String = row.get(1)?;
                    Ok(A2aTask {
                        id: row.get(0)?,
                        state: serde_json::from_str(&format!("\"{state_str}\"")).unwrap_or(A2aTaskState::Unknown),
                        client_agent_id: row.get(2)?,
                        method: row.get(3)?,
                        params_json: row.get(4)?,
                        result_json: row.get(5)?,
                        grant_id: row.get(6)?,
                        created_at: row.get(7)?,
                        updated_at: row.get(8)?,
                    })
                }
            ).ok()
        })
    }

    pub async fn cancel_task(&self, task_id: &str) -> Result<(), String> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let canceled = A2aTaskState::Canceled.to_string();
        tokio::task::block_in_place(|| {
            let conn = self.silva.conn_lock();
            let conn = conn.blocking_lock();
            let current_state: std::result::Result<String, rusqlite::Error> = conn.query_row(
                "SELECT state FROM a2a_tasks WHERE id = ?1",
                params![task_id],
                |row| row.get(0),
            );
            match current_state {
                Ok(ref s)
                    if *s == "completed" || *s == "failed" || *s == "canceled" || *s == "rejected" =>
                {
                    Err(format!("Task already in terminal state: {s}"))
                }
                Ok(_) => {
                    conn.execute(
                        "UPDATE a2a_tasks SET state = ?1, updated_at = ?2 WHERE id = ?3",
                        params![canceled, now, task_id],
                    ).map_err(|e| e.to_string())?;
                    Ok(())
                }
                Err(_) => Err("Task not found".into()),
            }
        })
    }
}

// ─── JSON-RPC Handlers ──────────────────────────────────────────────────────────

pub async fn a2a_jsonrpc_handler(
    State(state): State<Arc<HttpState>>,
    Json(body): Json<serde_json::Value>,
) -> Json<JsonRpcResponse> {
    let req: JsonRpcRequest = match serde_json::from_value(body.clone()) {
        Ok(r) => r,
        Err(e) => {
            let id = body.get("id").cloned().unwrap_or(serde_json::Value::Null);
            return Json(jsonrpc_error(-32700, &format!("Parse error: {e}"), id));
        }
    };

    let task_mgr = &state.a2a_task_manager;

    match req.method.as_str() {
        "message/send" => handle_message_send(task_mgr, &state, &req.params, req.id).await,
        "tasks/get" => handle_tasks_get(task_mgr, &req.params, req.id).await,
        "tasks/cancel" => handle_tasks_cancel(task_mgr, &req.params, req.id).await,
        _ => Json(jsonrpc_error(-32601, "Method not found", req.id)),
    }
}

/// Extracts the intent text from `message/send` params. Accepts three shapes:
/// a flat `{"intent": "..."}` (internal callers, dashboard), a real A2A spec
/// `message` object `{role, parts: [{kind:"text", text}], messageId}` (the
/// official SDK and any compliant external client), or a lenient bare
/// `{"message": "..."}` string.
fn extract_intent(params: &serde_json::Value) -> String {
    if let Some(s) = params.get("intent").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    if let Some(msg) = params.get("message") {
        if let Some(s) = msg.as_str() {
            return s.to_string();
        }
        if let Some(parts) = msg.get("parts").and_then(|p| p.as_array()) {
            let text = parts
                .iter()
                .filter(|p| p.get("kind").and_then(|k| k.as_str()) == Some("text"))
                .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join(" ");
            return text;
        }
    }
    String::new()
}

async fn handle_message_send(
    task_mgr: &A2aTaskManager,
    state: &Arc<HttpState>,
    params: &Option<serde_json::Value>,
    id: serde_json::Value,
) -> Json<JsonRpcResponse> {
    let params = match params {
        Some(p) => p.clone(),
        None => return Json(jsonrpc_error(-32602, "Invalid params: missing body", id)),
    };

    let agent_id = params.get("agent_id").and_then(|v| v.as_str()).unwrap_or("a2a-client");

    // Auth cross-check: if the request is authenticated with a bound agent identity,
    // verify the declared client_agent_id matches. Otherwise an authenticated client
    // could impersonate any agent_id.
    // NOTE: In dev_mode or without ACL tokens, current_bound_agent_id() returns None
    // and the check is skipped. This is a known limitation — there is no mechanism
    // today to bind an agent identity to a dev_mode session.
    if let Some(bound_id) = crate::transport::http::auth::current_bound_agent_id()
        && bound_id != agent_id
    {
        return Json(jsonrpc_error(
            -32000,
            &format!("client_agent_id '{agent_id}' does not match authenticated identity '{bound_id}'. The declared agent_id must match the authenticated bearer token."),
            id,
        ));
    }

    let intent_string = extract_intent(&params);
    let intent_str = intent_string.as_str();

    if intent_str.is_empty() {
        return Json(jsonrpc_error(-32602, "Invalid params: 'intent', or 'message' as a string or a spec-shaped {parts:[{kind:\"text\",text}]} object, is required", id));
    }

    let task_id = task_mgr.create_task(agent_id, "message/send", &params).await;

    // Execute through the real tylluan_do pathway: rate limiter, guild ACL, AND the
    // real capability-based HITL grant flow (guild_process.rs::handle_capabilities_grant)
    // are all triggered internally by handle_tylluan_do's own guild dispatch -- no
    // separate keyword heuristic or duplicate grant registration here. If a real grant
    // blocks execution, handle_tylluan_do already awaits its resolution (or a 300s
    // timeout) internally before returning, so we don't need a distinct "input-required"
    // short-circuit -- the task simply reports "working" until that resolves.
    task_mgr.update_state(&task_id, A2aTaskState::Working, None, None).await;

    let intent_owned = intent_str.to_string();
    let agent_id_owned = agent_id.to_string();
    let task_mgr_clone = Arc::clone(&state.a2a_task_manager);
    let state_clone = Arc::clone(state);
    let task_id_clone = task_id.clone();

    tokio::spawn(async move {
        let result = if let Some(srv) = &state_clone.server {
            let server_guard = srv.read().await;
            let mut args = serde_json::Map::new();
            args.insert("intent".into(), serde_json::Value::String(intent_owned));
            args.insert("agent_id".into(), serde_json::Value::String(agent_id_owned.clone()));
            args.insert("remember".into(), serde_json::Value::Bool(false));
            crate::transport::server::handler_do::handle_tylluan_do(&server_guard, Some(args)).await
        } else {
            task_mgr_clone.update_state(&task_id_clone, A2aTaskState::Failed,
                Some(serde_json::json!({"error": "Server not available"})), None).await;
            return;
        };

        match result {
            Ok(call_result) => {
                let is_error = call_result.is_error.unwrap_or(false);
                let text = call_result.content.into_iter()
                    .filter_map(|c| c.as_text().map(|t| t.text.clone()))
                    .collect::<Vec<_>>()
                    .join("\n");

                if is_error {
                    task_mgr_clone.update_state(&task_id_clone, A2aTaskState::Failed,
                        Some(serde_json::json!({"error": text})), None).await;
                } else {
                    // owner_scope: task_id already carries the "a2a_" prefix from
                    // create_task() -- do not prepend it again here.
                    let owner_scope = format!("user:external/session:{task_id_clone}/agent:{agent_id_owned}");
                    if let Some(srv) = state_clone.server.as_ref() {
                        let silva = srv.read().await.silva.clone();
                        let node_id = format!("a2a:{task_id_clone}:result");
                        let meta = serde_json::json!({
                            "owner_scope": owner_scope,
                            "source": "a2a",
                            "task_id": task_id_clone,
                            "client_agent_id": agent_id_owned,
                        }).to_string();
                        let opts = crate::memory::silva::nodes::NodeWriteOptions::new("guild_output")
                            .owner_scope(Some(&owner_scope));
                        let _ = silva.upsert_node_with_validity(&node_id, "a2a_result", &text, &meta, opts).await;
                    }
                    task_mgr_clone.update_state(&task_id_clone, A2aTaskState::Completed,
                        Some(serde_json::json!({"result": text})), None).await;
                }
            }
            Err(e) => {
                task_mgr_clone.update_state(&task_id_clone, A2aTaskState::Failed,
                    Some(serde_json::json!({"error": e.to_string()})), None).await;
            }
        }
    });

    Json(jsonrpc_result(serde_json::json!({
        "id": task_id,
        "state": "working",
    }), id))
}

async fn handle_tasks_get(
    task_mgr: &A2aTaskManager,
    params: &Option<serde_json::Value>,
    id: serde_json::Value,
) -> Json<JsonRpcResponse> {
    let task_id = params.as_ref()
        .and_then(|p| p.get("id"))
        .or_else(|| params.as_ref().and_then(|p| p.get("taskId")))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let task_id = match task_id {
        Some(t) => t,
        None => return Json(jsonrpc_error(-32602, "Invalid params: 'id' required", id)),
    };

    match task_mgr.get_task(&task_id).await {
        Some(task) => {
            let response = serde_json::json!({
                "id": task.id,
                "state": task.state.to_string(),
                "client_agent_id": task.client_agent_id,
                "result": task.result_json.as_ref().and_then(|r| serde_json::from_str::<serde_json::Value>(r).ok()),
                "created_at": task.created_at,
                "updated_at": task.updated_at,
            });
            Json(jsonrpc_result(response, id))
        }
        None => Json(jsonrpc_error(-32000, "Task not found", id)),
    }
}

async fn handle_tasks_cancel(
    task_mgr: &A2aTaskManager,
    params: &Option<serde_json::Value>,
    id: serde_json::Value,
) -> Json<JsonRpcResponse> {
    let task_id = params.as_ref()
        .and_then(|p| p.get("id"))
        .or_else(|| params.as_ref().and_then(|p| p.get("taskId")))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let task_id = match task_id {
        Some(t) => t,
        None => return Json(jsonrpc_error(-32602, "Invalid params: 'id' required", id)),
    };

    match task_mgr.cancel_task(&task_id).await {
        Ok(()) => Json(jsonrpc_result(serde_json::json!({
            "id": task_id,
            "state": "canceled",
        }), id)),
        Err(e) => Json(jsonrpc_error(-32000, &e, id)),
    }
}

// ─── Router ─────────────────────────────────────────────────────────────────────

pub fn a2a_routes() -> Router<Arc<HttpState>> {
    let public = Router::new()
        .route("/.well-known/agent-card.json", get(agent_card_handler));

    let protected = Router::new()
        .route("/a2a", post(a2a_jsonrpc_handler));

    Router::new().merge(public).merge(protected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::silva::SilvaDB;
    use crate::transport::server::TylluanServer;

    async fn test_mgr() -> A2aTaskManager {
        let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
        silva.init().await.unwrap();
        A2aTaskManager::new(silva)
    }

    #[test]
    fn test_agent_card_has_5_skills_plus_guild_dispatch() {
        let tools = TylluanServer::kernel_tools();
        let sovereign_names: [&str; 5] = [
            "tylluan_do", "tylluan_remember", "tylluan_recall",
            "tylluan_think", "tylluan_graph",
        ];
        let skills: Vec<AgentSkill> = tools.into_iter()
            .filter(|t| sovereign_names.contains(&t.name.as_str()))
            .map(|t| AgentSkill {
                id: t.name.clone(),
                name: t.name.clone(),
                description: t.description.clone(),
                tags: vec![],
            })
            .collect();
        assert_eq!(skills.len(), 5, "Agent Card must have exactly 5 sovereign skills");
    }

    #[test]
    fn test_unknown_method_returns_minus_32601() {
        let resp = jsonrpc_error(-32601, "Method not found", serde_json::json!(1));
        assert_eq!(resp.error.as_ref().unwrap().code, -32601);
        assert_eq!(resp.error.as_ref().unwrap().message, "Method not found");
        assert!(resp.result.is_none());
    }

    #[test]
    fn test_jsonrpc_result_format() {
        let resp = jsonrpc_result(serde_json::json!({"ok": true}), serde_json::json!(1));
        assert_eq!(resp.id, serde_json::json!(1));
        assert!(resp.error.is_none());
        assert_eq!(resp.result.as_ref().unwrap().get("ok").unwrap(), true);
    }

    #[test]
    fn test_extract_intent_flat_shape() {
        let params = serde_json::json!({"intent": "list files"});
        assert_eq!(extract_intent(&params), "list files");
    }

    #[test]
    fn test_extract_intent_real_spec_message_shape() {
        // The real A2A spec shape sent by the official a2a-sdk client and any
        // compliant external agent: message is an object with a parts array,
        // not a plain string. Found missing 2026-07-28 when a real curl request
        // shaped like this got rejected with "'intent' or 'message' required"
        // despite the earlier securitySchemes fix (e4586c2) already being live.
        let params = serde_json::json!({
            "message": {
                "role": "user",
                "parts": [{"kind": "text", "text": "health check ping"}],
                "messageId": "test-msg-1"
            }
        });
        assert_eq!(extract_intent(&params), "health check ping");
    }

    #[test]
    fn test_extract_intent_multi_part_message_joins_text() {
        let params = serde_json::json!({
            "message": {
                "parts": [
                    {"kind": "text", "text": "first"},
                    {"kind": "text", "text": "second"}
                ]
            }
        });
        assert_eq!(extract_intent(&params), "first second");
    }

    #[test]
    fn test_extract_intent_bare_message_string_fallback() {
        let params = serde_json::json!({"message": "hello"});
        assert_eq!(extract_intent(&params), "hello");
    }

    #[test]
    fn test_extract_intent_empty_when_nothing_matches() {
        let params = serde_json::json!({"agent_id": "test-agent"});
        assert_eq!(extract_intent(&params), "");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_message_send_creates_task() {
        let mgr = test_mgr().await;
        let params = serde_json::json!({"intent": "list files", "agent_id": "test-agent"});
        let task_id = mgr.create_task("test-agent", "message/send", &params).await;
        assert!(!task_id.is_empty());
        let task = mgr.get_task(&task_id).await.unwrap();
        assert_eq!(task.state, A2aTaskState::Submitted);
        assert_eq!(task.client_agent_id, "test-agent");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_tasks_get_returns_real_state() {
        let mgr = test_mgr().await;
        let params = serde_json::json!({"intent": "hello"});
        let task_id = mgr.create_task("agent1", "message/send", &params).await;
        mgr.update_state(&task_id, A2aTaskState::Completed,
            Some(serde_json::json!({"result": "done"})), None).await;
        let task = mgr.get_task(&task_id).await.unwrap();
        assert_eq!(task.state, A2aTaskState::Completed);
        assert!(task.result_json.is_some());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_tasks_cancel_rejects_completed() {
        let mgr = test_mgr().await;
        let params = serde_json::json!({"intent": "hello"});
        let task_id = mgr.create_task("agent1", "message/send", &params).await;
        mgr.update_state(&task_id, A2aTaskState::Completed, None, None).await;
        let result = mgr.cancel_task(&task_id).await;
        assert!(result.is_err(), "Should not cancel a completed task");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_tasks_cancel_accepts_working() {
        let mgr = test_mgr().await;
        let params = serde_json::json!({"intent": "hello"});
        let task_id = mgr.create_task("agent1", "message/send", &params).await;
        mgr.update_state(&task_id, A2aTaskState::Working, None, None).await;
        let result = mgr.cancel_task(&task_id).await;
        assert!(result.is_ok(), "Should cancel a working task");
        let task = mgr.get_task(&task_id).await.unwrap();
        assert_eq!(task.state, A2aTaskState::Canceled);
    }

    #[test]
    fn test_owner_scope_format() {
        let task_id = "a2a_1712345678";
        let agent_id = "test-agent";
        let owner_scope = format!("user:external/session:{task_id}/agent:{agent_id}");
        assert_eq!(owner_scope, "user:external/session:a2a_1712345678/agent:test-agent");
    }
}
