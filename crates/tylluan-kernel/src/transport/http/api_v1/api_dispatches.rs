//! # API Dispatches — HTTP endpoints for the Coloquio Push Dispatcher (BWC-2)
//!
//! Implements BWC-2 of `docs/architecture/coloquio_dispatcher_spec.md` (fb069dd).
//!
//! Endpoints:
//! - `GET /api/v1/dispatches`: list pending dispatches (optional agent_id filter)
//! - `GET /api/v1/dispatches/{id}`: get dispatch by ID
//! - `POST /api/v1/dispatches/{id}/approve`: approve dispatch with expected hash and CAS transition
//! - `POST /api/v1/dispatches/{id}/reject`: reject dispatch (human says no)

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::security::dispatch_queue::{
    dispatch_db_path, ApprovalOutcome, DispatchQueue, PendingDispatch,
};
use crate::transport::http::HttpState;

#[derive(Debug, Deserialize, Serialize)]
pub struct ListDispatchesQuery {
    pub agent_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ApproveDispatchPayload {
    pub expected_hash: String,
}

fn open_queue() -> Result<DispatchQueue, (StatusCode, Json<serde_json::Value>)> {
    let path = dispatch_db_path();
    DispatchQueue::open(path.to_str().unwrap_or("./data/pending_dispatches.db")).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "queue_open_failed",
                "message": e.to_string(),
            })),
        )
    })
}

/// List all pending dispatches, optionally filtered by `agent_id`.
pub async fn dispatches_list_handler(
    State(_state): State<Arc<HttpState>>,
    Query(params): Query<ListDispatchesQuery>,
) -> Result<Json<Vec<PendingDispatch>>, (StatusCode, Json<serde_json::Value>)> {
    let queue = open_queue()?;
    let dispatches = queue.list_pending().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })?;

    if let Some(ref target_agent) = params.agent_id {
        let filtered: Vec<PendingDispatch> = dispatches
            .into_iter()
            .filter(|d| d.agent_id.eq_ignore_ascii_case(target_agent))
            .collect();
        Ok(Json(filtered))
    } else {
        Ok(Json(dispatches))
    }
}

/// Get a single dispatch by ID.
pub async fn dispatch_get_handler(
    State(_state): State<Arc<HttpState>>,
    Path(id): Path<String>,
) -> Result<Json<PendingDispatch>, (StatusCode, Json<serde_json::Value>)> {
    let queue = open_queue()?;
    match queue.get(&id) {
        Ok(Some(dispatch)) => Ok(Json(dispatch)),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "not_found", "id": id })),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )),
    }
}

/// Approve a pending dispatch with hash-binding and CAS atomicity.
pub async fn dispatch_approve_handler(
    State(_state): State<Arc<HttpState>>,
    Path(id): Path<String>,
    Json(body): Json<ApproveDispatchPayload>,
) -> impl IntoResponse {
    let queue = match open_queue() {
        Ok(q) => q,
        Err(err) => return err.into_response(),
    };
    match queue.approve(&id, &body.expected_hash) {
        Ok(ApprovalOutcome::Transitioned) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "approved",
                "id": id,
            })),
        )
            .into_response(),
        Ok(ApprovalOutcome::AlreadyResolved(st)) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "already_resolved",
                "state": st.as_str(),
                "id": id,
            })),
        )
            .into_response(),
        Ok(ApprovalOutcome::NotFound) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "not_found",
                "id": id,
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "hash_mismatch",
                "message": e.to_string(),
                "id": id,
            })),
        )
            .into_response(),
    }
}

/// Reject a pending dispatch (human rejection).
pub async fn dispatch_reject_handler(
    State(_state): State<Arc<HttpState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let queue = match open_queue() {
        Ok(q) => q,
        Err(err) => return err.into_response(),
    };
    match queue.reject(&id) {
        Ok(ApprovalOutcome::Transitioned) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "rejected",
                "id": id,
            })),
        )
            .into_response(),
        Ok(ApprovalOutcome::AlreadyResolved(st)) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "already_resolved",
                "state": st.as_str(),
                "id": id,
            })),
        )
            .into_response(),
        Ok(ApprovalOutcome::NotFound) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "not_found",
                "id": id,
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": e.to_string(),
                "id": id,
            })),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::dispatch_queue::DispatchQueue;

    #[test]
    fn test_approve_payload_serde() {
        let json = r#"{"expected_hash":"abcdef123456"}"#;
        let payload: ApproveDispatchPayload = serde_json::from_str(json).unwrap();
        assert_eq!(payload.expected_hash, "abcdef123456");
    }

    #[test]
    fn test_list_query_serde() {
        let json = r#"{"agent_id":"deep"}"#;
        let query: ListDispatchesQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.agent_id.as_deref(), Some("deep"));
    }

    #[test]
    fn test_dispatch_queue_direct_api_flow() {
        let queue = DispatchQueue::in_memory().unwrap();
        let d = queue
            .enqueue(
                "deep",
                "claude-code",
                "general",
                565,
                "@deep fix the bug",
                vec!["opencode".into(), "run".into()],
            )
            .unwrap();

        assert_eq!(d.agent_id, "deep");
        let list = queue.list_pending().unwrap();
        assert_eq!(list.len(), 1);

        // Mismatch fails
        let err = queue.approve(&d.id, "wrong_hash");
        assert!(err.is_err());

        // Correct hash succeeds
        let res = queue.approve(&d.id, &d.content_hash).unwrap();
        assert_eq!(res, ApprovalOutcome::Transitioned);

        // Second approve is conflict (AlreadyResolved)
        let res2 = queue.approve(&d.id, &d.content_hash).unwrap();
        assert_eq!(
            res2,
            ApprovalOutcome::AlreadyResolved(
                crate::security::dispatch_queue::DispatchState::Approved
            )
        );

        // Unknown ID is NotFound
        let res3 = queue.approve("unknown_id", "some_hash").unwrap();
        assert_eq!(res3, ApprovalOutcome::NotFound);
    }

    #[test]
    fn test_dispatch_queue_reject_flow() {
        let queue = DispatchQueue::in_memory().unwrap();
        let d = queue
            .enqueue(
                "antigravity",
                "claude-code",
                "general",
                566,
                "@antigravity update UI",
                vec!["npm".into(), "run".into(), "build".into()],
            )
            .unwrap();

        let res = queue.reject(&d.id).unwrap();
        assert_eq!(res, ApprovalOutcome::Transitioned);

        let res2 = queue.reject(&d.id).unwrap();
        assert_eq!(
            res2,
            ApprovalOutcome::AlreadyResolved(
                crate::security::dispatch_queue::DispatchState::Rejected
            )
        );
    }
}
