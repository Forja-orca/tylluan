//! # Enriched Tool Registry
//!
//! Provides metadata enrichment for MCP tools, including risk levels,
//! categories, and agent-optimized descriptions.

use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    /// Read-only or safe operations (e.g., search, health, list)
    Low,
    /// Operations that modify state but are relatively safe (e.g., git commit, memory write)
    Medium,
    /// Dangerous operations (e.g., bash execute, docker run, file delete)
    High,
}

impl RiskLevel {
    pub fn as_emoji(&self) -> &'static str {
        match self {
            RiskLevel::Low => "🟢",
            RiskLevel::Medium => "🟡",
            RiskLevel::High => "🔴",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetadata {
    pub category: String,
    pub risk_level: RiskLevel,
    pub enriched_description: String,
}

/// Master Metadata Map for core Tylluan tools.
/// Hardcoded for performance and sovereign integrity.
pub static TOOL_METADATA: LazyLock<HashMap<String, ToolMetadata>> = LazyLock::new(|| {
    let mut m = HashMap::new();

    // --- System & Execution ---
    m.insert("bash_execute".to_string(), ToolMetadata {
        category: "system".to_string(),
        risk_level: RiskLevel::High,
        enriched_description: "Execute arbitrary commands in the host system. RISK: Can delete files or install software. Use only for specific tasks requested by the human.".to_string(),
    });

    // --- Knowledge & Memory ---
    m.insert("memory_search".to_string(), ToolMetadata {
        category: "memory".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Perform semantic search in SilvaDB (Tylluan's long-term memory). Safely retrieves project context and lessons.".to_string(),
    });
    m.insert("memory_write".to_string(), ToolMetadata {
        category: "memory".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Store a new document or lesson in SilvaDB. Categorize correctly to ensure future retrieval.".to_string(),
    });
    m.insert("graph_add_triple".to_string(), ToolMetadata {
        category: "memory".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Add a semantic link (S-P-O) to the Knowledge Graph. Use for representing complex relationships.".to_string(),
    });

    // --- Filesystem ---
    m.insert("read_file".to_string(), ToolMetadata {
        category: "filesystem".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Read file contents. Safe for auditing and analysis.".to_string(),
    });
    m.insert("write_file".to_string(), ToolMetadata {
        category: "filesystem".to_string(),
        risk_level: RiskLevel::High,
        enriched_description: "Overwrite or create a file. RISK: Can break build or logic if used carelessly.".to_string(),
    });

    // --- Research & Search ---
    m.insert("search_query".to_string(), ToolMetadata {
        category: "research".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Search the web or local indexes for information. Safe and recommended for ground-truth verification.".to_string(),
    });

    // --- System Metrics (New Guild) ---
    m.insert("system_cpu".to_string(), ToolMetadata {
        category: "monitoring".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Get CPU usage percentage. Read-only, safe.".to_string(),
    });
    m.insert("system_memory".to_string(), ToolMetadata {
        category: "monitoring".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Get memory usage details. Read-only, safe.".to_string(),
    });
    m.insert("system_metrics".to_string(), ToolMetadata {
        category: "monitoring".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Get complete system status (CPU, Memory, Disk). Read-only, safe.".to_string(),
    });

    // --- Code Analysis (New Guild) ---
    m.insert("analyze_python".to_string(), ToolMetadata {
        category: "analysis".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Analyze Python file structure (functions, classes, imports). Read-only, safe.".to_string(),
    });
    m.insert("count_code_lines".to_string(), ToolMetadata {
        category: "analysis".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Count code lines in a file. Read-only, safe.".to_string(),
    });

    // --- Browser (Lazy Guild) ---
    m.insert("browser_navigate".to_string(), ToolMetadata {
        category: "browser".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Open URL in system browser. No code execution, safe.".to_string(),
    });
    m.insert("search_web".to_string(), ToolMetadata {
        category: "browser".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Search web via browser. Safe operation.".to_string(),
    });
    m.insert("browser_tabs".to_string(), ToolMetadata {
        category: "browser".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "List open browser tabs. Read-only, safe.".to_string(),
    });
    m.insert("browser_screenshot".to_string(), ToolMetadata {
        category: "browser".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Capture a screenshot of the current page. Safe operation.".to_string(),
    });
    m.insert("browser_status".to_string(), ToolMetadata {
        category: "browser".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Check browser availability and debugging status. Safe operation.".to_string(),
    });

    // --- Docker (Lazy Guild) ---
    m.insert("docker_ps".to_string(), ToolMetadata {
        category: "container".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "List Docker containers. Read-only, safe.".to_string(),
    });
    m.insert("docker_run".to_string(), ToolMetadata {
        category: "container".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Run a Docker container. Medium risk - can consume resources.".to_string(),
    });
    m.insert("docker_stop".to_string(), ToolMetadata {
        category: "container".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Stop a Docker container. Medium risk.".to_string(),
    });
    m.insert("docker_exec".to_string(), ToolMetadata {
        category: "container".to_string(),
        risk_level: RiskLevel::High,
        enriched_description: "Execute a command inside a running container. RISK: Can modify container state or access sensitive data within the container.".to_string(),
    });
    m.insert("docker_status".to_string(), ToolMetadata {
        category: "container".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Get Docker daemon status and info. Safe, read-only.".to_string(),
    });
    m.insert("docker_images".to_string(), ToolMetadata {
        category: "container".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "List available Docker images. Safe, read-only.".to_string(),
    });

    // --- PDF (Lazy Guild) ---
    m.insert("pdf_extract_text".to_string(), ToolMetadata {
        category: "document".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Extract text from PDF. Read-only operation.".to_string(),
    });
    m.insert("pdf_info".to_string(), ToolMetadata {
        category: "document".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Get PDF metadata. Read-only operation.".to_string(),
    });
    m.insert("pdf_merge".to_string(), ToolMetadata {
        category: "document".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Merge multiple PDF files into one.".to_string(),
    });

    // --- Clipboard ---
    m.insert("clipboard_read".to_string(), ToolMetadata {
        category: "system".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Read text from the system clipboard.".to_string(),
    });
    m.insert("clipboard_write".to_string(), ToolMetadata {
        category: "system".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Write text to the system clipboard. Overwrites previous content.".to_string(),
    });

    // --- Audio ---
    m.insert("audio_transcribe".to_string(), ToolMetadata {
        category: "media".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Transcribe audio file to text. Read-only operation.".to_string(),
    });

    // --- Git ---
    m.insert("git_status".to_string(), ToolMetadata {
        category: "vcs".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Show working tree status. Read-only.".to_string(),
    });
    m.insert("git_status_quick".to_string(), ToolMetadata {
        category: "vcs".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Quick git status summary. Read-only.".to_string(),
    });
    m.insert("git_status_shell".to_string(), ToolMetadata {
        category: "vcs".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Git status via shell. Read-only.".to_string(),
    });
    m.insert("git_diff".to_string(), ToolMetadata {
        category: "vcs".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Show file differences. Read-only.".to_string(),
    });
    m.insert("git_add".to_string(), ToolMetadata {
        category: "vcs".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Stage files for commit. Mutates staging area.".to_string(),
    });
    m.insert("git_log".to_string(), ToolMetadata {
        category: "vcs".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Show commit history. Read-only.".to_string(),
    });
    m.insert("git_commit".to_string(), ToolMetadata {
        category: "vcs".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Create a commit. Mutates repository history.".to_string(),
    });
    m.insert("git_branch".to_string(), ToolMetadata {
        category: "vcs".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Create, list, or switch branches. Mutates branch state.".to_string(),
    });

    // --- Code (edit/generate guild) ---
    m.insert("code_parse".to_string(), ToolMetadata {
        category: "analysis".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Parse source code structure. Read-only.".to_string(),
    });
    m.insert("code_stats".to_string(), ToolMetadata {
        category: "analysis".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Compute code statistics. Read-only.".to_string(),
    });
    m.insert("code_todos".to_string(), ToolMetadata {
        category: "analysis".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Extract TODO/FIXME comments from code. Read-only.".to_string(),
    });
    m.insert("code_security_scan".to_string(), ToolMetadata {
        category: "analysis".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Scan code for security issues. Read-only.".to_string(),
    });

    // --- Code Analysis (extended) ---
    m.insert("search_codebase".to_string(), ToolMetadata {
        category: "analysis".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Search across the codebase for patterns. Read-only.".to_string(),
    });
    m.insert("git_context".to_string(), ToolMetadata {
        category: "analysis".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Get git context for a file or range. Read-only.".to_string(),
    });
    m.insert("find_dead_code".to_string(), ToolMetadata {
        category: "analysis".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Detect unused code. Read-only analysis.".to_string(),
    });

    // --- Code Graph ---
    m.insert("analyze_file".to_string(), ToolMetadata {
        category: "analysis".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Analyze file dependencies and call graph. Read-only.".to_string(),
    });
    m.insert("analyze_repo".to_string(), ToolMetadata {
        category: "analysis".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Analyze full repository dependency graph. Read-only.".to_string(),
    });

    // --- Code Reviewer ---
    m.insert("review_code".to_string(), ToolMetadata {
        category: "analysis".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Automated code review. Read-only analysis.".to_string(),
    });
    m.insert("suggest_refactoring".to_string(), ToolMetadata {
        category: "analysis".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Suggest refactoring improvements. Read-only suggestions.".to_string(),
    });
    m.insert("check_coverage".to_string(), ToolMetadata {
        category: "analysis".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Check test coverage. Read-only.".to_string(),
    });

    // --- Coloquio ---
    m.insert("read_channel".to_string(), ToolMetadata {
        category: "communication".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Read messages from a Coloquio channel. Read-only.".to_string(),
    });
    m.insert("post_to_channel".to_string(), ToolMetadata {
        category: "communication".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Post a message to a Coloquio channel. Mutates channel state.".to_string(),
    });
    m.insert("search_channel".to_string(), ToolMetadata {
        category: "communication".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Search messages in a Coloquio channel. Read-only.".to_string(),
    });
    m.insert("get_turn".to_string(), ToolMetadata {
        category: "communication".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Get a specific Coloquio turn by ID. Read-only.".to_string(),
    });
    m.insert("whats_new".to_string(), ToolMetadata {
        category: "communication".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Get recent activity summary. Read-only.".to_string(),
    });
    m.insert("list_channels".to_string(), ToolMetadata {
        category: "communication".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "List available Coloquio channels. Read-only.".to_string(),
    });
    m.insert("create_channel".to_string(), ToolMetadata {
        category: "communication".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Create a new Coloquio channel. Mutates channel state.".to_string(),
    });
    m.insert("post_to_coloquio".to_string(), ToolMetadata {
        category: "communication".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Post a message to Coloquio. Mutates channel state.".to_string(),
    });

    // --- Coloquio Digest ---
    m.insert("digest_channel".to_string(), ToolMetadata {
        category: "communication".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Generate digest summary of a Coloquio channel. Read-only.".to_string(),
    });
    m.insert("digest_all_channels".to_string(), ToolMetadata {
        category: "communication".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Generate digest summary of all channels. Read-only.".to_string(),
    });
    m.insert("auto_reason_cycle".to_string(), ToolMetadata {
        category: "communication".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Run automated reasoning cycle over channel content. Read-only analysis.".to_string(),
    });
    m.insert("digest_status".to_string(), ToolMetadata {
        category: "communication".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Check digest generation status. Read-only.".to_string(),
    });

    // --- ComfyUI ---
    m.insert("generate_image".to_string(), ToolMetadata {
        category: "media".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Generate image via ComfyUI workflow. Consumes GPU resources.".to_string(),
    });
    m.insert("img2img".to_string(), ToolMetadata {
        category: "media".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Image-to-image transformation via ComfyUI. Consumes GPU resources.".to_string(),
    });
    m.insert("get_node_schema".to_string(), ToolMetadata {
        category: "media".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Get ComfyUI node schema. Read-only.".to_string(),
    });
    m.insert("comfy_status".to_string(), ToolMetadata {
        category: "media".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Check ComfyUI server status. Read-only.".to_string(),
    });
    m.insert("generate_short".to_string(), ToolMetadata {
        category: "media".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Generate short video via ComfyUI. Consumes GPU resources.".to_string(),
    });
    m.insert("generate_documentary_video".to_string(), ToolMetadata {
        category: "media".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Generate documentary-style video. Heavy GPU compute.".to_string(),
    });
    m.insert("generate_wan_video".to_string(), ToolMetadata {
        category: "media".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Generate video via Wan model. Heavy GPU compute.".to_string(),
    });

    // --- Coordinator ---
    m.insert("coordinate".to_string(), ToolMetadata {
        category: "orchestration".to_string(),
        risk_level: RiskLevel::High,
        enriched_description: "Decompose and orchestrate multi-step tasks. May invoke arbitrary guilds.".to_string(),
    });

    // --- Cron Scheduler ---
    m.insert("cron_schedule".to_string(), ToolMetadata {
        category: "automation".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Create a recurring cron job. Mutates scheduler state.".to_string(),
    });
    m.insert("cron_list".to_string(), ToolMetadata {
        category: "automation".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "List active cron jobs. Read-only.".to_string(),
    });
    m.insert("cron_cancel".to_string(), ToolMetadata {
        category: "automation".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Cancel a cron job. Mutates scheduler state.".to_string(),
    });

    // --- Data Tools ---
    m.insert("json_query".to_string(), ToolMetadata {
        category: "data".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Query JSON data structures. Read-only.".to_string(),
    });
    m.insert("yaml_merge".to_string(), ToolMetadata {
        category: "data".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Merge YAML files. Produces output file.".to_string(),
    });
    m.insert("csv_query".to_string(), ToolMetadata {
        category: "data".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Query CSV data. Read-only.".to_string(),
    });

    // --- Database ---
    m.insert("db_query".to_string(), ToolMetadata {
        category: "data".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Execute read-only SQL query. Safe for inspection.".to_string(),
    });
    m.insert("db_list_tables".to_string(), ToolMetadata {
        category: "data".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "List database tables. Read-only.".to_string(),
    });
    m.insert("db_schema".to_string(), ToolMetadata {
        category: "data".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Show table schema. Read-only.".to_string(),
    });
    m.insert("db_export_csv".to_string(), ToolMetadata {
        category: "data".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Export query results to CSV. Read-only data, writes output file.".to_string(),
    });
    m.insert("db_execute_write".to_string(), ToolMetadata {
        category: "data".to_string(),
        risk_level: RiskLevel::High,
        enriched_description: "Execute write SQL (INSERT/UPDATE/DELETE). RISK: Can mutate or destroy data.".to_string(),
    });

    // --- Deep Analysis ---
    m.insert("sovereign_structural_mapper".to_string(), ToolMetadata {
        category: "analysis".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Map structural relationships in documents. Read-only analysis.".to_string(),
    });
    m.insert("get_impact_radius".to_string(), ToolMetadata {
        category: "analysis".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Calculate impact radius of a code change. Read-only analysis.".to_string(),
    });

    // --- Deep Web Research ---
    m.insert("fetch_page".to_string(), ToolMetadata {
        category: "research".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Fetch and extract web page content. Read-only network access.".to_string(),
    });
    m.insert("research_topic".to_string(), ToolMetadata {
        category: "research".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Multi-hop web research on a topic. Read-only network access.".to_string(),
    });

    // --- Docker (extended) ---
    m.insert("docker_logs".to_string(), ToolMetadata {
        category: "container".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "View Docker container logs. Read-only.".to_string(),
    });

    // --- FFmpeg ---
    m.insert("media_probe".to_string(), ToolMetadata {
        category: "media".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Probe media file metadata. Read-only.".to_string(),
    });
    m.insert("video_trim".to_string(), ToolMetadata {
        category: "media".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Trim video segment. Produces output file.".to_string(),
    });
    m.insert("video_concat".to_string(), ToolMetadata {
        category: "media".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Concatenate video segments. Produces output file.".to_string(),
    });
    m.insert("video_resize".to_string(), ToolMetadata {
        category: "media".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Resize video dimensions. Produces output file.".to_string(),
    });
    m.insert("audio_extract".to_string(), ToolMetadata {
        category: "media".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Extract audio track from video. Read-only source, writes output.".to_string(),
    });
    m.insert("video_add_text".to_string(), ToolMetadata {
        category: "media".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Overlay text on video. Produces output file.".to_string(),
    });

    // --- Filesystem (extended, names match actual Python functions) ---
    m.insert("file_read".to_string(), ToolMetadata {
        category: "filesystem".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Read file contents. Safe for auditing and analysis.".to_string(),
    });
    m.insert("file_write".to_string(), ToolMetadata {
        category: "filesystem".to_string(),
        risk_level: RiskLevel::High,
        enriched_description: "Overwrite or create a file. RISK: Can break build or logic if used carelessly.".to_string(),
    });
    m.insert("file_search".to_string(), ToolMetadata {
        category: "filesystem".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Search file contents by pattern. Read-only.".to_string(),
    });
    m.insert("file_list".to_string(), ToolMetadata {
        category: "filesystem".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "List directory contents. Read-only.".to_string(),
    });

    // --- Formatter ---
    m.insert("format_code".to_string(), ToolMetadata {
        category: "code_quality".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Format code files with Ruff/Prettier/Rustfmt. Mutates file content in-place.".to_string(),
    });

    // --- Ingest ---
    m.insert("ingest_file".to_string(), ToolMetadata {
        category: "data".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Ingest a file into SilvaDB memory. Mutates memory state.".to_string(),
    });
    m.insert("ingest_directory".to_string(), ToolMetadata {
        category: "data".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Ingest all files in a directory into SilvaDB. Batch memory mutation.".to_string(),
    });
    m.insert("list_allowed_types".to_string(), ToolMetadata {
        category: "data".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "List file types allowed for ingestion. Read-only.".to_string(),
    });

    // --- Knowledge ---
    m.insert("extract_triples".to_string(), ToolMetadata {
        category: "memory".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Extract knowledge triples from text. Read-only analysis.".to_string(),
    });

    // --- Llama Backend ---
    m.insert("list_models".to_string(), ToolMetadata {
        category: "inference".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "List available local LLM models. Read-only.".to_string(),
    });
    m.insert("query_model".to_string(), ToolMetadata {
        category: "inference".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Query a local LLM model. Read-only inference.".to_string(),
    });
    m.insert("backend_health".to_string(), ToolMetadata {
        category: "inference".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Check llama-server health status. Read-only.".to_string(),
    });

    // --- Local LLM Proxy ---
    m.insert("llm_chat".to_string(), ToolMetadata {
        category: "inference".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Proxy chat request to external LLM (Ollama/vLLM). Read-only network access.".to_string(),
    });
    m.insert("llm_list_models".to_string(), ToolMetadata {
        category: "inference".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "List models on external LLM server. Read-only.".to_string(),
    });

    // --- MCP Bridge ---
    m.insert("mcp_list_tools".to_string(), ToolMetadata {
        category: "integration".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "List tools on a connected MCP server. Read-only.".to_string(),
    });
    m.insert("mcp_call".to_string(), ToolMetadata {
        category: "integration".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Call a tool on an external MCP server. Side effects depend on target tool.".to_string(),
    });
    m.insert("mcp_ping".to_string(), ToolMetadata {
        category: "integration".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Ping an MCP server for connectivity check. Read-only.".to_string(),
    });

    // --- Memory (extended) ---
    m.insert("memory_status".to_string(), ToolMetadata {
        category: "memory".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Check memory system status and statistics. Read-only.".to_string(),
    });

    // --- Monitor ---
    m.insert("system_info".to_string(), ToolMetadata {
        category: "monitoring".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Get system information. Read-only.".to_string(),
    });
    m.insert("process_list".to_string(), ToolMetadata {
        category: "monitoring".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "List running processes. Read-only.".to_string(),
    });
    m.insert("network_stats".to_string(), ToolMetadata {
        category: "monitoring".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Get network statistics. Read-only.".to_string(),
    });

    // --- n8n Bridge ---
    m.insert("list_workflows".to_string(), ToolMetadata {
        category: "automation".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "List n8n workflows. Read-only.".to_string(),
    });
    m.insert("execute_workflow".to_string(), ToolMetadata {
        category: "automation".to_string(),
        risk_level: RiskLevel::High,
        enriched_description: "Execute an n8n workflow. RISK: Can trigger arbitrary external automations.".to_string(),
    });
    m.insert("kernel_pulse".to_string(), ToolMetadata {
        category: "automation".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Check n8n-kernel connectivity. Read-only heartbeat.".to_string(),
    });
    m.insert("trigger_webhook".to_string(), ToolMetadata {
        category: "automation".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Trigger an n8n webhook. Can initiate external workflows.".to_string(),
    });
    m.insert("get_execution_status".to_string(), ToolMetadata {
        category: "automation".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Check n8n workflow execution status. Read-only.".to_string(),
    });
    m.insert("n8n_status".to_string(), ToolMetadata {
        category: "automation".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Check n8n server status. Read-only.".to_string(),
    });

    // --- Night Reasoner ---
    m.insert("analyze_feedback".to_string(), ToolMetadata {
        category: "memory".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Analyze feedback for memory consolidation. Read-only analysis.".to_string(),
    });
    m.insert("reason_about".to_string(), ToolMetadata {
        category: "memory".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Reason about a topic using memory context. Read-only analysis.".to_string(),
    });
    m.insert("route_intent".to_string(), ToolMetadata {
        category: "memory".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Route an intent to the appropriate memory system. Read-only.".to_string(),
    });

    // --- Sandbox ---
    m.insert("sandbox_run".to_string(), ToolMetadata {
        category: "system".to_string(),
        risk_level: RiskLevel::High,
        enriched_description: "Execute code in sandboxed container. RISK: Arbitrary code execution within sandbox.".to_string(),
    });
    m.insert("sandbox_python".to_string(), ToolMetadata {
        category: "system".to_string(),
        risk_level: RiskLevel::High,
        enriched_description: "Execute Python in sandboxed container. RISK: Arbitrary code execution within sandbox.".to_string(),
    });
    m.insert("sandbox_status".to_string(), ToolMetadata {
        category: "system".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Check sandbox status. Read-only.".to_string(),
    });
    m.insert("sandbox_ingest".to_string(), ToolMetadata {
        category: "system".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Ingest sandbox output into memory. Mutates memory state.".to_string(),
    });

    // --- Scheduler ---
    m.insert("schedule".to_string(), ToolMetadata {
        category: "automation".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Schedule a one-shot task. Mutates scheduler state.".to_string(),
    });
    m.insert("schedule_deepeval".to_string(), ToolMetadata {
        category: "automation".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Schedule a DeepEval run. Mutates scheduler state.".to_string(),
    });
    m.insert("cancel_schedule".to_string(), ToolMetadata {
        category: "automation".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Cancel a scheduled task. Mutates scheduler state.".to_string(),
    });
    m.insert("list_pending".to_string(), ToolMetadata {
        category: "automation".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "List pending scheduled tasks. Read-only.".to_string(),
    });

    // --- Scrapling ---
    m.insert("scrape_url".to_string(), ToolMetadata {
        category: "research".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Scrape content from a URL. Read-only network access.".to_string(),
    });
    m.insert("scrape_search".to_string(), ToolMetadata {
        category: "research".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Scrape search engine results. Read-only network access.".to_string(),
    });
    m.insert("extract_structured".to_string(), ToolMetadata {
        category: "research".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Extract structured data from web page. Read-only.".to_string(),
    });

    // --- Screenshot ---
    m.insert("screenshot_capture".to_string(), ToolMetadata {
        category: "system".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Capture screen screenshot. Read-only.".to_string(),
    });

    // --- Search ---
    m.insert("search_content".to_string(), ToolMetadata {
        category: "research".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Search indexed content. Read-only.".to_string(),
    });
    m.insert("find_files".to_string(), ToolMetadata {
        category: "filesystem".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Find files by name or pattern. Read-only.".to_string(),
    });
    m.insert("search_code_structure".to_string(), ToolMetadata {
        category: "analysis".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Search code structure (AST-level). Read-only.".to_string(),
    });
    m.insert("web_search".to_string(), ToolMetadata {
        category: "research".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Search the web. Read-only network access.".to_string(),
    });
    m.insert("search_and_remember".to_string(), ToolMetadata {
        category: "memory".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Search content and store result in memory. Mutates memory state.".to_string(),
    });

    // --- Seed Tools ---
    m.insert("seed_export".to_string(), ToolMetadata {
        category: "data".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Export seed/tool configuration. Read-only export.".to_string(),
    });
    m.insert("seed_import".to_string(), ToolMetadata {
        category: "data".to_string(),
        risk_level: RiskLevel::Medium,
        enriched_description: "Import seed/tool configuration. Mutates tool state.".to_string(),
    });

    // --- Sequential Thinking ---
    m.insert("think".to_string(), ToolMetadata {
        category: "reasoning".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Structured chain-of-thought reasoning. Read-only analysis.".to_string(),
    });
    m.insert("analyze_thought_chain".to_string(), ToolMetadata {
        category: "reasoning".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Analyze a chain of reasoning steps. Read-only.".to_string(),
    });
    m.insert("compare_options".to_string(), ToolMetadata {
        category: "reasoning".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Compare multiple options with trade-offs. Read-only analysis.".to_string(),
    });

    // --- System Metrics (extended) ---
    m.insert("system_disk".to_string(), ToolMetadata {
        category: "monitoring".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Get disk usage statistics. Read-only.".to_string(),
    });
    m.insert("process_info".to_string(), ToolMetadata {
        category: "monitoring".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Get info for a specific process. Read-only.".to_string(),
    });

    // --- Vision ---
    m.insert("vision_device_status".to_string(), ToolMetadata {
        category: "vision".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Check vision device/GPU status. Read-only.".to_string(),
    });
    m.insert("vision_analyze".to_string(), ToolMetadata {
        category: "vision".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Analyze image content. Read-only inference.".to_string(),
    });
    m.insert("vision_extract".to_string(), ToolMetadata {
        category: "vision".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Extract information from image. Read-only.".to_string(),
    });
    m.insert("vision_ocr".to_string(), ToolMetadata {
        category: "vision".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "OCR text extraction from image. Read-only.".to_string(),
    });

    // --- Vision Moondream (excluded but kept for metadata completeness) ---
    m.insert("analyze_image".to_string(), ToolMetadata {
        category: "vision".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Analyze image with Moondream model. Read-only inference.".to_string(),
    });
    m.insert("caption_image".to_string(), ToolMetadata {
        category: "vision".to_string(),
        risk_level: RiskLevel::Low,
        enriched_description: "Generate image caption with Moondream. Read-only inference.".to_string(),
    });

    m
});

/// Enrich a raw MCP Tool with Tylluan metadata.
pub fn enrich_tool(mut tool: rmcp::model::Tool) -> rmcp::model::Tool {
    let mut risk = RiskLevel::Low;
    let mut category = "agnostic".to_string();
    let mut enriched_desc = String::new();

    // 1. Check master registry
    if let Some(meta) = TOOL_METADATA.get(tool.name.as_ref()) {
        risk = meta.risk_level.clone();
        category = meta.category.clone();
        enriched_desc = meta.enriched_description.clone();
    }

    // 2. Dynamic override from tool description (Python-side signals)
    if tool.description.contains("approval=\"always\"") {
        risk = RiskLevel::High;
        if enriched_desc.is_empty() {
            enriched_desc = "Mandatory human approval required by tool provider.".to_string();
        }
    }

    // 3. Apply enrichment
    let prefix = format!(
        "[{}] [CAT: {}] [RISK: {:?}] ",
        risk.as_emoji(),
        category.to_uppercase(),
        risk
    );
    
    let current_desc = &tool.description;
    let final_desc = if enriched_desc.is_empty() {
        format!("{prefix}— {current_desc}")
    } else {
        format!("{prefix}— {enriched_desc}\n(Original: {current_desc})")
    };
    
    tool.description = final_desc.into();
    tool
}
