use tylluan_kernel::memory::mailbox::{Mailbox, BlackboardMessage};
use serde_json::json;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test(flavor = "multi_thread")]
async fn test_mailbox_protocol_full_cycle() {
    let mb = Mailbox::in_memory().await.expect("Failed to create in-memory mailbox");
    
    let sender = "agent-alpha";
    let receiver = "agent-beta";
    let payload = json!({
        "type": "agent_message",
        "from": sender,
        "to": receiver,
        "subject": "Mission Briefing",
        "body": "Secure the perimeter.",
        "ttl_secs": 2
    }).to_string();

    // 1. Send message with short TTL
    let msg_id = mb.send_mail_with_ttl(sender, receiver, &payload, 2).await.expect("Failed to send mail");
    assert!(msg_id.starts_with("msg_"));

    // 2. Verify it exists
    let msgs = mb.check_mail(receiver, false, 10).await.expect("Failed to check mail");
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].message_id, msg_id);

    // 3. Wait for expiry
    sleep(Duration::from_secs(3)).await;

    // 4. Purge
    let purged = mb.purge_expired().await.expect("Failed to purge");
    assert_eq!(purged, 1, "Should have purged exactly 1 message");

    // 5. Verify it's gone
    let msgs_after = mb.check_mail(receiver, false, 10).await.expect("Failed to check mail after purge");
    assert!(msgs_after.is_empty(), "Mailbox should be empty after purge");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_mailbox_get_thread() {
    let mb = Mailbox::in_memory().await.expect("Failed to create in-memory mailbox");
    let thread_id = "thread-123";
    
    let m1 = json!({ "thread_id": thread_id, "body": "Hello" }).to_string();
    let m2 = json!({ "thread_id": thread_id, "body": "World" }).to_string();
    let m3 = json!({ "thread_id": "other", "body": "Noise" }).to_string();

    mb.send_mail("a", "b", &m1).await.unwrap();
    mb.send_mail("b", "a", &m2).await.unwrap();
    mb.send_mail("a", "b", &m3).await.unwrap();

    let thread = mb.get_thread("a", thread_id).await.expect("Failed to get thread");
    assert_eq!(thread.len(), 2);
    assert!(thread[0].payload.contains("Hello"));
    assert!(thread[1].payload.contains("World"));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_blackboard_message_roundtrip() {
    let mb = Mailbox::in_memory().await.unwrap();
    mb.init().await.unwrap();

    let msg = BlackboardMessage::task("agent-a", "agent-b", "analyze the repo");
    mb.send_mail("agent-a", "agent-b", &msg.to_payload()).await.unwrap();

    let messages = mb.check_mail("agent-b", false, 10).await.unwrap();
    assert_eq!(messages.len(), 1);

    let parsed = BlackboardMessage::from_payload(&messages[0].payload)
        .expect("Should parse as BlackboardMessage");
    assert_eq!(parsed.msg_type, "task");
    assert_eq!(parsed.body, "analyze the repo");
    assert_eq!(parsed.from, "agent-a");
    assert_eq!(parsed.priority, 5);
}
