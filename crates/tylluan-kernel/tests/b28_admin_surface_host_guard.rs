//! B-28 (2026-11-01): the rest of the `admin/*` surface must answer loopback
//! `Host` only. B-22 fixed the loopback *comparison* for shutdown /
//! emergency-kill / kill-guild; these four handlers had no check at all:
//!
//! - `POST /api/v1/admin/meta-prune` (archives cognitive metadata: a write)
//! - `POST /api/v1/admin/reload` (kills and restarts guilds)
//! - `GET  /api/v1/admin/audit` (reveals the operator's audit trail)
//! - `GET  /api/v1/admin/audit/file` (returns audit log contents)
//!
//! Every test here fails on the pre-fix code: the handlers returned 200 for
//! `Host: evil.example`, because `Host` was never looked at. The master token
//! remains the real boundary (all requests below carry it) — this guard is
//! defence in depth, but a route that claims admin-only must not answer a
//! hostname the attacker controls.
//!
//! `reload` is additionally checked with `dev_mode=false`: the guard must run
//! *before* the dev_mode gate, otherwise a foreign Host can tell whether
//! dev_mode is on from the error text alone.

mod support;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use std::sync::Arc;
use tower::ServiceExt;

use tylluan_kernel::transport::http::{build_router, HttpState};

const TOKEN: &str = "b28-admin-test-token-0000";
const LOOPBACK_HOST: &str = "127.0.0.1:47200";
const HOSTILE_HOSTS: [&str; 4] = [
    "evil.example",
    "127.0.0.1.evil.example",
    "localhost.evil.example",
    "[::1].evil.example",
];

/// State bound to a NON-loopback address: `build_router` steps aside from the
/// outer MCP/SSE Host guard in exactly this configuration, so the handler-level
/// check is the only layer left answering. Probing with the default loopback
/// binding would only prove the outer guard works.
async fn state(dev_mode: bool) -> Arc<HttpState> {
    let state = support::test_state_custom("b28", Some(TOKEN.to_string()), dev_mode).await;
    {
        let mut cfg = state.config.write().await;
        cfg.nexus.host = "0.0.0.0".to_string();
    }
    state
}

async fn call(state: &Arc<HttpState>, method: Method, path: &str, host: &str) -> (StatusCode, String) {
    let app = build_router(state.clone());
    let res = app
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("host", host)
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .expect("router must not panic");
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap_or_default();
    let text = String::from_utf8_lossy(&bytes).to_string();
    assert!(!text.contains(TOKEN), "response leaked the master token");
    (status, text)
}

fn assert_refused(name: &str, host: &str, status: StatusCode, body: &str) {
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "{name} with Host {host:?} must be 403, got {status}: {body}"
    );
    assert!(
        body.contains("localhost"),
        "{name} with Host {host:?}: expected the localhost-only body, got {body}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b28_meta_prune_is_loopback_only() {
    let st = state(false).await;
    for host in HOSTILE_HOSTS {
        let (s, b) = call(&st, Method::POST, "/api/v1/admin/meta-prune", host).await;
        assert_refused("meta-prune", host, s, &b);
        assert!(
            !b.contains("\"archived\""),
            "refused meta-prune still reported an archive count: {b}"
        );
    }
    // Positive control: loopback Host reaches the handler.
    let (s, b) = call(&st, Method::POST, "/api/v1/admin/meta-prune", LOOPBACK_HOST).await;
    assert!(
        !b.contains("localhost-only"),
        "loopback Host must not be refused, got {s}: {b}"
    );
    assert!(
        b.contains("\"archived\"") || b.contains("error"),
        "loopback meta-prune should reach the handler body, got {s}: {b}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b28_reload_guard_wins_over_the_dev_mode_gate() {
    // dev_mode = false: pre-fix this returned 403 with "only available in
    // dev_mode", which told a foreign caller the mode. The loopback guard must
    // answer first.
    let st = state(false).await;
    for host in HOSTILE_HOSTS {
        let (s, b) = call(&st, Method::POST, "/api/v1/admin/reload", host).await;
        assert_refused("reload", host, s, &b);
        assert!(
            !b.contains("dev_mode"),
            "reload with Host {host:?} leaked the dev_mode gate: {b}"
        );
    }
    // dev_mode = true and a hostile Host: still refused.
    let st_dev = state(true).await;
    let (s, b) = call(&st_dev, Method::POST, "/api/v1/admin/reload", "evil.example").await;
    assert_refused("reload(dev_mode=true)", "evil.example", s, &b);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b28_audit_list_and_file_are_loopback_only() {
    let st = state(false).await;
    for host in HOSTILE_HOSTS {
        let (s, b) = call(&st, Method::GET, "/api/v1/admin/audit", host).await;
        assert_refused("audit(list)", host, s, &b);
        assert!(
            !b.contains("\"files\""),
            "refused audit list still returned the file listing: {b}"
        );

        let (s, b) = call(
            &st,
            Method::GET,
            "/api/v1/admin/audit/file?file=audit.log",
            host,
        )
        .await;
        assert_refused("audit(file)", host, s, &b);
        assert!(
            !b.contains("audit.log"),
            "refused audit file read still mentioned the file name: {b}"
        );
    }

    // Positive control: loopback reaches the listing (200, possibly empty).
    let (s, b) = call(&st, Method::GET, "/api/v1/admin/audit", LOOPBACK_HOST).await;
    assert_eq!(
        s,
        StatusCode::OK,
        "loopback audit list must be served, got {s}: {b}"
    );
    assert!(
        b.contains("\"enabled\""),
        "loopback audit list should be the real listing body, got {b}"
    );
}
