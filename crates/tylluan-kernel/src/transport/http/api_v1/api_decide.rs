//! # REST twin of `tylluan_decide`: `POST /api/v1/decide` (ADR-021)
//!
//! Same body (`DecideRequest`) and same response (`DecideResponse`) as the MCP
//! tool, same service call. Agent class (never operator). A request that fails
//! the schema limits is answered `400` with the canonical response
//! (`status: error`); the detail goes to the kernel log, as on the MCP door.

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use std::sync::Arc;

use crate::router::decision::{DecideRequest, DecideStatus};
use crate::transport::http::HttpState;

pub async fn decide_handler(
    State(state): State<Arc<HttpState>>,
    Json(req): Json<DecideRequest>,
) -> impl IntoResponse {
    let Some(server_arc) = state.server.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "service_unavailable",
                "detail": "sovereign server not initialized",
            })),
        )
            .into_response();
    };

    let service = server_arc.read().await.decision_service.clone();
    let resp = service.decide(req, "rest").await;
    let code = if resp.status == DecideStatus::Error {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::OK
    };
    (code, Json(resp)).into_response()
}
