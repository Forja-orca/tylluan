//! Integration test: mailbox broadcast protocol.

use tylluan_kernel::memory::mailbox::Mailbox;
use std::sync::Arc;

#[tokio::test(flavor = "multi_thread")]
async fn test_broadcast_sends_to_known_agents() {
    let mb = Arc::new(Mailbox::in_memory().await.unwrap());

    // Seed: agente-b has sent a message before (so broadcast finds it in history)
    mb.send_mail("agente-b", "agente-a", r#"{"type":"hello"}"#).await.unwrap();

    let sent = mb.broadcast("agente-a", "knowledge_share", "Rust uses ownership for memory safety", &[]).await.unwrap();
    assert!(sent > 0, "broadcast should deliver to at least one agent");

    let msgs = mb.get_recent_broadcasts("agente-b", "knowledge_share", 1).await.unwrap();
    assert!(!msgs.is_empty(), "agente-b should have received the broadcast");
    let payload: serde_json::Value = serde_json::from_str(&msgs[0].payload).unwrap();
    assert_eq!(payload["type"].as_str(), Some("knowledge_share"));
    assert_eq!(payload["from"].as_str(), Some("agente-a"));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_broadcast_reaches_known_agents_without_history() {
    let mb = Mailbox::in_memory().await.unwrap();

    // No prior mail history — but we pass known_agents explicitly
    let known = vec!["agente-b".to_string(), "agente-c".to_string()];
    let sent = mb.broadcast("agente-a", "knowledge_share", "test content", &known).await.unwrap();
    assert_eq!(sent, 2, "should reach both known agents even with no mail history");

    let msgs_b = mb.get_recent_broadcasts("agente-b", "knowledge_share", 1).await.unwrap();
    assert!(!msgs_b.is_empty(), "agente-b should receive broadcast");
    let msgs_c = mb.get_recent_broadcasts("agente-c", "knowledge_share", 1).await.unwrap();
    assert!(!msgs_c.is_empty(), "agente-c should receive broadcast");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_broadcast_does_not_send_to_sender() {
    let mb = Arc::new(Mailbox::in_memory().await.unwrap());

    mb.send_mail("agente-b", "agente-a", "seed").await.unwrap();
    mb.broadcast("agente-a", "knowledge_share", "test content", &[]).await.unwrap();

    let self_msgs = mb.get_recent_broadcasts("agente-a", "knowledge_share", 1).await.unwrap();
    assert!(self_msgs.is_empty(), "sender should not receive its own broadcast");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_broadcast_count_last_hours() {
    let mb = Mailbox::in_memory().await.unwrap();
    let known = vec!["agente-b".to_string()];

    mb.broadcast("agente-a", "knowledge_share", "knowledge 1", &known).await.unwrap();
    mb.broadcast("agente-a", "knowledge_share", "knowledge 2", &known).await.unwrap();

    let count = mb.broadcast_count_last_hours(1).await.unwrap();
    assert_eq!(count, 2, "Should count 2 broadcasts in last hour");
}
