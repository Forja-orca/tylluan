//! H2 (revisión de Buffy / turno 1234): `/health` debe exponer **y chequear**
//! el recuento de herramientas soberanas — la promesa del mensaje de 44d5828
//! ("/health and /monitor expect tool_count == 6") era falsa: no existía
//! ningún chequeo. CONTRACT-01 fija exactamente 6 herramientas; este test
//! pina el componente `sovereign_tools` del health (count == 6, ok == true).

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use tylluan_kernel::transport::http::build_router;

#[tokio::test(flavor = "multi_thread")]
async fn health_reports_and_checks_six_sovereign_tools() {
    let app = build_router(support::test_state("health_sovereign").await);
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/health?verbose=true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "health must answer 200");
    let bytes = axum::body::to_bytes(resp.into_body(), 256 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).expect("health verbose must be JSON");
    let st = &json["components"]["sovereign_tools"];
    assert_eq!(
        st["count"],
        serde_json::json!(6),
        "CONTRACT-01: exactamente 6 herramientas soberanas: {json}"
    );
    assert_eq!(st["ok"], serde_json::json!(true), "el chequeo debe pasar: {json}");
}
