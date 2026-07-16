use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;
use crate::transport::http::HttpState;

/// M31-P4: GET /api/v1/repo-map
/// Returns the lightweight repo map built once at kernel startup.
pub async fn repo_map_handler(
    State(state): State<Arc<HttpState>>,
) -> impl IntoResponse {
    let json = serde_json::to_value(&*state.repo_map).unwrap_or(serde_json::json!({"error": "serialization failed"}));
    (StatusCode::OK, Json(json))
}
