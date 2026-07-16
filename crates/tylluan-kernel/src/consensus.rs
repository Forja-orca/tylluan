//! # Truth Consensus Engine
//!
//! Processes the conflict queue using semantic clustering, truth weighting,
//! and deterministic freshness resolution (paper 1.3 — "Don't Ask the LLM
//! to Track Freshness").

use crate::memory::silva::{GraphNode, SilvaDB};
use anyhow::Result;
use std::sync::Arc;
use tracing::info;
use chrono::Utc;

pub struct ConsensusEngine {
    silva: Arc<SilvaDB>,
}

impl ConsensusEngine {
    pub fn new(silva: Arc<SilvaDB>) -> Self {
        Self { silva }
    }

    pub async fn resolve_conflicts(&self) -> Result<()> {
        let conflicted = self.silva.get_all_conflicted().await?;
        if conflicted.is_empty() {
            return Ok(());
        }

        info!("🧠 [Consensus] Found {} conflicted nodes. Starting resolution...", conflicted.len());

        for node in conflicted {
            let node_id = node.id.clone();

            if let Ok(Some((matched, _score))) = self.find_similar(&node.content, 0.88).await {
                info!("🔄 [Consensus] Merge '{}' -> '{}'", node_id, matched.id);

                let current_meta: serde_json::Value = serde_json::from_str(&matched.metadata).unwrap_or(serde_json::json!({}));
                let mut updated_meta = current_meta.clone();

                let weight = updated_meta.get("weight").and_then(|v| v.as_f64()).unwrap_or(1.0);
                updated_meta["weight"] = serde_json::json!(weight + 0.1);
                updated_meta["last_reinforced"] = serde_json::json!(Utc::now().to_rfc3339());

                self.silva.upsert_node(&matched.id, &matched.node_type, &matched.content, &updated_meta.to_string()).await?;
                self.silva.set_weight(&node_id, 0.0).await?;
            } else {
                let _ = self.silva.mark_conflicted(&node_id, false).await;
                info!("✅ [Consensus] Approved unique thought: '{}'", node_id);
            }
        }

        Ok(())
    }

    async fn find_similar(&self, content: &str, threshold: f64) -> Result<Option<(GraphNode, f64)>> {
        let emb = self.silva.get_node_embedding(&format!("query:{content}")).await?;
        let Some(emb) = emb else { return Ok(None); };

        let results = self.silva.search_vector(&emb, 5).await?;
        for (node, score) in results {
            if score as f64 >= threshold && node.id != format!("query:{content}") {
                return Ok(Some((node, score as f64)));
            }
        }
        Ok(None)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Paper 1.3: Deterministic Freshness Resolution
// "Don't Ask the LLM to Track Freshness" (Reddy & Challaram, 2026)
// ═══════════════════════════════════════════════════════════════════════════════

/// Outcome of a deterministic freshness check between two versions of a node.
#[derive(Debug, Clone, PartialEq)]
pub enum FreshnessResolution {
    /// Content is identical (same SHA-256 hash) — no conflict, skip update.
    Identical,
    /// Remote version wins (newer, higher priority, or tiebreak).
    AcceptRemote {
        /// Human-readable reason for the decision.
        reason: String,
    },
    /// Local version wins — skip the incoming update.
    KeepLocal {
        reason: String,
    },
    /// Neither clearly wins — mark as conflicted for human review.
    MarkConflicted {
        reason: String,
    },
}

/// Run the deterministic freshness resolution between a local node and an
/// incoming remote version. This implements the SH-conflict + CAR protocol
/// from paper 1.3, with zero LLM calls in the critical path.
///
/// # Resolution rules (in order):
/// 1. **SH-conflict** — Same SHA-256 content_hash → Identical (no-op).
/// 2. **Protected guard** — Local node is protected → KeepLocal.
/// 3. **Peer priority** — Lower-numbered peer priority wins
///    (priority 1 beats priority 2).
/// 4. **Timestamp** — Newer `updated_at` wins (lexical ISO-8601 comparison).
/// 5. **Deterministic tiebreak** — Lexicographic peer ID comparison.
pub fn resolve_node_freshness(
    local_content_hash: &str,
    local_protected: bool,
    local_updated_at: &str,
    remote_content_hash: &str,
    remote_peer_priority: u32,
    remote_peer_name: &str,
    remote_updated_at: &str,
) -> FreshnessResolution {
    // Rule 1: SH-conflict — same hash = same content, no conflict.
    if !local_content_hash.is_empty() && local_content_hash == remote_content_hash {
        return FreshnessResolution::Identical;
    }

    // Rule 2: Protected nodes are never overwritten.
    if local_protected {
        return FreshnessResolution::KeepLocal {
            reason: "local node is protected".to_string(),
        };
    }

    // Rule 3: Lower peer priority number wins (1 > 2 > 3).
    // Local priority is always 1 (highest — the node's origin peer).
    // Remote priority 0 means "highest possible" (used for special infra peers).
    const LOCAL_PEER_PRIORITY: u32 = 1;
    if remote_peer_priority > LOCAL_PEER_PRIORITY && remote_peer_priority <= 100 {
        // Remote has lower priority (higher number, in plausible range) → local wins
        return FreshnessResolution::KeepLocal {
            reason: format!("local peer priority {LOCAL_PEER_PRIORITY} beats remote priority {remote_peer_priority}"),
        };
    }
    if remote_peer_priority < LOCAL_PEER_PRIORITY && remote_peer_priority == 0 {
        // Remote has higher priority (0 = infra tier) → remote wins
        return FreshnessResolution::AcceptRemote {
            reason: format!("remote peer priority {remote_peer_priority} beats local priority {LOCAL_PEER_PRIORITY}"),
        };
    }

    // Rule 4: Timestamp comparison (ISO-8601 lexical works for same-length strings).
    match local_updated_at.cmp(remote_updated_at) {
        std::cmp::Ordering::Less => FreshnessResolution::AcceptRemote {
            reason: format!("remote version is newer ({remote_updated_at} > {local_updated_at})"),
        },
        std::cmp::Ordering::Greater => FreshnessResolution::KeepLocal {
            reason: format!("local version is newer ({local_updated_at} > {remote_updated_at})"),
        },
        // Rule 5: Same timestamp → lexicographic peer ID tiebreak.
        std::cmp::Ordering::Equal => {
            // "local" is always from the current peer, named "local".
            // Remote peer name is the tiebreaker.
            if remote_peer_name < "local" {
                FreshnessResolution::AcceptRemote {
                    reason: format!("tiebreak: remote peer '{remote_peer_name}' < local"),
                }
            } else {
                FreshnessResolution::KeepLocal {
                    reason: format!("tiebreak: local < remote peer '{remote_peer_name}'"),
                }
            }
        }
    }
}

#[cfg(test)]
mod freshness_tests {
    use super::*;

    #[test]
    fn test_sh_conflict_identical_hash() {
        let result = resolve_node_freshness(
            "abc123", false, "2026-07-01T00:00:00Z",
            "abc123", 1, "peer-b", "2026-07-02T00:00:00Z",
        );
        assert_eq!(result, FreshnessResolution::Identical);
    }

    #[test]
    fn test_protected_node_not_overwritten() {
        let result = resolve_node_freshness(
            "hash-a", true, "2026-07-01T00:00:00Z",
            "hash-b", 1, "peer-b", "2026-07-02T00:00:00Z",
        );
        assert_eq!(result, FreshnessResolution::KeepLocal { reason: "local node is protected".into() });
    }

    #[test]
    fn test_priority_remote_lower_wins_local() {
        // Priority: remote = 2 (lower than local = 1) → local wins
        let result = resolve_node_freshness(
            "hash-a", false, "2026-07-01T00:00:00Z",
            "hash-b", 2, "peer-b", "2026-07-02T00:00:00Z",
        );
        assert_eq!(
            result,
            FreshnessResolution::KeepLocal {
                reason: "local peer priority 1 beats remote priority 2".into()
            }
        );
    }

    #[test]
    fn test_priority_remote_higher_wins_remote() {
        // Priority: remote = 0 (higher than local = 1) → remote wins
        let result = resolve_node_freshness(
            "hash-a", false, "2026-07-01T00:00:00Z",
            "hash-b", 0, "peer-b", "2026-07-01T00:00:00Z",
        );
        assert_eq!(
            result,
            FreshnessResolution::AcceptRemote {
                reason: "remote peer priority 0 beats local priority 1".into()
            }
        );
    }

    #[test]
    fn test_equal_priority_remote_newer_wins() {
        let result = resolve_node_freshness(
            "hash-a", false, "2026-07-01T00:00:00Z",
            "hash-b", 1, "peer-b", "2026-07-02T00:00:00Z",
        );
        assert_eq!(
            result,
            FreshnessResolution::AcceptRemote {
                reason: "remote version is newer (2026-07-02T00:00:00Z > 2026-07-01T00:00:00Z)".into()
            }
        );
    }

    #[test]
    fn test_equal_priority_local_newer_wins() {
        let result = resolve_node_freshness(
            "hash-a", false, "2026-07-03T00:00:00Z",
            "hash-b", 1, "peer-b", "2026-07-02T00:00:00Z",
        );
        assert_eq!(
            result,
            FreshnessResolution::KeepLocal {
                reason: "local version is newer (2026-07-03T00:00:00Z > 2026-07-02T00:00:00Z)".into()
            }
        );
    }

    #[test]
    fn test_equal_priority_same_timestamp_tiebreak() {
        let result = resolve_node_freshness(
            "hash-a", false, "2026-07-01T00:00:00Z",
            "hash-b", 1, "z-peer", "2026-07-01T00:00:00Z",
        );
        // "z-peer" > "local" → KeepLocal
        assert_eq!(
            result,
            FreshnessResolution::KeepLocal {
                reason: "tiebreak: local < remote peer 'z-peer'".into()
            }
        );
    }

    #[test]
    fn test_equal_priority_tiebreak_remote_wins() {
        let result = resolve_node_freshness(
            "hash-a", false, "2026-07-01T00:00:00Z",
            "hash-b", 1, "a-peer", "2026-07-01T00:00:00Z",
        );
        // "a-peer" < "local" → remote wins
        assert_eq!(
            result,
            FreshnessResolution::AcceptRemote {
                reason: "tiebreak: remote peer 'a-peer' < local".into()
            }
        );
    }

    #[test]
    fn test_empty_local_hash_no_sh_conflict() {
        let result = resolve_node_freshness(
            "", false, "2026-07-01T00:00:00Z",
            "hash-new", 1, "peer-b", "2026-07-02T00:00:00Z",
        );
        // Empty hash means no SH-conflict match → falls through to timestamp
        assert_eq!(
            result,
            FreshnessResolution::AcceptRemote {
                reason: "remote version is newer (2026-07-02T00:00:00Z > 2026-07-01T00:00:00Z)".into()
            }
        );
    }
}
