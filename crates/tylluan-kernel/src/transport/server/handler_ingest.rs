use rmcp::{Error as McpError, model::*};
use serde_json;

use crate::registry::proxy::error_result;
use super::TylluanServer;

fn chunk_text(text: &str, size: usize, overlap: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let end = (start + size).min(chars.len());
        chunks.push(chars[start..end].iter().collect());
        if end == chars.len() { break; }
        start += size - overlap;
    }
    chunks
}

pub async fn handle_tylluan_ingest(
    server: &TylluanServer,
    arguments: Option<serde_json::Map<String, serde_json::Value>>,
) -> Result<CallToolResult, McpError> {
    let source = arguments.as_ref()
        .and_then(|a| a.get("source")).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let name = arguments.as_ref()
        .and_then(|a| a.get("name")).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let ingest_type = arguments.as_ref()
        .and_then(|a| a.get("ingest_type")).and_then(|v| v.as_str()).unwrap_or("text").to_string();
    let content = arguments.as_ref()
        .and_then(|a| a.get("content")).and_then(|v| v.as_str()).unwrap_or("").to_string();

    if source.trim().is_empty() {
        return Ok(error_result("source is required"));
    }
    if name.trim().is_empty() {
        return Ok(error_result("name is required"));
    }

    let (text_to_ingest, actual_source) = match ingest_type.as_str() {
        "text" => {
            if content.trim().is_empty() {
                return Ok(error_result("content is required when ingest_type is 'text'"));
            }
            (content.clone(), format!("text:{}", name))
        }
        "file" => {
            let path = std::path::Path::new(&source);
            if !path.exists() {
                return Ok(error_result(&format!("File does not exist: {}", source)));
            }
            match tokio::fs::read_to_string(&source).await {
                Ok(text) => (text, format!("file:{}", name)),
                Err(e) => return Ok(error_result(&format!("Failed to read file: {}", e))),
            }
        }
        "url" => {
            return Ok(error_result("URL ingest requires network guild — use tylluan_do with intent 'fetch URL'"));
        }
        _ => {
            return Ok(error_result("ingest_type must be 'text', 'file', or 'url'"));
        }
    };

    let chunks = chunk_text(&text_to_ingest, 1000, 100);
    let mut node_ids = Vec::new();
    let mut total_triples = 0;

    let chunk_type = if ingest_type == "text" { "ingested" } else { "chunk" };

    for (idx, chunk) in chunks.iter().enumerate() {
        let node_id = format!("ingested:{}:{}", name, idx);
        let meta = serde_json::json!({
            "source": actual_source,
            "chunk_index": idx,
            "total_chunks": chunks.len(),
        }).to_string();

        if let Err(e) = server.silva.upsert_node(&node_id, chunk_type, chunk, &meta).await {
            tracing::warn!("Failed to insert chunk {}: {}", node_id, e);
            continue;
        }

        node_ids.push(node_id.clone());

        let triples = crate::memory::triple_extractor::extract_triples_local(chunk);
        total_triples += triples.len();

        for (subject, predicate, object) in triples {
            let _ = server.silva.add_edge(&subject, &object, &predicate, 0.8, &format!(r#"{{"source_node":"{}"}}"#, node_id)).await;
        }
    }

    let preview_ids: Vec<String> = node_ids.iter().take(5).cloned().collect();
    let response = serde_json::json!({
        "ingested": node_ids.len(),
        "node_ids": preview_ids,
        "triples_extracted": total_triples,
        "source": name,
    }).to_string();

    Ok(CallToolResult {
        content: vec![Content::text(response)],
        is_error: Some(false),
    })
}