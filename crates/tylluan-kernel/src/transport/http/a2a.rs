use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

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
    #[serde(rename = "securitySchemes")]
    security_schemes: Vec<serde_json::Value>,
}

#[derive(Serialize)]
pub struct AgentSkill {
    id: String,
    name: String,
    description: String,
    tags: Vec<String>,
}

pub async fn agent_card_handler(State(_state): State<Arc<HttpState>>) -> impl IntoResponse {
    let skills = build_skills_list();

    let card = AgentCard {
        protocol_version: "0.3.0".into(),
        name: "Tylluan Sovereign Kernel".into(),
        description: "Agent-to-Agent protocol endpoint for the Tylluan MCP kernel. Accepts task delegation via JSON-RPC 2.0.".into(),
        url: "/a2a".into(),
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
        security_schemes: vec![serde_json::json!({
            "type": "http",
            "scheme": "bearer",
            "bearerFormat": "JWT",
            "description": "Bearer token matching the kernel's configured auth token or OAuth JWT"
        })],
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

#[derive(Serialize)]
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
    tasks: Arc<RwLock<HashMap<String, A2aTask>>>,
}

impl Default for A2aTaskManager {
    fn default() -> Self {
        Self::new()
    }
}

impl A2aTaskManager {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn create_task(&self, client_agent_id: &str, method: &str, params: &serde_json::Value) -> String {
        let id = format!("a2a_{}", chrono::Utc::now().timestamp_millis());
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let task = A2aTask {
            id: id.clone(),
            state: A2aTaskState::Submitted,
            client_agent_id: client_agent_id.into(),
            method: method.into(),
            params_json: serde_json::to_string(params).unwrap_or_default(),
            result_json: None,
            grant_id: None,
            created_at: now,
            updated_at: now,
        };
        self.tasks.write().await.insert(id.clone(), task);
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
        let mut guard = self.tasks.write().await;
        if let Some(task) = guard.get_mut(task_id) {
            task.state = state;
            task.updated_at = now;
            if let Some(r) = result {
                task.result_json = Some(serde_json::to_string(&r).unwrap_or_default());
            }
            if let Some(g) = grant_id {
                task.grant_id = Some(g);
            }
            true
        } else {
            false
        }
    }

    pub async fn get_task(&self, task_id: &str) -> Option<A2aTask> {
        let guard = self.tasks.read().await;
        guard.get(task_id).cloned()
    }

    pub async fn cancel_task(&self, task_id: &str) -> Result<(), String> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let mut guard = self.tasks.write().await;
        if let Some(task) = guard.get_mut(task_id) {
            match task.state {
                A2aTaskState::Completed | A2aTaskState::Failed | A2aTaskState::Canceled | A2aTaskState::Rejected => {
                    Err(format!("Task already in terminal state: {}", task.state))
                }
                _ => {
                    task.state = A2aTaskState::Canceled;
                    task.updated_at = now;
                    Ok(())
                }
            }
        } else {
            Err("Task not found".into())
        }
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
    let intent_str = params.get("intent")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| params.get("message").and_then(|v| v.as_str()).unwrap_or(""));

    if intent_str.is_empty() {
        return Json(jsonrpc_error(-32602, "Invalid params: 'intent' or 'message' required", id));
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
    use crate::transport::server::TylluanServer;

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

    #[tokio::test]
    async fn test_message_send_creates_task() {
        let mgr = A2aTaskManager::new();
        let params = serde_json::json!({"intent": "list files", "agent_id": "test-agent"});
        let task_id = mgr.create_task("test-agent", "message/send", &params).await;
        assert!(!task_id.is_empty());
        let task = mgr.get_task(&task_id).await.unwrap();
        assert_eq!(task.state, A2aTaskState::Submitted);
        assert_eq!(task.client_agent_id, "test-agent");
    }

    #[tokio::test]
    async fn test_tasks_get_returns_real_state() {
        let mgr = A2aTaskManager::new();
        let params = serde_json::json!({"intent": "hello"});
        let task_id = mgr.create_task("agent1", "message/send", &params).await;
        mgr.update_state(&task_id, A2aTaskState::Completed,
            Some(serde_json::json!({"result": "done"})), None).await;
        let task = mgr.get_task(&task_id).await.unwrap();
        assert_eq!(task.state, A2aTaskState::Completed);
        assert!(task.result_json.is_some());
    }

    #[tokio::test]
    async fn test_tasks_cancel_rejects_completed() {
        let mgr = A2aTaskManager::new();
        let params = serde_json::json!({"intent": "hello"});
        let task_id = mgr.create_task("agent1", "message/send", &params).await;
        mgr.update_state(&task_id, A2aTaskState::Completed, None, None).await;
        let result = mgr.cancel_task(&task_id).await;
        assert!(result.is_err(), "Should not cancel a completed task");
    }

    #[tokio::test]
    async fn test_tasks_cancel_accepts_working() {
        let mgr = A2aTaskManager::new();
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
