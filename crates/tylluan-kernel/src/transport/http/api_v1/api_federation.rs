use axum::{
    Json,
    extract::{State, Path, Query},
    http::{StatusCode, HeaderMap, header},
    response::IntoResponse,
};
use std::sync::Arc;
use serde::Deserialize;
use crate::transport::http::HttpState;
use crate::federation::permissions;

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

// ── ADR-019 step 2: shared peer gates ────────────────────────────────────────
//
// THE single ingestion helper (used by receive / pull manual / sync_both /
// pull_from_peer_internal) and THE single send-side body builder (used by
// push manual / sync_both / auto-sync). Duplicating the gates per flow is how
// holes appear — see docs/internal/DESIGN_adr019_step2_2026-10.md §4.

/// Outcome of ingesting one envelope from a peer.
#[derive(Debug, PartialEq, Eq)]
pub enum IngestOutcome {
    Received,
    Skipped(&'static str),
    Denied(String),
}

/// Pure freshness decision — separated so an `Err` reading freshness is a
/// REJECT (TL decision), never a silent upsert. Kept generic over the error
/// type so tests can feed it any `Err`.
pub fn decide_freshness<E>(
    fresh: Result<Option<(String, bool, String)>, E>,
    remote_hash: &str,
    remote_updated_at: &str,
    peer_name: &str,
) -> Result<(), &'static str> {
    match fresh {
        Ok(Some((local_hash, local_protected, local_updated_at))) => {
            match crate::consensus::resolve_node_freshness(
                &local_hash,
                local_protected,
                &local_updated_at,
                remote_hash,
                10, // REMOTE_PEER_PRIORITY — remote always loses to local
                peer_name,
                remote_updated_at,
            ) {
                crate::consensus::FreshnessResolution::AcceptRemote { .. } => Ok(()),
                crate::consensus::FreshnessResolution::Identical => Err("identical hash"),
                crate::consensus::FreshnessResolution::KeepLocal { .. } => Err("local wins"),
                crate::consensus::FreshnessResolution::MarkConflicted { .. } => Err("conflicted"),
            }
        }
        Ok(None) => Ok(()), // node doesn't exist locally — first sync
        Err(_) => Err("freshness read error"),
    }
}

/// ADR-019 step 2 — THE single peer ingestion path:
/// sig-verify → protected skip → space+direction gate (deny-by-default,
/// audited on denial) → provenance stamp → freshness (Err = reject) →
/// hardened upsert (a peer write never changes `protected`/`weight` of an
/// existing node, nor can it raise a local weight).
pub async fn ingest_from_peer(
    state: &Arc<HttpState>,
    peer: &crate::federation::FederationPeer,
    envelope: &tylluan_link::identity::SignedEnvelope,
    flow: &str,
) -> IngestOutcome {
    let has_pubkey = !peer.ed25519_pubkey.is_empty();
    if has_pubkey
        && let Err(e) = tylluan_link::identity::verify_envelope(envelope, &peer.ed25519_pubkey)
    {
        tracing::warn!("⛔ {flow}: sig verify failed from '{}': {e}", peer.name);
        return IngestOutcome::Skipped("signature verification failed");
    }

    let is_protected = envelope.node.get("protected").and_then(|v| v.as_bool()).unwrap_or(false);
    if is_protected {
        return IngestOutcome::Skipped("protected node");
    }

    let node_id = envelope.node.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let node_type = envelope.node.get("node_type").and_then(|v| v.as_str()).unwrap_or("entity");
    let content = envelope.node.get("content").and_then(|v| v.as_str()).unwrap_or("");

    // Space gate BEFORE any mutation work — deny-by-default.
    let raw_meta = envelope.node.get("metadata").and_then(|m| m.as_str()).unwrap_or("{}");
    let topic_key = envelope.node.get("topic_key").and_then(|v| v.as_str());
    let space = permissions::resolve_space(topic_key, raw_meta);
    if !peer.permissions.may_receive_space(&space) {
        let reason = format!("space '{space}' not allowed (direction receive)");
        permissions::audit_permission_denied(&peer.name, flow, "federation", "sync", &reason);
        return IngestOutcome::Denied(reason);
    }

    let mut meta: serde_json::Value = serde_json::from_str(raw_meta).unwrap_or_else(|_| serde_json::json!({}));
    if let serde_json::Value::Object(ref mut map) = meta {
        map.insert("federation_source".into(), serde_json::Value::String(peer.name.clone()));
    }
    let meta_str = serde_json::to_string(&meta).unwrap_or_default();

    let remote_hash = envelope.node.get("content_hash").and_then(|v| v.as_str()).unwrap_or("");
    let remote_updated_at = envelope.node.get("updated_at").and_then(|v| v.as_str()).unwrap_or("");

    let fresh = state.silva.get_freshness_info(node_id).await;
    if let Err(reason) = decide_freshness(fresh, remote_hash, remote_updated_at, &peer.name) {
        return IngestOutcome::Skipped(reason);
    }

    let opts = crate::memory::silva::NodeWriteOptions::new("federation_peer").federation_ingest();
    match state.silva.upsert_node_with_validity(node_id, node_type, content, &meta_str, opts).await {
        Ok(_) => IngestOutcome::Received,
        Err(e) => {
            tracing::trace!("{flow}: upsert rejected for '{node_id}': {e}");
            IngestOutcome::Skipped("upsert rejected")
        }
    }
}

/// ADR-019 step 2 — THE single send-side body builder. Applies the shareable
/// filter, the no-echo filter and the space/direction gate for THIS peer
/// (deny-by-default), then signs the envelopes. Send-side exclusion is silent
/// policy (trace); audit is reserved for peer-attempted denials (receive and
/// dispatch), avoiding one audit row per node on every auto-sync cycle.
pub async fn push_body_for_peer(
    state: &Arc<HttpState>,
    peer: &crate::federation::FederationPeer,
) -> Vec<tylluan_link::identity::SignedEnvelope> {
    let Ok(nodes) = state.silva.get_shareable_nodes().await else {
        return Vec::new();
    };
    let mut envelopes = Vec::new();
    let mut excluded_by_policy = 0usize;
    for node in nodes.iter() {
        let meta: serde_json::Value = serde_json::from_str(&node.metadata).unwrap_or_default();
        if meta.get("federation_source").is_some() {
            continue; // echo-loop prevention (unchanged)
        }
        let space = permissions::resolve_space(node.topic_key.as_deref(), &node.metadata);
        if !peer.permissions.may_send_space(&space) {
            excluded_by_policy += 1;
            continue;
        }
        let node_json = serde_json::to_value(node).unwrap_or_default();
        if let Ok(e) = tylluan_link::identity::sign_node(&state.node_identity, &node_json) {
            envelopes.push(e);
        }
    }
    if excluded_by_policy > 0 {
        tracing::trace!(
            "push filter: {excluded_by_policy} node(s) excluded by space policy for '{}'",
            peer.name
        );
    }
    envelopes
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
            external_address: String::new(), permissions: crate::federation::PeerPermissions::default(),
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
                            // M12-B: fetch Ed25519 public key
                            if let Some(pubkey) = body.get("public_key").and_then(|v| v.as_str())
                                && !pubkey.is_empty() {
                                    let _ = state.peer_db.update_ed25519_pubkey(&name, pubkey);
                                    tracing::info!("🔑 Auto-fetched Ed25519 pubkey for peer '{}'", name);
                                }
                            // M12-C: fetch external address for hole-punching
                            if let Some(addr) = body.get("external_address").and_then(|v| v.as_str())
                                && !addr.is_empty() {
                                    let _ = state.peer_db.update_external_address(&name, addr);
                                    tracing::info!("🌐 Auto-fetched external address for peer '{}': {}", name, addr);
                                }
                            reload_peers_cache(&state).await;
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

// ── ADR-019 step 2: permissions endpoint ────────────────────────────────────

/// PUT /api/v1/federation/peers/{name}/permissions — admin-protected (NOT in
/// `is_public_bypass_path`). Body = PeerPermissions JSON. Rejects `default`
/// spaces, wildcards and malformed `guild.tool` entries. Dangerous dispatch
/// families produce a STRONG warning + audit entry (never an error).
pub async fn federation_set_permissions(
    State(state): State<Arc<HttpState>>,
    Path(name): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let perms: crate::federation::PeerPermissions = match serde_json::from_value(body) {
        Ok(p) => p,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
                "error": format!("invalid permissions body: {e}")
            }))).into_response()
        }
    };
    if let Err(msg) = permissions::validate_permissions(&perms) {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": msg }))).into_response();
    }
    let dangerous = permissions::dangerous_dispatch_entries(&perms);
    if !dangerous.is_empty() {
        tracing::warn!(
            "⚠️ STRONG: peer '{}' allow-lists code-executing tools (sandbox is OFF by default): {:?}",
            name, dangerous
        );
        permissions::audit_permission_denied(
            &name,
            "permissions-update",
            "federation",
            "set_permissions",
            &format!("dangerous families listed in dispatch_allow: {}", dangerous.join(", ")),
        );
    }
    let json_perms = permissions::permissions_to_json(&perms);
    match state.peer_db.update_permissions(&name, &json_perms) {
        Ok(true) => {
            reload_peers_cache(&state).await;
            tracing::info!("🔐 Federation peer '{}' permissions updated", name);
            (StatusCode::OK, Json(serde_json::json!({
                "status": "updated", "name": name, "permissions": perms
            }))).into_response()
        }
        Ok(false) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": format!("Peer '{}' not found", name)}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

// ── Push sync ────────────────────────────────────────────────────────────────

pub async fn perform_sync_push_internal(state: &Arc<HttpState>) -> anyhow::Result<usize> {
    let peers = state.config.read().await.federation_peers.clone();
    if peers.is_empty() {
        return Ok(0);
    }

    let mut synced_count = 0;
    let mut nodes_sent = 0usize;
    let client = reqwest::Client::new();

    for peer in &peers {
        if !peer.approved {
            tracing::warn!("⛔ Federation: skipping unapproved peer '{}'", peer.name);
            continue;
        }
        // ADR-019 step 2: per-peer body filtered by space+direction
        // (deny-by-default). A peer with nothing permitted is skipped.
        let envelopes = push_body_for_peer(state, peer).await;
        if envelopes.is_empty() {
            tracing::trace!("Federation: nothing permitted to push to '{}'", peer.name);
            continue;
        }
        let plain_body = serde_json::to_vec(&envelopes).unwrap_or_default();
        let encrypted = match crate::federation::encrypt_for_peer(&plain_body, &state.node_identity, peer) {
            Ok(enc) => enc,
            Err(e) => { tracing::error!("Federation encrypt failed for '{}': {}", peer.name, e); continue; }
        };
        let sync_url = peer_url_for_sync(peer, &client, "/api/v1/federation/sync/receive").await;
        let resp = client
            .post(&sync_url)
            .bearer_auth(&peer.auth_token)
            .header("content-type", "application/octet-stream")
            .body(encrypted)
            .send()
            .await;

        if let Ok(r) = resp && r.status().is_success() {
            synced_count += 1;
            nodes_sent += envelopes.len();
            let _ = state.peer_db.update_last_sync(&peer.name, now_secs());
        }
    }

    reload_peers_cache(state).await;
    let _ = state.broadcast_tx.send(serde_json::json!({
        "type": "federation_sync", "direction": "push",
        "synced": synced_count, "total_peers": peers.len(), "nodes_synced": nodes_sent,
        "ts": chrono::Utc::now().timestamp_millis()
    }));

    Ok(synced_count)
}

pub async fn federation_sync_push(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    match perform_sync_push_internal(&state).await {
        Ok(synced_count) => {
            let peers_len = state.config.read().await.federation_peers.len();
            let nodes_len = match state.silva.get_shareable_nodes().await {
                Ok(nodes) => nodes.len(),
                Err(_) => 0,
            };
            (StatusCode::OK, Json(serde_json::json!({
                "synced": synced_count,
                "total_peers": peers_len,
                "nodes_synced": nodes_len,
            }))).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
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

    let plain = match crate::federation::decrypt_from_peer(&body, &state.node_identity, &peer) {
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
        // M12-B counter: the helper re-verifies as part of the single gate.
        if has_pubkey && tylluan_link::identity::verify_envelope(envelope, &peer.ed25519_pubkey).is_ok() {
            verified += 1;
        }
        // ADR-019 step 2: THE single ingestion path (space gate + hardening).
        match ingest_from_peer(&state, &peer, envelope, "receive").await {
            IngestOutcome::Received => received += 1,
            IngestOutcome::Skipped(_) | IngestOutcome::Denied(_) => skipped += 1,
        }
    }

    let _ = state.broadcast_tx.send(serde_json::json!({
        "type": "federation_sync", "direction": "receive", "peer": &peer.name,
        "received": received, "skipped": skipped, "verified": verified, "total": envelopes.len(),
        "ts": chrono::Utc::now().timestamp_millis()
    }));
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

    // ADR-019 step 2: space+direction gate for THIS peer (deny-by-default).
    // Send-side exclusion is silent policy (trace) — auditing here would chain
    // one row per unshared node on every export/pull cycle.
    let mut excluded_by_policy = 0usize;
    let gated: Vec<_> = nodes_for_export
        .into_iter()
        .filter(|n| {
            let space = permissions::resolve_space(n.topic_key.as_deref(), &n.metadata);
            let allowed = peer.permissions.may_send_space(&space);
            if !allowed { excluded_by_policy += 1; }
            allowed
        })
        .collect();
    if excluded_by_policy > 0 {
        tracing::trace!(
            "export filter: {excluded_by_policy} node(s) excluded by space policy for '{}'",
            peer.name
        );
    }

    // M12-B: sign each node before encrypting
    let mut envelopes = Vec::with_capacity(gated.len());
    for node in &gated {
        let node_json = serde_json::to_value(node).unwrap_or_default();
        match tylluan_link::identity::sign_node(&state.node_identity, &node_json) {
            Ok(envelope) => envelopes.push(envelope),
            Err(e) => tracing::warn!("Failed to sign node '{}': {e}", node.id),
        }
    }

    let plain_body = serde_json::to_vec(&envelopes).unwrap_or_default();
    let encrypted = match crate::federation::encrypt_for_peer(&plain_body, &state.node_identity, &peer) {
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

    let client = reqwest::Client::new();
    let export_url = peer_url_for_sync(&peer, &client, "/api/v1/federation/sync/export").await;

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

    let plain = match crate::federation::decrypt_from_peer(&encrypted, &state.node_identity, &peer) {
        Ok(p) => p,
        Err(e) => return (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": format!("Decryption failed: {e}")}))).into_response(),
    };

    let envelopes: Vec<tylluan_link::identity::SignedEnvelope> = match serde_json::from_slice(&plain) {
        Ok(e) => e,
        Err(e) => return (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": format!("Invalid JSON from peer: {e}")}))).into_response(),
    };

    let (mut received, mut skipped) = (0u64, 0u64);

    for envelope in &envelopes {
        // ADR-019 step 2: THE single ingestion path (space gate + hardening).
        match ingest_from_peer(&state, &peer, envelope, "pull").await {
            IngestOutcome::Received => received += 1,
            IngestOutcome::Skipped(_) | IngestOutcome::Denied(_) => skipped += 1,
        }
    }

    let _ = state.peer_db.update_last_sync(&peer.name, now_secs());
    reload_peers_cache(&state).await;

    let _ = state.broadcast_tx.send(serde_json::json!({
        "type": "federation_sync", "direction": "pull", "peer": &peer.name,
        "received": received, "skipped": skipped, "total": envelopes.len(),
        "ts": chrono::Utc::now().timestamp_millis()
    }));
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

    // Push: per-peer body filtered by space+direction (ADR-019 step 2)
    let push_ok = {
        let envelopes = push_body_for_peer(&state, &peer).await;
        if envelopes.is_empty() {
            false
        } else if let Ok(plain) = serde_json::to_vec(&envelopes) {
            if let Ok(enc) = crate::federation::encrypt_for_peer(&plain, &state.node_identity, &peer) {
                let push_url = peer_url_for_sync(&peer, &client, "/api/v1/federation/sync/receive").await;
                matches!(
                    client.post(&push_url).bearer_auth(&peer.auth_token)
                        .header("content-type", "application/octet-stream")
                        .body(enc).send().await,
                    Ok(r) if r.status().is_success()
                )
            } else { false }
        } else { false }
    };

    // Pull: peer's /export → local SilvaDB
    let mut pulled = 0u64;
    let export_url = peer_url_for_sync(&peer, &client, "/api/v1/federation/sync/export").await;
    if let Ok(r) = client.get(&export_url).bearer_auth(&peer.auth_token).send().await
        && r.status().is_success()
            && let Ok(enc_bytes) = r.bytes().await
                && let Ok(plain) = crate::federation::decrypt_from_peer(&enc_bytes, &state.node_identity, &peer)
                    && let Ok(envelopes) = serde_json::from_slice::<Vec<tylluan_link::identity::SignedEnvelope>>(&plain) {
                        for envelope in &envelopes {
                            // ADR-019 step 2: THE single ingestion path (covers
                            // both sync_both upserts that used to sit here).
                            match ingest_from_peer(&state, &peer, envelope, "both").await {
                                IngestOutcome::Received => pulled += 1,
                                IngestOutcome::Skipped(_) | IngestOutcome::Denied(_) => {}
                            }
                        }
                    }

    let _ = state.peer_db.update_last_sync(&peer.name, now_secs());
    reload_peers_cache(&state).await;

    let _ = state.broadcast_tx.send(serde_json::json!({
        "type": "federation_sync", "direction": "both", "peer": &peer.name,
        "push_succeeded": push_ok, "pulled_nodes": pulled,
        "ts": chrono::Utc::now().timestamp_millis()
    }));
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

// ── M12-C: Hole-punch ping health check ──────────────────────────────────────

/// GET /api/v1/federation/ping
/// Lightweight endpoint used by peers to test if a direct hole-punched connection works.
/// Returns 200 OK with minimal body so peers can verify connectivity before attempting sync.
pub async fn federation_ping() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({"status": "pong"})))
}

// ── SLO + routing anchors (unchanged) ────────────────────────────────────────

pub async fn slo_summary_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let statuses = state.registry.status_all().await.unwrap_or_default();
    let silva = state.silva.clone();
    // TC-03: objetivo SLO desde `[observability]` (era el literal 99.9).
    let slo_target = state.config.read().await.observability.slo_target;
    let always_on_count = statuses.iter().filter(|s| s.always_on).count() as f64;
    let online_always_on = statuses.iter().filter(|s| s.always_on && s.running).count() as f64;
    let availability: f64 = if always_on_count > 0.0 { online_always_on / always_on_count * 100.0 } else { 100.0 };
    let node_count = silva.node_count().await.unwrap_or(0) as f64;
    let error_budget_remaining = error_budget_remaining_percent(availability, slo_target);
    let slo_status = slo_status_for(availability, slo_target);
    (StatusCode::OK, Json(serde_json::json!({
        "slo_target": slo_target,
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

/// TC-03: banda del SLO para una disponibilidad y objetivo dados —
/// `healthy` si cumple el objetivo, `degraded` dentro de 1.0 pp por debajo,
/// `violated` por debajo. Con el default 99.9 reproduce exactamente la banda
/// histórica (99.0–99.9 → `degraded`).
fn slo_status_for(availability: f64, target: f64) -> &'static str {
    if availability >= target {
        "healthy"
    } else if availability >= target - 1.0 {
        "degraded"
    } else {
        "violated"
    }
}

/// TC-03: porcentaje restante del error budget respecto al objetivo
/// configurado. Generalización directa de la fórmula histórica
/// `(availability - 99.9) / 0.1 * 100` → `(a - t) / (100 - t) * 100`,
/// idéntica con el default. Objetivo ≥ 100: sin división por cero.
fn error_budget_remaining_percent(availability: f64, target: f64) -> f64 {
    if target >= 100.0 {
        if availability >= target { 100.0 } else { 0.0 }
    } else {
        ((availability - target) / (100.0 - target) * 100.0).clamp(0.0, 100.0)
    }
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
        Some(engine) => match state.silva.reembed_anchors(&engine).await {
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
        let embedding = engine.as_ref()
            .and_then(|e| tokio::task::block_in_place(|| e.embed_batch_coalesced_routing(&entry.intent)).ok()
                .map(|emb| (emb, e.engine_id())));
        match state.silva.upsert_routing_anchor(
            &entry.guild,
            &entry.intent,
            &entry.source,
            embedding.as_ref().map(|(v, m)| (v.as_slice(), m.as_str())),
        ).await {
            Ok(_) => inserted += 1,
            Err(_) => errors += 1,
        }
    }
    Json(serde_json::json!({"inserted": inserted, "errors": errors})).into_response()
}

// ── M12-C: Hole-punch helper ────────────────────────────────────────────────

/// Given a peer and a path, returns the best URL to use for sync.
/// If the peer has an external_address (STUN-discovered) and a ping succeeds,
/// returns the hole-punched URL. Otherwise falls back to the configured URL.
async fn peer_url_for_sync(
    peer: &crate::federation::FederationPeer,
    client: &reqwest::Client,
    path: &str,
) -> String {
    let configured_url = format!("{}{}", peer.url.trim_end_matches('/'), path);

    if peer.external_address.is_empty() {
        return configured_url;
    }

    let ping_url = format!("http://{}/api/v1/federation/ping", peer.external_address);
    match client
        .get(&ping_url)
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            tracing::debug!(
                "🌐 Hole-punch success for '{}': {} → {} via external address",
                peer.name, configured_url, ping_url
            );
            format!("http://{}{}", peer.external_address, path)
        }
        _ => {
            tracing::debug!(
                "🌐 Hole-punch skipped for '{}': external address {} unreachable, using configured URL",
                peer.name, peer.external_address
            );
            configured_url
        }
    }
}

// ── M11-D: Scheduled Auto-Sync Helpers ──────────────────────────────────────

async fn push_to_peer_internal(
    state: &Arc<HttpState>,
    peer: &crate::federation::FederationPeer,
    client: &reqwest::Client,
    plain_body: &[u8],
) -> anyhow::Result<()> {
    let encrypted = crate::federation::encrypt_for_peer(plain_body, &state.node_identity, peer)?;
    let sync_url = peer_url_for_sync(peer, client, "/api/v1/federation/sync/receive").await;
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
    let export_url = peer_url_for_sync(peer, client, "/api/v1/federation/sync/export").await;
    let resp = client
        .get(&export_url)
        .bearer_auth(&peer.auth_token)
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("Peer responded with status {}", resp.status());
    }

    let encrypted = resp.bytes().await?.to_vec();
    let plain = crate::federation::decrypt_from_peer(&encrypted, &state.node_identity, peer)?;
    let envelopes: Vec<tylluan_link::identity::SignedEnvelope> = serde_json::from_slice(&plain)?;

    let mut received = 0;
    for envelope in &envelopes {
        // ADR-019 step 2: THE single ingestion path (space gate + hardening).
        match ingest_from_peer(state, peer, envelope, "pull").await {
            IngestOutcome::Received => received += 1,
            IngestOutcome::Skipped(_) | IngestOutcome::Denied(_) => {}
        }
    }

    let _ = state.peer_db.update_last_sync(&peer.name, now_secs());
    Ok(received)
}

pub fn spawn_auto_sync(state: Arc<HttpState>) {
    // Overlap guard: only one auto-sync cycle may be in flight at a time.
    // The 2026-08-30 GraphRAG CPU incident showed a 5s sync cycle firing
    // indefinitely over a saturated system; this guard guarantees a slow
    // cycle can never stack on top of a still-running one.
    let cycle_in_flight = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    tokio::spawn(async move {
        // Read configuration values initially. Prefer
        // federation.sync_interval_ms (new home, 2026-08-31); fall back to the
        // deprecated silva.sync_interval_ms only when federation left it at
        // default while the deprecated silva value was customized.
        let mut interval_ms = {
            let config = state.config.read().await;
            effective_sync_interval_ms(&config)
        };

        if interval_ms == 0 {
            tracing::info!("🔄 Federation auto-sync is disabled at startup (interval = 0)");
            return;
        }

        tracing::info!("🔄 Federation auto-sync background task started (interval = {interval_ms}ms)");

        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(interval_ms)).await;

            // Overlap guard: skip this tick if the previous cycle is still
            // running (a hung peer with per-request timeouts can still hold a
            // cycle past the interval). swap(true) then restore at cycle end.
            // RAII guard: a panic mid-cycle must ALSO release the flag —
            // without it, one panic kills auto-sync silently forever.
            if cycle_in_flight.swap(true, std::sync::atomic::Ordering::AcqRel) {
                tracing::warn!("🔄 Auto-sync: previous cycle still in flight — skipping this tick");
                continue;
            }
            struct CycleInFlightGuard<'a>(&'a std::sync::atomic::AtomicBool);
            impl Drop for CycleInFlightGuard<'_> {
                fn drop(&mut self) {
                    self.0.store(false, std::sync::atomic::Ordering::Release);
                }
            }
            let _cycle_guard = CycleInFlightGuard(&cycle_in_flight);

            // Re-read configuration values in case of runtime changes
            let (curr_interval, curr_mode) = {
                let config = state.config.read().await;
                let mode = if config.federation.auto_sync_mode.is_empty() {
                    "both".to_string()
                } else {
                    config.federation.auto_sync_mode.clone()
                };
                (effective_sync_interval_ms(&config), mode)
            };

            if curr_interval == 0 {
                tracing::info!("🔄 Federation auto-sync disabled dynamically");
                break;
            }
            interval_ms = curr_interval; // Update loop sleep duration

            tracing::info!("🔄 Starting scheduled auto-sync cycle (mode = '{curr_mode}')...");

            let peers = state.config.read().await.federation_peers.clone();
            // Explicit per-request timeout: reqwest::Client::new() has NONE
            // by default. Without this, the JoinSet below only stops one hung
            // peer from blocking the others -- its task still never
            // completes, join_next() never returns for it, and
            // cycle_in_flight (above) stays stuck at true forever, silently
            // killing auto-sync for every peer from then on. 15s matches the
            // conservative 30s default cycle interval with headroom to spare.
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new());

            // Per-peer, concurrent: one hung peer can no longer stall the
            // whole cycle (this was a sequential for-loop — every other peer
            // waited behind a single dead endpoint).
            let mut join_set = tokio::task::JoinSet::new();
            for peer in peers.into_iter().filter(|p| p.approved) {
                let state = state.clone();
                let client = client.clone();
                let curr_mode = curr_mode.clone();
                join_set.spawn(async move {
                    // ADR-019 step 2: per-peer body (space+direction filtered)
                    if curr_mode == "push" || curr_mode == "both" {
                        let envelopes = push_body_for_peer(&state, &peer).await;
                        if !envelopes.is_empty() {
                            let plain_body = serde_json::to_vec(&envelopes).unwrap_or_default();
                            match push_to_peer_internal(&state, &peer, &client, &plain_body).await {
                                Ok(_) => tracing::info!("🔄 Auto-sync: successfully pushed to '{}'", peer.name),
                                Err(e) => tracing::error!("🔄 Auto-sync: push to '{}' failed: {e}", peer.name),
                            }
                        }
                    }

                    if curr_mode == "pull" || curr_mode == "both" {
                        match pull_from_peer_internal(&state, &peer, &client).await {
                            Ok(n) => tracing::info!("🔄 Auto-sync: successfully pulled {n} nodes from '{}'", peer.name),
                            Err(e) => tracing::error!("🔄 Auto-sync: pull from '{}' failed: {e}", peer.name),
                        }
                    }
                });
            }
            while join_set.join_next().await.is_some() {}

            reload_peers_cache(&state).await;
        }
    });
}

/// 30s conservative default for small meshes (see FederationConfig docs).
const DEFAULT_SYNC_INTERVAL_MS: u64 = 30_000;

/// Resolve the effective auto-sync interval. New home is
/// federation.sync_interval_ms; the deprecated silva.sync_interval_ms is
/// honored only when federation was left at default while silva was
/// customized (legacy tylluan.toml migration path).
fn effective_sync_interval_ms(config: &crate::config::TylluanConfig) -> u64 {
    let fed = config.federation.sync_interval_ms;
    let silva = config.silva.sync_interval_ms;
    if fed != DEFAULT_SYNC_INTERVAL_MS {
        fed
    } else if silva != DEFAULT_SYNC_INTERVAL_MS {
        silva
    } else {
        fed
    }
}

pub async fn federation_identity(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let external_address = {
        let cache = state.nat_cache.read().await;
        cache.as_ref().map(|a| format!("{}:{}", a.ip, a.port))
    };
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "node_id": state.node_identity.node_id(),
            "public_key": state.node_identity.public_key_hex(),
            "tylluan_version": env!("CARGO_PKG_VERSION"),
            "external_address": external_address,
        })),
    )
}

// ── M12-C: NAT Traversal ──────────────────────────────────────────────────────

/// GET /api/v1/nat/external-address
/// Returns the detected external IP:port via STUN, or a cached result.
pub async fn nat_external_address(
    State(state): State<Arc<HttpState>>,
) -> impl IntoResponse {
    let cached = state.nat_cache.read().await;
    if let Some(ref addr) = *cached {
        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "external_ip": addr.ip.to_string(),
                "external_port": addr.port,
                "stun_server": addr.stun_server,
                "cached": true,
            })),
        ).into_response();
    }
    drop(cached);

    let config = tylluan_link::nat::NatConfig {
        stun_servers: state.config.read().await.nat.stun_servers.clone(),
        stun_timeout_secs: state.config.read().await.nat.stun_timeout_secs,
        stun_retries: state.config.read().await.nat.stun_retries,
    };

    match tylluan_link::nat::discover_external_addr(&config).await {
        Ok(addr) => {
            let mut cache = state.nat_cache.write().await;
            *cache = Some(addr.clone());
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "external_ip": addr.ip.to_string(),
                    "external_port": addr.port,
                    "stun_server": addr.stun_server,
                    "cached": false,
                })),
            ).into_response()
        }
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": format!("NAT discovery failed: {e}"),
            })),
        ).into_response(),
    }
}


#[cfg(test)]
mod step2_unit_tests {
    use super::*;

    /// T17: an ERROR reading freshness must REJECT, never fall through to upsert.
    #[test]
    fn t17_freshness_error_rejects() {
        let r: Result<Option<(String, bool, String)>, String> = Err("db exploded".to_string());
        assert_eq!(decide_freshness(r, "rh", "rt", "peer"), Err("freshness read error"));
    }

    #[test]
    fn freshness_ok_none_accepts_new_node() {
        let r: Result<Option<(String, bool, String)>, String> = Ok(None);
        assert_eq!(decide_freshness(r, "rh", "rt", "peer"), Ok(()));
    }

    #[test]
    fn freshness_identical_is_skipped() {
        let r: Result<Option<(String, bool, String)>, String> =
            Ok(Some(("same".into(), false, "2026-01-01 00:00:00".into())));
        assert!(decide_freshness(r, "same", "2026-01-01 00:00:00", "peer").is_err());
    }
}

#[cfg(test)]
mod tc03_slo_tests {
    use super::*;

    /// Bandas exactas con el objetivo por defecto (histórico intacto) y con
    /// un objetivo custom — las bandas SE MUEVEN con el objetivo.
    #[test]
    fn slo_bands_follow_the_configured_target() {
        assert_eq!(slo_status_for(100.0, 99.9), "healthy");
        assert_eq!(slo_status_for(99.95, 99.9), "healthy");
        assert_eq!(slo_status_for(99.5, 99.9), "degraded");
        assert_eq!(slo_status_for(99.0, 99.9), "degraded");
        assert_eq!(slo_status_for(98.5, 99.9), "violated");

        assert_eq!(slo_status_for(98.5, 98.0), "healthy");
        assert_eq!(slo_status_for(97.5, 98.0), "degraded");
        assert_eq!(slo_status_for(96.9, 98.0), "violated");
    }

    /// El error budget generaliza la fórmula histórica: con el default es
    /// idéntico; con un objetivo custom escala con (100 − target). En el
    /// objetivo exacto el restante es 0 (el budget se consume al alcanzar
    /// el límite de disponibilidad admitido).
    #[test]
    fn error_budget_follows_the_configured_target() {
        assert!((error_budget_remaining_percent(100.0, 99.9) - 100.0).abs() < 1e-9);
        assert!((error_budget_remaining_percent(99.95, 99.9) - 50.0).abs() < 1e-9);
        assert!((error_budget_remaining_percent(99.9, 99.9) - 0.0).abs() < 1e-9);
        assert!((error_budget_remaining_percent(99.0, 99.9) - 0.0).abs() < 1e-9);

        assert!((error_budget_remaining_percent(100.0, 98.0) - 100.0).abs() < 1e-9);
        assert!((error_budget_remaining_percent(99.0, 98.0) - 50.0).abs() < 1e-9);
        assert!((error_budget_remaining_percent(98.0, 98.0) - 0.0).abs() < 1e-9);
        assert!((error_budget_remaining_percent(97.0, 98.0) - 0.0).abs() < 1e-9);

        assert_eq!(error_budget_remaining_percent(0.0, 98.0), 0.0);
        assert_eq!(error_budget_remaining_percent(100.0, 100.0), 100.0);
        assert_eq!(error_budget_remaining_percent(99.0, 100.5), 0.0);
    }
}
