//! M40-P2: unified agent bootstrap context.
//!
//! Before this, an agent connecting to Tylluan had to make several separate
//! calls to reconstruct "who am I, what was I doing, what's waiting on me" --
//! `whoami` for identity, `tylluan_recall` for memory, `list_pending_actions`
//! for approvals, each a round-trip. `build_agent_bootstrap` assembles all of
//! it in one call, grounded in real, already-existing pieces (IdentityManager,
//! AgentMemoryManager, security::grants) rather than inventing new storage.
use std::sync::Arc;

use crate::memory::agent_memory::AgentMemoryManager;
use crate::memory::identity::IdentityManager;
use crate::memory::silva::SilvaDB;
use crate::security::grants;
use crate::transport::http::api_v1::api_journal::{JournalDb, is_stale};

/// How many recent memories to surface. Small on purpose -- bootstrap is meant
/// to be a compact orientation, not a memory dump (the M40 vision explicitly
/// warns against volcar 60 recuerdos de baja relevancia; this pulls only the
/// agent's own memories, already scoped, not a broad recall search).
const RECENT_MEMORIES_LIMIT: usize = 5;

/// Assemble the bootstrap context for `agent_id`. Never fails outright --
/// missing pieces (no identity yet, no memories yet) show up as empty/null
/// fields rather than an error, since "you're new here" is a valid state.
pub async fn build_agent_bootstrap(silva: Arc<SilvaDB>, agent_id: &str) -> serde_json::Value {
    let identity_mgr = IdentityManager::new(silva.clone());
    let raw_identity = identity_mgr.get_identity(agent_id).await;
    let needs_real_bio = raw_identity.as_ref().map(|i| i.role == "unregistered").unwrap_or(true);
    let bio_context = identity_mgr.get_agent_context(agent_id).await;

    let mem_mgr = AgentMemoryManager::new(silva, RECENT_MEMORIES_LIMIT);
    let summary = mem_mgr.get_summary(agent_id).await;
    let recent_memories = mem_mgr.get_memories(agent_id, RECENT_MEMORIES_LIMIT).await;

    let all_pending = grants::list_pending().await;
    let my_pending: Vec<&serde_json::Value> = all_pending
        .iter()
        .filter(|g| g.get("agent_id").and_then(|v| v.as_str()) == Some(agent_id))
        .collect();

    serde_json::json!({
        "agent_id": agent_id,
        "identity": {
            "registered": raw_identity.is_some(),
            "needs_real_bio": needs_real_bio,
            "context": bio_context,
        },
        "last_session_summary": summary,
        "recent_memories": recent_memories,
        "pending_actions_for_me": my_pending,
        "register_hint": if needs_real_bio {
            Some(serde_json::json!({
                "message": "Your biography is a placeholder or unset. Call register_identity with real values.",
                "example_call": {
                    "name": "register_identity",
                    "arguments": {
                        "agent_id": agent_id,
                        "human_name": "<your display name>",
                        "role": "<your role, e.g. 'Builder Backend'>",
                        "purpose": "<your current focus, one sentence>",
                    }
                }
            }))
        } else { None },
    })
}

/// M40-P5: the single cross-client resume package. Everything `build_agent_bootstrap`
/// already assembles (identity + summary + recent memories + pending actions) plus the
/// journal's last in-progress task -- "what was I doing" -- with staleness. One
/// assembler, every client path (MCP bootstrap subtool, GET/POST /sessions/resume, CLI
/// `tylluan resume`) reads the same shape, so an agent handoff between clients never
/// loses context.
///
/// Also flattens the summary into compatibility fields (`found`/`summary`/`node_id`/
/// `node_type`/`created_at`/`weight`) so the M31-P3 CLI consumer keeps working without
/// schema churn. `last_task` is `null` when the journal has no entry -- absence is
/// explicit, never fabricated.
pub async fn build_resume_context(
    silva: Arc<SilvaDB>,
    journal: &JournalDb,
    agent_id: &str,
) -> serde_json::Value {
    let mut ctx = build_agent_bootstrap(silva, agent_id).await;

    let last_task = match journal.recover(agent_id) {
        Ok(Some(entry)) => {
            let (stale, stale_secs) = is_stale(entry.updated_at);
            Some(serde_json::json!({
                "task": entry.task,
                "updated_at_unix": entry.updated_at,
                "stale": stale,
                "stale_secs": stale_secs,
            }))
        }
        _ => None,
    };

    let summary = ctx["last_session_summary"].clone();
    let obj = ctx.as_object_mut().expect("resume context is an object");
    obj.insert("last_task".to_string(), last_task.unwrap_or(serde_json::Value::Null));

    // Compatibility flatten: the M31-P3 CLI and callers read these flat fields.
    obj.insert("agent_id".to_string(), serde_json::json!(agent_id));
    match summary.as_object() {
        Some(s) => {
            obj.insert("found".to_string(), serde_json::json!(true));
            obj.insert("summary".to_string(), s.get("content").cloned().unwrap_or(serde_json::Value::Null));
            obj.insert("node_id".to_string(), s.get("id").cloned().unwrap_or(serde_json::Value::Null));
            obj.insert("node_type".to_string(), s.get("node_type").cloned().unwrap_or(serde_json::Value::Null));
            obj.insert("created_at".to_string(), s.get("created_at").cloned().unwrap_or(serde_json::Value::Null));
            obj.insert("weight".to_string(), s.get("weight").cloned().unwrap_or(serde_json::Value::Null));
        }
        None => {
            obj.insert("found".to_string(), serde_json::json!(false));
        }
    }
    ctx
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_silva() -> Arc<SilvaDB> {
        Arc::new(SilvaDB::in_memory().await.unwrap())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn bootstrap_for_unknown_agent_is_not_an_error_and_hints_registration() {
        ensure_grants_init();
        let silva = test_silva().await;
        let ctx = build_agent_bootstrap(silva, "brand-new-agent").await;
        assert_eq!(ctx["identity"]["registered"], false);
        assert_eq!(ctx["identity"]["needs_real_bio"], true);
        assert!(ctx["register_hint"].is_object(), "a new agent must get an explicit hint to register itself");
        assert!(ctx["recent_memories"].as_array().unwrap().is_empty());
        assert!(ctx["pending_actions_for_me"].as_array().unwrap().is_empty());
    }

    fn ensure_grants_init() {
        // grants::store_plan/list_pending both panic on an uninitialized
        // registry -- init() is idempotent-safe to call more than once here
        // since each test gets its own OS thread under multi_thread flavor,
        // but the underlying OnceLock only ever initializes once globally.
        grants::init();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn bootstrap_surfaces_recent_memories_for_the_agent() {
        ensure_grants_init();
        let silva = test_silva().await;
        let mem_mgr = AgentMemoryManager::new(silva.clone(), RECENT_MEMORIES_LIMIT);
        mem_mgr.record_memory("agent-x", "decided to use approach A over B", 1.0).await;

        let ctx = build_agent_bootstrap(silva, "agent-x").await;
        let memories = ctx["recent_memories"].as_array().unwrap();
        assert!(!memories.is_empty(), "bootstrap must surface the agent's own recorded memory");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn bootstrap_only_surfaces_pending_actions_owned_by_this_agent() {
        ensure_grants_init();
        let silva = test_silva().await;
        // grants::register() (capability escalation, M30-P3) is what list_pending()
        // actually reads -- not store_plan() (M31-P2 plan mode), a separate system
        // that happens to share "pending action" vocabulary.
        let (tx_a, _rx_a) = tokio::sync::oneshot::channel();
        grants::register(grants::GrantRequest {
            guild: "bash".into(),
            tool_name: "bash_execute".into(),
            agent_id: "agent-a".into(),
            reason: "blocked by policy".into(),
            arguments: serde_json::Map::new(),
            tx: tx_a,
            expires_at: tokio::time::Instant::now() + tokio::time::Duration::from_secs(60),
        }).await;
        let (tx_b, _rx_b) = tokio::sync::oneshot::channel();
        grants::register(grants::GrantRequest {
            guild: "bash".into(),
            tool_name: "bash_execute".into(),
            agent_id: "agent-b".into(),
            reason: "blocked by policy".into(),
            arguments: serde_json::Map::new(),
            tx: tx_b,
            expires_at: tokio::time::Instant::now() + tokio::time::Duration::from_secs(60),
        }).await;

        let ctx = build_agent_bootstrap(silva, "agent-a").await;
        let pending = ctx["pending_actions_for_me"].as_array().unwrap();
        assert_eq!(pending.len(), 1, "must only see its own pending action, not agent-b's");
        assert_eq!(pending[0]["agent_id"], "agent-a");
    }

    fn test_journal() -> JournalDb {
        JournalDb::open(":memory:").unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn resume_context_is_bootstrap_superset_and_flattens_summary_for_compat() {
        ensure_grants_init();
        let silva = test_silva().await;
        let journal = test_journal();
        journal.checkin("agent-x", "tool:tylluan_do").unwrap();

        let ctx = build_resume_context(silva.clone(), &journal, "agent-x").await;

        // Every bootstrap field must survive the composition (parity: no drift
        // between what agent_bootstrap returns and what resume returns).
        let boot = build_agent_bootstrap(silva, "agent-x").await;
        for key in ["agent_id", "identity", "last_session_summary", "recent_memories", "pending_actions_for_me", "register_hint"] {
            assert!(ctx.get(key).is_some(), "resume context lost bootstrap key '{key}'");
            assert_eq!(ctx[key], boot[key], "resume context drifted from bootstrap on '{key}'");
        }

        // Compatibility flatten: M31-P3 CLI consumers read found/summary/node_id...
        assert_eq!(ctx["found"], false, "no summary recorded -> found:false, never fabricated");
        assert_eq!(ctx["last_task"]["task"], "tool:tylluan_do");
        assert!(ctx["last_task"]["updated_at_unix"].as_i64().is_some());
        assert!(ctx["last_task"]["stale"].is_boolean());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn resume_context_omits_last_task_when_journal_has_no_entry() {
        ensure_grants_init();
        let silva = test_silva().await;
        let journal = test_journal();
        let ctx = build_resume_context(silva, &journal, "agent-unknown").await;
        assert!(ctx["last_task"].is_null(), "absence must be explicit null, not fabricated");
        assert_eq!(ctx["found"], false);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn resume_context_flattens_real_summary_when_one_exists() {
        ensure_grants_init();
        let silva = test_silva().await;
        let journal = test_journal();
        let mem_mgr = AgentMemoryManager::new(silva.clone(), 20);
        mem_mgr.record_memory("agent-x", "decided to use approach A over B", 1.0).await;
        mem_mgr.create_session_digest("agent-x", "session-abc123").await;

        let ctx = build_resume_context(silva, &journal, "agent-x").await;
        assert_eq!(ctx["found"], true);
        assert!(ctx["summary"].as_str().is_some(), "summary must be the real node content");
        assert!(ctx["node_id"].as_str().is_some());
    }
}
