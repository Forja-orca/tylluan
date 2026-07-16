//! M31-P6: Background job execution for long-running guild calls.
//!
//! Two prefix handlers for tylluan_do:
//!   `@bg:<intent>` — enqueue a guild call as a background job, return job ID immediately
//!   `@job:<id>`    — check status of a previously submitted background job
//!
//! A background worker (spawned from main.rs) polls the JobQueue, pre-resolves guild+tool+args
//! from the original intent context, executes the guild call, stores results in SilvaDB as
//! `job_result` nodes, and emits `guild_job_complete` SSE events.
//!
//! Architecture:
//!   User → @bg:analyze codebase → handler_do → resolve guild+tool → enqueue JobQueue → return job ID
//!   Worker loop → claim_next → guild.call_tool_readonly() → store in SilvaDB → notify SSE

use rmcp::{Error as McpError, model::*};
use rmcp::model::CallToolRequestParam;
use std::sync::Arc;

use super::TylluanServer;
use crate::memory::jobs::JobQueue;
use crate::registry::guild_process::GuildRegistry;
use crate::registry::proxy::error_result;

/// Task type constant for the JobQueue.
pub const BG_TASK_TYPE: &str = "bg_guild_call";

/// State passed to the background worker for making guild calls.
/// Holds all the Arc references the worker needs without requiring a full TylluanServer.
#[derive(Clone)]
pub struct BackgroundWorkerState {
    pub registry: Arc<tokio::sync::RwLock<GuildRegistry>>,
    pub silva: Arc<crate::memory::silva::SilvaDB>,
    pub jobs: Arc<JobQueue>,
    pub notifier: Option<tokio::sync::broadcast::Sender<serde_json::Value>>,
}

impl BackgroundWorkerState {
    pub fn from_server(server: &TylluanServer) -> Option<Self> {
        let jobs = server.jobs.as_ref()?.clone();
        Some(Self {
            registry: server.registry.clone(),
            silva: server.silva.clone(),
            jobs,
            notifier: server.notifier.clone(),
        })
    }
}

/// Handle `@bg:<intent>` — enqueue a guild call as a background job.
///
/// Resolves guild+tool right here (like handle_tylluan_do does), stores
/// the pre-resolved params in the job payload so the background worker
/// doesn't need the matcher.
pub async fn handle_bg_prefix(
    server: &TylluanServer,
    intent: &str,
    agent_id: &Option<String>,
) -> Option<Result<CallToolResult, McpError>> {
    let trimmed = intent.trim();
    if !(trimmed.starts_with("@bg:") || trimmed.starts_with("@background:")) {
        return None;
    }

    let actual_intent = trimmed.split_once(':')
        .map(|(_, rest)| rest.trim())
        .unwrap_or("")
        .to_string();

    if actual_intent.is_empty() {
        return Some(Ok(error_result(
            "@bg:<intent> requires a non-empty intent. Usage: @bg:analyze the system"
        )));
    }

    let jobs = match &server.jobs {
        Some(j) => j.clone(),
        None => return Some(Ok(error_result(
            "Background jobs are not available (JobQueue not initialized)."
        ))),
    };

    // Pre-resolve guild name right here (same logic as handle_tylluan_do)
    let routing_intent = crate::transport::server::intent_enhancer::strip_ctx_prefix(&actual_intent);
    let guild_name = match crate::transport::server::handler_do::routing::resolve_guild_name(
        server, routing_intent, None, agent_id.as_deref()
    ).await {
        Ok((name, _trace)) => name,
        Err(e) => {
            let err_text = e.content.iter()
                .filter_map(|c| c.as_text())
                .map(|t| t.text.clone())
                .collect::<String>();
            return Some(Ok(error_result(&format!(
                "Failed to resolve guild for '@bg:{actual_intent}': {err_text}"
            ))));
        }
    };

    // Build tool_name (same pattern as handle_tylluan_do)
    let tool_name = {
        let reg = server.registry.read().await;
        reg.guilds.get(&guild_name).and_then(|guild| {
            use crate::router::matcher::{tokenize, keyword_score};
            let tokens = tokenize(&actual_intent);
            guild.tools.iter()
                .max_by(|a, b| {
                    let sa = keyword_score(&tokens, a.description.as_ref(), a.name.as_ref());
                    let sb = keyword_score(&tokens, b.description.as_ref(), b.name.as_ref());
                    sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|t| t.name.to_string())
        }).unwrap_or_default()
    };

    let tool_args = serde_json::json!({
        "command": actual_intent,
        "intent": actual_intent,
        "query": actual_intent,
        "text": actual_intent,
        "content": actual_intent,
        "prompt": actual_intent,
        "message": actual_intent,
        "input": actual_intent,
        "timeout_secs": 3600,
    });

    let payload = serde_json::json!({
        "intent": actual_intent,
        "agent_id": agent_id.as_deref().unwrap_or("anonymous"),
        "guild_name": guild_name,
        "tool_name": tool_name,
        "tool_args": tool_args,
    });

    let job_id = match jobs.enqueue(BG_TASK_TYPE, &payload) {
        Ok(id) => id,
        Err(e) => return Some(Ok(error_result(&format!("Failed to enqueue job: {e}")))),
    };

    let short_id = job_id.split(':').next_back().unwrap_or(&job_id).to_string();

    // Emit notification that a background job was queued
    server.notify("guild_job_queued", serde_json::json!({
        "job_id": job_id,
        "guild": guild_name,
        "intent": actual_intent,
        "agent_id": agent_id.as_deref().unwrap_or("anonymous"),
        "ts": chrono::Utc::now().timestamp_millis()
    }));

    Some(Ok(CallToolResult {
        content: vec![Content::text(format!(
            "⏳ Background job started: {job_id}\n\
             Guild: {guild_name}\n\
             Intent: {actual_intent}\n\
             Check status: tylluan_do(intent='@job:{short_id}')"
        ))],
        is_error: Some(false),
    }))
}

/// Handle `@job:<id>` — check status of a background job.
///
/// Looks up the job in SilvaDB (completed results) or returns pending status.
pub async fn handle_job_status(
    server: &TylluanServer,
    intent: &str,
) -> Option<Result<CallToolResult, McpError>> {
    let trimmed = intent.trim();
    if !trimmed.starts_with("@job:") && !trimmed.starts_with("job:") {
        return None;
    }

    let query = trimmed.split_once(':')
        .map(|(_, rest)| rest.trim())
        .unwrap_or("")
        .to_string();

    if query.is_empty() {
        return Some(Ok(error_result(
            "@job:<id> requires a job ID. Usage: @job:<uuid_or_short_id>"
        )));
    }

    // Try SilvaDB for completed job results (full job ID)
    let full_id = format!("job:{query}");
    if let Ok(Some(node)) = server.silva.get_node(&full_id).await {
        return Some(Ok(CallToolResult {
            content: vec![Content::text(format!(
                "Job '{}' completed:\n{}",
                query, node.content
            ))],
            is_error: Some(false),
        }));
    }

    // Try with bg_guild_call prefix (job:bg_guild_call:<short_id>)
    let alt_id = format!("job:bg_guild_call:{query}");
    if let Ok(Some(node)) = server.silva.get_node(&alt_id).await {
        return Some(Ok(CallToolResult {
            content: vec![Content::text(format!(
                "Job '{}' completed:\n{}",
                query, node.content
            ))],
            is_error: Some(false),
        }));
    }

    // Not in SilvaDB — still pending or not found
    Some(Ok(CallToolResult {
        content: vec![Content::text(format!(
            "Job '{query}' has not completed yet. It may still be running or the ID is invalid."
        ))],
        is_error: Some(false),
    }))
}

/// Spawn the background worker that processes queued guild calls.
///
/// Runs as a tokio task, polled every 2 seconds. Processes one job at a time.
/// Stores completed results in SilvaDB and emits `guild_job_complete` SSE events.
pub fn spawn_background_worker(state: BackgroundWorkerState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
        let _ = state.jobs.resume_pending();
        tracing::info!("Background worker started for task type: {BG_TASK_TYPE}");
        loop {
            interval.tick().await;
            let job = match state.jobs.claim_next(BG_TASK_TYPE) {
                Ok(Some(j)) => j,
                Ok(None) => continue,
                Err(e) => {
                    tracing::warn!("Background worker: claim_next failed: {e}");
                    continue;
                }
            };

            tracing::info!("Background worker processing job: {}", job.id);

            let payload: serde_json::Value = match serde_json::from_str(&job.payload) {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!("Background worker: invalid payload for job {}: {e}", job.id);
                    let _ = state.jobs.mark_failed(&job.id);
                    continue;
                }
            };

            let job_intent = payload.get("intent").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let guild_name = payload.get("guild_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let tool_name = payload.get("tool_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let tool_args = payload.get("tool_args").and_then(|v| v.as_object()).cloned().unwrap_or_default();
            let agent_id = payload.get("agent_id").and_then(|v| v.as_str()).map(|s| s.to_string());

            let short_id = job.id.split(':').next_back().unwrap_or(&job.id).to_string();
            let t0 = std::time::Instant::now();

            if guild_name.is_empty() || tool_name.is_empty() {
                let err_content = format!(
                    "Job {short_id} failed: could not resolve guild or tool for intent '{job_intent}'."
                );
                let meta = serde_json::json!({
                    "job_id": job.id, "intent": job_intent, "status": "failed",
                    "error": "no_guild_or_tool", "completed_at": chrono::Utc::now().to_rfc3339(),
                });
                let _ = state.silva.upsert_node(
                    &format!("job:bg_guild_call:{short_id}"), "job_result", &err_content, &meta.to_string(),
                ).await;
                let _ = state.jobs.mark_failed(&job.id);
                emit_job_complete(&state.notifier, &job.id, &guild_name, "failed", &err_content);
                continue;
            }

            let call_params = CallToolRequestParam {
                name: tool_name.clone().into(),
                arguments: Some(tool_args),
            };

            let effective_timeout = crate::transport::server::handler_do::guild_effective_timeout(&guild_name, false);
            let call_timeout_ms = effective_timeout + 10_000;

            let result: CallToolResult = match tokio::time::timeout(
                std::time::Duration::from_millis(call_timeout_ms),
                async {
                    let reg = state.registry.read().await;
                    if let Some(guild) = reg.guilds.get(&guild_name) {
                        guild.call_tool_readonly(call_params).await
                    } else {
                        CallToolResult {
                            content: vec![Content::text(
                                format!("Guild '{guild_name}' not found or not running.")
                            )],
                            is_error: Some(true),
                        }
                    }
                }
            ).await {
                Ok(res) => res,
                Err(_) => CallToolResult {
                    content: vec![Content::text(format!(
                        "Guild '{guild_name}' timed out after {call_timeout_ms}ms."
                    ))],
                    is_error: Some(true),
                },
            };

            let latency_ms = t0.elapsed().as_millis() as u64;
            let is_success = result.is_error != Some(true);

            let result_text = result.content.iter()
                .filter_map(|c| c.as_text())
                .map(|t| t.text.clone())
                .collect::<Vec<_>>()
                .join("\n");

            let status_str = if is_success { "completed" } else { "failed" };
            let aid_display = agent_id.as_deref().unwrap_or("anonymous");
            let content = format!(
                "Job {short_id} ({status_str})\n\
                 Guild: {guild_name}\n\
                 Tool: {tool_name}\n\
                 Intent: {job_intent}\n\
                 Agent: {aid_display}\n\
                 Latency: {latency_ms}ms\n\n{result_text}"
            );

            let meta = serde_json::json!({
                "job_id": job.id,
                "intent": job_intent,
                "guild": guild_name,
                "tool": tool_name,
                "agent_id": agent_id,
                "status": status_str,
                "latency_ms": latency_ms,
                "completed_at": chrono::Utc::now().to_rfc3339(),
            });

            let _ = state.silva.upsert_node(
                &format!("job:bg_guild_call:{short_id}"),
                "job_result",
                &content,
                &meta.to_string(),
            ).await;

            if is_success {
                let _ = state.jobs.mark_done(&job.id);
            } else {
                let _ = state.jobs.mark_failed(&job.id);
            }

            emit_job_complete(&state.notifier, &job.id, &guild_name, status_str, &result_text);
            tracing::info!("Background worker {status_str} job {} ({latency_ms}ms)", job.id);
        }
    });
}

fn emit_job_complete(
    notifier: &Option<tokio::sync::broadcast::Sender<serde_json::Value>>,
    job_id: &str,
    guild: &str,
    status: &str,
    summary: &str,
) {
    if let Some(tx) = notifier {
        let _ = tx.send(serde_json::json!({
            "jsonrpc": "2.0",
            "method": "guild_job_complete",
            "params": {
                "job_id": job_id,
                "guild": guild,
                "status": status,
                "summary": summary.chars().take(200).collect::<String>(),
                "ts": chrono::Utc::now().timestamp_millis()
            }
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::silva::SilvaDB;
    use std::path::Path;

    async fn test_server() -> TylluanServer {
        let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
        let jobs = Arc::new(JobQueue::open(Path::new(":memory:")).unwrap());
        let mut server = crate::transport::server::handler_do::base_test_server(silva.clone()).await;
        server.set_jobs(jobs);
        server
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_bg_prefix_enqueues_job() {
        let server = test_server().await;
        let result = handle_bg_prefix(&server, "@bg:analyze the system", &None).await;
        assert!(result.is_some());
        let r = result.unwrap().unwrap();
        assert_eq!(r.is_error, Some(false));
        let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(text.contains("job:"), "response should mention job ID");
        assert!(text.contains("analyze the system"));

        let jobs = server.jobs.as_ref().unwrap();
        let count = jobs.pending_count().unwrap();
        assert_eq!(count, 1, "should have 1 pending job");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_bg_empty_intent_returns_error() {
        let server = test_server().await;
        let result = handle_bg_prefix(&server, "@bg:", &None).await;
        assert!(result.is_some());
        let r = result.unwrap().unwrap();
        assert_eq!(r.is_error, Some(true));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_bg_non_matching_returns_none() {
        let server = test_server().await;
        let result = handle_bg_prefix(&server, "list files", &None).await;
        assert!(result.is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_job_status_nonexistent() {
        let server = test_server().await;
        let result = handle_job_status(&server, "@job:nonexistent").await;
        assert!(result.is_some());
        let r = result.unwrap().unwrap();
        let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(!text.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_job_status_completed_in_silva() {
        let server = test_server().await;
        let meta = serde_json::json!({
            "job_id": "job:bg_guild_call:test123",
            "status": "completed"
        }).to_string();
        server.silva.upsert_node("job:bg_guild_call:test123", "job_result", "Analysis complete.", &meta).await.unwrap();

        let result = handle_job_status(&server, "@job:bg_guild_call:test123").await;
        assert!(result.is_some());
        let r = result.unwrap().unwrap();
        assert_eq!(r.is_error, Some(false));
        let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(text.contains("Analysis complete"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_job_status_completed_with_short_id() {
        let server = test_server().await;
        let meta = serde_json::json!({
            "job_id": "job:bg_guild_call:abc123",
            "status": "completed"
        }).to_string();
        server.silva.upsert_node("job:bg_guild_call:abc123", "job_result", "Short ID lookup works.", &meta).await.unwrap();

        let result = handle_job_status(&server, "@job:abc123").await;
        assert!(result.is_some());
        let r = result.unwrap().unwrap();
        assert_eq!(r.is_error, Some(false));
        let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(text.contains("Short ID lookup works"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_job_status_non_matching_returns_none() {
        let server = test_server().await;
        let result = handle_job_status(&server, "list files").await;
        assert!(result.is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_background_worker_state_from_server() {
        let server = test_server().await;
        let state = BackgroundWorkerState::from_server(&server);
        assert!(state.is_some());
    }
}
