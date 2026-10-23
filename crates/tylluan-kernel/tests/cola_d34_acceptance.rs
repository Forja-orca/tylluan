//! D-34 -- acceptance test: a guild receives an agent-class credential, not
//! the master token. That credential reaches `silva/node` (agent class), gets
//! 403 on the WHOLE operator class except the audited exception
//! `POST /api/v1/silva/edge`, and the master Bearer keeps working unchanged.
//!
//! TL decision (turn 1588 / queue 85861c6), option A:
//!  (1) the exception is only `POST /api/v1/silva/edge` and only for
//!      `AclAuthKind::GuildToken`;
//!  (2) every use leaves a row in `guild_audit_log` with the guild and the
//!      truncated subject/object pair;
//!  (3) live sweep of the operator class;
//!  (4) credential per process, never on disk and never logged.
//!
//! Run:
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test cola_d34_acceptance
//!
//! No external network, no fixed ports, no `cfg(unix)`/`cfg(windows)`; this
//! never starts the production kernel and never touches `.tylluan-token`.

mod support;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;

use tylluan_kernel::security::guild_credential;
use tylluan_kernel::transport::http::build_router;

/// The master token the harness accepts (not the real one, never written).
const MASTER: &str = "master-test-token";
const GUILD: &str = "d34-acceptance-guild";

/// Router with auth ENABLED (dev_mode=false) plus a freshly issued credential.
async fn app_with_auth(tag: &str) -> (axum::Router, String) {
    let state = support::test_state_custom(tag, Some(MASTER.to_string()), false).await;
    let app = build_router(state);
    let guild_token = guild_credential::issue(GUILD);
    (app, guild_token)
}

async fn call(
    app: &axum::Router,
    method: &str,
    uri: &str,
    bearer: &str,
    body: Option<&str>,
) -> (StatusCode, String) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"));
    if body.is_some() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    let req = builder
        .body(match body {
            Some(b) => Body::from(b.to_string()),
            None => Body::empty(),
        })
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

/// Every operator-class surface the table covers, with a method that would be
/// dangerous if the gate leaked. A guild credential must get 403 on all of
/// them. (`POST /api/v1/silva/edge` is deliberately absent: it is the one
/// audited exception, covered by its own test below.)
const OPERATOR_SWEEP: &[(&str, &str)] = &[
    ("GET", "/api/v1/config"),
    ("POST", "/api/v1/config"),
    ("POST", "/api/v1/config/device"),
    ("POST", "/api/v1/admin/shutdown"),
    ("POST", "/api/v1/admin/reload"),
    ("POST", "/api/v1/maintenance/purge"),
    ("POST", "/api/v1/bash"),
    ("POST", "/api/v1/guilds/llama_backend/start"),
    ("POST", "/api/v1/guilds/llama_backend/stop"),
    ("POST", "/api/v1/guilds/llama_backend/reset-backoff"),
    ("POST", "/api/v1/guilds/dispatch/execute"),
    ("POST", "/api/v1/dispatches/x/approve"),
    ("POST", "/api/v1/memory/quarantine"),
    ("DELETE", "/api/v1/silva/nodes/n1"),
    ("POST", "/api/v1/silva/consolidate"),
    ("GET", "/api/v1/decide/shadow"),
    ("GET", "/api/v1/silva/conflicted"),
    ("POST", "/api/v1/mcp/external/discover"),
    ("PUT", "/api/v1/federation/peers/p/permissions"),
    ("POST", "/api/v1/federation/sync"),
    ("POST", "/api/v1/doctor/repair"),
    ("POST", "/api/v1/guilds/register"),
];

/// Criterion 1 of the decision: agent-class routes keep working.
#[tokio::test(flavor = "multi_thread")]
async fn d34_guild_credential_reaches_silva_node() {
    let (app, token) = app_with_auth("d34_node").await;
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/silva/node",
        &token,
        Some(r#"{"content":"D-34 acceptance node","node_type":"entity"}"#),
    )
    .await;
    assert!(
        status.is_success(),
        "a guild credential must reach silva/node (agent class), got {status}: {body}"
    );
}

/// Criterion 2 + the TL sweep requirement: 403 on the whole operator class.
#[tokio::test(flavor = "multi_thread")]
async fn d34_guild_credential_is_403_on_every_operator_route() {
    let (app, token) = app_with_auth("d34_sweep").await;
    let mut leaked = Vec::new();
    for (method, uri) in OPERATOR_SWEEP {
        let (status, body) = call(&app, method, uri, &token, None).await;
        if status != StatusCode::FORBIDDEN {
            leaked.push(format!("{method} {uri} -> {status}: {body}"));
        }
    }
    assert!(
        leaked.is_empty(),
        "D-34: the guild credential crossed into the operator class: {leaked:#?}"
    );
}

/// The exception exists, and only for the guild class: the master token keeps
/// its full authority (Jose: additive; a master client must keep working).
#[tokio::test(flavor = "multi_thread")]
async fn d34_master_token_still_passes_operator_gate() {
    let (app, _token) = app_with_auth("d34_master").await;
    for (method, uri) in [
        ("GET", "/api/v1/config"),
        ("GET", "/api/v1/silva/conflicted"),
    ] {
        let (status, body) = call(&app, method, uri, MASTER, None).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "the master Bearer must still pass {method} {uri}: {status} {body}"
        );
    }
}

/// A token that only *looks* like a guild credential authenticates nothing.
#[tokio::test(flavor = "multi_thread")]
async fn d34_unknown_guild_token_is_401() {
    let (app, _token) = app_with_auth("d34_unknown").await;
    let fake = format!("tyg_{}", "A".repeat(43));
    assert!(
        guild_credential::authenticate(&fake).is_none(),
        "a token that was never issued must not resolve to any guild"
    );
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/silva/node",
        &fake,
        Some(r#"{"content":"x"}"#),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "an invented token must be 401"
    );
}

/// Criterion 3: the one audited exception works AND leaves the audit row with
/// the issuing guild and a truncated `subject -> object` preview.
#[tokio::test(flavor = "multi_thread")]
async fn d34_guild_edge_exception_is_audited() {
    let audit_path = std::env::temp_dir().join(format!(
        "d34-audit-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&audit_path);
    // SAFETY: process-global variable of THIS test binary; no other test in
    // this file writes to the audit store.
    unsafe {
        std::env::set_var("TYLLUAN_AUDIT_DB", &audit_path);
    }

    let (app, token) = app_with_auth("d34_edge").await;
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/silva/edge",
        &token,
        Some(
            r#"{"source":"d34-subject-node","target":"d34-object-node","edge_type":"d34_accepts","metadata":"{}"}"#,
        ),
    )
    .await;
    assert_ne!(
        status,
        StatusCode::FORBIDDEN,
        "the audited exception must let the guild through on silva/edge: {status} {body}"
    );

    // The audit write is fire-and-forget (spawn_blocking): give it a moment.
    let mut row: Option<(String, String, String, String)> = None;
    for _ in 0..40 {
        if let Ok(conn) = rusqlite::Connection::open(&audit_path)
            && let Ok(mut stmt) = conn.prepare(
                "SELECT guild, agent_id, intent, result_preview FROM guild_audit_log \
                 ORDER BY id DESC LIMIT 1",
            )
            && let Ok(mut rows) = stmt.query([])
            && let Ok(Some(r)) = rows.next()
        {
            row = Some((
                r.get(0).unwrap_or_default(),
                r.get(1).unwrap_or_default(),
                r.get(2).unwrap_or_default(),
                r.get(3).unwrap_or_default(),
            ));
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    let (guild, agent_id, intent, preview) =
        row.expect("D-34: the silva/edge exception left no row in guild_audit_log");
    assert_eq!(
        guild, GUILD,
        "the row must name the guild that issued the token"
    );
    assert_eq!(agent_id, format!("guild:{GUILD}"), "canonical agent_id");
    assert!(
        intent.contains("D-34"),
        "the intent must identify the D-34 exception: {intent}"
    );
    assert_eq!(
        preview, "d34-subject-node -[d34_accepts]-> d34-object-node",
        "truncated subject/predicate/object preview"
    );

    let _ = std::fs::remove_file(&audit_path);
}
