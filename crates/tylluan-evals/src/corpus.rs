use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CorpusNode {
    pub id: String,
    pub content: String,
    pub node_type: String,
    pub metadata: String,
}

#[derive(Debug, Clone)]
pub struct CorpusEdge {
    pub source: String,
    pub target: String,
    pub edge_type: String,
}

#[derive(Debug, Clone)]
pub struct TestQuery {
    pub query: String,
    pub relevant_ids: Vec<String>,
    pub description: &'static str,
}

#[derive(Debug, Clone)]
pub struct SyntheticCorpus {
    pub nodes: Vec<CorpusNode>,
    pub edges: Vec<CorpusEdge>,
    pub queries: Vec<TestQuery>,
    pub contradiction_pairs: Vec<[String; 2]>,
}

pub fn build_synthetic_corpus() -> SyntheticCorpus {
    let nodes = vec![
        // == Concepts (5) ==
        CorpusNode {
            id: "concept:tylluan".into(),
            content: "TylluanNexus is an open-source sovereign MCP kernel written in Rust. It provides 5 sovereign tools to AI agents via the Model Context Protocol, enabling autonomous multi-agent coordination over a local-first cognitive stack.".into(),
            node_type: "concept".into(),
            metadata: r#"{"topic":"tylluan","weight":1.0}"#.into(),
        },
        CorpusNode {
            id: "concept:silvadb".into(),
            content: "SilvaDB is the vector-graph memory engine of TylluanNexus. It stores nodes as rows in SQLite with BGE-M3 embeddings, and edges as triple-based relationships. Search uses hybrid RRF fusion combining cosine vector similarity with SQLite LIKE text search.".into(),
            node_type: "concept".into(),
            metadata: r#"{"topic":"silvadb","weight":1.0}"#.into(),
        },
        CorpusNode {
            id: "concept:guilds".into(),
            content: "The Guild system routes agent intents to specialized Python FastMCP subprocesses via semantic matching. Each guild is an independent fastmcp server registered in the GuildRegistry. Built-in guilds include bash, git, filesystem, memory, monitor, and coloquio.".into(),
            node_type: "concept".into(),
            metadata: r#"{"topic":"guilds","weight":1.0}"#.into(),
        },
        CorpusNode {
            id: "concept:hybrid_search".into(),
            content: "tylluan_recall uses hybrid search combining vector similarity (cosine) with text search (SQLite LIKE) via Reciprocal Rank Fusion (RRF) with k=60. The fusion merges ranked lists from both search strategies into a single score-sorted result set.".into(),
            node_type: "concept".into(),
            metadata: r#"{"topic":"search","weight":1.0}"#.into(),
        },
        CorpusNode {
            id: "concept:episodic_memory".into(),
            content: "Episodic memory stores agent actions chronologically with @episode tags, enabling temporal recall across sessions. Each episode captures the agent_id, intent, result, and timestamp in a structured format that supports session-scoped retrieval.".into(),
            node_type: "concept".into(),
            metadata: r#"{"topic":"memory","weight":1.0}"#.into(),
        },
        // == Tools (4) ==
        CorpusNode {
            id: "tool:do".into(),
            content: "tylluan_do executes any task via the semantic router, dispatching natural-language intents to the appropriate guild through the GuildMatcher. Supports optional agent_id tagging and long-term memory storage.".into(),
            node_type: "tool".into(),
            metadata: r#"{"topic":"tools","weight":1.0}"#.into(),
        },
        CorpusNode {
            id: "tool:remember".into(),
            content: "tylluan_remember stores information in SilvaDB with automatic BGE-M3 embedding generation and TMS contradiction detection. New facts that contradict existing knowledge are detected via cosine similarity > 0.80 and the older version is deprecated.".into(),
            node_type: "tool".into(),
            metadata: r#"{"topic":"tools","weight":1.0}"#.into(),
        },
        CorpusNode {
            id: "tool:recall".into(),
            content: "tylluan_recall searches SilvaDB using hybrid search (vector + text RRF fusion) and returns the most relevant memories ranked by relevance score. Supports an @episode command to filter results to a specific agent's session history.".into(),
            node_type: "tool".into(),
            metadata: r#"{"topic":"tools","weight":1.0}"#.into(),
        },
        CorpusNode {
            id: "tool:think".into(),
            content: "tylluan_think performs graph analysis over the knowledge graph, finding paths, hub nodes, and contradictions using PageRank reranking on the subgraph of retrieved nodes.".into(),
            node_type: "tool".into(),
            metadata: r#"{"topic":"tools","weight":1.0}"#.into(),
        },
        // == Decisions (5) ==
        CorpusNode {
            id: "decision:local_first".into(),
            content: "The system uses local-first architecture with no cloud dependencies in the critical path. Embeddings and memory are entirely local using ONNX runtime with BGE-M3 via fastembed.".into(),
            node_type: "decision".into(),
            metadata: r#"{"topic":"architecture","weight":1.0}"#.into(),
        },
        CorpusNode {
            id: "decision:port".into(),
            content: "The kernel HTTP server runs on port 3030 by default, bound to 127.0.0.1 only. This is the single well-known port for all MCP traffic — HTTP Streamable and SSE both arrive here.".into(),
            node_type: "decision".into(),
            metadata: r#"{"topic":"architecture","weight":1.0}"#.into(),
        },
        CorpusNode {
            id: "decision:license".into(),
            content: "The entire TylluanNexus project is licensed under AGPL v3, ensuring sovereignty and preventing proprietary forks. All crates, guilds, and scripts carry the same license.".into(),
            node_type: "decision".into(),
            metadata: r#"{"topic":"legal","weight":1.0}"#.into(),
        },
        CorpusNode {
            id: "decision:tools_count".into(),
            content: "MCP clients see exactly 5 sovereign tools: tylluan_do, tylluan_remember, tylluan_recall, tylluan_think, and tylluan_graph. all_tools() in server.rs MUST filter to these 5 and nothing else.".into(),
            node_type: "decision".into(),
            metadata: r#"{"topic":"protocol","weight":1.0}"#.into(),
        },
        CorpusNode {
            id: "decision:storage".into(),
            content: "All persistent data is stored in local SQLite databases: silva.db for the knowledge graph and memory, mailbox.db for agent coloquio channels and inter-agent messages.".into(),
            node_type: "decision".into(),
            metadata: r#"{"topic":"architecture","weight":1.0}"#.into(),
        },
        // == Contradictions — planted wrong facts (5) ==
        CorpusNode {
            id: "contradiction:port_wrong".into(),
            content: "TylluanNexus runs on port 3031 by default for the kernel HTTP server. This port was chosen to avoid conflicts with common web services on port 3030.".into(),
            node_type: "memory".into(),
            metadata: r#"{"topic":"architecture","weight":1.0,"contradicts":"decision:port"}"#.into(),
        },
        CorpusNode {
            id: "contradiction:license_wrong".into(),
            content: "TylluanNexus is licensed under MIT for maximum permissiveness and broad adoption. This allows companies to integrate it freely without AGPL restrictions.".into(),
            node_type: "memory".into(),
            metadata: r#"{"topic":"legal","weight":1.0,"contradicts":"decision:license"}"#.into(),
        },
        CorpusNode {
            id: "contradiction:tools_wrong".into(),
            content: "MCP clients see exactly 7 sovereign tools: tylluan_do, tylluan_remember, tylluan_recall, tylluan_think, tylluan_graph, tylluan_query, and tylluan_plan. The extra tools provide better coverage for complex workflows.".into(),
            node_type: "memory".into(),
            metadata: r#"{"topic":"protocol","weight":1.0,"contradicts":"decision:tools_count"}"#.into(),
        },
        CorpusNode {
            id: "contradiction:storage_wrong".into(),
            content: "All data is stored in PostgreSQL for production-grade reliability and ACID compliance across distributed deployments.".into(),
            node_type: "memory".into(),
            metadata: r#"{"topic":"architecture","weight":1.0,"contradicts":"decision:storage"}"#.into(),
        },
        CorpusNode {
            id: "contradiction:bge_dim_wrong".into(),
            content: "BGE-M3 produces 2048-dimensional embeddings for maximum semantic precision and fine-grained similarity discrimination.".into(),
            node_type: "memory".into(),
            metadata: r#"{"topic":"embeddings","weight":1.0,"contradicts":"concept:silvadb"}"#.into(),
        },
        // == Additional memory nodes (6) ==
        CorpusNode {
            id: "memory:agent_alpha".into(),
            content: "Agent-Alpha is a deep learning model-powered agent running as a code extension. It is specialized in fast code edits, refactoring, and direct file manipulation.".into(),
            node_type: "memory".into(),
            metadata: r#"{"topic":"agents","weight":1.0}"#.into(),
        },
        CorpusNode {
            id: "memory:agent_beta".into(),
            content: "Agent-Claude uses a small language model for focused file edits, tests, and minor fixes. It is efficient for bounded subtasks with clear acceptance criteria.".into(),
            node_type: "memory".into(),
            metadata: r#"{"topic":"agents","weight":1.0}"#.into(),
        },
        CorpusNode {
            id: "memory:agent_gamma".into(),
            content: "Agent-Gamma is a multi-modal model-powered agent with full browser control and HTTP Streamable MCP capabilities. It handles web research and multi-agent orchestration.".into(),
            node_type: "memory".into(),
            metadata: r#"{"topic":"agents","weight":1.0}"#.into(),
        },
        CorpusNode {
            id: "memory:dashboard".into(),
            content: "The Sovereign Dashboard is a React TypeScript app at dashboard/ that provides process monitoring, guild status, and memory graph visualization in real-time. Accessible at localhost:5173 in dev mode or bundled at port 3030 in production.".into(),
            node_type: "memory".into(),
            metadata: r#"{"topic":"ui","weight":1.0}"#.into(),
        },
        CorpusNode {
            id: "memory:reranker".into(),
            content: "Jina Turbo ONNX reranker with 37 million parameters provides cross-encoder second-pass reranking on top-N candidates. It runs in approximately 60 milliseconds for top-10 results, significantly improving precision over bi-encoder cosine similarity alone.".into(),
            node_type: "memory".into(),
            metadata: r#"{"topic":"search","weight":1.0}"#.into(),
        },
        CorpusNode {
            id: "memory:coloquio".into(),
            content: "Coloquio is the inter-agent communication system using a SQLite mailbox database. Agents post messages to named channels (mision-activa, Mileston M6) and others can read the full conversation history via tylluan_recall or direct SQLite queries.".into(),
            node_type: "memory".into(),
            metadata: r#"{"topic":"communication","weight":1.0}"#.into(),
        },
    ];

    let edge_tuples: Vec<(&str, &str, &str)> = vec![
        // concept → concept relationships
        ("concept:tylluan", "concept:silvadb", "uses"),
        ("concept:tylluan", "concept:guilds", "routes_to"),
        ("concept:silvadb", "concept:hybrid_search", "implements"),
        ("concept:silvadb", "concept:episodic_memory", "stores"),
        ("concept:guilds", "concept:silvadb", "stores_in"),
        // tool → concept relationships
        ("tool:do", "concept:guilds", "dispatches_to"),
        ("tool:remember", "concept:silvadb", "writes_to"),
        ("tool:recall", "concept:hybrid_search", "uses"),
        ("tool:recall", "concept:silvadb", "queries"),
        ("tool:remember", "concept:episodic_memory", "creates"),
        ("tool:think", "concept:silvadb", "analyzes"),
        // decision → concept relationships
        ("decision:local_first", "concept:tylluan", "guides"),
        ("decision:port", "concept:tylluan", "configures"),
        ("decision:tools_count", "concept:tylluan", "constrains"),
        ("decision:storage", "concept:silvadb", "backs"),
        ("decision:license", "concept:tylluan", "protects"),
        // contradiction → decision (conflict edges)
        ("contradiction:port_wrong", "decision:port", "contradicts"),
        ("contradiction:license_wrong", "decision:license", "contradicts"),
        ("contradiction:tools_wrong", "decision:tools_count", "contradicts"),
        ("contradiction:storage_wrong", "decision:storage", "contradicts"),
        ("contradiction:bge_dim_wrong", "concept:silvadb", "contradicts"),
        // memory → concept
        ("memory:agent_alpha", "concept:tylluan", "runs_on"),
        ("memory:agent_beta", "concept:tylluan", "runs_on"),
        ("memory:agent_gamma", "concept:tylluan", "runs_on"),
        ("memory:dashboard", "concept:tylluan", "monitors"),
        ("memory:reranker", "concept:hybrid_search", "enhances"),
        ("memory:coloquio", "concept:tylluan", "enables"),
        // tool → tool
        ("tool:do", "tool:remember", "may_trigger"),
        ("tool:remember", "tool:recall", "feeds"),
        ("tool:recall", "tool:think", "inputs_to"),
        // agent → tool
        ("memory:agent_alpha", "tool:do", "uses"),
        ("memory:agent_alpha", "tool:remember", "uses"),
        ("memory:agent_alpha", "tool:recall", "uses"),
        ("memory:agent_beta", "tool:do", "uses"),
        ("memory:agent_beta", "tool:recall", "uses"),
        ("memory:agent_gamma", "tool:do", "uses"),
        ("memory:agent_gamma", "tool:recall", "uses"),
        // cross-topic edges
        ("memory:reranker", "decision:local_first", "aligns_with"),
        ("memory:coloquio", "decision:storage", "uses"),
        ("decision:local_first", "memory:reranker", "requires"),
        ("concept:episodic_memory", "memory:coloquio", "related_to"),
        ("concept:guilds", "memory:coloquio", "includes"),
        ("memory:dashboard", "memory:reranker", "displays"),
        ("concept:silvadb", "memory:agent_alpha", "used_by"),
        ("concept:silvadb", "memory:agent_gamma", "used_by"),
        ("concept:silvadb", "memory:agent_beta", "used_by"),
        // architecture cluster
        ("decision:port", "decision:local_first", "supports"),
        ("decision:storage", "decision:local_first", "supports"),
        ("decision:port", "contradiction:port_wrong", "disambiguates"),
        ("decision:license", "contradiction:license_wrong", "disambiguates"),
        ("decision:tools_count", "contradiction:tools_wrong", "disambiguates"),
        ("decision:storage", "contradiction:storage_wrong", "disambiguates"),
        ("concept:silvadb", "contradiction:bge_dim_wrong", "disambiguates"),
    ];

    let edges: Vec<CorpusEdge> = edge_tuples.into_iter().map(|(s, t, et)| CorpusEdge {
        source: s.to_string(),
        target: t.to_string(),
        edge_type: et.to_string(),
    }).collect();

    let queries = vec![
        TestQuery {
            query: "what port does the kernel use".into(),
            relevant_ids: vec!["decision:port".into()],
            description: "contradiction: correct port 3030 vs planted 3031",
        },
        TestQuery {
            query: "project license type".into(),
            relevant_ids: vec!["decision:license".into()],
            description: "contradiction: AGPL vs planted MIT",
        },
        TestQuery {
            query: "how many sovereign tools".into(),
            relevant_ids: vec!["decision:tools_count".into()],
            description: "contradiction: 5 tools vs planted 7",
        },
        TestQuery {
            query: "storage backend database".into(),
            relevant_ids: vec!["decision:storage".into()],
            description: "contradiction: SQLite vs planted PostgreSQL",
        },
        TestQuery {
            query: "what is silva memory engine".into(),
            relevant_ids: vec!["concept:silvadb".into()],
            description: "concept: SilvaDB definition",
        },
        TestQuery {
            query: "how does memory search work".into(),
            relevant_ids: vec!["concept:hybrid_search".into(), "tool:recall".into()],
            description: "concept+tool: hybrid search and recall tool",
        },
        TestQuery {
            query: "available ai agents".into(),
            relevant_ids: vec![
                "memory:agent_alpha".into(),
                "memory:agent_beta".into(),
                "memory:agent_gamma".into(),
            ],
            description: "memory: three registered agents",
        },
        TestQuery {
            query: "guild routing system".into(),
            relevant_ids: vec!["concept:guilds".into(), "tool:do".into()],
            description: "concept+tool: guild routing and do executor",
        },
        TestQuery {
            query: "what is episodic memory".into(),
            relevant_ids: vec!["concept:episodic_memory".into(), "tool:remember".into()],
            description: "concept+tool: episode storage and recall",
        },
        TestQuery {
            query: "bge dimensions".into(),
            relevant_ids: vec!["concept:silvadb".into()],
            description: "contradiction: BGE-M3 actual dims vs planted 2048",
        },
    ];

    let contradiction_pairs = vec![
        ["decision:port".into(), "contradiction:port_wrong".into()],
        ["decision:license".into(), "contradiction:license_wrong".into()],
        ["decision:tools_count".into(), "contradiction:tools_wrong".into()],
        ["decision:storage".into(), "contradiction:storage_wrong".into()],
        ["concept:silvadb".into(), "contradiction:bge_dim_wrong".into()],
    ];

    SyntheticCorpus { nodes, edges, queries, contradiction_pairs }
}
