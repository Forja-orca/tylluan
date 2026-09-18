//! # Dispatch subscriber — internal dry-run (BWC-3, coloquio_dispatcher_spec.md)
//!
//! The kernel is the always-on system: when a Coloquio message mentions an
//! agent with an ACTIVE `[wake]` policy from a trusted author, the kernel
//! itself must notice — not a client-side loop that dies with a terminal
//! (the pull-design lesson, T541-T558).
//!
//! BWC-3 delivers the subscriber WITHOUT execution: it consumes the existing
//! `coloquio:new_turn` broadcast (api_coloquio.rs:173), runs the full filter
//! chain and only LOGS what it would queue. Zero tools (CONTRACT-01), zero
//! process spawning (BWC-4), no queue writes (BWC-1).
//!
//! Existence note: `active_wake_config` returns `Some` only for agents listed
//! in the contract, so contract membership IS the existence gate — the
//! SilvaDB-identity fallback adds nothing here (an agent without a `[wake]`
//! table is invisible either way = "solo buzon" behavior).

use crate::security::agents_contract::AgentsContract;
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
        });
    }
    out
}

/// Background task: subscribe to the kernel's broadcast and dry-run the
/// dispatch filter. Deliberately NOT wired with process execution (BWC-4).
pub fn spawn_dispatch_subscriber(
    notifier: broadcast::Sender<Value>,
    contract: Arc<AgentsContract>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut rx = notifier.subscribe();
        info!("[dispatch-dry-run] subscriber active (BWC-3, dry-run only)");
        loop {
            match rx.recv().await {
                Ok(event) => {
                    for d in evaluate_event(&event, &contract) {
                        info!(
                            "[dispatch-dry-run] WOULD queue agent={} author={} channel={} turn={} hash={} command={:?}",
                            d.agent_id, d.author_id, d.channel, d.turn, &d.content_hash[..8], d.command
                        );
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => {
                    info!("[dispatch-dry-run] broadcast closed, subscriber exiting");
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
}