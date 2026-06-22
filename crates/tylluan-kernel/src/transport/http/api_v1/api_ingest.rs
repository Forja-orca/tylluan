use axum::{
    Json,
    extract::{State, Query, Path, Multipart},
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;
use std::fs;
//
use crate::transport::http::HttpState;
use rmcp::model::CallToolRequestParam;
use uuid;

#[derive(serde::Deserialize)]
pub struct IngestQuery {
    pub tags: Option<String>,
    pub context: Option<String>,
    pub node_type: Option<String>,
}

pub async fn ingest_upload_handler(
    State(state): State<Arc<HttpState>>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let mut file_bytes: Option<axum::body::Bytes> = None;
    let mut original_name = "upload".to_string();
    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("file") {
            original_name = field.file_name().unwrap_or("upload").to_string();
            match field.bytes().await {
                Ok(b) => file_bytes = Some(b),
                Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("read bytes: {}", e)}))).into_response(),
            }
            break;
        }
    }
    let file_bytes = match file_bytes {
        Some(b) => b,
        None => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "missing file field"}))).into_response(),
    };
    let extension = std::path::Path::new(&original_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e))
        .unwrap_or_default();
    if file_bytes.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "empty body"}))).into_response();
    }
    let ingest_dir = std::path::Path::new("data/ingest");
    if let Err(e) = fs::create_dir_all(ingest_dir) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("create_dir_all: {}", e)}))).into_response();
    }
    let base_name = original_name.rsplit_once('.').map(|(n, _)| n).unwrap_or(&original_name);
    let safe_name = base_name.chars().map(|c: char| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect::<String>();
    let filename = format!("upload_{}_{}{}", chrono::Utc::now().timestamp_millis(), safe_name, extension);
    let filepath = ingest_dir.join(&filename);
    if let Err(e) = fs::write(&filepath, &file_bytes) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("write: {}", e)}))).into_response();
    }
    let path_str = filepath.to_string_lossy().to_string();
    let ext_lower = extension.to_lowercase();
    let is_image = matches!(ext_lower.as_str(), ".png" | ".jpg" | ".jpeg" | ".webp" | ".bmp" | ".gif");
    let reg = state.registry.clone();
    // Fail fast: check guild availability before spawning
    if is_image {
        if let Err(e) = reg.ensure_running("vision").await {
            return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"error": format!("vision guild unavailable: {}", e)}))).into_response();
        }
    } else if let Err(e) = reg.ensure_running("ingest").await {
        return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"error": format!("ingest guild unavailable: {}", e)}))).into_response();
    }
    tokio::spawn(async move {
        if is_image {
            let params = CallToolRequestParam {
                name: "vision_analyze".into(),
                arguments: Some(serde_json::json!({
                    "image_path": path_str,
                    "prompt": "Describe this image in detail. Extract any text visible (OCR). Identify objects, people, diagrams, or data.",
                    "query": path_str
                }).as_object().cloned().unwrap_or_default()),
            };
            if let Err(e) = reg.call_tool("vision", params).await {
                tracing::error!("❌ vision ingest failed: {}", e);
            }
        } else {
            let params = CallToolRequestParam {
                name: "ingest_file".into(),
                arguments: Some(serde_json::json!({"path": path_str, "query": path_str}).as_object().cloned().unwrap_or_default()),
            };
            if let Err(e) = reg.call_tool("ingest", params).await {
                tracing::error!("❌ text ingest failed: {}", e);
            }
        }
    });
    (StatusCode::OK, Json(serde_json::json!({"status": "ingested", "file": filename, "original_name": original_name, "pipeline": if is_image { "vision" } else { "ingest" }}))).into_response()
}

/// GET /api/v1/ingest/files/{filename}
/// Serves uploaded files from the data/ingest directory.
pub async fn serve_ingested_file_handler(
    Path(filename): Path<String>,
) -> impl IntoResponse {
    // Prevent directory traversal attacks
    if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        return (StatusCode::BAD_REQUEST, "Invalid filename").into_response();
    }
    let filepath = std::path::Path::new("data/ingest").join(&filename);
    match tokio::fs::read(&filepath).await {
        Ok(bytes) => {
            let extension = std::path::Path::new(&filename)
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .unwrap_or_default();
            let mime = match extension.as_str() {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "webp" => "image/webp",
                "pdf" => "application/pdf",
                "txt" => "text/plain",
                _ => "application/octet-stream",
            };
            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, mime)],
                bytes,
            ).into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "File not found").into_response(),
    }
}

pub async fn ingest_handler(
    State(state): State<Arc<HttpState>>,
    Query(q): Query<IngestQuery>,
    req: axum::extract::Request,
) -> impl IntoResponse {
    let headers = req.headers().clone();
    let content_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");

    let mut text_content: Option<String> = None;
    let mut file_bytes: Option<axum::body::Bytes> = None;
    let mut file_name: String = "upload".to_string();
    let mut mime_type: Option<String> = None;
    let mut tags: Vec<String> = q.tags
        .as_ref()
        .map(|s| s.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect())
        .unwrap_or_default();

    if content_type.contains("application/json") {
        let body_bytes = match axum::body::to_bytes(req.into_body(), 10 * 1024 * 1024).await {
            Ok(b) => b,
            Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": format!("Failed to read body: {}", e) }))).into_response(),
        };
        let json_val: serde_json::Value = match serde_json::from_slice(&body_bytes) {
            Ok(v) => v,
            Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": format!("Invalid JSON: {}", e) }))).into_response(),
        };

        if let Some(url_str) = json_val.get("url").and_then(|v| v.as_str()) {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap_or_default();
            match client.get(url_str).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        match resp.text().await {
                            Ok(body_text) => {
                                text_content = Some(body_text);
                                file_name = url_str.to_string();
                            }
                            Err(e) => return (StatusCode::BAD_GATEWAY, Json(serde_json::json!({ "error": format!("Failed to read response body from URL: {}", e) }))).into_response(),
                        }
                    } else {
                        return (StatusCode::BAD_GATEWAY, Json(serde_json::json!({ "error": format!("URL returned status: {}", resp.status()) }))).into_response();
                    }
                }
                Err(e) => return (StatusCode::BAD_GATEWAY, Json(serde_json::json!({ "error": format!("Failed to fetch URL: {}", e) }))).into_response(),
            }
        } else if let Some(text_str) = json_val.get("text").and_then(|v| v.as_str()) {
            text_content = Some(text_str.to_string());
        } else {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "JSON payload must contain either 'text' or 'url'" }))).into_response();
        }

        if let Some(tags_arr) = json_val.get("tags").and_then(|v| v.as_array()) {
            for t in tags_arr {
                if let Some(t_str) = t.as_str() {
                    tags.push(t_str.trim().to_string());
                }
            }
        }
    } else {
        use axum::extract::FromRequest;
        let mut multipart = match Multipart::from_request(req, &state).await {
            Ok(m) => m,
            Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
        };

        while let Ok(Some(field)) = multipart.next_field().await {
            let name = field.name().unwrap_or("").to_string();
            if name == "text" {
                if let Ok(bytes) = field.bytes().await {
                    text_content = Some(String::from_utf8_lossy(&bytes).to_string());
                }
            } else if name == "file" {
                file_name = field.file_name().unwrap_or("upload").to_string();
                mime_type = field.content_type().map(|s| s.to_string());
                if let Ok(b) = field.bytes().await {
                    file_bytes = Some(b);
                }
            }
        }
    }

    let content: String;
    let content_type: &str;

    if let Some(bytes) = file_bytes {
        if !bytes.is_empty() {
            let ext = std::path::Path::new(&file_name)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            if ext == "pdf" || mime_type.as_deref() == Some("application/pdf") {
                content_type = "pdf";
                let ingest_dir = std::path::Path::new("data/ingest");
                let _ = fs::create_dir_all(ingest_dir);
                let filename = format!("ingest_{}_{}.pdf", chrono::Utc::now().timestamp_millis(), uuid::Uuid::new_v4().simple());
                let filepath = ingest_dir.join(&filename);
                if fs::write(&filepath, &bytes).is_ok() {
                    content = format!("[PDF uploaded: {}]", filepath.to_string_lossy());
                } else {
                    content = "[PDF upload failed]".to_string();
                }
            } else if mime_type.as_ref().map(|m| m.starts_with("image/")).unwrap_or(false)
                || ["jpg", "jpeg", "png", "gif", "webp", "bmp"].contains(&ext.as_str())
            {
                content_type = "image";
                let ingest_dir = std::path::Path::new("data/ingest");
                let _ = fs::create_dir_all(ingest_dir);
                let ext2 = std::path::Path::new(&file_name)
                    .extension().and_then(|e| e.to_str()).unwrap_or("png");
                let filename = format!("ingest_{}_{}.{}", chrono::Utc::now().timestamp_millis(), uuid::Uuid::new_v4().simple(), ext2);
                let filepath = ingest_dir.join(&filename);
                if fs::write(&filepath, &bytes).is_ok() {
                    content = format!("[IMAGE uploaded: {}]", filepath.to_string_lossy());
                } else {
                    content = "[Image upload failed]".to_string();
                }
            } else {
                content_type = "document";
                content = String::from_utf8_lossy(&bytes).to_string();
            }
        } else if let Some(t) = text_content.take() {
            content = t;
            content_type = "text";
        } else {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
                "error": "Empty file or no content provided"
            }))).into_response();
        }
    } else if let Some(t) = text_content {
        content = t;
        content_type = "text";
    } else {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "Provide either 'file' or 'text' field"
        }))).into_response();
    }

    if content.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "Content is empty"
        }))).into_response();
    }

    let node_type = q.node_type.clone().unwrap_or_else(|| content_type.to_string());
    let content_preview = content.chars().take(200).collect::<String>();
    let warnings: Vec<String> = vec![];

    let silva = state.silva.clone();
    let _registry = state.registry.clone();
    let tags_owned = tags.clone();
    let node_type_owned = node_type.clone();

    let node_id: String = format!("{}__{}", node_type.clone(), uuid::Uuid::new_v4().simple());

    let metadata = serde_json::json!({
        "source": "ingest_pipeline",
        "filename": file_name,
        "content_type": content_type,
        "tags": tags_owned,
        "context": q.context,
    }).to_string();

    let upsert_result = silva.upsert_node(&node_id, &node_type_owned, &content, &metadata).await;

    let node_id_final = node_id.clone();
    let final_status: &str = match upsert_result {
        Ok(()) => "ok",
        Err(e) => {
            tracing::warn!("ingest upsert_node failed: {}", e);
            "degraded"
        }
    };

    let mut triples_extracted: usize = 0;

    let _ = state.registry.ensure_running("knowledge").await;
    {
        let params = CallToolRequestParam {
            name: "extract_triples".into(),
            arguments: Some(serde_json::json!({
                "text": content.clone(),
                "context": q.context.clone().unwrap_or_default(),
                "max_triples": 5
            }).as_object().cloned().unwrap_or_default()),
        };
        if let Ok(res) = state.registry.call_tool("knowledge", params).await {
            for c in res.content {
                if let Some(text) = c.as_text()
                    && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text.text)
                        && let Some(triples) = parsed.get("triples").and_then(|v| v.as_array()) {
                            triples_extracted = triples.len();
                            for triple in triples.iter().filter_map(|t| t.as_object()) {
                                let s = triple.get("subject").and_then(|v| v.as_str()).unwrap_or("");
                                let p = triple.get("predicate").and_then(|v| v.as_str()).unwrap_or("relates_to");
                                let o = triple.get("object").and_then(|v| v.as_str()).unwrap_or("");
                                if !s.is_empty() && !o.is_empty() {
                                    let _ = silva.add_edge(s, o, p, 0.8, "{}").await;
                                }
                            }
                        }
            }
        }
    }

    if final_status == "ok" && content_type == "image"
    {
        let _ = state.registry.ensure_running("vision").await;
        let img_path = content.trim_start_matches("[IMAGE uploaded: ").trim_end_matches(']');
        let params = CallToolRequestParam {
            name: "vision_analyze".into(),
            arguments: Some(serde_json::json!({
                "image_path": img_path,
                "prompt": "Describe this image concisely. Focus on what it contains, who or what is in it, and any notable visual elements."
            }).as_object().cloned().unwrap_or_default()),
        };
        if let Ok(res) = state.registry.call_tool("vision", params).await {
            for c in res.content {
                if let Some(text) = c.as_text() {
                    let vision_desc = text.text.trim();
                    if !vision_desc.is_empty() && !vision_desc.starts_with('[') {
                        let meta = serde_json::json!({"source": "vision", "image_path": img_path}).to_string();
                        let vision_node_id = format!("vision_desc__{}", uuid::Uuid::new_v4().simple());
                        let _ = silva.upsert_node(&vision_node_id, "vision_description", vision_desc, &meta).await;
                        let _ = silva.add_edge(&node_id_final, &vision_node_id, "described_as", 1.0, "{}").await;
                    }
                }
            }
        }
    }

    (StatusCode::OK, Json(serde_json::json!({
        "node_id": node_id_final,
        "type": content_type,
        "content_preview": content_preview,
        "triples_extracted": triples_extracted,
        "embedding_dims": 1024,
        "status": final_status,
        "warnings": warnings
    }))).into_response()
}
