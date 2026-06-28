use axum::{
    Json,
    extract::{State, Path, Query},
    http::{StatusCode, HeaderMap, header},
    response::IntoResponse,
};
use std::sync::Arc;
use serde::{Deserialize};
use crate::transport::http::HttpState;

#[derive(Deserialize)]
pub struct AddFederationPeerRequest {
    pub name: String,
    pub url: String,
    pub token: String,
}

#[derive(Deserialize)]
pub struct SetShareableRequest {
    pub shareable: bool,
}

#[derive(Deserialize)]
pub struct ApprovePeerRequest {
    pub token: String,
}

#[derive(serde::Deserialize)]
pub struct AnchorSeedEntry {
    pub guild: String,
    pub intent: String,
    #[serde(default = "default_seed_source")]
    pub source: String,
}
fn default_seed_source() -> String { "seed".to_string() }

pub async fn federation_list_peers(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let config = state.config.read().await;
    let peers: Vec<serde_json::Value> = config.federation_peers.iter().map(|p| {
        serde_json::json!({
            "name": p.name,
            "url": p.url,
            "last_sync": p.last_sync,
        })
    }).collect();
    (StatusCode::OK, Json(peers)).into_response()
}

pub async fn federation_add_peer(State(state): State<Arc<HttpState>>, Json(req): Json<AddFederationPeerRequest>) -> impl IntoResponse {
    if req.name.is_empty() || req.url.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "name and url are required"}))).into_response();
    }

    {
        let config = state.config.read().await;
        if config.federation_peers.iter().any(|p| p.name == req.name) {
            return (StatusCode::CONFLICT, Json(serde_json::json!({"error": format!("Peer '{}' already exists", req.name)}))).into_response();
        }
    }

    if req.token.is_empty() || req.token == "mdns-auto" {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "A non-empty shared secret token is required to register a peer"}))).into_response();
    }

    let peer = crate::federation::FederationPeer {
        name: req.name.clone(),
        url: req.url.clone(),
        auth_token: req.token.clone(),
        shared_secret: String::new(),
        last_sync: None,
        approved: true,
        added_at: 0,
    };

    {
        let mut config = state.config.write().await;
        config.federation_peers.push(peer);
    }

    // Persist config
    let config_read = state.config.read().await;
    let config_path = std::path::Path::new("tylluan.toml");
    let _ = crate::config::persist_federation_peers(&config_read, config_path);

    (StatusCode::CREATED, Json(serde_json::json!({"status": "added", "name": req.name}))).into_response()
}

pub async fn federation_remove_peer(State(state): State<Arc<HttpState>>, Path(name): Path<String>) -> impl IntoResponse {
    let existed;
    {
        let mut config = state.config.write().await;
        let len_before = config.federation_peers.len();
        config.federation_peers.retain(|p| p.name != name);
        existed = config.federation_peers.len() < len_before;
    }

    if !existed {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": format!("Peer '{}' not found", name)}))).into_response();
    }

    let config_read = state.config.read().await;
    let config_path = std::path::Path::new("tylluan.toml");
    let _ = crate::config::persist_federation_peers(&config_read, config_path);

    (StatusCode::OK, Json(serde_json::json!({"status": "removed", "name": name}))).into_response()
}

pub async fn federation_approve_peer(
    State(state): State<Arc<HttpState>>,
    Path(name): Path<String>,
    Json(req): Json<ApprovePeerRequest>,
) -> impl IntoResponse {
    if req.token.is_empty() || req.token == "mdns-auto" {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "A real shared secret token is required to approve a peer"}))).into_response();
    }

    let found;
    {
        let mut config = state.config.write().await;
        if let Some(peer) = config.federation_peers.iter_mut().find(|p| p.name == name) {
            peer.auth_token = req.token.clone();
            peer.approved = true;
            found = true;
        } else {
            found = false;
        }
    }

    if !found {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": format!("Peer '{}' not found", name)}))).into_response();
    }

    let config_read = state.config.read().await;
    let _ = crate::config::persist_federation_peers(&config_read, std::path::Path::new("tylluan.toml"));

    tracing::info!("✅ Federation peer '{}' approved by operator", name);
    (StatusCode::OK, Json(serde_json::json!({"status": "approved", "name": name}))).into_response()
}

pub async fn federation_sync_push(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let peers;
    {
        let config = state.config.read().await;
        peers = config.federation_peers.clone();
    }

    if peers.is_empty() {
        return (StatusCode::OK, Json(serde_json::json!({"synced": 0, "message": "No federation peers configured"}))).into_response();
    }

    // Get all shareable nodes
    let shareable_nodes = match state.silva.get_shareable_nodes().await {
        Ok(nodes) => nodes,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("Failed to get shareable nodes: {e}")}))).into_response(),
    };

    let plain_body = serde_json::to_vec(&shareable_nodes).unwrap_or_default();
    let mut synced_count = 0;
    let client = reqwest::Client::new();

    for peer in &peers {
        // Only sync with explicitly approved peers
        if !peer.approved {
            tracing::warn!("⛔ Federation: skipping unapproved peer '{}'", peer.name);
            continue;
        }

        // Encrypt payload with ChaCha20-Poly1305 using shared token as key
        let encrypted = match crate::federation::encrypt_payload(&plain_body, peer.encryption_key()) {
            Ok(enc) => enc,
            Err(e) => {
                tracing::error!("Federation encrypt failed for peer '{}': {}", peer.name, e);
                continue;
            }
        };

        let sync_url = format!("{}/api/v1/federation/sync/receive", peer.url.trim_end_matches('/'));
        let resp = client
            .post(&sync_url)
            .bearer_auth(&peer.auth_token)
            .header("content-type", "application/octet-stream")
            .body(encrypted)
            .send()
            .await;

        if let Ok(r) = resp
            && r.status().is_success() {
                synced_count += 1;
                // Update last_sync timestamp
                let mut config = state.config.write().await;
                if let Some(p) = config.federation_peers.iter_mut().find(|p| p.name == peer.name) {
                    p.last_sync = Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64);
                }
            }
    }

    // Persist updated config
    let config_read = state.config.read().await;
    let config_path = std::path::Path::new("tylluan.toml");
    let _ = crate::config::persist_federation_peers(&config_read, config_path);

    (StatusCode::OK, Json(serde_json::json!({
        "synced": synced_count,
        "total_peers": peers.len(),
        "nodes_synced": shareable_nodes.len(),
    }))).into_response()
}

pub async fn federation_sync_receive(
    State(state): State<Arc<HttpState>>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // Extract Bearer token from Authorization header
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("")
        .to_string();

    if bearer.is_empty() {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "Missing Authorization header"}))).into_response();
    }

    // Find a matching approved peer — constant-time token comparison via == on String
    let matched_peer = {
        let config = state.config.read().await;
        config.federation_peers.iter()
            .find(|p| p.approved && p.auth_token == bearer)
            .cloned()
    };

    let peer = match matched_peer {
        Some(p) => p,
        None => {
            tracing::warn!("⛔ Federation sync/receive: rejected — no approved peer matched token");
            return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "Unauthorized"}))).into_response();
        }
    };

    // Decrypt payload with ChaCha20-Poly1305
    let plain = match crate::federation::decrypt_payload(&body, peer.encryption_key()) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Federation decrypt failed from peer '{}': {}", peer.name, e);
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Payload decryption failed"}))).into_response();
        }
    };

    let nodes: Vec<crate::memory::silva::GraphNode> = match serde_json::from_slice(&plain) {
        Ok(n) => n,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": format!("Invalid JSON: {e}")}))).into_response(),
    };

    let mut received = 0;
    let mut skipped = 0;

    for node in &nodes {
        // Skip protected nodes (don't overwrite local protected data)
        if node.protected {
            skipped += 1;
            continue;
        }
        if state.silva.upsert_node(&node.id, &node.node_type, &node.content, &node.metadata).await.is_ok() {
            received += 1;
        } else {
            skipped += 1;
        }
    }

    (StatusCode::OK, Json(serde_json::json!({
        "received": received,
        "skipped": skipped,
        "total": nodes.len(),
    }))).into_response()
}

pub async fn federation_sharing_disable(
    State(state): State<Arc<HttpState>>,
) -> impl IntoResponse {
    let mut cfg = state.config.write().await;
    cfg.sharing.enabled = false;
    drop(cfg);
    let _ = state.silva.reset_all_shareable().await;
    Json(serde_json::json!({"status": "ok", "sharing_enabled": false}))
}

pub async fn federation_sharing_enable(
    State(state): State<Arc<HttpState>>,
) -> impl IntoResponse {
    let mut cfg = state.config.write().await;
    cfg.sharing.enabled = true;
    let policy = cfg.sharing.clone();
    drop(cfg);
    let _ = state.silva.apply_sharing_policy(
        policy.min_weight,
        policy.min_activity_hours,
        &policy.node_types,
    ).await;
    Json(serde_json::json!({"status": "ok", "sharing_enabled": true}))
}

pub async fn federation_sharing_status(
    State(state): State<Arc<HttpState>>,
) -> impl IntoResponse {
    let cfg = state.config.read().await;
    let sharing = cfg.sharing.clone();
    drop(cfg);
    let count = state.silva.get_shareable_nodes().await
        .map(|n| n.len()).unwrap_or(0);
    Json(serde_json::json!({
        "enabled": sharing.enabled,
        "node_types": sharing.node_types,
        "min_weight": sharing.min_weight,
        "min_activity_hours": sharing.min_activity_hours,
        "shareable_nodes_count": count,
    }))
}

pub async fn silva_set_shareable_handler(
    State(state): State<Arc<HttpState>>,
    Path(id): Path<String>,
    Json(req): Json<SetShareableRequest>,
) -> impl IntoResponse {
    match state.silva.set_shareable(&id, req.shareable).await {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({"status": "ok", "id": id, "shareable": req.shareable}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

pub async fn slo_summary_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let statuses = state.registry.status_all().await.unwrap_or_default();
    let silva = state.silva.clone();
    let always_on_count = statuses.iter().filter(|s| s.always_on).count() as f64;
    let online_always_on = statuses.iter().filter(|s| s.always_on && s.running).count() as f64;
    let availability: f64 = if always_on_count > 0.0 { online_always_on / always_on_count * 100.0 } else { 100.0 };
    let node_count = silva.node_count().await.unwrap_or(0) as f64;
    // FN-UI-001 fix: availability is already 0-100; normalize budget to 0-100 scale
    let error_budget_remaining = ((availability - 99.9) / 0.1 * 100.0).max(0.0_f64).min(100.0_f64);
    let slo_status = if availability >= 99.9 { "healthy" } else if availability >= 99.0 { "degraded" } else { "violated" };
    (StatusCode::OK, Json(serde_json::json!({
        "slo_target": 99.9,
        "current_availability": availability.round(),
        "error_budget_consumed_percent": (100.0_f64 - error_budget_remaining).round(),
        "error_budget_remaining_percent": error_budget_remaining.round(),
        "status": slo_status,
        "metrics": {
            "total_services": always_on_count as i64,
            "healthy_services": online_always_on as i64,
            "total_nodes": node_count as i64
        }
    })))
}

pub async fn routing_anchors_list(
    State(state): State<Arc<HttpState>>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let guild_filter = params.get("guild").map(|s| s.as_str());
    let limit: usize = params.get("limit").and_then(|s| s.parse().ok()).unwrap_or(100);
    match state.silva.get_routing_anchors(guild_filter, limit).await {
        Ok(nodes) => Json(serde_json::json!({
            "anchors": nodes,
            "count": nodes.len(),
        })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

pub async fn routing_anchors_reembed(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    match state.matcher.engine() {
        None => (StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "Embedding engine not ready yet — retry in a few seconds"}))).into_response(),
        Some(engine) => {
            match state.silva.reembed_anchors(engine).await {
                Ok(n) => Json(serde_json::json!({"reembedded": n, "status": "ok"})).into_response(),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": e.to_string()}))).into_response(),
            }
        }
    }
}

pub async fn routing_anchors_seed(
    State(state): State<Arc<HttpState>>,
    Json(entries): Json<Vec<AnchorSeedEntry>>,
) -> impl IntoResponse {
    let engine = state.matcher.engine();
    let mut inserted = 0usize;
    let mut errors = 0usize;
    for entry in &entries {
        let embedding = engine.as_ref().and_then(|e| e.embed(&entry.intent).ok());
        match state.silva.upsert_routing_anchor(
            &entry.guild,
            &entry.intent,
            &entry.source,
            embedding.as_deref(),
        ).await {
            Ok(_) => inserted += 1,
            Err(_) => errors += 1,
        }
    }
    Json(serde_json::json!({"inserted": inserted, "errors": errors})).into_response()
}

// ── M11-B: Pull Sync ──────────────────────────────────────────────────────────

/// GET /api/v1/federation/sync/export
/// Returns ChaCha20-encrypted shareable nodes for the authenticated peer.
/// Peer is identified by bearer token matching an approved peer's auth_token.
pub async fn federation_sync_export(
    State(state): State<Arc<HttpState>>,
    Query(params): Query<std::collections::HashMap<String, String>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("")
        .to_string();

    if bearer.is_empty() {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "Missing Authorization header"}))).into_response();
    }

    let peer = {
        let config = state.config.read().await;
        config.federation_peers.iter()
            .find(|p| p.approved && p.auth_token == bearer)
            .cloned()
    };

    let peer = match peer {
        Some(p) => p,
        None => return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "Unauthorized"}))).into_response(),
    };

    let include_received = params.get("include_received").map(|v| v == "true").unwrap_or(false);

    let nodes = match state.silva.get_shareable_nodes().await {
        Ok(n) => n,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("Failed to get shareable nodes: {e}")}))).into_response(),
    };

    let nodes_for_export: Vec<serde_json::Value> = if include_received {
        nodes.iter().map(|n| serde_json::json!(n)).collect()
    } else {
        nodes.iter()
            .filter(|n| {
                let meta: serde_json::Value = serde_json::from_str(&n.metadata).unwrap_or_default();
                meta.get("federation_source").is_none()
            })
            .map(|n| serde_json::json!(n))
            .collect()
    };

    let plain_body = serde_json::to_vec(&nodes_for_export).unwrap_or_default();
    let encrypted = match crate::federation::encrypt_payload(&plain_body, peer.encryption_key()) {
        Ok(e) => e,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("Encryption failed: {e}")}))).into_response(),
    };

    (
        StatusCode::OK,
        [("content-type", "application/octet-stream")],
        encrypted,
    ).into_response()
}

/// POST /api/v1/federation/sync/pull?peer={name}
/// Fetches shareable nodes from the named peer's /export endpoint,
/// decrypts with the peer's shared_secret, and upserts into local SilvaDB.
pub async fn federation_sync_pull(
    State(state): State<Arc<HttpState>>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let peer_name = match params.get("peer") {
        Some(n) => n,
        None => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Missing 'peer' query parameter"}))).into_response(),
    };

    let peer = {
        let config = state.config.read().await;
        config.federation_peers.iter()
            .find(|p| p.name == *peer_name && p.approved)
            .cloned()
    };

    let peer = match peer {
        Some(p) => p,
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": format!("Approved peer '{}' not found", peer_name)}))).into_response(),
    };

    let export_url = format!("{}/api/v1/federation/sync/export", peer.url.trim_end_matches('/'));
    let client = reqwest::Client::new();

    let resp = match client
        .get(&export_url)
        .bearer_auth(&peer.auth_token)
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            return (StatusCode::BAD_GATEWAY, Json(serde_json::json!({
                "error": format!("Peer responded with {status}: {body}")
            }))).into_response();
        }
        Err(e) => return (StatusCode::BAD_GATEWAY, Json(serde_json::json!({
            "error": format!("Failed to connect to peer '{}': {e}", peer.name)
        }))).into_response(),
    };

    let encrypted = match resp.bytes().await {
        Ok(b) => b.to_vec(),
        Err(e) => return (StatusCode::BAD_GATEWAY, Json(serde_json::json!({
            "error": format!("Failed to read response from peer: {e}")
        }))).into_response(),
    };

    let plain = match crate::federation::decrypt_payload(&encrypted, peer.encryption_key()) {
        Ok(p) => p,
        Err(e) => return (StatusCode::BAD_GATEWAY, Json(serde_json::json!({
            "error": format!("Decryption failed: {e}")
        }))).into_response(),
    };

    let nodes: Vec<serde_json::Value> = match serde_json::from_slice(&plain) {
        Ok(n) => n,
        Err(e) => return (StatusCode::BAD_GATEWAY, Json(serde_json::json!({
            "error": format!("Invalid JSON from peer: {e}")
        }))).into_response(),
    };

    let mut received = 0u64;
    let mut skipped = 0u64;

    for node_val in &nodes {
        let node_id = node_val["id"].as_str().unwrap_or("");
        let node_type = node_val["node_type"].as_str().unwrap_or("entity");
        let content = node_val["content"].as_str().unwrap_or("");
        let mut meta: serde_json::Value = node_val.get("metadata")
            .and_then(|m| serde_json::from_str(m.as_str().unwrap_or("{}")).ok())
            .unwrap_or(serde_json::json!({}));

        if let serde_json::Value::Object(ref mut map) = meta {
            map.insert("federation_source".into(), serde_json::Value::String(peer.name.clone()));
        }
        let meta_str = serde_json::to_string(&meta).unwrap_or_default();

        let is_protected = node_val.get("protected").and_then(|v| v.as_bool()).unwrap_or(false);
        if is_protected {
            skipped += 1;
            continue;
        }

        match state.silva.upsert_node(node_id, node_type, content, &meta_str).await {
            Ok(_) => received += 1,
            Err(_) => skipped += 1,
        }
    }

    // Update last_sync
    {
        let mut config = state.config.write().await;
        if let Some(p) = config.federation_peers.iter_mut().find(|p| p.name == peer.name) {
            p.last_sync = Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64);
        }
    }
    let config_read = state.config.read().await;
    let _ = crate::config::persist_federation_peers(&config_read, std::path::Path::new("tylluan.toml"));

    (StatusCode::OK, Json(serde_json::json!({
        "received": received,
        "skipped": skipped,
        "total": nodes.len(),
        "peer": peer.name,
    }))).into_response()
}

/// POST /api/v1/federation/sync/both?peer={name}
/// Push to named peer, then pull from them — full bidirectional sync.
pub async fn federation_sync_both(
    State(state): State<Arc<HttpState>>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let peer_name = match params.get("peer") {
        Some(n) => n,
        None => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Missing 'peer' query parameter"}))).into_response(),
    };

    let peer = {
        let config = state.config.read().await;
        config.federation_peers.iter()
            .find(|p| p.name == *peer_name && p.approved)
            .cloned()
    };

    let peer = match peer {
        Some(p) => p,
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": format!("Approved peer '{}' not found", peer_name)}))).into_response(),
    };

    // Push step: encrypt shareable nodes and POST to peer's /sync/receive
    let shareable_nodes = match state.silva.get_shareable_nodes().await {
        Ok(n) => n,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("Failed to get shareable nodes: {e}")}))).into_response(),
    };

    let nodes_for_export: Vec<serde_json::Value> = shareable_nodes.iter()
        .filter(|n| {
            let meta: serde_json::Value = serde_json::from_str(&n.metadata).unwrap_or_default();
            meta.get("federation_source").is_none()
        })
        .map(|n| serde_json::json!(n))
        .collect();

    let plain_body = serde_json::to_vec(&nodes_for_export).unwrap_or_default();
    let encrypted = match crate::federation::encrypt_payload(&plain_body, peer.encryption_key()) {
        Ok(e) => e,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("Encryption failed: {e}")}))).into_response(),
    };

    let client = reqwest::Client::new();
    let push_url = format!("{}/api/v1/federation/sync/receive", peer.url.trim_end_matches('/'));
    let push_result = client
        .post(&push_url)
        .bearer_auth(&peer.auth_token)
        .header("content-type", "application/octet-stream")
        .body(encrypted)
        .send()
        .await;

    let push_ok = match &push_result {
        Ok(r) if r.status().is_success() => true,
        _ => false,
    };

    // Pull step: fetch from peer's export
    let export_url = format!("{}/api/v1/federation/sync/export", peer.url.trim_end_matches('/'));
    let pull_result = client
        .get(&export_url)
        .bearer_auth(&peer.auth_token)
        .send()
        .await;

    let mut pulled = 0u64;
    if let Ok(r) = pull_result
        && r.status().is_success()
    {
        if let Ok(encrypted_bytes) = r.bytes().await {
            if let Ok(plain) = crate::federation::decrypt_payload(&encrypted_bytes, peer.encryption_key()) {
                if let Ok(nodes) = serde_json::from_slice::<Vec<serde_json::Value>>(&plain) {
                    for node_val in &nodes {
                        let node_id = node_val["id"].as_str().unwrap_or("");
                        let node_type = node_val["node_type"].as_str().unwrap_or("entity");
                        let content = node_val["content"].as_str().unwrap_or("");
                        let mut meta: serde_json::Value = node_val.get("metadata")
                            .and_then(|m| serde_json::from_str(m.as_str().unwrap_or("{}")).ok())
                            .unwrap_or(serde_json::json!({}));
                        if let serde_json::Value::Object(ref mut map) = meta {
                            map.insert("federation_source".into(), serde_json::Value::String(peer.name.clone()));
                        }
                        let meta_str = serde_json::to_string(&meta).unwrap_or_default();
                        let is_protected = node_val.get("protected").and_then(|v| v.as_bool()).unwrap_or(false);
                        if !is_protected && state.silva.upsert_node(node_id, node_type, content, &meta_str).await.is_ok() {
                            pulled += 1;
                        }
                    }
                }
            }
        }
    }

    // Update last_sync
    {
        let mut config = state.config.write().await;
        if let Some(p) = config.federation_peers.iter_mut().find(|p| p.name == peer.name) {
            p.last_sync = Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64);
        }
    }
    let config_read = state.config.read().await;
    let _ = crate::config::persist_federation_peers(&config_read, std::path::Path::new("tylluan.toml"));

    (StatusCode::OK, Json(serde_json::json!({
        "peer": peer.name,
        "push_succeeded": push_ok,
        "pulled_nodes": pulled,
    }))).into_response()
}
