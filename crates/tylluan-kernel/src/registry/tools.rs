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
        format!("{}— {}", prefix, current_desc)
    } else {
        format!("{}— {}\n(Original: {})", prefix, enriched_desc, current_desc)
    };
    
    tool.description = final_desc.into();
    tool
}
