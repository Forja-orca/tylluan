//! MCP transport core: JSON-RPC handler, dialect detection and the
//! stateless (2026-07-28) protocol metadata validation. Split from api_v1.rs.
use axum::{
    Json,
    extract::State,
    http::{StatusCode, HeaderMap, header::ACCEPT},
    response::IntoResponse,
};
use std::collections::HashMap;
use std::sync::Arc;

use rmcp::model::CallToolRequestParam;

use crate::transport::http::HttpState;

use super::validate_task_status_transition;

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

/// MCP protocol versions Tylluan's transport implements, newest first.
/// The 2026 entry is advertised only after the stateless core is wired through
/// `mcp_handler` and verified end-to-end. Legacy handshake negotiation remains
/// intentionally separate because 2026 has no initialize/session handshake.
pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2026-07-28", "2025-06-18", "2025-03-26", "2024-11-05"];
pub const LEGACY_PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
pub const STATELESS_PROTOCOL_VERSION: &str = "2026-07-28";
pub const META_PROTOCOL_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
pub const META_CLIENT_INFO: &str = "io.modelcontextprotocol/clientInfo";
pub const META_CLIENT_CAPABILITIES: &str = "io.modelcontextprotocol/clientCapabilities";

#[derive(Debug, Clone)]
pub struct StatelessRequestMeta {
    pub protocol_version: String,
    pub client_info: serde_json::Value,
    pub client_capabilities: serde_json::Value,
}

/// Parse and validate the per-request metadata required by the 2026 stateless core.
/// Returning `Ok(None)` means this is a legacy request, not that metadata is optional
/// for a request which claims the stateless protocol.
pub fn parse_stateless_request_meta(
    headers: &HeaderMap,
    payload: &serde_json::Value,
) -> Result<Option<StatelessRequestMeta>, (StatusCode, serde_json::Value)> {
    let header_version = headers
        .get("MCP-Protocol-Version")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let params = payload.get("params").and_then(serde_json::Value::as_object);
    let meta = params
        .and_then(|value| value.get("_meta"))
        .and_then(serde_json::Value::as_object);
    let meta_version = meta
        .and_then(|value| value.get(META_PROTOCOL_VERSION))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);

    let claims_stateless = header_version.as_deref() == Some(STATELESS_PROTOCOL_VERSION)
        || meta_version.as_deref() == Some(STATELESS_PROTOCOL_VERSION);
    if !claims_stateless {
        return Ok(None);
    }

    let Some(header_version) = header_version else {
        return Err((
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "jsonrpc": "2.0",
                "error": {
                    "code": -32602,
                    "message": "MCP-Protocol-Version header is required for the stateless protocol"
                },
                "id": payload.get("id")
            }),
        ));
    };
    let Some(meta) = meta else {
        return Err((
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "jsonrpc": "2.0",
                "error": {
                    "code": -32602,
                    "message": "params._meta is required for the stateless protocol"
                },
                "id": payload.get("id")
            }),
        ));
    };
    let Some(meta_version) = meta.get(META_PROTOCOL_VERSION).and_then(serde_json::Value::as_str) else {
        return Err((
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "jsonrpc": "2.0",
                "error": {
                    "code": -32602,
                    "message": format!("params._meta.{META_PROTOCOL_VERSION} is required")
                },
                "id": payload.get("id")
            }),
        ));
    };
    if header_version != meta_version {
        return Err((
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "jsonrpc": "2.0",
                "error": {
                    "code": -32602,
                    "message": "MCP-Protocol-Version must match params._meta protocolVersion"
                },
                "id": payload.get("id")
            }),
        ));
    }
    let Some(client_info) = meta.get(META_CLIENT_INFO).filter(|value| value.is_object()) else {
        return Err((
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "jsonrpc": "2.0",
                "error": {
                    "code": -32602,
                    "message": format!("params._meta.{META_CLIENT_INFO} is required and must be an object")
                },
                "id": payload.get("id")
            }),
        ));
    };
    let valid_client_info = client_info.get("name").and_then(serde_json::Value::as_str).is_some()
        && client_info.get("version").and_then(serde_json::Value::as_str).is_some();
    if !valid_client_info {
        return Err((
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "jsonrpc": "2.0",
                "error": {
                    "code": -32602,
                    "message": format!("params._meta.{META_CLIENT_INFO} requires name and version")
                },
                "id": payload.get("id")
            }),
        ));
    }
    let Some(client_capabilities) = meta.get(META_CLIENT_CAPABILITIES).filter(|value| value.is_object()) else {
        return Err((
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "jsonrpc": "2.0",
                "error": {
                    "code": -32602,
                    "message": format!("params._meta.{META_CLIENT_CAPABILITIES} is required and must be an object")
                },
                "id": payload.get("id")
            }),
        ));
    };

    Ok(Some(StatelessRequestMeta {
        protocol_version: meta_version.to_owned(),
        client_info: client_info.clone(),
        client_capabilities: client_capabilities.clone(),
    }))
}

/// Negotiate the protocol version to declare in `initialize`'s response.
/// If the client's requested version is one Tylluan actually implements, honor it
/// (real negotiation). Otherwise, declare the newest version Tylluan does support --
/// never echo back a version the server doesn't speak.
pub fn negotiate_protocol_version(requested: &str) -> &'static str {
    LEGACY_PROTOCOL_VERSIONS
        .iter()
        .find(|&&v| v == requested)
        .copied()
        .unwrap_or(LEGACY_PROTOCOL_VERSIONS[0])
}

/// Detect MCP dialect using 5-step heuristic (first match wins)
pub fn detect_mcp_dialect(
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

    // Step 4: protocolVersion in initialize body or per-request _meta.
    if let Some(version) = body
        .get("params")
        .and_then(|p| p.get("protocolVersion").or_else(|| p.get("_meta")))
        .and_then(|value| {
            value.get(META_PROTOCOL_VERSION).or(Some(value))
        })
        .and_then(|v| v.as_str())
    {
        match version {
            "2024-11-05" => return McpDialect::SseClassic,
            "2025-03-26" | "2025-06-18" => return McpDialect::HttpStreamableJson,
            STATELESS_PROTOCOL_VERSION => return McpDialect::HttpStreamableJson,
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
    let stateless_meta = match parse_stateless_request_meta(&headers, &payload) {
        Ok(meta) => meta,
        Err((status, error)) => return (status, Json(error)).into_response(),
    };
    let stateless = stateless_meta.is_some();
    if stateless {
        if session_id.is_some() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "jsonrpc": "2.0",
                    "error": {
                        "code": -32602,
                        "message": "sessionId/session_id is not allowed by the stateless protocol"
                    },
                    "id": id
                })),
            ).into_response();
        }
        // REAL BUG FIX (2026-08-19, found live: Claude Code -- and every other
        // real, standards-compliant MCP client -- connected fine but every
        // tool call failed): this used to require two custom, self-invented
        // HTTP headers (Mcp-Method, Mcp-Name) that duplicate data already
        // present in the JSON-RPC body (`method`, `params.name`) and are not
        // part of any real MCP spec revision. No real client anywhere sends
        // them -- only this repo's own test helpers did, which is why 665+
        // tests never caught this: the tests were written to satisfy their
        // own invented requirement, not to exercise real client behavior.
        // Introduced 2026-08-10 (6133b5a, "M39-P2 stateless request meta
        // (WIP)" -- explicitly marked WIP in its own commit message) and
        // silently broke every real stateless MCP connection for 9 days.
        // Checking header==body is not a real security boundary either: an
        // attacker who can forge the body can trivially forge a matching
        // header. Removed outright rather than made optional, since making
        // it optional-but-enforced-if-present would still be dead weight no
        // real client benefits from.
        if method == "initialize" || method == "notifications/initialized" {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "jsonrpc": "2.0",
                    "error": { "code": -32601, "message": format!("Method not found: {method}") },
                    "id": id
                })),
            ).into_response();
        }
        if let Some(meta) = stateless_meta.as_ref() {
            tracing::debug!(
                protocol_version = %meta.protocol_version,
                client = ?meta.client_info,
                capabilities = ?meta.client_capabilities,
                "MCP stateless request metadata accepted"
            );
        }
    }

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
        let client_supports_apps = crate::transport::http::mcp_apps::client_supports_mcp_apps(&payload);
        if client_supports_apps
            && let Some(session) = state.sessions.write().await.get_mut(&sess_key) {
                session.mcp_apps = true;
            }
        let requested_protocol = payload
            .get("params").and_then(|p| p.get("protocolVersion"))
            .and_then(|v| v.as_str())
            .unwrap_or("2024-11-05");
        let client_protocol = negotiate_protocol_version(requested_protocol).to_string();
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
                    "resources": { "subscribe": false, "listChanged": false },
                    // Presence-flag objects, not boolean sub-fields -- matches the real
                    // spec's extension-negotiation shape (e.g. "io.modelcontextprotocol/
                    // tasks": {}). {"cancel": true, "update": true} broke MCP clients'
                    // own schema validation on reconnect (2026-08-09, caught live by
                    // Claude Code itself failing to reconnect after a kernel restart).
                    "tasks": {},
                    "extensions": {
                        "io.modelcontextprotocol/ui": {
                            "mimeTypes": ["text/html;profile=mcp-app"]
                        }
                    }
                },
                "serverInfo": { "name": "tylluan-nexus-sovereign", "version": "3.0.0" }
            },
            "id": id
        });
        if let Some(info) = session_resumed_info {
            result["result"]["session"] = info;
        }
        // Identity continuity: if this connection's bearer token is bound to a
        // registered agent identity, hand it back at the exact moment the
        // client connects instead of making the agent re-derive who it is.
        if let Some(bound_agent) = crate::transport::http::auth::current_bound_agent_id() {
            let identity_mgr = crate::memory::identity::IdentityManager::new(state.silva.clone());
            if let Some(context_prompt) = identity_mgr.get_agent_context(&bound_agent).await {
                result["result"]["identity"] = serde_json::json!({
                    "agent_id": bound_agent,
                    "context": context_prompt,
                });
            }
            // Last in-progress task: JournalDb already tracks this on every tool
            // call (crash-safe checkpoint), but it only lived behind a REST
            // endpoint nobody called. Hand it back at connect time, same as
            // identity above -- this is "what was I doing" continuity, not just
            // "who am I".
            if let Ok(Some(entry)) = state.journal.recover(&bound_agent) {
                let (stale, stale_secs) = crate::transport::http::api_v1::api_journal::is_stale(entry.updated_at);
                result["result"]["last_task"] = serde_json::json!({
                    "task": entry.task,
                    "updated_at_unix": entry.updated_at,
                    "stale": stale,
                    "stale_secs": stale_secs,
                });
            }
        }
        // Temporal grounding: an agent reconnecting has no inherent sense of
        // "when" it is unless the kernel tells it. UTC reference plus a
        // curated world clock -- enough to answer "what time is it in Tokyo"
        // without a round trip. whoami accepts an explicit `timezone` arg for
        // any other IANA zone.
        let now = chrono::Utc::now();
        const WORLD_CLOCK_ZONES: [(&str, chrono_tz::Tz); 6] = [
            ("Madrid", chrono_tz::Europe::Madrid),
            ("London", chrono_tz::Europe::London),
            ("New_York", chrono_tz::America::New_York),
            ("Tokyo", chrono_tz::Asia::Tokyo),
            ("Shanghai", chrono_tz::Asia::Shanghai),
            ("Sydney", chrono_tz::Australia::Sydney),
        ];
        let world_clock: serde_json::Map<String, serde_json::Value> = WORLD_CLOCK_ZONES
            .iter()
            .map(|(label, tz)| (label.to_string(), serde_json::Value::String(now.with_timezone(tz).to_rfc3339())))
            .collect();
        result["result"]["now"] = serde_json::json!({
            "utc": now.to_rfc3339(),
            "unix_epoch": now.timestamp(),
            "weekday": now.format("%A").to_string(),
            "world_clock": world_clock,
        });
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

    let mut response_json = match method {
        "initialize" | "ping" | "notifications/initialized" => unreachable!("handled above"),
        "tools/list" => {
            let tools = server.all_tools().await;
            let request_supports_apps = crate::transport::http::mcp_apps::client_supports_mcp_apps(&payload);
            if request_supports_apps
                && let Some(session_key) = session_id.as_deref()
                && let Some(session) = state.sessions.write().await.get_mut(session_key) {
                    session.mcp_apps = true;
                }
            let session_supports_apps = if stateless {
                false
            } else {
                session_id
                    .as_deref()
                    .and_then(|session_key| state.sessions.try_read().ok()?.get(session_key).map(|session| session.mcp_apps))
                    .unwrap_or(false)
            };
            let apps_enabled = request_supports_apps || session_supports_apps;
            let tools = crate::transport::http::mcp_apps::tools_json(&tools, apps_enabled);
            // REAL BUG FIX (2026-08-19, corrected twice): protocol revision
            // 2026-07-28 requires every list-style result to carry `resultType`
            // explicitly (this server never paginates, so "complete" is always
            // correct). It also defines a CacheableResult interface: `ttlMs`
            // (freshness hint, ms) and `cacheScope` ("public"|"private") --
            // per the real spec example (modelcontextprotocol.io/specification/
            // 2026-07-28/server/tools), these live on the RESULT object itself,
            // as siblings of `tools`, NOT inside each individual tool entry.
            // An earlier version of this fix put them per-tool instead, which
            // still failed strict-client validation (the field existed, just
            // in the wrong place) -- caught live when Claude Code kept
            // rejecting tools/list even after that fix shipped and restarted.
            // This server has no result-caching layer, so `ttlMs: 0` (never
            // cache) and `cacheScope: "private"` (never treat as shareable)
            // are the conservative, always-correct values.
            serde_json::json!({
                "jsonrpc": "2.0",
                "result": { "tools": tools, "resultType": "complete", "ttlMs": 0, "cacheScope": "private" },
                "id": id
            })
        }
        "tools/call" => {
            let tool_params = payload.get("params").cloned().unwrap_or(serde_json::Value::Null);
            let tool_name = tool_params.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let arguments = tool_params.get("arguments").cloned().unwrap_or(serde_json::json!({}));
            let explicit_agent_id = arguments.get("agent_id").and_then(|v| v.as_str()).map(str::to_owned);
            let mcp_agent_id = if stateless {
                // The stateless protocol never derives application identity from
                // transport state. Agents that need continuity pass agent_id as an
                // ordinary tool argument, where the model can see and forward it.
                explicit_agent_id.clone().unwrap_or_else(|| "mcp-client".to_string())
            } else {
                explicit_agent_id
                    .or_else(|| session_id.clone())
                    .or_else(|| {
                        // Legacy compatibility: recover the registered client name
                        // from its sessionId when using the pre-2026 transport.
                        if let Some(ref sid) = session_id {
                            let sessions_guard = state.sessions.try_read().ok();
                            sessions_guard.and_then(|g| g.get(sid).map(|s| s.client_name.clone()))
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| "mcp-client".to_string())
            };
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

            if !stateless {
                // Upsert virtual session so dashboard avatars reflect active HTTP agents.
                crate::transport::http::create_or_update_session(&state.sessions, &mcp_agent_id, &mcp_agent_id, Some(&mcp_agent_id)).await;
                {
                    let mut sessions = state.sessions.write().await;
                    if let Some(entry) = sessions.get_mut(&mcp_agent_id) {
                        entry.tool_count += 1;
                        if !intent.is_empty() { entry.last_intent = Some(intent.clone()); }
                    }
                }
            }
            let _ = state.broadcast_tx.send(serde_json::json!({ "type": "tool_call", "status": "started", "tool": &tool_name, "intent": &intent, "agent_id": &mcp_agent_id, "ts": chrono::Utc::now().timestamp_millis() }));
            let _ = state.silva.touch_node(&format!("agent:{mcp_agent_id}"), &mcp_agent_id, &format!("tool_call:{tool_name}")).await;
            let request = CallToolRequestParam { name: tool_name.clone().into(), arguments: Some(arguments.as_object().cloned().unwrap_or_default()) };
            
            // handle_call_internal is a trait method
            let internal_session_id = if stateless { "" } else { session_id.as_deref().unwrap_or_default() };
            match server.handle_call_internal(request, tylluan_common::types::Channel::Http { authenticated: true }, internal_session_id).await {
                Ok(res) => {
                    let is_error = res.is_error.unwrap_or(false);
                    let structured_content = if tool_name == "tylluan_graph" {
                        crate::transport::http::mcp_apps::graph_structured_content(&res)
                    } else {
                        None
                    };
                    // Increment tool_count on completion (symmetric with /api/v1/do path)
                    if !stateless {
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
                    // REAL BUG FIX (2026-08-19, found live via a real tools/call
                    // through Claude Code's native MCP connection, after tools/list
                    // was already fixed): resultType is required on every result
                    // object under protocol revision 2026-07-28, not just list
                    // endpoints -- the spec's own tools/call example shows it
                    // (modelcontextprotocol.io/specification/2026-07-28/server/tools).
                    // This server never uses the MRTR "input_required" interim-result
                    // pattern, so "complete" is always correct here.
                    let mut result_obj = serde_json::json!({
                        "resultType": "complete",
                        "content": res.content,
                        "isError": is_error,
                    });
                    if let Some(structured_content) = structured_content {
                        result_obj["structuredContent"] = structured_content;
                    }
                    serde_json::json!({ "jsonrpc": "2.0", "result": result_obj, "id": id })
                }
                Err(e) => {
                    // Increment tool_count on error too (symmetric with /api/v1/do path)
                    if !stateless {
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
            // See the resultType comment on tools/list above -- same fix, same
            // reason: this endpoint never paginates either.
            serde_json::json!({
                "jsonrpc": "2.0",
                "result": {
                    "prompts": [
                        {
                            "name": "tylluan_capabilities",
                            "description": "What TylluanNexus can do ??? read this before your first call to understand the 5 sovereign tools and example intents"
                        },
                        {
                            "name": "tylluan_engineering_constitution",
                            "description": "Universal multi-agent engineering discipline (the 10 sins, red zones, briefing/handoff templates) ??? product-agnostic, useful for building on Tylluan or bootstrapping any new project"
                        }
                    ],
                    "resultType": "complete",
                    "ttlMs": 0,
                    "cacheScope": "private"
                },
                "id": id
            })
        }
        "prompts/get" => {
            let prompt_name = payload.get("params").and_then(|p| p.get("name")).and_then(|v| v.as_str()).unwrap_or("");
            if prompt_name == "tylluan_engineering_constitution" {
                const CONSTITUTION: &str = include_str!("../../../../../../docs/concepts/ENGINEERING_CONSTITUTION.md");
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "result": {
                        "resultType": "complete",
                        "description": "Universal multi-agent engineering constitution ??? product-agnostic discipline for any agent building on Tylluan or elsewhere",
                        "messages": [{ "role": "user", "content": { "type": "text", "text": CONSTITUTION } }]
                    },
                    "id": id
                })
            } else if prompt_name != "tylluan_capabilities" {
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
                        "resultType": "complete",
                        "description": "TylluanNexus sovereign tool reference and workflow patterns",
                        "messages": [{ "role": "user", "content": { "type": "text", "text": text } }]
                    },
                    "id": id
                })
            }
        }
        "resources/list" => {
            // See the resultType comment on tools/list above -- same fix, same
            // reason: this endpoint never paginates either.
            serde_json::json!({
                "jsonrpc": "2.0",
                    "result": {
                        "resources": [{
                            "uri": "tylluan://skills",
                        "name": "Tylluan Skill Catalog",
                        "description": "Example intents organized by guild ??? paste any of these into tylluan_do",
                        "mimeType": "text/plain"
                        }, crate::transport::http::mcp_apps::graph_resource_descriptor()],
                        "resultType": "complete",
                        "ttlMs": 0,
                        "cacheScope": "private"
                },
                "id": id
            })
        }
        "resources/read" => {
            let uri = payload.get("params").and_then(|p| p.get("uri")).and_then(|v| v.as_str()).unwrap_or("");
            if uri == crate::transport::http::mcp_apps::GRAPH_APP_URI {
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "result": {
                        "resultType": "complete",
                        "contents": [{
                            "uri": crate::transport::http::mcp_apps::GRAPH_APP_URI,
                            "mimeType": crate::transport::http::mcp_apps::MCP_APP_MIME,
                            "text": crate::transport::http::mcp_apps::GRAPH_APP_HTML,
                            "_meta": {
                                "ui": {
                                    "csp": {
                                        "connectDomains": [],
                                        "resourceDomains": [],
                                        "frameDomains": [],
                                        "baseUriDomains": []
                                    },
                                    "prefersBorder": true
                                }
                            }
                        }]
                    },
                    "id": id
                })
            } else if uri != "tylluan://skills" {
                serde_json::json!({ "jsonrpc": "2.0", "error": { "code": -32602, "message": "unknown resource uri" }, "id": id })
            } else {
                let text = "# Tylluan Skill Catalog — example intents for tylluan_do\n\n\
                    ## bash / shell\n\
                    - 'run ls -la in E:/myproject'\n\
                    - 'create directory E:/tmp/test'\n\
                    - 'check disk usage on C:'\n\n\
                    ## git\n\
                    - 'git status in E:/myproject'\n\
                    - 'show last 5 commits'\n\
                    - 'diff HEAD~1'\n\n\
                    ## filesystem\n\
                    - 'read file E:/myproject/tylluan.toml'\n\
                    - 'search for TODO in E:/myproject/src'\n\
                    - 'list all .rs files in crates/'\n\n\
                    ## code\n\
                    - 'analyze E:/myproject/crates/tylluan-kernel/src/main.rs'\n\
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
                        "resultType": "complete",
                        "contents": [{ "uri": "tylluan://skills", "mimeType": "text/plain", "text": text }]
                    },
                    "id": id
                })
            }
        }
        "server/discover" => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "result": {
                    "resultType": "complete",
                    "serverInfo": { "name": "tylluan-nexus-sovereign", "version": env!("CARGO_PKG_VERSION") },
                    "capabilities": {
                        "tools": { "listChanged": true },
                        "prompts": { "listChanged": false },
                        "resources": { "subscribe": false, "listChanged": false },
                        "tasks": {},
                        "extensions": {
                            "io.modelcontextprotocol/ui": {
                                "mimeTypes": ["text/html;profile=mcp-app"]
                            }
                        }
                    },
                    "supportedVersions": SUPPORTED_PROTOCOL_VERSIONS,
                    "instructions": "Every stateless request must include MCP-Protocol-Version and params._meta with protocolVersion, clientInfo, and clientCapabilities."
                },
                "id": id
            })
        }
        "tasks/get" => {
            let task_id = payload.get("params").and_then(|p| p.get("taskId").or_else(|| p.get("task_id"))).and_then(|v| v.as_str()).unwrap_or("");
            if task_id.is_empty() {
                serde_json::json!({ "jsonrpc": "2.0", "error": { "code": -32602, "message": "taskId parameter is required" }, "id": id })
            } else {
                match state.jobs.get_by_id(task_id) {
                    Ok(Some(job)) => {
                        let mcp_status = match job.status.as_str() {
                            "pending" | "running" => "working",
                            "done" => "completed",
                            "failed" => "failed",
                            "cancelled" => "cancelled",
                            other => other,
                        };
                        serde_json::json!({
                            "jsonrpc": "2.0",
                            "result": {
                                "resultType": "complete",
                                "taskId": job.id,
                                "taskType": job.task_type,
                                "status": mcp_status,
                                "payload": job.payload,
                                "created_at": job.created_at,
                                "updated_at": job.updated_at
                            },
                            "id": id
                        })
                    }
                    Ok(None) => serde_json::json!({ "jsonrpc": "2.0", "error": { "code": -32602, "message": format!("task '{}' not found", task_id) }, "id": id }),
                    Err(e) => serde_json::json!({ "jsonrpc": "2.0", "error": { "code": -32603, "message": format!("database error: {}", e) }, "id": id }),
                }
            }
        }
        "tasks/update" => {
            let task_id = payload.get("params").and_then(|p| p.get("taskId").or_else(|| p.get("task_id"))).and_then(|v| v.as_str()).unwrap_or("");
            let new_status = payload.get("params").and_then(|p| p.get("status")).and_then(|v| v.as_str()).unwrap_or("");
            let meta = payload.get("params").and_then(|p| p.get("meta").or_else(|| p.get("payload")));
            if task_id.is_empty() || new_status.is_empty() {
                serde_json::json!({ "jsonrpc": "2.0", "error": { "code": -32602, "message": "taskId and status parameters are required" }, "id": id })
            } else {
                match state.jobs.get_by_id(task_id) {
                    Ok(existing) => {
                        let current_status = existing.as_ref().map(|j| j.status.as_str());
                        match validate_task_status_transition(new_status, current_status) {
                            Err(msg) => serde_json::json!({ "jsonrpc": "2.0", "error": { "code": -32602, "message": msg }, "id": id }),
                            Ok(()) if existing.is_none() => serde_json::json!({ "jsonrpc": "2.0", "error": { "code": -32602, "message": format!("task '{}' not found for update", task_id) }, "id": id }),
                            Ok(()) => match state.jobs.update_status(task_id, new_status, meta) {
                                Ok(true) => {
                                    let updated = state.jobs.get_by_id(task_id).ok().flatten();
                                    serde_json::json!({
                                        "jsonrpc": "2.0",
                                        "result": {
                                            "resultType": "complete",
                                            "taskId": task_id,
                                            "status": new_status,
                                            "updated": updated
                                        },
                                        "id": id
                                    })
                                }
                                Ok(false) => serde_json::json!({ "jsonrpc": "2.0", "error": { "code": -32602, "message": format!("task '{}' not found for update", task_id) }, "id": id }),
                                Err(e) => serde_json::json!({ "jsonrpc": "2.0", "error": { "code": -32603, "message": format!("database error: {}", e) }, "id": id }),
                            },
                        }
                    }
                    Err(e) => serde_json::json!({ "jsonrpc": "2.0", "error": { "code": -32603, "message": format!("database error: {}", e) }, "id": id }),
                }
            }
        }
        "tasks/cancel" => {
            let task_id = payload.get("params").and_then(|p| p.get("taskId").or_else(|| p.get("task_id"))).and_then(|v| v.as_str()).unwrap_or("");
            if task_id.is_empty() {
                serde_json::json!({ "jsonrpc": "2.0", "error": { "code": -32602, "message": "taskId parameter is required" }, "id": id })
            } else {
                match state.jobs.cancel(task_id) {
                    Ok(true) => serde_json::json!({
                        "jsonrpc": "2.0",
                        "result": { "resultType": "complete", "taskId": task_id, "status": "cancelled" },
                        "id": id
                    }),
                    Ok(false) => serde_json::json!({ "jsonrpc": "2.0", "error": { "code": -32602, "message": format!("task '{}' not found or already completed", task_id) }, "id": id }),
                    Err(e) => serde_json::json!({ "jsonrpc": "2.0", "error": { "code": -32603, "message": format!("database error: {}", e) }, "id": id }),
                }
            }
        }
        _ => serde_json::json!({ "jsonrpc": "2.0", "error": { "code": -32601, "message": format!("Method not found: {}", method) }, "id": id })
    };

    if stateless && response_json.get("result").is_some() {
        response_json["result"]["_meta"] = serde_json::json!({
            "io.modelcontextprotocol/serverInfo": {
                "name": "tylluan-nexus-sovereign",
                "version": env!("CARGO_PKG_VERSION")
            }
        });
    }

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
