//! Guild outputs ledger — read-only HTTP surface (bwc-d0fb0812).
//!
//! `GET /api/v1/outputs`                    → recent run summaries (newest first)
//! `GET /api/v1/outputs/{run_id}/manifest`  → one full manifest
//!
//! Read-only by contract: producers are the guilds (artifacts) and the
//! registry actor (manifests). There is deliberately no write endpoint and
//! no delete endpoint — the ledger is not a filesystem API, it is an index.

use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use serde::Deserialize;

use crate::registry::outputs::OutputsStore;

#[derive(Debug, Deserialize)]
pub struct OutputsListQuery {
    pub guild: Option<String>,
    pub limit: Option<usize>,
}

/// GET /api/v1/outputs?guild=&limit=
pub async fn outputs_list_handler(Query(q): Query<OutputsListQuery>) -> impl IntoResponse {
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let store = OutputsStore::at_default_root();
    let guild = q.guild.clone();
    let runs = tokio::task::spawn_blocking(move || store.list_runs(guild.as_deref(), limit))
        .await
        .unwrap_or_default();
    Json(serde_json::json!({ "runs": runs }))
}

/// GET /api/v1/outputs/{run_id}/manifest
pub async fn outputs_manifest_handler(Path(run_id): Path<String>) -> impl IntoResponse {
    let store = OutputsStore::at_default_root();
    let manifest = tokio::task::spawn_blocking(move || store.read_manifest(&run_id)).await;
    match manifest {
        Ok(Some(m)) => Json(m).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "run not found" })),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "outputs read failed" })),
        )
            .into_response(),
    }
}
