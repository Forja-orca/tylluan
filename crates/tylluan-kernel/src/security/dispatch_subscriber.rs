//! # Dispatch subscriber — kernel-side enqueue (BWC-3 + BWC-4, coloquio_dispatcher_spec.md)
//!
//! The kernel is the always-on system: when a Coloquio message mentions an
//! agent with an ACTIVE `[wake]` policy from a trusted author, the kernel
//! itself must notice — not a client-side loop that dies with a terminal
//! (the pull-design lesson, T541-T558).
//!
//! BWC-3 delivered the subscriber consuming the existing `coloquio:new_turn`
//! broadcast (api_coloquio.rs:173) with the full filter chain — that chain
//! (`evaluate_event`) is unchanged and still owns the fail-closed gating:
//! type → mentions → self-exclusion → wake policy → trusted author.
//!
//! BWC-4 activates the real enqueue: every dispatch that passes the chain is
//! inserted EXACTLY-ONCE into the BWC-1 queue (`enqueue_once`) as `Pending`.
//! Nothing executes here — execution requires a human approval bound to the
//! content hash (BWC-2) plus the executor's CAS claim (`dispatch_executor.rs`).
//! Zero tools (CONTRACT-01), zero process spawning in this module.
//!
//! Existence note: `active_wake_config` returns `Some` only for agents listed
//! in the contract, so contract membership IS the existence gate — the
//! SilvaDB-identity fallback adds nothing here (an agent without a `[wake]`
//! table is invisible either way = "solo buzon" behavior).

use crate::security::agents_contract::AgentsContract;
use crate::security::dispatch_queue::DispatchQueue;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::info;

/// Everything the queue would have stored had this been BWC-1/BWC-4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DryRunDispatch {
    pub agent_id: String,
    pub author_id: String,
    pub channel: String,
    pub turn: i64,
    pub content_hash: String,
    pub command: Vec<String>,
    pub auto_approve: bool,
}

pub fn sha256_hex(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Full filter chain for one broadcast event (pure, testable):
/// type -> mentions -> self-exclusion -> wake policy -> trusted author.
/// Returns every (agent, event) pair that WOULD be queued.
pub fn evaluate_event(event: &Value, contract: &AgentsContract) -> Vec<DryRunDispatch> {
    let Some(obj) = event.as_object() else { return Vec::new() };
    if obj.get("type").and_then(|v| v.as_str()) != Some("coloquio:new_turn") {
        return Vec::new();
    }
    let Some(channel) = obj.get("channel_id").and_then(|v| v.as_str()) else {
        return Vec::new();
    };
    let Some(content) = obj.get("content").and_then(|v| v.as_str()) else {
        return Vec::new();
    };
    let author = obj.get("author_id").and_then(|v| v.as_str()).unwrap_or("");
    let turn = obj.get("turn").and_then(|v| v.as_i64()).unwrap_or(0);
    let content_hash = sha256_hex(content);

    let mut out = Vec::new();
    for mention in crate::memory::coloquio::extract_mentions(content) {
        if mention.eq_ignore_ascii_case(author) {
            continue; // auto-exclusion
        }
        let Some(wake) = contract.active_wake_config(&mention) else {
            continue; // no wake policy = inbox-only
        };
        if !wake.is_active() {
            continue; // fail-closed: incomplete config is inert
        }
        if !wake.trusts(author) {
            continue; // allowlist (Jose's decision 2026-09-17)
        }
        out.push(DryRunDispatch {
            agent_id: mention,
            author_id: author.to_string(),
            channel: channel.to_string(),
            turn,
            content_hash: content_hash.clone(),
            command: wake.command.clone(),
            auto_approve: wake.auto_approve,
        });
    }
    out
}

/// Per-event action, shared by the spawn task and tests: enqueue every
/// dispatch that passes the filter chain, exactly-once per message. Returns
/// the number of rows THIS call inserted (a replay or duplicate suppresses
/// to 0 without error).
pub fn queue_event(
    queue: &DispatchQueue,
    event: &Value,
    contract: &AgentsContract,
) -> anyhow::Result<usize> {
    let content = event.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let mut queued = 0;
    for d in evaluate_event(event, contract) {
        let inserted = if d.auto_approve {
            queue.enqueue_once_auto_approved(&d.agent_id, &d.author_id, &d.channel, d.turn, content, d.command.clone())?
        } else {
            queue.enqueue_once(&d.agent_id, &d.author_id, &d.channel, d.turn, content, d.command.clone())?
        };
        if inserted {
            queued += 1;
            if d.auto_approve {
                info!(
                    "[dispatch-queue] AUTO-APPROVED agent={} author={} channel={} turn={} hash={} — [wake].auto_approve=true, executor will claim on next poll",
                    d.agent_id, d.author_id, d.channel, d.turn, &d.content_hash[..8]
                );
            } else {
                info!(
                    "[dispatch-queue] QUEUED agent={} author={} channel={} turn={} hash={} — awaiting human approval (hash-bound)",
                    d.agent_id, d.author_id, d.channel, d.turn, &d.content_hash[..8]
                );
            }
        } else {
            info!(
                "[dispatch-queue] duplicate suppressed agent={} author={} channel={} turn={} (exactly-once per message)",
                d.agent_id, d.author_id, d.channel, d.turn
            );
        }
    }
    Ok(queued)
}

/// Background task: subscribe to the kernel's broadcast and enqueue real
/// PendingDispatch rows for qualifying events. The queue is injected by the
/// boot wiring (opened once there via `dispatch_db_path` — one owner of the
/// path, shared with the executor). Execution NEVER happens here: it
/// requires human approval via BWC-2 and the executor's claim (BWC-4).
pub fn spawn_dispatch_subscriber(
    notifier: broadcast::Sender<Value>,
    contract: Arc<AgentsContract>,
    queue: Arc<DispatchQueue>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut rx = notifier.subscribe();
        info!("[dispatch-queue] subscriber active (BWC-4: real enqueue; execution = human approval + executor claim)");
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Err(e) = queue_event(&queue, &event, &contract) {
                        tracing::error!("[dispatch-queue] queue_event failed: {e:#}");
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => {
                    info!("[dispatch-queue] broadcast closed, subscriber exiting");
                    return;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::agents_contract::{AgentContractEntry, AgentsContract};
    use std::collections::HashMap;

    fn contract_with_wake(agent: &str, enabled: bool, trusted: Vec<String>, command: Vec<String>) -> AgentsContract {
        let mut agents = HashMap::new();
        agents.insert(
            agent.to_string(),
            AgentContractEntry {
                role: "contributor".to_string(),
                description: "test agent".to_string(),
                wake: Some(crate::security::agents_contract::WakeConfig {
                    enabled,
                    trusted_authors: trusted,
                    command,
                    ..Default::default()
                }),
            },
        );
        AgentsContract { agents }
    }

    fn event(channel: &str, author: &str, content: &str, turn: i64) -> Value {
        serde_json::json!({
            "type": "coloquio:new_turn",
            "channel_id": channel,
            "author_id": author,
            "content": content,
            "turn": turn,
        })
    }

    #[test]
    fn trusted_author_with_active_wake_is_detected() {
        let c = contract_with_wake("deep", true, vec!["claude-code".into()], vec!["opencode".into(), "run".into()]);
        let d = evaluate_event(&event("general", "claude-code", "revisa esto @deep", 5), &c);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].agent_id, "deep");
        assert_eq!(d[0].content_hash, sha256_hex("revisa esto @deep"));
        assert_eq!(d[0].command, vec!["opencode".to_string(), "run".to_string()]);
    }

    #[test]
    fn untrusted_author_is_skipped() {
        let c = contract_with_wake("deep", true, vec!["claude-code".into()], vec!["opencode".into()]);
        assert!(evaluate_event(&event("general", "antigravity", "haz algo @deep", 6), &c).is_empty());
    }

    #[test]
    fn inactive_wake_is_inert() {
        // enabled=true but empty trusted/command must be inert (fail-closed).
        let c = contract_with_wake("deep", true, vec![], vec![]);
        assert!(evaluate_event(&event("general", "claude-code", "haz algo @deep", 7), &c).is_empty());
    }

    #[test]
    fn self_mention_never_wakes() {
        let c = contract_with_wake("deep", true, vec!["claude-code".into()], vec!["opencode".into()]);
        assert!(evaluate_event(&event("general", "deep", "mira @deep esto", 8), &c).is_empty());
    }

    #[test]
    fn non_coloquio_events_ignored() {
        let c = contract_with_wake("deep", true, vec!["claude-code".into()], vec!["opencode".into()]);
        let other = serde_json::json!({ "type": "session_updated", "data": {} });
        assert!(evaluate_event(&other, &c).is_empty());
    }

    #[test]
    fn agent_without_wake_policy_is_invisible() {
        let c = AgentsContract { agents: HashMap::new() };
        assert!(evaluate_event(&event("general", "claude-code", "haz algo @deep", 9), &c).is_empty());
    }

    #[test]
    fn queue_event_enqueues_passing_dispatch_exactly_once() {
        // BWC-4: the real enqueue — a passing mention becomes a Pending row
        // bound to the message hash, and a broadcast replay is deduped.
        let c = contract_with_wake("deep", true, vec!["claude-code".into()], vec!["opencode".into(), "run".into()]);
        let q = DispatchQueue::in_memory().unwrap();
        let e = event("general", "claude-code", "revisa esto @deep", 20);

        assert_eq!(queue_event(&q, &e, &c).unwrap(), 1);
        let pending = q.list_pending().unwrap();
        assert_eq!(pending.len(), 1);
        let d = &pending[0];
        assert_eq!(d.agent_id, "deep");
        assert_eq!(d.author_id, "claude-code");
        assert_eq!(d.channel, "general");
        assert_eq!(d.turn, 20);
        assert_eq!(d.content_snapshot, "revisa esto @deep");
        assert_eq!(d.content_hash, sha256_hex("revisa esto @deep"));
        assert_eq!(d.command, vec!["opencode".to_string(), "run".to_string()]);

        // Replay of the same message: suppressed, nothing inserted.
        assert_eq!(queue_event(&q, &e, &c).unwrap(), 0);
        assert_eq!(q.list_pending().unwrap().len(), 1);
    }

    #[test]
    fn queue_event_auto_approve_lands_directly_in_approved_not_pending() {
        // 2026-09-21 (Jose): auto_approve skips the human hash-bound click.
        // The row must land straight in Approved so the executor's next
        // poll picks it up without anyone calling the BWC-2 endpoint.
        let mut c = contract_with_wake("deep", true, vec!["claude-code".into()], vec!["opencode".into(), "run".into()]);
        c.agents.get_mut("deep").unwrap().wake.as_mut().unwrap().auto_approve = true;
        let q = DispatchQueue::in_memory().unwrap();
        let e = event("general", "claude-code", "revisa esto @deep", 30);

        assert_eq!(queue_event(&q, &e, &c).unwrap(), 1);
        assert!(q.list_pending().unwrap().is_empty(), "auto-approved dispatch must not sit in Pending");
        let approved = q.list_approved().unwrap();
        assert_eq!(approved.len(), 1);
        assert_eq!(approved[0].agent_id, "deep");
        assert_eq!(approved[0].state, crate::security::dispatch_queue::DispatchState::Approved);

        // Exactly-once still holds for the auto-approve path.
        assert_eq!(queue_event(&q, &e, &c).unwrap(), 0);
        assert_eq!(q.list_approved().unwrap().len(), 1);
    }

    #[test]
    fn queue_event_without_auto_approve_still_requires_human_approval() {
        // Regression guard: the default (auto_approve omitted/false) must
        // keep the pre-2026-09-21 behavior exactly — no silent widening.
        let c = contract_with_wake("deep", true, vec!["claude-code".into()], vec!["opencode".into()]);
        assert!(!c.agents["deep"].wake.as_ref().unwrap().auto_approve, "default must stay false");
        let q = DispatchQueue::in_memory().unwrap();
        let e = event("general", "claude-code", "revisa esto @deep", 31);

        assert_eq!(queue_event(&q, &e, &c).unwrap(), 1);
        assert_eq!(q.list_pending().unwrap().len(), 1, "without auto_approve, row must sit in Pending");
        assert!(q.list_approved().unwrap().is_empty());
    }

    #[test]
    fn queue_event_skips_non_qualifying_events() {
        let c = contract_with_wake("deep", true, vec!["claude-code".into()], vec!["opencode".into()]);
        let q = DispatchQueue::in_memory().unwrap();
        // Untrusted author.
        assert_eq!(queue_event(&q, &event("general", "antigravity", "haz algo @deep", 21), &c).unwrap(), 0);
        // Self-mention.
        assert_eq!(queue_event(&q, &event("general", "deep", "mira @deep", 22), &c).unwrap(), 0);
        // Non-coloquio event.
        let other = serde_json::json!({ "type": "session_updated" });
        assert_eq!(queue_event(&q, &other, &c).unwrap(), 0);
        assert!(q.list_pending().unwrap().is_empty());
    }

}
