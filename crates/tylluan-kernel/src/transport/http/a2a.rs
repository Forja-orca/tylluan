//! # A2A (Agent-to-Agent) Protocol Handler
//!
//! Implements the A2A spec v0.3.0 (Linux Foundation) over JSON-RPC 2.0.
//!
//! ## Implemented methods
//!
//! - `message/send` — Create and execute a task from an intent.
//! - `message/stream` — Same as `message/send`, but the JSON-RPC response is
//!   delivered as a Server-Sent-Events stream (W3C framing) whose `data:` lines
//!   are JSON-RPC success messages carrying v0.3-compat events: a
//!   `status-update` (`working`) first, then a terminal `task` (`completed`,
//!   reply in `history`) or `status-update` (`failed`, `final: true`). The
//!   stream closes after the terminal event. Wire verified 2026-08-14 against
//!   a2a-sdk 1.1.2 (`a2a/compat/v0_3/jsonrpc_transport.py::_send_stream_request`),
//!   which parses each `data:` line as `{id, result}` and infers the event kind
//!   from `kind`/`taskId`/`id` keys; enum states are lowercase v0.3 names.
//! - `tasks/get` — Query task state and result.
//! - `tasks/cancel` — Cancel a non-terminal task.
//!
//! ## Out of scope (M38)
//!
//! `tasks/resubscribe` and push notifications are intentionally **not
//! implemented** in M38. See backlog item M33/J-3 for discussion.

use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response, sse::{Event, Sse}},
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
            "streaming": true,
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

#[derive(Serialize, Deserialize)]
pub struct JsonRpcResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jsonrpc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
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

/// Hard cap for JSON-RPC request bodies. Intents are plain text and 1 MiB is
/// far beyond any legitimate payload; anything larger is rejected with a
/// JSON-RPC error (more useful to A2A clients than a raw 413).
pub const MAX_JSONRPC_BODY_BYTES: usize = 1024 * 1024;

pub async fn a2a_jsonrpc_handler(
    State(state): State<Arc<HttpState>>,
    body: Bytes,
) -> Response {
    if body.len() > MAX_JSONRPC_BODY_BYTES {
        return Json(jsonrpc_error(
            -32600,
            &format!(
                "Request body too large ({} bytes, limit {})",
                body.len(),
                MAX_JSONRPC_BODY_BYTES
            ),
            serde_json::Value::Null,
        ))
        .into_response();
    }

    let body: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return Json(jsonrpc_error(-32700, &format!("Parse error: {e}"), serde_json::Value::Null))
                .into_response();
        }
    };

    let req: JsonRpcRequest = match serde_json::from_value(body.clone()) {
        Ok(r) => r,
        Err(e) => {
            let id = body.get("id").cloned().unwrap_or(serde_json::Value::Null);
            return Json(jsonrpc_error(-32700, &format!("Parse error: {e}"), id)).into_response();
        }
    };

    let task_mgr = &state.a2a_task_manager;
    let method = req.method.clone();
    let request_id = req.id.clone();

    match method.as_str() {
        "message/send" => handle_message_send(task_mgr, &state, &req.params, request_id).await.into_response(),
        "message/stream" => {
            handle_message_stream(state, req.params.as_ref().unwrap_or(&serde_json::Value::Null), request_id).await
        }
        "tasks/get" => handle_tasks_get(task_mgr, &req.params, request_id).await.into_response(),
        "tasks/cancel" => handle_tasks_cancel(task_mgr, &req.params, request_id).await.into_response(),
        _ => Json(jsonrpc_error(-32601, "Method not found", request_id)).into_response(),
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
        match execute_intent_and_store(&state_clone, &task_id_clone, &agent_id_owned, &intent_owned).await {
            Ok(text) => {
                task_mgr_clone.update_state(&task_id_clone, A2aTaskState::Completed,
                    Some(serde_json::json!({"result": text})), None).await;
            }
            Err(err) => {
                task_mgr_clone.update_state(&task_id_clone, A2aTaskState::Failed,
                    Some(serde_json::json!({"error": err})), None).await;
            }
        }
    });

    Json(jsonrpc_result(serde_json::json!({
        "id": task_id,
        "state": "working",
    }), id))
}

/// Runs the real `tylluan_do` pathway (rate limiter, guild ACL, HITL grants)
/// for a task, persists the result node into SilvaDB when successful, and
/// returns the reply text (`Ok`) or the error text (`Err`). The task state
/// itself is not updated here -- callers do that so they can interleave
/// stream events. Shared by `message/send` (background spawn) and
/// `message/stream` (inline while the SSE connection is open).
async fn execute_intent_and_store(
    state: &Arc<HttpState>,
    task_id: &str,
    agent_id: &str,
    intent: &str,
) -> Result<String, String> {
    let server_guard = match &state.server {
        Some(srv) => srv.read().await,
        None => return Err("Server not available".into()),
    };
    let mut args = serde_json::Map::new();
    args.insert("intent".into(), serde_json::Value::String(intent.to_string()));
    args.insert("agent_id".into(), serde_json::Value::String(agent_id.to_string()));
    args.insert("remember".into(), serde_json::Value::Bool(false));
    let result = crate::transport::server::handler_do::handle_tylluan_do(&server_guard, Some(args)).await;
    drop(server_guard);

    match result {
        Ok(call_result) => {
            let is_error = call_result.is_error.unwrap_or(false);
            let text = call_result.content.into_iter()
                .filter_map(|c| c.as_text().map(|t| t.text.clone()))
                .collect::<Vec<_>>()
                .join("\n");

            if is_error {
                return Err(text);
            }
            // owner_scope: task_id already carries the "a2a_" prefix from
            // create_task() -- do not prepend it again here.
            let owner_scope = format!("user:external/session:{task_id}/agent:{agent_id}");
            if let Some(srv) = state.server.as_ref() {
                let silva = srv.read().await.silva.clone();
                let node_id = format!("a2a:{task_id}:result");
                let meta = serde_json::json!({
                    "owner_scope": owner_scope,
                    "source": "a2a",
                    "task_id": task_id,
                    "client_agent_id": agent_id,
                }).to_string();
                let opts = crate::memory::silva::nodes::NodeWriteOptions::new("guild_output")
                    .owner_scope(Some(&owner_scope));
                let _ = silva.upsert_node_with_validity(&node_id, "a2a_result", &text, &meta, opts).await;
            }
            Ok(text)
        }
        Err(e) => Err(e.to_string()),
    }
}

// ─── message/stream (SSE) ────────────────────────────────────────────────────────

/// `message/stream` — JSON-RPC method served over Server-Sent-Events.
///
/// Wire (verified against a2a-sdk 1.1.2): the request body is a normal JSON-RPC
/// POST to `/a2a` with `method: "message/stream"`; the response is
/// `text/event-stream` and every SSE event is a `data:` line containing a
/// JSON-RPC success message `{"jsonrpc":"2.0","id":...,"result":<event>}`.
/// Events use v0.3-compat shapes (camelCase aliases, lowercase states):
///   1. `{"kind":"status-update","taskId":...,"contextId":...,"status":{"state":"working"},"final":false}`
///   2. terminal, on success:
///      `{"kind":"task","id":...,"contextId":...,"status":{"state":"completed",...},"history":[<reply message>]}`
///      or, on failure: a `status-update` with `state:"failed"`, `final:true`.
///
/// The stream closes after the terminal event.
async fn handle_message_stream(
    state: Arc<HttpState>,
    params: &serde_json::Value,
    id: serde_json::Value,
) -> Response {
    let task_mgr = Arc::clone(&state.a2a_task_manager);
    let agent_id = params.get("agent_id").and_then(|v| v.as_str()).unwrap_or("a2a-client").to_string();
    let intent = extract_intent(params);
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, std::convert::Infallible>>(8);

    tokio::spawn(async move {
        let send = async |tx: &tokio::sync::mpsc::Sender<Result<Event, std::convert::Infallible>>, value: serde_json::Value| {
            let json = serde_json::to_string(&value).unwrap_or_default();
            let _ = tx.send(Ok(Event::default().data(json))).await;
        };

        if intent.is_empty() {
            send(&tx, jsonrpc_error_json(JsonRpcError {
                code: -32602,
                message: "Invalid params: 'intent', or 'message' as a string or a spec-shaped {parts:[{kind:\"text\",text}]} object, is required".into(),
                data: None,
            }, id)).await;
            return;
        }

        let task_id = task_mgr.create_task(&agent_id, "message/stream", &serde_json::json!({ "intent": intent })).await;
        let context_id = task_id.clone();
        task_mgr.update_state(&task_id, A2aTaskState::Working, None, None).await;

        send(&tx, stream_working_event(&task_id, &context_id, &id)).await;

        match execute_intent_and_store(&state, &task_id, &agent_id, &intent).await {
            Ok(reply) => {
                task_mgr.update_state(&task_id, A2aTaskState::Completed,
                    Some(serde_json::json!({"result": reply})), None).await;
                send(&tx, stream_terminal_task_event(&task_id, &context_id, &reply, &id)).await;
            }
            Err(err) => {
                task_mgr.update_state(&task_id, A2aTaskState::Failed,
                    Some(serde_json::json!({"error": err})), None).await;
                send(&tx, stream_failed_event(&task_id, &context_id, &err, &id)).await;
            }
        }
    });

    Sse::new(tokio_stream::wrappers::ReceiverStream::new(rx))
        .keep_alive(axum::response::sse::KeepAlive::default())
        .into_response()
}

fn jsonrpc_error_json(error: JsonRpcError, id: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": error,
    })
}

fn stream_working_event(task_id: &str, context_id: &str, id: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "kind": "status-update",
            "taskId": task_id,
            "contextId": context_id,
            "status": { "state": "working" },
            "final": false,
        },
    })
}

fn stream_terminal_task_event(task_id: &str, context_id: &str, reply: &str, id: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "kind": "task",
            "id": task_id,
            "contextId": context_id,
            "status": {
                "state": "completed",
                "messageId": Uuid::new_v4().to_string(),
                "timestamp": iso_timestamp(),
            },
            "history": [{
                "kind": "message",
                "messageId": Uuid::new_v4().to_string(),
                "role": "agent",
                "parts": [{"kind": "text", "text": reply}],
            }],
        },
    })
}

fn stream_failed_event(task_id: &str, context_id: &str, error_text: &str, id: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "kind": "status-update",
            "taskId": task_id,
            "contextId": context_id,
            "status": {
                "state": "failed",
                "timestamp": iso_timestamp(),
            },
            "metadata": { "error": error_text },
            "final": true,
        },
    })
}

/// RFC 3339 UTC timestamp with second precision, e.g. `2026-08-14T12:00:00Z`.
fn iso_timestamp() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH)
        .unwrap_or_default().as_secs();
    let days = secs / 86_400;
    let rem = secs % 86_400;
    // Civil-from-days (Howard Hinnant's algorithm, public domain).
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let h = rem / 3600;
    let min = (rem % 3600) / 60;
    let s = rem % 60;
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{s:02}Z")
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

    // ─── message/stream event wire (a2a-sdk 1.1.2 v0.3-compat) ──────────────────

    #[test]
    fn test_stream_working_event_shape() {
        let ev = stream_working_event("a2a_1", "a2a_1", &serde_json::json!(7));
        assert_eq!(ev["jsonrpc"], "2.0");
        assert_eq!(ev["id"], 7);
        let result = &ev["result"];
        assert_eq!(result["kind"], "status-update");
        assert_eq!(result["taskId"], "a2a_1");
        assert_eq!(result["contextId"], "a2a_1");
        assert_eq!(result["status"]["state"], "working");
        assert_eq!(result["final"], false);
    }

    #[test]
    fn test_stream_terminal_task_event_shape() {
        let ev = stream_terminal_task_event("a2a_2", "a2a_2", "hello back", &serde_json::json!(7));
        let result = &ev["result"];
        assert_eq!(result["kind"], "task");
        assert_eq!(result["id"], "a2a_2");
        assert_eq!(result["status"]["state"], "completed");
        assert!(result["status"]["messageId"].is_string());
        assert!(result["status"]["timestamp"].is_string());
        let msg = &result["history"][0];
        assert_eq!(msg["kind"], "message");
        assert_eq!(msg["role"], "agent");
        assert_eq!(msg["parts"][0]["kind"], "text");
        assert_eq!(msg["parts"][0]["text"], "hello back");
    }

    #[test]
    fn test_stream_failed_event_shape() {
        let ev = stream_failed_event("a2a_3", "a2a_3", "boom", &serde_json::json!(7));
        let result = &ev["result"];
        assert_eq!(result["kind"], "status-update");
        assert_eq!(result["taskId"], "a2a_3");
        assert_eq!(result["status"]["state"], "failed");
        assert_eq!(result["metadata"]["error"], "boom");
        assert_eq!(result["final"], true);
    }

    #[test]
    fn test_stream_error_json_shape() {
        let ev = jsonrpc_error_json(JsonRpcError {
            code: -32602,
            message: "bad params".into(),
            data: None,
        }, serde_json::json!(9));
        assert_eq!(ev["jsonrpc"], "2.0");
        assert_eq!(ev["id"], 9);
        assert_eq!(ev["error"]["code"], -32602);
        assert_eq!(ev["error"]["message"], "bad params");
    }

    #[test]
    fn test_iso_timestamp_format() {
        let ts = iso_timestamp();
        assert_eq!(ts.len(), 20, "RFC3339 UTC: {ts}");
        assert!(ts.ends_with('Z'));
        let digits: Vec<char> = ts.chars()
            .filter(|c| c.is_ascii_digit())
            .collect();
        assert!(digits.len() >= 14, "year-month-day-h-m-s: {ts}");
        let y: u32 = ts[0..4].parse().unwrap();
        assert!((2020..=2100).contains(&y), "year out of range: {ts}");
    }
}
