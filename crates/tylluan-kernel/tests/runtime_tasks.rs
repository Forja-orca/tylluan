//! T-DSH-140 — `GET /api/v1/runtime/tasks` (ADR-025 R2): solo lectura,
//! clase operador, campos saneados, sin tool MCP nueva.

use axum::response::IntoResponse;

use tylluan_kernel::transport::http::api_v1::api_runtime::runtime_tasks_handler;
use tylluan_kernel::transport::http::auth::operator_class_required;

#[tokio::test]
async fn runtime_tasks_returns_sanitized_registry() {
    // Registro global: la primera tarea migrada es el reindexer.
    tylluan_kernel::runtime::tasks::global().register("reindexer", "Memoria");

    let resp = runtime_tasks_handler().await.into_response();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("body");
    let body: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    let tasks = body["tasks"].as_array().expect("tasks array");
    assert!(body["count"].as_u64().expect("count") >= 1);
    let reindexer = tasks
        .iter()
        .find(|t| t["name"] == "reindexer")
        .expect("reindexer registrado");
    assert_eq!(reindexer["subruntime"], "Memoria");
    assert!(reindexer["state"].is_string());
    assert!(reindexer["runs"].is_u64());
    // Campos saneados: nunca rutas, agentes ni contenido.
    let raw = serde_json::to_string(&body).expect("json");
    for forbidden in ["path", "content", "agent_id"] {
        assert!(!raw.contains(forbidden), "campo prohibido en la vista: {forbidden}");
    }
}

#[test]
fn runtime_tasks_is_operator_class() {
    assert!(operator_class_required("GET", "/api/v1/runtime/tasks"));
    assert!(!operator_class_required("GET", "/health"));
}
