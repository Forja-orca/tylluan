//! Coloquio — identidad del autor no suplantable (D180b, ADR-031).
//!
//! Estos tests ejercitan el **handler real** (`POST /api/v1/coloquio/channels/{id}/post`),
//! no solo la función pura `effective_author`: el hueco que señaló el TL en T4146
//! era que nada comprobaba que la identidad resuelta fuese la que **se guarda**.
//!
//! Casos:
//! 1. Clase operador → puede declarar `author_id`.
//! 2. Guild `coloquio` (proxy auditado) → puede publicar en nombre de otro agente.
//! 3. Guild normal (p. ej. `scheduler`) intentando firmar como otro → se fuerza a
//!    su propia identidad (`guild:<nombre>`).
//! 4. Sin contexto de autenticación → **403** (fail-closed).
//!
//! La identidad guardada se lee del propio kernel (`GET .../turn/{n}`), así que
//! el test falla si el handler deja de usar la identidad resuelta.

mod support;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;
use tylluan_kernel::transport::http::auth::{AclAuthKind, ACL_AGENT_ID, ACL_AUTH_KIND};

const CANAL: &str = "general";

async fn post(
    app: &axum::Router,
    requested: &str,
) -> (StatusCode, Option<i64>) {
    let body = serde_json::json!({
        "author_id": requested,
        "role": "agent",
        "content": "nota de prueba de identidad"
    });
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/coloquio/channels/{CANAL}/post"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap_or_default();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json.get("turn").and_then(|t| t.as_i64()))
}

/// Identidad con la que el kernel **guardó** el turno `n`.
async fn stored_author(app: &axum::Router, turn: i64) -> Option<String> {
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/coloquio/channels/{CANAL}/turn/{turn}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap_or_default();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    json.get("message")
        .and_then(|m| m.get("author_id"))
        .and_then(|a| a.as_str())
        .map(|s| s.to_string())
}

/// Escenifica la petición con el contexto de autenticación indicado.
async fn post_as(
    tag: &str,
    kind: Option<AclAuthKind>,
    bound: Option<&str>,
    requested: &str,
) -> (StatusCode, Option<String>) {
    let state = support::test_state(tag).await;
    let app = support::build_test_app(state);
    match kind {
        None => {
            let (status, turn) = post(&app, requested).await;
            (status, turn.and_then(|_| None))
        }
        Some(kind) => {
            let bound_owned = bound.map(|b| b.to_string()).unwrap_or_default();
            ACL_AGENT_ID
                .scope(bound_owned, async {
                    ACL_AUTH_KIND
                        .scope(kind, async {
                            let (status, turn) = post(&app, requested).await;
                            let author = match turn {
                                Some(n) => stored_author(&app, n).await,
                                None => None,
                            };
                            (status, author)
                        })
                        .await
                })
                .await
        }
    }
}

/// 1. Operador: puede firmar con el nombre pedido.
#[tokio::test(flavor = "multi_thread")]
async fn operador_puede_declarar_el_autor_y_se_guarda_tal_cual() {
    let (status, author) = post_as("colident-op", Some(AclAuthKind::MasterToken), None, "deep").await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(author.as_deref(), Some("deep"), "el operador firma con su nombre");
}

/// 2. La guild `coloquio` es el proxy auditado: publica en nombre de otro.
#[tokio::test(flavor = "multi_thread")]
async fn la_guild_proxy_coloquio_publica_en_nombre_de_otro() {
    let (status, author) = post_as(
        "colident-proxy",
        Some(AclAuthKind::GuildToken),
        Some("guild:coloquio"),
        "deep",
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        author.as_deref(),
        Some("deep"),
        "el proxy del Coloquio no debe perder el nombre del agente"
    );
}

/// 3. Una guild normal no puede firmar como otro: se guarda su identidad.
#[tokio::test(flavor = "multi_thread")]
async fn una_guild_normal_no_puede_firmar_como_otro_agente() {
    let (status, author) = post_as(
        "colident-guild",
        Some(AclAuthKind::GuildToken),
        Some("guild:scheduler"),
        "deep",
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        author.as_deref(),
        Some("guild:scheduler"),
        "el intento de suplantación se guarda con la identidad real"
    );
}

/// 4. Sin contexto de auth: fail-closed, no se escribe nada.
#[tokio::test(flavor = "multi_thread")]
async fn sin_contexto_de_autenticacion_devuelve_403() {
    let (status, _) = post_as("colident-noauth", None, None, "deep").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
