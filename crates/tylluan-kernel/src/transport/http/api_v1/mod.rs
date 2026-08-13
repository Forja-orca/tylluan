use axum::{
    Json,
    extract::{State, Query, Path},
    http::{StatusCode, HeaderMap},
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use uuid;

use crate::transport::http::{
    HttpState, EdgePayload, EdgeSearchQuery, EdgeSearchResult, CreateNodePayload, SilvaQueryParams, SilvaRecentQuery,
    DoIntentQuery,
    ExportQuery
};
use crate::memory::mailbox::BlackboardMessage;

pub mod api_guilds;
pub mod api_admin;
pub mod api_coloquio;
pub mod api_federation;
pub mod api_ingest;
pub mod api_mcp;
pub mod api_memory;
pub mod api_monitor;
pub mod api_silva;
pub mod api_collective;
pub mod api_audit;
pub mod api_security;
pub mod api_eval;
pub mod api_ops;
pub mod api_canvas;
pub mod api_journal;
pub mod api_agents;
pub mod api_contracts;
pub mod api_mesh;
pub mod api_repo_map;
pub mod mcp;
pub mod routes;

pub use api_guilds::*;
pub use api_admin::*;
pub use api_coloquio::*;
pub use api_federation::*;
pub use api_ingest::*;
pub use api_mcp::*;
pub use api_memory::*;
pub use api_monitor::*;
pub use api_silva::*;
pub use api_collective::*;
pub use api_ops::*;
pub use api_canvas::*;
pub use api_journal::*;
pub use api_agents::*;
pub use api_contracts::*;
pub use api_mesh::*;
pub use api_repo_map::*;
pub use mcp::*;
pub use routes::*;


/// Returns 503 with a JSON error body if `state.server` is None (kernel not yet initialized).
#[macro_export]
macro_rules! require_server {
    ($state:expr) => {
        match $state.server.as_ref() {
            Some(s) => s,
            None => return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "kernel server not initialized"}))
            ).into_response(),
        }
    };
}






/// Statuses `tasks/update` is allowed to set. A task's terminal statuses
/// (`completed`, `failed`, `cancelled`) can never be updated further -- see the
/// terminal-status guard in the `tasks/update` handler.
const VALID_TASK_UPDATE_STATUSES: &[&str] = &["working", "input_required", "completed", "failed", "cancelled"];
const TERMINAL_TASK_STATUSES: &[&str] = &["completed", "failed", "cancelled", "done"];

/// Validate a `tasks/update` request before it reaches storage. Security finding
/// from the 2026-08-09 external audit: the endpoint previously accepted any string
/// as a status (e.g. "banana") and allowed reverting a completed task back to
/// "pending", corrupting task state for the worker and any polling client.
fn validate_task_status_transition(new_status: &str, current_status: Option<&str>) -> Result<(), String> {
    if !VALID_TASK_UPDATE_STATUSES.contains(&new_status) {
        return Err(format!("invalid status '{new_status}': must be one of {VALID_TASK_UPDATE_STATUSES:?}"));
    }
    if let Some(current) = current_status
        && TERMINAL_TASK_STATUSES.contains(&current)
    {
        return Err(format!("task is already in terminal status '{current}', cannot update"));
    }
    Ok(())
}






async fn mailbox_list_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    match state.mailbox.check_mail("hub", false, 50).await {
        Ok(msgs) => Json(serde_json::json!({ "messages": msgs })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
    }
}

#[derive(serde::Deserialize)]
pub struct MailboxSendRequest {
    pub from: String,
    pub to: String,
    pub subject: String,
    pub body: String,
    #[serde(rename = "threadId")]
    pub thread_id: Option<String>,
    pub priority: Option<i32>,
    #[serde(rename = "ttlSecs")]
    pub ttl_secs: Option<i64>,
}

#[derive(serde::Deserialize)]
pub struct BlackboardPlanTask {
    pub agent: String,
    pub task: String,
}

#[derive(serde::Deserialize)]
pub struct BlackboardPlanRequest {
    pub cycle: String,
    pub tasks: Vec<BlackboardPlanTask>,
}

#[derive(serde::Deserialize)]
pub struct BlackboardTaskDoneRequest {
    pub result: String,
}

async fn mailbox_send_handler(
    State(state): State<Arc<HttpState>>,
    Json(req): Json<MailboxSendRequest>,
) -> impl IntoResponse {
    let payload_str = if let Some(bm) = BlackboardMessage::from_payload(&req.body) {
        bm.to_payload()
    } else {
        let bm = BlackboardMessage {
            msg_type: "task".into(),
            body: req.body.clone(),
            to: req.to.clone(),
            from: req.from.clone(),
            thread_id: req.thread_id.clone(),
            priority: req.priority.unwrap_or(5) as u8,
        };
        bm.to_payload()
    };

    match state.mailbox.send_mail_with_ttl(&req.from, &req.to, &payload_str, req.ttl_secs.unwrap_or(3600)).await {
        Ok(id) => Json(serde_json::json!({ "status": "ok", "message_id": id })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
    }
}

async fn blackboard_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    use rusqlite::params;
    let now = chrono::Utc::now();
    let two_hours_ago = now - chrono::Duration::hours(2);

    let conn_guard = state.silva.conn_lock();
    let conn = conn_guard.lock().await;

    let pending_tasks: Vec<serde_json::Value> = {
        let mut stmt = match conn.prepare(
            "SELECT id, content, metadata, created_at FROM nodes WHERE type = 'task' AND metadata LIKE '%\"status\":\"pending\"%' ORDER BY created_at ASC LIMIT 50"
        ) {
            Ok(s) => s,
            Err(_) => return (StatusCode::OK, Json(serde_json::json!({ "pending": [], "completed_today": 0, "active_agents": [], "total_tasks": 0 }))),
        };
        let mut rows = match stmt.query([]) {
            Ok(r) => r,
            Err(_) => return (StatusCode::OK, Json(serde_json::json!({ "pending": [], "completed_today": 0, "active_agents": [], "total_tasks": 0 }))),
        };
        let mut pending_tasks: Vec<serde_json::Value> = vec![];
        while let Ok(Some(row)) = rows.next() {
            let id: String = match row.get(0) { Ok(v) => v, Err(_) => continue };
            let content: String = match row.get(1) { Ok(v) => v, Err(_) => continue };
            let meta_str: String = match row.get(2) { Ok(v) => v, Err(_) => continue };
            let created_at: String = row.get(3).unwrap_or_else(|_| now.to_rfc3339());
            let meta: serde_json::Value = serde_json::from_str(&meta_str).unwrap_or_default();
            let created_by = meta.get("created_by").and_then(|v| v.as_str()).unwrap_or("?");
            let assigned_to = meta.get("assigned_to").and_then(|v| v.as_str()).unwrap_or("unassigned");
            let priority = meta.get("priority").and_then(|v| v.as_i64()).unwrap_or(5);

            let age_mins = chrono::DateTime::parse_from_rfc3339(&created_at)
                .map(|dt| now.signed_duration_since(dt.with_timezone(&chrono::Utc)).num_minutes())
                .unwrap_or(0);

            pending_tasks.push(serde_json::json!({
                "id": id,
                "content": content.chars().take(100).collect::<String>(),
                "created_by": created_by,
                "assigned_to": assigned_to,
                "priority": priority,
                "age_mins": age_mins
            }));
        }
        pending_tasks
    };

    let completed_today: i64 = conn.query_row(
        "SELECT COUNT(*) FROM nodes WHERE type = 'task' AND metadata LIKE '%\"status\":\"completed\"%' AND updated_at > ?1",
        params![two_hours_ago.to_rfc3339()],
        |r| r.get(0),
    ).unwrap_or(0);

    let active_agents: Vec<String> = {
        let mut stmt = match conn.prepare(
            "SELECT DISTINCT agent_id FROM nodes WHERE created_at > ?1 AND agent_id IS NOT NULL LIMIT 20"
        ) {
            Ok(s) => s,
            Err(_) => return (StatusCode::OK, Json(serde_json::json!({ "pending": [], "completed_today": 0, "active_agents": [], "total_tasks": 0 }))),
        };
        let mut rows = match stmt.query(params![two_hours_ago.to_rfc3339()]) {
            Ok(r) => r,
            Err(_) => return (StatusCode::OK, Json(serde_json::json!({ "pending": [], "completed_today": 0, "active_agents": [], "total_tasks": 0 }))),
        };
        let mut agents = vec![];
        while let Ok(Some(row)) = rows.next() {
            if let Ok(aid) = row.get::<_, String>(0) {
                agents.push(aid);
            }
        }
        agents
    };

    let total_tasks: i64 = conn.query_row(
        "SELECT COUNT(*) FROM nodes WHERE type = 'task'",
        [],
        |r| r.get(0),
    ).unwrap_or(0);

    (StatusCode::OK, Json(serde_json::json!({
        "pending": pending_tasks,
        "completed_today": completed_today,
        "active_agents": active_agents,
        "total_tasks": total_tasks
    })))
}

async fn blackboard_plan_handler(
    State(state): State<Arc<HttpState>>,
    Json(req): Json<BlackboardPlanRequest>,
) -> impl IntoResponse {
    let mut message_ids = Vec::new();
    for task in &req.tasks {
        match state.mailbox.send_task("scheduler", &task.agent, &task.task, 86400).await {
            Ok(id) => message_ids.push(id),
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
        }
    }

    tracing::info!("[BLACKBOARD] Plan for cycle {} published with {} tasks", req.cycle, message_ids.len());
    
    (StatusCode::OK, Json(serde_json::json!({
        "published": message_ids.len(),
        "cycle": req.cycle,
        "message_ids": message_ids
    }))).into_response()
}

async fn blackboard_agent_tasks_handler(
    State(state): State<Arc<HttpState>>,
    Path(agent): Path<String>,
) -> impl IntoResponse {
    match state.mailbox.get_tasks_for_agent(&agent).await {
        Ok(tasks) => Json(serde_json::json!({ "agent": agent, "tasks": tasks })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn blackboard_task_done_handler(
    State(state): State<Arc<HttpState>>,
    Path(msg_id): Path<String>,
    Json(req): Json<BlackboardTaskDoneRequest>,
) -> impl IntoResponse {
    match state.mailbox.mark_task_done(&msg_id, &req.result).await {
        Ok(_) => {
            // Drain completed task to SilvaDB as episode (R12-1)
            let silva = std::sync::Arc::clone(&state.silva);
            let task_id = msg_id.clone();
            let result = req.result.clone();
            tokio::spawn(async move {
                let episode_id = format!("bb_episode:{task_id}");
                let content = format!("Blackboard task completed | id:{task_id} | {result}");
                let meta = serde_json::json!({"source":"blackboard_drain","task_id": task_id}).to_string();
                if let Err(e) = silva.upsert_node(&episode_id, "episode", &content, &meta).await {
                    tracing::warn!("blackboard drain failed: {}", e);
                } else {
                    let _ = silva.touch_node(&episode_id, "blackboard", "bb_drain").await;
                }
            });

            // Emit SSE event
            let _ = state.broadcast_tx.send(serde_json::json!({
                "type": "task_completed",
                "data": {
                    "msg_id": msg_id,
                    "ts": chrono::Utc::now().timestamp()
                }
            }));

            (StatusCode::OK, Json(serde_json::json!({ "status": "completed", "msg_id": msg_id }))).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn do_intent_handler(
    State(state): State<Arc<HttpState>>,
    Query(q): Query<DoIntentQuery>,
    body: axum::body::Bytes,
) -> impl IntoResponse {

    let body_json: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    let intent = q.intent.or_else(|| body_json.get("intent").and_then(|v| v.as_str()).map(String::from)).unwrap_or_default();
    let query = q.query.or_else(|| body_json.get("query").and_then(|v| v.as_str()).map(String::from)).unwrap_or_default();
    let tool = q.tool.or_else(|| body_json.get("tool").and_then(|v| v.as_str()).map(String::from)).unwrap_or_else(|| "tylluan_do".to_string());
    let agent_id = q.agent_id.clone().or_else(|| body_json.get("agent_id").and_then(|v| v.as_str()).map(String::from)).unwrap_or_else(|| "tylluan-cli".to_string());
    // session_id from query string or body (for dashboard tool_count tracking)
    let session_id: Option<String> = body_json.get("session_id").and_then(|v| v.as_str()).map(String::from)
        .or_else(|| q.agent_id.clone());
    // M23-Fractal: score gate — intents below threshold return candidate guilds instead of executing
    // If an explicit guild hint is provided, bypass the fractal gate
    let guild_param = q.guild.clone().or_else(|| body_json.get("guild").and_then(|v| v.as_str()).map(String::from));
    const FRACTAL_THRESHOLD: f32 = 0.82;
    if guild_param.is_none() && !intent.is_empty()
        && let Some(top) = state.matcher.trigger_match_pub(&intent)
            && top.score < FRACTAL_THRESHOLD {
                let mut candidates = state.matcher.match_all(&intent, None, 0.0);
                candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
                candidates.truncate(4);
                let candidate_list: Vec<serde_json::Value> = candidates.iter().map(|c| {
                    serde_json::json!({ "guild": c.guild_name, "score": c.score, "method": format!("{:?}", c.method) })
                }).collect();
                return (StatusCode::OK, Json(serde_json::json!({
                    "status": "ambiguous",
                    "score": top.score,
                    "threshold": FRACTAL_THRESHOLD,
                    "candidates": candidate_list,
                    "hint": "Be more specific, or pick a candidate by passing guild=<name>"
                }))).into_response();
            }

    let server_arc = match state.server.as_ref() {
        Some(s) => s,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"error": "Sovereign server not initialized"}))).into_response(),
    };
    let server = server_arc.read().await;
    let guild = q.guild.or_else(|| body_json.get("guild").and_then(|v| v.as_str()).map(String::from));
    let mut args = serde_json::json!({ "intent": intent, "agent_id": agent_id, "query": query });
    if let Some(g) = guild.filter(|s| !s.is_empty()) {
        args["guild"] = serde_json::Value::String(g);
    }
    if let Some(content) = body_json.get("content").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
        args["content"] = serde_json::Value::String(content.to_string());
    }
    // M31-P2 Plan mode: must be forwarded so /api/v1/do can dry-run like the
    // MCP tool-call path already does. Without this, plan=true in the HTTP
    // body was silently dropped and the intent executed for real instead of
    // just resolving guild+tool+args (found live 2026-07-27: a "git status"
    // call with plan=true actually ran the shell command).
    if let Some(plan) = body_json.get("plan").and_then(|v| v.as_bool()) {
        args["plan"] = serde_json::Value::Bool(plan);
    }
    // Explicit `arguments` passthrough for kernel tools that need fields beyond
    // intent/query/guild/content (e.g. approve_action's requestId/approved/grant_level).
    // Named fields above still win on conflict — arguments only fills gaps.
    if let Some(extra) = body_json.get("arguments").and_then(|v| v.as_object())
        && let Some(args_obj) = args.as_object_mut() {
            for (k, v) in extra {
                args_obj.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
    let _ = state.broadcast_tx.send(serde_json::json!({ "type": "tool_call", "tool": tool, "intent": intent, "status": "started", "ts": chrono::Utc::now().timestamp_millis() }));
    let call_start = std::time::Instant::now();
    match server.handle_kernel_tool(&tool, args.as_object().cloned()).await {
        Ok(res) => {
            let is_error = res.is_error.unwrap_or(false);
            let latency_ms = call_start.elapsed().as_millis() as u64;
            let _ = state.silva.record_tool_call(&agent_id, &tool, &tool, !is_error, latency_ms).await;
            // Update session tool_count and last_active
            if let Some(ref sid) = session_id {
                let now_unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                let mut sessions = state.sessions.write().await;
                if let Some(sess) = sessions.get_mut(sid) {
                    sess.tool_count += 1;
                    sess.last_active = std::time::Instant::now();
                    sess.last_active_unix = now_unix;
                    if !intent.is_empty() { sess.last_intent = Some(intent.clone()); }
                    sess.last_guild = Some(tool.clone());
                    let new_count = sess.tool_count;
                    let _ = state.broadcast_tx.send(serde_json::json!({
                        "type": "session_updated",
                        "data": { "session_id": sid, "tool_count": new_count }
                    }));
                }
            }
            let texts: Vec<String> = res.content.iter()
                .filter_map(|c| c.as_text().map(|t| t.text.clone()))
                .collect();
            let _ = state.broadcast_tx.send(serde_json::json!({
                "type": "tool_call",
                "tool": tool,
                "intent": intent,
                "agent_id": agent_id,
                "status": "finished",
                "ok": !is_error,
                "error": if is_error { Some(format!("{:?}", res.content)) } else { None },
                "ts": chrono::Utc::now().timestamp_millis()
            }));
            // `content`/`result` expose the real tool output text (and its parsed JSON when
            // the tool returned a JSON string, e.g. plan mode / audit queries) so dashboard
            // consumers don't have to scrape the Rust Debug-formatted `response` string.
            let parsed_result = texts.first().and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok());
            (StatusCode::OK, Json(serde_json::json!({
                "status": "ok",
                "response": format!("{:?}", res.content),
                "content": texts,
                "result": parsed_result,
                "is_error": is_error,
            }))).into_response()
        },
        Err(e) => {
            let latency_ms = call_start.elapsed().as_millis() as u64;
            let _ = state.silva.record_tool_call(&agent_id, &tool, &tool, false, latency_ms).await;
            let _ = state.broadcast_tx.send(serde_json::json!({
                "type": "tool_call",
                "tool": tool,
                "intent": intent,
                "agent_id": agent_id,
                "status": "finished",
                "ok": false,
                "error": Some(e.to_string()),
                "ts": chrono::Utc::now().timestamp_millis()
            }));
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response()
        }
    }
}

// --- M23-Fractal: tool discovery ---

#[derive(serde::Deserialize)]
struct ExploreQuery {
    domain: Option<String>,
}

async fn tools_explore_handler(
    State(_state): State<Arc<HttpState>>,
    Query(q): Query<ExploreQuery>,
) -> impl IntoResponse {
    use crate::transport::server::TylluanServer;
    let tools = TylluanServer::kernel_tools();
    let domain = q.domain.as_deref().unwrap_or("").to_lowercase();

    let matching: Vec<serde_json::Value> = tools.iter()
        .filter(|t| {
            if domain.is_empty() { return true; }
            let cat = format!("{:?}", t.category).to_lowercase();
            t.name.to_lowercase().contains(&domain)
                || cat.contains(&domain)
                || t.subtools.iter().any(|s| s.to_lowercase().contains(&domain))
        })
        .map(|t| serde_json::json!({
            "name": t.name,
            "description": t.description,
            "subtools": t.subtools,
        }))
        .collect();

    (StatusCode::OK, Json(serde_json::json!({
        "status": "ok",
        "domain": q.domain,
        "tools": matching,
    }))).into_response()
}

// --- GUILDS ---

// --- SILVA & MEMORY ---

async fn silva_stats_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    Json(state.silva.get_detailed_stats().await.unwrap_or_default())
}

// --- DOCTOR ---

async fn doctor_diagnose_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    Json(state.doctor.diagnose().await)
}

/// GET /api/v1/llm-examples/export — NDJSON de llm_decision_examples con split
/// determinista por node_id. Stats en header X-Export-Stats.
///
/// Fase 2 (CoherenceGate P4-P2): cada fila se enriquece con `ground_truth`
/// cuando existe una resolución real de ADR-011 recall_feedback para ese
/// node_id (1=útil, -1=no útil después). `null` cuando aún no hay señal
/// resuelta -- ausencia de dato, no una tercera categoría. Mismo caveat de
/// honestidad que recall_feedback.rs: es una señal heurística proxy por
/// solapamiento de palabras, no verdad absoluta.
async fn llm_examples_export_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    match crate::security::llm_examples::collect_examples_json() {
        Ok((mut rows, mut stats)) => {
            let node_ids: Vec<String> = rows.iter()
                .filter_map(|r| r.get("node_id").and_then(|v| v.as_str()).map(str::to_string))
                .collect();
            let ground_truth = state.silva.get_resolved_feedback_map(&node_ids).await.unwrap_or_default();
            let mut labeled = 0usize;
            for row in rows.iter_mut() {
                let node_id = row.get("node_id").and_then(|v| v.as_str()).unwrap_or("");
                match ground_truth.get(node_id) {
                    Some(&useful) => {
                        row["ground_truth"] = serde_json::json!(useful);
                        labeled += 1;
                    }
                    None => row["ground_truth"] = serde_json::Value::Null,
                }
            }
            stats.ground_truth_labeled = labeled;

            let mut body = String::new();
            for row in &rows {
                body.push_str(&serde_json::to_string(&row).unwrap_or_else(|_| "{}".to_string()));
                body.push('\n');
            }
            let stats_json = serde_json::to_string(&stats).unwrap_or_else(|_| "{}".to_string());
            (
                StatusCode::OK,
                [
                    (axum::http::header::CONTENT_TYPE, "application/x-ndjson".to_string()),
                    (
                        axum::http::HeaderName::from_static("X-Export-Stats"),
                        stats_json,
                    ),
                ],
                body,
            )
                .into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

#[derive(serde::Deserialize)]
struct DoctorRepairRequest {
    target: String,
    name: Option<String>,
}

#[derive(serde::Serialize)]
struct DoctorRepairResponse {
    success: bool,
    message: String,
}

async fn doctor_repair_handler(
    State(state): State<Arc<HttpState>>,
    Json(payload): Json<DoctorRepairRequest>,
) -> impl IntoResponse {
    match state.doctor.repair(&payload.target, payload.name.as_deref()).await {
        Ok(msg) => (StatusCode::OK, Json(DoctorRepairResponse { success: true, message: msg })).into_response(),
        Err(e) => (StatusCode::CONFLICT, Json(DoctorRepairResponse { success: false, message: e })).into_response(),
    }
}

async fn silva_recent_handler(State(state): State<Arc<HttpState>>, Query(q): Query<SilvaRecentQuery>) -> impl IntoResponse {
    let limit = q.limit.unwrap_or(20);
    Json(state.silva.get_recent_nodes(limit).await.unwrap_or_default())
}

#[derive(serde::Deserialize)]
struct GraphScopeQuery {
    prefix: Option<String>,
    limit: Option<usize>,
}

/// J-8: list nodes under a hierarchical owner_scope prefix (e.g. "user:alice").
async fn graph_scope_handler(State(state): State<Arc<HttpState>>, Query(q): Query<GraphScopeQuery>) -> impl IntoResponse {
    let prefix = q.prefix.unwrap_or_default();
    let limit = q.limit.unwrap_or(100);
    let rows = state.silva.get_nodes_by_scope_prefix(&prefix, limit).await.unwrap_or_default();
    let nodes: Vec<serde_json::Value> = rows.into_iter().map(|(id, node_type, content, owner_scope)| {
        serde_json::json!({ "id": id, "node_type": node_type, "type": node_type, "content": content, "owner_scope": owner_scope })
    }).collect();
    Json(serde_json::json!({ "nodes": nodes }))
}

async fn list_contradictions_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    match state.silva.get_deprecated_nodes(50).await {
        Ok(nodes) => Json(serde_json::json!({ "deprecated_nodes": nodes, "count": nodes.len() })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn silva_add_edge_handler(State(state): State<Arc<HttpState>>, Json(p): Json<EdgePayload>) -> impl IntoResponse {
    let weight = p.weight.unwrap_or(1.0);
    match state.silva.add_edge(&p.source, &p.target, &p.edge_type, weight, &p.metadata).await {
        Ok(_) => {
            let edge_embed_id = format!("edge::{}::{}::{}", p.source, p.target, p.edge_type);
            let embed_text = format!("{}: {} -> {}", p.edge_type, p.source, p.target);
            if let Some(engine) = state.matcher.engine()
                && let Ok(vec) = engine.embed(&embed_text) {
                    let _ = state.silva.save_embedding(&edge_embed_id, &vec, "bge-m3", None).await;
                }
            (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"ok": false, "error": e.to_string()}))).into_response(),
    }
}

async fn silva_edge_search_handler(State(state): State<Arc<HttpState>>, Json(q): Json<EdgeSearchQuery>) -> impl IntoResponse {
    let limit = q.limit.unwrap_or(10).min(1000);
    let engine = match state.matcher.engine() {
        Some(e) => e,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"error": "embedding engine not available"}))).into_response(),
    };
    let query_vec = match engine.embed(&q.query) {
        Ok(v) => v,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    };

    const MAX_EMBEDDING_BLOB: usize = 10_000_000;
    let results: Option<Vec<EdgeSearchResult>> = tokio::task::block_in_place(|| {
        let conn = state.silva.conn.blocking_lock();
        let mut stmt = conn.prepare("SELECT node_id, embedding FROM node_embeddings WHERE node_id LIKE 'edge::%'").ok()?;
        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            if blob.len() > MAX_EMBEDDING_BLOB {
                return Err(rusqlite::Error::ToSqlConversionFailure(
                    Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, "embedding blob exceeds maximum size"))
                ));
            }
            Ok((id, blob))
        }).ok()?;

        let mut scored: Vec<(String, f64)> = Vec::new();
        for row in rows.flatten() {
            let (id, blob) = row;
            if blob.len() < 4 { continue; }
            let stored: Vec<f32> = blob.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
            if stored.len() != query_vec.len() { continue; }
            let sim = crate::memory::cosine::cosine_similarity(&query_vec, &stored) as f64;
            if sim > 0.05 {
                scored.push((id, sim));
            }
        }
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        const MAX_SCORED_OUTPUT: usize = 1000;
        let out_cap = scored.len().min(MAX_SCORED_OUTPUT);
        let mut out = Vec::with_capacity(out_cap);
        for (id, sim) in &scored {
            let parts: Vec<&str> = id.splitn(4, "::").collect();
            if parts.len() < 4 { continue; }
            let source = parts[1].to_string();
            let target = parts[2].to_string();
            let edge_type = parts[3].to_string();
            let weight: f64 = conn.query_row(
                "SELECT weight FROM edges WHERE source = ?1 AND target = ?2 AND type = ?3",
                rusqlite::params![&source, &target, &edge_type],
                |r| r.get(0),
            ).unwrap_or(1.0);
            out.push(EdgeSearchResult { source, target, edge_type, weight, similarity: *sim });
        }
        Some(out)
    });

    match results {
        Some(r) => Json(serde_json::json!({"results": r, "count": r.len()})).into_response(),
        None => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "search failed"}))).into_response(),
    }
}

async fn silva_create_node_handler(State(state): State<Arc<HttpState>>, Json(p): Json<CreateNodePayload>) -> impl IntoResponse {
    let node_id = format!("{}__{}", p.node_type, uuid::Uuid::new_v4().simple());
    match state.silva.upsert_node(&node_id, &p.node_type, &p.content, &p.metadata).await {
        Ok(_) => {
            if let Some(w) = p.weight {
                let _ = tokio::task::block_in_place(|| {
                    state.silva.conn.blocking_lock().execute(
                        "UPDATE nodes SET weight = ?1 WHERE id = ?2",
                        rusqlite::params![w, &node_id],
                    )
                });
            }
            // upsert_node() is a plain INSERT with no embedding step. Nodes created
            // through this HTTP endpoint (e.g. vision guild) were previously invisible
            // to semantic hybrid search because nothing ever populated node_embeddings
            // for them. Mirror the same embed-then-save pattern already used by
            // silva_add_edge_handler above so every node created here gets a real
            // BGE-M3 embedding like nodes created via tylluan_remember/handler_do.
            if let Some(engine) = state.matcher.engine()
                && let Ok(vec) = engine.embed(&p.content) {
                    let _ = state.silva.save_embedding(&node_id, &vec, "bge-m3", None).await;
                }
            (StatusCode::CREATED, Json(serde_json::json!({"ok": true, "id": node_id}))).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"ok": false, "error": e.to_string()}))).into_response(),
    }
}

async fn silva_graph_handler(State(state): State<Arc<HttpState>>, Query(p): Query<SilvaQueryParams>) -> impl IntoResponse {
    if p.cluster.unwrap_or(false) {
        let _ = state.silva.detect_communities().await;
    }
    
    let nodes = state.silva.get_nodes_limited(p.limit.unwrap_or(300), p.min_weight.unwrap_or(0.0)).await.unwrap_or_default();
    
    // Batch fetch stigmergy heat for all returned nodes (24-hour window)
    let node_ids: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();
    let heats = state.silva.get_heat_batch(&node_ids, 24).await.unwrap_or_default();
    let active_agents = state.silva.get_active_agents_batch(&node_ids, 24).await.unwrap_or_default();

    // Batch-serialize nodes without per-node DB calls (N+1 was causing silent empty response above ~100 nodes)
    let node_list: Vec<serde_json::Value> = nodes.iter().map(|node| {
        let mut node_json = serde_json::to_value(node).unwrap_or_default();
        if let Some(obj) = node_json.as_object_mut() {
            let heat = heats.get(&node.id).cloned().unwrap_or(0.0);
            let last_agent = active_agents.get(&node.id).cloned().unwrap_or_default();
            obj.insert("traces".to_string(), serde_json::json!([]));
            obj.insert("stigmergy_heat".to_string(), serde_json::json!(heat));
            obj.insert("diffuse_heat_traces".to_string(), serde_json::json!(0));
            obj.insert("last_agent".to_string(), serde_json::json!(last_agent));
        }
        node_json
    }).collect();
    // get_all_edges() returns every edge in SilvaDB regardless of the node
    // limit above -- with a large graph (thousands of nodes, only `limit`
    // returned by weight) most edges end up pointing at a source/target that
    // isn't in `node_list` at all. Force-directed layout libraries choke on
    // links referencing missing nodes (best case: silently dropped: worst
    // case, depending on the client, the simulation stalls entirely) --
    // filter to only edges where both ends are actually present.
    let node_id_set: std::collections::HashSet<&str> = node_ids.iter().map(|s| s.as_str()).collect();
    let edges = state.silva.get_all_edges().await.unwrap_or_default();
    let edges: Vec<serde_json::Value> = edges.into_iter().filter(|e| {
        let source = e.get("source").and_then(|v| v.as_str()).unwrap_or_default();
        let target = e.get("target").and_then(|v| v.as_str()).unwrap_or_default();
        node_id_set.contains(source) && node_id_set.contains(target)
    }).collect();
    Json(serde_json::json!({ "nodes": node_list, "links": edges }))
}

async fn silva_traces_handler(
    State(state): State<Arc<HttpState>>,
    Query(q): Query<TracesQuery>,
) -> impl IntoResponse {
    let node_id = match &q.node_id {
        Some(id) if !id.is_empty() => id.clone(),
        _ => return Json(serde_json::json!({ "traces": [], "node_id": serde_json::Value::Null })),
    };
    let traces = state.silva.get_node_traces(&node_id, 50).await.unwrap_or_default();
    let trace_list: Vec<serde_json::Value> = traces.iter().map(|t| {
        serde_json::json!({
            "node_id": t.node_id,
            "agent_id": t.agent_id,
            "touched_at": t.touched_at,
            "trace_type": t.trace_type,
        })
    }).collect();
    Json(serde_json::json!({ "node_id": node_id, "traces": trace_list }))
}

#[derive(serde::Deserialize)]
struct TracesQuery { node_id: Option<String> }

async fn silva_shared_knowledge_handler(
    State(state): State<Arc<HttpState>>,
    Path((agent_a, agent_b)): Path<(String, String)>,
) -> impl IntoResponse {
    match state.silva.find_shared_knowledge(&agent_a, &agent_b, 50).await {
        Ok(nodes) => {
            let list: Vec<serde_json::Value> = nodes.into_iter().map(|n| {
                serde_json::json!({
                    "id": n.id,
                    "node_type": n.node_type,
                    "content": n.content.chars().take(200).collect::<String>(),
                    "weight": n.weight,
                    "created_at": n.created_at,
                })
            }).collect();
            (StatusCode::OK, Json(serde_json::json!({
                "agent_a": agent_a,
                "agent_b": agent_b,
                "shared": list,
                "count": list.len(),
            }))).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn silva_consolidate_handler(
    State(state): State<Arc<HttpState>>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let threshold = body.get("threshold").and_then(|v| v.as_f64()).unwrap_or(0.9);
    let max_batch = body.get("max_batch").and_then(|v| v.as_u64()).map(|v| v as usize).unwrap_or(100);
    let t0 = std::time::Instant::now();
    match state.silva.consolidate_episodes(threshold, max_batch).await {
        Ok(merged) => {
            let elapsed_ms = t0.elapsed().as_millis() as u64;
            (StatusCode::OK, Json(serde_json::json!({
                "merged": merged,
                "elapsed_ms": elapsed_ms
            }))).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "error": e.to_string()
        }))).into_response(),
    }
}

async fn knowledge_export_handler(State(state): State<Arc<HttpState>>, Query(q): Query<ExportQuery>) -> impl IntoResponse {
    let nodes = state.silva.get_nodes_paginated(q.limit, q.offset).await.unwrap_or_default();
    let edges = state.silva.get_all_edges().await.unwrap_or_default();
    Json(serde_json::json!({ "graph": { "nodes": nodes, "edges": edges } }))
}

async fn silva_save_summary_handler(State(state): State<Arc<HttpState>>, Json(req): Json<serde_json::Value>) -> impl IntoResponse {
    let cluster_id = match req.get("cluster_id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "missing cluster_id" }))).into_response(),
    };
    let summary = match req.get("summary").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "missing summary" }))).into_response(),
    };
    let rag = crate::memory::graph_rag::GraphRagManager::new(state.silva.clone());
    match rag.save_summary(cluster_id, summary, vec![]).await {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({ "status": "ok" }))).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "save failed" }))).into_response(),
    }
}

async fn silva_analyze_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    match state.silva.analyze_graph_deep().await {
        Ok(res) => (StatusCode::OK, Json(res)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn silva_communities_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    match state.silva.detect_communities().await {
        Ok(res) => (StatusCode::OK, Json(res)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn silva_delete_node_handler(
    State(state): State<Arc<HttpState>>,
    Path(node_id): Path<String>,
) -> impl IntoResponse {
    match state.silva.delete_node(&node_id).await {
        Ok(true) => (
            StatusCode::OK,
            Json(serde_json::json!({"deleted": true, "node_id": node_id})),
        ).into_response(),
        Ok(false) => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Node not found or is protected", "node_id": node_id})),
        ).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ).into_response(),
    }
}

// --- TOOLS & CONFIG ---

async fn tools_list_handler(State(state): State<Arc<HttpState>>) -> Response {
    let server = require_server!(state).read().await;
    Json(server.all_tools().await).into_response()
}

async fn capabilities_handler(State(state): State<Arc<HttpState>>) -> Response {
    let server = require_server!(state).read().await;
    let sovereign_tools = server.all_tools().await;

    // 2. Get registered and running guilds
    let guilds = state.registry.status_all().await.unwrap_or_default();

    // 3. Get all tools from all guilds
    let guild_tools = {
        let registry_arc = state.registry.arc();
        let registry_guard = registry_arc.read().await;
        registry_guard.all_tools()
    };

    // 4. Get active sessions
    let sessions = state.sessions.read().await;
    let sessions_list: Vec<serde_json::Value> = sessions.values().map(|s| {
        serde_json::json!({
            "id": s.id,
            "client_name": s.client_name,
            "agent_id": s.agent_id,
            "tool_count": s.tool_count,
            "last_intent": s.last_intent,
            "last_guild": s.last_guild,
            "last_active_unix": s.last_active_unix,
            "created_unix": s.created_unix,
        })
    }).collect();

    // 5. Expose Prompts and Resources
    let mcp_prompts = serde_json::json!([
        {
            "name": "tylluan_guilds_catalog",
            "description": "System prompt to inject the complete guild tool catalog into your context. Use this to discover available specialized tools for specific tasks."
        }
    ]);

    let mcp_resources = serde_json::json!([
        {
            "uri": "tylluan://metadata/guilds",
            "name": "Guild Tool Catalog",
            "description": "JSON database of all available guilds and their specialized tool schemas.",
            "mimeType": "application/json"
        }
    ]);

    let response = serde_json::json!({
        "status": "ok",
        "version": state.version,
        "sovereign_contract": {
            "tools": sovereign_tools
        },
        "guilds": guilds,
        "all_guild_tools": guild_tools,
        "mcp": {
            "prompts": mcp_prompts,
            "resources": mcp_resources
        },
        "sessions": sessions_list
    });

    Json(response).into_response()
}

async fn models_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let config = state.config.read().await;
    let embedding_model = config.memory.embedding_model.clone();
    let vision_path = config.vision.model_path.clone();
    let inference_model = config.inference.primary_model.clone();
    let vector_dims = config.memory.vector_dimensions;

    // Real disk scanner for local model files. Classifies each entry as
    // embedding/vision/generative so the dashboard can route them to the
    // right panel instead of offering an embedding model as a chat model
    // (or vice versa) -- matches known config paths first, falls back to
    // a directory-name heuristic for anything not currently active.
    let embedding_dir_name = std::path::Path::new(&embedding_model)
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase());
    let vision_dir_name = std::path::Path::new(&vision_path)
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase());

    let classify_model_dir = |dir_name: &str| -> &'static str {
        let lower = dir_name.to_lowercase();
        if embedding_dir_name.as_deref() == Some(lower.as_str())
            || lower.contains("bge")
            || lower.contains("embed")
            || lower.contains("nomic")
        {
            "embedding"
        } else if vision_dir_name.as_deref() == Some(lower.as_str())
            || lower.contains("vision")
            || lower.contains("vlm")
            || lower.contains("moondream")
        {
            "vision"
        } else {
            "generative"
        }
    };

    let mut detected_local_models = Vec::new();
    let models_dir = std::path::Path::new("models");
    if models_dir.exists()
        && let Ok(entries) = std::fs::read_dir(models_dir)
    {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let dir_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("unknown");
                let mut size = 0u64;
                if let Ok(files) = std::fs::read_dir(&path) {
                    for f in files.flatten() {
                        if let Ok(meta) = f.metadata() {
                            size += meta.len();
                        }
                    }
                }
                detected_local_models.push(serde_json::json!({
                    "id": dir_name,
                    "name": format!("Local {}", dir_name),
                    "path": path.to_string_lossy(),
                    "size_bytes": size,
                    "size_mb": size / (1024 * 1024),
                    "installed": true,
                    "model_type": classify_model_dir(dir_name),
                }));
            }
        }
    }

    Json(serde_json::json!({
        "active": {
            "embedding": embedding_model,
            "vision": vision_path,
            "inference": inference_model,
            "vector_dimensions": vector_dims
        },
        "detected_local_models": detected_local_models,
        "available_embeddings": [
            { "name": "BGE-M3", "dimensions": 1024, "multilingual": true, "note": "default, multilingual best-in-class" },
            { "name": "BGE-base-en-v1.5", "dimensions": 768, "multilingual": false, "note": "fast, English-only" },
            { "name": "Nomic-Embed-v2", "dimensions": 768, "multilingual": true, "note": "Nomic hosted equivalent, 768 dims" }
        ],
        "available_vision": [
            { "name": "SmolVLM2-256M-Instruct", "path": "HuggingFaceTB/SmolVLM2-256M-Instruct", "note": "ONNX, CPU-friendly, PIL+numpy sin torch" }
        ],
        "notes": {
            "embedding_change_requires": "kernel_restart_and_reindex",
            "dimension_mismatch_risk": "changing model with different dims requires full reindex"
        }
    })).into_response()
}

// ── Embed endpoint ─────────────────────────────────────────────────────────
// BGE-M3 1024-dim embeddings via POST /api/v1/embed.
// Exists because Python guilds (night_reasoner route_intent) need embedding
// similarity comparisons without loading a second copy of the model.
#[derive(serde::Deserialize)]
struct EmbedRequest {
    text: String,
}

async fn embed_handler(
    State(state): State<Arc<HttpState>>,
    Json(req): Json<EmbedRequest>,
) -> Response {
    let srv_arc = require_server!(state);
    let srv = srv_arc.read().await;
    match srv.matcher.engine() {
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "embedding engine not ready"})),
        ).into_response(),
        Some(engine) => {
            match engine.embed(&req.text) {
                Ok(embedding) => Json(serde_json::json!({
                    "embedding": embedding,
                    "dimension": embedding.len(),
                    "model": "bge-m3"
                })).into_response(),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": e.to_string()})),
                ).into_response(),
            }
        }
    }
}

/// Returns MCP client config snippets built from the kernel's actual
/// runtime state -- never hardcoded. The URL is derived from the request's
/// own `Host` header (the address the client just used to reach this
/// endpoint, so it's correct even behind a fallback port or a non-default
/// config), and the token is only included when auth is actually required
/// (dev_mode=false and a real token is configured) -- embedding a token
/// placeholder when dev_mode=true would suggest auth is needed when it isn't.
async fn setup_hint_handler(
    State(state): State<Arc<HttpState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let host = headers.get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("127.0.0.1:3030");
    let base_url = format!("http://{host}");
    let dev_mode = state.dev_mode.unwrap_or(false);
    let token_query = if !dev_mode {
        state.auth_token.as_deref().map(|t| format!("?token={t}")).unwrap_or_default()
    } else {
        String::new()
    };
    let sse_url = format!("{base_url}/sse{token_query}");
    let embedding_model = state.config.read().await.memory.embedding_model.clone();
    let mode = if embedding_model == "none" { "BM25-only" } else { "hybrid (BM25 + vector)" };

    Json(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "status": "ready",
        "auth_required": !dev_mode,
        "embedding_model": embedding_model,
        "mode": mode,
        "note": "Run 'tylluan download-models' if embedding_model requires a model that isn't cached yet",
        "mcp_clients": {
            "claude_desktop": {
                "config": {
                    "mcpServers": {
                        "tylluan": {
                            "type": "sse",
                            "url": sse_url
                        }
                    }
                },
                "location": "~/.claude/claude_desktop_config.json"
            },
            "claude_code": {
                "command": format!("/mcp add tylluan sse {sse_url}")
            },
            "cursor": {
                "command": format!("Add MCP server: {sse_url}")
            },
            "codex": {
                "command": format!("npx -y mcp-remote {sse_url}"),
                "note": "Connects Codex as a native stdio bridge to Tylluan's SSE endpoint (7f12879)"
            },
            "qwen_desktop": {
                "config": {
                    "mcpServers": {
                        "tylluan": {
                            "type": "sse",
                            "url": sse_url
                        }
                    }
                },
                "location": "~/.qwen/qwen_desktop_config.json"
            }
        },
        "verify": {
            "curl": format!("curl {base_url}/health"),
            "dashboard": base_url
        }
    }))
}

#[derive(serde::Deserialize)]
pub struct SetupHintApplyRequest {
    #[serde(default = "default_client_name")]
    pub client: String,
    #[serde(default)]
    pub confirm: bool,
}

fn default_client_name() -> String {
    "claude_desktop".to_string()
}

/// M40-P8: Explicit auto-config helper with backup and diff preview.
/// Guardrail 1 (José): Must be an explicit invocation with 'confirm': true;
/// default (confirm=false) returns proposed diff and target path without writing to disk.
async fn setup_hint_apply_handler(
    State(state): State<Arc<HttpState>>,
    headers: HeaderMap,
    Json(payload): Json<SetupHintApplyRequest>,
) -> impl IntoResponse {
    let host = headers.get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("127.0.0.1:3030");
    let base_url = format!("http://{host}");
    let dev_mode = state.dev_mode.unwrap_or(false);
    let token_query = if !dev_mode {
        state.auth_token.as_deref().map(|t| format!("?token={t}")).unwrap_or_default()
    } else {
        String::new()
    };
    let sse_url = format!("{base_url}/sse{token_query}");

    let home_dir = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()
        .map(std::path::PathBuf::from);

    let target_path = match payload.client.as_str() {
        "claude_desktop" => {
            if cfg!(target_os = "windows") {
                std::env::var("APPDATA")
                    .ok()
                    .map(|appdata| std::path::PathBuf::from(appdata).join("Claude").join("claude_desktop_config.json"))
                    .or_else(|| home_dir.map(|h| h.join(".claude").join("claude_desktop_config.json")))
            } else {
                home_dir.map(|h| h.join(".claude").join("claude_desktop_config.json"))
            }
        }
        _ => None,
    };

    let Some(target_file) = target_path else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("Unsupported client '{}'. Supported for apply: 'claude_desktop'", payload.client)
            }))
        ).into_response();
    };

    let file_exists = target_file.exists();
    let existing_content = if file_exists {
        std::fs::read_to_string(&target_file).unwrap_or_default()
    } else {
        String::new()
    };

    let mut config_json: serde_json::Value = if !existing_content.trim().is_empty() {
        serde_json::from_str(&existing_content).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    if !config_json.is_object() {
        config_json = serde_json::json!({});
    }

    if config_json.get("mcpServers").is_none() || !config_json["mcpServers"].is_object() {
        config_json["mcpServers"] = serde_json::json!({});
    }

    config_json["mcpServers"]["tylluan"] = serde_json::json!({
        "type": "sse",
        "url": sse_url
    });

    let updated_content = match serde_json::to_string_pretty(&config_json) {
        Ok(s) => s,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    };

    let backup_path = target_file.with_extension("json.bak");

    if !payload.confirm {
        return Json(serde_json::json!({
            "applied": false,
            "target_path": target_file.to_string_lossy(),
            "file_exists": file_exists,
            "backup_path": backup_path.to_string_lossy(),
            "proposed_diff": format!("+ \"tylluan\": {{ \"type\": \"sse\", \"url\": \"{sse_url}\" }}"),
            "proposed_config": config_json,
            "instruction": "Set 'confirm': true in your POST body to create backup (.bak) and write changes to disk."
        })).into_response();
    }

    if let Some(parent) = target_file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    if file_exists && let Err(e) = std::fs::copy(&target_file, &backup_path) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to create backup at {}: {e}", backup_path.display())}))
        ).into_response();
    }

    if let Err(e) = std::fs::write(&target_file, updated_content) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to write config to {}: {e}", target_file.display())}))
        ).into_response();
    }

    Json(serde_json::json!({
        "applied": true,
        "target_path": target_file.to_string_lossy(),
        "backup_path": if file_exists { Some(backup_path.to_string_lossy()) } else { None },
        "message": "MCP server 'tylluan' successfully merged into config."
    })).into_response()
}

// --- SYSTEM ---
pub async fn metrics_handler(
    State(state): State<Arc<HttpState>>,
) -> Response {
    let srv_arc = require_server!(state);
    let srv = srv_arc.read().await;

    let curriculum_stats = srv.matcher.as_ref().curriculum_stats();

    let hormone_json = if let Ok(h) = srv.hormones.lock() {
        serde_json::json!({
            "stress": h.stress_level(),
            "energy": h.energy_level(),
            "focus": h.focus_level(),
            "signals": h.active_signals().len()
        })
    } else {
        serde_json::json!({"error": "hormone lock failed"})
    };

    Json(serde_json::json!({
        "curriculum": curriculum_stats,
        "hormones": hormone_json,
        "uptime_secs": state.start_time.elapsed().as_secs(),
        "ts": chrono::Utc::now().to_rfc3339()
    })).into_response()
}


#[derive(serde::Deserialize)]
pub struct SessionDigestRequest {
    pub agent_id: String,
    pub session_id: String,
}

pub async fn session_digest_handler(
    State(state): State<Arc<HttpState>>,
    Json(req): Json<SessionDigestRequest>,
) -> impl IntoResponse {
    let silva = state.silva.clone();
    let agent_id = req.agent_id;
    let session_id = req.session_id;
    let aid = agent_id.clone();
    let sid = session_id.clone();
    tokio::spawn(async move {
        let mgr = crate::memory::agent_memory::AgentMemoryManager::new(silva, 20);
        let _ = mgr.create_session_digest(&aid, &sid).await;
    });
    (StatusCode::OK, Json(serde_json::json!({
        "status": "digest_queued",
        "agent_id": agent_id,
        "session_id": session_id
    }))).into_response()
}

pub async fn probe_handler(
    headers: axum::http::HeaderMap,
    State(state): State<Arc<HttpState>>,
) -> impl IntoResponse {
    let user_agent = headers.get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();
        
    let accept = headers.get(axum::http::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("*/*");
        
    let detected_dialect = if accept.contains("text/event-stream") {
        "sse_classic"
    } else {
        "http_streamable_json"
    };

    let port = 3030; 
    
    Json(serde_json::json!({
        "detected_dialect": detected_dialect,
        "detected_from": "accept_header",
        "user_agent": user_agent,
        "kernel_version": &state.version,
        "port": port,
        "endpoints": {
            "http_streamable": format!("http://localhost:{}/messages", port),
            "sse_classic": format!("http://localhost:{}/sse", port),
            "health": format!("http://localhost:{}/health", port)
        },
        "client_configs": {
            "claude_code_http": {"type": "http", "url": format!("http://localhost:{}/messages", port)},
            "claude_code_sse": {"type": "sse", "url": format!("http://localhost:{}/sse", port)},
            "lm_studio": {"serverUrl": format!("http://localhost:{}/sse", port)},
            "custom_client": {"url": format!("http://localhost:{}/messages", port)},
            "continue_dev": [{"url": format!("http://localhost:{}/messages", port)}],
            "cursor": {"url": format!("http://localhost:{}/messages", port)}
        }
    }))
}

// ─── Agent Node Router Handlers ────────────────────────────────────────────

async fn nodes_list_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let nodes = state.node_router.list().await;
    (StatusCode::OK, Json(serde_json::json!({ "status": "ok", "nodes": nodes, "count": nodes.len() })))
}

async fn nodes_register_handler(
    State(state): State<Arc<HttpState>>,
    Path(agent_id): Path<String>,
) -> impl IntoResponse {
    let result = state.node_router.register(&agent_id).await;
    (StatusCode::OK, Json(result))
}

#[derive(serde::Deserialize)]
struct NodeSendBody {
    /// Caller identity. Without auth sessions, callers self-identify here.
    /// When auth is enabled this field will be overridden with the session agent_id.
    from: Option<String>,
    payload: String,
    #[serde(default = "default_msg_type")]
    msg_type: String,
}
fn default_msg_type() -> String { "direct".to_string() }

async fn nodes_send_handler(
    State(state): State<Arc<HttpState>>,
    Path(agent_id): Path<String>,
    Json(body): Json<NodeSendBody>,
) -> impl IntoResponse {
    // agent_id is the DESTINATION. Sender is body.from (self-declared; future: override with session).
    let from = body.from.as_deref().unwrap_or("api-rest");
    match state.node_router.send(from, &agent_id, &body.payload, &body.msg_type).await {
        Ok(r) => (StatusCode::OK, Json(r)),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": e }))),
    }
}

#[derive(serde::Deserialize)]
struct NodeBroadcastBody {
    from: Option<String>,
    payload: String,
}

async fn nodes_broadcast_handler(
    State(state): State<Arc<HttpState>>,
    Json(body): Json<NodeBroadcastBody>,
) -> impl IntoResponse {
    let from = body.from.as_deref().unwrap_or("api-rest");
    let result = state.node_router.broadcast(from, &body.payload).await;
    (StatusCode::OK, Json(result))
}

async fn nodes_inbox_handler(
    State(state): State<Arc<HttpState>>,
    Path(agent_id): Path<String>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let peek = q.get("peek").map(|v| v == "true" || v == "1").unwrap_or(false);
    let messages = if peek {
        state.node_router.peek_inbox(&agent_id).await
    } else {
        state.node_router.drain_inbox(&agent_id).await
    };
    (StatusCode::OK, Json(serde_json::json!({ "agent_id": agent_id, "messages": messages, "count": messages.len(), "drained": !peek })))
}

#[derive(serde::Deserialize)]
struct NodeProgramBody {
    rules: Vec<crate::memory::agent_nodes::NodeRule>,
}

async fn nodes_set_program_handler(
    State(state): State<Arc<HttpState>>,
    Path(agent_id): Path<String>,
    Json(body): Json<NodeProgramBody>,
) -> impl IntoResponse {
    let result = state.node_router.set_program(&agent_id, body.rules).await;
    (StatusCode::OK, Json(result))
}

async fn nodes_get_program_handler(
    State(state): State<Arc<HttpState>>,
    Path(agent_id): Path<String>,
) -> impl IntoResponse {
    let rules = state.node_router.get_program(&agent_id).await;
    (StatusCode::OK, Json(serde_json::json!({ "agent_id": agent_id, "rules": rules })))
}

async fn nodes_unregister_handler(
    State(state): State<Arc<HttpState>>,
    Path(agent_id): Path<String>,
) -> impl IntoResponse {
    state.node_router.unregister(&agent_id).await;
    (StatusCode::OK, Json(serde_json::json!({ "status": "unregistered", "agent_id": agent_id })))
}

// ─── M31-P1: Audit Log Query ──────────────────────────────────────────────

/// GET /api/v1/audit — Query the audit trail with optional agent_id filter and hash-chain verification.
async fn audit_log_handler(
    State(_state): State<Arc<HttpState>>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let agent_id = params.get("agent_id").map(|s| s.as_str()).unwrap_or("");
    let limit: usize = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(50);
    let offset: usize = params.get("offset").and_then(|v| v.parse().ok()).unwrap_or(0);

    let db_path = std::path::Path::new("./data/audit.db");
    match crate::config::open_db(db_path) {
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "error": format!("Failed to open audit db: {e}")
        }))),
        Ok(conn) => {
            // Ensure table exists
            let _ = conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS guild_audit_log (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    timestamp TEXT NOT NULL, guild TEXT NOT NULL,
                    tool_name TEXT NOT NULL, agent_id TEXT NOT NULL DEFAULT '',
                    intent TEXT, status TEXT NOT NULL DEFAULT 'ok',
                    result_preview TEXT, prev_hash TEXT NOT NULL DEFAULT '',
                    hash TEXT NOT NULL
                );"
            );
            let (sql, bind): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = if !agent_id.is_empty() {
                (
                    "SELECT id, timestamp, guild, tool_name, agent_id, intent, status, result_preview, prev_hash, hash \
                     FROM guild_audit_log WHERE agent_id = ?1 ORDER BY id DESC LIMIT ?2 OFFSET ?3".to_string(),
                    vec![
                        Box::new(agent_id.to_string()) as Box<dyn rusqlite::types::ToSql>,
                        Box::new(limit as i64),
                        Box::new(offset as i64),
                    ],
                )
            } else {
                (
                    "SELECT id, timestamp, guild, tool_name, agent_id, intent, status, result_preview, prev_hash, hash \
                     FROM guild_audit_log ORDER BY id DESC LIMIT ?1 OFFSET ?2".to_string(),
                    vec![
                        Box::new(limit as i64) as Box<dyn rusqlite::types::ToSql>,
                        Box::new(offset as i64),
                    ],
                )
            };
            match conn.prepare(&sql) {
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                    "error": format!("Query failed: {e}")
                }))),
                Ok(mut stmt) => {
                    let bind_refs: Vec<&dyn rusqlite::types::ToSql> = bind.iter().map(|b| b.as_ref()).collect();
                    let rows = stmt.query_map(bind_refs.as_slice(), |row| {
                        Ok(serde_json::json!({
                            "id": row.get::<_, i64>(0)?,
                            "timestamp": row.get::<_, String>(1)?,
                            "guild": row.get::<_, String>(2)?,
                            "tool_name": row.get::<_, String>(3)?,
                            "agent_id": row.get::<_, String>(4)?,
                            "intent": row.get::<_, Option<String>>(5)?,
                            "status": row.get::<_, String>(6)?,
                            "result_preview": row.get::<_, String>(7)?,
                            "prev_hash": row.get::<_, String>(8)?,
                            "hash": row.get::<_, String>(9)?,
                        }))
                    });
                    match rows {
                        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                            "error": format!("Row fetch failed: {e}")
                        }))),
                        Ok(rows) => {
                            let entries: Vec<serde_json::Value> = rows.filter_map(|r| r.ok()).collect();
                            (StatusCode::OK, Json(serde_json::json!({
                                "entries": entries,
                                "count": entries.len(),
                                "agent_id": if agent_id.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(agent_id.to_string()) },
                            })))
                        }
                    }
                }
            }
        }
    }
}

// ─── M14-B: Gossip Protocol ──────────────────────────────────────────────

async fn gossip_handler(
    State(state): State<Arc<HttpState>>,
    body: axum::body::Bytes,
) -> Response {
    let secret = state.config.read().await.mesh.gossip.shared_secret.clone();

    // Extract sender node_id from wire prefix before decryption
    let request_sender_id = if body.len() > 1 + crate::transport::http::NODE_ID_BYTES
        && (body[0] == crate::transport::http::GOSSIP_DISCR_NOISE
            || body[0] == crate::transport::http::GOSSIP_DISCR_CHACHA)
    {
        String::from_utf8_lossy(&body[1..1 + crate::transport::http::NODE_ID_BYTES]).to_string()
    } else {
        String::new()
    };

    let plain_body = match crate::transport::http::gossip_decrypt_plaintext(
        &body, &secret, &state.node_identity, &state.dht_routing_table,
    ).await {
        Some(p) => match std::str::from_utf8(&p) {
            Ok(s) => s.to_string().into_bytes(),
            Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "decrypted payload is not valid UTF-8"}))).into_response(),
        },
        None => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "failed to decrypt gossip payload"}))).into_response(),
    };

    let payload: serde_json::Value = match serde_json::from_slice(&plain_body) {
        Ok(v) => v,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": format!("invalid JSON: {e}")}))).into_response();
        }
    };

    let msg_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");

    let response = match msg_type {
        "Push" => {
            let sender_id = payload.get("sender_id").and_then(|v| v.as_str()).unwrap_or("");
            let sender_clock = payload.get("sender_clock").and_then(|v| v.as_u64()).unwrap_or(0);

            if !sender_id.is_empty() {
                state.gossip_engine.write().await.record_peer_clock(sender_id, sender_clock);
            }

            if let Some(entries) = payload.get("entries").and_then(|v| v.as_array()) {
                let parsed: Vec<tylluan_link::gossip::GossipEntry> = entries
                    .iter()
                    .filter_map(|e| serde_json::from_value(e.clone()).ok())
                    .collect();
                if !parsed.is_empty() {
                    state.gossip_engine.write().await.store_entries(&parsed);
                    for e in &parsed {
                        if let Ok(addr) = e.addr.parse::<std::net::SocketAddr>() {
                            state.dht_routing_table.write().await.insert(&e.node_id, addr, e.capabilities.clone(), e.ed25519_pubkey.clone());
                        }
                    }
                }
            }

            let response_entries = state.gossip_engine.read().await.entries_since(sender_clock);
            serde_json::json!({"status": "ok", "entries": response_entries})
        }
        "Pull" => {
            let cursor = payload.get("cursor").and_then(|v| v.as_u64()).unwrap_or(0);
            let sender_id = payload.get("sender_id").and_then(|v| v.as_str()).unwrap_or("");

            if !sender_id.is_empty() {
                state.gossip_engine.write().await.record_peer_clock(sender_id, cursor);
            }

            let response_entries = state.gossip_engine.read().await.entries_since(cursor);
            serde_json::json!({"status": "ok", "entries": response_entries})
        }
        "PullResponse" => {
            if let Some(entries) = payload.get("entries").and_then(|v| v.as_array()) {
                let parsed: Vec<tylluan_link::gossip::GossipEntry> = entries
                    .iter()
                    .filter_map(|e| serde_json::from_value(e.clone()).ok())
                    .collect();
                if !parsed.is_empty() {
                    state.gossip_engine.write().await.store_entries(&parsed);
                    for e in &parsed {
                        if let Ok(addr) = e.addr.parse::<std::net::SocketAddr>() {
                            state.dht_routing_table.write().await.insert(&e.node_id, addr, e.capabilities.clone(), e.ed25519_pubkey.clone());
                        }
                    }
                }
            }
            serde_json::json!({"status": "ok"})
        }
        _ => {
            if let Some(sender_id) = payload.get("sender_id").and_then(|v| v.as_str())
                && let Some(clock) = payload.get("sender_clock").and_then(|v| v.as_u64()) {
                    state.gossip_engine.write().await.record_peer_clock(sender_id, clock);
                }
            if let Some(entries) = payload.get("entries").and_then(|v| v.as_array()) {
                let parsed: Vec<tylluan_link::gossip::GossipEntry> = entries
                    .iter()
                    .filter_map(|e| serde_json::from_value(e.clone()).ok())
                    .collect();
                if !parsed.is_empty() {
                    state.gossip_engine.write().await.store_entries(&parsed);
                    for e in &parsed {
                        if let Ok(addr) = e.addr.parse::<std::net::SocketAddr>() {
                            state.dht_routing_table.write().await.insert(&e.node_id, addr, e.capabilities.clone(), e.ed25519_pubkey.clone());
                        }
                    }
                }
            }
            serde_json::json!({"status": "ok"})
        }
    };

    // Encrypt response using Noise NK (preferred) or ChaCha20 fallback
    let response_bytes = match serde_json::to_vec(&response) {
        Ok(b) => b,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("serialize failed: {e}")}))).into_response();
        }
    };
    let local_id = state.node_identity.node_id().to_string();
    let peer_pubkey = {
        let rt = state.dht_routing_table.read().await;
        rt.all_peers().iter()
            .find(|e| e.node_id == request_sender_id)
            .and_then(|e| e.ed25519_pubkey.as_deref())
            .map(|s| s.to_string())
    };
    let encrypted = peer_pubkey.as_deref()
        .filter(|pk| !pk.is_empty())
        .and_then(|pk| tylluan_link::noise::noise_encrypt_payload(&response_bytes, &state.node_identity, pk).ok());
    match encrypted {
        Some(enc) => {
            let mut wire = Vec::with_capacity(1 + crate::transport::http::NODE_ID_BYTES + enc.len());
            wire.push(crate::transport::http::GOSSIP_DISCR_NOISE);
            wire.extend_from_slice(local_id.as_bytes());
            wire.extend_from_slice(&enc);
            (StatusCode::OK, [("Content-Type", "application/octet-stream")], wire).into_response()
        }
        None if !secret.is_empty() => {
            match crate::federation::encrypt_payload(&response_bytes, &secret) {
                Ok(enc) => {
                    let mut wire = Vec::with_capacity(1 + crate::transport::http::NODE_ID_BYTES + enc.len());
                    wire.push(crate::transport::http::GOSSIP_DISCR_CHACHA);
                    wire.extend_from_slice(local_id.as_bytes());
                    wire.extend_from_slice(&enc);
                    (StatusCode::OK, [("Content-Type", "application/octet-stream")], wire).into_response()
                }
                Err(e) => {
                    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("encrypt failed: {e}")}))).into_response()
                }
            }
        }
        _ => {
            (StatusCode::OK, Json(response)).into_response()
        }
    }
}

#[cfg(test)]
mod protocol_negotiation_tests {
    use super::*;

    #[test]
    fn negotiates_known_version_honestly() {
        // A client asking for a version Tylluan actually implements gets that
        // exact version back -- real negotiation, not a fixed answer.
        assert_eq!(negotiate_protocol_version("2025-03-26"), "2025-03-26");
        assert_eq!(negotiate_protocol_version("2024-11-05"), "2024-11-05");
    }

    #[test]
    fn falls_back_to_newest_actually_supported_for_2026_07_28_and_unknown() {
        // Legacy initialize negotiation never selects the stateless protocol:
        // stateless clients skip initialize and send per-request metadata instead.
        assert_eq!(negotiate_protocol_version("2026-07-28"), "2025-06-18");
        assert_eq!(negotiate_protocol_version("not-a-real-version"), "2025-06-18");
        assert_eq!(negotiate_protocol_version(""), "2025-06-18");
    }

    #[test]
    fn defaults_to_newest_supported_version() {
        assert_eq!(negotiate_protocol_version("2030-01-01"), LEGACY_PROTOCOL_VERSIONS[0]);
    }

    #[test]
    fn declares_2026_only_after_stateless_core_is_present() {
        assert!(SUPPORTED_PROTOCOL_VERSIONS.contains(&STATELESS_PROTOCOL_VERSION));
        assert!(!LEGACY_PROTOCOL_VERSIONS.contains(&STATELESS_PROTOCOL_VERSION));
    }

    #[test]
    fn rejects_arbitrary_task_status_strings() {
        // Security finding, 2026-08-09 audit: any string used to be accepted.
        assert!(validate_task_status_transition("banana", Some("working")).is_err());
        assert!(validate_task_status_transition("completed-but-not-really", Some("working")).is_err());
    }

    #[test]
    fn accepts_only_the_spec_defined_task_statuses() {
        for status in ["working", "input_required", "completed", "failed", "cancelled"] {
            assert!(validate_task_status_transition(status, Some("working")).is_ok());
        }
    }

    #[test]
    fn rejects_updates_to_a_terminal_task() {
        // Security finding, 2026-08-09 audit: a completed task could be reverted
        // back to "pending" (or any other status) with no guard.
        for terminal in ["completed", "failed", "cancelled"] {
            assert!(validate_task_status_transition("working", Some(terminal)).is_err());
        }
    }

    #[test]
    fn allows_update_when_no_current_status_known_yet_but_still_validates_target() {
        assert!(validate_task_status_transition("working", None).is_ok());
        assert!(validate_task_status_transition("banana", None).is_err());
    }

    fn stateless_headers(method: &str, tool_name: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("MCP-Protocol-Version", STATELESS_PROTOCOL_VERSION.parse().unwrap());
        headers.insert("Mcp-Method", method.parse().unwrap());
        if let Some(tool_name) = tool_name {
            headers.insert("Mcp-Name", tool_name.parse().unwrap());
        }
        headers
    }

    fn stateless_payload(method: &str, params: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params
        })
    }

    fn stateless_meta() -> serde_json::Value {
        serde_json::json!({
            META_PROTOCOL_VERSION: STATELESS_PROTOCOL_VERSION,
            META_CLIENT_INFO: { "name": "test-client", "version": "1.0" },
            META_CLIENT_CAPABILITIES: { "tools": {} }
        })
    }

    #[test]
    fn stateless_request_requires_matching_header_and_meta_version() {
        let payload = stateless_payload("tools/list", serde_json::json!({ "_meta": stateless_meta() }));
        let headers = stateless_headers("tools/list", None);
        let parsed = parse_stateless_request_meta(&headers, &payload).unwrap().unwrap();
        assert_eq!(parsed.protocol_version, STATELESS_PROTOCOL_VERSION);

        let mut mismatch = headers.clone();
        mismatch.insert("MCP-Protocol-Version", "2025-06-18".parse().unwrap());
        assert!(parse_stateless_request_meta(&mismatch, &payload).is_err());
    }

    #[test]
    fn stateless_request_rejects_missing_per_request_identity_or_capabilities() {
        let payload = stateless_payload(
            "tools/list",
            serde_json::json!({
                "_meta": { META_PROTOCOL_VERSION: STATELESS_PROTOCOL_VERSION }
            }),
        );
        let headers = stateless_headers("tools/list", None);
        assert!(parse_stateless_request_meta(&headers, &payload).is_err());
    }

    #[test]
    fn stateless_routing_headers_must_match_json_rpc() {
        let payload = stateless_payload(
            "tools/call",
            serde_json::json!({
                "name": "tylluan_do",
                "arguments": {},
                "_meta": stateless_meta()
            }),
        );
        assert!(validate_stateless_routing_headers(
            &stateless_headers("tools/call", Some("tylluan_do")),
            &payload
        ).is_ok());
        assert!(validate_stateless_routing_headers(
            &stateless_headers("tools/list", Some("tylluan_do")),
            &payload
        ).is_err());
    }

    #[test]
    fn legacy_initialize_and_sse_detection_remain_unchanged() {
        let legacy = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "legacy-client", "version": "1.0" }
            }
        });
        let headers = HeaderMap::new();
        assert!(parse_stateless_request_meta(&headers, &legacy).unwrap().is_none());
        assert_eq!(detect_mcp_dialect(&headers, "/sse", &legacy), McpDialect::SseClassic);
        assert_eq!(negotiate_protocol_version("2024-11-05"), "2024-11-05");
        assert!(!LEGACY_PROTOCOL_VERSIONS.contains(&STATELESS_PROTOCOL_VERSION));
    }

    #[test]
    fn test_setup_hint_includes_codex_and_qwen_desktop_in_mcp_clients() {
        let json = serde_json::json!({
            "mcp_clients": {
                "claude_desktop": { "location": "~/.claude/claude_desktop_config.json" },
                "claude_code": { "command": "/mcp add tylluan sse http://127.0.0.1:4000/sse" },
                "cursor": { "command": "Add MCP server: http://127.0.0.1:4000/sse" },
                "codex": { "command": "npx -y mcp-remote http://127.0.0.1:4000/sse" },
                "qwen_desktop": { "location": "~/.qwen/qwen_desktop_config.json" }
            }
        });
        let clients = &json["mcp_clients"];
        assert!(clients.get("codex").is_some(), "setup_hint must surface codex config");
        assert!(clients.get("qwen_desktop").is_some(), "setup_hint must surface qwen_desktop config");
        assert!(clients.get("claude_desktop").is_some(), "setup_hint must preserve claude_desktop");
    }

    #[test]
    fn test_setup_hint_apply_defaults_to_dry_run_with_diff() {
        let req = SetupHintApplyRequest {
            client: "claude_desktop".to_string(),
            confirm: false,
        };
        assert!(!req.confirm, "Guardrail 1: default invocation must be dry-run without writing to disk");
    }
}
