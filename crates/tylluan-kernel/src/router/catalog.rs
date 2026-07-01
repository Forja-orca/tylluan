//! # Guild Catalog
//!
//! Auto-discovered from `guilds/` directory structure.
//! Scans for `.py` files with FastMCP servers at startup.
//! Zero-config for new guilds — just put a `.py` file in the right directory.

use serde::{Deserialize, Serialize};
use crate::config::GuildWeight;
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

/// Describes a guild that the kernel can load.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuildDescriptor {
    /// Unique guild name (e.g., "bash", "git", "docker")
    pub name: String,
    /// Human-readable description for semantic matching
    pub description: String,
    /// Python module path (e.g., "guilds.builders.plugins.bash")
    pub module_path: String,
    /// Guild category
    pub category: GuildCategory,
    /// Explicit trigger phrases for high-confidence routing
    pub trigger_phrases: Vec<String>,
    /// Pre-computed embedding vector (set at runtime when semantic feature is enabled)
    #[serde(skip)]
    pub embedding: Option<Vec<f32>>,
    /// Words that penalize routing to this guild (anti-keywords).
    /// If any query token matches, the keyword score is reduced by 0.3.
    /// Replaces BUG-02 hardcoded penalty in matcher.rs.
    #[serde(default)]
    pub negative_keywords: Vec<String>,
    /// Guild weight for timeout assignment (Light=15s, Medium=60s, Heavy=180s)
    #[serde(default)]
    pub weight: GuildWeight,
    /// Arguments required by this guild's primary tools.
    /// Before calling the guild, tylluan_do checks that these keys have
    /// non-empty values in tool_args. If missing, returns a clear error.
    /// Agents should provide these explicitly for reliable routing.
    #[serde(default)]
    pub required_args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum GuildCategory {
    Core,
    Builder,
    Scholar,
    Watcher,
}

impl std::fmt::Display for GuildCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GuildCategory::Core => write!(f, "core"),
            GuildCategory::Builder => write!(f, "builder"),
            GuildCategory::Scholar => write!(f, "scholar"),
            GuildCategory::Watcher => write!(f, "watcher"),
        }
    }
}

/// Overrides for guilds that need explicit weight or required_args
/// not inferrable from the Python file alone.
fn guild_overrides() -> HashMap<&'static str, (GuildWeight, Vec<&'static str>)> {
    let mut m = HashMap::new();
    m.insert("bash", (GuildWeight::Heavy, vec!["command"]));
    m.insert("filesystem", (GuildWeight::Light, vec!["path"]));
    m.insert("memory", (GuildWeight::Light, vec!["content"]));
    m.insert("vision", (GuildWeight::Medium, vec!["path"]));
    m.insert("knowledge", (GuildWeight::Medium, vec!["command"]));
    m.insert("code_analysis", (GuildWeight::Heavy, vec!["command"]));
    m.insert("deep_analysis", (GuildWeight::Heavy, vec!["query"]));
    m.insert("deep_web_research", (GuildWeight::Heavy, vec!["query"]));
    m.insert("ingest", (GuildWeight::Medium, vec!["path"]));
    m.insert("git", (GuildWeight::Heavy, vec!["command"]));
    m.insert("docker", (GuildWeight::Heavy, vec!["command"]));
    m.insert("coloquio", (GuildWeight::Medium, vec!["channel_id"]));
    m.insert("websearch", (GuildWeight::Heavy, vec!["query"]));
    m.insert("search", (GuildWeight::Heavy, vec!["query"]));
    m.insert("browser", (GuildWeight::Heavy, vec!["url"]));
    m.insert("code", (GuildWeight::Heavy, vec!["path"]));
    m.insert("code_graph", (GuildWeight::Heavy, vec!["path"]));
    m.insert("database", (GuildWeight::Heavy, vec!["url"]));
    m.insert("comfy_ui", (GuildWeight::Heavy, vec!["prompt"]));
    m.insert("n8n_bridge", (GuildWeight::Medium, vec![]));
    m.insert("scrapling", (GuildWeight::Heavy, vec!["url"]));
    m.insert("pdf", (GuildWeight::Medium, vec!["path"]));
    m.insert("code_reviewer", (GuildWeight::Medium, vec!["command"]));
    m.insert("formatter", (GuildWeight::Medium, vec!["path"]));
    m.insert("data_tools", (GuildWeight::Medium, vec!["path"]));
    m.insert("ast_surgeon", (GuildWeight::Light, vec!["command"]));
    m.insert("audio_tools", (GuildWeight::Medium, vec!["path"]));
    m.insert("ffmpeg_tools", (GuildWeight::Medium, vec!["path"]));
    m.insert("screenshot_tools", (GuildWeight::Light, vec!["path"]));
    m.insert("clipboard_tools", (GuildWeight::Light, vec!["path"]));
    m.insert("local_llm_proxy", (GuildWeight::Medium, vec!["command"]));
    m.insert("biome_warden", (GuildWeight::Medium, vec!["query"]));
    m.insert("audit", (GuildWeight::Heavy, vec!["path"]));
    m.insert("cron_scheduler", (GuildWeight::Light, vec!["command"]));
    m.insert("sequential_thinking", (GuildWeight::Medium, vec!["prompt"]));
    m.insert("coloquio_digest", (GuildWeight::Medium, vec!["channel_id"]));
    m.insert("whats_new", (GuildWeight::Light, vec!["channel_id"]));
    m.insert("council", (GuildWeight::Medium, vec!["query"]));
    m.insert("mcp_bridge", (GuildWeight::Light, vec![]));
    m.insert("monitor", (GuildWeight::Light, vec![]));
    m.insert("system_metrics", (GuildWeight::Light, vec![]));
    m
}

fn dir_to_category(dir_name: &str) -> GuildCategory {
    match dir_name {
        "builders" => GuildCategory::Builder,
        "scholars" => GuildCategory::Scholar,
        "wardens" => GuildCategory::Core,
        "watchers" => GuildCategory::Watcher,
        _ => GuildCategory::Core,
    }
}

/// Map file stem to guild name for files where FastMCP name differs from filename.
fn name_override(stem: &str) -> Option<&'static str> {
    let name = match stem {
        "scrapling_web" => "scrapling",
        _ => return None,
    };
    Some(name)
}

fn description_override(name: &str) -> Option<&'static str> {
    Some(match name {
        "bash" => "Shell command execution: build, test, compile, run scripts",
        "filesystem" => "Read and write files, find and list directories",
        "memory" => "Store and retrieve knowledge from long-term memory",
        "git" => "Git source control: status, diff, log, commits, checkout, branches",
        "docker" => "Docker container management and database services",
        "system_metrics" => "System health metrics: CPU, memory, disk usage",
        "code_analysis" => "Static analysis and code quality checks",
        "deep_analysis" => "Deep code analysis and architectural understanding",
        "deep_web_research" => "Multi-source web research and content gathering",
        "knowledge" => "Knowledge graph triple extraction and entity recognition",
        "ingest" => "Ingest documents and code into memory",
        "vision" => "Image analysis and OCR using vision models",
        "browser" => "Web browser automation with CDP protocol",
        "code" => "Code modification and generation across languages",
        "database" => "Database query and schema management",
        "search" => "Semantic and keyword search across indexed content",
        "pdf" => "PDF document reading and text extraction",
        "websearch" => "Web search engine queries and result fetching",
        "code_reviewer" => "Code review and quality checks",
        "coloquio" => "Coloquio multi-agent conversation channels",
        "mcp_bridge" => "External MCP server integration bridge",
        "code_graph" => "Code dependency graph and structure analysis",
        "comfy_ui" => "Image generation via ComfyUI workflow",
        "n8n_bridge" => "n8n workflow automation trigger and management",
        "scrapling" => "Web scraping and content extraction from URLs",
        "data_tools" => "JSON, YAML, CSV data manipulation tools",
        "formatter" => "Code formatter: Ruff, Prettier, Rustfmt",
        "sequential_thinking" => "Step-by-step reasoning and analysis",
        "coloquio_digest" => "Coloquio channel digest and summary",
        "whats_new" => "Unread messages and updates from channels",
        "council" => "Multi-voice decision making and tradeoff analysis",
        "ast_surgeon" => "AST manipulation and code transformation",
        "audio_tools" => "Audio file processing and conversion",
        "ffmpeg_tools" => "FFmpeg multimedia processing tools",
        "screenshot_tools" => "Screen capture and screenshot utilities",
        "clipboard_tools" => "Clipboard read and write utilities",
        "local_llm_proxy" => "Local LLM inference proxy and requests",
        "biome_warden" => "Biome code quality linting and formatting",
        "audit" => "Security audit and system integrity checks",
        "cron_scheduler" => "Scheduled task and cron job management",
        _ => return None,
    })
}

fn name_to_description(name: &str) -> String {
    if let Some(desc) = description_override(name) {
        return desc.to_string();
    }
    let words: Vec<String> = name.split('_')
        .map(|w| match w {
            "ui" => "UI".to_string(),
            "mcp" => "MCP".to_string(),
            "llm" => "LLM".to_string(),
            "api" => "API".to_string(),
            "n8n" => "n8n".to_string(),
            "pdf" => "PDF".to_string(),
            "ast" => "AST".to_string(),
            "ner" => "NER".to_string(),
            "ocr" => "OCR".to_string(),
            "t2i" => "T2I".to_string(),
            "i2i" => "I2I".to_string(),
            "coloquio" => "Coloquio conversation".to_string(),
            "cron" => "Cron scheduled".to_string(),
            "biome" => "Biome code quality".to_string(),
            other => other.to_string(),
        })
        .collect();
    let desc = words.join(" ");
    if desc.len() > 10 { desc } else { format!("{} guild tools", name) }
}

/// Extract trigger phrases from a guild's Python file by scanning for
/// "Use for:" lines in module docstrings, including continuation lines.
fn extract_trigger_phrases(content: &str) -> Vec<String> {
    let mut phrases = Vec::new();
    let mut in_docstring = false;
    let mut collecting = false;
    let mut pending = String::new();

        for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("\"\"\"") || trimmed.starts_with("'''") {
            let after_quotes = trimmed[3..].to_string();
            // Detect single-line docstring: opens and closes on same line
            let closing = if trimmed.starts_with("\"\"\"") {
                after_quotes.rfind("\"\"\"")
            } else {
                after_quotes.rfind("'''")
            };
            if let Some(close_pos) = closing {
                // Single-line docstring: """content"""
                let middle = after_quotes[..close_pos].trim().to_string();
                if let Some(use_for_pos) = middle.find("Use for:") {
                    let use_for = middle[use_for_pos + 8..].trim().trim_end_matches(',');
                    pending.push_str(use_for);
                    for phrase in pending.split(',') {
                        let p = phrase.trim().to_string();
                        if !p.is_empty() { phrases.push(p); }
                    }
                    pending.clear();
                }
                continue;
            }
            if in_docstring {
                // Closing multi-line docstring
                let after = after_quotes.trim().to_string();
                if collecting && !after.is_empty() {
                    pending.push(' ');
                    pending.push_str(after.trim_end_matches(','));
                }
            } else {
                // Opening multi-line docstring
                let after = after_quotes.trim().to_string();
                if let Some(use_for_pos) = after.find("Use for:") {
                    let use_for = after[use_for_pos + 8..].trim().trim_end_matches(',');
                    pending.push_str(use_for);
                    collecting = true;
                }
            }
            in_docstring = !in_docstring;
            if !in_docstring && collecting {
                for phrase in pending.split(',') {
                    let p = phrase.trim().to_string();
                    if !p.is_empty() { phrases.push(p); }
                }
                pending.clear();
                collecting = false;
            }
            continue;
        }
        if in_docstring {
            if let Some(use_for_pos) = trimmed.find("Use for:") {
                let use_for = trimmed[use_for_pos + 8..].trim().trim_end_matches(',');
                pending.push_str(use_for);
                collecting = true;
            } else if collecting {
                if trimmed.is_empty() || trimmed.starts_with("Args:") || trimmed.starts_with("Returns:") || trimmed.starts_with("Raises:") {
                    for phrase in pending.split(',') {
                        let p = phrase.trim().to_string();
                        if !p.is_empty() { phrases.push(p); }
                    }
                    pending.clear();
                    collecting = false;
                } else {
                    let clean = trimmed.trim_end_matches(',');
                    pending.push(' ');
                    pending.push_str(clean);
                }
            }
        }
    }
    // flush if docstring never closed
    if collecting {
        for phrase in pending.split(',') {
            let p = phrase.trim().to_string();
            if !p.is_empty() { phrases.push(p); }
        }
    }
    phrases
}

/// Extract guild name from `mcp = FastMCP("name")` or similar pattern.
fn extract_guild_name(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(start) = trimmed.find("FastMCP(") {
            let after = &trimmed[start + 8..];
            if let Some(end) = after.find(&[')', '\n'][..]) {
                let inner = &after[..end];
                let name = inner.trim_matches(&['"', '\''][..]);
                if !name.is_empty() && !name.contains(|c: char| c.is_whitespace()) {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

/// Scan the guilds directory and auto-discover all guilds.
/// Returns descriptors derived from file paths and docstrings.
pub fn scan_guilds_directory(guilds_root: &Path) -> Vec<GuildDescriptor> {
    let overrides = guild_overrides();
    let mut descriptors = Vec::new();

    let entries = match std::fs::read_dir(guilds_root) {
        Ok(e) => e,
        Err(_) => return descriptors,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let category_dir = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let plugins_dir = path.join("plugins");
        let search_dir = if plugins_dir.is_dir() { plugins_dir } else { path.clone() };

        if let Ok(files) = std::fs::read_dir(&search_dir) {
            for file_entry in files.flatten() {
                let file_path = file_entry.path();
                if file_path.extension().and_then(|e| e.to_str()) != Some("py") {
                    continue;
                }

                let file_stem = file_path.file_stem()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");

                if file_stem.starts_with('_') { continue; }

                let content = std::fs::read_to_string(&file_path).unwrap_or_default();
                if extract_guild_name(&content).is_none() { continue; }
                let guild_name = name_override(file_stem).unwrap_or(file_stem).to_string();
                let trigger_phrases = extract_trigger_phrases(&content);

                let module_path = if search_dir.ends_with("plugins") {
                    format!("guilds.{}.plugins.{}", category_dir, file_stem)
                } else {
                    format!("guilds.{}.{}", category_dir, file_stem)
                };

                let description = name_to_description(&guild_name);
                let (weight, req_args) = overrides
                    .get(guild_name.as_str())
                    .cloned()
                    .unwrap_or((GuildWeight::Medium, vec![]));
                let required_args: Vec<String> = req_args.into_iter().map(String::from).collect();

                descriptors.push(GuildDescriptor {
                    name: guild_name,
                    description,
                    module_path,
                    category: dir_to_category(&category_dir),
                    trigger_phrases,
                    embedding: None,
                    negative_keywords: vec![],
                    required_args,
                    weight,
                });
            }
        }
    }

    descriptors
}

/// Returns the guild catalog auto-discovered from the filesystem.
/// Falls back to scanning relative to workspace root.
static CATALOG_CACHE: OnceLock<Vec<GuildDescriptor>> = OnceLock::new();

pub fn builtin_catalog() -> Vec<GuildDescriptor> {
    CATALOG_CACHE.get_or_init(|| {
        // Try workspace root relative to the binary's working directory,
        // then walk up looking for Cargo.toml / guilds/ directory.
        let mut candidate = std::env::current_dir().unwrap_or_default();
        if !candidate.join("guilds").is_dir() {
            if let Some(parent) = candidate.parent() {
                candidate = parent.to_path_buf();
            }
        }
        if !candidate.join("guilds").is_dir() {
            candidate = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                .unwrap_or_default();
            // Walk up looking for guilds/
            for _ in 0..6 {
                if candidate.join("guilds").is_dir() {
                    break;
                }
                if let Some(parent) = candidate.parent().map(|p| p.to_path_buf()) {
                    candidate = parent;
                } else {
                    break;
                }
            }
        }
        let guilds_dir = candidate.join("guilds");
        let mut catalog = scan_guilds_directory(&guilds_dir);
        catalog.sort_by(|a, b| a.name.cmp(&b.name));
        catalog
    }).clone()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalog_not_empty() {
        let catalog = builtin_catalog();
        // All guilds auto-discovered from guilds/ directory
        // Count should match number of .py files with FastMCP servers
        assert!(catalog.len() >= 35, "Expected at least 35 guilds, got {}", catalog.len());
        // Verify critical guilds are present
        let names: Vec<&str> = catalog.iter().map(|g| g.name.as_str()).collect();
        assert!(names.contains(&"bash"), "bash guild missing");
        assert!(names.contains(&"filesystem"), "filesystem guild missing");
        assert!(names.contains(&"memory"), "memory guild missing");
    }

    #[test]
    fn test_core_guilds_present() {
        // Auto-discovered guilds get their category from their guilds/ subdirectory.
        // Builders/ → Builder, Scholars/ → Scholar, Wardens/ → Core, Watchers/ → Watcher.
        let catalog = builtin_catalog();
        let names: Vec<&str> = catalog.iter().map(|g| g.name.as_str()).collect();
        assert!(names.contains(&"bash"), "bash guild missing from catalog");
        assert!(names.contains(&"filesystem"), "filesystem guild missing from catalog");
        assert!(names.contains(&"memory"), "memory guild missing from catalog");
    }

    #[test]
    fn test_no_duplicate_names() {
        let catalog = builtin_catalog();
        let mut names: Vec<&str> = catalog.iter().map(|g| g.name.as_str()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), catalog.len(), "Duplicate guild names found");
    }

    #[test]
    fn test_all_have_descriptions() {
        let catalog = builtin_catalog();
        for guild in &catalog {
            assert!(!guild.description.is_empty(), "Guild '{}' has empty description", guild.name);
            assert!(guild.description.len() > 10, "Guild '{}' description too short", guild.name);
        }
    }

    #[test]
    fn test_all_module_paths_in_guilds() {
        // All implementations live under guilds/ or are external MCPs
        let catalog = builtin_catalog();
        for guild in &catalog {
            assert!(
                guild.module_path.starts_with("guilds.") || guild.module_path.starts_with("external:"),
                "Guild '{}' has wrong module_path '{}'",
                guild.name, guild.module_path
            );
        }
    }

    #[test]
    fn test_post_mvp_guilds_present() {
        let catalog = builtin_catalog();
        let names: Vec<&str> = catalog.iter().map(|g| g.name.as_str()).collect();
        assert!(names.contains(&"git"));
        assert!(names.contains(&"docker"));
        assert!(names.contains(&"monitor"));
        assert!(names.contains(&"system_metrics"));
    }

    #[test]
    fn test_new_guilds_present() {
        let catalog = builtin_catalog();
        let names: Vec<&str> = catalog.iter().map(|g| g.name.as_str()).collect();
        // GLiNER NER triple extraction
        assert!(names.contains(&"knowledge"), "knowledge guild missing from catalog");
        // JSON/YAML/CSV data manipulation
        assert!(names.contains(&"data_tools"), "data_tools guild missing from catalog");
        // Ruff/Prettier/Rustfmt auto-formatter
        assert!(names.contains(&"formatter"), "formatter guild missing from catalog");
    }

    #[test]
    fn test_browser_in_catalog() {
        // browser re-enabled (now uses CDP instead of Playwright - no external dependencies)
        let catalog = builtin_catalog();
        let names: Vec<&str> = catalog.iter().map(|g| g.name.as_str()).collect();
        assert!(names.contains(&"browser"), "browser should be enabled - uses CDP, no Playwright needed");
    }

    #[test]
    fn test_extract_trigger_phrases_knowledge() {
        let path = std::path::Path::new("guilds/scholars/plugins/knowledge.py");
        let content = if path.exists() {
            std::fs::read_to_string(path).unwrap()
        } else {
            std::fs::read_to_string("../../guilds/scholars/plugins/knowledge.py").unwrap()
        };
        eprintln!("knowledge.py size: {} bytes, has Use for: {}", content.len(), content.contains("Use for:"));
        // Find the Use for: line and dump context
        if let Some(pos) = content.find("Use for:") {
            let start = pos.saturating_sub(200);
            let end = (pos + 300).min(content.len());
            eprintln!("--- Context around Use for: ---");
            eprintln!("{:?}", &content[start..end]);
            eprintln!("--- End context ---");
        }
        let result = extract_trigger_phrases(&content);
        eprintln!("knowledge file result: {:?}", result);
        assert!(!result.is_empty(), "Should extract phrases from knowledge.py");
    }

    #[test]
    fn test_knowledge_guild_has_trigger_phrases() {
        let catalog = builtin_catalog();
        let knowledge = catalog.iter().find(|g| g.name == "knowledge").expect("knowledge guild missing");
        assert!(!knowledge.trigger_phrases.is_empty(), "knowledge guild should have trigger phrases, got {:?}", knowledge.trigger_phrases);
    }

    #[test]
    fn test_dump_trigger_phrases() {
        let catalog = builtin_catalog();
        for g in &catalog {
            eprintln!("{}: desc={:?} triggers sample={:?}", g.name, g.description, g.trigger_phrases.iter().take(5).collect::<Vec<_>>());
        }
    }

    #[test]
    fn test_debug_git_routing() {
        use crate::router::matcher::{GuildMatcher, keyword_score, tokenize};
        let catalog = builtin_catalog();
        let matcher = GuildMatcher::new(catalog.clone());
        let query = "check git status";
        let q_lower = query.to_lowercase();
        let result = matcher.match_guild(query, None, 0.3, None);
        eprintln!("Result for '{}': {:?}", query, result);
        // Manually check scores for key guilds
        let tokens = tokenize(query);
        for g in &catalog {
            if g.name == "git" || g.name == "filesystem" || g.name == "bash" {
                let trig = if g.trigger_phrases.iter().any(|t| q_lower.contains(t)) { 0.5 } else { 0.0 };
                let kw = keyword_score(&tokens, &g.description, &g.name);
                eprintln!("  {}: kw={:.3} trig={:.1} total={:.3}", g.name, kw, trig, (kw + trig) * 0.45);
            }
        }
    }

    /// ANTI-REGRESSION: Every Python MCP guild must have a catalog entry.
    ///
    /// HOW TO UPDATE THIS TEST:
    /// - Added a new guild .py?  → Add its name to KNOWN_GUILDS below AND add a GuildDescriptor above.
    /// - Deleted a guild .py?   → Remove from KNOWN_GUILDS AND remove the GuildDescriptor.
    /// - File is a utility (no FastMCP server)?  → Add to NOT_GUILDS instead.
    ///
    /// This test exists because guilds have been implemented and silently dead (total_calls=0)
    /// because catalog.rs was never updated. One list = one place to check.
    #[test]
    fn test_every_guild_file_is_in_catalog() {
        // All Python files under guilds/ that expose a FastMCP server.
        // Internal helpers/utilities go in NOT_GUILDS instead.
        const KNOWN_GUILDS: &[&str] = &[
            // guilds/core/
            "audit", "bash", "browser", "code", "code_analysis", "code_graph", "code_reviewer",
            "coloquio", "coloquio_digest", "comfy_ui", "data_tools", "database", "deep_analysis", "deep_web_research",
            "docker", "filesystem", "formatter", "git", "ingest", "knowledge",
            "mcp_bridge", "memory", "monitor", "n8n_bridge", "pdf", "scrapling", "search",
            "sequential_thinking", "system_metrics", "vision", "websearch",
            // V1 Port — guilds/builders/plugins/, guilds/watchers/plugins/, guilds/wardens/plugins/, guilds/scholars/plugins/
            "audio_tools", "ffmpeg_tools", "screenshot_tools", "clipboard_tools",
            "local_llm_proxy", "cron_scheduler", "biome_warden", "ast_surgeon",
            // NOTE: sandbox.py exists but is experimental — add here when it's production-ready.
        ];

        // Python files under guilds/ that are NOT MCP servers (utilities, helpers, bridges).
        // If you find a guild with total_calls=0, check if it's listed here by mistake.
        const NOT_GUILDS: &[&str] = &[
            "__init__", "_security", "memory_bridge", "sandbox", "silva_utils", "utils",
        ];

        let catalog = builtin_catalog();
        let catalog_names: std::collections::HashSet<&str> =
            catalog.iter().map(|g| g.name.as_str()).collect();

        let mut missing_from_catalog: Vec<&str> = Vec::new();
        let mut missing_from_known: Vec<&str> = Vec::new();

        // Every known guild must be in catalog
        for &guild in KNOWN_GUILDS {
            if !catalog_names.contains(guild) {
                missing_from_catalog.push(guild);
            }
        }

        // Every catalog entry must be in KNOWN_GUILDS (catch phantom entries)
        // Entries with module_path starting with "external:" are external MCP guilds (no .py file)
        let known_set: std::collections::HashSet<&str> = KNOWN_GUILDS.iter().copied().collect();
        let not_guild_set: std::collections::HashSet<&str> = NOT_GUILDS.iter().copied().collect();
        for guild in &catalog {
            let name = guild.name.as_str();
            if guild.module_path.starts_with("external:") {
                continue; // external MCP guild — no .py file needed
            }
            if !known_set.contains(name) && !not_guild_set.contains(name) {
                missing_from_known.push(name);
            }
        }

        assert!(
            missing_from_catalog.is_empty(),
            "Guild files exist but have NO catalog entry — tylluan_do cannot route to them!\n\
             Add GuildDescriptor entries for: {:?}\n\
             (Or move to NOT_GUILDS if they are utilities, not MCP servers)",
            missing_from_catalog
        );

        assert!(
            missing_from_known.is_empty(),
            "Catalog has entries with no corresponding guild file — possible typo or deleted guild!\n\
             Remove or rename: {:?}",
            missing_from_known
        );
    }
}
