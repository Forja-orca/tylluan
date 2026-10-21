//! T-DSH-14 — Auditoría adversarial de D-15 (consentimiento OAuth, Fase 2).
//!
//! Base: `crates/tylluan-kernel/tests/oauth_f2_consent.rs` (harness de P1 y del
//! router real). Esta matriz ataca los 7 vectores pedidos por el TL:
//!
//!   V1. emitir un code sin credencial
//!   V2. reusar `consent_id`/CSRF tras aprobar o denegar
//!   V3. CSRF cruzado entre peticiones
//!   V4. `client_id`/`redirect_uri` manipulados para heredar un consentimiento
//!   V5. `scope=operator` por variantes (mayúsculas, espacios, duplicados)
//!   V6. XSS en `client_id`/`state` reflejados en la página
//!   V7. bypass del rate limit por `X-Forwarded-For`
//!
//! Convención: cada vector es un `#[test]` independiente. Los que fallan HOY
//! llevan `#[ignore = "..."]` con el motivo; los que pasan (la mayoría) corren
//! en la suite normal como pruebas de especificación.
//!
//! Run:
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test oauth_f2_consent_adversarial

mod support;

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use tower::ServiceExt;
use tylluan_kernel::transport::http::build_router;

const MASTER: &str = "master-test-token-d15-adversarial";
const VERIFIER: &str = "d15-adv-verifier-0123456789abcdefghijklmnop";
const REDIRECT: &str = "http://127.0.0.1:9999/cb";
const CLIENT: &str = "claude-desktop-test";

fn s256(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn authorize_uri(client_id: &str, redirect: &str, extra: &str) -> String {
    format!(
        "/oauth/authorize?client_id={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256{}",
        urlencoding::encode(client_id),
        urlencoding::encode(redirect),
        s256(VERIFIER),
        extra
    )
}

struct Reply {
    status: StatusCode,
    location: Option<String>,
    headers: HeaderMap,
    body: String,
}

async fn get(app: &axum::Router, uri: &str, forwarded_for: Option<&str>) -> Reply {
    let mut builder = Request::builder().method("GET").uri(uri);
    if let Some(xff) = forwarded_for {
        builder = builder.header("x-forwarded-for", xff);
    }
    let resp = app
        .clone()
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let location = headers
        .get(header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .map(|b| String::from_utf8_lossy(&b).to_string())
        .unwrap_or_default();
    Reply { status, location, headers, body }
}

async fn post_form(
    app: &axum::Router,
    cookie: Option<&str>,
    fields: &[(&str, &str)],
    connect_ip: Option<SocketAddr>,
    forwarded_for: Option<&str>,
) -> Reply {
    let body = fields
        .iter()
        .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    let mut builder = Request::builder()
        .method("POST")
        .uri("/oauth/authorize")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(c) = cookie {
        builder = builder.header(header::COOKIE, c);
    }
    if let Some(xff) = forwarded_for {
        builder = builder.header("x-forwarded-for", xff);
    }
    let mut req = builder.body(Body::from(body)).unwrap();
    if let Some(addr) = connect_ip {
        req.extensions_mut().insert(axum::extract::ConnectInfo(addr));
    }
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let location = headers
        .get(header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .map(|b| String::from_utf8_lossy(&b).to_string())
        .unwrap_or_default();
    Reply { status, location, headers, body }
}

fn hidden(html: &str, name: &str) -> String {
    let needle = format!("name=\"{name}\" value=\"");
    html.split(&needle)
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap_or("")
        .to_string()
}

fn cookie(reply: &Reply) -> String {
    reply
        .headers
        .get(header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .unwrap_or("")
        .to_string()
}

fn code_from(location: &str) -> String {
    location
        .split("code=")
        .nth(1)
        .unwrap_or("")
        .split('&')
        .next()
        .unwrap_or("")
        .to_string()
}

fn has_code(reply: &Reply) -> bool {
    reply
        .location
        .as_deref()
        .map(|l| !code_from(l).is_empty())
        .unwrap_or(false)
}

async fn app_for(tag: &str) -> axum::Router {
    build_router(support::test_state_custom(tag, Some(MASTER.to_string()), false).await)
}

/// Abre P1 y aprueba con la credencial correcta; devuelve (Location, cookie).
async fn approve_fresh(app: &axum::Router, client: &str, redirect: &str, extra: &str) -> Reply {
    let page = get(app, &authorize_uri(client, redirect, extra), None).await;
    assert_eq!(page.status, StatusCode::OK, "P1 debe servirse");
    post_form(
        app,
        Some(&cookie(&page)),
        &[
            ("consent_id", &hidden(&page.body, "consent_id")),
            ("csrf", &hidden(&page.body, "csrf")),
            ("decision", "approve"),
            ("credential", MASTER),
        ],
        None,
        None,
    )
    .await
}

// ───────────────────────── V1: code sin credencial ──────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn v1_no_code_without_credential() {
    let app = app_for("t14_v1").await;
    let uri = authorize_uri(CLIENT, REDIRECT, "");

    // Sin consentimiento previo: P1, sin code y sin redirect.
    let page = get(&app, &uri, None).await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(!has_code(&page));
    assert!(page.location.is_none());

    // POST con credencial vacía → 403 sin code.
    let empty = post_form(
        &app,
        Some(&cookie(&page)),
        &[
            ("consent_id", &hidden(&page.body, "consent_id")),
            ("csrf", &hidden(&page.body, "csrf")),
            ("decision", "approve"),
            ("credential", ""),
        ],
        None,
        None,
    )
    .await;
    assert_eq!(empty.status, StatusCode::FORBIDDEN);
    assert!(!has_code(&empty));

    // POST con credencial incorrecta → 403 sin code.
    let wrong = post_form(
        &app,
        Some(&cookie(&page)),
        &[
            ("consent_id", &hidden(&page.body, "consent_id")),
            ("csrf", &hidden(&page.body, "csrf")),
            ("decision", "approve"),
            ("credential", "no-es-la-credencial"),
        ],
        None,
        None,
    )
    .await;
    assert_eq!(wrong.status, StatusCode::FORBIDDEN);
    assert!(!has_code(&wrong));

    // Credencial por query string: no la lee nadie (solo el form POST).
    let sneaky = get(
        &app,
        &format!("{uri}&credential={}&decision=approve", urlencoding::encode(MASTER)),
        None,
    )
    .await;
    assert_eq!(sneaky.status, StatusCode::OK);
    assert!(!has_code(&sneaky), "la credencial en query no emite nada");

    // Con la credencial correcta sí (control positivo), y solo entonces el
    // consentimiento recordado emite sin acciones humanas (por diseño, 30 días).
    let ok = approve_fresh(&app, CLIENT, REDIRECT, "").await;
    assert_eq!(ok.status, StatusCode::TEMPORARY_REDIRECT);
    assert!(has_code(&ok));
    let remembered = get(&app, &uri, None).await;
    assert_eq!(
        remembered.status,
        StatusCode::TEMPORARY_REDIRECT,
        "recordado tras aprobación explícita (por diseño)"
    );
    assert!(has_code(&remembered));
}

// ─────────────────── V2: reuso de consent_id/CSRF tras decidir ──────────────

#[tokio::test(flavor = "multi_thread")]
async fn v2_consent_id_and_csrf_cannot_be_replayed() {
    let app = app_for("t14_v2").await;
    let uri = authorize_uri(CLIENT, REDIRECT, "");

    // Aprobar: el pending se consume.
    let page = get(&app, &uri, None).await;
    let session = cookie(&page);
    let consent_id = hidden(&page.body, "consent_id");
    let csrf = hidden(&page.body, "csrf");
    let approved = post_form(
        &app,
        Some(&session),
        &[
            ("consent_id", &consent_id),
            ("csrf", &csrf),
            ("decision", "approve"),
            ("credential", MASTER),
        ],
        None,
        None,
    )
    .await;
    assert_eq!(approved.status, StatusCode::TEMPORARY_REDIRECT);
    assert!(has_code(&approved));

    // Replay exacto del POST con los mismos ids y credencial → 403 sin code.
    let replay = post_form(
        &app,
        Some(&session),
        &[
            ("consent_id", &consent_id),
            ("csrf", &csrf),
            ("decision", "approve"),
            ("credential", MASTER),
        ],
        None,
        None,
    )
    .await;
    assert_eq!(
        replay.status,
        StatusCode::FORBIDDEN,
        "un consent_id consumido no puede emitir un segundo code"
    );
    assert!(!has_code(&replay));

    // Denegar consume el pending: replay de approve con los mismos ids → 403.
    let uri2 = authorize_uri("otro-cliente", "http://127.0.0.1:7001/cb", "");
    let page2 = get(&app, &uri2, None).await;
    let session2 = cookie(&page2);
    let consent_id2 = hidden(&page2.body, "consent_id");
    let csrf2 = hidden(&page2.body, "csrf");
    let denied = post_form(
        &app,
        Some(&session2),
        &[
            ("consent_id", &consent_id2),
            ("csrf", &csrf2),
            ("decision", "deny"),
            ("credential", ""),
        ],
        None,
        None,
    )
    .await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN);
    assert!(!has_code(&denied));

    let after_deny = post_form(
        &app,
        Some(&session2),
        &[
            ("consent_id", &consent_id2),
            ("csrf", &csrf2),
            ("decision", "approve"),
            ("credential", MASTER),
        ],
        None,
        None,
    )
    .await;
    assert_eq!(
        after_deny.status,
        StatusCode::FORBIDDEN,
        "un consent_id denegado no puede aprobarse después"
    );
    assert!(!has_code(&after_deny));
}

// ───────────────────── V3: CSRF cruzado entre peticiones ────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn v3_csrf_is_bound_to_its_request_and_session() {
    let app = app_for("t14_v3").await;

    // Dos peticiones vivas en el mismo kernel.
    let a_uri = authorize_uri("cliente-a", "http://127.0.0.1:6001/cb", "");
    let b_uri = authorize_uri("cliente-b", "http://127.0.0.1:6002/cb", "");
    let page_a = get(&app, &a_uri, None).await;
    let page_b = get(&app, &b_uri, None).await;
    let (cid_a, csrf_a, cookie_a) = (
        hidden(&page_a.body, "consent_id"),
        hidden(&page_a.body, "csrf"),
        cookie(&page_a),
    );
    let (cid_b, csrf_b, cookie_b) = (
        hidden(&page_b.body, "consent_id"),
        hidden(&page_b.body, "csrf"),
        cookie(&page_b),
    );
    assert_ne!(cid_a, cid_b);
    assert_ne!(csrf_a, csrf_b);

    // CSRF de B sobre consent_id de A, con la cookie de A → 403.
    let cross_csrf = post_form(
        &app,
        Some(&cookie_a),
        &[
            ("consent_id", &cid_a),
            ("csrf", &csrf_b),
            ("decision", "approve"),
            ("credential", MASTER),
        ],
        None,
        None,
    )
    .await;
    assert_eq!(cross_csrf.status, StatusCode::FORBIDDEN, "CSRF cruzado");
    assert!(!has_code(&cross_csrf));

    // consent_id y CSRF de A con la cookie de B → 403.
    let cross_cookie = post_form(
        &app,
        Some(&cookie_b),
        &[
            ("consent_id", &cid_a),
            ("csrf", &csrf_a),
            ("decision", "approve"),
            ("credential", MASTER),
        ],
        None,
        None,
    )
    .await;
    assert_eq!(cross_cookie.status, StatusCode::FORBIDDEN, "cookie de otra sesión");
    assert!(!has_code(&cross_cookie));

    // La petición A sigue viva y válida con su propia cookie+CSRF.
    let legit = post_form(
        &app,
        Some(&cookie_a),
        &[
            ("consent_id", &cid_a),
            ("csrf", &csrf_a),
            ("decision", "approve"),
            ("credential", MASTER),
        ],
        None,
        None,
    )
    .await;
    assert_eq!(
        legit.status,
        StatusCode::TEMPORARY_REDIRECT,
        "los intentos cruzados no inutilizan la petición legítima"
    );
    assert!(has_code(&legit));
}

// ──────── V4: client_id/redirect_uri manipulados no heredan consentimiento ──

#[tokio::test(flavor = "multi_thread")]
async fn v4_consent_is_scoped_to_exact_client_and_redirect() {
    let app = app_for("t14_v4").await;
    let ok = approve_fresh(&app, CLIENT, REDIRECT, "").await;
    assert_eq!(ok.status, StatusCode::TEMPORARY_REDIRECT);

    // El par exacto está recordado (control positivo).
    let exact = get(&app, &authorize_uri(CLIENT, REDIRECT, ""), None).await;
    assert_eq!(exact.status, StatusCode::TEMPORARY_REDIRECT);

    let variants = [
        // mismo cliente, redirect distinto
        (CLIENT.to_string(), "http://127.0.0.1:9999/cb?x=1".to_string()),
        (CLIENT.to_string(), "http://127.0.0.1:9999/cb/".to_string()),
        (CLIENT.to_string(), "http://localhost:9999/cb".to_string()),
        // identificadores manipulados
        ("CLAUDE-DESKTOP-TEST".to_string(), REDIRECT.to_string()),
        ("claude-desktop-test ".to_string(), REDIRECT.to_string()),
        ("claude-desktop-test\u{0}".to_string(), REDIRECT.to_string()),
        // inyección del separador interno del key ("client|redirect")
        (format!("{CLIENT}|{REDIRECT}"), REDIRECT.to_string()),
        (
            format!("{CLIENT}|{REDIRECT}"),
            "http://127.0.0.1:9999/cb|junk".to_string(),
        ),
    ];
    for (client, redirect) in variants {
        let reply = get(&app, &authorize_uri(&client, &redirect, ""), None).await;
        assert!(
            !has_code(&reply),
            "una variante NO puede heredar el consentimiento: client={client:?} redirect={redirect:?} status={:?}",
            reply.status
        );
        // Lo esperado es volver a P1 (200); si el redirect no es loopback, 400.
        assert!(
            reply.status == StatusCode::OK || reply.status == StatusCode::BAD_REQUEST,
            "variante: status inesperado {:?}",
            reply.status
        );
    }
}

// ────────────────── V5: scope=operator por variantes ────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn v5_operator_scope_rejected_in_every_shape() {
    let app = app_for("t14_v5").await;
    let operator_variants = [
        "operator",
        "Operator",
        "OPERATOR",
        " operator ",
        "  operator  ",
        "mcp operator",
        "operator mcp",
        "mcp,operator",
        "operator,operator",
        "operator\toperator",
        "mcp\u{00a0}operator", // NBSP: split_whitespace NO lo separa → scope desconocido
    ];
    for raw in operator_variants {
        let uri = format!(
            "/oauth/authorize?client_id={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256&scope={}",
            urlencoding::encode(CLIENT),
            urlencoding::encode(REDIRECT),
            s256(VERIFIER),
            urlencoding::encode(raw),
        );
        let reply = get(&app, &uri, None).await;
        assert_eq!(
            reply.status,
            StatusCode::BAD_REQUEST,
            "scope={raw:?} debe rechazarse"
        );
        assert!(
            reply.body.contains("invalid_scope"),
            "scope={raw:?} debe decir invalid_scope: {}",
            reply.body
        );
        assert!(!has_code(&reply));
    }

    // `mcp` repetido y con espacios sobrantes sí es válido (dedup, sin code aún
    // por no haber consentimiento: P1).
    for raw in ["mcp", "mcp mcp", "  mcp  ", "mcp mcp mcp"] {
        let uri = format!(
            "/oauth/authorize?client_id={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256&scope={}",
            urlencoding::encode(CLIENT),
            urlencoding::encode(REDIRECT),
            s256(VERIFIER),
            urlencoding::encode(raw),
        );
        let reply = get(&app, &uri, None).await;
        assert_eq!(reply.status, StatusCode::OK, "scope={raw:?} debe aceptarse");
        assert!(!has_code(&reply), "sin consentimiento no hay code");
    }
}

// ─────────────────── V6: XSS en client_id/state reflejados ──────────────────

#[tokio::test(flavor = "multi_thread")]
async fn v6_client_controlled_values_cannot_break_out_of_the_page() {
    let app = app_for("t14_v6").await;

    let evil_client = "\"><script>alert(document.cookie)</script>";
    let page = get(&app, &authorize_uri(evil_client, REDIRECT, ""), None).await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(
        !page.body.contains("<script>alert(document.cookie)</script>"),
        "el client_id no debe reflejarse crudo"
    );
    assert!(
        !page.body.contains("onerror="),
        "sin manejadores inline en el HTML"
    );
    let csp = page
        .headers
        .get(header::CONTENT_SECURITY_POLICY)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(csp.contains("default-src 'none'"), "CSP: {csp}");
    assert_eq!(
        page.headers.get(header::X_FRAME_OPTIONS).and_then(|v| v.to_str().ok()),
        Some("DENY")
    );

    // `state` no se pinta en P1 y, al redirigir, debe ir percent-encoded.
    let evil_state = "\"><script>alert(1)</script>\r\nSet-Cookie: x=1";
    let page2 = approving_page_with_state(&app, "cliente-xss", REDIRECT, evil_state).await;
    assert!(
        !page2.body.contains("<script>alert(1)</script>"),
        "state no se refleja crudo en la página"
    );
    let approved = post_form(
        &app,
        Some(&cookie(&page2)),
        &[
            ("consent_id", &hidden(&page2.body, "consent_id")),
            ("csrf", &hidden(&page2.body, "csrf")),
            ("decision", "approve"),
            ("credential", MASTER),
        ],
        None,
        None,
    )
    .await;
    assert_eq!(approved.status, StatusCode::TEMPORARY_REDIRECT);
    let location = approved.location.expect("Location");
    assert!(
        !location.contains("<script>") && !location.contains('\r') && !location.contains('\n'),
        "Location con state escapado: {location}"
    );
    assert!(
        location.contains("state=") && location.contains("%3C"),
        "state percent-encoded en el redirect: {location}"
    );
}

/// P1 con un `state` concreto (para el vector de reflexión en redirect).
async fn approving_page_with_state(
    app: &axum::Router,
    client: &str,
    redirect: &str,
    state: &str,
) -> Reply {
    let uri = format!(
        "{}&state={}",
        authorize_uri(client, redirect, ""),
        urlencoding::encode(state)
    );
    get(app, &uri, None).await
}

// ─────────────── V7: rate limit y X-Forwarded-For ───────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn v7_forwarded_for_does_not_bypass_the_rate_limit() {
    let app = app_for("t14_v7").await;

    // (a) Sin ConnectInfo (bucket "unknown"): rotar X-Forwarded-For NO abre
    // cupos nuevos; a los 5 fallos la página queda bloqueada.
    let uri = authorize_uri("cliente-rl", "http://127.0.0.1:8001/cb", "");
    let page = get(&app, &uri, None).await;
    let session = cookie(&page);
    let consent_id = hidden(&page.body, "consent_id");
    let mut csrf = hidden(&page.body, "csrf");
    let mut last = page;
    for i in 0..tylluan_kernel::transport::http::oauth_consent::MAX_FAILURES {
        let reply = post_form(
            &app,
            Some(&session),
            &[
                ("consent_id", &consent_id),
                ("csrf", &csrf),
                ("decision", "approve"),
                ("credential", "mala"),
            ],
            None,
            Some(&format!("10.0.0.{i}")),
        )
        .await;
        assert_eq!(reply.status, StatusCode::FORBIDDEN, "fallo {i} registrado");
        csrf = hidden(&reply.body, "csrf");
        last = reply;
    }
    let _ = last;
    let locked = post_form(
        &app,
        Some(&session),
        &[
            ("consent_id", &consent_id),
            ("csrf", &csrf),
            ("decision", "approve"),
            ("credential", MASTER),
        ],
        None,
        Some("10.0.0.99"),
    )
    .await;
    assert_eq!(
        locked.status,
        StatusCode::TOO_MANY_REQUESTS,
        "X-Forwarded-For no debe crear cupos nuevos (IP del socket)"
    );
    assert!(locked.headers.get(header::RETRY_AFTER).is_some());

    // (b) Con ConnectInfo real: el cupo es POR IP DE SOCKET, no por cabecera.
    let app2 = app_for("t14_v7b").await;
    let page2 = get(&app2, &uri, None).await;
    let session2 = cookie(&page2);
    let cid2 = hidden(&page2.body, "consent_id");
    let mut csrf2 = hidden(&page2.body, "csrf");
    let ip_a: SocketAddr = "127.0.0.1:41001".parse().unwrap();
    for i in 0..4 {
        let reply = post_form(
            &app2,
            Some(&session2),
            &[
                ("consent_id", &cid2),
                ("csrf", &csrf2),
                ("decision", "approve"),
                ("credential", "mala"),
            ],
            Some(ip_a),
            Some(&format!("192.168.1.{i}")),
        )
        .await;
        assert_eq!(reply.status, StatusCode::FORBIDDEN);
        csrf2 = hidden(&reply.body, "csrf");
    }
    let fifth = post_form(
        &app2,
        Some(&session2),
        &[
            ("consent_id", &cid2),
            ("csrf", &csrf2),
            ("decision", "approve"),
            ("credential", "mala"),
        ],
        Some(ip_a),
        Some("192.168.1.200"),
    )
    .await;
    assert_eq!(fifth.status, StatusCode::FORBIDDEN, "5º fallo de la IP A");
    let locked_a = post_form(
        &app2,
        Some(&session2),
        &[
            ("consent_id", &cid2),
            ("csrf", &csrf2),
            ("decision", "approve"),
            ("credential", MASTER),
        ],
        Some(ip_a),
        None,
    )
    .await;
    assert_eq!(
        locked_a.status,
        StatusCode::TOO_MANY_REQUESTS,
        "la IP A queda bloqueada aunque cambie X-Forwarded-For"
    );

    // (c) D-25: el techo global solo FRENA el ritmo; ya no bloquea a una IP que
    // no ha fallado. 20 fallos desde 20 IPs distintas (un tercero) no pueden
    // dejar la pantalla inservible para el operador: antes, esos 20 POSTs
    // bastaban para 300 s de 429 para todo el mundo.
    let app3 = app_for("t14_v7c").await;
    let page3 = get(&app3, &uri, None).await;
    let session3 = cookie(&page3);
    let cid3 = hidden(&page3.body, "consent_id");
    let mut csrf3 = hidden(&page3.body, "csrf");
    for i in 0..20 {
        // IP de socket DISTINTA en cada iteración (el puerto no cambia la IP):
        // cada una acumula un solo fallo propio.
        let ip: SocketAddr = format!("127.0.0.{}:40000", 10 + i).parse().unwrap();
        let reply = post_form(
            &app3,
            Some(&session3),
            &[
                ("consent_id", &cid3),
                ("csrf", &csrf3),
                ("decision", "approve"),
                ("credential", "mala"),
            ],
            Some(ip),
            None,
        )
        .await;
        assert_eq!(
            reply.status,
            StatusCode::FORBIDDEN,
            "IP {ip}: primer fallo propio, no puede estar bloqueada"
        );
        csrf3 = hidden(&reply.body, "csrf");
    }
    let fresh_ip: SocketAddr = "127.0.0.3:43000".parse().unwrap();
    let global = post_form(
        &app3,
        Some(&session3),
        &[
            ("consent_id", &cid3),
            ("csrf", &csrf3),
            ("decision", "approve"),
            ("credential", MASTER),
        ],
        Some(fresh_ip),
        None,
    )
    .await;
    assert_eq!(
        global.status,
        StatusCode::TEMPORARY_REDIRECT,
        "el techo global no puede bloquear a una IP sin fallos propios"
    );
    assert!(has_code(&global), "la aprobación legítima debe completarse");
}
