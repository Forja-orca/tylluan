//! ADR-019/OAuth Fase 1 — integration tests over the REAL production router
//! (`build_router`), so the bearer → operator-gate middleware ORDER is
//! exercised, not just the pure functions.
//!
//! R13: authorize refuses with dev_mode=false (closes the LAN JWT-dispenser).
//! R14: operator-class endpoints require the master token (OAuth JWT → 403);
//!      the same request with the master token passes the gate (handler 404);
//!      no credentials at all → 401 by the bearer middleware.
//! R15 + R12: loopback-only redirects; dev_mode=true keeps auto-approval.

mod support;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;
use tylluan_kernel::transport::http::build_router;

const PERMS_BODY: &str = r#"{"sync_spaces":[],"dispatch_allow":[],"coloquio_channels":[]}"#;

/// R13: with dev_mode=false the OAuth authorize endpoint must refuse.
#[tokio::test(flavor = "multi_thread")]
async fn r13_authorize_refuses_without_dev_mode() {
    let state = support::test_state_custom(
        "oauth_f1_r13",
        Some("master-test-token".to_string()),
        false,
    )
    .await;
    let app = build_router(state);
    let req = Request::builder()
        .method("GET")
        .uri("/oauth/authorize?redirect_uri=http%3A%2F%2F127.0.0.1%3A9999%2Fcb&code_challenge=abc&code_challenge_method=S256")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "authorize must refuse when dev_mode=false (Fase 1-a)"
    );
}

/// R14: operator-class endpoints (here: federation permissions PUT) require
/// master-token auth; an OAuth JWT must be rejected by the gate.
#[tokio::test(flavor = "multi_thread")]
async fn r14_operator_class_requires_master_token() {
    let state = support::test_state_custom(
        "oauth_f1_r14",
        Some("master-test-token".to_string()),
        false,
    )
    .await;
    let jwt = state.oauth.sign_jwt("test-jti-f1", 60);
    let app = build_router(state);

    // 1) OAuth JWT on an operator-class endpoint → 403 by the operator gate.
    let req = Request::builder()
        .method("PUT")
        .uri("/api/v1/federation/peers/nonexistent/permissions")
        .header(header::AUTHORIZATION, format!("Bearer {jwt}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(PERMS_BODY))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "an OAuth JWT must NOT satisfy the operator class"
    );

    // 2) Master token → gate passes; the handler reports the missing peer (404).
    let req = Request::builder()
        .method("PUT")
        .uri("/api/v1/federation/peers/nonexistent/permissions")
        .header(header::AUTHORIZATION, "Bearer master-test-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(PERMS_BODY))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "master token passes the gate; handler answers peer-not-found"
    );

    // 3) No credentials at all → 401 by the bearer middleware (RCE chain dead).
    let req = Request::builder()
        .method("PUT")
        .uri("/api/v1/federation/peers/nonexistent/permissions")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(PERMS_BODY))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// R15 + R12: with dev_mode=true, loopback redirects keep working (auto-approval
/// intact) and non-loopback redirects are rejected.
#[tokio::test(flavor = "multi_thread")]
async fn r15_loopback_enforced_and_r12_dev_mode_intact() {
    let state = support::test_state("oauth_f1_r15").await; // dev_mode=true harness
    let app = build_router(state);

    let ok = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/oauth/authorize?redirect_uri=http%3A%2F%2F127.0.0.1%3A9999%2Fcb&code_challenge=abc&code_challenge_method=S256")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        ok.status().is_redirection(),
        "dev_mode=true keeps the local auto-approval flow (R12): got {}",
        ok.status()
    );

    let bad = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/oauth/authorize?redirect_uri=http%3A%2F%2Fevil.example%2Fcb&code_challenge=abc&code_challenge_method=S256")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        bad.status(),
        StatusCode::BAD_REQUEST,
        "non-loopback redirect_uri must be rejected (R15)"
    );
}
