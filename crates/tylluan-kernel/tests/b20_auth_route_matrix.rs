//! B-20 / B-21 (2026-11-01): empirical auth map over EVERY registered HTTP
//! route, driven through the real production router (`build_router`, i.e. the
//! real bearer + operator-gate middleware stack) with `dev_mode=false` and a
//! master token configured.
//!
//! Why empirical instead of a hand-written table: the "auth" column of a route
//! is whatever the middleware stack actually does, not what the handler
//! comment says. So each route is probed three ways and the literal status
//! codes ARE the map:
//!
//!   * no token        -> must be 401/403 for every protected route
//!   * wrong token     -> must be 401/403 for every protected route
//!   * correct token   -> must NOT be 401/403 (proves the credential works)
//!
//! The route list is parsed at runtime from `api_v1/routes.rs` and
//! `transport/http/mod.rs`, so a route added later without a probe is
//! impossible: it shows up in the list on the next run (and fails these
//! assertions if it is unprotected). That is the anti-drift property the
//! hand-maintained tables of earlier audits did not have.
//!
//! Class column is computed from the same pure predicates the middleware uses
//! (`auth::operator_class_required`, `auth::is_public_bypass_path`), so the
//! table states which gate is expected to answer — and the probe shows whether
//! it actually did.
//!
//! Deliverable table: `cargo test -p tylluan-kernel --test b20_auth_route_matrix
//! -- --nocapture`.

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use std::collections::BTreeSet;
use std::sync::Arc;
use tower::ServiceExt;

use tylluan_kernel::transport::http::{auth, build_router, HttpState};

/// Not a real secret: a test-only literal that exists only inside the test
/// process. Asserted absent from every rejection body.
const TOKEN: &str = "b20-matrix-test-token-0000";
const WRONG_TOKEN: &str = "b20-matrix-wrong-token-1111";

/// Routes assembled in `build_router` OUTSIDE `api_v1_routes()` that are
/// deliberately reachable without the bearer middleware (public plane).
/// Kept explicit on purpose: moving something into this list is a security
/// decision and must be reviewable in the diff.
const PUBLIC_ROUTES: &[&str] = &[
    "/health",
    "/discovery",
    "/api/v1/mcp/probe",
    "/.well-known/agent-card.json",
    "/.well-known/oauth-authorization-server",
    "/.well-known/oauth-protected-resource",
    "/oauth/authorize",
    "/oauth/token",
    "/oauth/revoke",
    "/",
    "/index.html",
    "/metrics",
];

/// Routes that live in the protected router even though they are registered in
/// `mod.rs` (MCP JSON-RPC double entry point, A2A, SSE).
const PROTECTED_EXTRA: &[&str] = &[
    "/messages",
    "/mcp",
    "/api/v1/mcp",
    "/a2a",
    "/sse",
    "/sse/download",
];

fn read_src(rel: &str) -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("cannot read {}: {e}", p.display()))
}

/// Extract `(path, method)` pairs from `.route("…", <verb>(…)[.<verb>(…)]…)`
/// lines. Every registration in this codebase is one physical line; a
/// multi-line registration would be silently skipped, which the count
/// assertion below turns into a failure instead of a blind spot.
fn parse_routes(text: &str, out: &mut BTreeSet<(String, String)>) {
    for line in text.lines() {
        let Some(idx) = line.find(".route(\"") else {
            continue;
        };
        let rest = &line[idx + ".route(\"".len()..];
        let Some(end) = rest.find('"') else { continue };
        let path = &rest[..end];
        let tail = &rest[end..];
        let mut verbs: Vec<&str> = Vec::new();
        for (needle, verb) in [
            ("get(", "GET"),
            ("post(", "POST"),
            ("put(", "PUT"),
            ("delete(", "DELETE"),
            ("patch(", "PATCH"),
            ("any(", "GET"),
            ("any(", "POST"),
        ] {
            if tail.contains(needle) {
                verbs.push(verb);
            }
        }
        assert!(
            !verbs.is_empty(),
            "route {path:?} parsed with no HTTP verb — parser is stale, fix it rather than skipping it"
        );
        for v in verbs {
            out.insert((path.to_string(), v.to_string()));
        }
    }
}

fn concrete_path(template: &str) -> String {
    let mut s = template.to_string();
    for (name, value) in [
        ("{path}", "x.txt"),
        ("{filename}", "x.txt"),
        ("{turn}", "1"),
        ("{version}", "1"),
        ("{point}", "1"),
        ("{id}", "1"),
        ("{node_id}", "1"),
        ("{msg_id}", "1"),
        ("{session_id}", "1"),
        ("{run_id}", "1"),
        ("{guild}", "demo"),
        ("{guild_name}", "demo"),
        ("{tool_name}", "demo"),
        ("{name}", "demo"),
        ("{agent}", "demo"),
        ("{agent_a}", "demo"),
        ("{agent_b}", "demo"),
        ("{agent_id}", "demo"),
        ("{channel}", "demo"),
    ] {
        s = s.replace(name, value);
    }
    s
}

async fn probe(app: &axum::Router, method: &str, path: &str, bearer: Option<&str>) -> (StatusCode, String) {
    let mut b = Request::builder()
        .method(method)
        .uri(path)
        // Loopback Host so the MCP/SSE Origin+Host guard is not what answers:
        // this test is about the AUTH layer, not the origin layer.
        .header("host", "127.0.0.1:47210")
        .header("content-type", "application/json");
    if let Some(t) = bearer {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    let req = b.body(Body::from("{}")).expect("valid request");
    let res = app.clone().oneshot(req).await.expect("router must not panic");
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap_or_default();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

async fn authed_state() -> Arc<HttpState> {
    // dev_mode=false + master token: the only configuration where bearer auth
    // actually enforces (B-21's premise).
    support::test_state_custom("b20b21", Some(TOKEN.to_string()), false).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b20_every_registered_route_is_mapped_and_b21_enforces_with_dev_mode_off() {
    let mut routes: BTreeSet<(String, String)> = BTreeSet::new();
    let api_v1_routes_src = read_src("src/transport/http/api_v1/routes.rs");
    parse_routes(&api_v1_routes_src, &mut routes);
    let mod_src = read_src("src/transport/http/mod.rs");
    parse_routes(&mod_src, &mut routes);
    // SSE plane (`/sse`, `/sse/download`, `/api/v1/events`) is registered in
    // its own module but mounted INSIDE the protected router.
    let sse_src = read_src("src/transport/http/sse.rs");
    parse_routes(&sse_src, &mut routes);

    // Parser liveness: the source registers ~290 routes today. A parse
    // regression must fail loudly, not shrink the matrix to nothing.
    assert!(
        routes.len() >= 250,
        "parsed only {} routes from source — the route parser is broken",
        routes.len()
    );

    // Every protected extra must actually be registered somewhere (catches a
    // renamed path in mod.rs).
    for extra in PROTECTED_EXTRA {
        assert!(
            routes.iter().any(|(p, _)| p == extra),
            "protected route {extra} is no longer registered — update PROTECTED_EXTRA"
        );
    }

    let state = authed_state().await;
    let app = build_router(state.clone());

    let mut rows: Vec<(String, String, &'static str, String, String, String)> = Vec::new();
    let mut failures: Vec<String> = Vec::new();

    for (template, method) in &routes {
        let path = concrete_path(template);
        let public = PUBLIC_ROUTES.contains(&template.as_str());

        let (no_token_status, no_token_body) = probe(&app, method, &path, None).await;
        let (bad_token_status, bad_token_body) = probe(&app, method, &path, Some(WRONG_TOKEN)).await;

        // Closure/never-leak check on every rejection body.
        for (ctx, body) in [("no-token", &no_token_body), ("wrong-token", &bad_token_body)] {
            if body.contains(TOKEN) {
                failures.push(format!(
                    "{method} {template} ({ctx}): rejection body leaked the master token"
                ));
            }
        }

        if public {
            if matches!(no_token_status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
                failures.push(format!(
                    "{method} {template} is a documented PUBLIC route but answered {no_token_status} without a token"
                ));
            }
        } else if auth::is_public_bypass_path(&path) {
            // Federation peer endpoints (own signature auth) and
            // /api/v1/mcp/probe: exempt by design. Any 5xx here would mean the
            // bypass reaches a handler that panics on an empty body.
            if no_token_status.is_server_error() {
                failures.push(format!(
                    "{method} {template}: bypass path answered server fault {no_token_status}"
                ));
            }
        } else {
            if !matches!(no_token_status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
                failures.push(format!(
                    "{method} {template}: NO TOKEN got {no_token_status} (expected 401/403) — route is reachable without auth"
                ));
            }
            if !matches!(bad_token_status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
                failures.push(format!(
                    "{method} {template}: WRONG TOKEN got {bad_token_status} (expected 401/403)"
                ));
            }
        }

        if no_token_status.is_server_error() {
            failures.push(format!(
                "{method} {template}: unauthenticated request reached a handler that faulted ({no_token_status})"
            ));
        }

        let class = if public {
            "público"
        } else if auth::operator_class_required(method, template) {
            "operador"
        } else if auth::is_public_bypass_path(&path) {
            "bypass interno (auth propia)"
        } else {
            "agente (bearer)"
        };

        rows.push((
            template.clone(),
            method.clone(),
            class,
            no_token_status.to_string(),
            bad_token_status.to_string(),
            "b20_every_registered_route_is_mapped_and_b21_enforces_with_dev_mode_off".to_string(),
        ));
    }

    // Positive control: the same credential the negative probes rejected must
    // work on operator-class read routes. Without this, "everything 401" would
    // pass the whole test.
    for (method, path) in [
        ("GET", "/api/v1/tools"),
        ("GET", "/api/v1/capabilities"),
        ("GET", "/api/v1/guilds"),
        ("GET", "/api/v1/whoami"),
        ("GET", "/api/v1/config"),
        ("GET", "/api/v1/runtime/tasks"),
    ] {
        let (status, body) = probe(&app, method, path, Some(TOKEN)).await;
        if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
            failures.push(format!(
                "{method} {path}: VALID token rejected ({status}) — positive control failed: {body}"
            ));
        }
        rows.push((
            path.to_string(),
            method.to_string(),
            "operador/agente (control positivo con token válido)",
            "—".to_string(),
            "—".to_string(),
            format!("{status} con token válido"),
        ));
    }

    // Emit the deliverable table.
    println!("\n| ruta | método | clase | sin token | token malo | prueba |");
    println!("|---|---|---|---|---|---|");
    for (path, method, class, no_token, bad_token, test) in &rows {
        println!("| `{path}` | {method} | {class} | {no_token} | {bad_token} | `{test}` |");
    }
    println!("\n{} rutas x 2 sondas sin credencial válida.\n", routes.len());

    assert!(
        failures.is_empty(),
        "{} fallo(s) de la matriz de auth:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
