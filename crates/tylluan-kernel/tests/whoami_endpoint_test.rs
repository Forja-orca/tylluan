//! Integration test for GET /api/v1/whoami through the real router (AG-4).
//!
//! Verifies:
//! 1. GET /api/v1/whoami returns 200 OK with expected JSON structure.
//! 2. Default agent_id is "anonymous".
//! 3. Honors X-Agent-Id header.
//! 4. Honors ?agent_id= query param.
//! 5. Honors ?timezone= parameter (reporting local time and weekday).

mod support;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;

#[tokio::test(flavor = "multi_thread")]
async fn test_whoami_endpoint_anonymous_default() {
    let state = support::test_state("whoami_anon").await;
    let app = support::build_test_app(state);

    let req = Request::builder()
        .uri("/api/v1/whoami")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "whoami should return 200");

    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["agent_id"], "anonymous");
    assert!(json["now"]["utc"].is_string());
    assert!(json["now"]["unix_epoch"].is_number());
    assert!(json["world_grounding_hint"].is_string());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_whoami_endpoint_with_header_and_timezone() {
    let state = support::test_state("whoami_hdr").await;
    let app = support::build_test_app(state);

    let req = Request::builder()
        .uri("/api/v1/whoami?timezone=Europe/Madrid")
        .header("X-Agent-Id", "antigravity-test-agent")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "whoami with header should return 200");

    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["agent_id"], "antigravity-test-agent");
    assert_eq!(json["now"]["local"]["timezone"], "Europe/Madrid");
    assert!(json["now"]["local"]["local_time"].is_string());
    assert!(json["now"]["local"]["weekday"].is_string());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_whoami_endpoint_query_param_precedence() {
    let state = support::test_state("whoami_q").await;
    let app = support::build_test_app(state);

    let req = Request::builder()
        .uri("/api/v1/whoami?agent_id=explicit-query-agent")
        .header("X-Agent-Id", "header-agent")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["agent_id"], "explicit-query-agent");
}
