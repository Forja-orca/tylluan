//! T1253 — endpoints de diagnóstico de clase OPERADOR (sync T1255: shapes
//! congelados para los paneles del dashboard):
//! - `GET /api/v1/decide/shadow?limit=N` — auditoría del decision_shadow_log
//!   (`{rows, summary, limit, total}`; 11 campos por fila; sin state ni texto
//!   de preguntas).
//! - `GET /api/v1/silva/conflicted?limit=N` — nodos con `conflicted = 1`
//!   (`{nodes, total_conflicted, limit}`; excerpt ≤300 chars saneados).
//!
//! Cubre: gate de operador sobre el router de PRODUCCIÓN (403 con JWT OAuth,
//! 401 sin credenciales, 200 con token maestro), límite duro, resumen por
//! contrato sobre filas sembradas, y que la respuesta no filtra el state.

mod support;

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;

use tylluan_kernel::router::decision::api_types::{DecideRequest, QuestionKind, QuestionSpec};
use tylluan_kernel::transport::http::{HttpState, build_router};

const MASTER: &str = "master-test-token";

// ── Helpers ─────────────────────────────────────────────────────────────────

/// INSERT directo (determinista) de una fila de `decision_shadow_log` sobre
/// la SilvaDB in-memory del estado de test.
async fn seed_shadow_row(state: &Arc<HttpState>, contract: &str, agreed: Option<i64>) {
    let g = state.silva.conn_lock();
    let conn = g.lock().await;
    conn.execute(
        "INSERT INTO decision_shadow_log
         (ts, contract, request_hash, rules_value, provider, latency_ms, agreed, mode)
         VALUES ('2026-10-18T00:00:00Z', ?1, ?2, 'seeded', 'rules', 1, ?3, 'shadow')",
        rusqlite::params![contract, format!("hash-{contract}-{agreed:?}"), agreed],
    )
    .expect("seed shadow row");
}

async fn get_json(app: &axum::Router, uri: &str) -> serde_json::Value {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "GET {uri}");
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).expect("valid JSON body")
}

// ── /api/v1/decide/shadow ───────────────────────────────────────────────────

/// Gate de operador sobre el router de producción: JWT OAuth → 403, sin
/// credenciales → 401, token maestro por cabecera → 200.
#[tokio::test(flavor = "multi_thread")]
async fn shadow_requires_operator_master_token() {
    let state = support::test_state_custom(
        "t1253_shadow_auth",
        Some(MASTER.to_string()),
        false,
    )
    .await;
    let jwt = state.oauth.sign_jwt("t1253-shadow", 60);
    let app = build_router(state);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/decide/shadow")
                .header(header::AUTHORIZATION, format!("Bearer {jwt}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "an OAuth JWT must NOT satisfy the operator class on the shadow read"
    );

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/decide/shadow")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/decide/shadow")
                .header(header::AUTHORIZATION, format!("Bearer {MASTER}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "the master token by header passes the gate"
    );
}

/// Límite duro + resumen por contrato calculado SOBRE la ventana devuelta +
/// total de la tabla entera (shape congelado `{rows, summary, limit, total}`).
#[tokio::test(flavor = "multi_thread")]
async fn shadow_limit_respected_and_summary_correct() {
    let state = support::test_state("t1253_shadow_shape").await;
    // 5 filas: c_a agreed 1/1/0, c_b agreed 1/NULL — las 2 más recientes (id
    // DESC) son c_b/1 y c_b/NULL.
    seed_shadow_row(&state, "c_a", Some(1)).await;
    seed_shadow_row(&state, "c_a", Some(1)).await;
    seed_shadow_row(&state, "c_a", Some(0)).await;
    seed_shadow_row(&state, "c_b", Some(1)).await;
    seed_shadow_row(&state, "c_b", None).await;
    let app = support::build_test_app(state);

    let v = get_json(&app, "/api/v1/decide/shadow?limit=2").await;
    assert_eq!(v["limit"], 2, "effective limit echoed");
    assert_eq!(v["total"], 5, "total is the whole-table COUNT(*)");
    let rows = v["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "limit respected");
    // id DESC: la más reciente es c_b/agreed NULL, debajo c_b/agreed 1.
    assert_eq!(rows[0]["contract"], "c_b");
    assert!(rows[0]["agreed"].is_null(), "NULL agreed stays JSON null");
    assert_eq!(rows[1]["contract"], "c_b");
    assert_eq!(rows[1]["agreed"], 1);

    let summary = v["summary"].as_array().unwrap();
    assert_eq!(summary.len(), 1, "summary covers only the window");
    assert_eq!(summary[0]["contract"], "c_b");
    assert_eq!(summary[0]["n"], 2);
    assert_eq!(summary[0]["n_agreed"], 1);
    assert_eq!(summary[0]["n_disagreed"], 0);
    assert_eq!(summary[0]["n_null"], 1);

    // Límites degenerados: ausente o 0 → 50; 9999 → techo 500.
    let v = get_json(&app, "/api/v1/decide/shadow").await;
    assert_eq!(v["limit"], 50);
    assert_eq!(v["rows"].as_array().unwrap().len(), 5);
    let v = get_json(&app, "/api/v1/decide/shadow?limit=0").await;
    assert_eq!(v["limit"], 50, "0 falls back to the default");
    let v = get_json(&app, "/api/v1/decide/shadow?limit=9999").await;
    assert_eq!(v["limit"], 500, "hard cap at 500");
}

/// El state del request NUNCA aparece en la respuesta (solo su hash) y cada
/// fila lleva exactamente los 11 campos congelados (sin model_probs_json ni
/// model_latency_ms).
#[tokio::test(flavor = "multi_thread")]
async fn shadow_response_carries_no_state_or_question_text() {
    let state = support::test_state("t1253_shadow_leak").await;
    const MARKER: &str = "T1253_LEAK_MARKER_XYZ";
    let svc = state
        .server
        .as_ref()
        .unwrap()
        .read()
        .await
        .decision_service
        .clone();
    let req = DecideRequest {
        state: serde_json::json!({ "marker": MARKER }),
        questions: BTreeMap::from([(
            "leak_check".to_string(),
            QuestionSpec {
                kind: QuestionKind::Choice,
                instructions: None,
                options: vec!["a".into(), "b".into()],
                levels: vec![],
            },
        )]),
        contract: Some("leak_check_contract".to_string()),
        max_latency_ms: None,
    };
    let resp = svc.decide(req, "test").await;
    assert!(
        matches!(
            resp.status,
            tylluan_kernel::router::decision::api_types::DecideStatus::Ok
                | tylluan_kernel::router::decision::api_types::DecideStatus::Degraded
        ),
        "rules answer or abstain-degraded — both audit the row"
    );

    let app = support::build_test_app(state);
    let v = get_json(&app, "/api/v1/decide/shadow?limit=50").await;
    let body = v.to_string();
    assert!(
        !body.contains(MARKER),
        "the state text must never reach the shadow read"
    );
    assert!(
        !body.contains("model_probs_json"),
        "model_probs_json is not part of the frozen shape"
    );
    let rows = v["rows"].as_array().unwrap();
    assert!(!rows.is_empty(), "the decide call must have seeded a row");
    let expected_keys: std::collections::BTreeSet<&str> = [
        "id",
        "ts",
        "contract",
        "request_hash",
        "rules_value",
        "model_value",
        "provider",
        "latency_ms",
        "agreed",
        "mode",
        "model_identity",
    ]
    .into_iter()
    .collect();
    for row in rows {
        let keys: std::collections::BTreeSet<&str> =
            row.as_object().unwrap().keys().map(|s| s.as_str()).collect();
        assert_eq!(keys, expected_keys, "exactly the 11 frozen row fields");
    }
}

// ── /api/v1/silva/conflicted ────────────────────────────────────────────────

/// Mismo gate de operador que la vista de sombra.
#[tokio::test(flavor = "multi_thread")]
async fn conflicted_requires_operator_master_token() {
    let state = support::test_state_custom(
        "t1253_conflicted_auth",
        Some(MASTER.to_string()),
        false,
    )
    .await;
    let jwt = state.oauth.sign_jwt("t1253-conflicted", 60);
    let app = build_router(state);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/silva/conflicted")
                .header(header::AUTHORIZATION, format!("Bearer {jwt}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/silva/conflicted")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/silva/conflicted")
                .header(header::AUTHORIZATION, format!("Bearer {MASTER}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

/// Límite duro, total real y excerpt acotado a 300 chars saneados; los nodos
/// NO marcados como conflicted no aparecen.
#[tokio::test(flavor = "multi_thread")]
async fn conflicted_limit_and_excerpt_cap() {
    let state = support::test_state("t1253_conflicted_shape").await;
    state
        .silva
        .upsert_node("t1253_long", "fact", &"x".repeat(500), "{}")
        .await
        .unwrap();
    state.silva.mark_conflicted("t1253_long", true).await.unwrap();
    state
        .silva
        .upsert_node("t1253_short_a", "fact", "alpha", "{}")
        .await
        .unwrap();
    state
        .silva
        .mark_conflicted("t1253_short_a", true)
        .await
        .unwrap();
    state
        .silva
        .upsert_node("t1253_short_b", "fact", "beta", "{}")
        .await
        .unwrap();
    state
        .silva
        .mark_conflicted("t1253_short_b", true)
        .await
        .unwrap();
    state
        .silva
        .upsert_node("t1253_clean", "fact", "clean node", "{}")
        .await
        .unwrap();
    let app = support::build_test_app(state);

    let v = get_json(&app, "/api/v1/silva/conflicted?limit=3").await;
    assert_eq!(v["limit"], 3);
    assert_eq!(v["total_conflicted"], 3, "only conflicted=1 nodes count");
    let nodes = v["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 3);
    assert!(
        nodes.iter().all(|n| n["id"] != "t1253_clean"),
        "a non-conflicted node must never appear"
    );
    let long = nodes
        .iter()
        .find(|n| n["id"] == "t1253_long")
        .expect("long node in the window");
    let excerpt = long["content_excerpt"].as_str().unwrap();
    assert_eq!(
        excerpt.chars().count(),
        300,
        "excerpt capped at 300 Unicode chars"
    );

    let v = get_json(&app, "/api/v1/silva/conflicted?limit=2").await;
    assert_eq!(v["limit"], 2);
    assert_eq!(v["nodes"].as_array().unwrap().len(), 2, "limit respected");
    assert_eq!(v["total_conflicted"], 3, "total independent of the limit");

    let v = get_json(&app, "/api/v1/silva/conflicted").await;
    assert_eq!(v["limit"], 50, "default limit");
    let v = get_json(&app, "/api/v1/silva/conflicted?limit=9999").await;
    assert_eq!(v["limit"], 200, "hard cap at 200");
}

/// T1277-B1: los caracteres Unicode de FORMATO (Cf — zero-width, bidi
/// overrides, BOM) NO sobreviven en el excerpt: pueden ocultar o reordenar
/// texto en el panel de operador. Los de control (Cc) siguen siendo espacio.
#[tokio::test(flavor = "multi_thread")]
async fn conflicted_excerpt_strips_format_chars() {
    let state = support::test_state("t1277_cf_strip").await;
    state
        .silva
        .upsert_node(
            "t1277_bidi",
            "fact",
            "pago\u{202E}GNP.exe\u{202C} ok\u{200B}fin\u{FEFF}",
            "{}",
        )
        .await
        .unwrap();
    state
        .silva
        .mark_conflicted("t1277_bidi", true)
        .await
        .unwrap();
    let app = support::build_test_app(state);

    let v = get_json(&app, "/api/v1/silva/conflicted?limit=50").await;
    let nodes = v["nodes"].as_array().unwrap();
    let node = nodes
        .iter()
        .find(|n| n["id"] == "t1277_bidi")
        .expect("bidi node present");
    let excerpt = node["content_excerpt"].as_str().unwrap();
    for bad in ['\u{202E}', '\u{202C}', '\u{200B}', '\u{FEFF}'] {
        assert!(
            !excerpt.contains(bad),
            "Cf {bad:?} must be stripped: {excerpt:?}"
        );
    }
    assert_eq!(excerpt, "pagoGNP.exe okfin", "Cf removed, visible text intact");
}
