//! T-DSH-12 — A4 de T-DSH-10: `@inbox` con payload **legacy** (sin la clave
//! `untrusted`, de antes de D-18) de un remitente externo debe mostrarse con el
//! banner de contenido no confiable; el criterio es de LECTURA: si el payload no
//! declara el flag y el remitente no tiene identidad local, se trata como
//! externo. Los payloads que sí declaran el flag (A3) no cambian, y un payload
//! legacy de un remitente LOCAL sigue sin banner.
//!
//! Run:
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test trust_inbox_legacy

mod support;

use axum::body::Body;
use axum::http::{header, Request};
use tower::ServiceExt;
use tylluan_kernel::memory::identity::{AgentIdentity, IdentityManager};
use tylluan_kernel::transport::http::build_router;

const MASTER_TOKEN: &str = "master-t12-token";
const BANNER: &str = "[CONTENIDO NO CONFIABLE";

async fn recall_inbox(app: &axum::Router, bearer: &str, agent: &str) -> String {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "tylluan_recall",
            "arguments": { "query": "@inbox", "agent_id": agent }
        }
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
    let raw = String::from_utf8_lossy(&bytes).to_string();
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
    parsed["result"]["content"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or(raw)
}

#[tokio::test(flavor = "multi_thread")]
async fn inbox_legacy_payloads_are_marked_untrusted_by_read() {
    let audit_db =
        std::env::temp_dir().join(format!("tylluan_t12_audit_{}.db", std::process::id()));
    let _ = std::fs::remove_file(&audit_db);
    // SAFETY (set_var): edition-2024 unsafe op; un único #[test] en el binario.
    unsafe { std::env::set_var("TYLLUAN_AUDIT_DB", &audit_db) };

    tylluan_kernel::security::grants::init();
    tylluan_kernel::security::grants::init_plan_store();

    let state = support::test_state_custom("t12", Some(MASTER_TOKEN.to_string()), false).await;
    let jwt = state.oauth.sign_jwt("t12-jti", 300);
    let app = build_router(state.clone());
    let silva = state.silva.clone();

    // Identidad LOCAL (para el control negativo del criterio de lectura).
    IdentityManager::new(silva.clone())
        .register_agent(&AgentIdentity::new(
            "local-t12",
            "Local T12",
            "agent",
            "remitente local del test",
        ))
        .await
        .unwrap();

    // A4: payload legacy (sin clave `untrusted`) de remitente EXTERNO.
    let legacy_external = serde_json::json!({
        "msg_type": "task",
        "body": "ZZLEGACYEXT cuerpo sin clave",
        "to": "victim-legacy-ext",
        "from": "ext-legacy"
    })
    .to_string();
    state
        .mailbox
        .send_mail_with_ttl(
            "ext-legacy",
            "victim-legacy-ext",
            &legacy_external,
            3600,
        )
        .await
        .unwrap();
    let text_ext = recall_inbox(&app, &jwt, "victim-legacy-ext").await;
    assert!(
        text_ext.contains("ZZLEGACYEXT"),
        "control: el mensaje debe aparecer: {text_ext}"
    );
    assert!(
        text_ext.contains(BANNER),
        "A4: un payload legacy de remitente externo debe llevar banner: {text_ext}"
    );

    // A3 (regresión): payload NUEVO que declara `untrusted: true` sigue con banner.
    let declared = serde_json::json!({
        "msg_type": "task",
        "body": "ZZDECLARED cuerpo marcado",
        "to": "victim-declared",
        "from": "ext-declared",
        "untrusted": true
    })
    .to_string();
    state
        .mailbox
        .send_mail_with_ttl("ext-declared", "victim-declared", &declared, 3600)
        .await
        .unwrap();
    let text_declared = recall_inbox(&app, &jwt, "victim-declared").await;
    assert!(
        text_declared.contains("ZZDECLARED") && text_declared.contains(BANNER),
        "A3: el flag declarado debe respetarse (banner): {text_declared}"
    );

    // Control negativo: payload legacy de remitente LOCAL -> SIN banner.
    let legacy_local = serde_json::json!({
        "msg_type": "task",
        "body": "ZZLEGACYLOCAL cuerpo local",
        "to": "victim-legacy-local",
        "from": "local-t12"
    })
    .to_string();
    state
        .mailbox
        .send_mail_with_ttl("local-t12", "victim-legacy-local", &legacy_local, 3600)
        .await
        .unwrap();
    let text_local = recall_inbox(&app, &jwt, "victim-legacy-local").await;
    assert!(
        text_local.contains("ZZLEGACYLOCAL"),
        "control negativo: el mensaje local debe aparecer: {text_local}"
    );
    assert!(
        !text_local.contains(BANNER),
        "control negativo: un remitente local NO debe marcarse: {text_local}"
    );

    // El flag declarado como `false` explícito se respeta (no se re-marca):
    // el productor tomó la decisión y el criterio de lectura no la pisa.
    let explicit_false = serde_json::json!({
        "msg_type": "task",
        "body": "ZZEXPLICITFALSE cuerpo declarado limpio",
        "to": "victim-explicit-false",
        "from": "ext-explicit-false",
        "untrusted": false
    })
    .to_string();
    state
        .mailbox
        .send_mail_with_ttl("ext-explicit-false", "victim-explicit-false", &explicit_false, 3600)
        .await
        .unwrap();
    let text_false = recall_inbox(&app, &jwt, "victim-explicit-false").await;
    assert!(
        text_false.contains("ZZEXPLICITFALSE") && !text_false.contains(BANNER),
        "el flag declarado como false no se pisa: {text_false}"
    );
}
