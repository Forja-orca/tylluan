use crate::memory::silva::{GraphNode, SilvaDB, NodeWriteOptions};
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

/// Manages per-agent memory nodes in SilvaDB.
///
/// Agent memories are stored as `node_type = "agent_memory"` nodes with
/// metadata containing `agent_id` and `importance`. The manager handles
/// recording, retrieval, slow-decay, and consolidation into summaries.
pub struct AgentMemoryManager {
    silva: Arc<SilvaDB>,
    max_memories_before_summary: usize,
}

impl AgentMemoryManager {
    pub fn new(silva: Arc<SilvaDB>, max_memories: usize) -> Self {
        Self { silva, max_memories_before_summary: max_memories }
    }

    /// Record a memory for an agent.
    ///
    /// Stores a SilvaDB node with:
    /// - `node_type = "agent_memory"`
    /// - `metadata` containing `agent_id` and `importance`
    /// - `content` prefixed with `[agent_id]` so search can find it
    /// - initial `weight = importance.clamp(0.1, 5.0)`
    pub async fn record_memory(&self, agent_id: &str, content: &str, importance: f64) -> String {
        let node_id = format!("agent_memory:{}:{}", agent_id, Uuid::new_v4().simple());
        let tagged = format!("[{agent_id}] {content}");
        let meta = serde_json::json!({
            "agent_id": agent_id,
            "importance": importance,
        }).to_string();

        if self.silva.upsert_node_with_provenance(&node_id, "agent_memory", &tagged, &meta, "agent_generated").await.is_ok() {
            let weight = importance.clamp(0.1, 5.0);
            let _ = self.silva.set_weight(&node_id, weight).await;
        }
        node_id
    }

    /// Ouroboros Loop — record half. The agent (the LLM) performs its own
    /// reflection on an action's outcome (Reflexion, Shinn et al. NeurIPS 2023)
    /// and passes the verdict; the kernel only persists it, never judges — no
    /// LLM in Tylluan's critical path. Stored per-agent (owner_scope=agent:{id})
    /// so an agent consults ITS OWN past experience, not a shared pool.
    ///
    /// `verdict`: "worked" | "failed" | "partial". Failures are weighted
    /// HIGHER than successes — the most actionable lesson is what not to repeat.
    pub async fn record_experience(
        &self,
        agent_id: &str,
        action: &str,
        outcome: &str,
        verdict: &str,
        lesson: &str,
    ) -> String {
        let node_id = format!("experience:{}:{}", agent_id, Uuid::new_v4().simple());
        let content = format!("[{verdict}] {action} → {outcome}");
        let content = if lesson.trim().is_empty() {
            content
        } else {
            format!("{content} | lección: {lesson}")
        };
        let meta = serde_json::json!({
            "agent_id": agent_id,
            "verdict": verdict,
            "kind": "experience",
        }).to_string();
        let scope = format!("agent:{agent_id}");
        let opts = NodeWriteOptions::new("agent_generated").owner_scope(Some(&scope));
        if self.silva.upsert_node_with_validity(&node_id, "experience", &content, &meta, opts).await.is_ok() {
            // Reflexion weighting: failures persist longest (they're the
            // actionable "don't do this again" lessons), partial next, wins least.
            let weight = match verdict {
                "failed" => 2.0,
                "partial" => 1.2,
                _ => 0.8,
            };
            let _ = self.silva.set_weight(&node_id, weight).await;
        }
        node_id
    }

    /// Ouroboros Loop — retrieve half. Returns this agent's own past
    /// experiences most relevant to `query`, weight-ordered (so failures,
    /// weighted higher, surface first). Consulted by tylluan_think before the
    /// agent reasons about what to do — "have I tried this before, how did it go".
    pub async fn get_relevant_critiques(&self, agent_id: &str, query: &str, limit: usize) -> Vec<GraphNode> {
        let mut results = self.silva
            .search(query, limit * 3, Some(&["experience"]))
            .await
            .unwrap_or_default();
        // Scope to THIS agent only — never leak another agent's experience.
        results.retain(|n| n.metadata.contains(&format!("\"agent_id\":\"{agent_id}\"")));
        results.sort_by(|a, b| b.weight.partial_cmp(&a.weight).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);
        results
    }

    /// Retrieve memories for an agent, ordered by weight descending.
    ///
    /// Uses FTS search for agent_id in content/metadata, filtered by
    /// the `agent_memory` node type.
    pub async fn get_memories(&self, agent_id: &str, limit: usize) -> Vec<GraphNode> {
        let mut results = self.silva
            .search(agent_id, limit, Some(&["agent_memory"]))
            .await
            .unwrap_or_default();

        // Sort by weight descending (search may return in any order)
        results.sort_by(|a, b| b.weight.partial_cmp(&a.weight).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);
        results
    }

    /// Retrieve memories for an agent using a direct SQL query by content prefix.
    ///
    /// More reliable than FTS search for exact agent_id retrieval:
    /// `SELECT ... FROM nodes WHERE type='agent_memory' AND content LIKE '[{agent_id}]%' ORDER BY weight DESC`
    pub async fn get_memories_raw(&self, agent_id: &str, limit: usize) -> Vec<GraphNode> {
        let prefix = format!("[{agent_id}]");
        self.silva
            .get_nodes_by_type_and_prefix("agent_memory", &prefix, limit)
            .await
            .unwrap_or_default()
    }

    /// Apply slow decay to agent memory nodes.
    ///
    /// Decay multiplier is 0.98 (vs the global 0.85 decay rate).
    /// Memories with `importance > 0.8` are protected from decay.
    pub async fn decay_agent_memories(&self, agent_id: &str) {
        let memories = self.get_memories(agent_id, 200).await;
        for node in &memories {
            let importance: f64 = serde_json::from_str(&node.metadata)
                .ok()
                .and_then(|v: serde_json::Value| v.get("importance").and_then(|i| i.as_f64()))
                .unwrap_or(0.0);

            if importance > 0.8 {
                continue;
            }

            let _ = self.silva.decay_node(&node.id, 36000).await; // ~2% half-life decay
        }
    }

    /// Consolidate old low-weight memories into a summary if count exceeds threshold.
    ///
    /// 1. Count agent_memory nodes.
    /// 2. If count > `max_memories_before_summary`:
    ///    - Take the 15 oldest with lowest weight
    ///    - Extract first 80 chars of each
    ///    - Create a node with `node_type = "agent_summary"`
    ///    - Apply `decay_node(old_id, 0.3)` to mark original memories for pruning
    pub async fn consolidate_if_needed(&self, agent_id: &str) {
        let memories = self.get_memories(agent_id, 500).await;
        if memories.len() < self.max_memories_before_summary {
            return;
        }

        info!(
            "Consolidating {} memories for agent '{}' (threshold: {})",
            memories.len(), agent_id, self.max_memories_before_summary
        );

        // Pick 15 oldest with lowest weight
        let mut candidates: Vec<&GraphNode> = memories.iter().collect();
        candidates.sort_by(|a, b| {
            a.weight.partial_cmp(&b.weight)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.created_at.cmp(&b.created_at))
        });
        let to_summarize: Vec<&&GraphNode> = candidates.iter().take(15).collect();

        let summary_text: String = to_summarize.iter()
            .enumerate()
            .map(|(i, n)| {
                let preview: String = n.content.chars().take(80).collect();
                format!("{}. {} (w={:.2})", i + 1, preview, n.weight)
            })
            .collect::<Vec<_>>()
            .join("\n");

        let summary = format!(
            "Resumen de {} memorias:\n{}",
            to_summarize.len(),
            summary_text
        );

        let summary_id = format!("agent_summary:{}:{}", agent_id, chrono::Utc::now().timestamp());
        let summary_meta = serde_json::json!({
            "agent_id": agent_id,
            "consolidated": true,
            "source_count": to_summarize.len(),
        }).to_string();

        let _ = self.silva.upsert_node_with_validity(&summary_id, "agent_summary", &summary, &summary_meta, NodeWriteOptions::new("agent_generated").drift_allowed(true)).await;

        let count = to_summarize.len();

        // Decay old memories so they are eventually pruned
        for n in &to_summarize {
            let _ = self.silva.decay_node(&n.id, 2592000).await; // 30 days ~70% decay
        }

        info!("Agent '{}' consolidation complete — summary '{}' created, {} memories decayed",
            agent_id, summary_id, count);
    }

    /// Get the most recent summary node for an agent (agent_summary or session_digest).
    pub async fn get_summary(&self, agent_id: &str) -> Option<GraphNode> {
        let mut candidates = Vec::new();
        for node_type in &["agent_summary", "session_digest"] {
            let results = self.silva
                .search(agent_id, 10, Some(&[node_type]))
                .await
                .unwrap_or_default();
            candidates.extend(results);
        }
        candidates.into_iter()
            .filter(|n| n.metadata.contains(&format!("\"agent_id\":\"{agent_id}\"")))
            .max_by_key(|n| n.created_at.clone())
    }

    /// Called at session end. Creates a "session_digest" node with the most
    /// relevant episodes from this session (highest weight, most recent).
    pub async fn create_session_digest(&self, agent_id: &str, session_id: &str) {
        let memories = self.get_memories(agent_id, 100).await;
        let mut recent: Vec<&GraphNode> = memories.iter()
            .filter(|n| n.node_type == "agent_memory")
            .collect();
        recent.sort_by(|a, b| {
            b.created_at.cmp(&a.created_at)
                .then_with(|| b.weight.partial_cmp(&a.weight).unwrap_or(std::cmp::Ordering::Equal))
        });
        let top: Vec<&GraphNode> = recent.into_iter().take(10).collect();
        if top.is_empty() { return; }
        let meaningful: Vec<String> = top.iter()
            .filter_map(|n| {
                let content = &n.content;
                if content.contains("├──") || content.contains("│") { return None; }
                Some(content.chars().take(120).collect::<String>())
            })
            .collect();
        if meaningful.is_empty() { return; }
        let digest = format!(
            "Sesión {} — {} episodios relevantes:\n{}",
            &session_id[..8.min(session_id.len())],
            meaningful.len(),
            meaningful.iter().enumerate()
                .map(|(i, s)| format!("{}. {}", i + 1, s))
                .collect::<Vec<_>>().join("\n")
        );
        let digest_id = format!("session_digest:{}:{}", agent_id, chrono::Utc::now().timestamp());
        let meta = serde_json::json!({
            "agent_id": agent_id,
            "session_id": session_id,
            "digest": true,
            "episode_count": meaningful.len(),
        }).to_string();
        let _ = self.silva.upsert_node_with_provenance(&digest_id, "session_digest", &digest, &meta, "agent_generated").await;
        info!("📝 Session digest created for agent '{}': {} episodes", agent_id, meaningful.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::silva::SilvaDB;

    /// End-to-end coverage for the Generative-Agents-style session summarization
    /// pipeline (record -> create_session_digest -> get_summary), which was wired
    /// end-to-end (sse.rs disconnect hook, tylluan_recall's session_context
    /// prepend in handler_recall.rs) but had zero test coverage before this.
    #[tokio::test(flavor = "multi_thread")]
    async fn session_digest_roundtrip_produces_a_retrievable_summary() {
        let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
        let mgr = AgentMemoryManager::new(silva, 1000);
        let agent_id = "test-agent-digest";

        for i in 0..5 {
            mgr.record_memory(agent_id, &format!("episode {i}: did something noteworthy"), 0.6).await;
        }

        // No digest yet — get_summary must not fabricate one.
        assert!(mgr.get_summary(agent_id).await.is_none());

        mgr.create_session_digest(agent_id, "session-abc123").await;

        let summary = mgr.get_summary(agent_id).await
            .expect("create_session_digest should produce a node get_summary can find");
        assert_eq!(summary.node_type, "session_digest");
        assert!(summary.content.contains("episodios relevantes"));
        assert!(summary.content.contains("episode"));
        assert!(summary.metadata.contains(&format!("\"agent_id\":\"{agent_id}\"")));
    }

    /// create_session_digest with no recorded memories must not create an empty
    /// or garbage digest node -- get_summary should still report "nothing yet".
    #[tokio::test(flavor = "multi_thread")]
    async fn session_digest_with_no_memories_creates_nothing() {
        let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
        let mgr = AgentMemoryManager::new(silva, 1000);
        let agent_id = "test-agent-empty";

        mgr.create_session_digest(agent_id, "session-empty").await;

        assert!(mgr.get_summary(agent_id).await.is_none());
    }

    /// get_summary must not leak another agent's digest across the agent_id
    /// boundary -- this is the same class of guarantee as the identity/
    /// impersonation checks elsewhere in the kernel.
    #[tokio::test(flavor = "multi_thread")]
    async fn session_digest_is_scoped_per_agent() {
        let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
        let mgr = AgentMemoryManager::new(silva, 1000);

        mgr.record_memory("agent-a", "agent-a's private episode", 0.6).await;
        mgr.create_session_digest("agent-a", "session-a").await;

        let summary_b = mgr.get_summary("agent-b").await;
        assert!(summary_b.is_none(), "agent-b must not see agent-a's session digest");

        let summary_a = mgr.get_summary("agent-a").await;
        assert!(summary_a.is_some());
    }

    /// Ouroboros: recording an experience and retrieving it back for the same
    /// agent, with the agent's verdict and lesson preserved.
    #[tokio::test(flavor = "multi_thread")]
    async fn experience_roundtrip_preserves_verdict_and_lesson() {
        let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
        let mgr = AgentMemoryManager::new(silva, 1000);
        let agent = "test-ouroboros";

        mgr.record_experience(
            agent,
            "run git reset --hard on shared branch",
            "lost 2 hours of uncommitted work",
            "failed",
            "stash before any destructive git op",
        ).await;

        let crit = mgr.get_relevant_critiques(agent, "git reset destructive", 5).await;
        assert!(!crit.is_empty(), "agent must be able to retrieve its own recorded experience");
        let node = &crit[0];
        assert_eq!(node.node_type, "experience");
        assert!(node.content.contains("[failed]"));
        assert!(node.content.contains("stash before"));
        assert!(node.metadata.contains(&format!("\"agent_id\":\"{agent}\"")));
    }

    /// Reflexion weighting: a failed experience outranks a successful one, so
    /// "what not to repeat" surfaces first when critiques are retrieved.
    #[tokio::test(flavor = "multi_thread")]
    async fn failed_experiences_outrank_successful_ones() {
        let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
        let mgr = AgentMemoryManager::new(silva, 1000);
        let agent = "test-weight";

        mgr.record_experience(agent, "deploy config X", "worked fine", "worked", "").await;
        mgr.record_experience(agent, "deploy config X", "broke prod", "failed", "validate config X first").await;

        let crit = mgr.get_relevant_critiques(agent, "deploy config X", 5).await;
        assert!(crit.len() >= 2);
        assert!(crit[0].content.contains("[failed]"), "the failure must surface first");
    }

    /// An agent's experiences never leak into another agent's critique lookup.
    #[tokio::test(flavor = "multi_thread")]
    async fn experiences_are_scoped_per_agent() {
        let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
        let mgr = AgentMemoryManager::new(silva, 1000);

        mgr.record_experience("agent-x", "did something", "it failed", "failed", "avoid").await;

        let for_y = mgr.get_relevant_critiques("agent-y", "did something", 5).await;
        assert!(for_y.is_empty(), "agent-y must not see agent-x's experience");

        let for_x = mgr.get_relevant_critiques("agent-x", "did something", 5).await;
        assert!(!for_x.is_empty());
    }
}
