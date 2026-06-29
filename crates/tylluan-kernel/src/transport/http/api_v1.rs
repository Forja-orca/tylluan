use axum::{
    Router, Json,
    extract::{State, Query, Path},
    http::{StatusCode, HeaderMap, header::ACCEPT},
    response::{IntoResponse, Response},
    routing::{get, post, delete, any},
};
use std::sync::Arc;
use std::collections::HashMap;
use uuid;

use rmcp::model::CallToolRequestParam;

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
pub mod api_ops;
pub mod api_canvas;
pub mod api_journal;
pub mod api_agents;
pub mod api_contracts;
pub mod api_mesh;

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

/// MCP Dialect detected from client request heuristics
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpDialect {
    /// Modern HTTP Streamable JSON (LM Studio, Cursor, etc.)
    HttpStreamableJson,
    /// HTTP Streamable with SSE responses (Claude Code type:http)
    HttpStreamableSse,
    /// Classic SSE-based MCP (Claude Desktop, Cline, older clients)
    SseClassic,
}

/// Detect MCP dialect using 5-step heuristic (first match wins)
fn detect_mcp_dialect(
    headers: &HeaderMap,
    path: &str,
    body: &serde_json::Value,
) -> McpDialect {
    // Step 1: User-Agent header (most reliable)
    if let Some(ua) = headers.get("user-agent").and_then(|v| v.to_str().ok()) {
        let ua_lower = ua.to_lowercase();
        if ua_lower.contains("claude-code") || ua_lower.contains("anthropic") {
            // Claude Code - check Accept for sub-type
            if let Some(accept) = headers.get(ACCEPT).and_then(|v| v.to_str().ok())
                && accept.contains("text/event-stream") {
                    return McpDialect::HttpStreamableSse;
                }
            return McpDialect::HttpStreamableSse;
        }
        if ua_lower.contains("vscode") {
            return McpDialect::HttpStreamableJson;
        }
    }

    // Step 2: Accept header
    if let Some(accept) = headers.get(ACCEPT).and_then(|v| v.to_str().ok()) {
        let accept_lower = accept.to_lowercase();
        let has_event_stream = accept_lower.contains("text/event-stream");
        let has_json = accept_lower.contains("application/json");

        if has_event_stream && !has_json {
            // Only event-stream, no JSON ??? SSE Classic
            return McpDialect::SseClassic;
        }
        if has_event_stream && has_json {
            // Both types present (most modern clients send both)
            // Default to JSON ??? only Claude Code explicitly needs SSE encoding
            // and Claude Code is caught by User-Agent in Step 1
            return McpDialect::HttpStreamableJson;
        }
        if has_json && !has_event_stream {
            return McpDialect::HttpStreamableJson;
        }
    }

    // Step 3: Path
    let path_lower = path.to_lowercase();
    if path_lower.contains("/sse") {
        return McpDialect::SseClassic;
    }
    if path_lower.contains("/messages") || path_lower.contains("/api/v1/mcp") || path_lower.contains("/mcp") {
        return McpDialect::HttpStreamableJson;
    }

    // Step 4: protocolVersion in initialize body
    if let Some(version) = body
        .get("params")
        .and_then(|p| p.get("protocolVersion"))
        .and_then(|v| v.as_str())
    {
        match version {
            "2024-11-05" => return McpDialect::SseClassic,
            "2025-03-26" | "2025-06-18" => return McpDialect::HttpStreamableJson,
            _ => {}
        }
    }

    // Step 5: clientInfo.name in initialize body
    if let Some(client_name) = body
        .get("params")
        .and_then(|p| p.get("clientInfo"))
        .and_then(|c| c.get("name"))
        .and_then(|v| v.as_str())
    {
        let name_lower = client_name.to_lowercase();
        if name_lower.contains("gemini") || name_lower.contains("google") {
            return McpDialect::HttpStreamableJson;
        }
        if name_lower.contains("lm studio") {
            return McpDialect::HttpStreamableJson;
        }
        if name_lower.contains("claude") || name_lower.contains("cursor") {
            return McpDialect::HttpStreamableSse;
        }
    }

    // Fallback to modern JSON streamable
    McpDialect::HttpStreamableJson
}


// @CONTRACT: HTTP-API-V1 (CONTRACT-05)
// No eliminar rutas existentes ??? el dashboard depende de sus firmas
// Add new routes at the end of the block, never reorder
// See CONTRACTS.md section CONTRACT-05 for stable route list
pub fn api_v1_routes() -> Router<Arc<HttpState>> {
    Router::new()
        .route("/api/v1/do", post(do_intent_handler))
        .route("/api/v1/guilds", get(guilds_list_handler))
        .route("/api/v1/guilds/health", get(guilds_health_handler))
        .route("/api/v1/guilds/register", post(guild_register_handler))
        .route("/api/v1/guilds/request", post(guild_request_handler))
        .route("/api/v1/guilds/{name}/start", post(guild_start_handler))
        .route("/api/v1/guilds/{name}/stop", post(guild_stop_handler))
        .route("/api/v1/guilds/{name}/reset-backoff", post(guild_reset_backoff_handler))
        .route("/api/v1/guilds/{guild_name}/tools/{tool_name}", post(guild_tool_call_handler))
        .route("/api/v1/silva/stats", get(silva_stats_handler))
        .route("/api/v1/silva/recent", get(silva_recent_handler))
        .route("/api/v1/silva/edge", post(silva_add_edge_handler))
        .route("/api/v1/silva/node", post(silva_create_node_handler))
        .route("/api/v1/silva/graph", get(silva_graph_handler))
        .route("/api/v1/silva/export", get(knowledge_export_handler))
        .route("/api/v1/silva/save-cluster-summary", post(silva_save_summary_handler))
        .route("/api/v1/silva/analyze", any(silva_analyze_handler))
        .route("/api/v1/silva/communities", post(silva_communities_handler))
        .route("/api/v1/silva/shared/{agent_a}/{agent_b}", get(silva_shared_knowledge_handler))
        .route("/api/v1/silva/consolidate", post(silva_consolidate_handler))
        .route("/api/v1/silva/graphrag-trigger", post(graphrag_trigger_handler))
        .route("/api/v1/silva/contradictions", get(list_contradictions_handler))
        .route("/api/v1/dream/status", get(dream_status_handler))
        .route("/api/v1/silva/nodes/{node_id}", delete(silva_delete_node_handler))
        .route("/api/v1/sessions", get(list_sessions_handler))
        .route("/api/v1/sessions/{session_id}", get(session_detail_handler).delete(revoke_session_handler))
        .route("/api/v1/system/sessions", get(list_sessions_handler))
        .route("/api/v1/mailbox", get(mailbox_list_handler))
        .route("/api/v1/interoception", get(interoception_handler))
        .route("/api/v1/graph/viz", get(silva_graph_handler))

        .route("/api/v1/ingest/upload", post(ingest_upload_handler))
        .route("/api/v1/ingest/files/{filename}", get(serve_ingested_file_handler))
        .route("/api/v1/docker/ps", get(docker_containers_handler))
        .route("/api/v1/docker/containers", get(docker_containers_handler))
        .route("/api/v1/system/status", get(system_status_handler))
        .route("/api/v1/memory/reindex", post(reindex_handler))
        .route("/api/v1/memory/write", post(memory_write_handler))
        .route("/api/v1/memory/search", any(memory_search_handler))
        .route("/api/v1/tools", get(tools_list_handler))
        .route("/api/v1/capabilities", get(capabilities_handler))
        .route("/api/v1/audit/logs", get(audit_logs_handler))
        .route("/api/v1/config", get(get_config_handler).post(save_config_handler))
        .route("/api/v1/config/device", post(set_inference_device_handler))
        .route("/api/v1/models", get(models_handler))
        .route("/api/v1/bash", post(bash_execute_handler)) // DEPRECATED - usar tylluan_do

        .route("/api/v1/security/events", get(security_events_handler))
        .route("/api/v1/inference/providers", get(list_inference_providers_handler).post(add_inference_provider_handler))
        .route("/api/v1/mcp/external", get(list_mcp_servers_handler).post(add_mcp_server_handler))
        .route("/api/v1/mcp/external/discover", post(discover_mcp_servers_handler))
        .route("/api/v1/mcp/external/{name}", delete(remove_mcp_server_handler).put(update_mcp_server_handler))
        .route("/api/v1/system/signals", get(golden_signals_handler))

        .route("/api/v1/system/approvals", get(approval_list_handler))
        .route("/api/v1/approval/list", get(approval_list_handler))
        .route("/api/v1/system/approvals/{id}/approve", post(approval_approve_handler))
        .route("/api/v1/approval/{id}/approve", post(approval_approve_handler))
        .route("/api/v1/system/approvals/{id}/reject", post(approval_reject_handler))
        .route("/api/v1/approval/{id}/reject", post(approval_reject_handler))
        .route("/api/v1/maintenance/status", get(maintenance_status_handler))
        .route("/api/v1/system/maintenance/status", get(maintenance_status_handler))
        .route("/api/v1/maintenance/export", post(maintenance_export_handler))
        .route("/api/v1/maintenance/vacuum", post(maintenance_vacuum_handler))
        .route("/api/v1/maintenance/checkpoint", post(maintenance_checkpoint_handler))
        .route("/api/v1/maintenance/decay", post(maintenance_decay_handler))
        .route("/api/v1/maintenance/purge", post(maintenance_purge_handler))
        .route("/api/v1/maintenance/purge-lessons", post(maintenance_purge_lessons_handler))
        .route("/api/v1/maintenance/clean-orphans", post(maintenance_clean_orphans_handler))
        .route("/api/v1/guilds/{name}/test", post(guild_test_handler))
        .route("/api/v1/test-connection", post(test_connection_handler))
        .route("/api/v1/config/wsl", post(update_wsl_config_handler))
        // Recovered endpoints (were lost in http.rs ??? http/ migration)
        .route("/api/v1/slo/summary", get(slo_summary_handler))
        .route("/api/v1/guilds/utilization", get(guilds_utilization_handler))
        .route("/api/v1/memory/retention", get(memory_retention_handler))
        .route("/api/v1/guild-start", post(guild_start_alt_handler))

        // Compatibility aliases
        .route("/api/v1/health/golden-signals", get(golden_signals_handler))

        .route("/api/v1/mailbox/send", post(mailbox_send_handler))
        .route("/api/v1/blackboard", get(blackboard_handler))
        .route("/api/v1/guilds/{guild_name}/call/{tool_name}", post(guild_tool_call_handler))
        .route("/memory/graph", get(silva_graph_handler))
        // Cognitive forest endpoints

        .route("/api/v1/hormones", get(hormones_handler))
        .route("/api/v1/agent-profiles", get(agent_profiles_handler))
        .route("/api/v1/collective/pulse", get(collective_pulse_handler))
        .route("/api/v1/collective/timeline", get(collective_timeline_handler))
        .route("/api/v1/collective/heatmap", get(collective_heatmap_handler))
        .route("/api/v1/collective/reputation", get(collective_reputation_handler))
        .route("/api/v1/metrics", get(metrics_handler))
        .route("/api/v1/agents", get(agents_list_handler))
        .route("/api/v1/collective/suggest", get(collective_suggest_handler))

        .route("/api/v1/agent-memories/{agent_id}", get(agent_memories_handler))
        .route("/api/v1/agent-memories/{agent_id}/summary", get(agent_memories_summary_handler))
        .route("/api/v1/silva/traces", get(silva_traces_handler))
        .route("/api/v1/agent-memories/{agent_id}", delete(agent_memories_delete_handler))
        .route("/api/v1/session-digest", post(session_digest_handler))
        .route("/api/v1/health/detailed", get(health_detailed_handler))
        .route("/api/v1/canary", get(canary_handler))
        .route("/api/v1/logs", get(logs_handler))
        .route("/api/v1/sandbox/sessions", get(sandbox_sessions_handler))

        // --- Blackboard Coordination Protocol ---
        .route("/api/v1/blackboard/plan", post(blackboard_plan_handler))
        .route("/api/v1/blackboard/tasks/{agent}", get(blackboard_agent_tasks_handler))
.route("/api/v1/blackboard/tasks/{msg_id}/done", post(blackboard_task_done_handler))
        .route("/api/v1/tylluan_graph", post(tylluan_graph_handler).get(tylluan_graph_get_handler))

        // Unified ingest pipeline (R15)
        .route("/api/v1/ingest", post(ingest_handler))

        // Metrics history ring buffer
        .route("/api/v1/metrics/history", get(metrics_history_handler))

        // Admin endpoints
        .route("/api/v1/admin/reload", post(admin_reload_handler))
        .route("/api/v1/admin/meta-prune", post(meta_prune_handler))

        // Federation (M3)
        .route("/api/v1/federation/peers", get(federation_list_peers).post(federation_add_peer))
        .route("/api/v1/federation/peers/{name}", delete(federation_remove_peer))
        .route("/api/v1/federation/peers/{name}/approve", post(federation_approve_peer))
        .route("/api/v1/federation/sync", post(federation_sync_push))
        .route("/api/v1/federation/sync/receive", post(federation_sync_receive))
        // M11-B: Pull sync + bidirectional
        .route("/api/v1/federation/sync/export", get(federation_sync_export))
        .route("/api/v1/federation/sync/pull", post(federation_sync_pull))
        .route("/api/v1/federation/sync/both", post(federation_sync_both))
        // M11-C: Provenance query
        .route("/api/v1/federation/identity", get(federation_identity))
        .route("/api/v1/federation/nodes", get(federation_nodes_query))
        // M12-C: NAT traversal
        .route("/api/v1/nat/external-address", get(nat_external_address))
        .route("/api/v1/federation/ping", get(federation_ping))
        .route("/api/v1/federation/sharing/disable", post(federation_sharing_disable))
        .route("/api/v1/federation/sharing/enable", post(federation_sharing_enable))
        .route("/api/v1/federation/sharing/status", get(federation_sharing_status))
        .route("/api/v1/silva/node/{id}/shareable", post(silva_set_shareable_handler))
        // Routing anchors (M3 anchor routing)
        .route("/api/v1/routing/anchors", get(routing_anchors_list).post(routing_anchors_seed))
        .route("/api/v1/routing/anchors/reembed", post(routing_anchors_reembed))
        .route("/api/v1/silva/edge/search", post(silva_edge_search_handler))
        // Coloquio ??? shared async group chat (M7)
        .route("/api/v1/coloquio/channels", get(coloquio_list_channels).post(coloquio_create_channel))
        .route("/api/v1/coloquio/channels/{id}", get(coloquio_get_thread).delete(coloquio_delete_channel))
        .route("/api/v1/coloquio/channels/{id}/messages", get(coloquio_get_thread))
        .route("/api/v1/coloquio/channels/{id}/post", post(coloquio_post_message))
        .route("/api/v1/coloquio/channels/{id}/search", get(coloquio_search))
        .route("/api/v1/coloquio/channels/{id}/turn/{turn}", get(coloquio_get_turn))
        .route("/api/v1/coloquio/unread", get(coloquio_unread))
        .route("/api/v1/coloquio/channels/{id}/new", get(coloquio_new_messages))
        .route("/api/v1/coloquio/channels/{id}/read", post(coloquio_mark_read))
        .route("/api/v1/coloquio/channels/{id}/typing", post(coloquio_post_typing))
        .route("/api/v1/coloquio/repair-msgids", post(coloquio_repair_msgids))
        .route("/api/v1/coloquio/documents", get(coloquio_list_docs).post(coloquio_create_doc))
        .route("/api/v1/coloquio/documents/{id}", get(coloquio_get_doc).put(coloquio_update_doc).delete(coloquio_delete_doc))
        .route("/api/v1/coloquio/documents/{id}/append", post(coloquio_append_doc))
        .route("/api/v1/coloquio/documents/{id}/versions", get(coloquio_list_doc_versions))
        .route("/api/v1/coloquio/documents/{id}/versions/{version}", get(coloquio_get_doc_version))
        .route("/api/v1/dashboard/summary", get(dashboard_summary_handler))
        .route("/api/v1/autoresearch/summary", get(autoresearch_summary_handler))
        .route("/api/v1/autoresearch/start", post(autoresearch_start_handler))
        .route("/api/v1/autoresearch/stop", post(autoresearch_stop_handler))
        .route("/api/v1/autoresearch/evaluate", post(autoresearch_evaluate_handler))
        .route("/api/v1/skill", get(skill_handler))
        .route("/api/v1/admin/shutdown", post(admin_shutdown_handler))
        .route("/api/v1/admin/emergency-kill", post(admin_emergency_kill_handler))
        .route("/api/v1/admin/kill-guild/{name}", post(admin_kill_guild_handler))
        .route("/api/v1/canvas/ws", get(canvas_ws_handler))
        .route("/api/v1/canvas/{channel}/nodes", post(canvas_create_node_handler))
        // M23-Fractal: tool discovery endpoint
        .route("/api/v1/tools/explore", get(tools_explore_handler))
        // Agent Node Router
        .route("/api/v1/nodes", get(nodes_list_handler))
        .route("/api/v1/nodes/{agent_id}/register", post(nodes_register_handler))
        .route("/api/v1/nodes/{agent_id}/send", post(nodes_send_handler))
        .route("/api/v1/nodes/broadcast", post(nodes_broadcast_handler))
        .route("/api/v1/nodes/{agent_id}/inbox", get(nodes_inbox_handler))
        .route("/api/v1/nodes/{agent_id}/program", get(nodes_get_program_handler).put(nodes_set_program_handler))
        .route("/api/v1/nodes/{agent_id}/unregister", post(nodes_unregister_handler))
        // Agent Journal — crash-safe checkin/recover
        .route("/api/v1/journal", get(journal_list))
        .route("/api/v1/journal/{agent_id}/checkin", post(journal_checkin))
        .route("/api/v1/journal/{agent_id}/recover", get(journal_recover))
        // M9 Autonomous Mode — Agent Registry
        .route("/api/v1/agents/session/start", post(agent_session_start_handler))
        .route("/api/v1/agents/session/stop", post(agent_session_stop_handler))
        .route("/api/v1/agents/session", get(agent_session_list_handler))
        .route("/api/v1/agents/stats", get(agent_stats_handler))
        .route("/api/v1/agents/heartbeat", post(agent_heartbeat_handler))
        // M10 Bounded Work Contracts
        .route("/api/v1/work-contracts", post(contract_create_handler))
        .route("/api/v1/work-contracts/active", get(contract_active_handler))
        .route("/api/v1/work-contracts/{id}", get(contract_get_handler))
        .route("/api/v1/work-contracts/{id}/tick", post(contract_tick_handler))
        .route("/api/v1/work-contracts/{id}/deliver", post(contract_deliver_handler))
        .route("/api/v1/work-contracts/{id}/vote", post(contract_vote_handler))
        .route("/api/v1/work-contracts/{id}/close", post(contract_close_handler))
        // M14-A: Mesh DHT peer discovery
        .route("/api/v1/mesh/peers", get(mesh_peers_handler))
        .route("/api/v1/mesh/refresh", post(mesh_refresh_handler))
        // M14-B: Gossip Protocol — peer knowledge exchange
        .route("/api/v1/gossip", post(gossip_handler))
}

// --- HANDLERS ---

pub async fn mcp_handler(
    State(state): State<Arc<HttpState>>,
    req: axum::extract::Request,
) -> impl IntoResponse {
    let method_http = req.method().clone();
    let path = req.uri().path().to_string();
    let headers = req.headers().clone();
    let query_str = req.uri().query().unwrap_or("").to_string();
    let mut params = HashMap::new();
    for pair in query_str.split('&') {
        let mut parts = pair.splitn(2, '=');
        if let (Some(k), Some(v)) = (parts.next(), parts.next()) {
            params.insert(k.to_string(), v.to_string());
        }
    }
    
    if method_http == axum::http::Method::OPTIONS {
        return (StatusCode::OK, [("allow", "POST, OPTIONS")]).into_response();
    }

    let body = match axum::body::to_bytes(req.into_body(), 10 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, "Body too large").into_response(),
    };

    let session_id = params.get("sessionId").cloned().or_else(|| params.get("session_id").cloned());
    let payload: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => match serde_json::from_str(&String::from_utf8_lossy(&body)) {
            Ok(v) => v,
            Err(e) => return (StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("Invalid JSON body: {}", e)}))
            ).into_response(),
        },
    };
    let id = payload.get("id").cloned();
    tracing::info!("📥 [MCP] Received payload: {}", payload);
    let method = payload.get("method").and_then(|v| v.as_str()).unwrap_or("");

    // ?????? Fast-path: initialize + ping never need the server lock ???????????????????????????????????????????????????
    // Capabilities are static; acquiring the server RwLock would block during
    // guild spawning and cause client timeout during the boot storm.
    if method == "initialize" {
        let client_name = payload
            .get("params").and_then(|p| p.get("clientInfo")).and_then(|c| c.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("mcp-client")
            .to_string();
        let sess_key = session_id.clone().unwrap_or_else(|| client_name.clone());
        crate::transport::http::create_or_update_session(&state.sessions, &sess_key, &client_name, Some(&client_name)).await;
        let client_protocol = payload
            .get("params").and_then(|p| p.get("protocolVersion"))
            .and_then(|v| v.as_str())
            .unwrap_or("2024-11-05")
            .to_string();
        let session_resumed_info = {
            let sessions = state.sessions.read().await;
            sessions.get(&sess_key).filter(|s| s.tool_count > 0).map(|s| serde_json::json!({
                "session_resumed": true,
                "previous_tool_count": s.tool_count,
                "last_guild": s.last_guild,
                "last_intent": s.last_intent,
            }))
        };
        let mut result = serde_json::json!({
            "jsonrpc": "2.0",
            "result": {
                "protocolVersion": client_protocol,
                "capabilities": {
                    "tools": { "listChanged": true },
                    "prompts": { "listChanged": false },
                    "resources": { "subscribe": false, "listChanged": false }
                },
                "serverInfo": { "name": "tylluan-nexus-sovereign", "version": "3.0.0" }
            },
            "id": id
        });
        if let Some(info) = session_resumed_info {
            result["result"]["session"] = info;
        }
        return (StatusCode::OK, axum::Json(result)).into_response();
    }

    if method == "ping" || method == "notifications/initialized" {
        return (StatusCode::OK, axum::Json(serde_json::json!({ "jsonrpc": "2.0", "result": {}, "id": id }))).into_response();
    }

    let server_arc = match state.server.as_ref() {
        Some(s) => s,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Sovereign server not initialized").into_response(),
    };
    let server = server_arc.read().await;

    let response_json = match method {
        "initialize" | "ping" | "notifications/initialized" => unreachable!("handled above"),
        "tools/list" => {
            let tools = server.all_tools().await;
            serde_json::json!({ "jsonrpc": "2.0", "result": { "tools": tools }, "id": id })
        }
        "tools/call" => {
            let tool_params = payload.get("params").cloned().unwrap_or(serde_json::Value::Null);
            let tool_name = tool_params.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let arguments = tool_params.get("arguments").cloned().unwrap_or(serde_json::json!({}));
            let mcp_agent_id = arguments.get("agent_id").and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| session_id.clone())
                .or_else(|| {
                    // Fallback to registered session's client_name if sessionId/agent_id not in args
                    if let Some(ref sid) = session_id {
                        let sessions_guard = state.sessions.try_read().ok();
                        sessions_guard.and_then(|g| g.get(sid).map(|s| s.client_name.clone()))
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| "mcp-client".to_string());
            let intent = arguments.get("intent").and_then(|v| v.as_str()).unwrap_or("").to_string();
            
            // Auto-register node in the router if the client is calling node operations or posting messages,
            // ensuring zero-config out-of-the-box operation for Qwen, Claude, and external clients.
            if !mcp_agent_id.is_empty() && mcp_agent_id != "mcp-client" && mcp_agent_id != "unknown" {
                let node_intent_parsed = crate::memory::agent_nodes::parse_node_intent(&intent);
                let is_coloquio = intent.trim().starts_with("@coloquio") || tool_name == "tylluan_remember" && intent.starts_with("@coloquio:");
                if node_intent_parsed.is_some() || is_coloquio || tool_name == "tylluan_do" {
                    let router = &server.node_router;
                    let _ = router.register(&mcp_agent_id).await;
                }
            }

            // Upsert virtual session so dashboard avatars reflect active HTTP agents
            crate::transport::http::create_or_update_session(&state.sessions, &mcp_agent_id, &mcp_agent_id, Some(&mcp_agent_id)).await;
            {
                let mut sessions = state.sessions.write().await;
                if let Some(entry) = sessions.get_mut(&mcp_agent_id) {
                    entry.tool_count += 1;
                    if !intent.is_empty() { entry.last_intent = Some(intent.clone()); }
                }
            }
            let _ = state.broadcast_tx.send(serde_json::json!({ "type": "tool_call", "status": "started", "tool": &tool_name, "intent": &intent, "agent_id": &mcp_agent_id, "ts": chrono::Utc::now().timestamp_millis() }));
            let _ = state.silva.touch_node(&format!("agent:{}", mcp_agent_id), &mcp_agent_id, &format!("tool_call:{}", tool_name)).await;
            let request = CallToolRequestParam { name: tool_name.clone().into(), arguments: Some(arguments.as_object().cloned().unwrap_or_default()) };
            
            // handle_call_internal is a trait method
            match server.handle_call_internal(request, tylluan_common::types::Channel::Http { authenticated: true }, &session_id.clone().unwrap_or_default()).await {
                Ok(res) => {
                    let is_error = res.is_error.unwrap_or(false);
                    // Increment tool_count on completion (symmetric with /api/v1/do path)
                    {
                        let now_unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                        let mut sessions = state.sessions.write().await;
                        if let Some(entry) = sessions.get_mut(&mcp_agent_id) {
                            entry.tool_count += 1;
                            entry.last_active = std::time::Instant::now();
                            entry.last_active_unix = now_unix;
                        }
                    }
                    let _ = state.broadcast_tx.send(serde_json::json!({ "type": "tool_call", "status": "finished", "tool": &tool_name, "intent": &intent, "agent_id": &mcp_agent_id, "ok": !is_error, "ts": chrono::Utc::now().timestamp_millis() }));
                    // Emit active hormone signals to SSE broadcast for dashboard
                    if let Some(srv) = &state.server {
                        let srv_read = srv.read().await;
                        if let Ok(h) = srv_read.hormones.lock() {
                            let signals = h.active_signals();
                            if !signals.is_empty() {
                                let _ = state.broadcast_tx.send(serde_json::json!({ "type": "hormone_signal", "signals": signals, "ts": chrono::Utc::now().timestamp_millis() }));
                            }
                        }
                    }
                    let result_obj = serde_json::json!({
                        "content": res.content,
                        "isError": is_error,
                    });
                    serde_json::json!({ "jsonrpc": "2.0", "result": result_obj, "id": id })
                }
                Err(e) => {
                    // Increment tool_count on error too (symmetric with /api/v1/do path)
                    {
                        let now_unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                        let mut sessions = state.sessions.write().await;
                        if let Some(entry) = sessions.get_mut(&mcp_agent_id) {
                            entry.tool_count += 1;
                            entry.last_active = std::time::Instant::now();
                            entry.last_active_unix = now_unix;
                        }
                    }
                    serde_json::json!({ "jsonrpc": "2.0", "error": { "code": -32000, "message": e.to_string() }, "id": id })
                }
            }
        }
        "prompts/list" => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "result": {
                    "prompts": [{
                        "name": "tylluan_capabilities",
                        "description": "What TylluanNexus can do ??? read this before your first call to understand the 5 sovereign tools and example intents"
                    }]
                },
                "id": id
            })
        }
        "prompts/get" => {
            let prompt_name = payload.get("params").and_then(|p| p.get("name")).and_then(|v| v.as_str()).unwrap_or("");
            if prompt_name != "tylluan_capabilities" {
                serde_json::json!({ "jsonrpc": "2.0", "error": { "code": -32602, "message": "unknown prompt" }, "id": id })
            } else {
                let text = "# TylluanNexus ??? 5 Sovereign Tools\n\n\
                    ## tylluan_do\n\
                    Execute any task in natural language. The kernel routes to the right guild automatically.\n\
                    Examples:\n\
                    - tylluan_do(intent='list files in /tmp')\n\
                    - tylluan_do(intent='run git status', remember=true)\n\
                    - tylluan_do(intent='create a Python virtualenv in E:/myproject', guild='bash')\n\n\
                    ## tylluan_remember\n\
                    Store information in long-term memory for future recall.\n\
                    Examples:\n\
                    - tylluan_remember(content='The API key rotates every 90 days')\n\
                    - tylluan_remember(content='User prefers concise answers', agent_id='agent-1')\n\n\
                    ## tylluan_recall\n\
                    Semantic search over long-term memory. Returns ranked results with scores.\n\
                    Examples:\n\
                    - tylluan_recall(query='what did we discuss about auth?', limit=5)\n\
                    - tylluan_recall(query='deployment steps', agent_id='agent-1')\n\n\
                    ## tylluan_think\n\
                    Graph-based reasoning without side effects. Returns entities, relationships, evidence.\n\
                    Use BEFORE acting when you need to understand what the system knows about a topic.\n\
                    Examples:\n\
                    - tylluan_think(query='what is the architecture of this project?', depth=2)\n\
                    - tylluan_think(query='sovereign tools contract', chain=true)\n\n\
                    ## tylluan_graph\n\
                    Direct knowledge graph operations: add triples, query paths, list neighbors.\n\
                    Examples:\n\
                    - tylluan_graph(command='stats')\n\
                    - tylluan_graph(command='add_triple', subject='auth', predicate='uses', object='JWT')\n\
                    - tylluan_graph(command='list_neighbors', entity='auth')\n\n\
                    ## Workflow pattern for new sessions\n\
                    1. tylluan_think(query='<topic>') ??? understand what is known\n\
                    2. tylluan_recall(query='<topic>') ??? retrieve relevant memory\n\
                    3. tylluan_do(intent='<task>') ??? execute with context\n\
                    4. tylluan_remember(content='<insight>', agent_id='<your-id>') ??? persist what matters\n";
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "result": {
                        "description": "TylluanNexus sovereign tool reference and workflow patterns",
                        "messages": [{ "role": "user", "content": { "type": "text", "text": text } }]
                    },
                    "id": id
                })
            }
        }
        "resources/list" => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "result": {
                    "resources": [{
                        "uri": "tylluan://skills",
                        "name": "Tylluan Skill Catalog",
                        "description": "Example intents organized by guild ??? paste any of these into tylluan_do",
                        "mimeType": "text/plain"
                    }]
                },
                "id": id
            })
        }
        "resources/read" => {
            let uri = payload.get("params").and_then(|p| p.get("uri")).and_then(|v| v.as_str()).unwrap_or("");
            if uri != "tylluan://skills" {
                serde_json::json!({ "jsonrpc": "2.0", "error": { "code": -32602, "message": "unknown resource uri" }, "id": id })
            } else {
                let text = "# Tylluan Skill Catalog ??? example intents for tylluan_do\n\n\
                    ## bash / shell\n\
                    - 'run ls -la in E:/myproject'\n\
                    - 'create directory E:/tmp/test'\n\
                    - 'check disk usage on C:'\n\n\
                    ## git\n\
                    - 'git status in E:/TylluanMCPo3'\n\
                    - 'show last 5 commits'\n\
                    - 'diff HEAD~1'\n\n\
                    ## filesystem\n\
                    - 'read file E:/TylluanMCPo3/tylluan.toml'\n\
                    - 'search for TODO in E:/TylluanMCPo3/src'\n\
                    - 'list all .rs files in crates/'\n\n\
                    ## code\n\
                    - 'analyze E:/TylluanMCPo3/crates/tylluan-kernel/src/main.rs'\n\
                    - 'find all functions in handler_recall.rs'\n\n\
                    ## monitor\n\
                    - 'show system resource usage'\n\
                    - 'check process list'\n\n\
                    ## docker\n\
                    - 'list running containers'\n\
                    - 'show docker images'\n";
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "result": {
                        "contents": [{ "uri": "tylluan://skills", "mimeType": "text/plain", "text": text }]
                    },
                    "id": id
                })
            }
        }
        _ => serde_json::json!({ "jsonrpc": "2.0", "error": { "code": -32601, "message": format!("Method not found: {}", method) }, "id": id })
    };

    // Detect MCP dialect using 5-step heuristic
    let dialect = detect_mcp_dialect(&headers, &path, &payload);
    
    // Log detected dialect for debugging
    let user_agent = headers.get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");
    tracing::debug!(dialect = ?dialect, ua = %user_agent, path = %path, "MCP dialect detected");

    // Build response based on dialect
    match dialect {
        McpDialect::SseClassic => {
            // SSE Classic: broadcast to session channel, return 202
            // Note: For now, fall back to same response format as SSE
            let sse_body = format!("data: {}\n\n", serde_json::to_string(&response_json).unwrap_or_default());
            (
                StatusCode::OK,
                [
                    ("content-type", "text/event-stream"),
                    ("cache-control", "no-cache"),
                    ("x-accel-buffering", "no"),
                ],
                sse_body,
            ).into_response()
        }
        McpDialect::HttpStreamableSse => {
            // HTTP Streamable with SSE responses (Claude Code)
            let sse_body = format!("data: {}\n\n", serde_json::to_string(&response_json).unwrap_or_default());
            (
                StatusCode::OK,
                [
                    ("content-type", "text/event-stream"),
                    ("cache-control", "no-cache"),
                    ("x-accel-buffering", "no"),
                ],
                sse_body,
            ).into_response()
        }
        McpDialect::HttpStreamableJson => {
            // Modern HTTP Streamable JSON (default)
            (StatusCode::OK, Json(response_json)).into_response()
        }
    }
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
                let episode_id = format!("bb_episode:{}", task_id);
                let content = format!("Blackboard task completed | id:{} | {}", task_id, result);
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
    const FRACTAL_THRESHOLD: f32 = 0.82;
    if !intent.is_empty()
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
            (StatusCode::OK, Json(serde_json::json!({"status": "ok", "response": format!("{:?}", res.content)}))).into_response()
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

async fn silva_recent_handler(State(state): State<Arc<HttpState>>, Query(q): Query<SilvaRecentQuery>) -> impl IntoResponse {
    let limit = q.limit.unwrap_or(20);
    Json(state.silva.get_recent_nodes(limit).await.unwrap_or_default())
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
    let limit = q.limit.unwrap_or(10);
    let engine = match state.matcher.engine() {
        Some(e) => e,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"error": "embedding engine not available"}))).into_response(),
    };
    let query_vec = match engine.embed(&q.query) {
        Ok(v) => v,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    };

    let results: Option<Vec<EdgeSearchResult>> = tokio::task::block_in_place(|| {
        let conn = state.silva.conn.blocking_lock();
        let mut stmt = conn.prepare("SELECT node_id, embedding FROM node_embeddings WHERE node_id LIKE 'edge::%'").ok()?;
        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
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

        let mut out = Vec::with_capacity(scored.len());
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
    let edges = state.silva.get_all_edges().await.unwrap_or_default();
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

    Json(serde_json::json!({
        "active": {
            "embedding": embedding_model,
            "vision": vision_path,
            "inference": inference_model,
            "vector_dimensions": vector_dims
        },
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

// ─── M14-B: Gossip Protocol ──────────────────────────────────────────────

async fn gossip_handler(
    State(state): State<Arc<HttpState>>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    // Record sender's clock so we don't re-send known entries
    if let Some(sender_id) = payload.get("sender_id").and_then(|v| v.as_str()) {
        if let Some(clock) = payload.get("sender_clock").and_then(|v| v.as_u64()) {
            state.gossip_engine.write().await.record_peer_clock(sender_id, clock);
        }
    }
    // Accept any incoming gossip entries into routing table
    if let Some(entries) = payload.get("entries").and_then(|v| v.as_array()) {
        for entry in entries {
            if let (Some(node_id), Some(addr)) = (
                entry.get("node_id").and_then(|v| v.as_str()),
                entry.get("addr").and_then(|v| v.as_str()),
            ) {
                if let Ok(addr) = addr.parse::<std::net::SocketAddr>() {
                    let mut rt = state.dht_routing_table.write().await;
                    rt.insert(&node_id.to_string(), addr, vec!["mesh".into()]);
                }
            }
        }
    }
    (StatusCode::OK, Json(serde_json::json!({ "status": "ok" })))
}
