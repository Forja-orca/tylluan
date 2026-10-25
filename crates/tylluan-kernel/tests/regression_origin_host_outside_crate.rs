//! M-BUFFY-10 — regresión de la validación `Origin`/`Host` en las rutas
//! MCP/SSE (`/messages`, `/sse`), vista DESDE FUERA del crate con el router
//! de producción real (`build_router` + `support::test_state_custom`), sin
//! lanzar kernel. Espejo de la tabla de 8 filas de T-DSH-85 (turno 1760) y
//! del enunciado de T-DSH-86 (turno 1762); cola: docs/internal/COLA_buffy_2026-10-25.md.
//!
//! Spec MCP 2025-06-18: el servidor "MUST validate the Origin header" de toda
//! petición entrante (anti DNS-rebinding). Hoy el kernel NO valida: cualquier
//! página web puede resolver un dominio propio a 127.0.0.1 y ejecutar
//! `tools/list`/`tools/call` contra el kernel sin token (T-DSH-85, hallazgo
//! reproducido con router en proceso).
//!
//! Los 5 casos que hoy devuelven 200 en vez de 403 llevan
//! salida en `docs/internal/IMPL_M-BUFFY-10_*.md`). Los 3 casos que ya
//! deben pasar (orígenes loopback y cliente sin Origin) corren siempre.
//! Quitaremos el `ignore` cuando T-DSH-86 aterrice.
//!
//! Cada caso se ejecuta en ambos modos: `dev_mode=true` (bypass de auth —
//! la validación de origen no depende de auth) y `dev_mode=false` con Bearer
//! maestro (el fix "no exime por tener auth válida").
//!
//! Run:
//!   cargo test -p tylluan-kernel --test regression_origin_host_outside_crate
//!   cargo test -p tylluan-kernel --test regression_origin_host_outside_crate -- --ignored   # rojo esperado hoy

mod support;

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;
use tylluan_kernel::transport::http::{build_router, HttpState};

const MASTER: &str = "mb10-master-token";

const EVIL_ORIGIN: &str = "http://evil.example";
const LOOP_ORIGIN: &str = "http://127.0.0.1:47004";
const LOOP_ORIGIN_LOCALHOST: &str = "http://localhost:47004";
/// Host de un cliente real en loopback (todo cliente HTTP/1.1 envía Host).
const LOOP_HOST: &str = "127.0.0.1:47004";
const EVIL_HOST: &str = "evil.example";

const TOOLS_LIST: &str = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
const WHOAMI_CALL: &str =
    r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"whoami","arguments":{}}}"#;

/// `dev_mode=true` (sin necesidad de token) y `dev_mode=false` + Bearer.
const MODES: [(&str, bool); 2] = [("dev=true", true), ("dev=false", false)];

#[derive(Clone, Copy)]
struct Probe {
    case: &'static str,
    method: &'static str,
    uri: &'static str,
    origin: Option<&'static str>,
    host: &'static str,
    content_type: Option<&'static str>,
    accept: Option<&'static str>,
    body: Option<&'static str>,
    extra: &'static [(&'static str, &'static str)],
}

impl Probe {
    /// Envia la peticion contra el router de produccion y devuelve
    /// (status, body). El body se lee con timeout: una respuesta SSE en
    /// stream (caso 3, HOY rojo) nunca termina y no debe colgar el test.
    async fn send(self, app: &axum::Router) -> (StatusCode, String) {
        let mut builder = Request::builder()
            .method(self.method)
            .uri(self.uri)
            .header("Host", self.host)
            .header(header::AUTHORIZATION, format!("Bearer {MASTER}"));
        if let Some(o) = self.origin {
            builder = builder.header("Origin", o);
        }
        if let Some(ct) = self.content_type {
            builder = builder.header(header::CONTENT_TYPE, ct);
        }
        if let Some(a) = self.accept {
            builder = builder.header(header::ACCEPT, a);
        }
        for (k, v) in self.extra {
            builder = builder.header(*k, *v);
        }
        let body = match self.body {
            Some(b) => Body::from(b.to_string()),
            None => Body::empty(),
        };
        let req = builder.body(body).expect("peticion de test valida");

        let resp = tokio::time::timeout(Duration::from_secs(15), app.clone().oneshot(req)).await;
        let resp = match resp {
            Ok(r) => r.unwrap(),
            Err(_) => {
                return (
                    StatusCode::from_u16(599).unwrap(),
                    "TIMEOUT 15s: el router no respondio".to_string(),
                );
            }
        };
        let status = resp.status();
        let body = match tokio::time::timeout(
            Duration::from_millis(1500),
            axum::body::to_bytes(resp.into_body(), 4 << 20),
        )
        .await
        {
            Ok(Ok(bytes)) => String::from_utf8_lossy(&bytes).to_string(),
            Ok(Err(e)) => format!("<error leyendo body: {e}>"),
            Err(_) => "<body en stream sin fin (SSE)>".to_string(),
        };
        (status, body)
    }
}

async fn app_for(dev_mode: bool, tag: &str) -> (Arc<HttpState>, axum::Router) {
    let state = support::test_state_custom(tag, Some(MASTER.into()), dev_mode).await;
    let app = build_router(state.clone());
    (state, app)
}

fn failures_report(test: &str, failures: &[String]) -> String {
    format!("{test}: {} fallo(s):\n  - {}", failures.len(), failures.join("\n  - "))
}

fn clip(body: &str) -> String {
    body.chars().take(220).collect::<String>()
}

/// Marcador distintivo del resultado de un tool: si una peticion con Origin
/// malicioso devuelve ESTE texto, el tool se ejecuto.
fn marker_of(body: &str) -> String {
    body.trim().chars().take(80).collect::<String>()
}

fn tools_probe(
    case: &'static str,
    origin: Option<&'static str>,
    host: &'static str,
    content_type: &'static str,
) -> Probe {
    Probe {
        case,
        method: "POST",
        uri: "/messages",
        origin,
        host,
        content_type: Some(content_type),
        accept: None,
        body: Some(TOOLS_LIST),
        extra: &[],
    }
}

// ─── Casos 403 (ROJO HOY: #[ignore] hasta T-DSH-86) ─────────────────────────

/// Caso 1 — POST /messages con `Origin: http://evil.example` => 403.
#[tokio::test(flavor = "multi_thread")]
async fn c1_post_messages_evil_origin_403() {
    let probe = tools_probe("c1", Some(EVIL_ORIGIN), LOOP_HOST, "application/json");
    let mut failures: Vec<String> = Vec::new();
    for (label, dev) in MODES {
        let (_, app) = app_for(dev, if dev { "mb10c1d" } else { "mb10c1p" }).await;
        let (status, body) = probe.send(&app).await;
        if status != StatusCode::FORBIDDEN {
            failures.push(format!(
                "[{label}] POST /messages Origin evil => {status} (esperado 403); body: {}",
                clip(&body)
            ));
        }
    }
    if !failures.is_empty() {
        panic!("{}", failures_report(probe.case, &failures));
    }
}

/// Caso 2 — `Origin` evil + `Content-Type: text/plain` (peticion CORS simple,
/// SIN preflight) => 403 **y el tool NO se ejecuta** (el body 403 no puede
/// contener el resultado del tool que devuelve el mismo `tools/call` sin
/// Origin, que sirve de control).
#[tokio::test(flavor = "multi_thread")]
async fn c2_post_messages_evil_origin_text_plain_403_tool_not_executed() {
    let control = Probe {
        case: "c2-control",
        method: "POST",
        uri: "/messages",
        origin: None,
        host: LOOP_HOST,
        content_type: Some("application/json"),
        accept: None,
        body: Some(WHOAMI_CALL),
        extra: &[],
    };
    let evil = Probe {
        case: "c2",
        method: "POST",
        uri: "/messages",
        origin: Some(EVIL_ORIGIN),
        host: LOOP_HOST,
        content_type: Some("text/plain"),
        accept: None,
        body: Some(WHOAMI_CALL),
        extra: &[],
    };
    let mut failures: Vec<String> = Vec::new();
    for (label, dev) in MODES {
        let (_, app) = app_for(dev, if dev { "mb10c2d" } else { "mb10c2p" }).await;

        // Control: la misma llamada sin Origin debe seguir funcionando.
        let (c_status, c_body) = control.send(&app).await;
        if c_status != StatusCode::OK {
            failures.push(format!(
                "[{label}] control (sin Origin) => {c_status} (esperado 200); body: {}",
                clip(&c_body)
            ));
            continue;
        }
        let marker = marker_of(&c_body);

        let (status, body) = evil.send(&app).await;
        if status != StatusCode::FORBIDDEN {
            failures.push(format!(
                "[{label}] text/plain + Origin evil => {status} (esperado 403, el preflight \
                 no protege esta peticion simple); body: {}",
                clip(&body)
            ));
        } else if body.contains(&marker) {
            failures.push(format!(
                "[{label}] devolvio 403 pero el body contiene el resultado del tool \
                 ({marker:?}): el tool se ejecuto igualmente"
            ));
        }
    }
    if !failures.is_empty() {
        panic!("{}", failures_report(evil.case, &failures));
    }
}

/// Caso 3 — GET /sse con `Origin` evil => 403 (hoy abre el stream 200).
#[tokio::test(flavor = "multi_thread")]
async fn c3_get_sse_evil_origin_403() {
    let probe = Probe {
        case: "c3",
        method: "GET",
        uri: "/sse",
        origin: Some(EVIL_ORIGIN),
        host: LOOP_HOST,
        content_type: None,
        accept: Some("text/event-stream"),
        body: None,
        extra: &[],
    };
    let mut failures: Vec<String> = Vec::new();
    for (label, dev) in MODES {
        let (_, app) = app_for(dev, if dev { "mb10c3d" } else { "mb10c3p" }).await;
        let (status, body) = probe.send(&app).await;
        if status != StatusCode::FORBIDDEN {
            failures.push(format!(
                "[{label}] GET /sse Origin evil => {status} (esperado 403); body: {}",
                clip(&body)
            ));
        }
    }
    if !failures.is_empty() {
        panic!("{}", failures_report(probe.case, &failures));
    }
}

/// Caso 4 — `Host: evil.example` SIN Origin => 403 (defensa complementaria
/// cuando el kernel esta enlazado a loopback; hoy el Host se ignora).
#[tokio::test(flavor = "multi_thread")]
async fn c4_host_evil_without_origin_403() {
    let probe = tools_probe("c4", None, EVIL_HOST, "application/json");
    let mut failures: Vec<String> = Vec::new();
    for (label, dev) in MODES {
        let (_, app) = app_for(dev, if dev { "mb10c4d" } else { "mb10c4p" }).await;
        let (status, body) = probe.send(&app).await;
        if status != StatusCode::FORBIDDEN {
            failures.push(format!(
                "[{label}] Host evil sin Origin => {status} (esperado 403); body: {}",
                clip(&body)
            ));
        }
    }
    if !failures.is_empty() {
        panic!("{}", failures_report(probe.case, &failures));
    }
}

/// Caso 5 — preflight OPTIONS con `Origin` evil => 403 (hoy 200 sin ACAO).
#[tokio::test(flavor = "multi_thread")]
async fn c5_options_preflight_evil_origin_403() {
    let probe = Probe {
        case: "c5",
        method: "OPTIONS",
        uri: "/messages",
        origin: Some(EVIL_ORIGIN),
        host: LOOP_HOST,
        content_type: None,
        accept: None,
        body: None,
        extra: &[
            ("Access-Control-Request-Method", "POST"),
            ("Access-Control-Request-Headers", "content-type"),
        ],
    };
    let mut failures: Vec<String> = Vec::new();
    for (label, dev) in MODES {
        let (_, app) = app_for(dev, if dev { "mb10c5d" } else { "mb10c5p" }).await;
        let (status, body) = probe.send(&app).await;
        if status != StatusCode::FORBIDDEN {
            failures.push(format!(
                "[{label}] OPTIONS preflight Origin evil => {status} (esperado 403); body: {}",
                clip(&body)
            ));
        }
    }
    if !failures.is_empty() {
        panic!("{}", failures_report(probe.case, &failures));
    }
}

// ─── Casos 200 (deben pasar HOY y tras el fix — sin ignore) ─────────────────

/// Caso 6 — `Origin: http://127.0.0.1:<puerto>` (loopback) => 200.
#[tokio::test(flavor = "multi_thread")]
async fn c6_post_messages_loopback_origin_127_200() {
    let probe = tools_probe("c6", Some(LOOP_ORIGIN), LOOP_HOST, "application/json");
    let mut failures: Vec<String> = Vec::new();
    for (label, dev) in MODES {
        let (_, app) = app_for(dev, if dev { "mb10c6d" } else { "mb10c6p" }).await;
        let (status, body) = probe.send(&app).await;
        if status != StatusCode::OK {
            failures.push(format!(
                "[{label}] Origin loopback 127.0.0.1 => {status} (esperado 200); body: {}",
                clip(&body)
            ));
        } else if !body.contains("\"tools\"") {
            failures.push(format!(
                "[{label}] 200 pero sin catalogo de tools en el body: {}",
                clip(&body)
            ));
        }
    }
    if !failures.is_empty() {
        panic!("{}", failures_report(probe.case, &failures));
    }
}

/// Caso 7 — `Origin: http://localhost:<puerto>` (loopback) => 200.
#[tokio::test(flavor = "multi_thread")]
async fn c7_post_messages_loopback_origin_localhost_200() {
    let probe = tools_probe("c7", Some(LOOP_ORIGIN_LOCALHOST), LOOP_HOST, "application/json");
    let mut failures: Vec<String> = Vec::new();
    for (label, dev) in MODES {
        let (_, app) = app_for(dev, if dev { "mb10c7d" } else { "mb10c7p" }).await;
        let (status, body) = probe.send(&app).await;
        if status != StatusCode::OK {
            failures.push(format!(
                "[{label}] Origin loopback localhost => {status} (esperado 200); body: {}",
                clip(&body)
            ));
        } else if !body.contains("\"tools\"") {
            failures.push(format!(
                "[{label}] 200 pero sin catalogo de tools en el body: {}",
                clip(&body)
            ));
        }
    }
    if !failures.is_empty() {
        panic!("{}", failures_report(probe.case, &failures));
    }
}

/// Caso 8 — SIN Origin (cliente no navegador: Claude Code, mcp-remote,
/// agentes) => 200; el fix no puede romper esta via.
#[tokio::test(flavor = "multi_thread")]
async fn c8_post_messages_without_origin_200() {
    let probe = tools_probe("c8", None, LOOP_HOST, "application/json");
    let mut failures: Vec<String> = Vec::new();
    for (label, dev) in MODES {
        let (_, app) = app_for(dev, if dev { "mb10c8d" } else { "mb10c8p" }).await;
        let (status, body) = probe.send(&app).await;
        if status != StatusCode::OK {
            failures.push(format!(
                "[{label}] sin Origin => {status} (esperado 200); body: {}",
                clip(&body)
            ));
        } else if !body.contains("\"tools\"") {
            failures.push(format!(
                "[{label}] 200 pero sin catalogo de tools en el body: {}",
                clip(&body)
            ));
        }
    }
    if !failures.is_empty() {
        panic!("{}", failures_report(probe.case, &failures));
    }
}
