use axum::{
    Router,
    extract::{State, Query},
    response::{sse::{Event, Sse}, IntoResponse},
    routing::{get, any},
    http::Method,
};
use serde::Deserialize;
use std::sync::Arc;
use std::convert::Infallible;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;
use uuid::Uuid;
use tracing::info;
use crate::transport::http::HttpState;
use crate::transport::server::types::NexusEvent;

#[derive(Deserialize)]
pub struct SseParams {
    #[serde(alias = "sessionId")]
    pub session_id: Option<String>,
    pub token: Option<String>,
}

pub fn sse_routes() -> Router<Arc<HttpState>> {
    Router::new()
        .route("/sse", any(sse_handler))
        .route("/api/v1/events", get(dashboard_events_handler))
        .route("/sse/download", get(download_sse_handler))
}

/// Standard MCP SSE endpoint
async fn sse_handler(
    State(state): State<Arc<HttpState>>,
    method: Method,
    Query(params): Query<SseParams>,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    if method == Method::POST || method == Method::OPTIONS {
        let req = axum::http::Request::builder()
            .method(method)
            .uri("/mcp")
            .body(axum::body::Body::from(body))
            .expect("valid SSE builder request");
        return crate::transport::http::api_v1::mcp_handler(
            State(state),
            req
        ).await.into_response();
    }

    let session_id = params.session_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let was_resumed = {
        let sessions = state.sessions.read().await;
        sessions.contains_key(&session_id)
    };
    if was_resumed {
        info!("SSE session resumed: {} (restoring state)", session_id);
    } else {
        info!("New SSE session established: {}", session_id);
    }

    // Register session immediately so dashboard shows the connection before any tool call
    crate::transport::http::create_or_update_session(&state.sessions, &session_id, "sse-client", None).await;

    let rx = state.broadcast_tx.subscribe();
    
    let mut endpoint_url = format!("/messages?sessionId={}", session_id);
    if let Some(t) = &params.token {
        endpoint_url.push_str(&format!("&token={}", t));
    }
    
    let initial_event = Event::default()
        .event("endpoint")
        .data(endpoint_url);

    let stream = BroadcastStream::new(rx).filter_map(|msg| {
        match msg {
            Ok(json) if json.get("jsonrpc").is_some() => {
                Some(Ok::<Event, Infallible>(Event::default().event("message").data(json.to_string())))
            }
            _ => None,
        }
    });

    let combined_stream = tokio_stream::once(Ok(initial_event)).chain(stream);

    let mut response = Sse::new(combined_stream)
        .keep_alive(axum::response::sse::KeepAlive::default())
        .into_response();

    // Advertise OAuth discovery endpoint so desktop clients find it from a 200 response
    response.headers_mut().insert(
        "Link",
        axum::http::HeaderValue::from_static(
            "</.well-known/oauth-authorization-server>; rel=\"oauth-authorization-server\""
        ),
    );

    response
}

/// Dashboard SSE endpoint — public (no auth required).
async fn dashboard_events_handler(
    State(state): State<Arc<HttpState>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    info!("📊 Dashboard SSE client connected");
    let start_time = state.start_time;
    let rx = state.broadcast_tx.subscribe();

    // Send immediate heartbeat using NexusEvent taxonomy
    let uptime = start_time.elapsed().as_secs();
    let heart = NexusEvent::SystemHeart {
        uptime_secs: uptime,
        active_sessions: 1,
        memory_nodes: 0,
    };
    let initial_event = heart.to_sse_event();

    let stream = BroadcastStream::new(rx).filter_map(|msg| {
        match msg {
            Ok(json) => {
                // Convert raw JSON events to NexusEvent taxonomy
                let nexus_event = if let Some(type_val) = json.get("type").and_then(|v| v.as_str()) {
                    match type_val {
                        "tool_call_start" => {
                            let tool = json.get("tool").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
                            let agent_id = json.get("agent_id").and_then(|v| v.as_str()).map(String::from);
                            let intent = json.get("intent").and_then(|v| v.as_str()).map(String::from);
                            let guild = json.get("guild").and_then(|v| v.as_str()).map(String::from);
                            Some(NexusEvent::ToolCallStart { tool, agent_id, intent, guild })
                        }
                        "tool_call" => {
                            let tool = json.get("tool").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
                            let agent_id = json.get("agent_id").and_then(|v| v.as_str()).map(String::from);
                            let duration_ms = json.get("duration_ms").and_then(|v| v.as_u64()).unwrap_or(0);
                            let success = json.get("success").and_then(|v| v.as_bool()).unwrap_or(true);
                            Some(NexusEvent::ToolCallEnd {
                                tool,
                                agent_id,
                                duration_ms,
                                success,
                                error: None,
                            })
                        }
                        "guild_status" => {
                            let guild = json.get("guild").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
                            let status = json.get("status").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
                            let pid = json.get("pid").and_then(|v| v.as_u64()).map(|p| p as u32);
                            Some(NexusEvent::GuildStatus { guild, status, pid })
                        }
                        "guild_progress" => {
                            let guild = json.get("guild").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
                            let task = json.get("task").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let progress = json.get("progress").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                            Some(NexusEvent::GuildProgress { guild, task, progress })
                        }
                        "error_result" => {
                            let source = json.get("source").and_then(|v| v.as_str()).unwrap_or("kernel").to_string();
                            let message = json.get("message").and_then(|v| v.as_str()).unwrap_or("Unknown error").to_string();
                            let code = json.get("code").and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
                            Some(NexusEvent::ErrorResult { source, message, code })
                        }
                        "heartbeat" => {
                            let uptime = json.get("uptime_secs").and_then(|v| v.as_u64()).unwrap_or(0);
                            let active = json.get("active_sessions").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                            let nodes = json.get("memory_nodes").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                            Some(NexusEvent::SystemHeart {
                                uptime_secs: uptime,
                                active_sessions: active,
                                memory_nodes: nodes,
                            })
                        }
                        "metrics" => {
                            // Unified metrics handling - handles both old and new keys
                            let cpu = json.get("cpu_percent").or(json.get("cpu")).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                            let mem = json.get("memory_percent").or(json.get("memory_pct")).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                            let storage = json.get("storage_bytes").and_then(|v| v.as_u64()).unwrap_or(0);
                            let guilds = json.get("guilds_online").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                            let errors = json.get("errors_today").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                            
                            // Inject curriculum and nodes_count if present in the raw event
                            let mut data = serde_json::json!({
                                "cpu_percent": cpu,
                                "memory_percent": mem,
                                "storage_bytes": storage,
                                "guilds_online": guilds,
                                "errors_today": errors,
                            });
                            
                            if let Some(obj) = data.as_object_mut() {
                                if let Some(nodes) = json.get("nodes_count").or(json.get("nodes")) {
                                    obj.insert("nodes_count".to_string(), nodes.clone());
                                }
                                if let Some(curr) = json.get("curriculum") {
                                    obj.insert("curriculum".to_string(), curr.clone());
                                }
                            }

                            // We still return NexusEvent::Metrics but the to_sse_event will use the enum
                            // So we must ensure NexusEvent::Metrics to_sse_event includes our extra fields if we want them there.
                            // Actually, let's keep NexusEvent simple and just ensure the basics are mapped.
                            Some(NexusEvent::Metrics {
                                cpu_percent: cpu,
                                memory_percent: mem,
                                storage_bytes: storage,
                                guilds_online: guilds,
                                errors_today: errors,
                            })
                        }
                        "thought" => {
                            let content = json.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let agent_id = json.get("agent_id").and_then(|v| v.as_str()).map(String::from);
                            let confidence = json.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
                            Some(NexusEvent::Thought { content, agent_id, confidence })
                        }
                        "approval_required" => {
                            let request_id = json.get("request_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let tool = json.get("tool").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let risk = json.get("risk").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
                            let agent_id = json.get("agent_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            Some(NexusEvent::ApprovalRequired { request_id, tool, risk, agent_id })
                        }
                        "edge_added" => {
                            let source = json.get("source").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let target = json.get("target").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let relation = json.get("relation").and_then(|v| v.as_str()).unwrap_or("related").to_string();
                            Some(NexusEvent::EdgeAdded { source, target, relation })
                        }
                        "memory_added" => {
                            let id = json.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let node_type = json.get("node_type").and_then(|v| v.as_str()).unwrap_or("node").to_string();
                            let content = json.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            Some(NexusEvent::MemoryAdded { id, node_type, content })
                        }
                        "memory_updated" => {
                            let id = json.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let weight = json.get("weight").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                            Some(NexusEvent::MemoryUpdated { id, weight })
                        }
                        "system_status" => {
                            let silva_healthy = json.get("silva_healthy").and_then(|v| v.as_bool()).unwrap_or(true);
                            let mailbox_healthy = json.get("mailbox_healthy").and_then(|v| v.as_bool()).unwrap_or(true);
                            let curriculum_entries = json.get("curriculum_entries").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                            let uptime_secs = json.get("uptime_secs").and_then(|v| v.as_u64()).unwrap_or(0);
                            let embeddings_loaded = json.get("embeddings_loaded").and_then(|v| v.as_bool()).unwrap_or(true);
                            let score = json.get("score").and_then(|v| v.as_u64()).unwrap_or(100) as u8;
                            Some(NexusEvent::SystemStatus { silva_healthy, mailbox_healthy, curriculum_entries, uptime_secs, embeddings_loaded, score })
                        }
                        "doc:updated" | "doc:created" | "coloquio:new_turn" => {
                            Some(NexusEvent::Raw(json.clone()))
                        }
                        _ => None,
                    }
                } else {
                    None
                };

                nexus_event.map(|e| Ok(e.to_sse_event()))
            }
            Err(_) => None,
        }
    });

    let combined = tokio_stream::once(Ok(initial_event)).chain(stream);
    Sse::new(combined).keep_alive(axum::response::sse::KeepAlive::default())
}

/// SSE endpoint for download progress events
async fn download_sse_handler(
    State(state): State<Arc<HttpState>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    info!("📥 Download progress SSE session established");
    let rx = state.download_progress_tx.subscribe();
    
    let stream = BroadcastStream::new(rx).filter_map(|msg| {
        match msg {
            Ok(progress) => {
                let json = serde_json::to_string(&progress).unwrap_or_default();
                Some(Ok(Event::default()
                    .event("download_progress")
                    .data(json)))
            }
            Err(_) => None,
        }
    });

    Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::default())
}

/// Spawns the global heartbeat loop for SSE clients.
pub fn spawn_heartbeat(
    broadcast_tx: tokio::sync::broadcast::Sender<serde_json::Value>,
    start_time: std::time::Instant,
    sessions: Arc<tokio::sync::RwLock<std::collections::HashMap<String, super::McpSession>>>,
    mailbox: Arc<crate::memory::mailbox::Mailbox>,
    silva: Arc<crate::memory::silva::SilvaDB>,
    decay_enabled: bool,
    decay_interval_secs: u64,
    decay_half_life_hours: u64,
    registry: Arc<crate::registry::actor::RegistryHandle>,
    matcher: Arc<crate::router::matcher::GuildMatcher>,

) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
        interval.tick().await;
        let mut ticks = 0u64;
        let decay_every_ticks = if decay_interval_secs > 0 {
            decay_interval_secs.div_ceil(15)
        } else { 0 };
        let mut decay_cycle = 0u64;
        loop {
            interval.tick().await;
            ticks += 1;

            // Update guild health scores every 120 ticks (30 min)
            if ticks.is_multiple_of(120) {
                let reg = registry.clone();
                let mat = matcher.clone();
                let bt = broadcast_tx.clone();
                tokio::spawn(async move {
                    let scores = reg.compute_health_scores().await;
                    if !scores.is_empty() {
                        mat.update_all_health(scores.clone());

                        tracing::info!("📊 Pressure-field routing: {} guilds health updated", scores.len());
                        let _ = bt.send(serde_json::json!({
                            "type": "guild_health_updated",
                            "data": scores,
                            "ts": chrono::Utc::now().timestamp_millis()
                        }));
                    }
                });
            }
            
            // Purge expired messages every 10 ticks (5 min)
            if ticks.is_multiple_of(10) {
                let _ = mailbox.purge_expired().await;
            }

            // Biological decay periodic run
            if decay_enabled && decay_every_ticks > 0 && decay_cycle.is_multiple_of(decay_every_ticks) {
                let silva_clone = silva.clone();
                let broadcast_clone = broadcast_tx.clone();
                tokio::spawn(async move {
                    if let Ok(affected) = silva_clone.apply_decay(decay_half_life_hours).await {
                        if affected > 0 {
                            tracing::info!("🌲 Biological decay: {} nodes affected", affected);
                            let _ = broadcast_clone.send(serde_json::json!({
                                "type": "memory_decay",
                                "data": { "affected": affected, "ts": chrono::Utc::now().timestamp() }
                            }));
                        }
                        if let Ok(pruned) = silva_clone.prune_old_traces(90).await
                            && pruned > 0 {
                                tracing::info!("🧹 Heartbeat pruned {} old traces", pruned);
                            }
                    }
                });
            }
            decay_cycle += 1;

            let ev = serde_json::json!({
                "type": "heartbeat",
                "uptime_secs": start_time.elapsed().as_secs(),
                "ts": chrono::Utc::now().timestamp_millis()
            });
            if broadcast_tx.send(ev).is_err() {
                break;
            }
            // Cleanup sessions idle > 10 min
            let mut s_map = sessions.write().await;
            let now = std::time::Instant::now();
            let expired: Vec<(String, Option<String>)> = s_map.iter()
                .filter(|(_, s)| now.duration_since(s.last_active) > std::time::Duration::from_secs(600))
                .map(|(id, s)| (id.clone(), s.agent_id.clone()))
                .collect();
            for (sid, aid) in expired {
                let agent_id = aid.unwrap_or_else(|| "anonymous".to_string());
                let session_id_clone = sid.clone();
                let silva_clone = silva.clone();
                tokio::spawn(async move {
                    let mgr = crate::memory::agent_memory::AgentMemoryManager::new(silva_clone, 20);
                    let _ = mgr.create_session_digest(&agent_id, &session_id_clone).await;
                });
                s_map.remove(&sid);
            }
            // Persist sessions
            let _ = silva.save_sessions(&s_map).await;
        }
    });
}

/// Spawns the real-time metrics broadcaster.
pub fn spawn_metrics_broadcaster(
    broadcast_tx: tokio::sync::broadcast::Sender<serde_json::Value>,
    doctor: Arc<crate::doctor::Doctor>,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        let mut last_metrics: Option<serde_json::Map<String, serde_json::Value>> = None;
        loop {
            interval.tick().await;
            let metrics = doctor.metrics().await;
            let Some(obj) = metrics.as_object() else {
                continue;
            };
            if last_metrics.as_ref() == Some(obj) {
                continue; // no change — skip broadcast
            }
            last_metrics = Some(obj.clone());
            let mut ev = obj.clone();
            ev.insert("type".to_string(), serde_json::json!("metrics"));
            if broadcast_tx.send(serde_json::Value::Object(ev)).is_err() {
                break;
            }
        }
    });
}
