//! E2E test verifying Coloquio thread pagination (limit/tail, since_turn, before_turn).

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test(flavor = "multi_thread")]
async fn test_coloquio_thread_tail_and_incremental_pagination() {
    let state = support::test_state("coloquio_pagination").await;
    let app = tylluan_kernel::transport::http::api_v1::routes::api_v1_routes().with_state(state.clone());

    // Create channel and post 10 messages directly into ColoquioDb
    state.coloquio.create_channel("test-chat", "Test Chat").await.unwrap();
    for i in 1..=10 {
        state.coloquio.post_message("test-chat", "alice", "human", &format!("msg {i}"), "{}").await.unwrap();
    }

    // 1. GET with limit=3 (no offset) returns the last 3 messages (turns 8, 9, 10 in ASC order)
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/coloquio/channels/test-chat?limit=3")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024).await.unwrap();
    let val: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let msgs = val["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 3, "must return exactly 3 messages");
    assert_eq!(msgs[0]["turn"], 8);
    assert_eq!(msgs[1]["turn"], 9);
    assert_eq!(msgs[2]["turn"], 10);

    // 2. GET with since_turn=8 returns messages with turn > 8 (turns 9, 10 in ASC order)
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/coloquio/channels/test-chat?since_turn=8")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024).await.unwrap();
    let val: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let msgs = val["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 2, "must return turns 9 and 10");
    assert_eq!(msgs[0]["turn"], 9);
    assert_eq!(msgs[1]["turn"], 10);

    // 3. GET with before_turn=8&limit=3 returns messages with turn < 8 (turns 5, 6, 7 in ASC order)
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/coloquio/channels/test-chat?before_turn=8&limit=3")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024).await.unwrap();
    let val: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let msgs = val["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 3, "must return turns 5, 6, 7");
    assert_eq!(msgs[0]["turn"], 5);
    assert_eq!(msgs[1]["turn"], 6);
    assert_eq!(msgs[2]["turn"], 7);
}
