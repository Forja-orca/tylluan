use crate::registry::tools::RiskLevel;
use super::types::{TylluanTool, ToolCategory};

// @CONTRACT: SOVEREIGN-TOOLS (CONTRACT-01)
// See CONTRACTS.md before modifying this function
// Invariant: exactly 5 sovereign tools
// Guarding test: test_sovereign_count_is_exactly_5
// DO NOT ADD tools here without CONTRACT-01 audit
impl super::TylluanServer {
    /// Build the complete tool list for the current session.
    ///
    /// Default mode: exactly 5 sovereign tools (tylluan_do, tylluan_remember, tylluan_recall, tylluan_think, tylluan_graph).
    /// Expanded mode (expose_guild_tools=true in tylluan.toml): sovereign + all kernel utility tools + all guild-provided tools.
    /// This gives agents without native tools direct access to
    /// everything without routing through tylluan_do.
    pub async fn all_tools(&self) -> Vec<rmcp::model::Tool> {
        let sovereign: &[&str] = &["tylluan_do", "tylluan_remember", "tylluan_recall", "tylluan_think", "tylluan_graph"];
        if !self.expose_guild_tools {
            return Self::kernel_tools()
                .iter()
                .filter(|t| sovereign.contains(&t.name.as_str()))
                .map(|t| t.to_mcp())
                .collect();
        }

        let mut seen = std::collections::HashSet::new();
        let mut all: Vec<rmcp::model::Tool> = Vec::new();

        for t in Self::kernel_tools() {
            let name = t.name.clone();
            seen.insert(name);
            all.push(t.to_mcp());
        }

        if let Ok(registry) = self.registry.try_read() {
            for t in registry.all_tools() {
                if seen.insert(t.name.to_string()) {
                    all.push(t);
                }
            }
        }

        all
    }

    /// Defines the built-in kernel tools using the enriched TylluanTool Registry.
    pub fn kernel_tools() -> Vec<TylluanTool> {
        vec![
            TylluanTool::new(
                "tylluan_do",
                "Execute any task using natural language. The kernel routes the intent to the right guild automatically.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "intent":   { "type": "string", "description": "What you want to do, in natural language." },
                        "guild":    { "type": "string", "description": "Optional. Force routing to a specific guild (e.g. 'bash', 'git', 'filesystem'). Skips the semantic router." },
                        "remember": { "type": "boolean", "default": false, "description": "If true, store the result in long-term memory so it can be retrieved later with tylluan_recall." },
                        "agent_id": { "type": "string", "description": "Optional. Your agent identity (e.g. 'agent-1', 'agent-2'). When set, episodes are auto-saved to your session history and retrievable later with tylluan_recall agent_id filter." },
                        "explore":  { "type": "string", "description": "Optional. Explore a branch of the fractal tool tree. Use 'explore' to discover sub-tools without executing anything. Example: 'explore memory', 'explore graph'." }
                    },
                    "required": ["intent"]
                }),
                ToolCategory::Kernel,
                RiskLevel::Medium,
            ).with_subtools(vec![
                "tylluan_ingest".into(),
                "health".into(),
                "list_available_guilds".into(),
                "request_guild".into(),
                "unload_guild".into(),
                "doctor_diagnose".into(),
                "doctor_repair".into(),
                "agent_send_mail".into(),
                "list_pending_actions".into(),
                "approve_action".into(),
                "agent_clear_bridge".into(),
                "ponder".into(),
                "nodo_send".into(),
                "nodo_inbox".into(),
                "nodo_broadcast".into(),
                "nodo_status".into(),
                "nodo_list".into(),
            ]),
            TylluanTool::new(
                "tylluan_remember",
                "Store information in long-term memory for future recall.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "content":  { "type": "string", "description": "The information to remember." },
                        "metadata": { "type": "object", "description": "Optional tags or context." },
                        "agent_id": { "type": "string", "description": "Optional. Tag this memory with your agent identity so it appears in your session history." },
                        "expires_in_days": { "type": "integer", "description": "Optional. Number of days until this memory expires. After expiry, retrieval score is penalized ×0.1.", "minimum": 1, "maximum": 3650 },
                        "explore":  { "type": "string", "description": "Optional. Discover sub-tools under this branch without executing." }
                    },
                    "required": ["content"]
                }),
                ToolCategory::Memory,
                RiskLevel::Low,
            ).with_subtools(vec![
                "memory_write".into(),
            ]),
            TylluanTool::new(
                "tylluan_recall",
                "Search long-term memory and return the most relevant stored knowledge.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query":    { "type": "string", "description": "What to search for in memory." },
                        "limit":    { "type": "number", "default": 5 },
                        "agent_id": { "type": "string", "description": "Optional. Filter recall results to episodes from this agent identity." },
                        "explore":  { "type": "string", "description": "Optional. Discover sub-tools under this branch without executing." }
                    },
                    "required": ["query"]
                }),
                ToolCategory::Memory,
                RiskLevel::Low,
            ).with_subtools(vec![
                "memory_search".into(),
                "agent_query_memory".into(),
                "agent_check_inbox".into(),
                "agent_synthesize_context".into(),
            ]),
            TylluanTool::new(
                "tylluan_think",
                "Reason over the knowledge graph and memory without side effects. Returns structured analysis: entities, relationships, and evidence. Use when you need to understand what the system knows about a topic before acting.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query":    { "type": "string", "description": "What to reason about." },
                        "depth":    { "type": "number", "description": "Graph traversal depth (1-3)", "default": 2 },
                        "chain":    { "type": "boolean", "description": "Enable multi-hop reasoning chain expansion", "default": false },
                        "agent_id": { "type": "string", "description": "Optional agent identity" },
                        "explore":  { "type": "string", "description": "Optional. Discover sub-tools under this branch without executing." }
                    },
                    "required": ["query"]
                }),
                ToolCategory::Kernel,
                RiskLevel::Low,
            ).with_subtools(vec![
                "agent_brain_report".into(),
                "reflect".into(),
            ]),
            TylluanTool::new(
                "tylluan_graph",
                "Directly manipulate the knowledge graph: add triples, query paths, list neighbors. Use when you want to explicitly record a relationship or explore the knowledge structure.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command":   { "type": "string", "enum": ["add_triple", "query_path", "list_neighbors", "stats", "retrograde_extract", "ppr"], "description": "Operation to perform. query_path returns the ordered shortest path as node IDs. retrograde_extract: background pass that extracts triples from all existing SilvaDB nodes. ppr: personalized PageRank from seed nodes." },
                        "subject":  { "type": "string", "description": "Subject for add_triple or source node for query_path" },
                        "predicate": { "type": "string", "description": "Predicate for add_triple" },
                        "object":   { "type": "string", "description": "Object for add_triple or target node for query_path" },
                        "entity":   { "type": "string", "description": "For list_neighbors" },
                        "max_depth": { "type": "integer", "default": 6, "description": "Maximum hops for query_path (capped at 12)" },
                        "limit":    { "type": "integer", "default": 50, "description": "Max nodes for retrograde_extract (default 50)" },
                        "seeds":    { "type": "array", "items": { "type": "string" }, "description": "Seed node IDs for PPR" },
                        "alpha":    { "type": "number", "default": 0.85, "description": "Damping factor for PPR" },
                        "top_k":    { "type": "integer", "default": 10, "description": "Number of PPR results to return" },
                        "explore":  { "type": "string", "description": "Optional. Discover sub-tools under this branch without executing." }
                    },
                    "required": ["command"]
                }),
                ToolCategory::Memory,
                RiskLevel::Low,
            ).with_subtools(vec![
                "graph_add_triple".into(),
                "silva_get_context".into(),
                "silva_find_clusters".into(),
                "generate_cluster_summary".into(),
            ]),
            TylluanTool::new(
                "tylluan_ingest",
                "Ingest an external MCP server from a git URL or local path, detect its type, and register it as a live guild.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "source": { "type": "string", "description": "Git URL (https/ssh) or local path to the MCP server repo." },
                        "name": { "type": "string", "description": "Short name for the new guild (e.g. 'my-tool')." },
                        "remember": { "type": "boolean", "default": false, "description": "Store ingestion episode in memory." }
                    },
                    "required": ["source", "name"]
                }),
                ToolCategory::Kernel,
                RiskLevel::Medium,
            ),
            TylluanTool::new(
                "health",
                "Check the health status of the TylluanNexus kernel and loaded guilds.",
                serde_json::json!({ "type": "object", "properties": {} }),
                ToolCategory::Kernel,
                RiskLevel::Low,
            ),
            TylluanTool::new(
                "list_available_guilds",
                "List the catalog of available guilds and their current running status.",
                serde_json::json!({ "type": "object", "properties": {} }),
                ToolCategory::Kernel,
                RiskLevel::Low,
            ),
            TylluanTool::new(
                "request_guild",
                "Request and load a guild on demand. Example: request_guild(query='bash')",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Name or semantic query of the guild to load." }
                    },
                    "required": ["query"]
                }),
                ToolCategory::Kernel,
                RiskLevel::Medium,
            ),
            TylluanTool::new(
                "unload_guild",
                "Forcefully unload a guild to free system resources.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "guildName": { "type": "string", "description": "Name of the guild to unload." }
                    },
                    "required": ["guildName"]
                }),
                ToolCategory::Kernel,
                RiskLevel::Medium,
            ),
            TylluanTool::new(
                "doctor_diagnose",
                "Perform a deep system health scan (SilvaDB, Memory, Guilds).",
                serde_json::json!({ "type": "object", "properties": {} }),
                ToolCategory::Kernel,
                RiskLevel::Low,
            ),
            TylluanTool::new(
                "doctor_repair",
                "Attempt to automatically repair a detected issue. Targets: 'guild', 'storage', 'benchmark'.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "target": { "type": "string", "description": "What to repair ('guild', 'storage', 'benchmark')" },
                        "name": { "type": "string", "description": "Specific name if target is 'guild'" }
                    },
                    "required": ["target"]
                }),
                ToolCategory::Kernel,
                RiskLevel::Medium,
            ),
            TylluanTool::new(
                "memory_write",
                "Store a document in long-term hybrid memory for semantic indexing.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "content": { "type": "string", "description": "Text to remember." },
                        "metadata": { "type": "object", "description": "Optional metadata (tags, source)." }
                    },
                    "required": ["content"]
                }),
                ToolCategory::Memory,
                RiskLevel::Low,
            ),
            TylluanTool::new(
                "memory_search",
                "Search long-term memory using hybrid semantic+keyword search (RRF).",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "limit": { "type": "number", "default": 5 }
                    },
                    "required": ["query"]
                }),
                ToolCategory::Memory,
                RiskLevel::Low,
            ),
            TylluanTool::new(
                "graph_add_triple",
                "Add a semantic Subject-Predicate-Object triple (GraphRAG) to SilvaDB.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "source": { "type": "string" },
                        "relation": { "type": "string" },
                        "target": { "type": "string" },
                        "metadata": { "type": "object" }
                    },
                    "required": ["source", "relation", "target"]
                }),
                ToolCategory::Memory,
                RiskLevel::Low,
            ),
            TylluanTool::new(
                "silva_get_context",
                "BFS traversal of the knowledge graph to find related context nodes.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "nodeId": { "type": "string" },
                        "maxDepth": { "type": "number", "default": 2 }
                    },
                    "required": ["nodeId"]
                }),
                ToolCategory::Memory,
                RiskLevel::Low,
            ),
            TylluanTool::new(
                "silva_find_clusters",
                "Find connected community clusters in the knowledge graph.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "minSize": { "type": "number", "default": 2 }
                    }
                }),
                ToolCategory::Memory,
                RiskLevel::Low,
            ),
            TylluanTool::new(
                "generate_cluster_summary",
                "Generate a cognitive summary of a cluster of knowledge nodes.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "nodeIds": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["nodeIds"]
                }),
                ToolCategory::Skill,
                RiskLevel::Low,
            ),
            TylluanTool::new(
                "agent_send_mail",
                "Send an asynchronous message/task to another Agent ID via Mailbox.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "receiverId": { "type": "string" },
                        "payload": { "type": "object" }
                    },
                    "required": ["receiverId", "payload"]
                }),
                ToolCategory::Kernel,
                RiskLevel::Low,
            ),
            TylluanTool::new(
                "agent_query_memory",
                "Query consolidated sovereign knowledge (lessons, concepts) via topic search.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Semantic query." },
                        "topic": { "type": "string", "description": "Filter by specific topic key." },
                        "limit": { "type": "number", "default": 5 }
                    }
                }),
                ToolCategory::Memory,
                RiskLevel::Low,
            ),
            TylluanTool::new(
                "agent_check_inbox",
                "Check for unread messages in the agent inbox.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agentId": { "type": "string" },
                        "markAsRead": { "type": "boolean", "default": true }
                    },
                    "required": ["agentId"]
                }),
                ToolCategory::Kernel,
                RiskLevel::Low,
            ),
            TylluanTool::new(
                "list_pending_actions",
                "List all tool calls currently awaiting human approval.",
                serde_json::json!({"type": "object", "properties": {}}),
                ToolCategory::Kernel,
                RiskLevel::Low,
            ),
            TylluanTool::new(
                "approve_action",
                "Approve or reject a pending tool call from the queue.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "requestId": { "type": "string", "description": "ID of the pending request." },
                        "approved": { "type": "boolean", "description": "True to execute, false to reject." }
                    },
                    "required": ["requestId", "approved"]
                }),
                ToolCategory::Kernel,
                RiskLevel::High,
            ),
            TylluanTool::new(
                "agent_clear_bridge",
                "Clear the volatile session memory (Session Bridge).",
                serde_json::json!({"type": "object", "properties": {}}),
                ToolCategory::Kernel,
                RiskLevel::Medium,
            ),
            TylluanTool::new(
                "agent_brain_report",
                "Generate a sovereign health report of the brain (SilvaDB stats, disk usage).",
                serde_json::json!({"type": "object", "properties": {}}),
                ToolCategory::Memory,
                RiskLevel::Low,
            ),
            TylluanTool::new(
                "agent_synthesize_context",
                "Synthesize a rich context briefing from memory, identity, and session bridge for LLM decision-making.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Context query to synthesize around." }
                    },
                    "required": ["query"]
                }),
                ToolCategory::Memory,
                RiskLevel::Low,
            ),
            TylluanTool::new(
                "ponder",
                "Allows the agent to take a moment to think and reason before acting.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "thought": { "type": "string", "description": "The current reasoning or doubt." }
                    },
                    "required": ["thought"]
                }),
                ToolCategory::Kernel,
                RiskLevel::Low,
            ),
            TylluanTool::new(
                "reflect",
                "Analyze the current session bridge and memory to identify contradictions or missing links.",
                serde_json::json!({"type": "object", "properties": {}}),
                ToolCategory::Kernel,
                RiskLevel::Low,
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::server::TylluanServer;

    #[test]
    fn fractal_sovereign_tools_have_subtools() {
        let sovereign = ["tylluan_do", "tylluan_remember", "tylluan_recall", "tylluan_think", "tylluan_graph"];
        let tools = TylluanServer::kernel_tools();
        for name in &sovereign {
            let t = tools.iter().find(|t| &t.name.as_str() == name)
                .unwrap_or_else(|| panic!("sovereign tool {} not found", name));
            assert!(!t.subtools.is_empty(), "{} must have subtools for fractal routing", name);
        }
    }

    #[test]
    fn explore_domain_memory_returns_memory_tools() {
        let tools = TylluanServer::kernel_tools();
        let domain = "memory";
        let matching: Vec<_> = tools.iter()
            .filter(|t| {
                let cat = format!("{:?}", t.category).to_lowercase();
                t.name.to_lowercase().contains(domain) || cat.contains(domain)
                    || t.subtools.iter().any(|s| s.to_lowercase().contains(domain))
            })
            .collect();
        assert!(!matching.is_empty(), "domain=memory must match at least one tool");
    }

    #[test]
    fn explore_empty_domain_returns_all_kernel_tools() {
        let tools = TylluanServer::kernel_tools();
        assert!(!tools.is_empty());
    }
}
