use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: String,
    pub tool_name: String,
    pub guild_name: String,
    pub params: serde_json::Value,
    pub status: ApprovalStatus,
    pub created_at: DateTime<Utc>,
}

pub struct ApprovalManager {
    requests: Arc<RwLock<HashMap<String, ApprovalRequest>>>,
}

impl Default for ApprovalManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ApprovalManager {
    pub fn new() -> Self {
        Self {
            requests: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new approval request and return its ID
    pub fn create_request(&self, guild: &str, tool: &str, params: serde_json::Value) -> String {
        let id = Uuid::new_v4().to_string();
        let request = ApprovalRequest {
            id: id.clone(),
            tool_name: tool.to_string(),
            guild_name: guild.to_string(),
            params,
            status: ApprovalStatus::Pending,
            created_at: Utc::now(),
        };

        let mut lock = self.requests.write().unwrap_or_else(|e| {
            tracing::warn!("ApprovalManager RwLock poisoned, recovering");
            e.into_inner()
        });
        lock.insert(id.clone(), request);
        id
    }

    /// Check status of a request
    pub fn get_status(&self, id: &str) -> Option<ApprovalStatus> {
        let lock = self.requests.read().unwrap_or_else(|e| {
            tracing::warn!("ApprovalManager RwLock poisoned, recovering");
            e.into_inner()
        });
        lock.get(id).map(|r| r.status.clone())
    }

    /// Approve a request
    pub fn approve(&self, id: &str) -> bool {
        let mut lock = self.requests.write().unwrap_or_else(|e| {
            tracing::warn!("ApprovalManager RwLock poisoned, recovering");
            e.into_inner()
        });
        if let Some(req) = lock.get_mut(id)
            && req.status == ApprovalStatus::Pending {
                req.status = ApprovalStatus::Approved;
                return true;
            }
        false
    }

    /// Reject a request
    pub fn reject(&self, id: &str) -> bool {
        let mut lock = self.requests.write().unwrap_or_else(|e| {
            tracing::warn!("ApprovalManager RwLock poisoned, recovering");
            e.into_inner()
        });
        if let Some(req) = lock.get_mut(id)
            && req.status == ApprovalStatus::Pending {
                req.status = ApprovalStatus::Rejected;
                return true;
            }
        false
    }

    /// Get all pending requests
    pub fn get_pending(&self) -> Vec<ApprovalRequest> {
        let lock = self.requests.read().unwrap_or_else(|e| {
            tracing::warn!("ApprovalManager RwLock poisoned, recovering");
            e.into_inner()
        });
        lock.values()
            .filter(|r| r.status == ApprovalStatus::Pending)
            .cloned()
            .collect()
    }
}
