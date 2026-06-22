pub mod types;
pub mod tools;
pub mod handlers;
pub mod handler_do;
pub mod handler_remember;
pub mod handler_recall;
pub mod handler_think;
pub mod handler_graph;
pub mod handler_ingest;
pub mod logic;
pub mod utils;
pub mod intent_enhancer;

use types::*;
use crate::registry::guild_process::GuildRegistry;
use crate::memory::hybrid::HybridMemory;
use crate::memory::silva::SilvaDB;
use crate::memory::mailbox::Mailbox;
use crate::memory::coloquio::ColoquioDb;
use crate::memory::agent_nodes::AgentNodeRouter;
use crate::router::matcher::GuildMatcher;
use crate::security::rate_limiter::RateLimiter;
use crate::security::circuit_breaker::CircuitBreaker;
use crate::transport::http::api_v1::api_journal::JournalDb;
use crate::doctor::Doctor;
use crate::hormones::HormoneSystem;
use crate::memory::agent_profile::AgentProfileStore;

use rmcp::{
    Error as McpError,
    model::*,
    service::{Peer, RequestContext, RoleServer},
    handler::server::ServerHandler,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use std::sync::Mutex;

use std::collections::{VecDeque, HashMap};
use crate::transport::server::handler_recall::{RecallCache, HotContext};

/// The TylluanNexus MCP server. Routes tool calls to kernel builtins or guild proxies.
#[derive(Clone)]
pub struct TylluanServer {
    peer: Arc<RwLock<Option<Peer<RoleServer>>>>,
    pub info: ServerInfo,
    pub registry: Arc<RwLock<GuildRegistry>>,
    pub matcher: Arc<GuildMatcher>,
    pub memory: Arc<HybridMemory>,
    pub silva: Arc<SilvaDB>,
    pub mailbox: Arc<Mailbox>,
    pub rate_limiter: Arc<RateLimiter>,
    pub breaker: Arc<CircuitBreaker>,
    pub doctor: Arc<Doctor>,
    pub pending_approvals: Arc<RwLock<HashMap<String, PendingAction>>>,
    pub notifier: Option<tokio::sync::broadcast::Sender<serde_json::Value>>,
    pub session_bridge: Arc<RwLock<VecDeque<BridgeEntry>>>,
    pub hormones: Arc<Mutex<HormoneSystem>>,
    pub agent_profiles: Option<Arc<Mutex<AgentProfileStore>>>,
    pub reranker: Option<Arc<crate::router::embeddings::RerankEngine>>,
    pub agent_memory: Option<Arc<crate::memory::agent_memory::AgentMemoryManager>>,
    /// Low memory mode: reduces guild timeouts by 50%.
    pub low_memory_mode: bool,
    pub recall_cache: Arc<tokio::sync::Mutex<RecallCache>>,
    pub hot_context: Arc<tokio::sync::Mutex<HotContext>>,
    pub coloquio: Option<Arc<ColoquioDb>>,
    pub node_router: Arc<AgentNodeRouter>,
    pub journal: Option<Arc<JournalDb>>,
    pub expose_guild_tools: bool,
}

impl TylluanServer {
    pub fn new(
        registry: Arc<RwLock<GuildRegistry>>,
        matcher: Arc<GuildMatcher>,
        memory: Arc<HybridMemory>,
        silva: Arc<SilvaDB>,
        mailbox: Arc<Mailbox>,
        doctor: Arc<Doctor>,
        node_router: Arc<AgentNodeRouter>,
    ) -> Self {
        let info = ServerInfo {
            server_info: Implementation {
                name: "tylluan-nexus-sovereign-hub".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability { list_changed: Some(true) }),
                prompts: Some(PromptsCapability { list_changed: Some(false) }),
                resources: Some(ResourcesCapability { subscribe: Some(false), list_changed: Some(false) }),
                ..Default::default()
            },
            ..Default::default()
        };

        Self {
            peer: Arc::new(RwLock::new(None)),
            info,
            registry,
            matcher,
            memory,
            silva,
            mailbox,
            rate_limiter: Arc::new(RateLimiter::new(Some(60))),
            breaker: Arc::new(CircuitBreaker::new()),
            doctor,
            pending_approvals: Arc::new(RwLock::new(HashMap::new())),
            notifier: None,
            session_bridge: Arc::new(RwLock::new(VecDeque::with_capacity(15))),
            hormones: Arc::new(Mutex::new(HormoneSystem::new())),
            agent_profiles: None,
            reranker: None,
            agent_memory: None,
            low_memory_mode: false,
            recall_cache: Arc::new(tokio::sync::Mutex::new(RecallCache::new(20))),
            hot_context: Arc::new(tokio::sync::Mutex::new(HotContext::new(20))),
            coloquio: None,
            node_router,
            journal: None,
            expose_guild_tools: false,
        }
    }

    pub async fn push_to_bridge(&self, guild: &str, tool: &str, output: &str) {
        let mut bridge = self.session_bridge.write().await;
        if bridge.len() >= 15 { bridge.pop_front(); }
        let truncated = if output.chars().count() > 800 { format!("{}... [TRUNCADO]", output.chars().take(800).collect::<String>()) } else { output.to_string() };
        bridge.push_back(BridgeEntry { guild: guild.to_string(), tool: tool.to_string(), output: truncated, timestamp: chrono::Local::now().to_rfc3339() });
        self.thought(&format!("💡 Session Bridge: {} Result saved.", guild), 0.9);
    }

    pub fn silva(&self) -> Arc<SilvaDB> { self.silva.clone() }
    pub fn doctor(&self) -> Arc<Doctor> { self.doctor.clone() }
    pub fn memory(&self) -> Arc<HybridMemory> { self.memory.clone() }

    pub fn set_notifier(&mut self, tx: tokio::sync::broadcast::Sender<serde_json::Value>) {
        self.notifier = Some(tx);
    }

    pub fn notify(&self, method: &str, params: serde_json::Value) {
        if let Some(tx) = &self.notifier {
            let _ = tx.send(serde_json::json!({ "jsonrpc": "2.0", "method": method, "params": params }));
        }
    }

    pub fn thought(&self, thought: &str, confidence: f32) {
        self.notify("thought", serde_json::json!({ "thought": thought, "confidence": confidence, "timestamp": chrono::Local::now().to_rfc3339() }));
    }

    pub fn edge_added(&self, source: &str, target: &str, rel_type: &str, weight: f64) {
        self.notify("edge_added", serde_json::json!({ "source": source, "target": target, "type": rel_type, "weight": weight, "ts": chrono::Utc::now().timestamp_millis() }));
    }

    pub async fn list_prompts_internal(&self, _request: PaginatedRequestParam) -> Result<ListPromptsResult, McpError> {
        Ok(ListPromptsResult {
            prompts: vec![Prompt::new(
                "tylluan_capabilities",
                Some("What TylluanNexus can do — read this before your first call to understand the 5 sovereign tools and example intents"),
                None,
            )],
            next_cursor: None,
        })
    }

    pub async fn get_prompt_internal(&self, request: GetPromptRequestParam) -> Result<GetPromptResult, McpError> {
        if request.name != "tylluan_capabilities" {
            return Err(McpError::invalid_params("unknown prompt", None));
        }
        let text = concat!(
            "# TylluanNexus — 5 Sovereign Tools\n\n",
            "## tylluan_do\n",
            "Execute any task in natural language. The kernel routes to the right guild automatically.\n",
            "Examples:\n",
            "- tylluan_do(intent='list files in /tmp')\n",
            "- tylluan_do(intent='run git status', remember=true)\n",
            "- tylluan_do(intent='create a Python virtualenv in E:/myproject', guild='bash')\n\n",
            "## tylluan_remember\n",
            "Store information in long-term memory for future recall.\n",
            "Examples:\n",
            "- tylluan_remember(content='The API key rotates every 90 days', metadata={\"tag\":\"ops\"})\n",
            "- tylluan_remember(content='User prefers concise answers', agent_id='agent-1')\n\n",
            "## tylluan_recall\n",
            "Semantic search over long-term memory. Returns ranked results with scores.\n",
            "Examples:\n",
            "- tylluan_recall(query='what did we discuss about auth?', limit=5)\n",
            "- tylluan_recall(query='deployment steps', agent_id='agent-1')\n\n",
            "## tylluan_think\n",
            "Graph-based reasoning without side effects. Returns entities, relationships, evidence.\n",
            "Use BEFORE acting when you need to understand what the system knows about a topic.\n",
            "Examples:\n",
            "- tylluan_think(query='what is the architecture of this project?', depth=2)\n",
            "- tylluan_think(query='sovereign tools contract', chain=true)\n\n",
            "## tylluan_graph\n",
            "Direct knowledge graph operations: add triples, query paths, list neighbors.\n",
            "Examples:\n",
            "- tylluan_graph(command='stats')\n",
            "- tylluan_graph(command='add_triple', subject='auth', predicate='uses', object='JWT')\n",
            "- tylluan_graph(command='list_neighbors', entity='auth')\n\n",
            "## Workflow pattern for new sessions\n",
            "1. tylluan_think(query='<topic>') — understand what is known\n",
            "2. tylluan_recall(query='<topic>') — retrieve relevant memory\n",
            "3. tylluan_do(intent='<task>') — execute with context\n",
            "4. tylluan_remember(content='<insight>', agent_id='<your-id>') — persist what matters\n"
        );
        Ok(GetPromptResult {
            description: Some("TylluanNexus sovereign tool reference and workflow patterns".into()),
            messages: vec![PromptMessage::new_text(PromptMessageRole::User, text)],
        })
    }

    pub async fn list_resources_internal(&self, _request: PaginatedRequestParam) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            resources: vec![Resource::new(
                RawResource {
                    uri: "tylluan://skills".into(),
                    name: "Tylluan Skill Catalog".into(),
                    description: Some("Example intents organized by guild — paste any of these into tylluan_do".into()),
                    mime_type: Some("text/plain".into()),
                    size: None,
                },
                None,
            )],
            next_cursor: None,
        })
    }

    pub async fn read_resource_internal(&self, request: ReadResourceRequestParam) -> Result<ReadResourceResult, McpError> {
        if request.uri != "tylluan://skills" {
            return Err(McpError::invalid_params("unknown resource uri", None));
        }
        let text = concat!(
            "# Tylluan Skill Catalog — example intents for tylluan_do\n\n",
            "## bash / shell\n",
            "- 'run ls -la in E:/myproject'\n",
            "- 'create directory E:/tmp/test'\n",
            "- 'check disk usage on C:'\n\n",
            "## git\n",
            "- 'git status in E:/TylluanMCPo3'\n",
            "- 'show last 5 commits'\n",
            "- 'diff HEAD~1'\n\n",
            "## filesystem\n",
            "- 'read file E:/TylluanMCPo3/tylluan.toml'\n",
            "- 'search for TODO in E:/TylluanMCPo3/src'\n",
            "- 'list all .rs files in crates/'\n\n",
            "## code\n",
            "- 'analyze E:/TylluanMCPo3/crates/tylluan-kernel/src/main.rs'\n",
            "- 'find all functions in handler_recall.rs'\n\n",
            "## monitor\n",
            "- 'show system resource usage'\n",
            "- 'check process list'\n\n",
            "## docker\n",
            "- 'list running containers'\n",
            "- 'show docker images'\n"
        );
        Ok(ReadResourceResult {
            contents: vec![ResourceContents::TextResourceContents {
                uri: "tylluan://skills".into(),
                mime_type: Some("text/plain".into()),
                text: text.into(),
            }],
        })
    }
}

impl ServerHandler for TylluanServer {
    fn get_info(&self) -> ServerInfo { self.info.clone() }
    fn get_peer(&self) -> Option<Peer<RoleServer>> { None }
    fn set_peer(&mut self, peer: Peer<RoleServer>) {
        let peer_store = self.peer.clone();
        tokio::spawn(async move { *peer_store.write().await = Some(peer); });
    }

    async fn list_tools(&self, _request: PaginatedRequestParam, _context: RequestContext<RoleServer>) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult { tools: self.all_tools().await, next_cursor: None })
    }

    async fn call_tool(&self, request: CallToolRequestParam, _context: RequestContext<RoleServer>) -> Result<CallToolResult, McpError> {
        self.handle_call_internal(request, tylluan_common::types::Channel::Stdio, "stdio-default").await
    }

    async fn list_prompts(&self, request: PaginatedRequestParam, _context: RequestContext<RoleServer>) -> Result<ListPromptsResult, McpError> {
        self.list_prompts_internal(request).await
    }

    async fn get_prompt(&self, request: GetPromptRequestParam, _context: RequestContext<RoleServer>) -> Result<GetPromptResult, McpError> {
        self.get_prompt_internal(request).await
    }

    async fn list_resources(&self, request: PaginatedRequestParam, _context: RequestContext<RoleServer>) -> Result<ListResourcesResult, McpError> {
        self.list_resources_internal(request).await
    }

    async fn read_resource(&self, request: ReadResourceRequestParam, _context: RequestContext<RoleServer>) -> Result<ReadResourceResult, McpError> {
        self.read_resource_internal(request).await
    }
}
