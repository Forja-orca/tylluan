use axum::{
    Json,
    extract::{State, Path as AxumPath},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use std::fs;
use serde::Serialize;
use serde_json;
use crate::require_server;
use crate::transport::http::{
    HttpState, BashExecuteRequest,
};
use rmcp::model::CallToolRequestParam;

#[derive(Serialize)]
pub struct DockerStatusResponse {
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

pub async fn bash_execute_handler(State(state): State<Arc<HttpState>>, Json(req): Json<BashExecuteRequest>) -> Response {
    // Redirigir al gremio bash de forma segura a travÃ©s de tylluan_do
    let server_arc = require_server!(state);
    let server = server_arc.read().await;

    let call = CallToolRequestParam {
        name: "bash_execute".into(),
        arguments: serde_json::json!({ "command": req.command }).as_object().cloned(),
    };
    
    match server.handle_call_internal(call, tylluan_common::types::Channel::Http { authenticated: true }, "http-bash-session").await {
        Ok(res) => {
            let stdout = serde_json::to_string(&res.content).unwrap_or_default();
            Json(serde_json::json!({ "stdout": stdout })).into_response()
        },
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
    }
}

pub async fn docker_containers_handler() -> impl IntoResponse {
    use tokio::process::Command;
    use tokio::time::{timeout, Duration};

    let result = timeout(
        Duration::from_secs(3),
        Command::new("docker")
            .args(["version", "--format", "{{.Server.Version}}"])
            .output()
    ).await;

    match result {
        Ok(Ok(output)) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Json(DockerStatusResponse {
                status: "online",
                error: None,
                version: if version.is_empty() { None } else { Some(version) },
            }).into_response()
        }
        Ok(Ok(output)) => {
            let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
            Json(DockerStatusResponse {
                status: "offline",
                error: Some(if err.is_empty() { "Docker daemon not responding".into() } else { err }),
                version: None,
            }).into_response()
        }
        Ok(Err(e)) => Json(DockerStatusResponse {
            status: "offline",
            error: Some(format!("docker binary not found: {e}")),
            version: None,
        }).into_response(),
        Err(_) => Json(DockerStatusResponse {
            status: "offline",
            error: Some("docker command timed out (3s)".into()),
            version: None,
        }).into_response(),
    }
}

pub async fn security_events_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    Json(state.doctor.diagnose().await)
}

pub async fn golden_signals_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let diag = state.doctor.diagnose().await;
    let statuses = state.registry.status_all().await.unwrap_or_default();
    
    let online = statuses.iter().filter(|s| s.running).count();
    let total_tools: usize = statuses.iter().map(|s| s.tools_count).sum();
    let edge_count = state.silva.edge_count().await.unwrap_or(0);
    
    Json(serde_json::json!({
        "traffic": {
            "active_guilds": online,
            "total_guilds": statuses.len(),
            "active_tools": total_tools
        },
        "errors": {
            "rate_percent": if diag.status == "healthy" { 0 } else if diag.status == "degraded" { 5 } else { 20 },
            "total_errors": 0,
            "critical": diag.status == "critical"
        },
        "saturation": {
            "memory_percent": diag.system.memory_percent.round(),
            "storage_percent": 0,
            "node_count": diag.storage.nodes_count,
            "edge_count": edge_count
        },
        "uptime_seconds": state.start_time.elapsed().as_secs(),
        "slo_target": 99.9,
        "status": {
            "guilds_online": online,
            "guilds_total": statuses.len(),
            "nodes": diag.storage.nodes_count,
            "edges": edge_count
        }
    }))
}

pub async fn logs_handler() -> impl IntoResponse {
    let log_path = "logs/kernel.log";
    let content = fs::read_to_string(log_path).unwrap_or_else(|_| "No kernel log found".to_string());
    let lines: Vec<&str> = content.lines().rev().take(200).collect();
    let reversed_lines: Vec<&str> = lines.into_iter().rev().collect();
    Json(serde_json::json!({
        "logs": reversed_lines,
        "path": log_path,
        "ts": chrono::Utc::now().to_rfc3339()
    }))
}

pub async fn sandbox_sessions_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let guilds: Vec<serde_json::Value> = state.registry.list_guilds(None).await.unwrap_or_default();
    let running_guilds: Vec<_> = guilds.into_iter().filter(|g| g["running"].as_bool().unwrap_or(false)).collect();
    Json(serde_json::json!({
        "sessions": running_guilds,
        "count": running_guilds.len(),
        "ts": chrono::Utc::now().to_rfc3339()
    }))
}

/// GET /api/v1/sandbox/files/{path}
/// Serves files from the workspace root with path traversal protection.
/// Rejects paths that resolve outside the workspace root (canonicalize-based).
pub async fn sandbox_files_handler(
    AxumPath(file_path): AxumPath<String>,
) -> impl IntoResponse {
    let workspace_root = find_workspace_root();
    // Axum URL-decodes path parameters automatically — no percent_encoding needed.
    let decoded_path = std::path::Path::new(&file_path);

    // Resolve and canonicalize (handles .. and symlinks)
    let resolved = if decoded_path.is_absolute() {
        decoded_path.to_path_buf()
    } else {
        workspace_root.join(decoded_path)
    };
    match resolved.canonicalize() {
        Ok(canonical) => {
            // Verify the resolved path is within workspace root
            if !canonical.starts_with(&workspace_root) {
                return (StatusCode::FORBIDDEN, "Access denied: path outside workspace root").into_response();
            }
            match tokio::fs::read(&canonical).await {
                Ok(bytes) => {
                    let ext = canonical.extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.to_lowercase())
                        .unwrap_or_default();
                    let mime = match ext.as_str() {
                        "png" => "image/png",
                        "jpg" | "jpeg" => "image/jpeg",
                        "gif" => "image/gif",
                        "webp" => "image/webp",
                        "svg" => "image/svg+xml",
                        "pdf" => "application/pdf",
                        "txt" | "md" => "text/plain",
                        "html" | "htm" => "text/html",
                        "css" => "text/css",
                        "js" => "application/javascript",
                        "json" => "application/json",
                        "woff2" => "font/woff2",
                        "woff" => "font/woff",
                        "ttf" => "font/ttf",
                        "otf" => "font/otf",
                        "ico" => "image/x-icon",
                        "wasm" => "application/wasm",
                        _ => "application/octet-stream",
                    };
                    (StatusCode::OK, [(axum::http::header::CONTENT_TYPE, mime)], bytes).into_response()
                }
                Err(_) => (StatusCode::NOT_FOUND, "File not found").into_response(),
            }
        }
        Err(_) => (StatusCode::NOT_FOUND, "File not found").into_response(),
    }
}

fn find_workspace_root() -> std::path::PathBuf {
    let mut root = std::env::current_dir().unwrap_or_default();
    for _ in 0..5 {
        if root.join("tylluan.toml").exists() { return root; }
        if let Some(parent) = root.parent() { root = parent.to_path_buf(); } else { break; }
    }
    std::env::current_dir().unwrap_or_default()
}
