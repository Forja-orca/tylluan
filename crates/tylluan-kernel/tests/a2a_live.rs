//! Live integration test: Tylluan's outbound A2A client against an agent
//! served by the OFFICIAL `a2a-sdk` (Linux Foundation): `tools/a2a_echo_agent.py`.
//!
//! Run:
//!   1. `python tools/a2a_echo_agent.py`   (serves http://127.0.0.1:8901)
//!   2. `cargo test -p tylluan-kernel --test a2a_live -- --ignored`
//!
//! This is the interop gate for F1: if the client's wire shape (parts[].kind,
//! status.state, card discovery) is wrong, the official SDK rejects it.

use std::time::Duration;

use tylluan_kernel::transport::http::a2a_client::{A2aClient, ExternalAgent, RemoteTaskState};

const DEFAULT_URL: &str = "http://127.0.0.1:8901";

#[tokio::test]
#[ignore = "requires the official a2a-sdk echo agent running locally"]
async fn a2a_client_against_official_sdk() {
    let url = std::env::var("A2A_ECHO_URL").unwrap_or_else(|_| DEFAULT_URL.to_string());
    let agent = ExternalAgent::new("sdk-echo", &url, "");
    let client = A2aClient::new().expect("client builds");

    // 1. Card discovery against the SDK's well-known endpoint.
    let card = client.fetch_card(&agent).await.expect("card fetch");
    assert_eq!(card.name, "sdk-echo", "official SDK card name");
    assert!(
        card.protocol_version.is_empty() || card.protocol_version.starts_with("0.3"),
        "expected 0.3.x protocolVersion, got '{}'",
        card.protocol_version
    );
    assert!(!card.skills.is_empty(), "SDK card must advertise skills");

    let endpoint =
        A2aClient::resolve_endpoint(&card, &agent.url).expect("valid card endpoint");
    assert!(endpoint.ends_with("/a2a"), "SDK mounts JSON-RPC at /a2a: {endpoint}");

    // 2. message/send + tasks/get polling until terminal (echo completes instantly).
    let task = client
        .run_task(&agent, &endpoint, "hola desde tylluan", Duration::from_secs(5))
        .await
        .expect("run_task completes against official SDK");
    assert_eq!(task.resolved_state(), RemoteTaskState::Completed);
    let text = A2aClient::task_text(&task);
    assert!(
        text.contains("hola desde tylluan"),
        "expected echo text, got: {text:?}"
    );
}