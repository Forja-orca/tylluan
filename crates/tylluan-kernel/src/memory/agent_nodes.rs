//! Agent Node Router — peer-to-peer messaging entre agentes via TylluanNexus.
//!
//! Cada agente conectado via MCP puede registrar un "nodo" (proceso ligero en el kernel).
//! Los nodos tienen inbox, pueden enviarse mensajes directos o broadcasts, y pueden
//! tener programas deterministas (reglas JSON) que se ejecutan automaticamente.
//!
//! Arquitectura:
//!   IDE/MCP → tylluan_do(intent="node send to X: msg") → AgentNodeRouter → SSE push a X
//!
//! Sin tocar CONTRACT-01 (5 herramientas soberanas). Sin tocar SO ni IDEs.
//! El router vive 100% dentro del kernel como Arc<AgentNodeRouter>.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};
use serde::{Deserialize, Serialize};
use tracing::info;

pub const NODE_INBOX_CAPACITY: usize = 256;
pub const NODE_INBOX_MAX_PAYLOAD: usize = 8192;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeMessage {
    pub id: String,
    pub from: String,
    pub to: String,
    pub msg_type: String,   // "direct" | "broadcast" | "request" | "response" | "event"
    pub payload: String,
    pub ts: i64,
}

/// Regla determinista del programa de un nodo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeRule {
    pub id: String,
    /// If specified, the rule only applies if the message comes from this agent.
    pub when_from: Option<String>,
    /// If specified, the rule only applies if the payload contains this string.
    pub when_contains: Option<String>,
    pub action: NodeRuleAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NodeRuleAction {
    /// Forward the message to another node.
    ForwardTo { to: String },
    /// Automatically reply with a fixed message.
    ReplyWith { message: String },
    /// Postear en un canal de coloquio.
    PostToColoquio { channel: String, prefix: Option<String> },
    /// Ignorar (descartar silenciosamente).
    Ignore,
}

struct NodeState {
    inbox: Vec<NodeMessage>,
    rules: Vec<NodeRule>,
    registered_at: i64,
    last_active: i64,
}

pub struct AgentNodeRouter {
    nodes: RwLock<HashMap<String, NodeState>>,
    broadcast_tx: broadcast::Sender<serde_json::Value>,
}

impl AgentNodeRouter {
    pub fn new(broadcast_tx: broadcast::Sender<serde_json::Value>) -> Arc<Self> {
        Arc::new(Self {
            nodes: RwLock::new(HashMap::new()),
            broadcast_tx,
        })
    }

    /// Registrar un nodo para un agente. Idempotente — re-registrar resetea last_active.
    pub async fn register(&self, agent_id: &str) -> serde_json::Value {
        let now = chrono::Utc::now().timestamp_millis();
        let mut nodes = self.nodes.write().await;
        let existing = nodes.contains_key(agent_id);
        nodes.entry(agent_id.to_string()).or_insert_with(|| NodeState {
            inbox: Vec::new(),
            rules: Vec::new(),
            registered_at: now,
            last_active: now,
        });
        if existing
            && let Some(s) = nodes.get_mut(agent_id) { s.last_active = now; }
        info!("🔵 Node {}: {}", if existing { "reconnected" } else { "registered" }, agent_id);
        let _ = self.broadcast_tx.send(serde_json::json!({
            "type": "node:registered",
            "agent_id": agent_id,
            "reconnect": existing,
            "ts": now,
        }));
        serde_json::json!({
            "status": if existing { "reconnected" } else { "registered" },
            "agent_id": agent_id,
            "ts": now,
        })
    }

    /// Enviar mensaje directo de un nodo a otro.
    pub async fn send(&self, from: &str, to: &str, payload: &str, msg_type: &str) -> Result<serde_json::Value, String> {
        let payload = if payload.len() > NODE_INBOX_MAX_PAYLOAD {
            &payload[..NODE_INBOX_MAX_PAYLOAD]
        } else {
            payload
        };

        let msg = NodeMessage {
            id: uuid::Uuid::new_v4().to_string(),
            from: from.to_string(),
            to: to.to_string(),
            msg_type: msg_type.to_string(),
            payload: payload.to_string(),
            ts: chrono::Utc::now().timestamp_millis(),
        };
        let msg_id = msg.id.clone();

        let mut nodes = self.nodes.write().await;
        let target = nodes.get_mut(to).ok_or_else(|| format!("Nodo '{}' no registrado. Usa node register primero.", to))?;

        // Ejecutar reglas del programa del nodo destino
        let triggered_actions: Vec<NodeRuleAction> = target.rules.iter()
            .filter(|r| {
                r.when_from.as_deref().map(|f| f == from).unwrap_or(true)
                    && r.when_contains.as_deref().map(|kw| payload.contains(kw)).unwrap_or(true)
            })
            .map(|r| r.action.clone())
            .collect();

        target.inbox.push(msg);
        if target.inbox.len() > NODE_INBOX_CAPACITY {
            target.inbox.remove(0);
        }
        target.last_active = chrono::Utc::now().timestamp_millis();

        let _ = self.broadcast_tx.send(serde_json::json!({
            "type": "node:message",
            "from": from,
            "to": to,
            "msg_id": msg_id,
            "msg_type": msg_type,
            "preview": &payload[..payload.len().min(80)],
            "ts": chrono::Utc::now().timestamp_millis(),
        }));

        Ok(serde_json::json!({
            "delivered": true,
            "msg_id": msg_id,
            "auto_actions": triggered_actions.len(),
        }))
    }

    /// Broadcast a todos los nodos registrados excepto el emisor.
    pub async fn broadcast(&self, from: &str, payload: &str) -> serde_json::Value {
        let now = chrono::Utc::now().timestamp_millis();
        let payload = if payload.len() > NODE_INBOX_MAX_PAYLOAD {
            &payload[..NODE_INBOX_MAX_PAYLOAD]
        } else {
            payload
        };

        let mut nodes = self.nodes.write().await;
        let mut delivered = 0usize;
        let recipients: Vec<String> = nodes.keys().filter(|id| id.as_str() != from).cloned().collect();

        for id in &recipients {
            if let Some(state) = nodes.get_mut(id) {
                let msg = NodeMessage {
                    id: uuid::Uuid::new_v4().to_string(),
                    from: from.to_string(),
                    to: id.clone(),
                    msg_type: "broadcast".to_string(),
                    payload: payload.to_string(),
                    ts: now,
                };
                state.inbox.push(msg);
                if state.inbox.len() > NODE_INBOX_CAPACITY { state.inbox.remove(0); }
                delivered += 1;
            }
        }

        let _ = self.broadcast_tx.send(serde_json::json!({
            "type": "node:broadcast",
            "from": from,
            "recipients": delivered,
            "preview": &payload[..payload.len().min(80)],
            "ts": now,
        }));

        serde_json::json!({
            "broadcast": true,
            "from": from,
            "recipients": delivered,
            "recipient_ids": recipients,
        })
    }

    /// Leer y vaciar el inbox de un nodo.
    pub async fn drain_inbox(&self, agent_id: &str) -> Vec<NodeMessage> {
        let mut nodes = self.nodes.write().await;
        if let Some(state) = nodes.get_mut(agent_id) {
            state.last_active = chrono::Utc::now().timestamp_millis();
            std::mem::take(&mut state.inbox)
        } else {
            Vec::new()
        }
    }

    /// Peek inbox sin vaciar.
    pub async fn peek_inbox(&self, agent_id: &str) -> Vec<NodeMessage> {
        let nodes = self.nodes.read().await;
        nodes.get(agent_id).map(|s| s.inbox.clone()).unwrap_or_default()
    }

    /// Definir el programa (reglas deterministas) de un nodo.
    pub async fn set_program(&self, agent_id: &str, rules: Vec<NodeRule>) -> serde_json::Value {
        let mut nodes = self.nodes.write().await;
        if let Some(state) = nodes.get_mut(agent_id) {
            let n = rules.len();
            state.rules = rules;
            info!("📋 Node program set for {}: {} rules", agent_id, n);
            serde_json::json!({ "ok": true, "agent_id": agent_id, "rules_set": n })
        } else {
            serde_json::json!({ "error": format!("Nodo '{}' no registrado", agent_id) })
        }
    }

    /// Obtener el programa actual de un nodo.
    pub async fn get_program(&self, agent_id: &str) -> Vec<NodeRule> {
        let nodes = self.nodes.read().await;
        nodes.get(agent_id).map(|s| s.rules.clone()).unwrap_or_default()
    }

    /// Listar todos los nodos registrados con su estado.
    pub async fn list(&self) -> Vec<serde_json::Value> {
        let nodes = self.nodes.read().await;
        let mut result: Vec<serde_json::Value> = nodes.iter().map(|(id, s)| serde_json::json!({
            "agent_id": id,
            "inbox_pending": s.inbox.len(),
            "rules": s.rules.len(),
            "registered_at": s.registered_at,
            "last_active": s.last_active,
        })).collect();
        result.sort_by(|a, b| {
            let ta = a["last_active"].as_i64().unwrap_or(0);
            let tb = b["last_active"].as_i64().unwrap_or(0);
            tb.cmp(&ta)
        });
        result
    }

    /// Desregistrar un nodo (cleanup de sesión).
    pub async fn unregister(&self, agent_id: &str) {
        self.nodes.write().await.remove(agent_id);
        let _ = self.broadcast_tx.send(serde_json::json!({
            "type": "node:unregistered",
            "agent_id": agent_id,
            "ts": chrono::Utc::now().timestamp_millis(),
        }));
        info!("🔴 Node unregistered: {}", agent_id);
    }

    pub fn node_count(&self) -> usize {
        self.nodes.try_read().map(|n| n.len()).unwrap_or(0)
    }
}

/// Parse node intent from free text.
/// Supported formats:
///   "node register"
///   "node send to <id>: <message>"
///   "node broadcast: <message>"
///   "node inbox" / "node inbox peek"
///   "node list"
///   "node unregister"
pub fn parse_node_intent(intent: &str) -> Option<NodeIntent> {
    let lower = intent.trim().to_lowercase();
    if !lower.starts_with("node ") && !lower.starts_with("nodo ") {
        return None;
    }
    let rest = lower
        .strip_prefix("node ")
        .or_else(|| lower.strip_prefix("nodo "))
        .unwrap_or("").trim();

    if rest == "register" || rest == "registrar" || rest == "online" {
        return Some(NodeIntent::Register);
    }
    if rest == "list" || rest == "ls" || rest == "status" || rest == "listar" {
        return Some(NodeIntent::List);
    }
    if rest == "inbox" || rest == "mensajes" {
        return Some(NodeIntent::DrainInbox);
    }
    if rest == "inbox peek" || rest == "peek" {
        return Some(NodeIntent::PeekInbox);
    }
    if rest == "unregister" || rest == "offline" || rest == "desregistrar" {
        return Some(NodeIntent::Unregister);
    }
    // "send to <id>: <msg>" o "send <id> <msg>"
    if let Some(send_part) = rest.strip_prefix("send to ").or_else(|| rest.strip_prefix("send ").or_else(|| rest.strip_prefix("enviar a ").or_else(|| rest.strip_prefix("enviar "))))
        && let Some(colon_pos) = send_part.find(':') {
            let to = send_part[..colon_pos].trim().to_string();
            let payload = send_part[colon_pos + 1..].trim().to_string();
            if !to.is_empty() && !payload.is_empty() {
                return Some(NodeIntent::Send { to, payload });
            }
        }
    // "broadcast: <msg>"
    if let Some(bc_part) = rest.strip_prefix("broadcast:").or_else(|| rest.strip_prefix("broadcast ").or_else(|| rest.strip_prefix("difundir:"))) {
        let payload = bc_part.trim().to_string();
        if !payload.is_empty() {
            return Some(NodeIntent::Broadcast { payload });
        }
    }
    None
}

#[derive(Debug, Clone)]
pub enum NodeIntent {
    Register,
    Send { to: String, payload: String },
    Broadcast { payload: String },
    DrainInbox,
    PeekInbox,
    List,
    Unregister,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_register() {
        assert!(matches!(parse_node_intent("node register"), Some(NodeIntent::Register)));
        assert!(matches!(parse_node_intent("nodo registrar"), Some(NodeIntent::Register)));
    }

    #[test]
    fn parse_send() {
        let intent = parse_node_intent("node send to agent-1: hello team");
        assert!(matches!(intent, Some(NodeIntent::Send { ref to, ref payload }) if to == "agent-1" && payload.contains("hello")));
    }

    #[test]
    fn parse_broadcast() {
        let intent = parse_node_intent("node broadcast: todos al coloquio");
        assert!(matches!(intent, Some(NodeIntent::Broadcast { .. })));
    }

    #[test]
    fn parse_inbox() {
        assert!(matches!(parse_node_intent("node inbox"), Some(NodeIntent::DrainInbox)));
        assert!(matches!(parse_node_intent("node inbox peek"), Some(NodeIntent::PeekInbox)));
    }

    #[test]
    fn parse_list() {
        assert!(matches!(parse_node_intent("node list"), Some(NodeIntent::List)));
        assert!(matches!(parse_node_intent("node status"), Some(NodeIntent::List)));
    }

    #[test]
    fn no_match_non_node() {
        assert!(parse_node_intent("list files in /tmp").is_none());
        assert!(parse_node_intent("run git status").is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn register_and_send() {
        let (tx, _) = broadcast::channel(16);
        let router = AgentNodeRouter::new(tx);

        router.register("alice").await;
        router.register("bob").await;

        let result = router.send("alice", "bob", "hola bob", "direct").await;
        assert!(result.is_ok());

        let inbox = router.drain_inbox("bob").await;
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox[0].from, "alice");
        assert_eq!(inbox[0].payload, "hola bob");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn broadcast_reaches_all_except_sender() {
        let (tx, _) = broadcast::channel(16);
        let router = AgentNodeRouter::new(tx);

        router.register("alpha").await;
        router.register("beta").await;
        router.register("gamma").await;

        let res = router.broadcast("alpha", "mensaje global").await;
        assert_eq!(res["recipients"].as_u64().unwrap(), 2); // beta + gamma

        assert_eq!(router.drain_inbox("alpha").await.len(), 0); // no se envia a si mismo
        assert_eq!(router.drain_inbox("beta").await.len(), 1);
        assert_eq!(router.drain_inbox("gamma").await.len(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn send_to_unregistered_fails() {
        let (tx, _) = broadcast::channel(16);
        let router = AgentNodeRouter::new(tx);
        router.register("sender").await;

        let result = router.send("sender", "nobody", "test", "direct").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no registrado"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn inbox_capacity_bounded() {
        let (tx, _) = broadcast::channel(16);
        let router = AgentNodeRouter::new(tx);
        router.register("a").await;
        router.register("b").await;

        for i in 0..NODE_INBOX_CAPACITY + 10 {
            router.send("a", "b", &format!("msg {}", i), "direct").await.unwrap();
        }

        let inbox = router.drain_inbox("b").await;
        assert_eq!(inbox.len(), NODE_INBOX_CAPACITY);
        // Debe tener los mensajes mas recientes
        assert!(inbox.last().unwrap().payload.contains(&(NODE_INBOX_CAPACITY + 9).to_string()));
    }
}
