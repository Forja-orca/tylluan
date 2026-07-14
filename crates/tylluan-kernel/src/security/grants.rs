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
