//! T-DSH-140 (ADR-025 R2): vista REST de solo lectura del `TaskRegistry`.
//!
//! `GET /api/v1/runtime/tasks` — clase operador, solo lectura, campos saneados
//! (nombre, subruntime, estado, última ejecución, contadores). Nunca rutas,
//! agentes ni contenido. No hay herramienta MCP nueva (CONTRACT-01).

use axum::Json;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::runtime::tasks;

/// `GET /api/v1/runtime/tasks`
pub async fn runtime_tasks_handler() -> impl IntoResponse {
    let tasks = tasks::global().snapshot();
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "tasks": tasks,
            "count": tasks.len(),
        })),
    )
        .into_response()
}
