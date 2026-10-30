//! H-52 (C-2, AUD-08): los contadores del filtro de huella de embeddings son
//! visibles en `GET /api/v1/system/status` (endpoint de solo lectura ya
//! existente; sin ruta nueva). Harness compartido en `tests/support/mod.rs`.
//!
//! Cobertura honesta: registro de la ruta en el router real, acceso a los
//! contadores atómicos de `SilvaDB` y contrato de claves del JSON — end to end
//! a través del handler real. El lado que INCREMENTA los contadores
//! (`search_hybrid` con `embedding_fingerprint_filter=true`) lo ejercitan los
//! lib tests de `memory/silva/search.rs`; activar el filtro en producción está
//! fuera de alcance (así lo fija la spec C-2).

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test(flavor = "multi_thread")]
async fn system_status_exposes_fingerprint_counters_and_keeps_rest_intact() {
    let state = support::test_state("fpcount").await;
    let app = support::build_test_app(state.clone());

    let res = app
        .oneshot(Request::get("/api/v1/system/status").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    // El resto del JSON queda INTACTO (exigencia de aceptación de C-2).
    for key in ["silva_healthy", "mailbox_healthy", "curriculum_entries", "uptime_secs", "score", "system"] {
        assert!(body.get(key).is_some(), "clave preexistente perdida: {key} — body: {body}");
    }

    // Bloque nuevo: las cuatro claves, con el estado de arranque de un
    // SilvaDB recién abierto (sin consultas todavía → contadores en 0,
    // filtro apagado por defecto).
    let fp = &body["embedding_fingerprint"];
    assert!(fp.is_object(), "falta el bloque embedding_fingerprint — body: {body}");
    assert_eq!(fp["filter_enabled"], false, "el filtro sigue apagado por defecto — fp: {fp}");
    assert_eq!(fp["would_exclude_model"], 0);
    assert_eq!(fp["would_exclude_dim"], 0);
    assert_eq!(fp["excluded"], 0);

    // Coherencia con la fuente real: los contadores del handler coinciden con
    // `SilvaDB::fingerprint_observation()` en el mismo instante.
    let (m, d, e) = state.silva.fingerprint_observation();
    assert_eq!(fp["would_exclude_model"], m);
    assert_eq!(fp["would_exclude_dim"], d);
    assert_eq!(fp["excluded"], e);
    assert_eq!(fp["filter_enabled"], state.silva.fingerprint_filter_enabled());
}
