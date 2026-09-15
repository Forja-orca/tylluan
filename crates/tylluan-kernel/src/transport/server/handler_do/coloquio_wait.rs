//! # Coloquio Long-Poll Wait (turn 498/500 design)
//!
//! Any MCP-connected agent can block on a single `tylluan_do` call until a
//! new message arrives in a Coloquio channel or the timeout expires. ZERO
//! new tools (CONTRACT-01): the wait rides the existing intent parser as an
//! early-return prefix in `handler_do`.
//!
//! Design per Antigravity's research (docs/architecture/coloquio_long_poll_research.md):
//! 1. Fast path â€” DB cursor check (`since_turn`): messages that already exist
//!    answer immediately (anti-race, 0ms).
//! 2. Subscription â€” `TylluanServer.notifier` (the same broadcast channel the
//!    HTTP post handler publishes `coloquio:new_turn` on).
//! 3. Race â€” `tokio::select!` between broadcast recv (filtered) and the
//!    timeout. Graceful timeout: structured JSON, never an error result.
//! 4. Lagged recovery â€” a missed batch is re-read from SQLite, never lost.
//!
//! Zero orphan tasks: the whole wait lives inside the request Future â€” if the
//! client disconnects, the runtime drops it and the subscription dies with it.

use super::coloquio_utils::parse_coloquio_wait;
use crate::memory::coloquio::ColoquioDb;
use crate::transport::server::TylluanServer;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::timeout as tokio_timeout;

/// Default and clamp bounds for the wait window (research Â§4.3).
pub const WAIT_TIMEOUT_MIN_SECS: u64 = 5;
pub const WAIT_TIMEOUT_MAX_SECS: u64 = 300;
pub const WAIT_TIMEOUT_DEFAULT_SECS: u64 = 60;
const WAIT_MESSAGES_LIMIT: usize = 20;

fn wait_timeout_secs(requested: Option<u64>) -> u64 {
    requested
        .map(|v| v.clamp(WAIT_TIMEOUT_MIN_SECS, WAIT_TIMEOUT_MAX_SECS))
        .unwrap_or(WAIT_TIMEOUT_DEFAULT_SECS)
}

fn message_json(msg: &crate::memory::coloquio::ColoquioMessage) -> serde_json::Value {
    serde_json::json!({
        "msg_id": msg.msg_id,
        "channel_id": msg.channel_id,
        "author_id": msg.author_id,
        "content": msg.content,
        "turn": msg.turn,
    })
}

/// Entry point called from `handler_do` before routing. Returns Some(result)
/// when the intent is a wait intent, None otherwise.
pub async fn handle_coloquio_wait_prefix(
    server: &TylluanServer,
    intent: &str,
    agent_id: &Option<String>,
) -> Option<CallToolResult> {
    let (channel, timeout, since, mentions_only) = parse_coloquio_wait(intent)?;
    let Some(coloquio_db) = server.coloquio.clone() else {
        return Some(super::error_result("coloquio wait: no coloquio db wired"));
    };
    let timeout_secs = wait_timeout_secs(timeout);
    let since_turn = match since {
        Some(t) => t,
        None => coloquio_db.get_last_turn(&channel).await.unwrap_or(0),
    };
    let result = coloquio_wait_core(
        &coloquio_db,
        server.notifier.as_ref(),
        &channel,
        Duration::from_secs(timeout_secs),
        since_turn,
        mentions_only,
        agent_id.as_deref(),
    )
    .await;
    Some(result)
}

use crate::transport::server::CallToolResult;

/// The wait core, parameterized for testability (no full TylluanServer needed).
pub async fn coloquio_wait_core(
    coloquio: &ColoquioDb,
    notifier: Option<&broadcast::Sender<serde_json::Value>>,
    channel: &str,
    timeout_dur: Duration,
    since_turn: i64,
    mentions_only: bool,
    agent_id: Option<&str>,
) -> CallToolResult {
    // Phase 1 â€” fast path: messages that already exist answer immediately.
    match coloquio.get_messages_since(channel, since_turn, WAIT_MESSAGES_LIMIT).await {
        Ok(msgs) if !msgs.is_empty() => {
            let last = msgs.last().map(|m| m.turn).unwrap_or(since_turn);
            let payload = serde_json::json!({
                "status": "new",
                "channel_id": channel,
                "new_messages": msgs.iter().map(message_json).collect::<Vec<_>>(),
                "last_turn": last,
                "waited_seconds": 0,
                "timed_out": false,
            });
            return CallToolResult {
                content: vec![crate::transport::server::Content::text(serde_json::to_string(&payload).unwrap_or_default())],
                is_error: Some(false),
            };
        }
        Ok(_) => {}
        Err(e) => {
            return super::error_result(&format!("coloquio wait: DB fast-path failed: {e}"));
        }
    }

    // Phase 2 â€” subscription; no notifier wired means the wait cannot fire.
    let Some(tx) = notifier else {
        return super::error_result("coloquio wait: no broadcast notifier wired in this kernel build");
    };
    let mut rx = tx.subscribe();

    // Phase 3 â€” race between filtered broadcast events and the timeout.
    let deadline = tokio::time::Instant::now() + timeout_dur;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio_timeout(remaining, rx.recv()).await {
            Ok(Ok(event)) => {
                let obj = event.as_object();
                let ev_type = obj.and_then(|o| o.get("type")).and_then(|v| v.as_str()).unwrap_or("");
                if ev_type != "coloquio:new_turn" {
                    continue;
                }
                let ev_channel = obj.and_then(|o| o.get("channel_id")).and_then(|v| v.as_str()).unwrap_or("");
                if ev_channel != channel {
                    continue;
                }
                let ev_turn = obj.and_then(|o| o.get("turn")).and_then(|v| v.as_i64()).unwrap_or(0);
                if ev_turn <= since_turn {
                    continue;
                }
                // Self-exclusion: don't wake the agent with its own post.
                let ev_author = obj.and_then(|o| o.get("author_id")).and_then(|v| v.as_str()).unwrap_or("");
                if let Some(aid) = agent_id
                    && ev_author == aid
                {
                    continue;
                }
                if mentions_only {
                    let content = obj.and_then(|o| o.get("content")).and_then(|v| v.as_str()).unwrap_or("");
                    let mention = agent_id
                        .map(|aid| content.to_lowercase().contains(&format!("@{aid}").to_lowercase()))
                        .unwrap_or(false);
                    if !mention {
                        continue;
                    }
                }
                let payload = serde_json::json!({
                    "status": "new",
                    "channel_id": channel,
                    "new_messages": [event],
                    "last_turn": ev_turn,
                    "waited_seconds": timeout_dur.as_secs(),
                    "timed_out": false,
                });
                return CallToolResult {
                    content: vec![crate::transport::server::Content::text(serde_json::to_string(&payload).unwrap_or_default())],
                    is_error: Some(false),
                };
            }
            Ok(Err(broadcast::error::RecvError::Lagged(_))) => {
                // Missed a burst: recover the batch from SQLite instead of failing.
                match coloquio.get_messages_since(channel, since_turn, WAIT_MESSAGES_LIMIT).await {
                    Ok(msgs) if !msgs.is_empty() => {
                        let last = msgs.last().map(|m| m.turn).unwrap_or(since_turn);
                        let payload = serde_json::json!({
                            "status": "new",
                            "channel_id": channel,
                            "new_messages": msgs.iter().map(message_json).collect::<Vec<_>>(),
                            "last_turn": last,
                            "waited_seconds": timeout_dur.as_secs(),
                            "timed_out": false,
                        });
                        return CallToolResult {
                            content: vec![crate::transport::server::Content::text(serde_json::to_string(&payload).unwrap_or_default())],
                            is_error: Some(false),
                        };
                    }
                    _ => continue,
                }
            }
            Ok(Err(broadcast::error::RecvError::Closed)) => {
                return super::error_result("coloquio wait: broadcast channel closed");
            }
            Err(_) => break, // timeout elapsed
        }
    }

    // Phase 4 â€” graceful timeout: structured JSON, never an error result.
    let payload = serde_json::json!({
        "status": "timeout",
        "channel_id": channel,
        "new_messages": [],
        "last_turn": since_turn,
        "waited_seconds": timeout_dur.as_secs(),
        "timed_out": true,
    });
    CallToolResult {
        content: vec![crate::transport::server::Content::text(serde_json::to_string(&payload).unwrap_or_default())],
        is_error: Some(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::coloquio::ColoquioDb;
    use rmcp::model::RawContent;
    use std::sync::Arc;

    fn text_of(r: &CallToolResult) -> String {
        r.content
            .iter()
            .filter_map(|c| match &c.raw {
                RawContent::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn fast_path_returns_existing_messages_immediately() {
        let db = Arc::new(ColoquioDb::new(":memory:").unwrap());
        db.post_message("general", "alice", "human", "hello before wait", "{}").await.unwrap();
        let (tx, _) = broadcast::channel(10);

        let r = coloquio_wait_core(&db, Some(&tx), "general", Duration::from_secs(5), 0, false, Some("deep")).await;
        assert!(!r.is_error.unwrap_or(true));
        let payload: serde_json::Value = serde_json::from_str(&text_of(&r)).unwrap();
        assert_eq!(payload["status"], "new");
        assert_eq!(payload["new_messages"].as_array().unwrap().len(), 1);
        assert_eq!(payload["last_turn"], 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn reactive_path_returns_message_posted_during_wait() {
        let db = Arc::new(ColoquioDb::new(":memory:").unwrap());
        let (tx, _) = broadcast::channel(10);

        let db2 = db.clone();
        let tx2 = tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            db2.post_message("general", "bob", "agent", "late reply", "{}").await.unwrap();
            let _ = tx2.send(serde_json::json!({
                "type": "coloquio:new_turn", "channel_id": "general",
                "author_id": "bob", "content": "late reply", "turn": 1
            }));
        });

        let r = coloquio_wait_core(&db, Some(&tx), "general", Duration::from_secs(5), 0, false, Some("deep")).await;
        assert!(!r.is_error.unwrap_or(true));
        let payload: serde_json::Value = serde_json::from_str(&text_of(&r)).unwrap();
        assert_eq!(payload["status"], "new");
        assert_eq!(payload["last_turn"], 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn timeout_returns_graceful_json_not_error() {
        let db = Arc::new(ColoquioDb::new(":memory:").unwrap());
        let (tx, _) = broadcast::channel(10);

        let r = coloquio_wait_core(&db, Some(&tx), "general", Duration::from_millis(50), 0, false, Some("deep")).await;
        assert!(!r.is_error.unwrap_or(true), "timeout must NOT be an error result");
        let payload: serde_json::Value = serde_json::from_str(&text_of(&r)).unwrap();
        assert_eq!(payload["status"], "timeout");
        assert_eq!(payload["timed_out"], true);
        assert_eq!(payload["new_messages"].as_array().unwrap().len(), 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn mentions_only_ignores_unrelated_posts() {
        let db = Arc::new(ColoquioDb::new(":memory:").unwrap());
        let (tx, _) = broadcast::channel(10);

        let r = coloquio_wait_core(&db, Some(&tx), "general", Duration::from_millis(50), 0, true, Some("deep")).await;
        assert_eq!(serde_json::from_str::<serde_json::Value>(&text_of(&r)).unwrap()["status"], "timeout");
    }
}