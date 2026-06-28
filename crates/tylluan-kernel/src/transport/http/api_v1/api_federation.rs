use axum::{
    Json,
    extract::{State, Path, Query},
    http::{StatusCode, HeaderMap, header},
    response::IntoResponse,
};
use std::sync::Arc;
use serde::Deserialize;
use crate::transport::http::HttpState;

#[derive(Deserialize)]
pub struct AddFederationPeerRequest {
    pub name: String,
    pub url: String,
    /// HTTP bearer token for authenticating to/from this peer.
    pub auth_token: String,
    /// ChaCha20 encryption key. Defaults to auth_token when omitted.
    pub shared_secret: Option<String>,
}

#[derive(Deserialize)]
pub struct SetShareableRequest {
    pub shareable: bool,
}

#[derive(Deserialize)]
pub struct ApprovePeerRequest {
    pub auth_token: String,
    pub shared_secret: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct AnchorSeedEntry {
    pub guild: String,
    pub intent: String,
    #[serde(default = "default_seed_source")]
    pub source: String,
}
fn default_seed_source() -> String { "seed".to_string() }

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Reload in-memory config.federation_peers from the DB after any mutation.
async fn reload_peers_cache(state: &Arc<HttpState>) {
    if let Ok(peers) = state.peer_db.load_all() {
        state.config.write().await.federation_peers = peers;
    }
}

// ── Peer management ──────────────────────────────────────────────────────────

pub async fn federation_list_peers(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let config = state.config.read().await;
    let peers: Vec<serde_json::Value> = config.federation_peers.iter().map(|p| {
        serde_json::json!({
            "name": p.name,
            "url": p.url,
            "approved": p.approved,
            "last_sync": p.last_sync,
            "added_at": p.added_at,
            "shared_secret_set": !p.shared_secret.is_empty(),
        })
    }).collect();
    (StatusCode::OK, Json(peers)).into_response()
}

pub async fn federation_add_peer(
    State(state): State<Arc<HttpState>>,
    Json(req): Json<AddFederationPeerRequest>,
) -> impl IntoResponse {
    if req.name.is_empty() || req.url.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "name and url are required"}))).into_response();
    }
    if req.auth_token.is_empty() || req.auth_token == "mdns-auto" {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "A non-empty auth_token is required"}))).into_response();
    }

    {
        let config = state.config.read().await;
        if config.federation_peers.iter().any(|p| p.name == req.name) {
            return (StatusCode::CONFLICT, Json(serde_json::json!({"error": format!("Peer '{}' already exists", req.name)}))).into_response();
        }
    }

    let peer = crate::federation::FederationPeer {
        name: req.name.clone(),
        url: req.url.clone(),
        auth_token: req.auth_token.clone(),
        shared_secret: req.shared_secret.unwrap_or_default(),
        last_sync: None,
        approved: true,
        added_at: now_secs() as u64,
        ed25519_pubkey: String::new(),
    };

    if let Err(e) = state.peer_db.insert(&peer) {
        tracing::error!("Failed to persist peer '{}': {}", peer.name, e);
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Failed to persist peer"}))).into_response();
    }
    reload_peers_cache(&state).await;

    (StatusCode::CREATED, Json(serde_json::json!({"status": "added", "name": req.name}))).into_response()
}

pub async fn federation_remove_peer(
    State(state): State<Arc<HttpState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match state.peer_db.remove(&name) {
        Ok(true) => {
            reload_peers_cache(&state).await;
            (StatusCode::OK, Json(serde_json::json!({"status": "removed", "name": name}))).into_response()
        }
        Ok(false) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": format!("Peer '{}' not found", name)}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

pub async fn federation_approve_peer(
    State(state): State<Arc<HttpState>>,
    Path(name): Path<String>,
    Json(req): Json<ApprovePeerRequest>,
) -> impl IntoResponse {
    if req.auth_token.is_empty() || req.auth_token == "mdns-auto" {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "A real auth_token is required to approve a peer"}))).into_response();
    }

    let secret = req.shared_secret.as_deref();
    match state.peer_db.update_approved(&name, &req.auth_token, secret, true) {
        Ok(true) => {
            reload_peers_cache(&state).await;
            // M12-B: auto-fetch peer's Ed25519 identity pubkey
            if let Some(peer) = state.config.read().await.federation_peers.iter().find(|p| p.name == name).cloned() {
                let identity_url = format!("{}/api/v1/federation/identity", peer.url.trim_end_matches('/'));
                let client = reqwest::Client::new();
                match client.get(&identity_url).bearer_auth(&peer.auth_token).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        if let Ok(body) = resp.json::<serde_json::Value>().await {
                            if let Some(pubkey) = body.get("public_key").and_then(|v| v.as_str()) {
                                if !pubkey.is_empty() {
                                    let _ = state.peer_db.update_ed25519_pubkey(&name, pubkey);
                                    tracing::info!("🔑 Auto-fetched Ed25519 pubkey for peer '{}'", name);
                                    reload_peers_cache(&state).await;
                                }
                            }
                        }
                    }
                    Ok(resp) => tracing::warn!("⛔ Identity fetch for '{}' returned {}", name, resp.status()),
                    Err(e) => tracing::warn!("⛔ Identity fetch for '{}' failed: {e}", name),
                }
            }

            tracing::info!("✅ Federation peer '{}' approved by operator", name);
            (StatusCode::OK, Json(serde_json::json!({"status": "approved", "name": name}))).into_response()
        }
        Ok(false) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": format!("Peer '{}' not found", name)}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

// ── Push sync ────────────────────────────────────────────────────────────────

pub async fn federation_sync_push(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let peers = state.config.read().await.federation_peers.clone();

    if peers.is_empty() {
        return (StatusCode::OK, Json(serde_json::json!({"synced": 0, "message": "No federation peers configured"}))).into_response();
    }

    let shareable_nodes = match state.silva.get_shareable_nodes().await {
        Ok(nodes) => nodes,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("Failed to get shareable nodes: {e}")}))).into_response(),
    };

    // Exclude nodes received from federation (no echo loops)
    let local_nodes: Vec<_> = shareable_nodes.iter()
        .filter(|n| {
            let meta: serde_json::Value = serde_json::from_str(&n.metadata).unwrap_or_default();
            meta.get("federation_source").is_none()
        })
        .collect();

    // M12-B: sign each node
    let mut envelopes = Vec::with_capacity(local_nodes.len());
    for node in &local_nodes {
        let node_json = serde_json::to_value(node).unwrap_or_default();
        if let Ok(e) = tylluan_link::identity::sign_node(&state.node_identity, &node_json) {
            envelopes.push(e);
        }
    }
    let plain_body = serde_json::to_vec(&envelopes).unwrap_or_default();
    let mut synced_count = 0;
    let client = reqwest::Client::new();

    for peer in &peers {
        if !peer.approved {
            tracing::warn!("⛔ Federation: skipping unapproved peer '{}'", peer.name);
            continue;
        }
        let encrypted = match crate::federation::encrypt_payload(&plain_body, peer.encryption_key()) {
            Ok(enc) => enc,
            Err(e) => { tracing::error!("Federation encrypt failed for '{}': {}", peer.name, e); continue; }
        };
        let sync_url = format!("{}/api/v1/federation/sync/receive", peer.url.trim_end_matches('/'));
        let resp = client
            .post(&sync_url)
            .bearer_auth(&peer.auth_token)
            .header("content-type", "application/octet-stream")
            .body(encrypted)
            .send()
            .await;

        if let Ok(r) = resp && r.status().is_success() {
            synced_count += 1;
            let _ = state.peer_db.update_last_sync(&peer.name, now_secs());
        }
    }

    reload_peers_cache(&state).await;
    (StatusCode::OK, Json(serde_json::json!({
        "synced": synced_count,
        "total_peers": peers.len(),
        "nodes_synced": local_nodes.len(),
    }))).into_response()
}

// ── Receive sync ─────────────────────────────────────────────────────────────

pub async fn federation_sync_receive(
    State(state): State<Arc<HttpState>>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
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
        None => {
            tracing::warn!("⛔ Federation sync/receive: rejected — no approved peer matched token");
            return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "Unauthorized"}))).into_response();
        }
    };

    let plain = match crate::federation::decrypt_payload(&body, peer.encryption_key()) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Federation decrypt failed from '{}': {}", peer.name, e);
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Payload decryption failed"}))).into_response();
        }
    };

    let envelopes: Vec<tylluan_link::identity::SignedEnvelope> = match serde_json::from_slice(&plain) {
        Ok(e) => e,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": format!("Invalid JSON: {e}")}))).into_response(),
    };

    let mut received = 0;
    let mut skipped = 0;
    let mut verified = 0;
    let has_pubkey = !peer.ed25519_pubkey.is_empty();

    for envelope in &envelopes {
        // M12-B: verify signature if peer has a pubkey on record
        if has_pubkey {
            match tylluan_link::identity::verify_envelope(envelope, &peer.ed25519_pubkey) {
                Ok(()) => verified += 1,
                Err(e) => {
                    tracing::warn!("⛔ Federation: signature verification failed from '{}': {e}", peer.name);
                    skipped += 1;
                    continue;
                }
            }
        }

        let node_id = envelope.node.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let node_type = envelope.node.get("node_type").and_then(|v| v.as_str()).unwrap_or("entity");
        let content = envelope.node.get("content").and_then(|v| v.as_str()).unwrap_or("");
        let is_protected = envelope.node.get("protected").and_then(|v| v.as_bool()).unwrap_or(false);

        if is_protected { skipped += 1; continue; }

        let mut meta: serde_json::Value = envelope.node.get("metadata")
            .and_then(|m| serde_json::from_str(m.as_str().unwrap_or("{}")).ok())
            .unwrap_or(serde_json::json!({}));
        if let serde_json::Value::Object(ref mut map) = meta {
            map.insert("federation_source".into(), serde_json::Value::String(peer.name.clone()));
        }
        let meta_str = serde_json::to_string(&meta).unwrap_or_default();

        if state.silva.upsert_node(node_id, node_type, content, &meta_str).await.is_ok() {
            received += 1;
        } else {
            skipped += 1;
        }
    }

    (StatusCode::OK, Json(serde_json::json!({
        "received": received,
        "skipped": skipped,
        "verified": verified,
        "total": envelopes.len(),
    }))).into_response()
}

// ── M11-B: Pull sync ─────────────────────────────────────────────────────────

/// GET /api/v1/federation/sync/export
/// Returns ChaCha20-encrypted signed envelopes for the authenticated peer.
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

    let nodes_for_export: Vec<_> = if include_received {
        nodes.iter().collect()
    } else {
        nodes.iter()
            .filter(|n| {
                let meta: serde_json::Value = serde_json::from_str(&n.metadata).unwrap_or_default();
                meta.get("federation_source").is_none()
            })
            .collect()
    };

    // M12-B: sign each node before encrypting
    let mut envelopes = Vec::with_capacity(nodes_for_export.len());
    for node in &nodes_for_export {
        let node_json = serde_json::to_value(node).unwrap_or_default();
        match tylluan_link::identity::sign_node(&state.node_identity, &node_json) {
            Ok(envelope) => envelopes.push(envelope),
            Err(e) => tracing::warn!("Failed to sign node '{}': {e}", node.id),
        }
    }

    let plain_body = serde_json::to_vec(&envelopes).unwrap_or_default();
    let encrypted = match crate::federation::encrypt_payload(&plain_body, peer.encryption_key()) {
        Ok(e) => e,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("Encryption failed: {e}")}))).into_response(),
    };

    (StatusCode::OK, [("content-type", "application/octet-stream")], encrypted).into_response()
}

/// POST /api/v1/federation/sync/pull?peer={name}
pub async fn federation_sync_pull(
    State(state): State<Arc<HttpState>>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let peer_name = match params.get("peer") {
        Some(n) => n.clone(),
        None => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Missing 'peer' query parameter"}))).into_response(),
    };

    let peer = {
        let config = state.config.read().await;
        config.federation_peers.iter()
            .find(|p| p.name == peer_name && p.approved)
            .cloned()
    };

    let peer = match peer {
        Some(p) => p,
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": format!("Approved peer '{}' not found", peer_name)}))).into_response(),
    };

    let export_url = format!("{}/api/v1/federation/sync/export", peer.url.trim_end_matches('/'));
    let client = reqwest::Client::new();

    let resp = match client.get(&export_url).bearer_auth(&peer.auth_token).send().await {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            return (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": format!("Peer responded with {status}: {body}")}))).into_response();
        }
        Err(e) => return (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": format!("Failed to connect to '{}': {e}", peer.name)}))).into_response(),
    };

    let encrypted = match resp.bytes().await {
        Ok(b) => b.to_vec(),
        Err(e) => return (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": format!("Failed to read response: {e}")}))).into_response(),
    };

    let plain = match crate::federation::decrypt_payload(&encrypted, peer.encryption_key()) {
        Ok(p) => p,
        Err(e) => return (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": format!("Decryption failed: {e}")}))).into_response(),
    };

    let envelopes: Vec<tylluan_link::identity::SignedEnvelope> = match serde_json::from_slice(&plain) {
        Ok(e) => e,
        Err(e) => return (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": format!("Invalid JSON from peer: {e}")}))).into_response(),
    };

    let (mut received, mut skipped) = (0u64, 0u64);
    let has_pubkey = !peer.ed25519_pubkey.is_empty();

    for envelope in &envelopes {
        if has_pubkey {
            if let Err(e) = tylluan_link::identity::verify_envelope(envelope, &peer.ed25519_pubkey) {
                tracing::warn!("⛔ Pull: sig verify failed from '{}': {e}", peer.name);
                skipped += 1;
                continue;
            }
        }

        let is_protected = envelope.node.get("protected").and_then(|v| v.as_bool()).unwrap_or(false);
        if is_protected { skipped += 1; continue; }

        let node_id = envelope.node["id"].as_str().unwrap_or("");
        let node_type = envelope.node["node_type"].as_str().unwrap_or("entity");
        let content = envelope.node["content"].as_str().unwrap_or("");
        let mut meta: serde_json::Value = envelope.node.get("metadata")
            .and_then(|m| serde_json::from_str(m.as_str().unwrap_or("{}")).ok())
            .unwrap_or(serde_json::json!({}));
        if let serde_json::Value::Object(ref mut map) = meta {
            map.insert("federation_source".into(), serde_json::Value::String(peer.name.clone()));
        }
        let meta_str = serde_json::to_string(&meta).unwrap_or_default();

        match state.silva.upsert_node(node_id, node_type, content, &meta_str).await {
            Ok(_) => received += 1,
            Err(_) => skipped += 1,
        }
    }

    let _ = state.peer_db.update_last_sync(&peer.name, now_secs());
    reload_peers_cache(&state).await;

    (StatusCode::OK, Json(serde_json::json!({
        "received": received,
        "skipped": skipped,
        "total": envelopes.len(),
        "peer": peer.name,
    }))).into_response()
}

/// POST /api/v1/federation/sync/both?peer={name} — push then pull
pub async fn federation_sync_both(
    State(state): State<Arc<HttpState>>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let peer_name = match params.get("peer") {
        Some(n) => n.clone(),
        None => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Missing 'peer' query parameter"}))).into_response(),
    };

    let peer = {
        let config = state.config.read().await;
        config.federation_peers.iter()
            .find(|p| p.name == peer_name && p.approved)
            .cloned()
    };

    let peer = match peer {
        Some(p) => p,
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": format!("Approved peer '{}' not found", peer_name)}))).into_response(),
    };

    let client = reqwest::Client::new();

    // Push: local shareable (no echo) → peer's /receive
    let push_ok = if let Ok(shareable) = state.silva.get_shareable_nodes().await {
        let local_nodes: Vec<_> = shareable.iter()
            .filter(|n| {
                let meta: serde_json::Value = serde_json::from_str(&n.metadata).unwrap_or_default();
                meta.get("federation_source").is_none()
            })
            .collect();
        // M12-B: sign each node
        let mut envelopes = Vec::with_capacity(local_nodes.len());
        for node in &local_nodes {
            let node_json = serde_json::to_value(node).unwrap_or_default();
            if let Ok(e) = tylluan_link::identity::sign_node(&state.node_identity, &node_json) {
                envelopes.push(e);
            }
        }
        if let Ok(plain) = serde_json::to_vec(&envelopes) {
            if let Ok(enc) = crate::federation::encrypt_payload(&plain, peer.encryption_key()) {
                let push_url = format!("{}/api/v1/federation/sync/receive", peer.url.trim_end_matches('/'));
                matches!(
                    client.post(&push_url).bearer_auth(&peer.auth_token)
                        .header("content-type", "application/octet-stream")
                        .body(enc).send().await,
                    Ok(r) if r.status().is_success()
                )
            } else { false }
        } else { false }
    } else { false };

    // Pull: peer's /export → local SilvaDB
    let mut pulled = 0u64;
    let export_url = format!("{}/api/v1/federation/sync/export", peer.url.trim_end_matches('/'));
    if let Ok(r) = client.get(&export_url).bearer_auth(&peer.auth_token).send().await {
        if r.status().is_success() {
            if let Ok(enc_bytes) = r.bytes().await {
                if let Ok(plain) = crate::federation::decrypt_payload(&enc_bytes, peer.encryption_key()) {
                    if let Ok(envelopes) = serde_json::from_slice::<Vec<tylluan_link::identity::SignedEnvelope>>(&plain) {
                        let has_pubkey = !peer.ed25519_pubkey.is_empty();
                        for envelope in &envelopes {
                            if has_pubkey {
                                if let Err(e) = tylluan_link::identity::verify_envelope(envelope, &peer.ed25519_pubkey) {
                                    tracing::warn!("⛔ Both sync: sig verify failed from '{}': {e}", peer.name);
                                    continue;
                                }
                            }
                            let is_protected = envelope.node.get("protected").and_then(|v| v.as_bool()).unwrap_or(false);
                            if is_protected { continue; }
                            let node_id = envelope.node["id"].as_str().unwrap_or("");
                            let node_type = envelope.node["node_type"].as_str().unwrap_or("entity");
                            let content = envelope.node["content"].as_str().unwrap_or("");
                            let mut meta: serde_json::Value = envelope.node.get("metadata")
                                .and_then(|m| serde_json::from_str(m.as_str().unwrap_or("{}")).ok())
                                .unwrap_or(serde_json::json!({}));
                            if let serde_json::Value::Object(ref mut map) = meta {
                                map.insert("federation_source".into(), serde_json::Value::String(peer.name.clone()));
                            }
                            let meta_str = serde_json::to_string(&meta).unwrap_or_default();
                            if state.silva.upsert_node(node_id, node_type, content, &meta_str).await.is_ok() {
                                pulled += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    let _ = state.peer_db.update_last_sync(&peer.name, now_secs());
    reload_peers_cache(&state).await;

    (StatusCode::OK, Json(serde_json::json!({
        "peer": peer.name,
        "push_succeeded": push_ok,
        "pulled_nodes": pulled,
    }))).into_response()
}

// ── M11-C: Provenance query endpoint ─────────────────────────────────────────

/// GET /api/v1/federation/nodes?source={peer_name|local}&limit=N
pub async fn federation_nodes_query(
    State(state): State<Arc<HttpState>>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let source = params.get("source").map(|s| s.as_str());
    let limit: usize = params.get("limit").and_then(|s| s.parse().ok()).unwrap_or(200);
    match state.silva.get_nodes_by_source(source, limit).await {
        Ok(nodes) => {
            let items: Vec<serde_json::Value> = nodes.iter().map(|n| serde_json::json!({
                "id": n.id,
                "node_type": n.node_type,
                "content": n.content,
                "weight": n.weight,
                "shareable": n.shareable,
                "topic_key": n.topic_key,
            })).collect();
            (StatusCode::OK, Json(serde_json::json!({
                "source": source.unwrap_or("local"),
                "count": items.len(),
                "nodes": items,
            }))).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

// ── Sharing policy ───────────────────────────────────────────────────────────

pub async fn federation_sharing_disable(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let mut cfg = state.config.write().await;
    cfg.sharing.enabled = false;
    drop(cfg);
    let _ = state.silva.reset_all_shareable().await;
    Json(serde_json::json!({"status": "ok", "sharing_enabled": false}))
}

pub async fn federation_sharing_enable(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
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

pub async fn federation_sharing_status(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let cfg = state.config.read().await;
    let sharing = cfg.sharing.clone();
    drop(cfg);
    let count = state.silva.get_shareable_nodes().await.map(|n| n.len()).unwrap_or(0);
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

// ── SLO + routing anchors (unchanged) ────────────────────────────────────────

pub async fn slo_summary_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let statuses = state.registry.status_all().await.unwrap_or_default();
    let silva = state.silva.clone();
    let always_on_count = statuses.iter().filter(|s| s.always_on).count() as f64;
    let online_always_on = statuses.iter().filter(|s| s.always_on && s.running).count() as f64;
    let availability: f64 = if always_on_count > 0.0 { online_always_on / always_on_count * 100.0 } else { 100.0 };
    let node_count = silva.node_count().await.unwrap_or(0) as f64;
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
        Ok(nodes) => Json(serde_json::json!({"anchors": nodes, "count": nodes.len()})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

pub async fn routing_anchors_reembed(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    match state.matcher.engine() {
        None => (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"error": "Embedding engine not ready yet"}))).into_response(),
        Some(engine) => match state.silva.reembed_anchors(engine).await {
            Ok(n) => Json(serde_json::json!({"reembedded": n, "status": "ok"})).into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
        }
    }
}

pub async fn routing_anchors_seed(
    State(state): State<Arc<HttpState>>,
    Json(entries): Json<Vec<AnchorSeedEntry>>,
) -> impl IntoResponse {
    let engine = state.matcher.engine();
    let (mut inserted, mut errors) = (0usize, 0usize);
    for entry in &entries {
        let embedding = engine.as_ref().and_then(|e| e.embed(&entry.intent).ok());
        match state.silva.upsert_routing_anchor(&entry.guild, &entry.intent, &entry.source, embedding.as_deref()).await {
            Ok(_) => inserted += 1,
            Err(_) => errors += 1,
        }
    }
    Json(serde_json::json!({"inserted": inserted, "errors": errors})).into_response()
}

// ── M11-D: Scheduled Auto-Sync Helpers ──────────────────────────────────────

async fn push_to_peer_internal(
    state: &Arc<HttpState>,
    peer: &crate::federation::FederationPeer,
    client: &reqwest::Client,
    plain_body: &[u8],
) -> anyhow::Result<()> {
    let encrypted = crate::federation::encrypt_payload(plain_body, peer.encryption_key())?;
    let sync_url = format!("{}/api/v1/federation/sync/receive", peer.url.trim_end_matches('/'));
    let resp = client
        .post(&sync_url)
        .bearer_auth(&peer.auth_token)
        .header("content-type", "application/octet-stream")
        .body(encrypted)
        .send()
        .await?;

    if resp.status().is_success() {
        let _ = state.peer_db.update_last_sync(&peer.name, now_secs());
        Ok(())
    } else {
        anyhow::bail!("Status {}", resp.status())
    }
}

async fn pull_from_peer_internal(
    state: &Arc<HttpState>,
    peer: &crate::federation::FederationPeer,
    client: &reqwest::Client,
) -> anyhow::Result<usize> {
    let export_url = format!("{}/api/v1/federation/sync/export", peer.url.trim_end_matches('/'));
    let resp = client
        .get(&export_url)
        .bearer_auth(&peer.auth_token)
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("Peer responded with status {}", resp.status());
    }

    let encrypted = resp.bytes().await?.to_vec();
    let plain = crate::federation::decrypt_payload(&encrypted, peer.encryption_key())?;
    let envelopes: Vec<tylluan_link::identity::SignedEnvelope> = serde_json::from_slice(&plain)?;

    let mut received = 0;
    let has_pubkey = !peer.ed25519_pubkey.is_empty();

    for envelope in &envelopes {
        // M12-B: verify signature if peer has a pubkey on record
        if has_pubkey {
            if let Err(e) = tylluan_link::identity::verify_envelope(envelope, &peer.ed25519_pubkey) {
                tracing::warn!("⛔ Pull internal: sig verify failed from '{}': {e}", peer.name);
                continue;
            }
        }

        let node_id = envelope.node["id"].as_str().unwrap_or("");
        let node_type = envelope.node["node_type"].as_str().unwrap_or("entity");
        let content = envelope.node["content"].as_str().unwrap_or("");
        let mut meta: serde_json::Value = envelope.node.get("metadata")
            .and_then(|m| serde_json::from_str(m.as_str().unwrap_or("{}")).ok())
            .unwrap_or(serde_json::json!({}));

        if let serde_json::Value::Object(ref mut map) = meta {
            map.insert("federation_source".into(), serde_json::Value::String(peer.name.clone()));
        }
        let meta_str = serde_json::to_string(&meta).unwrap_or_default();

        let is_protected = envelope.node.get("protected").and_then(|v| v.as_bool()).unwrap_or(false);
        if is_protected {
            continue;
        }

        if state.silva.upsert_node(node_id, node_type, content, &meta_str).await.is_ok() {
            received += 1;
        }
    }

    let _ = state.peer_db.update_last_sync(&peer.name, now_secs());
    Ok(received)
}

pub fn spawn_auto_sync(state: Arc<HttpState>) {
    tokio::spawn(async move {
        // Read configuration values initially
        let mut interval_secs = {
            let config = state.config.read().await;
            config.federation.auto_sync_interval_secs
        };

        if interval_secs == 0 {
            tracing::info!("🔄 Federation auto-sync is disabled at startup (interval = 0)");
            return;
        }

        tracing::info!("🔄 Federation auto-sync background task started (interval = {interval_secs}s)");

        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs)).await;

            // Re-read configuration values in case of runtime changes
            let (curr_interval, curr_mode) = {
                let config = state.config.read().await;
                (config.federation.auto_sync_interval_secs, config.federation.auto_sync_mode.clone())
            };

            if curr_interval == 0 {
                tracing::info!("🔄 Federation auto-sync disabled dynamically");
                break;
            }
            interval_secs = curr_interval; // Update loop sleep duration

            tracing::info!("🔄 Starting scheduled auto-sync cycle (mode = '{curr_mode}')...");

            let peers = state.config.read().await.federation_peers.clone();
            let client = reqwest::Client::new();

            // Prepare local shareable nodes in case of "push" or "both"
            let plain_body = if curr_mode == "push" || curr_mode == "both" {
                if let Ok(nodes) = state.silva.get_shareable_nodes().await {
                    let local_nodes: Vec<_> = nodes.iter()
                        .filter(|n| {
                            let meta: serde_json::Value = serde_json::from_str(&n.metadata).unwrap_or_default();
                            meta.get("federation_source").is_none()
                        })
                        .collect();
                    // M12-B: sign each node
                    let mut envelopes = Vec::with_capacity(local_nodes.len());
                    for node in &local_nodes {
                        let node_json = serde_json::to_value(node).unwrap_or_default();
                        if let Ok(e) = tylluan_link::identity::sign_node(&state.node_identity, &node_json) {
                            envelopes.push(e);
                        }
                    }
                    serde_json::to_vec(&envelopes).unwrap_or_default()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };

            for peer in &peers {
                if !peer.approved {
                    continue;
                }

                if curr_mode == "push" || curr_mode == "both" {
                    if !plain_body.is_empty() {
                        match push_to_peer_internal(&state, peer, &client, &plain_body).await {
                            Ok(_) => tracing::info!("🔄 Auto-sync: successfully pushed to '{}'", peer.name),
                            Err(e) => tracing::error!("🔄 Auto-sync: push to '{}' failed: {e}", peer.name),
                        }
                    }
                }

                if curr_mode == "pull" || curr_mode == "both" {
                    match pull_from_peer_internal(&state, peer, &client).await {
                        Ok(n) => tracing::info!("🔄 Auto-sync: successfully pulled {n} nodes from '{}'", peer.name),
                        Err(e) => tracing::error!("🔄 Auto-sync: pull from '{}' failed: {e}", peer.name),
                    }
                }
            }

            reload_peers_cache(&state).await;
        }
    });
}

pub async fn federation_identity(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "node_id": state.node_identity.node_id(),
            "public_key": state.node_identity.public_key_hex(),
            "tylluan_version": env!("CARGO_PKG_VERSION"),
        })),
    )
}

