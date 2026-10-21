//! Fase 2 (D-15) — consentimiento de operador end-to-end.
//!
//! Spec: `docs/internal/PLAN_oauth_f2_consentimiento_deep_2026-10.md`.
//! Complementa `oauth_f2_adversarial_spec.rs` (C1/C2a/C2c) con el camino
//! completo sobre el router real: P1 → credencial → code → JWT, y con los
//! estados del autómata (PENDING, APPROVED, CONSUMED, DENIED, LOCKED).
//!
//! Lo que aquí se afirma y no se puede afirmar en la suite adversarial:
//!   * sin credencial no hay code aunque la petición SÍ lleve `client_id`;
//!   * sin consentimiento previo P1 no emite nada (200 HTML, sin redirect);
//!   * con consentimiento previo son CERO acciones humanas (302 directo);
//!   * denegar no emite y no se recuerda;
//!   * 5 credenciales erróneas por IP → 429 (no un sexto intento);
//!   * otro `redirect_uri` del mismo cliente no hereda el consentimiento;
//!   * un kernel sin credencial configurada falla cerrado.

mod support;

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use tylluan_kernel::transport::http::build_router;

const MASTER: &str = "master-test-token-d15-consent";
const VERIFIER: &str = "d15-consent-verifier-0123456789abcdefghijklmnop";
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

async fn get(app: &axum::Router, uri: &str) -> Reply {
    let resp = app
        .clone()
        .oneshot(Request::builder().method("GET").uri(uri).body(Body::empty()).unwrap())
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
    let resp = app
        .clone()
        .oneshot(builder.body(Body::from(body)).unwrap())
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

/// Valor de un `<input type="hidden" name="…" value="…">` de P1.
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

async fn exchange(app: &axum::Router, code: &str, redirect: &str) -> Reply {
    let form = format!(
        "grant_type=authorization_code&code={}&redirect_uri={}&code_verifier={}",
        urlencoding::encode(code),
        urlencoding::encode(redirect),
        VERIFIER
    );
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/oauth/token")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(form))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .map(|b| String::from_utf8_lossy(&b).to_string())
        .unwrap_or_default();
    Reply { status, location: None, headers, body }
}

async fn state(tag: &str, token: Option<&str>) -> std::sync::Arc<tylluan_kernel::transport::http::HttpState> {
    support::test_state_custom(tag, token.map(str::to_string), false).await
}

#[tokio::test(flavor = "multi_thread")]
async fn p1_renders_and_issues_nothing() {
    let app = build_router(state("d15_p1", Some(MASTER)).await);
    let reply = get(&app, &authorize_uri(CLIENT, REDIRECT, "")).await;

    assert_eq!(reply.status, StatusCode::OK, "P1 se sirve, no se deniega");
    assert!(reply.location.is_none(), "P1 no redirige ni emite code");
    assert!(
        reply.body.contains(CLIENT),
        "el operador tiene que ver qué cliente pide acceso"
    );
    assert!(
        reply.body.contains("http://127.0.0.1:9999"),
        "y a qué host va el código"
    );
    for tool in [
        "tylluan_do",
        "tylluan_remember",
        "tylluan_recall",
        "tylluan_think",
        "tylluan_graph",
        "tylluan_decide",
    ] {
        assert!(reply.body.contains(tool), "P1 declara el scope mcp: falta {tool}");
    }
    assert!(!reply.body.contains("code="), "ningún code en el HTML");

    let h = &reply.headers;
    assert_eq!(h.get(header::CACHE_CONTROL).unwrap(), "no-store");
    assert_eq!(h.get(header::X_FRAME_OPTIONS).unwrap(), "DENY");
    let csp = h.get(header::CONTENT_SECURITY_POLICY).unwrap().to_str().unwrap();
    assert!(csp.contains("default-src 'none'"), "CSP: {csp}");
    assert!(csp.contains("frame-ancestors 'none'"), "CSP: {csp}");
    assert!(h.get(header::SET_COOKIE).is_some(), "cookie HttpOnly de sesión");
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_client_id_never_renders_or_issues() {
    let app = build_router(state("d15_noclient", Some(MASTER)).await);
    let uri = format!(
        "/oauth/authorize?redirect_uri={}&code_challenge={}&code_challenge_method=S256",
        urlencoding::encode(REDIRECT),
        s256(VERIFIER)
    );
    let reply = get(&app, &uri).await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN);
    assert!(reply.location.is_none());
    assert!(reply.body.contains("access_denied"));
}

#[tokio::test(flavor = "multi_thread")]
async fn correct_credential_issues_a_working_jwt() {
    let app = build_router(state("d15_ok", Some(MASTER)).await);
    let page = get(&app, &authorize_uri(CLIENT, REDIRECT, "")).await;
    assert_eq!(page.status, StatusCode::OK);
    let consent_id = hidden(&page.body, "consent_id");
    let csrf = hidden(&page.body, "csrf");
    assert!(!consent_id.is_empty() && !csrf.is_empty());

    let reply = post_form(
        &app,
        Some(&cookie(&page)),
        &[
            ("consent_id", &consent_id),
            ("csrf", &csrf),
            ("decision", "approve"),
            ("credential", MASTER),
        ],
    )
    .await;
    assert_eq!(reply.status, StatusCode::TEMPORARY_REDIRECT, "aprobado → 302 con code");
    let location = reply.location.expect("Location");
    assert!(location.starts_with(REDIRECT), "vuelve al redirect_uri: {location}");
    let code = code_from(&location);
    assert!(!code.is_empty());

    let token = exchange(&app, &code, REDIRECT).await;
    assert_eq!(token.status, StatusCode::OK, "code canjeable: {}", token.body);
    let v: serde_json::Value = serde_json::from_str(&token.body).unwrap();
    assert_eq!(v["token_type"], "bearer");
    assert!(v["access_token"].as_str().unwrap().split('.').count() == 3);

    // C4a: el code es de un solo uso, incluso tras un canje correcto.
    let replay = exchange(&app, &code, REDIRECT).await;
    assert_eq!(replay.status, StatusCode::BAD_REQUEST);
}

#[tokio::test(flavor = "multi_thread")]
async fn remembered_consent_needs_zero_human_actions() {
    let app = build_router(state("d15_remember", Some(MASTER)).await);
    let uri = authorize_uri(CLIENT, REDIRECT, "");
    let page = get(&app, &uri).await;
    let reply = post_form(
        &app,
        Some(&cookie(&page)),
        &[
            ("consent_id", &hidden(&page.body, "consent_id")),
            ("csrf", &hidden(&page.body, "csrf")),
            ("decision", "approve"),
            ("credential", MASTER),
        ],
    )
    .await;
    assert_eq!(reply.status, StatusCode::TEMPORARY_REDIRECT);

    // Segunda conexión: P1 no aparece, el code sale directo (302).
    let again = get(&app, &uri).await;
    assert_eq!(again.status, StatusCode::TEMPORARY_REDIRECT, "consentimiento recordado");
    assert!(!code_from(&again.location.clone().unwrap()).is_empty());

    // Otro redirect del mismo cliente NO hereda el consentimiento.
    let other = get(&app, &authorize_uri(CLIENT, "http://127.0.0.1:5000/cb", "")).await;
    assert_eq!(other.status, StatusCode::OK, "otro redirect vuelve a P1");
}

#[tokio::test(flavor = "multi_thread")]
async fn denial_emits_nothing_and_is_not_remembered() {
    let app = build_router(state("d15_deny", Some(MASTER)).await);
    let uri = authorize_uri(CLIENT, REDIRECT, "");
    let page = get(&app, &uri).await;
    let reply = post_form(
        &app,
        Some(&cookie(&page)),
        &[
            ("consent_id", &hidden(&page.body, "consent_id")),
            ("csrf", &hidden(&page.body, "csrf")),
            ("decision", "deny"),
            ("credential", ""),
        ],
    )
    .await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN);
    assert!(reply.location.is_none(), "denegar no emite code");
    assert!(reply.body.contains("access_denied"));

    let again = get(&app, &uri).await;
    assert_eq!(again.status, StatusCode::OK, "denegado no se recuerda");
}

#[tokio::test(flavor = "multi_thread")]
async fn five_wrong_credentials_lock_the_page() {
    let app = build_router(state("d15_lock", Some(MASTER)).await);
    let page = get(&app, &authorize_uri(CLIENT, REDIRECT, "")).await;
    let consent_id = hidden(&page.body, "consent_id");
    let session = cookie(&page);
    let mut csrf = hidden(&page.body, "csrf");

    let mut last = post_form(
        &app,
        Some(&session),
        &[
            ("consent_id", &consent_id),
            ("csrf", &csrf),
            ("decision", "approve"),
            ("credential", "no-es-la-credencial"),
        ],
    )
    .await;
    assert_eq!(last.status, StatusCode::FORBIDDEN, "sin credencial no hay code");
    assert!(last.location.is_none());
    assert!(last.body.contains("Credencial incorrecta"));

    for _ in 1..tylluan_kernel::transport::http::oauth_consent::MAX_FAILURES {
        csrf = hidden(&last.body, "csrf");
        last = post_form(
            &app,
            Some(&session),
            &[
                ("consent_id", &consent_id),
                ("csrf", &csrf),
                ("decision", "approve"),
                ("credential", "no-es-la-credencial"),
            ],
        )
        .await;
        assert_eq!(last.status, StatusCode::FORBIDDEN);
    }

    // Sexto intento, esta vez con la credencial CORRECTA: la página está
    // bloqueada y no emite nada.
    csrf = hidden(&last.body, "csrf");
    let locked = post_form(
        &app,
        Some(&session),
        &[
            ("consent_id", &consent_id),
            ("csrf", &csrf),
            ("decision", "approve"),
            ("credential", MASTER),
        ],
    )
    .await;
    assert_eq!(locked.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(locked.location.is_none());
    assert!(locked.headers.get(header::RETRY_AFTER).is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn post_without_session_cookie_is_rejected() {
    let app = build_router(state("d15_csrf", Some(MASTER)).await);
    let page = get(&app, &authorize_uri(CLIENT, REDIRECT, "")).await;
    let reply = post_form(
        &app,
        None,
        &[
            ("consent_id", &hidden(&page.body, "consent_id")),
            ("csrf", &hidden(&page.body, "csrf")),
            ("decision", "approve"),
            ("credential", MASTER),
        ],
    )
    .await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN);
    assert!(reply.location.is_none(), "sin cookie de sesión no hay code");
}

#[tokio::test(flavor = "multi_thread")]
async fn operator_scope_is_always_invalid() {
    let app = build_router(state("d15_scope", Some(MASTER)).await);
    for extra in ["&scope=operator", "&scope=mcp+operator"] {
        let reply = get(&app, &authorize_uri(CLIENT, REDIRECT, extra)).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "scope operator: {extra}");
        assert!(reply.body.contains("invalid_scope"), "{}", reply.body);
        assert!(reply.location.is_none());
    }
    // `mcp` explícito sí vale (y sigue sin emitir sin consentimiento).
    let ok = get(&app, &authorize_uri(CLIENT, REDIRECT, "&scope=mcp")).await;
    assert_eq!(ok.status, StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread")]
async fn kernel_without_operator_credential_fails_closed() {
    // Sin token maestro configurado no hay credencial que comparar: P1 se
    // sirve (el humano ve qué se le pide) pero aprobar no puede emitir.
    let app = build_router(state("d15_nocred", None).await);
    let page = get(&app, &authorize_uri(CLIENT, REDIRECT, "")).await;
    assert_eq!(page.status, StatusCode::OK);

    let reply = post_form(
        &app,
        Some(&cookie(&page)),
        &[
            ("consent_id", &hidden(&page.body, "consent_id")),
            ("csrf", &hidden(&page.body, "csrf")),
            ("decision", "approve"),
            ("credential", ""),
        ],
    )
    .await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN);
    assert!(reply.location.is_none(), "sin credencial configurada no se emite");
}
