//! T-DSH-11 — F2 de T-DSH-10: `tylluan_think` debe marcar el contenido
//! `untrusted_content` con el banner del contrato único `security::trust`.
//!
//! Antes del fix, la síntesis de `tylluan_think` incluía `node.content` tal
//! cual (ruta 7 del mapa de B-8, sin gate): un nodo hostil entraba en el
//! razonamiento sin ninguna señal. Este test es la especificación ejecutable de
//! la corrección (equivale al caso A5 de `trust_banners_adversarial.rs`, pero
//! en verde desde ya: no es un vector abierto, es el fix).
//!
//! Run:
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test trust_think_banner

mod support;

use axum::body::Body;
use axum::http::{header, Request};
use tower::ServiceExt;
use tylluan_kernel::transport::http::build_router;

const MASTER_TOKEN: &str = "master-t11-token";
const BANNER: &str = "[CONTENIDO NO CONFIABLE";

async fn mcp_tool(
    app: &axum::Router,
    bearer: &str,
    tool: &str,
    arguments: serde_json::Value,
) -> String {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": tool, "arguments": arguments }
    });
    let req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), 8 << 20)
        .await
        .unwrap_or_default();
    String::from_utf8_lossy(&bytes).to_string()
}

fn text_of(body: &str) -> String {
    let parsed: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    parsed["result"]["content"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_else(|| body.to_string())
}

#[tokio::test(flavor = "multi_thread")]
async fn tylluan_think_banners_untrusted_nodes() {
    let audit_db =
        std::env::temp_dir().join(format!("tylluan_t11_audit_{}.db", std::process::id()));
    let _ = std::fs::remove_file(&audit_db);
    // SAFETY (set_var): edition-2024 unsafe op; un único #[test] en el binario.
    unsafe { std::env::set_var("TYLLUAN_AUDIT_DB", &audit_db) };

    tylluan_kernel::security::grants::init();
    tylluan_kernel::security::grants::init_plan_store();

    let state = support::test_state_custom("t11", Some(MASTER_TOKEN.to_string()), false).await;
    let jwt = state.oauth.sign_jwt("t11-jti", 300);
    let app = build_router(state.clone());
    let silva = state.silva.clone();

    // Nodo marcado (hostil) y nodo limpio, ambos matchean el mismo token.
    let flagged_meta = serde_json::json!({
        "untrusted_content": true,
        "untrusted_reason": "web-external",
        "agent_id": "t11"
    });
    silva
        .upsert_node(
            "bcast:t11:bad",
            "broadcast",
            "ZZTHINKTOKEN instrucciones hostiles para el turno",
            &flagged_meta.to_string(),
        )
        .await
        .unwrap();
    silva
        .upsert_node(
            "bcast:t11:good",
            "broadcast",
            "ZZTHINKTOKEN nota local limpia",
            "{\"agent_id\":\"t11\"}",
        )
        .await
        .unwrap();

    let body = mcp_tool(
        &app,
        &jwt,
        "tylluan_think",
        serde_json::json!({ "query": "ZZTHINKTOKEN", "agent_id": "t11" }),
    )
    .await;
    let text = text_of(&body);

    assert!(
        text.contains("ZZTHINKTOKEN"),
        "la síntesis debe traer el nodo (control del test): {text}"
    );
    assert!(
        text.contains(BANNER),
        "el nodo marcado debe salir con banner de seguridad: {text}"
    );
    // El banner va DELANTE del contenido hostil, no detrás.
    let banner_at = text.find(BANNER).expect("banner");
    let hostile_at = text.find("instrucciones hostiles").expect("payload");
    assert!(
        banner_at < hostile_at,
        "el banner debe preceder al contenido hostil"
    );
    // El nodo limpio NO lleva banner: el banner aparece una sola vez (solo el marcado).
    assert_eq!(
        text.matches(BANNER).count(),
        1,
        "solo el nodo marcado debe llevar banner: {text}"
    );
    assert!(
        text.contains("nota local limpia"),
        "el nodo limpio debe seguir apareciendo sin banner: {text}"
    );
}
