use serde::{Serialize, Deserialize};
use rmcp::model::*;
use crate::registry::tools::RiskLevel;
use axum::response::sse::Event;

/// Standardized SSE event taxonomy for TylluanNexus.
/// All events follow `{ "type": "...", "data": {...} }` schema.
#[derive(Serialize, Debug, Clone)]
#[serde(tag = "type", content = "data")]
pub enum NexusEvent {
    /// Tool call started
    #[serde(rename = "tool_call_start")]
    ToolCallStart {
        tool: String,
        agent_id: Option<String>,
        intent: Option<String>,
        guild: Option<String>,
    },
    /// Tool call finished
    #[serde(rename = "tool_call_end")]
    ToolCallEnd {
        tool: String,
        agent_id: Option<String>,
        duration_ms: u64,
        success: bool,
        error: Option<String>,
    },
    /// Guild status change
    #[serde(rename = "guild_status")]
    GuildStatus {
        guild: String,
        status: String, // "started", "stopped", "error", "healthy"
        pid: Option<u32>,
    },
    /// Memory index update
    #[serde(rename = "memory_index")]
    MemoryIndex {
        node_count: usize,
        edge_count: usize,
        model: String,
    },
    /// Agent thought (from hormone system)
    #[serde(rename = "thought")]
    Thought {
        content: String,
        agent_id: Option<String>,
        confidence: f32,
    },
    /// System heartbeat
    #[serde(rename = "heartbeat")]
    SystemHeart {
        uptime_secs: u64,
        active_sessions: usize,
        memory_nodes: usize,
    },
    /// Metrics update (doctor diagnostics)
    #[serde(rename = "metrics")]
    Metrics {
        cpu_percent: f32,
        memory_percent: f32,
        storage_bytes: u64,
        guilds_online: usize,
        errors_today: u32,
    },
    /// Approval required (HITL security)
    #[serde(rename = "approval_required")]
    ApprovalRequired {
        request_id: String,
        tool: String,
        risk: String,
        agent_id: String,
    },
    /// Edge added to knowledge graph
    #[serde(rename = "edge_added")]
    EdgeAdded {
        source: String,
        target: String,
        relation: String,
    },
    /// Guild execution progress
    #[serde(rename = "guild_progress")]
    GuildProgress {
        guild: String,
        task: String,
        progress: f32, // 0.0 - 1.0
    },
    /// Error notification
    #[serde(rename = "error_result")]
    ErrorResult {
        source: String,
        message: String,
        code: i32,
    },
    /// New memory node added
    #[serde(rename = "memory_added")]
    MemoryAdded {
        id: String,
        node_type: String,
        content: String,
    },
    /// Memory node updated
    #[serde(rename = "memory_updated")]
    MemoryUpdated {
        id: String,
        weight: f32,
    },
    /// Comprehensive system status update
    #[serde(rename = "system_status")]
    SystemStatus {
        silva_healthy: bool,
        mailbox_healthy: bool,
        curriculum_entries: usize,
        uptime_secs: u64,
        embeddings_loaded: bool,
        score: u8,
    },
    /// Raw passthrough event (e.g. doc:updated, doc:created)
    #[serde(rename = "raw")]
    Raw(serde_json::Value),
}

/// Convert NexusEvent to SSE Event format
impl NexusEvent {
    pub fn to_sse_event(&self) -> Event {
        match self {
            NexusEvent::Raw(value) => {
                let event_type = value.get("type").and_then(|v| v.as_str()).unwrap_or("raw");
                Event::default().event(event_type).data(value.to_string())
            }
            _ => {
                let json = serde_json::to_string(self).unwrap_or_default();
                Event::default().event("nexus").data(json)
            }
        }
    }
}


/// Sovereignty categories for tools. Helps agents discover tools by domain.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ToolCategory {
    #[serde(rename = "kernel")]
    Kernel,
    #[serde(rename = "system")]
    System,
    #[serde(rename = "file")]
    File,
    #[serde(rename = "net")]
    Network,
    #[serde(rename = "bash")]
    Bash,
    #[serde(rename = "docker")]
    Docker,
    #[serde(rename = "memory")]
    Memory,
    #[serde(rename = "skill")]
    Skill,
}

/// Enriched tool definition for TylluanNexus (Claude Code Pattern).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TylluanTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub category: ToolCategory,
    pub risk: RiskLevel,
    /// Fractal sub-tools: names of tools nested under this sovereign tool.
    /// The agent discovers them progressively via tylluan_do("explore <branch>").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subtools: Vec<String>,
}

impl TylluanTool {
    pub fn new(name: &str, description: &str, schema: serde_json::Value, category: ToolCategory, risk: RiskLevel) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            input_schema: schema,
            category,
            risk,
            subtools: Vec::new(),
        }
    }

    /// Builder: attach a list of sub-tool names that live under this tool.
    pub fn with_subtools(mut self, subtools: Vec<String>) -> Self {
        self.subtools = subtools;
        self
    }

    /// Convert to a standard MCP tool for protocol compatibility.
    pub fn to_mcp(&self) -> Tool {
        let category_tag = serde_json::to_string(&self.category).unwrap_or_default().replace('"', "").to_uppercase();
        let risk_tag = serde_json::to_string(&self.risk).unwrap_or_default().replace('"', "").to_uppercase();
        let tag = format!("[{}] [{}]", category_tag, risk_tag);
        
        let mut desc = format!("{} {}", tag, self.description);
        if !self.subtools.is_empty() {
            desc.push_str(&format!("\n🌳 SUBTOOLS: {}", self.subtools.join(", ")));
        }
        
        Tool {
            name: self.name.clone().into(),
            description: desc.into(),
            input_schema: self.input_schema.clone().as_object().cloned().unwrap_or_default().into(),
        }
    }
}

/// An entry in the session bridge, used for cross-guild collaboration.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BridgeEntry {
    pub guild: String,
    pub tool: String,
    pub output: String,
    pub timestamp: String,
}

pub struct PendingAction {
    pub name: String,
    pub arguments: Option<serde_json::Map<String, serde_json::Value>>,
    pub tx: tokio::sync::oneshot::Sender<Result<CallToolResult, rmcp::Error>>,
}
