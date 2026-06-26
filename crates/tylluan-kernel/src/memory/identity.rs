//! # Agent Identity Manager
//!
//! Manages persistent biographical identity for agents.
//! Each agent has a unique identity node that:
//! - Is protected from biological decay
//! - Stores biographical metadata (name, role, purpose, philosophy)
//! - Can be injected into agent context on session start

use crate::memory::silva::SilvaDB;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use tracing::info;

/// Agent biographical identity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentIdentity {
    pub agent_id: String,
    pub human_name: String,
    pub role: String,
    pub purpose: String,
    pub born_at: DateTime<Utc>,
    pub philosophy: Option<String>,
}

impl AgentIdentity {
    /// Create a new agent identity
    pub fn new(agent_id: &str, human_name: &str, role: &str, purpose: &str) -> Self {
        Self {
            agent_id: agent_id.to_string(),
            human_name: human_name.to_string(),
            role: role.to_string(),
            purpose: purpose.to_string(),
            born_at: Utc::now(),
            philosophy: None,
        }
    }

    /// Get the context prompt for this identity (injected into agent system prompt)
    pub fn to_context_prompt(&self) -> String {
        format!(
            "You are {}. Your role is {}. You have been active since {}. Your current focus is: {}.",
            self.human_name,
            self.role,
            self.born_at.format("%Y-%m-%d"),
            self.purpose
        )
    }
}

/// Identity Manager - handles agent identity lifecycle
pub struct IdentityManager {
    silva: std::sync::Arc<SilvaDB>,
}

impl IdentityManager {
    pub fn new(silva: std::sync::Arc<SilvaDB>) -> Self {
        Self { silva }
    }

    /// Register or update an agent's identity
    pub async fn register_agent(&self, identity: &AgentIdentity) -> anyhow::Result<()> {
        let node_id = format!("agent:{}", identity.agent_id);
        let content = format!("{} - {}", identity.human_name, identity.role);
        let metadata = serde_json::json!({
            "agent_id": identity.agent_id,
            "human_name": identity.human_name,
            "role": identity.role,
            "purpose": identity.purpose,
            "born_at": identity.born_at.to_rfc3339(),
            "philosophy": identity.philosophy,
            "private": true
        }).to_string();

        self.silva.upsert_node(&node_id, "identity", &content, &metadata).await?;
        self.silva.set_protected(&node_id, true).await?;

        info!("📝 Identity registered for agent '{}' ({})", identity.human_name, identity.agent_id);
        Ok(())
    }

    /// Get agent identity (returns context prompt if found)
    pub async fn get_agent_context(&self, agent_id: &str) -> Option<String> {
        let node_id = format!("agent:{}", agent_id);

        if let Ok(Some(node)) = self.silva.get_node(&node_id).await {
            // Parse metadata to extract identity info
            if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&node.metadata) {
                let name = meta.get("human_name").and_then(|v| v.as_str()).unwrap_or("Unknown");
                let role = meta.get("role").and_then(|v| v.as_str()).unwrap_or("Assistant");
                let purpose = meta.get("purpose").and_then(|v| v.as_str()).unwrap_or("General assistance");
                let born = meta.get("born_at").and_then(|v| v.as_str()).unwrap_or("2026");

                return Some(format!(
                    "You are {}. Your role is {}. You have been active since {}. Your current focus is: {}.",
                    name, role, born, purpose
                ));
            }
        }
        None
    }

    /// Check if agent has identity registered
    pub async fn has_identity(&self, agent_id: &str) -> bool {
        let node_id = format!("agent:{}", agent_id);
        self.silva.get_node(&node_id).await.map(|n| n.is_some()).unwrap_or(false)
    }
}