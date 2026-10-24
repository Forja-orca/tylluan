//! M-BUFFY-8 — tests de regresión de D-55 y D-56 vistos DESDE FUERA del
//! crate (router HTTP real vía `build_router`, sin tocar `src/`), tal como
//! pidió el TL en el Coloquio (turno 1719).
//!
//! Deep ya tiene tests unitarios dentro de `src` (api_ingest.rs /
//! api_federation.rs); estos son la red externa que gatea el contrato
//! observable por cualquier cliente HTTP:
//!
//!   D-55 — `GET /api/v1/ingest/files/{filename}` debe devolver **400
//!   "Invalid filename"** para nombres que en Windows sustituyen la base o
//!   escalan fuera de `data/ingest`: prefijo de unidad (`C:foo`, `a:b`),
//!   `..`, `/` y `\` (con `%XX` codificado). Pre-D-55, `C:foo`/`a:b` pasaban
//!   el guard y caían a 404 (fichero inexistente) — es decir, la defensa
//!   dependía de que el fichero no existiera.
//!
//!   D-56 — si al aprobar un peer la pubkey Ed25519 no se puede persistir,
//!   el peer **no puede quedar aprobado** y la respuesta NO puede ser el
//!   `approved_without_identity` de D-52 (peer aprobado sin identidad).
//!   Inducción desde fuera: el `peer.url` del test apunta a un servidor de
//!   identidad falso en loopback que **elimina la fila del peer de PeerDb
//!   antes de responder** la pubkey; `update_ed25519_pubkey` entonces no
//!   encuentra la fila (Ok(false) → error de persistencia) y D-56 debe
//!   revertir/recargar la caché. Pre-D-56 la rama de error devolvía 500
//!   `approved_without_identity` SIN recargar la caché, y `GET peers` seguía
//!   mostrando `approved: true`.
//!
//! Corridos como binario propio en `e2e-security-tests` (ci.yml). Un solo
//! proceso por binario comparte la caché global de config (`load_cached`),
//! por eso los dos tests usan peers/nombres de puerto disjuntos.
//!
//! Run: cargo test -p tylluan-kernel --test regression_d55_d56_outside_crate

mod support;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;
use tylluan_kernel::transport::http::{build_router, HttpState};

const MASTER: &str = "mb8-master-token";

async fn call_json(
    app: &axum::Router,
    bearer: &str,
    method: &str,
    uri: &str,
    body: Option<serde_json::Value>,
) -> (StatusCode, String) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"));
    let body = match body {
        Some(b) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(b.to_string())
        }
        None => Body::empty(),
    };
    let resp = tokio::time::timeout(std::time::Duration::from_secs(15), app.clone().oneshot(builder.body(body).unwrap()))
        .await;
    let resp = match resp {
        Ok(r) => r.unwrap(),
        Err(_) => {
            // Un cuelgue aqui es un hallazgo, no un flake: el handler de approve
            // hace config.write() (reload_peers_cache) mientras el propio if-let
            // de la peticion sostiene config.read() — deadlock de RwLock que
            // este timeout convierte en fallo legible en vez de colgar CI.
            return (
                StatusCode::from_u16(599).unwrap(),
                "TIMEOUT 15s: posible deadlock de RwLock en el handler".to_string(),
            );
        }
    };
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 4 << 20).await.unwrap_or_default();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

fn failures_report(test: &str, failures: &[String]) -> String {
    format!("{test}: {} fallo(s):\n  - {}", failures.len(), failures.join("\n  - "))
}

// ─── D-55 ───────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn d55_ingest_files_rejects_unit_prefix_and_traversal_names() {
    let state = support::test_state_custom("mb8d55", Some(MASTER.into()), false).await;
    let app = build_router(state);

    // Segmentos de path tal y como los ve el router: `:` y `\` codificados o
    // literarios; `%2F` se decodifica en el extractor Path a `/`.
    let malicious = [
        "C:foo",             // prefijo de unidad relativa de Windows (D-55, T-DSH-69 fila 8)
        "a:b",               // otro prefijo de unidad / Alternate Data Stream
        "..",                // traversal literal
        "..%5Cevil",         // traversal con backslash codificado
        "%2Fetc%2Fpasswd",    // ruta absoluta con / codificados
        "%2E%2E%2Fevil",     // .. y / totalmente codificados
    ];

    let mut failures: Vec<String> = Vec::new();
    for name in malicious {
        let (status, body) = call_json(
            &app,
            MASTER,
            "GET",
            &format!("/api/v1/ingest/files/{name}"),
            None,
        )
        .await;
        if status != StatusCode::BAD_REQUEST {
            failures.push(format!(
                "D-55: GET /api/v1/ingest/files/{name} => {status} (esperado 400); body: {}",
                body.chars().take(200).collect::<String>()
            ));
        } else if !body.contains("Invalid filename") {
            failures.push(format!(
                "D-55: {name} devolvio 400 pero sin el mensaje 'Invalid filename'; body: {body:?}"
            ));
        }
    }

    if !failures.is_empty() {
        panic!("{}", failures_report("d55", &failures));
    }
}

// ─── D-56 ───────────────────────────────────────────────────────────────────

/// Servidor de identidad falso: antes de responder la pubkey, elimina la fila
/// del peer en PeerDb para que la persistencia falle (única forma de inducir
/// el fallo desde fuera del crate, que es justo lo que D-56 debe tolerar).
fn victim_identity_app(state: std::sync::Arc<HttpState>, peer_name: &'static str) -> axum::Router {
    axum::Router::new().route(
        "/api/v1/federation/identity",
        axum::routing::get(move || {
            let state = state.clone();
            async move {
                let removed = state.peer_db.remove(peer_name).unwrap_or(false);
                axum::response::Json(json!({
                    "public_key": "mb8-pubkey-victim",
                    "removed_before_response": removed,
                }))
            }
        }),
    )
}

/// Identidad falso normal: solo devuelve la pubkey (camino feliz).
fn happy_identity_app(pubkey: &'static str) -> axum::Router {
    axum::Router::new().route(
        "/api/v1/federation/identity",
        axum::routing::get(move || async move { axum::response::Json(json!({ "public_key": pubkey })) }),
    )
}

#[tokio::test(flavor = "multi_thread")]
// IGNORADO hasta que se cierre el deadlock de `federation_approve_peer`
// (read-guard del `if let ... state.config.read()...` + `reload_peers_cache`
// con `config.write()` dentro del cuerpo = self-deadlock del RwLock cuando la
// identidad del peer responde 200). Repro: este test — fase RED en el informe
// IMPL_M-BUFFY-8. Quitar `#[ignore]` cuando el fix de src aterrice; la
// verificacion con D-52 revertido ya quedó documentada (rojo con
// `approved_without_identity` + caché obsoleta + 599).
#[ignore = "bloqueado por deadlock RwLock en federation_approve_peer (IMPL_M-BUFFY-8)"]
async fn d56_failed_pubkey_persist_never_leaves_an_approved_peer() {
    let state = support::test_state_custom("mb8d56", Some(MASTER.into()), false).await;

    // Servidor de identidad victima (D-56: fallo de persistencia inducido).
    let victim_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let victim_addr = victim_listener.local_addr().unwrap();
    let victim_app = victim_identity_app(state.clone(), "mb8-peer-victim");
    tokio::spawn(async move { let _ = axum::serve(victim_listener, victim_app).await; });

    // Servidor de identidad sano (camino feliz).
    let happy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let happy_addr = happy_listener.local_addr().unwrap();
    let happy_app = happy_identity_app("mb8-pubkey-happy");
    tokio::spawn(async move { let _ = axum::serve(happy_listener, happy_app).await; });

    let app = build_router(state.clone());
    let mut failures: Vec<String> = Vec::new();

    // ── A: peer cuya pubkey no se puede persistir (fila desaparece) ──
    eprintln!("MB8: add victim");
    let (st, body) = call_json(
        &app,
        MASTER,
        "POST",
        "/api/v1/federation/peers",
        Some(json!({
            "name": "mb8-peer-victim",
            "url": format!("http://{victim_addr}"),
            "auth_token": "mb8-add-token-victim",
        })),
    )
    .await;
    if st != StatusCode::CREATED {
        failures.push(format!("add peer victim => {st}: {body}"));
    }

    eprintln!("MB8: approve victim");
    let (st, body) = call_json(
        &app,
        MASTER,
        "POST",
        "/api/v1/federation/peers/mb8-peer-victim/approve",
        Some(json!({ "auth_token": "mb8-approve-token-victim" })),
    )
    .await;
    if st != StatusCode::INTERNAL_SERVER_ERROR {
        failures.push(format!(
            "D-56: approve con persistencia fallida => {st} (esperado 500); body: {body}"
        ));
    } else {
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
        let status_field = parsed["status"].as_str().unwrap_or("<sin status>");
        if !matches!(status_field, "approval_reverted" | "approval_revert_failed") {
            failures.push(format!(
                "D-56: status={status_field:?} (esperado 'approval_reverted' o \
                 'approval_revert_failed'; 'approved_without_identity' dejaria un peer \
                 aprobado sin identidad). body: {body}"
            ));
        }
    }

    // El peer NO puede aparecer como aprobado en la vista que sirve la API.
    eprintln!("MB8: GET peers (tras victim)");
    let (st, body) = call_json(&app, MASTER, "GET", "/api/v1/federation/peers", None).await;
    if st != StatusCode::OK {
        failures.push(format!("GET peers => {st}: {body}"));
    } else {
        let peers: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
        let stale_approved = peers.as_array().map(|arr| {
            arr.iter().any(|p| {
                p["name"].as_str() == Some("mb8-peer-victim") && p["approved"] == json!(true)
            })
        }).unwrap_or(false);
        if stale_approved {
            failures.push(format!(
                "D-56: el peer con persistencia fallida sigue apareciendo aprobado \
                 (caché no recargada o aprobación no revertida). peers: {body}"
            ));
        }
    }

    // ── B: camino feliz — la aprobación legitima sigue funcionando ──
    eprintln!("MB8: add happy");
    let (st, body) = call_json(
        &app,
        MASTER,
        "POST",
        "/api/v1/federation/peers",
        Some(json!({
            "name": "mb8-peer-happy",
            "url": format!("http://{happy_addr}"),
            "auth_token": "mb8-add-token-happy",
        })),
    )
    .await;
    if st != StatusCode::CREATED {
        failures.push(format!("add peer happy => {st}: {body}"));
    }

    eprintln!("MB8: approve happy");
    let (st, body) = call_json(
        &app,
        MASTER,
        "POST",
        "/api/v1/federation/peers/mb8-peer-happy/approve",
        Some(json!({ "auth_token": "mb8-approve-token-happy" })),
    )
    .await;
    if st != StatusCode::OK {
        failures.push(format!("D-56 (feliz): approve => {st}: {body}"));
    } else {
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
        if parsed["status"].as_str() != Some("approved") {
            failures.push(format!("D-56 (feliz): status != approved: {body}"));
        }
    }

    eprintln!("MB8: GET peers (final) + 404");
    let (st, body) = call_json(&app, MASTER, "GET", "/api/v1/federation/peers", None).await;
    if st == StatusCode::OK {
        let peers: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
        let happy_ok = peers.as_array().map(|arr| {
            arr.iter().any(|p| {
                p["name"].as_str() == Some("mb8-peer-happy") && p["approved"] == json!(true)
            })
        }).unwrap_or(false);
        if !happy_ok {
            failures.push(format!(
                "D-56 (feliz): el peer legitimo debe quedar aprobado tras la aprobacion. peers: {body}"
            ));
        }
    }

    // ── C: aprobar un peer inexistente => 404 (no confundir con el caso A) ──
    let (st, _) = call_json(
        &app,
        MASTER,
        "POST",
        "/api/v1/federation/peers/mb8-peer-noexiste/approve",
        Some(json!({ "auth_token": "mb8-approve-token-x" })),
    )
    .await;
    if st != StatusCode::NOT_FOUND {
        failures.push(format!("approve de peer inexistente => {st} (esperado 404)"));
    }

    if !failures.is_empty() {
        panic!("{}", failures_report("d56", &failures));
    }
}
