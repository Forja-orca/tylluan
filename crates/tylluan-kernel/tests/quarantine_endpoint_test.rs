//! D-12 — endpoints de cuarentena ASI06 sobre el router REAL (`build_router`).
//!
//! Contrato fijado con Antigravity (lote AE); el panel del dashboard ya esta
//! escrito contra estos nombres de campo:
//!
//!   GET  /api/v1/memory/quarantine?limit=50&offset=0
//!        -> {"total":N,"nodes":[{"id","content_preview","quarantine_reason",
//!            "provenance","agent_id"|null,"created_at"(int)}]}
//!   POST /api/v1/memory/quarantine/{id}/release
//!        -> {"released":true,"id"} | 404 {"error":"not_found"}
//!
//! Clase operador (token maestro por cabecera): el listado expone texto que la
//! puerta de escritura juzgo hostil y la liberacion deshace una decision de
//! seguridad.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;
use tylluan_kernel::transport::http::{HttpState, build_router};

const MASTER: &str = "master-test-token";
const LIST: &str = "/api/v1/memory/quarantine";

/// Inserta un nodo directamente en la SilvaDB del estado, con control total de
/// `created_at`/`quarantined` — el gate real (L1/L2) no es el sujeto del test.
#[allow(clippy::too_many_arguments)]
async fn insert_node(
    state: &Arc<HttpState>,
    id: &str,
    content: &str,
    quarantined: bool,
    reason: Option<&str>,
    agent_id: Option<&str>,
    provenance: &str,
    created_at: &str,
) {
    let metadata = match agent_id {
        Some(a) => format!(r#"{{"agent_id":"{a}"}}"#),
        None => "{}".to_string(),
    };
    let conn = state.silva.conn_lock();
    let c = conn.lock().await;
    c.execute(
        "INSERT INTO nodes (id, type, content, metadata, provenance, created_at, quarantined, quarantine_reason)
         VALUES (?1, 'entity', ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            id,
            content,
            metadata,
            provenance,
            created_at,
            if quarantined { 1 } else { 0 },
            reason
        ],
    )
    .unwrap();
}

/// Lee `(quarantined, quarantine_reason)` del nodo. El guard se suelta antes de
/// volver: dejarlo vivo mientras corre una peticion posterior bloquearia el
/// mutex de la DB y el test se colgaria.
async fn quarantine_state(state: &Arc<HttpState>, id: &str) -> (i64, Option<String>) {
    let conn = state.silva.conn_lock();
    let c = conn.lock().await;
    c.query_row(
        "SELECT quarantined, quarantine_reason FROM nodes WHERE id = ?1",
        rusqlite::params![id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .unwrap()
}

async fn get_with(state: &Arc<HttpState>, uri: &str, auth: Option<&str>) -> (StatusCode, Value) {
    let app = build_router(state.clone());
    let mut req = Request::builder().method("GET").uri(uri);
    if let Some(token) = auth {
        req = req.header("Authorization", format!("Bearer {token}"));
    }
    let resp = app.oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn post_with(state: &Arc<HttpState>, uri: &str, auth: Option<&str>) -> (StatusCode, Value) {
    let app = build_router(state.clone());
    let mut req = Request::builder().method("POST").uri(uri);
    if let Some(token) = auth {
        req = req.header("Authorization", format!("Bearer {token}"));
    }
    let resp = app.oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn state_with_token(tag: &str) -> Arc<HttpState> {
    support::test_state_custom(tag, Some(MASTER.to_string()), false).await
}

#[tokio::test(flavor = "multi_thread")]
async fn list_returns_contract_shape_and_only_quarantined_nodes() {
    let state = state_with_token("quarantine_list").await;
    let long = format!("{}TAIL_MARKER", "x".repeat(260));
    insert_node(
        &state,
        "q1",
        &long,
        true,
        Some("ASI06 write-gate: LLM judge flagged as SUSPICIOUS"),
        Some("b4bench"),
        "agent_generated",
        "2026-10-20 10:00:00",
    )
    .await;
    insert_node(&state, "clean", "benign", false, None, None, "user_direct", "2026-10-20 11:00:00").await;

    let (status, body) = get_with(&state, LIST, Some(MASTER)).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["total"], 1, "only quarantined nodes are listed: {body}");
    let nodes = body["nodes"].as_array().expect("nodes array");
    assert_eq!(nodes.len(), 1);
    let n = &nodes[0];
    assert_eq!(n["id"], "q1");
    assert_eq!(n["quarantine_reason"], "ASI06 write-gate: LLM judge flagged as SUSPICIOUS");
    assert_eq!(n["provenance"], "agent_generated");
    assert_eq!(n["agent_id"], "b4bench");
    assert!(n["created_at"].is_i64(), "created_at must be an int, got {}", n["created_at"]);
    let preview = n["content_preview"].as_str().expect("content_preview string");
    assert_eq!(preview.chars().count(), 200, "preview must be at most 200 chars");
    assert!(!preview.contains("TAIL_MARKER"), "preview must not leak past 200 chars");
}

#[tokio::test(flavor = "multi_thread")]
async fn list_is_ordered_created_at_desc_and_paginates() {
    let state = state_with_token("quarantine_pagination").await;
    for (id, ts) in [
        ("oldest", "2026-10-01 00:00:00"),
        ("middle", "2026-10-10 00:00:00"),
        ("newest", "2026-10-20 00:00:00"),
    ] {
        insert_node(&state, id, id, true, Some("r"), None, "unverified", ts).await;
    }

    let (status, body) = get_with(&state, &format!("{LIST}?limit=2&offset=0"), Some(MASTER)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 3, "total is the whole quarantine, not the page");
    let first: Vec<&str> = body["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    assert_eq!(first, vec!["newest", "middle"], "newest first");

    let (_, body) = get_with(&state, &format!("{LIST}?limit=2&offset=2"), Some(MASTER)).await;
    let second: Vec<&str> = body["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    assert_eq!(second, vec!["oldest"]);

    // limit fuera de rango se recorta a 200, no es un error.
    let (status, body) = get_with(&state, &format!("{LIST}?limit=999"), Some(MASTER)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn release_existing_node_clears_flag_and_reason() {
    let state = state_with_token("quarantine_release_ok").await;
    insert_node(
        &state,
        "q1",
        "hostile",
        true,
        Some("ASI06 write-gate: LLM judge flagged as SUSPICIOUS"),
        None,
        "agent_generated",
        "2026-10-20 10:00:00",
    )
    .await;

    let (status, body) =
        post_with(&state, "/api/v1/memory/quarantine/q1/release", Some(MASTER)).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["released"], true);
    assert_eq!(body["id"], "q1");

    // El contrato dice: liberar deja quarantined=0 y borra quarantine_reason.
    let (q, reason) = quarantine_state(&state, "q1").await;
    assert_eq!(q, 0);
    assert_eq!(reason, None, "release must clear the reason, not keep it stale");

    // Y ya no aparece en el listado.
    let (_, body) = get_with(&state, LIST, Some(MASTER)).await;
    assert_eq!(body["total"], 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn release_missing_or_already_released_is_404() {
    let state = state_with_token("quarantine_release_404").await;
    insert_node(&state, "clean", "benign", false, None, None, "user_direct", "2026-10-20 10:00:00").await;

    let (status, body) =
        post_with(&state, "/api/v1/memory/quarantine/does-not-exist/release", Some(MASTER)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "not_found");

    // Un nodo que existe pero NUNCA estuvo en cuarentena: mismo 404 (el contrato
    // solo distingue "liberado" de "no habia nada que liberar").
    let (status, body) =
        post_with(&state, "/api/v1/memory/quarantine/clean/release", Some(MASTER)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "not_found");
}

#[tokio::test(flavor = "multi_thread")]
async fn oauth_jwt_with_mcp_scope_gets_insufficient_scope_on_both_routes() {
    let state = state_with_token("quarantine_oauth_403").await;
    insert_node(&state, "q1", "hostile", true, Some("r"), None, "agent_generated", "2026-10-20 10:00:00").await;
    let jwt = state.oauth.sign_jwt("test-jti-d12", 60);

    for (method, uri) in [
        ("GET", LIST.to_string()),
        ("POST", "/api/v1/memory/quarantine/q1/release".to_string()),
    ] {
        let app = build_router(state.clone());
        let resp = app
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(&uri)
                    .header("Authorization", format!("Bearer {jwt}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "{method} {uri} must refuse an mcp-scope JWT"
        );
        let challenge = resp
            .headers()
            .get(axum::http::header::WWW_AUTHENTICATE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(
            challenge.contains("insufficient_scope"),
            "{method} {uri} must answer with insufficient_scope, got: {challenge}"
        );
    }

    // Y el nodo sigue en cuarentena: el 403 no debe haber ejecutado nada.
    let (_, body) = get_with(&state, LIST, Some(MASTER)).await;
    assert_eq!(body["total"], 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn master_token_in_query_string_does_not_satisfy_the_operator_class() {
    // E1: las rutas de operador exigen el token por CABECERA. Un `?token=`
    // autentica la peticion pero no la clase — y debe seguir sin hacerlo aqui.
    let state = state_with_token("quarantine_query_token").await;
    let (status, _) = get_with(&state, &format!("{LIST}?token={MASTER}"), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
