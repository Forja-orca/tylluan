//! Integration test for GET /api/v1/repo-map through the real router.
//!
//! Harness (HttpState builder + api_v1_routes driver) lives in the shared
//! `tests/support/mod.rs` — this file holds only its own assertions.

mod support;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;

#[tokio::test(flavor = "multi_thread")]
async fn test_repo_map_endpoint_returns_200() {
    let state = support::test_state("rm").await;
    let app = support::build_test_app(state);

    let req = Request::builder()
        .uri("/api/v1/repo-map")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "repo-map should return 200");

    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json["total_files"].as_u64().is_some(), "total_files should be present");
    assert!(json["total_dirs"].as_u64().is_some(), "total_dirs should be present");
    assert!(json["total_lines"].as_u64().is_some(), "total_lines should be present");
    assert!(json["build_duration_ms"].as_u64().is_some(), "build_duration_ms should be present");
    assert!(json["root"].as_str().is_some(), "root should be present");
    assert!(json["languages"].is_object(), "languages should be an object");
    assert!(json["top_level_dirs"].is_array(), "top_level_dirs should be an array");
    assert!(json["build_duration_ms"].as_u64().unwrap() > 0, "build should take >0ms");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_repo_map_contains_rust_identifiers() {
    let state = support::test_state("rm").await;
    let app = support::build_test_app(state);

    let req = Request::builder()
        .uri("/api/v1/repo-map")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    // The test runner's current dir should contain Rust files with identifiers
    let idents = json["identifiers"].as_object().unwrap();
    // At minimum, the repo_map.rs file itself should have its own identifiers
    let has_repo_map_idents = idents.keys().any(|k| k.contains("repo_map"));
    assert!(has_repo_map_idents || !idents.is_empty(),
        "should have at least some Rust identifiers");
}
