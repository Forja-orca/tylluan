use std::collections::HashMap;
use std::sync::OnceLock;
use tokio::sync::{RwLock, oneshot};


static GRANT_REGISTRY: OnceLock<RwLock<GrantStore>> = OnceLock::new();
static GRANT_NOTIFIER: OnceLock<tokio::sync::broadcast::Sender<serde_json::Value>> = OnceLock::new();

pub type GrantId = String;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum GrantLevel {
    #[serde(rename = "this_time")]
    ThisTime,
    #[serde(rename = "this_session")]
    ThisSession,
    #[serde(rename = "always_for_guild")]
    AlwaysForGuild,
}

impl GrantLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            GrantLevel::ThisTime => "this_time",
            GrantLevel::ThisSession => "this_session",
            GrantLevel::AlwaysForGuild => "always_for_guild",
        }
    }
}

pub struct GrantRequest {
    pub guild: String,
    pub tool_name: String,
    pub agent_id: String,
    pub reason: String,
    pub arguments: serde_json::Map<String, serde_json::Value>,
    pub tx: oneshot::Sender<GrantLevel>,
    pub expires_at: tokio::time::Instant,
}

struct GrantStore {
    pending: HashMap<GrantId, GrantRequest>,
}

// M31-P2: Plan mode — stores resolved guild+tool+args for approval before execution
static PLAN_STORE: OnceLock<RwLock<HashMap<String, PendingPlan>>> = OnceLock::new();

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PendingPlan {
    pub guild: String,
    pub tool: String,
    pub args: serde_json::Map<String, serde_json::Value>,
    pub agent_id: String,
    pub intent: String,
}

pub fn init_plan_store() {
    PLAN_STORE.set(RwLock::new(HashMap::new())).ok();
}

pub async fn store_plan(id: &str, guild: &str, tool: &str, args: &serde_json::Value, agent_id: &str, intent: &str) {
    let store = PLAN_STORE.get_or_init(|| RwLock::new(HashMap::new()));
    let mut locked = store.write().await;
    locked.insert(id.to_string(), PendingPlan {
        guild: guild.to_string(),
        tool: tool.to_string(),
        args: args.as_object().cloned().unwrap_or_default(),
        agent_id: agent_id.to_string(),
        intent: intent.to_string(),
    });
}

pub async fn get_plan(id: &str) -> Option<PendingPlan> {
    let store = PLAN_STORE.get()?;
    let locked = store.read().await;
    locked.get(id).cloned()
}

pub async fn remove_plan(id: &str) -> bool {
    let Some(store) = PLAN_STORE.get() else { return false };
    let mut locked = store.write().await;
    locked.remove(id).is_some()
}

pub fn init() {
    GRANT_REGISTRY.set(RwLock::new(GrantStore {
        pending: HashMap::new(),
    })).ok();
}

pub fn set_notifier(tx: tokio::sync::broadcast::Sender<serde_json::Value>) {
    GRANT_NOTIFIER.set(tx).ok();
}

pub async fn register(request: GrantRequest) -> GrantId {
    let guild = request.guild.clone();
    let tool_name = request.tool_name.clone();
    let agent_id = request.agent_id.clone();
    let reason = request.reason.clone();

    let mut locked = GRANT_REGISTRY
        .get()
        .expect("GrantRegistry not initialized — call grants::init() during startup")
        .write()
        .await;
    let id = format!("{:08x}", rand::random::<u32>());
    locked.pending.insert(id.clone(), request);
    drop(locked);

    if let Some(tx) = GRANT_NOTIFIER.get() {
        let _ = tx.send(serde_json::json!({
            "jsonrpc": "2.0",
            "method": "grant_required",
            "params": {
                "id": id,
                "guild": guild,
                "tool_name": tool_name,
                "agent_id": agent_id,
                "reason": reason,
            }
        }));
    }

    id
}

pub async fn resolve(id: &str, level: GrantLevel) -> bool {
    let mut locked = GRANT_REGISTRY
        .get()
        .expect("GrantRegistry not initialized")
        .write()
        .await;
    if let Some(req) = locked.pending.remove(id) {
        req.tx.send(level).is_ok()
    } else {
        false
    }
}

/// Remove a grant without resolving it (receiver gets Canceled).
/// Used when the approver rejects the request.
pub async fn remove(id: &str) -> bool {
    let mut locked = GRANT_REGISTRY
        .get()
        .expect("GrantRegistry not initialized")
        .write()
        .await;
    locked.pending.remove(id).is_some()
}

pub async fn list_pending() -> Vec<serde_json::Value> {
    let locked = GRANT_REGISTRY
        .get()
        .expect("GrantRegistry not initialized")
        .read()
        .await;
    locked
        .pending
        .iter()
        .map(|(id, req)| {
            serde_json::json!({
                "id": id,
                "guild": req.guild,
                "tool_name": req.tool_name,
                "agent_id": req.agent_id,
                "reason": req.reason,
            })
        })
        .collect()
}

pub fn spawn_reaper() {
    tokio::spawn(async {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            let Some(store) = GRANT_REGISTRY.get() else { continue };
            let mut locked = store.write().await;
            let before = locked.pending.len();
            let now = tokio::time::Instant::now();
            locked.pending.retain(|_, req| req.expires_at > now);
            let reaped = before - locked.pending.len();
            if reaped > 0 {
                tracing::debug!("🗑️ GrantRegistry: reaped {} expired grants", reaped);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ensure_init() {
        if GRANT_REGISTRY.get().is_none() {
            init();
        }
    }

    #[tokio::test]
    async fn test_register_and_resolve_this_time() {
        ensure_init();
        let (tx, rx) = oneshot::channel();
        let id = register(GrantRequest {
            guild: "bash".into(),
            tool_name: "bash_execute".into(),
            agent_id: "test-agent".into(),
            reason: "blocked by policy".into(),
            arguments: serde_json::Map::new(),
            tx,
            expires_at: tokio::time::Instant::now() + tokio::time::Duration::from_secs(60),
        }).await;
        assert!(!id.is_empty());

        let resolved = resolve(&id, GrantLevel::ThisTime).await;
        assert!(resolved);

        let decision = rx.await.unwrap();
        assert_eq!(decision, GrantLevel::ThisTime);
    }

    #[tokio::test]
    async fn test_register_and_resolve_this_session() {
        ensure_init();
        let (tx, rx) = oneshot::channel();
        let id = register(GrantRequest {
            guild: "code".into(),
            tool_name: "code_analysis".into(),
            agent_id: "agent-x".into(),
            reason: "filesystem scope exceeded".into(),
            arguments: serde_json::Map::new(),
            tx,
            expires_at: tokio::time::Instant::now() + tokio::time::Duration::from_secs(60),
        }).await;

        assert!(resolve(&id, GrantLevel::ThisSession).await);
        assert_eq!(rx.await.unwrap(), GrantLevel::ThisSession);
    }

    #[tokio::test]
    async fn test_register_and_resolve_always_for_guild() {
        ensure_init();
        let (tx, rx) = oneshot::channel();
        let id = register(GrantRequest {
            guild: "docker".into(),
            tool_name: "docker_exec".into(),
            agent_id: "devops".into(),
            reason: "process execution blocked".into(),
            arguments: serde_json::Map::new(),
            tx,
            expires_at: tokio::time::Instant::now() + tokio::time::Duration::from_secs(60),
        }).await;

        assert!(resolve(&id, GrantLevel::AlwaysForGuild).await);
        assert_eq!(rx.await.unwrap(), GrantLevel::AlwaysForGuild);
    }

    #[tokio::test]
    async fn test_resolve_nonexistent_returns_false() {
        ensure_init();
        assert!(!resolve("nonexistent", GrantLevel::ThisTime).await);
    }

    #[tokio::test]
    async fn test_resolve_twice_returns_false_second_time() {
        ensure_init();
        let (tx, _rx) = oneshot::channel();
        let id = register(GrantRequest {
            guild: "bash".into(),
            tool_name: "bash_execute".into(),
            agent_id: "tester".into(),
            reason: "test".into(),
            arguments: serde_json::Map::new(),
            tx,
            expires_at: tokio::time::Instant::now() + tokio::time::Duration::from_secs(60),
        }).await;

        assert!(resolve(&id, GrantLevel::ThisTime).await);
        assert!(!resolve(&id, GrantLevel::ThisSession).await);
    }

    #[tokio::test]
    async fn test_list_pending_after_register() {
        ensure_init();
        let (tx, _rx) = oneshot::channel();
        let id = register(GrantRequest {
            guild: "websearch".into(),
            tool_name: "web_search".into(),
            agent_id: "agent-1".into(),
            reason: "network blocked".into(),
            arguments: serde_json::Map::new(),
            tx,
            expires_at: tokio::time::Instant::now() + tokio::time::Duration::from_secs(60),
        }).await;

        let pending = list_pending().await;
        let found = pending.iter().any(|v| {
            v.get("id").and_then(|i| i.as_str()) == Some(&id)
                && v.get("guild").and_then(|g| g.as_str()) == Some("websearch")
        });
        assert!(found, "registered grant must appear in list_pending");

        resolve(&id, GrantLevel::ThisTime).await;
        let pending_after = list_pending().await;
        let still_found = pending_after.iter().any(|v| {
            v.get("id").and_then(|i| i.as_str()) == Some(&id)
        });
        assert!(!still_found, "resolved grant must not appear in list_pending");
    }

    #[tokio::test]
    async fn test_cancelled_sender_returns_false() {
        ensure_init();
        let (tx, rx) = oneshot::channel();
        let id = register(GrantRequest {
            guild: "bash".into(),
            tool_name: "bash_execute".into(),
            agent_id: "anon".into(),
            reason: "test".into(),
            arguments: serde_json::Map::new(),
            tx,
            expires_at: tokio::time::Instant::now() + tokio::time::Duration::from_secs(60),
        }).await;

        // Drop the receiver to simulate cancellation
        drop(rx);

        // resolve should return false because the send failed (receiver dropped)
        assert!(!resolve(&id, GrantLevel::ThisTime).await);
    }

    #[tokio::test]
    async fn test_grant_level_as_str() {
        assert_eq!(GrantLevel::ThisTime.as_str(), "this_time");
        assert_eq!(GrantLevel::ThisSession.as_str(), "this_session");
        assert_eq!(GrantLevel::AlwaysForGuild.as_str(), "always_for_guild");
    }

    #[tokio::test]
    async fn test_reaper_removes_expired_grants() {
        ensure_init();
        let (tx1, _rx1) = oneshot::channel();
        let (tx2, _rx2) = oneshot::channel();

        let expired = tokio::time::Instant::now() - tokio::time::Duration::from_secs(1);
        let future = tokio::time::Instant::now() + tokio::time::Duration::from_secs(3600);

        let id1 = register(GrantRequest {
            guild: "bash".into(),
            tool_name: "bash_execute".into(),
            agent_id: "a".into(),
            reason: "expired".into(),
            arguments: serde_json::Map::new(),
            tx: tx1,
            expires_at: expired,
        }).await;

        let id2 = register(GrantRequest {
            guild: "code".into(),
            tool_name: "code_analysis".into(),
            agent_id: "b".into(),
            reason: "future".into(),
            arguments: serde_json::Map::new(),
            tx: tx2,
            expires_at: future,
        }).await;

        // Manually run the reaper logic
        {
            let store = GRANT_REGISTRY.get().unwrap();
            let mut locked = store.write().await;
            let now = tokio::time::Instant::now();
            locked.pending.retain(|_, req| req.expires_at > now);
        }

        let pending = list_pending().await;
        let expired_still_present = pending.iter().any(|v| {
            v.get("id").and_then(|i| i.as_str()) == Some(&id1)
        });
        assert!(!expired_still_present, "expired grant must be reaped");

        let future_still_present = pending.iter().any(|v| {
            v.get("id").and_then(|i| i.as_str()) == Some(&id2)
        });
        assert!(future_still_present, "future grant must survive reaping");
    }
}
