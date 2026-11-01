//! B-22 (2026-11-01): `POST /api/v1/admin/shutdown` and the rest of the
//! `admin/*` destructive surface must be (a) authenticated, (b) operator-class
//! and (c) refused when the `Host` header is not a real loopback host.
//!
//! Finding fixed alongside this test: the three admin handlers compared the
//! `Host` header with `starts_with("127.0.0.1" | "localhost" | "[::1]")`, so
//! `127.0.0.1.evil.example` and `localhost.evil.example` — hostnames an
//! attacker fully controls — passed the "only from localhost" gate. `Host` is
//! client-controlled, so that gate is defence in depth and never the boundary
//! (the master token is), but a check whose message claims localhost-only must
//! reject non-localhost names instead of accepting prefix look-alikes.
//!
//! Every "was it refused?" assertion is paired with a side-effect assertion
//! (`cancel_token` still live), because a handler that returns 403 *after*
//! doing the damage would pass a status-only test.

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use std::sync::Arc;
use tower::ServiceExt;

use tylluan_kernel::transport::http::{build_router, HttpState};

const TOKEN: &str = "b22-admin-test-token-0000";

async fn authed_state() -> Arc<HttpState> {
    support::test_state_custom("b22", Some(TOKEN.to_string()), false).await
}

/// State bound to a NON-loopback host. `build_router` disables the outer
/// MCP/SSE Host guard in exactly this configuration (its own startup warning
/// says Host validation "is only meaningful when the kernel is bound to
/// loopback"), so the handler-level loopback check is the ONLY layer left
/// answering — which is the layer B-22 is about. Probing with the default
/// loopback config would only prove the outer guard works.
async fn non_loopback_state() -> Arc<HttpState> {
    let state = support::test_state_custom("b22nl", Some(TOKEN.to_string()), false).await;
    {
        let mut cfg = state.config.write().await;
        cfg.nexus.host = "0.0.0.0".to_string();
    }
    state
}

async fn post(path: &str, host: &str, bearer: Option<&str>) -> (StatusCode, String) {
    // Non-loopback binding: the outer Origin/Host guard steps aside and the
    // handler-level check decides (see `non_loopback_state`).
    let state = non_loopback_state().await;
    let app = build_router(state.clone());
    let mut b = Request::builder()
        .method("POST")
        .uri(path)
        .header("host", host)
        .header("content-type", "application/json");
    if let Some(t) = bearer {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    let res = app
        .clone()
        .oneshot(b.body(Body::from("{}")).unwrap())
        .await
        .expect("router must not panic");
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap_or_default();
    let text = String::from_utf8_lossy(&bytes).to_string();

    // Side effect: shutdown must NOT have been initiated on any rejected path.
    if !status.is_success() {
        assert!(
            !state.cancel_token.is_cancelled(),
            "POST {path} (host={host}) was refused with {status} but STILL cancelled the shutdown token — refusal happens after the side effect"
        );
    }
    assert!(!text.contains(TOKEN), "response leaked the master token");
    (status, text)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b22_admin_shutdown_requires_credentials_and_a_real_loopback_host() {
    let path = "/api/v1/admin/shutdown";

    let (s, _) = post(path, "127.0.0.1:47200", None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "no token must be 401, got {s}");

    let (s, _) = post(path, "127.0.0.1:47200", Some("wrong-token")).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "wrong token must be 401, got {s}");

    // Authenticated but not from loopback: the handler-level localhost gate.
    let (s, body) = post(path, "evil.example", Some(TOKEN)).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "non-loopback Host must be 403, got {s}: {body}");
    assert!(
        body.contains("localhost"),
        "expected the localhost-only refusal body, got: {body}"
    );

    // Look-alike hostnames: the pre-fix `starts_with` accepted both of these.
    for spoof in ["127.0.0.1.evil.example", "localhost.evil.example", "[::1].evil.example"] {
        let (s, body) = post(path, spoof, Some(TOKEN)).await;
        assert_eq!(
            s,
            StatusCode::FORBIDDEN,
            "Host {spoof:?} is not loopback and must be 403, got {s}: {body}"
        );
    }

    // Positive control: real loopback Host with the master token does shut down.
    let state = authed_state().await;
    let app = build_router(state.clone());
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("host", "127.0.0.1:47200")
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .expect("router must not panic");
    let status = res.status();
    let body = String::from_utf8_lossy(
        &axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap_or_default(),
    )
    .to_string();
    assert!(
        status.is_success(),
        "loopback + master token must be accepted, got {status}: {body}"
    );
    assert!(
        state.cancel_token.is_cancelled(),
        "accepted shutdown must actually cancel the token"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b22_other_admin_destructive_routes_reject_spoofed_hosts() {
    // Only the handlers that ACTUALLY carry the loopback check. The other
    // four admin routes have no host check at all — reported as B22-H2 in
    // `IMPL_B-22_admin_shutdown_2026-11-01.md`, deliberately not asserted
    // here: a test that froze their current 200 would bless the gap.
    for path in [
        "/api/v1/admin/emergency-kill",
        "/api/v1/admin/kill-guild/demo",
    ] {
        let (s, _) = post(path, "127.0.0.1:47200", None).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{path}: no token must be 401, got {s}");

        let (s, _) = post(path, "127.0.0.1:47200", Some("wrong-token")).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{path}: wrong token must be 401, got {s}");

        let (s, _) = post(path, "127.0.0.1.evil.example", Some(TOKEN)).await;
        assert_eq!(
            s,
            StatusCode::FORBIDDEN,
            "{path}: spoofed loopback Host with a valid master token must be 403, got {s}"
        );
    }
}
