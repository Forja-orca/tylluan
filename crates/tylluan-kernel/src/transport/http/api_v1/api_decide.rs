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

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
};
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

// ── T1253: vista de auditoría del shadow log (clase OPERADOR) ────────────────

#[derive(serde::Deserialize)]
pub struct ShadowQuery {
    limit: Option<usize>,
}

/// `GET /api/v1/decide/shadow?limit=N` (T1253) — lectura del
/// `decision_shadow_log` para el panel de operador. Clase OPERADOR (ver
/// `operator_class_required`): token maestro por CABECERA; un JWT OAuth, un
/// token por query-string o un bypass de par no la satisfacen (403).
///
/// Shape CONGELADO (sync T1255): `{rows, summary, limit, total}` — sin alias.
/// - `rows`: los 11 campos públicos de la tabla (sin `model_probs_json`, sin
///   `model_latency_ms`; NUNCA `state` ni texto de preguntas — solo su
///   `request_hash`).
/// - `summary`: resumen por contrato SOBRE la ventana devuelta.
/// - `limit`: efectivo (por defecto 50; 0 ausente o 0 → 50; tope duro 500).
/// - `total`: COUNT(*) de la tabla entera.
pub async fn decide_shadow_handler(
    State(state): State<Arc<HttpState>>,
    Query(q): Query<ShadowQuery>,
) -> impl IntoResponse {
    let limit = match q.limit {
        None | Some(0) => 50,
        Some(n) => n.min(500),
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
    let overview = service.shadow_overview(limit).await;
    let rows: Vec<serde_json::Value> = overview
        .rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "ts": r.ts,
                "contract": r.contract,
                "request_hash": r.request_hash,
                "rules_value": r.rules_value,
                "model_value": r.model_value,
                "provider": r.provider,
                "latency_ms": r.latency_ms,
                "agreed": r.agreed,
                "mode": r.mode,
                "model_identity": r.model_identity,
            })
        })
        .collect();
    Json(serde_json::json!({
        "rows": rows,
        "summary": overview.summary,
        "limit": limit,
        "total": overview.total,
    }))
    .into_response()
}
