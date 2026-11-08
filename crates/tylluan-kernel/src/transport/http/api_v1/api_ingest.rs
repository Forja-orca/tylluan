use axum::{
    Json,
    extract::{State, Query, Path, Multipart},
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;
use std::fs;
use std::net::IpAddr;
//
use crate::transport::http::HttpState;
use rmcp::model::CallToolRequestParam;
use uuid;

/// Validate a URL for SSRF safety: must be http/https, must not resolve to
/// a private/loopback/link-local IP. Returns Ok(()) if safe.
/// True for any address an ingest fetch must never reach: loopback,
/// unspecified, private (RFC 1918), link-local (incl. cloud metadata), IPv6
/// unique-local (fc00::/7) and link-local (fe80::/10). IPv4-mapped IPv6 is
/// judged by the embedded IPv4 address.
fn ip_is_blocked(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 64) // 100.64/10 CGNAT (+ Alibaba metadata)
                || (v4.octets()[0] == 192 && v4.octets()[1] == 0 && v4.octets()[2] == 0) // 192.0.0.0/24
                || (v4.octets()[0] == 198 && (v4.octets()[1] & 0xfe) == 18) // 198.18/15 benchmarking
                || v4.octets()[0] >= 240 // 240/4 reserved
        }
        IpAddr::V6(v6) => {
            // IPv4-mapped (::ffff:a.b.c.d) and IPv4-compatible (::a.b.c.d)
            if v6.is_loopback() || v6.is_unspecified() {
                return true;
            }
            if let Some(v4) = v6.to_ipv4() {
                return ip_is_blocked(IpAddr::V4(v4));
            }
            let seg = v6.segments();
            let first = seg[0];
            // NAT64 64:ff9b::/96 and 6to4 2002::/16 embed an IPv4 address.
            if seg[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
                let o = v6.octets();
                return ip_is_blocked(IpAddr::V4(std::net::Ipv4Addr::new(o[12], o[13], o[14], o[15])));
            }
            if first == 0x2002 {
                let o = v6.octets();
                return ip_is_blocked(IpAddr::V4(std::net::Ipv4Addr::new(o[2], o[3], o[4], o[5])));
            }
            v6.is_loopback()
                || v6.is_unspecified()
                || (first & 0xfe00) == 0xfc00 // fc00::/7 unique-local
                || (first & 0xffc0) == 0xfe80 // fe80::/10 link-local
        }
    }
}

/// Validate a URL for SSRF safety: must be http/https and must not resolve to
/// a blocked address. Fails closed: a hostname that cannot be resolved is
/// rejected, because the later fetch could resolve it differently. Redirects
/// are disabled on the fetch client, so only this URL is ever contacted.
///
/// For a domain host it returns `(name, vetted_ip)` so the caller can pin the
/// connection to the exact address that was checked (no second DNS lookup, so
/// no rebinding window). IP-literal hosts need no pinning and return `None`.
fn validate_url_for_ssrf(url_str: &str) -> Result<Option<(String, IpAddr)>, String> {
    let parsed = reqwest::Url::parse(url_str).map_err(|e| format!("Invalid URL: {e}"))?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(format!("Blocked scheme '{scheme}': only http and https allowed"));
    }
    let host = parsed.host_str().ok_or_else(|| "URL has no host".to_string())?;
    // `host_str` keeps the brackets of an IPv6 literal; strip them to parse it.
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    let ips: Vec<IpAddr> = match bare.parse::<IpAddr>() {
        Ok(ip) => vec![ip],
        Err(_) => std::net::ToSocketAddrs::to_socket_addrs(&(bare, 0))
            .map_err(|e| format!("Cannot resolve host '{bare}': {e}"))?
            .map(|a| a.ip())
            .collect(),
    };
    if ips.is_empty() {
        return Err("Host resolved to no addresses".to_string());
    }
    if let Some(ip) = ips.iter().find(|ip| ip_is_blocked(**ip)) {
        return Err(format!("Blocked private IP: {ip}"));
    }
    if bare.parse::<IpAddr>().is_ok() {
        return Ok(None);
    }
    Ok(Some((bare.to_string(), ips[0])))
}

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
        .map(|e| format!(".{e}"))
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

/// D-55: resuelve un fichero de `data/ingest` sin permitir que el nombre
/// sustituya la base. En Windows `C:foo` es una ruta relativa a la unidad C:,
/// no un nombre de fichero; se rechaza `:` además de `..`, `/` y `\`.
fn resolve_ingest_file_path_in(
    base: &std::path::Path,
    filename: &str,
) -> Result<std::path::PathBuf, StatusCode> {
    if filename.is_empty()
        || filename.contains("..")
        || filename.contains('/')
        || filename.contains('\\')
        || filename.contains(':')
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let base = base.canonicalize().map_err(|_| StatusCode::NOT_FOUND)?;
    let resolved = base
        .join(filename)
        .canonicalize()
        .map_err(|_| StatusCode::NOT_FOUND)?;
    if !resolved.starts_with(&base) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(resolved)
}

/// Extensions that may be rendered inline. Everything else is forced to
/// download. `svg`/`svgz`/`html`/`xml` are deliberately ABSENT: a document
/// of those types opened top-level from this route runs its own script in
/// the kernel's origin, which is the same origin the dashboard keeps its
/// bearer token in (FILES-SVG-SAMEORIGIN). `txt` stays inline because it is
/// served as `text/plain` with `nosniff`, which cannot become a document.
const INLINE_SAFE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "pdf", "txt"];

fn extension_of(filename: &str) -> String {
    std::path::Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default()
}

fn mime_for_extension(extension: &str) -> &'static str {
    match extension {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        // No entry for svg/svgz/html/htm/xml: they leave as
        // application/octet-stream and the headers below force a download.
        _ => "application/octet-stream",
    }
}

/// ASCII-only name for `Content-Disposition`. Anything unexpected collapses
/// to `download`, so a crafted file name cannot inject header bytes or a
/// second `filename=` parameter.
fn disposition_filename(filename: &str) -> String {
    let leaf = std::path::Path::new(filename)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let cleaned: String = leaf
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        .take(120)
        .collect();
    let cleaned = cleaned.trim_matches('.');
    if cleaned.is_empty() {
        "download".to_string()
    } else {
        cleaned.to_string()
    }
}

/// Response headers for a served ingest file. Kept apart from the handler so
/// the security contract is testable without touching the filesystem.
///
/// * `X-Content-Type-Options: nosniff` always — without it a browser may
///   sniff an `application/octet-stream` body into an SVG/XML/HTML document
///   and execute it in this origin.
/// * `Content-Security-Policy: sandbox` always — belt and braces for the
///   `inline` types (a PDF's own viewer scripting).
/// * `Content-Disposition: attachment` unless the extension is in
///   `INLINE_SAFE_EXTENSIONS`.
fn file_serving_headers(
    filename: &str,
    extension: &str,
) -> Vec<(axum::http::HeaderName, axum::http::HeaderValue)> {
    let inline = INLINE_SAFE_EXTENSIONS.contains(&extension);
    let disposition = if inline { "inline" } else { "attachment" };
    let name = disposition_filename(filename);
    let headers: Vec<(axum::http::HeaderName, axum::http::HeaderValue)> = vec![
        (
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static(mime_for_extension(extension)),
        ),
        (
            axum::http::header::X_CONTENT_TYPE_OPTIONS,
            axum::http::HeaderValue::from_static("nosniff"),
        ),
        (
            axum::http::HeaderName::from_static("content-security-policy"),
            axum::http::HeaderValue::from_static(
                "sandbox; default-src 'none'; form-action 'none'; frame-ancestors 'none'",
            ),
        ),
        (
            axum::http::header::CONTENT_DISPOSITION,
            axum::http::HeaderValue::from_str(&format!("{disposition}; filename=\"{name}\""))
                .unwrap_or_else(|_| axum::http::HeaderValue::from_static("attachment")),
        ),
    ];
    headers
}

/// GET /api/v1/ingest/files/{filename}
/// Serves uploaded files from the data/ingest directory.
pub async fn serve_ingested_file_handler(
    Path(filename): Path<String>,
) -> impl IntoResponse {
    let filepath = match resolve_ingest_file_path_in(std::path::Path::new("data/ingest"), &filename) {
        Ok(path) => path,
        Err(status) => return (status, "Invalid filename").into_response(),
    };
    match tokio::fs::read(&filepath).await {
        Ok(bytes) => {
            let extension = extension_of(&filename);
            let headers: axum::http::HeaderMap =
                file_serving_headers(&filename, &extension).into_iter().collect();
            (StatusCode::OK, headers, bytes).into_response()
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
            let pinned = match validate_url_for_ssrf(url_str) {
                Ok(p) => p,
                Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": e }))).into_response(),
            };
            // No redirects: validate_url_for_ssrf only vetted the original URL, so
            // following a 302 would let a remote server aim the fetch inward.
            let mut builder = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .redirect(reqwest::redirect::Policy::none());
            if let Some((name, ip)) = pinned {
                // Connect to the address we vetted instead of resolving again.
                builder = builder.resolve(&name, std::net::SocketAddr::new(ip, 0));
            }
            let client = builder
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

    // D-17/H5: `ingest` guarda DOCUMENTOS, y un documento legítimo puede hablar
    // de inyección de prompts (un paper, un cheat-sheet, este mismo informe) —
    // rechazarlo daría falsos positivos. La decisión del TL es marcarlo: la capa 1
    // deja `untrusted_content: true` en el metadata y el juez decide después.
    // Antes de esto, B-9 midió 8/8 muestras de inyección guardadas SIN marca.
    // D-32: la marca vive en `security::trust::mark_if_layer1_flagged`, que es la
    // misma regla que aplican el resto de vías de escritura.
    let mut metadata_value = serde_json::json!({
        "source": "ingest_pipeline",
        "filename": file_name,
        "content_type": content_type,
        "tags": tags_owned,
        "context": q.context,
    });
    let _layer1_flagged =
        crate::security::trust::mark_if_layer1_flagged(&mut metadata_value, &content, "ingest");
    let metadata = metadata_value.to_string();

    let upsert_result = silva.upsert_node(&node_id, &node_type_owned, &content, &metadata).await;

    let node_id_final = node_id.clone();
    let wrote_ok = upsert_result.is_ok();
    let final_status: &str = match upsert_result {
        Ok(()) => "ok",
        Err(e) => {
            tracing::warn!("ingest upsert_node failed: {}", e);
            "degraded"
        }
    };

    // D-17: capa 2 en la ruta de ingesta (fire-and-forget, post-escritura).
    if wrote_ok {
        crate::security::write_gate::spawn_write_gate_judge(
            silva.clone(),
            node_id_final.clone(),
            content.clone(),
        );
    }

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
                        // D-32 (matiz 5 de T-DSH-33): la descripción de visión es
                        // otra vía de escritura (contenido derivado de un fichero
                        // que subió el usuario) y salía sin capa 1 ni juez.
                        let mut meta_value = serde_json::json!({"source": "vision", "image_path": img_path});
                        crate::security::trust::mark_if_layer1_flagged(&mut meta_value, vision_desc, "vision_description");
                        let meta = meta_value.to_string();
                        let vision_node_id = format!("vision_desc__{}", uuid::Uuid::new_v4().simple());
                        if silva.upsert_node(&vision_node_id, "vision_description", vision_desc, &meta).await.is_ok() {
                            crate::security::write_gate::spawn_write_gate_judge(
                                silva.clone(),
                                vision_node_id.clone(),
                                vision_desc.to_string(),
                            );
                        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssrf_blocks_loopback() {
        assert!(validate_url_for_ssrf("http://127.0.0.1/secret").is_err());
        assert!(validate_url_for_ssrf("http://localhost/secret").is_err());
    }

    #[test]
    fn test_ssrf_blocks_link_local_metadata_endpoint() {
        // AWS/GCP/Azure cloud metadata endpoint -- the canonical SSRF target.
        assert!(validate_url_for_ssrf("http://169.254.169.254/latest/meta-data/").is_err());
    }

    #[test]
    fn test_ssrf_blocks_private_rfc1918_ranges() {
        assert!(validate_url_for_ssrf("http://10.0.0.1/").is_err());
        assert!(validate_url_for_ssrf("http://172.16.0.1/").is_err());
        assert!(validate_url_for_ssrf("http://192.168.1.1/").is_err());
    }

    #[test]
    fn test_ssrf_blocks_non_http_schemes() {
        assert!(validate_url_for_ssrf("file:///etc/passwd").is_err());
        assert!(validate_url_for_ssrf("gopher://example.com/").is_err());
    }

    #[test]
    fn test_ssrf_blocks_ipv6_private_ranges() {
        for u in [
            "http://[::1]/",
            "http://[fd00::1]/",        // unique-local fc00::/7
            "http://[fe80::1]/",        // link-local fe80::/10
            "http://[::ffff:169.254.169.254]/", // IPv4-mapped metadata
            "http://[::ffff:10.0.0.1]/",
            "http://0.0.0.0/",
        ] {
            assert!(validate_url_for_ssrf(u).is_err(), "must block {u}");
        }
    }

    #[test]
    fn test_ssrf_blocks_reserved_and_embedded_ipv4_ranges() {
        for u in [
            "http://100.100.100.200/",      // CGNAT / Alibaba metadata
            "http://198.18.0.1/",
            "http://192.0.0.8/",
            "http://240.0.0.1/",
            "http://[64:ff9b::a9fe:a9fe]/", // NAT64 -> 169.254.169.254
            "http://[2002:a9fe:a9fe::1]/",  // 6to4 -> 169.254.169.254
            "http://[::7f00:1]/",           // IPv4-compatible 127.0.0.1
            "http://2130706433/",           // decimal form of 127.0.0.1
            "http://0x7f.1/",
        ] {
            assert!(validate_url_for_ssrf(u).is_err(), "must block {u}");
        }
    }

    #[test]
    fn test_ssrf_ip_literal_needs_no_pin() {
        assert!(matches!(validate_url_for_ssrf("https://93.184.216.34/"), Ok(None)));
    }

    #[test]
    fn test_ssrf_fails_closed_when_host_does_not_resolve() {
        // `.invalid` is reserved (RFC 2606) and never resolves.
        assert!(validate_url_for_ssrf("http://no-such-host.invalid/").is_err());
    }

    #[test]
    fn test_ip_is_blocked_leaves_public_addresses_alone() {
        assert!(!ip_is_blocked("93.184.216.34".parse().unwrap()));
        assert!(!ip_is_blocked("100.63.255.255".parse().unwrap()));
        assert!(!ip_is_blocked("2606:2800:220:1:248:1893:25c8:1946".parse().unwrap()));
    }

    #[test]
    fn test_ssrf_allows_public_https() {
        // A public IP literal: no DNS needed, so the test is hermetic.
        assert!(validate_url_for_ssrf("https://93.184.216.34/").is_ok());
    }

    #[test]
    fn ingest_file_path_rejects_windows_drive_and_stream_names() {
        let base = std::path::Path::new("data/ingest");
        for bad in ["C:foo", "a:b", "..\\evil", "../evil", "", "/etc/passwd"] {
            assert_eq!(
                resolve_ingest_file_path_in(base, bad).unwrap_err(),
                StatusCode::BAD_REQUEST,
                "{bad:?} must be rejected before touching the filesystem"
            );
        }
    }

    #[test]
    fn ingest_file_path_serves_benign_file_inside_base() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ok.txt"), b"hello").unwrap();
        let resolved = resolve_ingest_file_path_in(dir.path(), "ok.txt").unwrap();
        assert_eq!(std::fs::read(&resolved).unwrap(), b"hello");
    }

    fn header_value(
        headers: &[(axum::http::HeaderName, axum::http::HeaderValue)],
        key: &str,
    ) -> Option<String> {
        headers
            .iter()
            .find(|(k, _)| k.as_str() == key)
            .and_then(|(_, v)| v.to_str().ok())
            .map(|s| s.to_string())
    }

    #[test]
    fn svg_html_and_unknown_uploads_are_forced_to_download() {
        for name in [
            "diagram.svg", "BAD.SVG", "page.html", "page.htm", "feed.xml",
            "archive.svgz", "double.html.svg", "no_extension", "script.js",
        ] {
            let headers = file_serving_headers(name, &extension_of(name));
            let cd = header_value(&headers, "content-disposition").unwrap_or_default();
            assert!(
                cd.starts_with("attachment;"),
                "{name:?}: must force a download, got {cd:?}"
            );
            assert_eq!(
                header_value(&headers, "content-type").as_deref(),
                Some("application/octet-stream"),
                "{name:?}: must not be given a renderable media type"
            );
        }
    }

    #[test]
    fn served_files_always_get_nosniff_and_a_sandboxed_csp() {
        for name in [
            "diagram.svg", "report.pdf", "photo.png", "notes.txt",
            "no_extension", "page.html",
        ] {
            let headers = file_serving_headers(name, &extension_of(name));
            assert_eq!(
                header_value(&headers, "x-content-type-options").as_deref(),
                Some("nosniff"),
                "{name:?}: without nosniff a browser may sniff the body into a document"
            );
            let csp = header_value(&headers, "content-security-policy").unwrap_or_default();
            assert!(
                csp.contains("sandbox") && csp.contains("default-src 'none'"),
                "{name:?}: CSP must sandbox the document, got {csp:?}"
            );
        }
    }

    #[test]
    fn inline_is_limited_to_images_pdf_and_plain_text() {
        for name in ["a.png", "b.jpg", "c.JPEG", "d.gif", "e.webp", "f.pdf", "g.txt"] {
            let cd = header_value(&file_serving_headers(name, &extension_of(name)), "content-disposition")
                .unwrap_or_default();
            assert!(cd.starts_with("inline;"), "{name:?}: expected inline, got {cd:?}");
        }
        for name in ["a.svg", "a.html", "a.bin", "a"] {
            let cd = header_value(&file_serving_headers(name, &extension_of(name)), "content-disposition")
                .unwrap_or_default();
            assert!(!cd.starts_with("inline;"), "{name:?}: must not render inline, got {cd:?}");
        }
    }

    #[test]
    fn svg_never_gets_a_renderable_mime_type() {
        for ext in ["svg", "svgz", "html", "htm", "xml", "xhtml", "js", "css", "swf", ""] {
            assert_eq!(
                mime_for_extension(ext),
                "application/octet-stream",
                "{ext:?} must stay opaque; inline SVG/HTML would execute in this origin"
            );
        }
    }

    #[test]
    fn content_disposition_filename_cannot_smuggle_header_bytes() {
        for name in [
            "a\"; x=1\r\nSet-Cookie: evil=1",
            "ok.png\", ; filename=\"second.html",
            "..",
            "....png",
            "señor.svg",
            "a",
        ] {
            let headers = file_serving_headers(name, &extension_of(name));
            let cd = header_value(&headers, "content-disposition").unwrap_or_default();
            assert!(
                !cd.contains('\r') && !cd.contains('\n'),
                "{name:?}: CR/LF in a header value: {cd:?}"
            );
            assert_eq!(
                cd.matches('"').count(),
                2,
                "{name:?}: the quoted filename must stay a single token: {cd:?}"
            );
        }
    }
}
