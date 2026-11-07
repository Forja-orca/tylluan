//! T-DSH-86: the MCP/SSE DNS-rebinding guard (`validate_mcp_origin_host_middleware`).
//!
//! Drives the real router in-process (`support::test_state_custom`), no kernel
//! launched. Cases mirror T-DSH-85's table (403 in 1-7, 200 in 8) plus the
//! non-browser path (no Origin), both with and without dev_mode.

mod support;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;

const RPC_LIST: &str = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#;

#[allow(clippy::too_many_arguments)]
async fn call(
    app: &axum::Router,
    method: &str,
    uri: &str,
    origin: Option<&str>,
    host: Option<&str>,
    bearer: Option<&str>,
    body: Option<&str>,
) -> StatusCode {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(o) = origin {
        builder = builder.header(header::ORIGIN, o);
    }
    if let Some(h) = host {
        builder = builder.header(header::HOST, h);
    }
    if let Some(t) = bearer {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    if method == "OPTIONS" {
        builder = builder.header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST");
    } else if method == "GET" && uri == "/sse" {
        builder = builder.header(header::ACCEPT, "text/event-stream");
    } else {
        builder = builder
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT, "application/json");
    }
    let req = builder
        .body(match body {
            Some(text) => Body::from(text.to_string()),
            None => Body::empty(),
        })
        .unwrap();
    app.clone()
        .oneshot(req)
        .await
        .expect("router must answer")
        .status()
}

#[tokio::test(flavor = "multi_thread")]
async fn rejects_hostile_origin_and_host_in_dev_mode() {
    let _guard = config_test_lock().await;
    let state = support::test_state_custom("t86dev", None, true).await;
    let app = tylluan_kernel::transport::http::build_router(state);

    // 1-2. Origin hostile, with and without a valid JSON-RPC body (preflight-free
    // simple requests are covered by the TEXT/PLAIN variant of T-DSH-85; the guard
    // is header-based, so the body never matters).
    for uri in ["/messages", "/mcp", "/api/v1/mcp"] {
        assert_eq!(
            call(&app, "POST", uri, Some("http://evil.example"), Some("evil.example"), None, Some(RPC_LIST)).await,
            StatusCode::FORBIDDEN,
            "hostile Origin on {uri}"
        );
    }
    // 3. SSE.
    assert_eq!(
        call(&app, "GET", "/sse", Some("http://evil.example"), Some("evil.example"), None, None).await,
        StatusCode::FORBIDDEN,
        "hostile Origin on /sse"
    );
    // 4. Host hostile, no Origin.
    assert_eq!(
        call(&app, "POST", "/messages", None, Some("evil.example"), None, Some(RPC_LIST)).await,
        StatusCode::FORBIDDEN,
        "hostile Host must be rejected on a loopback-bound kernel"
    );
    // 5. Preflight OPTIONS with hostile Origin.
    assert_eq!(
        call(&app, "OPTIONS", "/messages", Some("http://evil.example"), Some("evil.example"), None, None).await,
        StatusCode::FORBIDDEN,
        "preflight must be rejected too"
    );
    // Origin: null (sandboxed page) is invalid.
    assert_eq!(
        call(&app, "POST", "/messages", Some("null"), Some("127.0.0.1:47004"), None, Some(RPC_LIST)).await,
        StatusCode::FORBIDDEN
    );
    // BUF-D4: la validacion de HOST cubre toda la API REST, tambien los GET de lectura
    // (un DNS rebinding no necesita escribir para leer la memoria del operador).
    assert_eq!(
        call(&app, "GET", "/api/v1/coloquio/channels", None, Some("evil.example"), None, None).await,
        StatusCode::FORBIDDEN,
        "BUF-D4: a hostile Host must be rejected on a read-only /api/v1 GET"
    );
    // ...pero el ORIGEN no se exige a una lectura: un GET no cambia estado y exigirlo
    // romperia a los clientes que solo leen (scope check del Origin).
    assert_eq!(
        call(&app, "GET", "/api/v1/coloquio/channels", Some("http://evil.example"), Some("127.0.0.1:47004"), None, None).await,
        StatusCode::OK,
        "the Origin guard must not cover read-only GETs of the REST API"
    );
    // Y un cliente local normal sigue entrando.
    assert_eq!(
        call(&app, "GET", "/api/v1/coloquio/channels", None, Some("127.0.0.1:47004"), None, None).await,
        StatusCode::OK,
        "a loopback client keeps reading the REST API"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn accepts_local_clients_and_loopback_origins() {
    let _guard = config_test_lock().await;
    let state = support::test_state_custom("t86devok", None, true).await;
    let app = tylluan_kernel::transport::http::build_router(state);

    // 8. Allowed (loopback) Origin: any port.
    for origin in [
        "http://127.0.0.1:47004",
        "http://localhost:5173",
        "http://[::1]:3030",
    ] {
        assert_eq!(
            call(&app, "POST", "/messages", Some(origin), Some("127.0.0.1:47004"), None, Some(RPC_LIST)).await,
            StatusCode::OK,
            "loopback origin {origin} must pass"
        );
    }
    // No Origin => non-browser client (Claude Code, mcp-remote, agents).
    assert_eq!(
        call(&app, "POST", "/messages", None, Some("127.0.0.1:47004"), None, Some(RPC_LIST)).await,
        StatusCode::OK
    );
    // No Origin and no Host header at all.
    assert_eq!(
        call(&app, "POST", "/messages", None, None, None, Some(RPC_LIST)).await,
        StatusCode::OK
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn rejects_hostile_origin_even_with_valid_bearer() {
    let _guard = config_test_lock().await;
    let token = "t86-master-token".to_string();
    let state = support::test_state_custom("t86prod", Some(token.clone()), false).await;
    let app = tylluan_kernel::transport::http::build_router(state);

    // 6. Authenticated caller with a hostile Origin still gets 403.
    assert_eq!(
        call(&app, "POST", "/messages", Some("http://evil.example"), Some("evil.example"), Some(&token), Some(RPC_LIST)).await,
        StatusCode::FORBIDDEN
    );
    // 7. Same for SSE.
    assert_eq!(
        call(&app, "GET", "/sse", Some("http://evil.example"), Some("evil.example"), Some(&token), None).await,
        StatusCode::FORBIDDEN
    );
    // Baseline: valid bearer, no hostile headers => 200.
    assert_eq!(
        call(&app, "POST", "/messages", None, Some("127.0.0.1:47004"), Some(&token), Some(RPC_LIST)).await,
        StatusCode::OK
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn configurable_allow_lists_work() {
    let _guard = config_test_lock().await;
    let state = support::test_state_custom("t86lists", None, true).await;
    let snapshot = snapshot_nexus(&state).await;
    {
        let mut config = state.config.write().await;
        config.nexus.allowed_origins = vec!["http://app.example".to_string()];
        config.nexus.allowed_hosts = vec!["tunnel.example".to_string()];
    }
    let app = tylluan_kernel::transport::http::build_router(state.clone());

    // Explicitly allowed origin + allowed host => 200.
    let allowed_both =
        call(&app, "POST", "/messages", Some("http://app.example"), Some("tunnel.example:47004"), None, Some(RPC_LIST)).await;
    // Allowed origin but hostile host => still 403.
    let hostile_host =
        call(&app, "POST", "/messages", Some("http://app.example"), Some("evil.example"), None, Some(RPC_LIST)).await;
    // Unlisted origin => 403.
    let unlisted_origin =
        call(&app, "POST", "/messages", Some("http://other.example"), Some("127.0.0.1:47004"), None, Some(RPC_LIST)).await;

    restore_nexus(&state, &snapshot).await;

    assert_eq!(allowed_both, StatusCode::OK);
    assert_eq!(hostile_host, StatusCode::FORBIDDEN);
    assert_eq!(unlisted_origin, StatusCode::FORBIDDEN);
}

// T-DSH-87: the mutating REST surface is guarded too.
#[tokio::test(flavor = "multi_thread")]
async fn rest_mutating_routes_reject_hostile_origin() {
    let _guard = config_test_lock().await;
    for dev_mode in [true, false] {
        let state = support::test_state_custom("t87rest", None, dev_mode).await;
        let app = tylluan_kernel::transport::http::build_router(state);
        for uri in [
            "/api/v1/maintenance/purge",
            "/api/v1/maintenance/vacuum",
            "/api/v1/admin/reload",
            "/api/v1/silva/graphrag-trigger",
            "/api/v1/do",
        ] {
            let status = call(&app, "POST", uri, Some("http://evil.example"), None, None, None).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{uri} must reject a hostile Origin (dev_mode={dev_mode})");
        }
        // Hostile Origin on a read-only GET is not this guard's concern.
        let get = call(&app, "GET", "/api/v1/health", Some("http://evil.example"), None, None, None).await;
        assert_ne!(get, StatusCode::FORBIDDEN, "GET stays outside the REST guard (dev_mode={dev_mode})");
        // No Origin (agents, scripts, CLI): never rejected by the guard.
        let local = call(&app, "POST", "/api/v1/maintenance/vacuum", None, None, None, None).await;
        assert_ne!(local, StatusCode::FORBIDDEN, "no Origin must pass the guard (dev_mode={dev_mode})");
        // Loopback Origin (the dashboard itself) is accepted.
        let dash = call(&app, "POST", "/api/v1/maintenance/vacuum", Some("http://127.0.0.1:47004"), None, None, None).await;
        assert_ne!(dash, StatusCode::FORBIDDEN, "loopback Origin must pass (dev_mode={dev_mode})");
    }
}

// D-147: `support::test_state_custom` comparte UNA config por proceso
// (`TylluanConfig::load_cached`), asi que mutarla en un test se la cambia a los
// demas que corren en paralelo. Todo test de este fichero toma este candado, y el
// que muta la deja como estaba antes de soltarlo.
static CONFIG_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn config_test_lock() -> tokio::sync::MutexGuard<'static, ()> {
    CONFIG_TEST_LOCK.lock().await
}

struct NexusSnapshot {
    host: String,
    allowed_hosts: Vec<String>,
    allowed_origins: Vec<String>,
    enforce: bool,
}

async fn snapshot_nexus(state: &std::sync::Arc<tylluan_kernel::transport::http::HttpState>) -> NexusSnapshot {
    let config = state.config.read().await;
    NexusSnapshot {
        host: config.nexus.host.clone(),
        allowed_hosts: config.nexus.allowed_hosts.clone(),
        allowed_origins: config.nexus.allowed_origins.clone(),
        enforce: config.nexus.enforce_host_guard_nonloopback,
    }
}

async fn restore_nexus(
    state: &std::sync::Arc<tylluan_kernel::transport::http::HttpState>,
    snapshot: &NexusSnapshot,
) {
    let mut config = state.config.write().await;
    config.nexus.host = snapshot.host.clone();
    config.nexus.allowed_hosts = snapshot.allowed_hosts.clone();
    config.nexus.allowed_origins = snapshot.allowed_origins.clone();
    config.nexus.enforce_host_guard_nonloopback = snapshot.enforce;
}

// D-147: el guard de `Host` no desaparece con un binding NO loopback: pasa a
// MODO AVISO (deja pasar, escribe el intento y lo cuenta) y solo rechaza si el
// operador activa `enforce_host_guard_nonloopback`. En loopback no cambia nada.
#[tokio::test(flavor = "multi_thread")]
async fn d147_nonloopback_binding_observes_by_default_and_enforces_on_demand() {
    let _guard = config_test_lock().await;
    let state = support::test_state_custom("d147bind", None, true).await;
    let snapshot = snapshot_nexus(&state).await;
    let app = tylluan_kernel::transport::http::build_router(state.clone());

    // (binding, enforce, Host, esperado)
    let cases: [(&str, bool, &str, StatusCode); 6] = [
        ("127.0.0.1", false, "evil.example", StatusCode::FORBIDDEN),
        ("127.0.0.1", true, "evil.example", StatusCode::FORBIDDEN),
        ("0.0.0.0", false, "evil.example", StatusCode::OK),
        ("0.0.0.0", true, "evil.example", StatusCode::FORBIDDEN),
        ("0.0.0.0", true, "tunnel.example:47004", StatusCode::OK),
        ("0.0.0.0", true, "127.0.0.1:47004", StatusCode::OK),
    ];
    let mut measured = Vec::new();
    for (binding, enforce, host, expected) in cases {
        {
            let mut config = state.config.write().await;
            config.nexus.host = binding.to_string();
            config.nexus.allowed_hosts = vec!["tunnel.example".to_string()];
            config.nexus.enforce_host_guard_nonloopback = enforce;
        }
        let got = call(&app, "GET", "/api/v1/metrics", None, Some(host), None, None).await;
        measured.push((binding, enforce, host, expected, got));
    }
    restore_nexus(&state, &snapshot).await;

    for (binding, enforce, host, expected, got) in measured {
        assert_eq!(
            got, expected,
            "binding={binding} enforce={enforce} Host={host}: el contrato D-147 cambio"
        );
    }
}

async fn body_of(app: &axum::Router, method: &str, uri: &str) -> Vec<u8> {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.expect("router must answer");
    axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("body")
        .to_vec()
}

async fn observed_host_guard_count(app: &axum::Router) -> u64 {
    let body = body_of(app, "GET", "/api/v1/metrics").await;
    let value: serde_json::Value =
        serde_json::from_slice(&body).expect("metrics tiene que ser JSON");
    value["host_guard"]["nonloopback_observed"]
        .as_u64()
        .expect("GET /api/v1/metrics expone host_guard.nonloopback_observed")
}

// D-147: el contador sube solo cuando el modo aviso deja pasar, no cuando el
// guard rechaza. Se lee del endpoint publico, no del estado interno.
#[tokio::test(flavor = "multi_thread")]
async fn d147_observe_mode_counts_only_when_it_does_not_reject() {
    let _guard = config_test_lock().await;
    let state = support::test_state_custom("d147count", None, true).await;
    let snapshot = snapshot_nexus(&state).await;
    let app = tylluan_kernel::transport::http::build_router(state.clone());
    {
        let mut config = state.config.write().await;
        config.nexus.host = "0.0.0.0".to_string();
        config.nexus.enforce_host_guard_nonloopback = false;
    }

    let before = observed_host_guard_count(&app).await;
    let mut observed = Vec::new();
    for _ in 0..3 {
        observed.push(
            call(&app, "GET", "/api/v1/metrics", None, Some("evil.example"), None, None).await,
        );
    }
    let after_observe = observed_host_guard_count(&app).await;

    // Con el endurecimiento activo no se deja pasar y no se cuenta.
    {
        let mut config = state.config.write().await;
        config.nexus.enforce_host_guard_nonloopback = true;
    }
    let mut enforced = Vec::new();
    for _ in 0..3 {
        enforced.push(
            call(&app, "GET", "/api/v1/metrics", None, Some("evil.example"), None, None).await,
        );
    }
    let after_enforce = observed_host_guard_count(&app).await;

    restore_nexus(&state, &snapshot).await;

    for status in &observed {
        assert_eq!(*status, StatusCode::OK, "modo aviso: deja pasar");
    }
    assert_eq!(
        after_observe,
        before + 3,
        "tres avisos dejan tres cuentas (antes={before}, despues={after_observe})"
    );
    for status in &enforced {
        assert_eq!(*status, StatusCode::FORBIDDEN, "con la clave activa: 403");
    }
    assert_eq!(
        after_enforce, after_observe,
        "un 403 no se cuenta como aviso"
    );
}
