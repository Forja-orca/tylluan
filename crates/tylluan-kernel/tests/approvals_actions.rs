//! Acciones de juicio sobre approvals por HTTP (contrato v2 «juicios pendientes»).
//!
//! Cubre las tres que **no ejecutan** nada: `reject`, `hold` y `withdraw`.
//! `approve` queda fuera a propósito: ejecuta el plan y debe delegar en el camino
//! ya auditado (`approve_action`), no reimplementarlo aquí.
//!
//! Nota: no se fija `TYLLUAN_DATA_DIR`, así que `plans_dir()` devuelve `None` y el
//! store trabaja **solo en memoria** (no se escribe `pending_plans.json` en el repo).

mod support;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;
use tylluan_kernel::security::grants;
use tylluan_kernel::transport::http::auth::ACL_AGENT_ID;

/// Inicializa los stores (OnceLock: llamarlo en cada test es seguro) para que
/// ningún test dependa del orden de ejecución de los demás.
fn prep() {
    grants::init();
    grants::init_plan_store();
}

async fn seed(id: &str, autor: &str) {
    prep();
    grants::store_plan(
        id,
        "bash",
        "run",
        &serde_json::json!({"cmd": "echo hola"}),
        autor,
        "prueba de acciones",
    )
    .await;
}

async fn post(app: &axum::Router, path: &str, body: serde_json::Value, who: &str) -> (StatusCode, serde_json::Value) {
    let req = Request::builder()
        .method("POST")
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = ACL_AGENT_ID
        .scope(who.to_string(), async { app.clone().oneshot(req).await.unwrap() })
        .await;
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap_or_default();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

/// `hold` sin motivo no vale: pedimos el motivo en vez de aceptar un «no me convence».
#[tokio::test(flavor = "multi_thread")]
async fn hold_sin_motivo_devuelve_400_y_no_toca_el_plan() {
    let state = support::test_state("aprh-old").await;
    let app = support::build_test_app(state);
    seed("apr-hold-sinmotivo", "deep").await;

    let (status, body) = post(
        &app,
        "/api/v1/approvals/apr-hold-sinmotivo/hold",
        serde_json::json!({"reason": "   "}),
        "buffy",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "hold_requires_reason");
    let plan = grants::get_plan("apr-hold-sinmotivo").await.expect("el plan sigue");
    assert!(plan.held.is_none(), "un hold rechazado no debe marcar nada");
    grants::remove_plan("apr-hold-sinmotivo").await;
}

/// `hold` con motivo: el plan queda marcado **y sigue pendiente**.
#[tokio::test(flavor = "multi_thread")]
async fn hold_con_motivo_marca_y_no_consume() {
    let state = support::test_state("aprh-ok").await;
    let app = support::build_test_app(state);
    seed("apr-hold-ok", "deep").await;

    let (status, body) = post(
        &app,
        "/api/v1/approvals/apr-hold-ok/hold",
        serde_json::json!({"reason": "falta saber si es produccion"}),
        "buffy",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "held");
    let plan = grants::get_plan("apr-hold-ok").await.expect("hold no consume el plan");
    assert_eq!(plan.held.expect("nota de duda").reason, "falta saber si es produccion");
    grants::remove_plan("apr-hold-ok").await;
}

/// `withdraw` es solo del autor: un tercero recibe 403 y el plan sigue.
#[tokio::test(flavor = "multi_thread")]
async fn withdraw_de_un_tercero_es_403_y_el_plan_sigue() {
    let state = support::test_state("aprw-no").await;
    let app = support::build_test_app(state);
    seed("apr-withdraw-ajeno", "deep").await;

    let (status, body) = post(
        &app,
        "/api/v1/approvals/apr-withdraw-ajeno/withdraw",
        serde_json::json!({}),
        "buffy",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "not_author");
    assert!(grants::get_plan("apr-withdraw-ajeno").await.is_some());
    grants::remove_plan("apr-withdraw-ajeno").await;
}

/// …y el autor sí puede retirarlo, y desaparece.
#[tokio::test(flavor = "multi_thread")]
async fn withdraw_del_autor_borra_el_plan() {
    let state = support::test_state("aprw-si").await;
    let app = support::build_test_app(state);
    seed("apr-withdraw-propio", "deep").await;

    let (status, body) = post(
        &app,
        "/api/v1/approvals/apr-withdraw-propio/withdraw",
        serde_json::json!({"reason": "me equivoque de ruta"}),
        "deep",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "withdrawn");
    assert!(grants::get_plan("apr-withdraw-propio").await.is_none());
}

/// `reject`: descarta sin ejecutar y devuelve 404 si ya no está.
#[tokio::test(flavor = "multi_thread")]
async fn reject_borra_y_segundo_reject_es_404() {
    let state = support::test_state("aprr").await;
    let app = support::build_test_app(state);
    seed("apr-reject", "deep").await;

    // D4977 (H-20261116): rechazar lo AJENO ya no es posible. Antes este test lo daba por
    // bueno (otro agente rechazaba el plan de `deep` con 200): eso era el defecto.
    let (status, body) = post(
        &app,
        "/api/v1/approvals/apr-reject/reject",
        serde_json::json!({"reason": "no aplica"}),
        "buffy",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "not_author");
    assert!(grants::get_plan("apr-reject").await.is_some(), "el plan ajeno no se toca");

    // El autor si puede descartar su propio plan.
    let (status, body) = post(
        &app,
        "/api/v1/approvals/apr-reject/reject",
        serde_json::json!({"reason": "no aplica"}),
        "deep",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "rejected");
    assert!(grants::get_plan("apr-reject").await.is_none());

    let (status, body) = post(
        &app,
        "/api/v1/approvals/apr-reject/reject",
        serde_json::json!({}),
        "deep",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "not_found");
}

/// `approve` de un id que no existe: la tool delega y su «Action not found» se
/// traduce a **404**, no a 200 optimista.
#[tokio::test(flavor = "multi_thread")]
async fn approve_de_id_inexistente_es_404() {
    prep();
    let state = support::test_state("apra-404").await;
    let app = support::build_test_app(state);

    let (status, body) = post(
        &app,
        "/api/v1/approvals/no-existe-nada/approve",
        serde_json::json!({"grant_level": "this_time"}),
        "buffy",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body: {body}");
}

/// `approve` del plan propio sin ser operador: la guarda D-13 vive en
/// `approve_action`, y el handler solo traduce a **403 `self_approval`**. Es la
/// prueba de que el camino HTTP **delega** en vez de reimplementar la regla.
#[tokio::test(flavor = "multi_thread")]
async fn approve_del_plan_propio_es_403_self_approval() {
    let state = support::test_state("apra-self").await;
    let app = support::build_test_app(state);
    seed("apr-approve-propio", "deep").await;

    let (status, body) = post(
        &app,
        "/api/v1/approvals/apr-approve-propio/approve",
        serde_json::json!({}),
        "deep",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body: {body}");
    assert!(
        body.to_string().contains("self_approval"),
        "debe nombrar la regla: {body}"
    );
    let plan = grants::get_plan("apr-approve-propio").await.expect("un rechazo por D-13 no consume el plan");
    assert!(plan.held.is_none());
    grants::remove_plan("apr-approve-propio").await;
}

/// T4361: sin identidad ligada a la petición NO se retira un plan (antes, una
/// petición sin identidad se saltaba la comprobación de autor y retiraba lo ajeno).
#[tokio::test(flavor = "multi_thread")]
async fn withdraw_sin_identidad_es_401() {
    prep();
    let state = support::test_state("aprw-anon").await;
    let app = support::build_test_app(state);
    seed("apr-withdraw-anon", "deep").await;

    // Sin `ACL_AGENT_ID.scope`: identidad vacía, como una petición sin binding.
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/approvals/apr-withdraw-anon/withdraw")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::json!({}).to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    assert!(
        grants::get_plan("apr-withdraw-anon").await.is_some(),
        "un withdraw sin identidad no puede borrar el plan"
    );
    grants::remove_plan("apr-withdraw-anon").await;
}
