//! ADR-019 OAuth FASE 2 — suite adversarial escrita como ESPECIFICACIÓN
//! (asignación B1 del TL, 2026-10-19; diseño F2 = T1281, aún en curso).
//!
//! Cada caso afirma el resultado ESPERADO y el POR QUÉ, con referencia al
//! diseño (`docs/internal/DESIGN_oauth_operator_2026-10.md`, §§2/5, números
//! R*) y al código F1 verificado hoy (`transport/http/oauth.rs`,
//! `transport/http/auth.rs`).
//!
//! ACTIVACION (B-5, 2026-10-20 + D-15, 2026-10-21): los 8 casos corren en
//! la corrida normal de cada CI. C2b, C3, C4a, C4b y C5 se activaron en B-5
//! con las fases A/B de F2 (`d88512d`). C1, C2a y C2c se activan aqui porque
//! el contrato de consentimiento ya esta congelado E IMPLEMENTADO
//! (`transport/http/oauth_consent.rs`, P1 + `consents.json`, D-15).
//!
//! Nota de D-15 sobre C1 y C2c: su peticion viaja SIN `client_id`, que RFC
//! 6749 4.1.1 exige en toda authorization request. Sin el no hay a quien
//! mostrar en P1 ni a quien recordar, asi que mueren en 403 `access_denied`
//! antes de renderizar: la asercion (403, sin Location, sin code canjeable)
//! se cumple por la puerta correcta. El invariante de fondo -sin credencial
//! del operador no hay code, y sin consentimiento previo tampoco- se prueba
//! CON `client_id` en `tests/oauth_f2_consent.rs` (D-15).
//!
//! Estado verificado el 2026-10-21 (D-15): 8 passed, 0 failed, 0 ignored.
//!
//! Historial: al escribir la suite (2026-10-19) solo C2b FALLABA - hallazgo
//! real en `is_loopback_redirect_uri` (ver doc del caso), cerrado en `1a2b6a0`.
//!
//! Lo que YA está cubierto por F1 y NO se duplica aquí: R12/R13/R14/R15 y E1
//! (token maestro por query no satisface la clase operador) viven en
//! `tests/oauth_operator_gate.rs` y en los `fase1_tests` de `oauth.rs`.
//!
//! Amenazas de referencia (DESIGN §2): T1 (dispensador JWT de LAN → RCE),
//! T2 (redirect_uri arbitraria filtra el code), T3 (cualquier bearer hereda
//! default_role=admin), T5 (token por query en logs/historial).

mod support;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use tylluan_kernel::transport::http::build_router;

const MASTER: &str = "master-test-token-f2-spec";
/// PKCE verifier/par de prueba (43+ chars según RFC 7636 §4.1).
const VERIFIER: &str = "f2-spec-verifier-0123456789abcdefghijklmnopqrstuv";
const REDIRECT: &str = "http://127.0.0.1:9999/cb";

fn s256(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// Issue an authorize request and return (status, Location, body).
async fn get(app: &axum::Router, uri: &str) -> (StatusCode, Option<String>, String) {
    let resp = app
        .clone()
        .oneshot(Request::builder().method("GET").uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let location = resp
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .map(|b| String::from_utf8_lossy(&b).to_string())
        .unwrap_or_default();
    (status, location, body)
}

fn code_from_location(location: &str) -> String {
    location
        .split("code=")
        .nth(1)
        .unwrap_or("")
        .split('&')
        .next()
        .unwrap_or("")
        .to_string()
}

/// Full F1 happy-path up to a code: authorize (dev_mode=true) → 302 → code.
async fn issue_code(app: &axum::Router, redirect: &str) -> String {
    let uri = format!(
        "/oauth/authorize?redirect_uri={}&code_challenge={}&code_challenge_method=S256",
        urlencoding::encode(redirect),
        s256(VERIFIER)
    );
    let (status, location, _) = get(app, &uri).await;
    assert!(status.is_redirection(), "authorize must redirect, got {status}");
    code_from_location(&location.expect("302 must carry a Location"))
}

/// POST /oauth/token with the given code+redirect; returns (status, body).
async fn exchange(app: &axum::Router, code: &str, redirect: &str) -> (StatusCode, String) {
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
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .map(|b| String::from_utf8_lossy(&b).to_string())
        .unwrap_or_default();
    (status, body)
}

/// Ruta de clase operador usada como sondeo (mismo patrón que
/// `oauth_operator_gate.rs`: el gate responde ANTES que el handler).
const OPERATOR_PROBE: &str = "/api/v1/federation/peers/nonexistent/permissions";
const PERMS_BODY: &str = r#"{"sync_spaces":[],"dispatch_allow":[],"coloquio_channels":[]}"#;

async fn put_operator(app: &axum::Router, authorization: Option<&str>) -> StatusCode {
    let mut req = Request::builder()
        .method("PUT")
        .uri(OPERATOR_PROBE)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(auth) = authorization {
        req = req.header(header::AUTHORIZATION, auth);
    }
    let resp = app.clone().oneshot(req.body(Body::from(PERMS_BODY)).unwrap()).await.unwrap();
    resp.status()
}

// ── C1: /oauth/authorize SIN credencial de operador ──────────────────────────

/// CASO C1 (DESIGN §3 Fase 2 + R10/R13).
///
/// ESPERADO: con `dev_mode=false` (producción/LAN) y sin credencial de
/// operador, `GET /oauth/authorize` responde 403 `access_denied`, NO redirige
/// (sin cabecera Location, sin code), y ningún código posteriormente
/// intercambiable llega a existir: un intercambio con cualquier verificador
/// responde 400 `invalid_grant`.
///
/// POR QUÉ: es el cierre de la cadena RCE T1 (DESIGN §0): sin emisión no
/// existe JWT que autentique `/messages` → `tylluan_do` → guild `bash`. En F2
/// el endpoint pasará a servir una página de consentimiento, pero la regla
/// fuerte no cambia: **sin la credencial de operador en ese consentimiento no
/// se emite code nunca**. Este test es la cota que F2 no debe cruzar.
///
/// HOY (F1): ya cumple — `oauth.rs` rehúsa con 403 cuando `dev_mode=false`
/// (`fase1_tests::r13` es el test unitario; aquí se afirma además que no se
/// filtra Location ni code intercambiable).
///
/// ACTIVO desde D-15 (2026-10-21), con el contrato ya implementado: esta
/// peticion no lleva `client_id` (RFC 6749 4.1.1 lo exige), asi que muere
/// en 403 antes de renderizar P1. El mismo invariante CON `client_id`
/// -pagina sin credencial = ningun code- se prueba en
/// `tests/oauth_f2_consent.rs`.
#[tokio::test(flavor = "multi_thread")]
async fn c1_authorize_without_operator_credential_issues_no_code() {
    let state = support::test_state_custom("f2_c1", Some(MASTER.to_string()), false).await;
    let app = build_router(state);

    let uri = format!(
        "/oauth/authorize?redirect_uri={}&code_challenge={}&code_challenge_method=S256",
        urlencoding::encode(REDIRECT),
        s256(VERIFIER)
    );
    let (status, location, body) = get(&app, &uri).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "sin credencial de operador: 403");
    assert!(location.is_none(), "no debe redirigir ni emitir code: {location:?}");
    assert!(body.contains("access_denied"), "sobre de error estándar OAuth: {body}");

    // Ningún code emitido es intercambiable: hasta un code adivinado muere en 400.
    let (status, body) = exchange(&app, "guessed-code", REDIRECT).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains("invalid_grant"), "rechazo explícito: {body}");
}

// ── C2: redirect_uri hostil / no registrada ──────────────────────────────────

/// CASO C2a (DESIGN §3 Fase 1-c, R15; amenaza T2).
///
/// ESPERADO: con `dev_mode=true`, todo `redirect_uri` cuyo host real NO sea
/// loopback responde 400 `invalid_request`: dominio de atacante, lookalike
/// `127.0.0.1.evil…`, DNS apuntando a loopback (`127.0.0.1.nip.io` resuelve a
/// 127.0.0.1 — el allowlist es de NOMBRES, no de IPs), y esquemas no-http
/// (`file:`, `javascript:`).
///
/// POR QUÉ: el 302 lleva el code a quien controle el host; con un redirect
/// hostil el atacante recibe el code y, como él mismo eligió el challenge,
/// puede canjearlo (PKCE no protege contra el dueño del challenge). Con
/// `dev_mode=true` el auto-apruebo es deliberado (R12), así que el loopback
/// allowlist es la ÚNICA defensa de la cadena.
///
/// HOY (F1): pasa — `is_loopback_redirect_uri` rehúsa todos estos vectores
/// (`fase1_tests::r15` cubre el lookalike básico).
#[tokio::test(flavor = "multi_thread")]
async fn c2a_hostile_redirect_uris_rejected() {
    let state = support::test_state("f2_c2a").await; // dev_mode=true
    let app = build_router(state);

    let vectors: &[(&str, &str)] = &[
        ("http://evil.example/cb", "dominio de atacante"),
        ("http://127.0.0.1.evil.example/cb", "lookalike con prefijo loopback"),
        ("http://127.0.0.1.nip.io/cb", "DNS de atacante que resuelve a loopback"),
        ("http://192.168.1.10:47004/cb", "otro host de LAN"),
        ("file://127.0.0.1/cb", "esquema no-http"),
        ("javascript:alert(1)", "esquema no-http"),
    ];
    for (uri, why) in vectors {
        let q = format!(
            "/oauth/authorize?redirect_uri={}&code_challenge={}&code_challenge_method=S256",
            urlencoding::encode(uri),
            s256(VERIFIER)
        );
        let (status, _, _) = get(&app, &q).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "vector '{uri}' debe morir en 400 ({why})");
    }
}

/// CASO C2b (DESIGN §3 Fase 1-c; amenaza T2) — hallazgo original de esta suite, CERRADO en `1a2b6a0`.
///
/// ESPERADO: `http://127.0.0.1:9999@evil.example/cb` (y su variante
/// `localhost`) → 400 `invalid_request`.
///
/// POR QUÉ: en la gramática RFC 3986 la autoridad es `userinfo@host`; aquí el
/// host REAL es `evil.example` y `127.0.0.1:9999` es solo userinfo. El
/// navegador (y el Location generado) seguiría el redirect hacia
/// `evil.example/cb?code=…`, filtrando el code al atacante — exactamente la
/// fuga T2 que el allowlist de loopback existe para cerrar. PKCE no ayuda:
/// el atacante eligió su propio challenge.
///
/// HISTORIAL (2026-10-19): **FALLABA** — `is_loopback_redirect_uri` (`oauth.rs`, Fase 1-c)
/// partía la autoridad con `split(':').next()` y veía "127.0.0.1" sin nunca
/// mirar el '@', así que aceptaba el URI (verificado en vivo: el test devolvió
/// 307, `Redirect::temporary`). Acotación del riesgo: solo emitía con
/// `dev_mode=true` (F1 ya cierra dev_mode=false), y el intercambio del code
/// exige alcanzar el endpoint de token en loopback — explotaba desde un
/// proceso local o una página web que fuerce la petición, no desde la LAN a
/// un kernel en 127.0.0.1.
/// CERRADO en `1a2b6a0` (la autoridad se parsea de verdad: '@' y backslash
/// rechazados). El test está ACTIVO (sin `#[ignore]`) desde entonces; pasa,
/// verificado en vivo el 2026-10-20.
#[tokio::test(flavor = "multi_thread")]
async fn c2b_userinfo_redirect_uri_rejected() {
    let state = support::test_state("f2_c2b").await; // dev_mode=true
    let app = build_router(state);

    let vectors = [
        "http://127.0.0.1:9999@evil.example/cb",
        "http://localhost:8080@evil.example/cb",
    ];
    for uri in vectors {
        let q = format!(
            "/oauth/authorize?redirect_uri={}&code_challenge={}&code_challenge_method=S256",
            urlencoding::encode(uri),
            s256(VERIFIER)
        );
        let (status, _, _) = get(&app, &q).await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "userinfo@host con apariencia loopback ('{uri}') debe morir en 400: el host real es evil.example"
        );
    }
}

/// CASO C2c — redirect_uri no registrada sin consentimiento (spec F2, R10).
///
/// ESPERADO: cualquier redirect_uri — registrada o no — sin consentimiento de
/// operador produce 403 y ningún code intercambiable. Con F2, la registración
/// (DESIGN §Fase 2: lista estática + loopback con puerto efímero para
/// clientes nativos, RFC 8252) añade una cota SUFICIENTE sobre la del
/// allowlist de loopback: loopback deja de ser condición suficiente y el
/// registro + consentimiento pasan a serlo.
///
/// POR QUÉ: sin este orden, "es loopback" equivale a "emito", que es el
/// estado T1. El registro es además la ancla donde el operador ve QUÉ cliente
/// recibe tokens.
///
/// HOY (F1): pasa — con `dev_mode=false` la única respuesta es 403, no
/// existe emisión que registrar.
///
/// ACTIVO desde D-15 (2026-10-21): P1 exige `client_id` + consentimiento
/// previo; esta peticion (sin `client_id`) no emite nada. La variante CON
/// `client_id` y sin consentimiento -pagina servida, cero emision- vive en
/// `tests/oauth_f2_consent.rs`.
#[tokio::test(flavor = "multi_thread")]
async fn c2c_unregistered_redirect_without_consent_issues_nothing() {
    let state = support::test_state_custom("f2_c2c", Some(MASTER.to_string()), false).await;
    let app = build_router(state);

    // Redirect loopback válida pero NO registrada (no existe registro aún):
    // sin consentimiento no emite, registrada o no.
    let uri = format!(
        "/oauth/authorize?redirect_uri={}&code_challenge={}&code_challenge_method=S256",
        urlencoding::encode("http://127.0.0.1:54321/unregistered-cb"),
        s256(VERIFIER)
    );
    let (status, location, _) = get(&app, &uri).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "loopback no registrada sin consentimiento: 403");
    assert!(location.is_none(), "sin code emitido: {location:?}");
}

// ── C3: escalada de scope mcp → operator ─────────────────────────────────────

/// CASO C3 (DESIGN §3 Fase 2, R11; amenaza T3).
///
/// ESPERADO: un JWT portando `scope=mcp` recibe 403 `operator_required` en
/// TODA la clase operador, incluso cuando el rol global por defecto sería
/// `admin` (el kernel por defecto tiene `default_role=admin`). El rol de un
/// JWT se deriva SIEMPRE de su scope, nunca del `default_role` global. Un
/// `scope=operator` solo puede emitirse tras consentimiento de operador
/// (invariante de emisión, C1); un atacante no puede forjar el claim porque
/// el HMAC lo firma el kernel.
///
/// POR QUÉ: T3 — hoy cualquier bearer cae al `default_role`; si F2 añade el
/// claim `scope` pero deja que el rol se resuelva por la vía antigua, un
/// cliente MCP de propósito general (clase mcp) hereda permisos de operador
/// (shutdown, purge, bash, federation…). El cinturón F1(b) ya rechaza
/// `AclAuthKind::OAuthJwt` en la clase operador; este caso exige que F2
/// mantenga el resultado 403 aunque internamente pase a mirar el scope.
///
/// HOY (F1): pasa — el operator gate rechaza cualquier JWT OAuth sin mirar
/// claims (r14 en la batería F1). El test afirma el MISMO resultado 403 con
/// un JWT minteado en directo, más el control positivo del token maestro.
#[tokio::test(flavor = "multi_thread")]
async fn c3_mcp_scope_never_satisfies_operator_class() {
    let state = support::test_state_custom("f2_c3", Some(MASTER.to_string()), false).await;
    // JWT firmado por el kernel con claim `scope=mcp` (desde d88512d): debe
    // producir exactamente el mismo 403 que cualquier otro JWT OAuth.
    let jwt = state.oauth.sign_jwt("f2-spec-jti-c3", 60);
    let app = build_router(state);

    let status = put_operator(&app, Some(&format!("Bearer {jwt}"))).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "un JWT OAuth (scope mcp) jamás satisface la clase operador (R11)"
    );

    // Control positivo: el token maestro SÍ pasa el gate (el handler 404ea).
    let status = put_operator(&app, Some(&format!("Bearer {MASTER}"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "token maestro: gate pasa, handler responde");
}

// ── C4: replay del code ──────────────────────────────────────────────────────

/// CASO C4a (DESIGN §5 R7).
///
/// ESPERADO: el code de autorización es de USO ÚNICO. Primer canje con el
/// verificador correcto → 200 con access_token; segundo canje idéntico →
/// 400 `invalid_grant` (ya sea por código no existente o expirado; TTL 60s).
///
/// POR QUÉ: replay de code = el code robado (por C2b, logs, o un proxy) deja
/// de valer tras el primer canje; RFC 6749 §4.1.2 lo manda y es la mitad
/// defensiva de PKCE: sin uso único, un code filtrado es un token de 1h.
///
/// HOY (F1): pasa — `token_handler` hace `remove()` atómico del code.
#[tokio::test(flavor = "multi_thread")]
async fn c4a_auth_code_is_single_use() {
    let state = support::test_state("f2_c4a").await; // dev_mode=true
    let app = build_router(state);

    let code = issue_code(&app, REDIRECT).await;
    assert!(!code.is_empty(), "authorize debe emitir un code");

    let (status, body) = exchange(&app, &code, REDIRECT).await;
    assert_eq!(status, StatusCode::OK, "primer canje válido: 200: {body}");
    assert!(body.contains("access_token"), "devuelve JWT: {body}");

    let (status, body) = exchange(&app, &code, REDIRECT).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "replay del mismo code: 400");
    assert!(body.contains("invalid_grant"), "rechazo estándar OAuth: {body}");
}

/// CASO C4b (DESIGN §5 R7 — binding del code a su redirect).
///
/// ESPERADO: un code emitido para `R1` no puede canjearse presentando `R2`:
/// 400 `invalid_grant` (redirect_uri mismatch).
///
/// POR QUÉ: el binding impide inyectar un code emitido en otro flujo
/// (ataque de canje cruzado entre clientes) y es la misma garantía que hará
/// falta cuando F2 registre redirect_uris por cliente (C2c): el code queda
/// atestado al par (cliente, redirect) concreto que lo recibió.
///
/// HOY (F1): pasa — `token_handler` compara contra `pending.redirect_uri`.
#[tokio::test(flavor = "multi_thread")]
async fn c4b_auth_code_bound_to_original_redirect() {
    let state = support::test_state("f2_c4b").await; // dev_mode=true
    let app = build_router(state);

    let code = issue_code(&app, REDIRECT).await;
    let (status, body) = exchange(&app, &code, "http://127.0.0.1:9999/other").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "redirect distinto al emisor: 400");
    assert!(body.contains("invalid_grant"), "rechazo estándar: {body}");
}

// ── C5: token por query-string ───────────────────────────────────────────────

/// CASO C5 (amenaza T5 + decisión E1 del Coloquio turno 1237).
///
/// ESPERADO: un OAuth JWT presentado como `?token=` (o `?access_token=`)
/// NUNCA autentica: 401. Solo el token maestro acepta el medio query (y aun
/// así no satisface la clase operador — E1, ya testado en la batería F1).
///
/// POR QUÉ: los query strings quedan en logs, historiales de navegador,
/// proxies y Referer; además la superficie de canje (cualquier JS que fuerce
/// la petición) crece. Con scopes en F2 el error sería peor: un `scope=operator`
/// filtrado por query sería un token de operador en texto plano. `?access_token=`
/// se incluye para que el nombre del parámetro RFC 9207 no se convierta por
/// accidente en un canal de autenticación.
///
/// HOY (F1): pasa — el middleware solo compara el query contra el token
/// maestro (`has_valid_query`, auth.rs); un JWT por query no autentica y la
/// petición cae en 401 del bearer middleware.
#[tokio::test(flavor = "multi_thread")]
async fn c5_oauth_jwt_in_query_never_authenticates() {
    let state = support::test_state_custom("f2_c5", Some(MASTER.to_string()), false).await;
    let jwt = state.oauth.sign_jwt("f2-spec-jti-c5", 60);
    let app = build_router(state);

    for param in ["token", "access_token"] {
        let uri = format!("/api/v1/coloquio/channels?{param}={}", urlencoding::encode(&jwt));
        let (status, _, _) = get(&app, &uri).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "un JWT OAuth por query-string ('{param}=') no autentica jamás"
        );
    }
}
