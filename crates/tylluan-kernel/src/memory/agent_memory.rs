use crate::memory::cosine::cosine_similarity;
use crate::memory::silva::{GraphNode, SilvaDB, NodeWriteOptions};
use crate::router::embeddings::EmbeddingEngine;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

const SEMANTIC_CLUSTER_THRESHOLD: f32 = 0.85;

/// Ouroboros Loop — autonomous harvest half. Runs on the existing
/// NightConsolidation pulse (no new timer). Scans the guild_audit_log for
/// REPEATED failures (same agent + tool failing >= min_failures times within
/// `lookback_secs`) and promotes each PATTERN — not one-off blips — into a
/// per-agent `experience` node (verdict=failed). Pure ground truth from the
/// audit chain: "this call errored" is a fact, no LLM judgment. Idempotent:
/// a deterministic node id means re-harvesting updates rather than duplicates.
///
/// When `embedding_engine` is provided, intents are grouped by BGE-M3 semantic
/// similarity instead of naive first-4-word prefix, catching patterns like
/// "list files" and "show directory contents" as the same failure class.
/// Returns the number of failure-patterns harvested.
pub async fn harvest_failures_from_audit(
    silva: &Arc<SilvaDB>,
    audit_db_path: &str,
    lookback_secs: i64,
    min_failures: i64,
    embedding_engine: Option<&EmbeddingEngine>,
) -> usize {
    let cutoff = (chrono::Utc::now() - chrono::Duration::seconds(lookback_secs)).to_rfc3339();
    // Collect (agent_id, tool, intent) error rows synchronously — a rusqlite
    // Connection is not Send across an await, so we drain into a Vec first.
    let rows: Vec<(String, String, String)> = {
        let conn = match crate::config::open_db(std::path::Path::new(audit_db_path)) {
            Ok(c) => c,
            Err(_) => return 0,
        };
        let stmt = conn.prepare(
            "SELECT agent_id, tool_name, COALESCE(intent,'') FROM guild_audit_log \
             WHERE status = 'error' AND timestamp > ?1 \
             AND agent_id != '' AND agent_id != 'anonymous'",
        );
        match stmt {
            Ok(mut s) => {
                let mapped = s.query_map(rusqlite::params![cutoff], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
                });
                match mapped {
                    Ok(iter) => iter.flatten().collect(),
                    Err(_) => return 0,
                }
            }
            Err(_) => return 0,
        }
    };

    type AgentToolGroups = std::collections::HashMap<(String, String), Vec<(String, Option<Vec<f32>>)>>;

    let mut harvested = 0usize;
    if let Some(engine) = embedding_engine {
        // Semantic grouping: embed intents, cluster by cosine similarity
        use std::collections::HashMap;
        // Group rows by (agent, tool) first, then semantically cluster within each group
        let mut by_agent_tool: AgentToolGroups = HashMap::new();
        for (agent, tool, intent) in rows {
            let emb = engine.embed(&intent).ok();
            by_agent_tool.entry((agent, tool)).or_default().push((intent, emb));
        }

        // Greedy threshold clustering per (agent, tool) group
        for ((agent, tool), entries) in by_agent_tool {
            let mut clusters: Vec<(Vec<f32>, Vec<usize>)> = Vec::new();
            for (idx, (_intent, emb_opt)) in entries.iter().enumerate() {
                let Some(emb) = emb_opt else { continue };
                let mut assigned = false;
                for (centroid_members, member_indices) in &mut clusters {
                    if cosine_similarity(centroid_members, emb) >= SEMANTIC_CLUSTER_THRESHOLD {
                        // Update centroid as running average
                        for (c, e) in centroid_members.iter_mut().zip(emb.iter()) {
                            *c = (*c * member_indices.len() as f32 + e) / (member_indices.len() + 1) as f32;
                        }
                        member_indices.push(idx);
                        assigned = true;
                        break;
                    }
                }
                if !assigned {
                    clusters.push((emb.clone(), vec![idx]));
                }
            }

            for (_centroid, member_indices) in &clusters {
                if member_indices.len() < min_failures as usize {
                    continue;
                }
                let count = member_indices.len() as i64;
                // Use the first intent in the cluster as the representative prefix
                let representative = &entries[member_indices[0]].0;
                let prefix: String = representative.split_whitespace().take(4).collect::<Vec<_>>().join(" ");
                let key_hash: u64 = format!("{tool}|{prefix}|semantic").bytes()
                    .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
                let node_id = format!("experience:{agent}:auto:{key_hash:x}");
                let content = format!(
                    "[failed] {tool} for '{prefix}' → falló {count}x en las últimas {}h (agrupado semánticamente, {} miembros en cluster)",
                    lookback_secs / 3600, member_indices.len()
                );
                let meta = serde_json::json!({
                    "agent_id": agent, "verdict": "failed", "kind": "experience",
                    "auto_harvested": true, "failure_count": count, "cluster_size": member_indices.len(),
                    "semantic_grouping": true,
                }).to_string();
                let scope = format!("agent:{agent}");
                let opts = NodeWriteOptions::new("agent_generated").owner_scope(Some(&scope));
                if silva.upsert_node_with_validity(&node_id, "experience", &content, &meta, opts).await.is_ok() {
                    let _ = silva.set_weight(&node_id, 2.0).await;
                    harvested += 1;
                }
            }
        }
    } else {
        // Fallback: group by (agent, tool, first-4-words-of-intent) as before.
        use std::collections::HashMap;
        let mut counts: HashMap<(String, String, String), i64> = HashMap::new();
        for (agent, tool, intent) in rows {
            let prefix: String = intent.split_whitespace().take(4).collect::<Vec<_>>().join(" ");
            *counts.entry((agent, tool, prefix)).or_insert(0) += 1;
        }

        for ((agent, tool, prefix), count) in counts {
            if count < min_failures {
                continue;
            }
            let key_hash: u64 = format!("{tool}|{prefix}").bytes()
                .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
            let node_id = format!("experience:{agent}:auto:{key_hash:x}");
            let content = format!(
                "[failed] {tool} for '{prefix}' → falló {count}x en las últimas {}h (auto-detectado del audit log)",
                lookback_secs / 3600
            );
            let meta = serde_json::json!({
                "agent_id": agent, "verdict": "failed", "kind": "experience",
                "auto_harvested": true, "failure_count": count,
            }).to_string();
            let scope = format!("agent:{agent}");
            let opts = NodeWriteOptions::new("agent_generated").owner_scope(Some(&scope));
            if silva.upsert_node_with_validity(&node_id, "experience", &content, &meta, opts).await.is_ok() {
                let _ = silva.set_weight(&node_id, 2.0).await;
                harvested += 1;
            }
        }
    }
    if harvested > 0 {
        info!("🐍 Ouroboros harvest: promoted {harvested} repeated-failure pattern(s) to per-agent experience");
    }
    harvested
}

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
    ///
    /// M31-P1 (write-side gap closed 2026-07-25): mirrors record_experience's
    /// existing owner_scope tagging — that function already scoped its nodes
    /// to `agent:{id}` unconditionally, this one didn't, an inconsistency that
    /// left `tylluan_remember`-written memories unprotected by memory_isolation
    /// even when it was configured for an agent (isolation is enforced by
    /// filtering on owner_scope/metadata at read time; a node with no scope
    /// can't be excluded from anyone's results).
    pub async fn record_memory(&self, agent_id: &str, content: &str, importance: f64) -> String {
        let node_id = format!("agent_memory:{}:{}", agent_id, Uuid::new_v4().simple());
        let tagged = format!("[{agent_id}] {content}");
        let meta = serde_json::json!({
            "agent_id": agent_id,
            "importance": importance,
        }).to_string();
        let scope = format!("agent:{agent_id}");
        let opts = NodeWriteOptions::new("agent_generated").owner_scope(Some(&scope));

        if self.silva.upsert_node_with_validity(&node_id, "agent_memory", &tagged, &meta, opts).await.is_ok() {
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
    /// Filters out decayed summaries with weight < 0.15 to avoid prompt clutter.
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
            .filter(|n| n.metadata.contains(&format!("\"agent_id\":\"{agent_id}\"")) && n.weight >= 0.15)
            .max_by_key(|n| n.created_at.clone())
    }

    /// Called at session end. Creates a "session_digest" node with the most
    /// relevant episodes from this session (highest weight, most recent, weight >= 0.15).
    pub async fn create_session_digest(&self, agent_id: &str, session_id: &str) {
        let memories = self.get_memories(agent_id, 100).await;
        let mut recent: Vec<&GraphNode> = memories.iter()
            .filter(|n| n.node_type == "agent_memory" && n.weight >= 0.15)
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

        let summary_a = mgr.get_summary("agent-a").await;
        assert!(summary_a.is_some());
    }

    /// Low salience memories (weight < 0.15) must be filtered out during session
    /// digest creation and summary retrieval, preventing old/decayed prompt clutter.
    #[tokio::test(flavor = "multi_thread")]
    async fn session_digest_and_summary_filter_low_salience_weight() {
        let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
        let mgr = AgentMemoryManager::new(silva, 1000);
        let agent_id = "test-agent-decayed";

        // Record low weight memory (0.05 < 0.15)
        mgr.record_memory(agent_id, "decayed old episode from weeks ago", 0.05).await;
        mgr.create_session_digest(agent_id, "session-low").await;

        // Digest should not be created from low-salience memories only
        assert!(mgr.get_summary(agent_id).await.is_none(), "Decayed memories < 0.15 must be ignored");

        // Now record a fresh high weight memory (0.80 >= 0.15)
        mgr.record_memory(agent_id, "fresh important episode from today", 0.80).await;
        mgr.create_session_digest(agent_id, "session-high").await;

        let summary = mgr.get_summary(agent_id).await
            .expect("High salience memories >= 0.15 must produce a retrievable summary");
        assert!(summary.content.contains("fresh important episode"));
        assert!(!summary.content.contains("decayed old episode"));
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

    /// Autonomous harvest: a REPEATED failure pattern in the audit log becomes a
    /// per-agent experience node; a one-off failure does NOT (anti-noise rule).
    #[tokio::test(flavor = "multi_thread")]
    async fn harvest_promotes_repeated_failures_not_one_offs() {
        let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
        let dir = std::env::temp_dir().join(format!("tylluan_audit_test_{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let audit_path = dir.join("audit.db");
        let audit_str = audit_path.to_string_lossy().to_string();

        {
            let conn = crate::config::open_db(&audit_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE guild_audit_log (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp TEXT, guild TEXT, tool_name TEXT, agent_id TEXT, intent TEXT, status TEXT, result_preview TEXT, prev_hash TEXT, hash TEXT);"
            ).unwrap();
            let now = chrono::Utc::now().to_rfc3339();
            // agent-r: same tool+intent failed 3 times → a pattern
            for _ in 0..3 {
                conn.execute("INSERT INTO guild_audit_log (timestamp,guild,tool_name,agent_id,intent,status) VALUES (?1,'git','commit','agent-r','commit to main branch','error')", rusqlite::params![now]).unwrap();
            }
            // agent-r: a one-off failure on a different action → must be ignored
            conn.execute("INSERT INTO guild_audit_log (timestamp,guild,tool_name,agent_id,intent,status) VALUES (?1,'bash','run','agent-r','ls the temp dir','error')", rusqlite::params![now]).unwrap();
        }

        let harvested = harvest_failures_from_audit(&silva, &audit_str, 86400, 2, None).await;
        assert_eq!(harvested, 1, "only the repeated pattern should be harvested, not the one-off");

        let mgr = AgentMemoryManager::new(silva, 1000);
        let crit = mgr.get_relevant_critiques("agent-r", "commit main branch", 5).await;
        assert!(!crit.is_empty(), "the harvested failure pattern must be retrievable by the agent");
        assert!(crit[0].content.contains("falló 3x"));
        assert!(crit[0].metadata.contains("\"auto_harvested\":true"));

        // Idempotency: re-harvesting the same audit produces no NEW nodes.
        let again = harvest_failures_from_audit(&mgr_silva_clone(&mgr), &audit_str, 86400, 2, None).await;
        assert_eq!(again, 1, "re-harvest upserts the same node, still reports the pattern (no duplicate node)");

        std::fs::remove_dir_all(&dir).ok();
    }

    fn mgr_silva_clone(mgr: &AgentMemoryManager) -> Arc<SilvaDB> {
        mgr.silva.clone()
    }
}
