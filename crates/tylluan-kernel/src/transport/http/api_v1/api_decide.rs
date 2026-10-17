//! # REST twin of `tylluan_decide`: `POST /api/v1/decide` (ADR-021)
//!
//! Same body (`DecideRequest`) and same response (`DecideResponse`) as the MCP
//! tool, same service call. Agent class (never operator). A request that fails
//! the schema limits — or whose body is not deserializable at all — is
//! answered `400` with the canonical response (`status: error`); the detail
//! goes to the kernel log, as on the MCP door.
//!
//! H1 (revisión de Buffy, turno 1234): el cuerpo se lee en crudo y se parsea
//! aquí — un JSON malformado debe producir el sobre canónico, nunca el
//! rechazo en texto plano del extractor `Json` de axum.

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use std::sync::Arc;

use crate::router::decision::{DecideRequest, DecideResponse, DecideStatus};
use crate::transport::http::HttpState;

pub async fn decide_handler(
    State(state): State<Arc<HttpState>>,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // H1: parseo manual → el fallo de deserialización responde el mismo
    // sobre canónico que un fallo de límites (400 + `status: error`).
    let req: DecideRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("decide: malformed request body from rest caller: {e}");
            return (StatusCode::BAD_REQUEST, Json(DecideResponse::error())).into_response();
        }
    };

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
