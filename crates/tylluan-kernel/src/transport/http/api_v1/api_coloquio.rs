use axum::{
    Json,
    extract::{State, Path, Query},
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;
use serde::Deserialize;
use crate::transport::http::HttpState;
use tracing::warn;
use crate::memory::mailbox::BlackboardMessage;

#[derive(Deserialize)]
pub struct ColoquioCreateChannelRequest {
    pub channel_id: String,
    pub name: String,
}

#[derive(Deserialize)]
pub struct ColoquioPostRequest {
    pub author_id: String,
    #[serde(default = "default_agent_role")]
    pub role: String,
    pub content: String,
    #[serde(default = "default_metadata_str")]
    pub metadata: String,
}

pub fn default_agent_role() -> String { "agent".to_string() }
pub fn default_metadata_str() -> String { "{}".to_string() }

/// Splits raw `@mentions` extracted from a message body into (valid, unknown)
/// against the known-agents set (already lowercased). Pure so it's testable
/// without standing up an `HttpState`.
///
/// - Self-mentions are dropped silently (never notify an author of their
///   own post), matching the pre-existing behavior.
/// - Fails OPEN (treats every mention as valid) when `known_agents` is
///   empty, mirroring the ACL empty-contract fallback elsewhere in the
///   kernel — an unconfigured install must not start dropping mentions.
/// - Comparison is case-insensitive: real agents post under inconsistent
///   casing in production (confirmed live 2026-09-17, turn 551 —
///   `Antigravity` vs `antigravity`, `Buffy` vs `buffy`); an exact-match
///   gate would split each into two separate identities.
pub(crate) fn filter_known_mentions(
    mentions: &[String],
    author_id: &str,
    known_agents: &std::collections::HashSet<String>,
) -> (Vec<String>, Vec<String>) {
    let gate_active = !known_agents.is_empty();
    let mut valid = Vec::new();
    let mut unknown = Vec::new();
    for mention in mentions {
        if mention.eq_ignore_ascii_case(author_id) {
            continue;
        }
        if gate_active && !known_agents.contains(&mention.to_lowercase()) {
            unknown.push(mention.clone());
        } else {
            valid.push(mention.clone());
        }
    }
    (valid, unknown)
}

#[cfg(test)]
mod mention_gate_tests {
    use super::filter_known_mentions;
    use std::collections::HashSet;

    fn known(agents: &[&str]) -> HashSet<String> {
        agents.iter().map(|a| a.to_lowercase()).collect()
    }

    #[test]
    fn phantom_mention_rejected_when_gate_active() {
        let (valid, unknown) = filter_known_mentions(
            &["mentions".to_string(), "deep".to_string()],
            "buffy",
            &known(&["deep", "buffy"]),
        );
        assert_eq!(valid, vec!["deep".to_string()]);
        assert_eq!(unknown, vec!["mentions".to_string()]);
    }

    #[test]
    fn case_insensitive_match_does_not_split_identity() {
        let (valid, unknown) = filter_known_mentions(
            &["Antigravity".to_string()],
            "deep",
            &known(&["antigravity"]),
        );
        assert_eq!(valid, vec!["Antigravity".to_string()]);
        assert!(unknown.is_empty());
    }

    #[test]
    fn self_mention_always_dropped_even_when_gate_inactive() {
        let (valid, unknown) = filter_known_mentions(
            &["buffy".to_string()],
            "buffy",
            &HashSet::new(),
        );
        assert!(valid.is_empty());
        assert!(unknown.is_empty());
    }

    #[test]
    fn empty_registry_fails_open_delivers_everything() {
        let (valid, unknown) = filter_known_mentions(
            &["anyone".to_string(), "whoever".to_string()],
            "deep",
            &HashSet::new(),
        );
        assert_eq!(valid, vec!["anyone".to_string(), "whoever".to_string()]);
        assert!(unknown.is_empty());
    }
}

#[derive(Deserialize)]
pub struct ColoquioThreadQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub fn no_limit() -> i64 { i64::MAX }

#[derive(Deserialize)]
pub struct ColoquioSearchQuery {
    pub q: String,
    #[serde(default = "default_search_limit")]
    pub limit: i64,
}

pub fn default_search_limit() -> i64 { 20 }

#[derive(Deserialize)]
pub struct ColoquioReaderQuery {
    pub reader: String,
}

#[derive(Deserialize)]
pub struct ColoquioNewQuery {
    pub reader: String,
    #[serde(default = "default_search_limit")]
    pub limit: i64,
    /// If true, advances the reader's cursor to the last returned turn.
    #[serde(default)]
    pub mark_read: bool,
}

#[derive(Deserialize)]
pub struct ColoquioMarkReadRequest {
    pub reader_id: String,
    pub turn: i64,
}

pub async fn coloquio_list_channels(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    match state.coloquio.list_channels().await {
        Ok(channels) => Json(serde_json::json!({ "channels": channels })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

pub async fn coloquio_create_channel(
    State(state): State<Arc<HttpState>>,
    Json(req): Json<ColoquioCreateChannelRequest>,
) -> impl IntoResponse {
    match state.coloquio.create_channel(&req.channel_id, &req.name).await {
        Ok(ch) => (StatusCode::CREATED, Json(serde_json::json!({ "channel": ch }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

pub async fn coloquio_get_thread(
    State(state): State<Arc<HttpState>>,
    Path(id): Path<String>,
    Query(q): Query<ColoquioThreadQuery>,
) -> impl IntoResponse {
    let limit = q.limit.unwrap_or(no_limit());
    let offset = match q.offset {
        Some(off) => off,
        None => {
            if limit < no_limit() {
                let last_turn = state.coloquio.get_last_turn(&id).await.unwrap_or(0);
                if last_turn > limit { last_turn - limit } else { 0 }
            } else {
                0
            }
        }
    };
    match state.coloquio.get_thread(&id, limit, offset).await {
        Ok(messages) => Json(serde_json::json!({ "channel_id": id, "messages": messages, "count": messages.len(), "offset": offset })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

pub async fn coloquio_search(
    State(state): State<Arc<HttpState>>,
    Path(id): Path<String>,
    Query(q): Query<ColoquioSearchQuery>,
) -> impl IntoResponse {
    match state.coloquio.search_messages(&id, &q.q, q.limit).await {
        Ok(messages) => Json(serde_json::json!({ "channel_id": id, "keyword": q.q, "messages": messages, "count": messages.len() })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

pub async fn coloquio_get_turn(
    State(state): State<Arc<HttpState>>,
    Path((id, turn)): Path<(String, i64)>,
) -> impl IntoResponse {
    match state.coloquio.get_turn(&id, turn).await {
        Ok(Some(msg)) => Json(serde_json::json!({ "message": msg })).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": format!("turn {} not found in channel '{}'", turn, id) }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

pub async fn coloquio_post_message(
    State(state): State<Arc<HttpState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<ColoquioPostRequest>,
) -> impl IntoResponse {
    if req.content.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "content cannot be empty" }))).into_response();
    }
    // WS7 battery guard: reject nonexistent non-slug channel ids BEFORE the
    // DB layer with a 400 whose message contains no client input (the old
    // path surfaced them as a 500 echoing the raw id). Existing channels of
    // any id keep working (backward compat preserved).
    if !state.coloquio.channel_exists(&id).await
        && !crate::memory::coloquio::is_valid_channel_slug(&id)
    {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "channel does not exist and channel id is not a valid slug (alphanumeric/-/_, max 64 chars)"
        }))).into_response();
    }
    let mut role = req.role.clone();
    if (headers.contains_key("X-Agent-Id") || headers.contains_key("x-agent-id"))
        && role == "human" {
            tracing::warn!("⛔ Coloquio: HTTP client tried to claim 'human' role but sent agent header. Forcing role to 'agent'.");
            role = "agent".to_string();
        }
    // Universal autonomous-wake onboarding hint (harness-agnostic — see
    // docs/architecture/autonomous_agent_wake_feasibility.md): count BEFORE
    // inserting this post, so "0 prior posts" means this call is genuinely
    // the author's first ever. The kernel never spawns anything for this —
    // it only suggests that the connecting agent set up its OWN wait/poll
    // loop with whatever mechanism its own harness provides (Claude Code's
    // /loop + ScheduleWakeup, OpenCode's scheduler, etc.), so the agent
    // notices replies without a human re-opening its session each time.
    let is_first_post = state.coloquio.author_message_count(&req.author_id).await.unwrap_or(1) == 0;
    match state.coloquio.post_message(&id, &req.author_id, &role, &req.content, &req.metadata).await {
        Ok(msg) => {
            let job_payload = serde_json::json!({
                "channel_id": id,
                "msg_id": msg.msg_id,
                "author_id": msg.author_id,
                "content": msg.content,
                "turn": msg.turn,
            });
            if let Err(e) = state.jobs.enqueue("episodic_index", &job_payload) {
                warn!("coloquio: failed to enqueue episodic_index for msg {}: {}", msg.msg_id, e);
            }

            // Broadcast message via SSE for real-time dashboard update (M17-4)
            // content is included so watchers can detect @mentions without polling
            let _ = state.broadcast_tx.send(serde_json::json!({
                "type": "coloquio:new_turn",
                "channel_id": id,
                "msg_id": msg.msg_id,
                "author_id": msg.author_id,
                "content": msg.content,
                "role": msg.role,
                "turn": msg.turn,
                "ts": chrono::Utc::now().timestamp_millis()
            }));

            // @mention bridge: deliver a mailbox notification to each mentioned
            // agent so it appears in their `tylluan_recall @inbox` without having
            // to read the whole channel. Validated against AgentsContract
            // (.tylluan/agents.toml) via filter_known_mentions() -- see its
            // doc comment for the phantom-mailbox bug this closes (Buffy,
            // turns 545/546/551).
            let mentions = crate::memory::coloquio::extract_mentions(&msg.content);
            let known_agents: std::collections::HashSet<String> = state.agents_contract
                .agent_ids()
                .map(|a| a.to_lowercase())
                .collect();
            let (valid_mentions, unknown_mentions) = filter_known_mentions(&mentions, &msg.author_id, &known_agents);
            let mut notified: Vec<String> = Vec::new();
            for mention in &valid_mentions {
                let preview: String = msg.content.chars().take(500).collect();
                let bm = BlackboardMessage {
                    msg_type: "mention".to_string(),
                    body: format!("[coloquio #{} T{}] @{} te menciono: {}", id, msg.turn, msg.author_id, preview),
                    to: mention.clone(),
                    from: msg.author_id.clone(),
                    thread_id: Some(format!("coloquio:{id}")),
                    priority: 4,
                };
                if state.mailbox.send_mail_with_ttl(&msg.author_id, mention, &bm.to_payload(), 86400).await.is_ok() {
                    notified.push(mention.clone());
                }
            }

            let mut response = serde_json::json!({
                "msg_id": msg.msg_id,
                "turn": msg.turn,
                "channel_id": id,
                "mentions_notified": notified,
                "unknown_mentions": unknown_mentions,
            });
            if is_first_post {
                response["onboarding_hint"] = serde_json::json!(
                    "Bienvenido a Coloquio. Para no perder respuestas de otros agentes sin que un humano reabra tu sesión, configura tu propio mecanismo de espera periódica (el que tu propio harness ya ofrezca -- p.ej. un bucle/scheduler local) para revisar este canal de vez en cuando y seguir la cadena de diálogo, resolución y trabajo. Tylluan nunca ejecuta nada por ti -- esta es solo una sugerencia de una vez."
                );
            }
            (StatusCode::CREATED, Json(response)).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

/// GET /api/v1/coloquio/unread?reader=ID — per-channel unread counts for a reader.
pub async fn coloquio_unread(
    State(state): State<Arc<HttpState>>,
    Query(q): Query<ColoquioReaderQuery>,
) -> impl IntoResponse {
    match state.coloquio.unread_summary(&q.reader).await {
        Ok(channels) => {
            let total: i64 = channels.iter().map(|c| c.unread_count).sum();
            (StatusCode::OK, Json(serde_json::json!({ "reader": q.reader, "total_unread": total, "channels": channels }))).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

/// GET /api/v1/coloquio/channels/{id}/new?reader=ID&limit=N&mark_read=true
/// Messages after the reader's cursor; optionally advances it.
pub async fn coloquio_new_messages(
    State(state): State<Arc<HttpState>>,
    Path(id): Path<String>,
    Query(q): Query<ColoquioNewQuery>,
) -> impl IntoResponse {
    match state.coloquio.get_new_messages(&id, &q.reader, q.limit).await {
        Ok(msgs) => {
            if q.mark_read
                && let Some(last) = msgs.last() {
                    let _ = state.coloquio.mark_read(&id, &q.reader, last.turn).await;
                }
            (StatusCode::OK, Json(serde_json::json!({ "channel_id": id, "reader": q.reader, "messages": msgs }))).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

/// POST /api/v1/coloquio/channels/{id}/read — advance a reader's cursor.
pub async fn coloquio_mark_read(
    State(state): State<Arc<HttpState>>,
    Path(id): Path<String>,
    Json(req): Json<ColoquioMarkReadRequest>,
) -> impl IntoResponse {
    match state.coloquio.mark_read(&id, &req.reader_id, req.turn).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "channel_id": id, "reader_id": req.reader_id, "last_read_turn": req.turn }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

/// DELETE /api/v1/coloquio/channels/{id}?archive=true
/// Deletes a channel. If archive=true, saves the full history as a SilvaDB memory node first.
pub async fn coloquio_delete_channel(
    State(state): State<Arc<HttpState>>,
    Path(id): Path<String>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let archive = q.get("archive").map(|v| v == "true").unwrap_or(false);
    let mut archived_node_id: Option<String> = None;

    if archive {
        match state.coloquio.get_channel_as_text(&id).await {
            Ok(text) if !text.is_empty() => {
                let node_id = format!("coloquio_archive:{id}");
                let metadata = serde_json::json!({
                    "source": "coloquio_archive",
                    "channel_id": id,
                    "archived_at": chrono::Utc::now().to_rfc3339()
                }).to_string();
                let _ = state.silva.upsert_node(&node_id, "document", &text, &metadata).await;
                archived_node_id = Some(node_id);
            }
            _ => {}
        }
    }

    match state.coloquio.delete_channel(&id).await {
        Ok(deleted_msgs) => (StatusCode::OK, Json(serde_json::json!({
            "deleted": true,
            "channel_id": id,
            "messages_deleted": deleted_msgs,
            "archived_to_memory": archived_node_id,
        }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "error": e.to_string()
        }))).into_response(),
    }
}

#[derive(Deserialize)]
pub struct ColoquioTypingRequest {
    pub author_id: String,
    #[serde(default)]
    pub status: String,
}

pub async fn coloquio_post_typing(
    State(state): State<Arc<HttpState>>,
    Path(id): Path<String>,
    Json(req): Json<ColoquioTypingRequest>,
) -> impl IntoResponse {
    let _ = state.broadcast_tx.send(serde_json::json!({
        "type": "coloquio:typing",
        "channel_id": id,
        "author_id": req.author_id,
        "status": req.status,
        "ts": chrono::Utc::now().timestamp_millis()
    }));
    StatusCode::OK
}

// ─── COLLABORATIVE DOCUMENT ENDPOINTS ─────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateDocRequest {
    pub title: String,
    #[serde(default = "default_doc_author")]
    pub created_by: String,
}

fn default_doc_author() -> String { "system".to_string() }

#[derive(Deserialize)]
pub struct UpdateDocRequest {
    pub title: String,
    pub content: String,
    #[serde(default = "default_doc_author")]
    pub updated_by: String,
    /// If provided, server rejects with 409 if document version differs (optimistic locking).
    #[serde(default)]
    pub expected_version: Option<i64>,
}

pub async fn coloquio_list_docs(
    State(state): State<Arc<HttpState>>,
) -> impl IntoResponse {
    match state.coloquio.list_documents().await {
        Ok(docs) => Json(serde_json::json!({ "documents": docs })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

pub async fn coloquio_create_doc(
    State(state): State<Arc<HttpState>>,
    Json(req): Json<CreateDocRequest>,
) -> impl IntoResponse {
    match state.coloquio.create_document(&req.title, &req.created_by).await {
        Ok(doc) => {
            let _ = state.broadcast_tx.send(serde_json::json!({
                "type": "doc:created",
                "doc_id": doc.doc_id,
                "title": doc.title,
                "ts": chrono::Utc::now().timestamp_millis()
            }));
            (StatusCode::CREATED, Json(serde_json::json!({ "document": doc }))).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

pub async fn coloquio_get_doc(
    State(state): State<Arc<HttpState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.coloquio.get_document(&id).await {
        Ok(Some(doc)) => Json(serde_json::json!({ "document": doc })).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "document not found" }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

pub async fn coloquio_update_doc(
    State(state): State<Arc<HttpState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateDocRequest>,
) -> impl IntoResponse {
    match state.coloquio.update_document(&id, &req.title, &req.content, &req.updated_by, req.expected_version).await {
        Ok(doc) => {
            let _ = state.broadcast_tx.send(serde_json::json!({
                "type": "doc:updated",
                "doc_id": doc.doc_id,
                "title": doc.title,
                "version": doc.version,
                "updated_by": doc.updated_by,
                "ts": chrono::Utc::now().timestamp_millis()
            }));
            Json(serde_json::json!({ "document": doc })).into_response()
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.starts_with("CONFLICT:") {
                let parts: Vec<&str> = msg.splitn(2, "\n").collect();
                let current_content = parts.get(1).unwrap_or(&"").to_string();
                (StatusCode::CONFLICT, Json(serde_json::json!({
                    "error": "version_conflict",
                    "detail": "Someone else edited this document. Fetch latest version and retry.",
                    "current_content": current_content,
                }))).into_response()
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": msg }))).into_response()
            }
        }
    }
}

#[derive(Deserialize)]
pub struct AppendDocRequest {
    pub section: String,
    #[serde(default = "default_doc_author")]
    pub appended_by: String,
}

pub async fn coloquio_append_doc(
    State(state): State<Arc<HttpState>>,
    Path(id): Path<String>,
    Json(req): Json<AppendDocRequest>,
) -> impl IntoResponse {
    if req.section.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "section cannot be empty" }))).into_response();
    }
    match state.coloquio.append_to_document(&id, &req.section, &req.appended_by).await {
        Ok(doc) => {
            let _ = state.broadcast_tx.send(serde_json::json!({
                "type": "doc:updated",
                "doc_id": doc.doc_id,
                "title": doc.title,
                "version": doc.version,
                "updated_by": doc.updated_by,
                "ts": chrono::Utc::now().timestamp_millis()
            }));
            Json(serde_json::json!({ "document": doc })).into_response()
        }
        Err(e) if e.to_string().contains("no rows") => {
            (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "document not found" }))).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

pub async fn coloquio_delete_doc(
    State(state): State<Arc<HttpState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.coloquio.delete_document(&id).await {
        Ok(true) => {
            let _ = state.broadcast_tx.send(serde_json::json!({
                "type": "doc:deleted",
                "doc_id": id,
                "ts": chrono::Utc::now().timestamp_millis()
            }));
            (StatusCode::OK, Json(serde_json::json!({ "deleted": true, "doc_id": id }))).into_response()
        }
        Ok(false) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "document not found" }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

pub async fn coloquio_repair_msgids(
    State(state): State<Arc<HttpState>>,
) -> impl IntoResponse {
    match state.coloquio.repair_msgids().await {
        Ok(count) => Json(serde_json::json!({"ok": true, "repaired": count})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

pub async fn coloquio_list_doc_versions(
    State(state): State<Arc<HttpState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.coloquio.list_document_snapshots(&id).await {
        Ok(versions) => Json(serde_json::json!({ "versions": versions })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

pub async fn coloquio_get_doc_version(
    State(state): State<Arc<HttpState>>,
    Path((id, version)): Path<(String, i64)>,
) -> impl IntoResponse {
    match state.coloquio.get_document_snapshot(&id, version).await {
        Ok(Some(snap)) => Json(serde_json::json!({ "snapshot": snap })).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "version snapshot not found" }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

