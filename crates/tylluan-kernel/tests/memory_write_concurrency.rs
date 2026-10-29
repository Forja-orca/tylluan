//! T-DSH-134 — regresión de `memory_write_handler`: ids únicos con N escrituras
//! concurrentes.
//!
//! Antes del arreglo el `node_id` era `memory_write:{timestamp_millis}`: dos
//! escrituras en el mismo milisegundo compartían id, la segunda sobrescribía a
//! la primera y ambas devolvían 200. Con uuid v4 no hay colisión.

mod support;

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use tylluan_kernel::transport::http::HttpState;
use tylluan_kernel::transport::http::api_v1::api_memory::memory_write_handler;

async fn write_memory(state: &Arc<HttpState>, content: String) -> (StatusCode, serde_json::Value) {
    let resp = memory_write_handler(
        State(state.clone()),
        Json(serde_json::json!({ "content": content, "agent_id": "t134" })),
    )
    .await
    .into_response();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("body");
    let body = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, body)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_memory_writes_get_unique_node_ids() {
    let state = support::test_state("t134_memory_write_concurrency").await;
    const N: usize = 8;
    let mut tasks = Vec::new();
    for i in 0..N {
        let st = state.clone();
        tasks.push(tokio::spawn(async move {
            write_memory(&st, format!("T134 concurrent write {i}")).await
        }));
    }
    let mut ids = std::collections::HashSet::new();
    for t in tasks {
        let (status, body) = t.await.expect("task");
        assert_eq!(status, StatusCode::OK, "write: {body}");
        let id = body["node_id"].as_str().expect("node_id").to_string();
        assert!(ids.insert(id.clone()), "node_id repetido: {id}");
    }
    assert_eq!(ids.len(), N, "cada escritura concurrente debe tener su propio node_id");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn memory_write_returns_ok_and_node_id() {
    let state = support::test_state("t134_memory_write_single").await;
    let (status, body) = write_memory(&state, "T134 single write".to_string()).await;
    assert_eq!(status, StatusCode::OK, "write: {body}");
    let id = body["node_id"].as_str().expect("node_id");
    assert!(id.starts_with("memory_write:"), "id: {id}");
}
