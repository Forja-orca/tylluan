//! WS7 adversarial security battery.
//!
//! Drives the real `api_v1_routes()` router (harness in the shared
//! `tests/support/mod.rs`, same as `repo_map_endpoint_test.rs` /
//! `audit_latency_endpoint_test.rs`) like a hostile user and asserts
//! graceful degradation everywhere: malformed JSON, oversized/deeply
//! nested payloads, injection-shaped strings, path traversal, garbage
//! auth, absurd query params, unicode/control characters, and a corrupted
//! store file.
//!
//! Invariants asserted throughout:
//!   1. No panic (a panic IS the failure — the test errors out).
//!   2. No 5xx on malformed *client input* (server-side faults may 500;
//!      hostile input must 4xx or degrade to a documented shape).
//!   3. No sensitive file content leaks through traversal-shaped paths.
//!
//! Store isolation: the only store-backed endpoint exercised is
//! `/api/v1/audit/latency`, pointed at a deliberately corrupted file via
//! the `TYLLUAN_AUDIT_DB` seam (cycle-4 lesson: ONE sequential test —
//! env vars are process-global). Write endpoints that would touch real
//! stores without a seam are exercised only where the harness's in-memory
//! stores isolate them (`/api/v1/coloquio/*` uses `ColoquioDb::new(":memory:")`
//! in `support::test_state`). Skipped for lack of a read seam:
//! `/api/v1/scheduler/confusion` (see the WS3 lib tests for its
//! path-injected coverage).

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use std::sync::Arc;
use tower::ServiceExt;

/// Authed production router: `auth_token` set, dev_mode OFF — the config
/// where bearer_auth_middleware actually enforces. Built with the REAL
/// `build_router` (pub for tests) because the auth middleware lives in the
/// production assembly, not on `api_v1_routes()` itself.
fn build_authed_app(mut state: Arc<tylluan_kernel::transport::http::HttpState>) -> axum::Router {
    let owned = Arc::get_mut(&mut state).expect("fresh state not shared yet");
    owned.auth_token = Some("battery-correct-token-123".to_string());
    owned.dev_mode = Some(false);
    tylluan_kernel::transport::http::build_router(state)
}

// ── assertion helpers ────────────────────────────────────────────────────────

async fn send(app: axum::Router, req: Request<Body>) -> (StatusCode, String) {
    let res = app.oneshot(req).await.expect("router must not panic");
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), 4 << 20).await.unwrap_or_default();
    let text = String::from_utf8_lossy(&bytes).to_string();
    (status, text)
}

fn json_req(method: &str, uri: &str, body: String) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap()
}

/// Server-side faults legitimately produce 5xx; hostile CLIENT INPUT must
/// not. This is the battery's core invariant.
fn assert_not_server_fault(status: StatusCode, ctx: &str, body: &str) {
    assert!(
        !status.is_server_error(),
        "{ctx}: hostile input produced server fault {status}; body: {body}"
    );
}

fn assert_no_leak(body: &str, needles: &[&str], ctx: &str) {
    for n in needles {
        assert!(
            !body.contains(n),
            "{ctx}: response leaked sensitive content (found {n:?}): {body}"
        );
    }
}

// ── the battery: one sequential test (env-var seam is process-global) ───────

#[tokio::test(flavor = "multi_thread")]
async fn adversarial_http_battery() {
    // The one env seam, set once at the very start: a deliberately corrupted
    // "audit DB" (binary garbage, not a SQLite file, not creatable as one).
    let hostile_db = std::env::temp_dir().join(format!(
        "tylluan_adv_hostile_audit_{}.db",
        std::process::id()
    ));
    std::fs::write(&hostile_db, b"\xDE\xAD\xBE\xEF not a sqlite file \x00\x01\x02").unwrap();
    // SAFETY (set_var): edition-2024 unsafe op; this test binary contains
    // exactly one #[test], so no concurrent env access is possible.
    unsafe { std::env::set_var("TYLLUAN_AUDIT_DB", &hostile_db) };

    let state = support::test_state("adv").await;
    let app = support::build_test_app(state);

    // ── A. Malformed / wrong-shaped bodies on a JSON write endpoint ──────
    let (st, body) = send(app.clone(), json_req("POST", "/api/v1/coloquio/channels", "{not json}}".into())).await;
    assert_not_server_fault(st, "A1 malformed json", &body);
    assert!(st.is_client_error() || st == StatusCode::OK, "A1: {st}");

    let (st, body) = send(app.clone(), Request::builder().method("POST").uri("/api/v1/coloquio/channels").body(Body::empty()).unwrap()).await;
    assert_not_server_fault(st, "A2 empty body", &body);

    let (st, body) = send(app.clone(), json_req("POST", "/api/v1/coloquio/channels", "[1,2,3]".into())).await;
    assert_not_server_fault(st, "A3 array instead of object", &body);

    // ── B. Oversized and deeply nested payloads ──────────────────────────
    let deep = format!("{}{}", "[".repeat(10_000), "]".repeat(10_000));
    let (st, body) = send(app.clone(), json_req("POST", "/api/v1/coloquio/channels", deep)).await;
    assert_not_server_fault(st, "B1 10k-deep nesting", &body);

    let big = "A".repeat(2_000_000);
    let (st, body) = send(app.clone(), json_req("POST", "/api/v1/coloquio/channels", format!(r#"{{"channel_id":"{big}","name":"x"}}"#))).await;
    assert_not_server_fault(st, "B2 2MB field", &body);

    // ── C. Injection-shaped strings in data fields (parameterized stores) ─
    let hostile_ch = "x'; DROP TABLE coloquio_messages; --";
    let (st, body) = send(app.clone(), json_req("POST", &format!("/api/v1/coloquio/channels/{}/post", uri_escape(hostile_ch)), r#"{"author_id":"adv","content":"'); DELETE FROM users; --"}"#.into())).await;
    assert_not_server_fault(st, "C1 hostile channel_id + content", &body);
    // Regression pin (WS7 finding #1): invalid channel ids are a CLIENT
    // error answered with a constant message — never a 500, never an echo
    // of the raw input.
    assert_eq!(st, StatusCode::BAD_REQUEST, "C1: {body}");
    assert_no_leak(&body, &["DROP TABLE", "DELETE FROM", "x';"], "C1 reflection");

    // The store must still be alive and serving reads afterwards.
    let (st, body) = send(app.clone(), Request::builder().method("GET").uri("/api/v1/coloquio/channels").body(Body::empty()).unwrap()).await;
    assert_eq!(st, StatusCode::OK, "C2 store alive after injection attempt: {body}");

    // ── D. Path traversal and odd encodings on file-serving path segments ─
    let cargo_marker = "[package]";
    let (st, body) = send(app.clone(), Request::builder().method("GET").uri("/api/v1/ingest/files/..%2F..%2F..%2FCargo.toml").body(Body::empty()).unwrap()).await;
    assert_not_server_fault(st, "D1 ingest traversal encoded", &body);
    assert_no_leak(&body, &[cargo_marker, "[dependencies]"], "D1");

    let (st, body) = send(app.clone(), Request::builder().method("GET").uri("/api/v1/ingest/files/..%5C..%5C..%5CCargo.toml").body(Body::empty()).unwrap()).await;
    assert_not_server_fault(st, "D2 ingest traversal backslash", &body);
    assert_no_leak(&body, &[cargo_marker], "D2");

    let (st, body) = send(app.clone(), Request::builder().method("GET").uri("/api/v1/sandbox/files/..%2F..%2F..%2FCargo.toml").body(Body::empty()).unwrap()).await;
    assert_not_server_fault(st, "D3 sandbox traversal encoded", &body);
    assert_no_leak(&body, &[cargo_marker, "[dependencies]"], "D3");

    let (st, body) = send(app.clone(), Request::builder().method("GET").uri("/api/v1/sandbox/files/%2E%2E%2F%2E%2E%2F%2E%2E%2FCargo.toml").body(Body::empty()).unwrap()).await;
    assert_not_server_fault(st, "D4 sandbox traversal dot-encoding", &body);
    assert_no_leak(&body, &[cargo_marker], "D4");

    let (st, body) = send(app.clone(), Request::builder().method("GET").uri("/api/v1/sandbox/files/C:%5CCargo.toml").body(Body::empty()).unwrap()).await;
    assert_not_server_fault(st, "D5 sandbox absolute windows path", &body);
    assert_no_leak(&body, &[cargo_marker], "D5");

    // ── E. Absurd query parameters ────────────────────────────────────────
    for (ctx, uri) in [
        ("E1 negative window", "/api/v1/audit/latency?window_minutes=-5"),
        ("E2 huge window", "/api/v1/audit/latency?window_minutes=999999999999"),
        ("E3 non-numeric window", "/api/v1/audit/latency?window_minutes=abc"),
        ("E4 empty window", "/api/v1/audit/latency?window_minutes="),
    ] {
        let (st, body) = send(app.clone(), Request::builder().method("GET").uri(uri).body(Body::empty()).unwrap()).await;
        assert_not_server_fault(st, ctx, &body);
        // Two valid degradations: axum rejects unparseable params with 400
        // before the handler (E3/E4), and the handler itself answers 200 +
        // available:false on the hostile DB (E1/E2). Both are graceful —
        // the only forbidden outcome is a 5xx/panic.
        let v: serde_json::Value = serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
        let handler_degraded = st == StatusCode::OK && v["available"] == false;
        let extractor_rejected = st == StatusCode::BAD_REQUEST;
        assert!(handler_degraded || extractor_rejected, "{ctx}: neither degradation shape, got {st}: {body}");
    }

    let (st, body) = send(app.clone(), Request::builder().method("GET").uri("/api/v1/coloquio/channels/general?limit=-1&offset=99999999999999").body(Body::empty()).unwrap()).await;
    assert_not_server_fault(st, "E5 absurd limit/offset", &body);

    // ── F. Missing / garbled auth headers (REAL middleware, authed state) ─
    // Drivers: `build_router` (production assembly, bearer middleware ON,
    // auth_token set, dev_mode off). A wrong token must be rejected 401
    // without leaking signals data; a correct token must pass. (Driving
    // api_v1_routes() directly would bypass auth by construction — that
    // harness artifact is documented in the module header.)
    let authed_app = build_authed_app(support::test_state("adv").await);
    for (ctx, header_val) in [
        ("F1 bearer sql-shaped", "Bearer '; DROP TABLE users; --"),
        ("F2 wrong scheme", "Basic YWRtaW46YWRtaW4="),
        ("F3 empty bearer", "Bearer "),
        // NB: tokens with raw control chars (\x00, \x07, \x1F) cannot be
        // sent at all — the http crate rejects them pre-flight (observed as
        // a transport-layer defense, reported in Coloquio). F4 uses the
        // worst header-LEGAL garbage so the kernel's comparison path runs.
        ("F4 garbage token", "Bearer !@#$%^&*()_+={}[]|;:,.?<>/\\~`"),
        ("F5 near-miss token", "Bearer battery-correct-token-124"),
    ] {
        let (st, body) = send(authed_app.clone(), Request::builder().method("GET").uri("/api/v1/system/signals").header("authorization", header_val).body(Body::empty()).unwrap()).await;
        assert_not_server_fault(st, ctx, &body);
        assert_eq!(st, StatusCode::UNAUTHORIZED, "{ctx}: expected 401 for garbage auth, got {st}; body: {body}");
        assert_no_leak(&body, &["active_guilds", "node_count"], ctx);
    }
    // Control: the correct token passes and serves real data.
    let (st, body) = send(authed_app.clone(), Request::builder().method("GET").uri("/api/v1/system/signals").header("authorization", "Bearer battery-correct-token-123").body(Body::empty()).unwrap()).await;
    assert_eq!(st, StatusCode::OK, "F-control valid token must pass: {body}");
    assert!(body.contains("active_guilds"), "F-control: {body}");

    // ── G. Wrong HTTP method on a GET route ───────────────────────────────
    let (st, body) = send(app.clone(), Request::builder().method("DELETE").uri("/api/v1/system/signals").body(Body::empty()).unwrap()).await;
    assert_not_server_fault(st, "G1 wrong method", &body);
    assert_eq!(st, StatusCode::METHOD_NOT_ALLOWED, "G1: {body}");

    // ── H. Unicode, control characters, overlong sequences ────────────────
    // Bidi chars built at runtime (rustc denies them in literals — the
    // compiler itself guards this class, which is a good sign for the kernel).
    let (rtl_open, rtl_close) = (
        char::from_u32(0x202B).unwrap_or(' '),
        char::from_u32(0x202C).unwrap_or(' '),
    );
    // \u{0}/\u{7} are real control chars (literal-level escapes, fine in format!).
    let weird = format!("héllo 🌮 世界 {rtl_open}override{rtl_close} \u{0}\u{7}");
    let post_body = serde_json::json!({ "author_id": "adv-ünïcode", "content": weird }).to_string();
    let (st, body) = send(app.clone(), json_req("POST", "/api/v1/coloquio/channels/uni-test/post", post_body)).await;
    assert_not_server_fault(st, "H1 unicode+control content", &body);

    let (st, body) = send(app.clone(), Request::builder().method("GET").uri("/api/v1/coloquio/channels/uni-test?reader=adv").body(Body::empty()).unwrap()).await;
    assert_not_server_fault(st, "H2 read back after hostile content", &body);
    assert_eq!(st, StatusCode::OK, "H2: {body}");

    // ── I. Unknown routes and unknown channel ids ─────────────────────────
    let (st, body) = send(app.clone(), Request::builder().method("GET").uri("/api/v1/definitely/not/a/route").body(Body::empty()).unwrap()).await;
    assert_not_server_fault(st, "I1 unknown route", &body);
    assert_eq!(st, StatusCode::NOT_FOUND, "I1: {body}");

    let (st, body) = send(app.clone(), Request::builder().method("GET").uri("/api/v1/coloquio/channels/..%2F..%2Fsecret?reader=adv").body(Body::empty()).unwrap()).await;
    assert_not_server_fault(st, "I2 traversal-shaped channel id", &body);
    assert_no_leak(&body, &["[package]"], "I2");

    let _ = std::fs::remove_file(&hostile_db);
}

fn uri_escape(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}
