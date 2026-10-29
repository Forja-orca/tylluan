//! ADR-019 / OAuth Fase 2 (D-3) — desafíos de scope sobre el router REAL
//! (`build_router`), para fijar las respuestas que MCP exige:
//!
//! - 401 sin credenciales: `WWW-Authenticate` con `resource_metadata` (RFC 9728)
//!   y `scope="mcp"`.
//! - 403 en ruta de clase operador con JWT OAuth válido:
//!   `error="insufficient_scope"`, `scope="operator"` y el mismo puntero.
//! - 403 con `?token=` en ruta de operador: `operator_required` genérico, SIN
//!   `insufficient_scope` (el medio no se arregla pidiendo un scope).
//! - El token maestro sigue pasando la puerta (404 del handler).

mod support;

use axum::body::Body;
use axum::http::{header, Request, Response, StatusCode};
use tower::ServiceExt;
use tylluan_kernel::transport::http::build_router;

const PERMS_BODY: &str = r#"{"sync_spaces":[],"dispatch_allow":[],"coloquio_channels":[]}"#;
const PRM: &str = "http://localhost:3030/.well-known/oauth-protected-resource";
const OPERATOR_ROUTE: &str = "/api/v1/federation/peers/nonexistent/permissions";
const OPERATOR_READ: &str = "/api/v1/silva/conflicted";

fn challenge(resp: &Response<Body>) -> String {
    resp.headers()
        .get(header::WWW_AUTHENTICATE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

/// 401: el cliente MCP debe poder descubrir el AS desde el desafío.
#[tokio::test(flavor = "multi_thread")]
async fn unauthorized_challenge_points_to_resource_metadata() {
    let state = support::test_state_custom(
        "oauth_f2_scope_401",
        Some("master-test-token".to_string()),
        false,
    )
    .await;
    let app = build_router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(OPERATOR_READ)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let c = challenge(&resp);
    assert!(c.starts_with("Bearer "), "challenge: {c}");
    assert!(c.contains(&format!("resource_metadata=\"{PRM}\"")), "challenge: {c}");
    assert!(c.contains("scope=\"mcp\""), "challenge: {c}");
}

/// 403 de scope: un JWT OAuth válido pero de clase `mcp` en ruta de operador.
#[tokio::test(flavor = "multi_thread")]
async fn oauth_jwt_gets_insufficient_scope_on_operator_route() {
    let state = support::test_state_custom(
        "oauth_f2_scope_403",
        Some("master-test-token".to_string()),
        false,
    )
    .await;
    let jwt = state.oauth.sign_jwt("test-jti-d3-403", 60);
    let app = build_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(OPERATOR_ROUTE)
                .header(header::AUTHORIZATION, format!("Bearer {jwt}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(PERMS_BODY))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let c = challenge(&resp);
    assert!(c.contains("error=\"insufficient_scope\""), "challenge: {c}");
    assert!(c.contains("scope=\"operator\""), "challenge: {c}");
    assert!(c.contains(&format!("resource_metadata=\"{PRM}\"")), "challenge: {c}");

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["error"], "insufficient_scope");
    assert_eq!(v["scope"], "operator");
}

/// El token por query-string NO recibe `insufficient_scope`: pedir un scope no
/// arregla que el medio no satisfaga la clase operador.
#[tokio::test(flavor = "multi_thread")]
async fn query_token_on_operator_route_has_no_scope_challenge() {
    let state = support::test_state_custom(
        "oauth_f2_scope_query",
        Some("master-test-token".to_string()),
        false,
    )
    .await;
    let app = build_router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("{OPERATOR_READ}?token=master-test-token"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let c = challenge(&resp);
    assert!(!c.contains("insufficient_scope"), "challenge: {c}");

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["error"], "operator_required");
}

/// Fricción cero: el token maestro sigue pasando la puerta (404 del handler).
#[tokio::test(flavor = "multi_thread")]
async fn master_token_still_passes_operator_gate() {
    let state = support::test_state_custom(
        "oauth_f2_scope_master",
        Some("master-test-token".to_string()),
        false,
    )
    .await;
    let app = build_router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(OPERATOR_ROUTE)
                .header(header::AUTHORIZATION, "Bearer master-test-token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(PERMS_BODY))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "master token passes the gate; handler answers peer-not-found"
    );
}

/// D-72: the dashboard needs a stable machine-readable reason to show
/// "missing token" instead of guessing from a generic 401.
#[tokio::test(flavor = "multi_thread")]
async fn unauthorized_without_credentials_has_missing_token_code() {
    let state = support::test_state_custom(
        "oauth_f2_scope_missing_token",
        Some("master-test-token".to_string()),
        false,
    )
    .await;
    let app = build_router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(OPERATOR_READ)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let bytes = axum::body::to_bytes(resp.into_body(), 16 * 1024)
        .await
        .expect("read 401 body");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("401 body is JSON");
    assert_eq!(json["code"], "missing_token", "body: {json}");
}
