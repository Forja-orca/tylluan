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
    // Non-MCP API/federation paths are NOT guarded (scope check).
    assert_eq!(
        call(&app, "GET", "/api/v1/coloquio/channels", Some("http://evil.example"), Some("evil.example"), None, None).await,
        StatusCode::OK,
        "the guard must not cover the rest of the API"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn accepts_local_clients_and_loopback_origins() {
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
    let state = support::test_state_custom("t86lists", None, true).await;
    {
        let mut config = state.config.write().await;
        config.nexus.allowed_origins = vec!["http://app.example".to_string()];
        config.nexus.allowed_hosts = vec!["tunnel.example".to_string()];
    }
    let app = tylluan_kernel::transport::http::build_router(state);

    // Explicitly allowed origin + allowed host => 200.
    assert_eq!(
        call(&app, "POST", "/messages", Some("http://app.example"), Some("tunnel.example:47004"), None, Some(RPC_LIST)).await,
        StatusCode::OK
    );
    // Allowed origin but hostile host => still 403.
    assert_eq!(
        call(&app, "POST", "/messages", Some("http://app.example"), Some("evil.example"), None, Some(RPC_LIST)).await,
        StatusCode::FORBIDDEN
    );
    // Unlisted origin => 403.
    assert_eq!(
        call(&app, "POST", "/messages", Some("http://other.example"), Some("127.0.0.1:47004"), None, Some(RPC_LIST)).await,
        StatusCode::FORBIDDEN
    );
}

// T-DSH-87: the mutating REST surface is guarded too.
#[tokio::test(flavor = "multi_thread")]
async fn rest_mutating_routes_reject_hostile_origin() {
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
