use crate::memory::silva::{GraphNode, SilvaDB};
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
        let tagged = format!("[{}] {}", agent_id, content);
        let meta = serde_json::json!({
            "agent_id": agent_id,
            "importance": importance,
        }).to_string();

        if self.silva.upsert_node(&node_id, "agent_memory", &tagged, &meta).await.is_ok() {
            let weight = importance.clamp(0.1, 5.0);
            let _ = self.silva.set_weight(&node_id, weight).await;
        }
        node_id
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
        let prefix = format!("[{}]", agent_id);
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

        let _ = self.silva.upsert_node_with_validity(&summary_id, "agent_summary", &summary, &summary_meta, None, true).await;

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
            .filter(|n| n.metadata.contains(&format!("\"agent_id\":\"{}\"", agent_id)))
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
        let _ = self.silva.upsert_node(&digest_id, "session_digest", &digest, &meta).await;
        info!("📝 Session digest created for agent '{}': {} episodes", agent_id, meaningful.len());
    }
}
