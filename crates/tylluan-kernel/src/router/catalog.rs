//! # Guild Catalog
//!
//! Static registry of all known guilds — the SINGLE SOURCE OF TRUTH.
//! Ported from TylluanMCP's `GUILD_MANIFESTS` in `SemanticRouter.ts`.
//!
//! Each guild has a name, description (for semantic matching), module path,
//! and type (core vs builder vs scholar vs watcher).

use serde::{Deserialize, Serialize};
use crate::config::GuildWeight;

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

/// Returns the complete built-in guild catalog.
/// This is the master list — guilds can also be added dynamically from config.
pub fn builtin_catalog() -> Vec<GuildDescriptor> {
    // All Python guild files live under guilds/ — organized by category:
    // builders/, scholars/, wardens/, watchers/ subdirectory packages
    vec![
        // ─── Core (always-on) ─────────────────────────────────────────
        GuildDescriptor {
            name: "bash".into(),
            description: "shell commands test run scripts execute".into(),
            module_path: "guilds.builders.plugins.bash".into(),
            category: GuildCategory::Core,
            trigger_phrases: vec![
                // Build & test commands
                "cargo ".into(), "cargo build".into(), "cargo test".into(),
                "cargo check".into(), "cargo run".into(), "cargo clippy".into(),
                "npm run".into(), "npm install".into(), "npm start".into(),
                "npx ".into(), "pip install".into(), "pip3 install".into(),
                "pytest".into(), "python -m".into(), "python3 ".into(),
                // Network commands — must trigger before filesystem interprets URLs as paths
                "curl ".into(), "curl http".into(), "curl https".into(),
                "wget ".into(), "wget http".into(), "wget https".into(),
                // Execution triggers
                "echo ".into(),
                "run script".into(), "run command".into(), "run tests".into(),
                "run the tests".into(), "execute command".into(), "execute this".into(),
                "shell command".into(), "terminal command".into(),
                "ejecuta ".into(), "ejecutar ".into(), "executar ".into(),
                "compila ".into(), "compilar ".into(),
                "make build".into(), "cmake ".into(), "rustc ".into(),
                "node ".into(), "deno ".into(), "powershell".into(),
                "taskkill".into(), "chmod ".into(), "sudo ".into(),
                "apt install".into(), "brew install".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["command".into()],
            weight: GuildWeight::Heavy, // bash runs cargo/npm/pytest — can take minutes on CPU
        },
        GuildDescriptor {
            name: "filesystem".into(),
            description: "read write files find directory list".into(),
            module_path: "guilds.builders.plugins.filesystem".into(),
            category: GuildCategory::Core,
            trigger_phrases: vec![
                "find files".into(), "list files".into(), "list directory".into(),
                "read file".into(), "write file".into(), "create file".into(),
                "delete file".into(), "move file".into(), "copy file".into(),
                "rename file".into(), "save file".into(), "open file".into(),
                "show file".into(), "file content".into(), "file list".into(),
                "cat file".into(), "buscar archivos".into(), "listar archivos".into(),
                "leer archivo".into(), "crear archivo".into(), "mostrar archivo".into(),
                "listar directorio".into(), "contenido del archivo".into(),
                "read the file".into(), "read the contents".into(),
                "show file contents".into(), "show contents of".into(),
                "leer el archivo".into(),
            ],
            embedding: None,
            negative_keywords: vec!["git".into()],
            required_args: vec!["path".into()],
            weight: GuildWeight::Light,
        },
        GuildDescriptor {
            name: "memory".into(),
            description: "memory remember recall store knowledge".into(),
            module_path: "guilds.scholars.plugins.memory".into(),
            category: GuildCategory::Core,
            trigger_phrases: vec![
                "remember ".into(), "remember:".into(), "recall ".into(), "store fact".into(),
                "store this".into(), "memorize".into(), "save this fact".into(),
                "add to memory".into(), "retrieve memory".into(),
                "recuerda ".into(), "recuerda esto".into(), "guarda esto".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["content".into()],
            weight: GuildWeight::Light,
        },
        GuildDescriptor {
            name: "monitor".into(),
            description: "monitor CPU memory disk processes network".into(),
            module_path: "guilds.watchers.plugins.monitor".into(),
            category: GuildCategory::Core,
            trigger_phrases: vec![
                "list processes".into(), "top processes".into(),
                "list top processes".into(), "show processes".into(),
                "process list".into(), "running processes".into(),
                "processes by memory".into(), "processes by cpu".into(),
                "list top".into(), "what is running".into(),
                "qué procesos".into(), "procesos en ejecución".into(),
                "network stats".into(), "network io".into(),
                "bytes sent".into(), "bytes received".into(),
                "bandwidth".into(), "network traffic".into(),
                "estadísticas de red".into(), "watch logs".into(),
                "tail logs".into(), "follow log".into(),
                "monitor process".into(), "ver logs".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec![], // self-contained, no args needed
            weight: GuildWeight::Light,
        },
        GuildDescriptor {
            name: "code_graph".into(),
            description: "code analysis parse structure dependencies".into(),
            module_path: "guilds.core.code_graph".into(),
            category: GuildCategory::Core,
            trigger_phrases: vec![
                "analiza el codigo".into(), "analiza el código".into(),
                "analyze code".into(), "parse codebase".into(),
                "code topology".into(), "index repository".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["path".into()],
            weight: GuildWeight::Medium,
        },

        // ─── Builders (on-demand) ─────────────────────────────────────
        GuildDescriptor {
            name: "git".into(),
            description: "git version control commit push pull".into(),
            module_path: "guilds.builders.plugins.git".into(),
            category: GuildCategory::Builder,
            trigger_phrases: vec![
                "git".into(), "commit".into(), "branch".into(), "diff".into(),
                "log".into(), "push".into(), "pull".into(), "merge".into(),
                "stash".into(), "rebase".into(), "checkout".into(), "clone".into(),
                "remote".into(), "tag".into(), "blame".into(), "git fetch".into(),
                "cherry-pick".into(), "version control".into(),
                "commit history".into(), "show diff".into(),
                "historial de commits".into(), "git status".into(),
                "git diff".into(), "git log".into(), "git commit".into(),
                "git push".into(), "git pull".into(), "git checkout".into(),
                "git branch".into(), "git merge".into(), "git stash".into(),
                "git rebase".into(), "git blame".into(), "git clone".into(),
                "git add".into(), "git reset".into(), "git tag".into(),
                "git remote".into(), "git show".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["command".into()],
            weight: GuildWeight::Medium,
        },
        GuildDescriptor {
            name: "docker".into(),
            description: "Docker container orchestration compose images".into(),
            module_path: "guilds.builders.plugins.docker".into(),
            category: GuildCategory::Builder,
            trigger_phrases: vec![
                "docker run".into(), "docker build".into(), "docker compose".into(),
                "docker ps".into(), "docker stop".into(), "docker pull".into(),
                "docker container".into(), "docker image".into(),
                "docker exec".into(), "docker logs".into(), "docker-compose".into(),
                "container status".into(), "inspect container".into(),
                "container environment".into(), "container variables".into(),
                "container inspect".into(), "list containers".into(),
                "running containers".into(), "container logs".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["command".into()],
            weight: GuildWeight::Heavy,
        },
        // NOTE: sandbox guild removed — guilds.builders.plugins.sandbox doesn't exist in runtime
        // Code execution intents now route to "code" guild instead
        GuildDescriptor {
            name: "database".into(),
            description: "SQL database queries schema tables".into(),
            module_path: "guilds.builders.plugins.database".into(),
            category: GuildCategory::Builder,
            trigger_phrases: vec![
                "sql query".into(), "run query".into(), "database schema".into(),
                "table schema".into(), "select from".into(), "insert into".into(),
                "create table".into(), "consulta sql".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["query".into()],
            weight: GuildWeight::Heavy,
        },
        GuildDescriptor {
            name: "code".into(),
            description: "write edit refactor code files functions".into(),
            module_path: "guilds.builders.plugins.code".into(),
            category: GuildCategory::Builder,
            trigger_phrases: vec![
                "write code".into(), "edit code".into(), "refactor".into(),
                "apply diff".into(), "apply patch".into(), "format code".into(),
                "modify function".into(), "implement feature".into(),
                "editar código".into(), "escribir código".into(),
                "analyze the file".into(), "analyze file".into(),
                "explain the file".into(), "explain file".into(),
                "parse the file".into(), "parse file".into(),
                "what does this file".into(), "what does the file".into(),
                "describe the file".into(), "describe file".into(),
                "analiza el archivo".into(), "analiza el fichero".into(),
                "explica el archivo".into(), "lee este archivo".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["path".into()],
            weight: GuildWeight::Medium,
        },

        // ─── Scholars (on-demand) ─────────────────────────────────────
        GuildDescriptor {
            name: "search".into(),
            description: "internet web search lookup research".into(),
            module_path: "guilds.scholars.plugins.search".into(),
            category: GuildCategory::Scholar,
            trigger_phrases: vec![
                "search the web".into(), "search online".into(),
                "search internet".into(), "look up".into(), "look this up".into(),
                "find online".into(), "google ".into(), "wikipedia ".into(),
                "buscar en internet".into(), "buscar en la web".into(),
                "buscar online".into(), "investigar ".into(), "pesquisar ".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["query".into()],
            weight: GuildWeight::Heavy,
        },
        GuildDescriptor {
            name: "code_analysis".into(),
            description: "static code analysis symbols dependencies".into(),
            module_path: "guilds.builders.plugins.code_analysis".into(),
            category: GuildCategory::Scholar,
            trigger_phrases: vec![],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["path".into()],
            weight: GuildWeight::Heavy,
        },
        GuildDescriptor {
            name: "pdf".into(),
            description: "PDF document text extraction parsing".into(),
            module_path: "guilds.scholars.plugins.pdf".into(),
            category: GuildCategory::Scholar,
            trigger_phrases: vec![
                "read pdf".into(), "parse pdf".into(), "extract pdf".into(),
                "pdf content".into(), "leer pdf".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["path".into()],
            weight: GuildWeight::Heavy,
        },
        GuildDescriptor {
            name: "vision".into(),
            description: "analyze images OCR recognition computer vision".into(),
            module_path: "guilds.core.vision".into(),
            category: GuildCategory::Scholar,
            trigger_phrases: vec![
                "analyze image".into(), "describe image".into(),
                "what is in this image".into(), "ocr ".into(), "analizar imagen".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["path".into()],
            weight: GuildWeight::Heavy,
        },
        GuildDescriptor {
            name: "browser".into(),
            description: "browser navigate web automation screenshots".into(),
            module_path: "guilds.core.browser".into(),
            category: GuildCategory::Scholar,
            trigger_phrases: vec![
                "open browser".into(), "navigate to".into(), "web browser".into(),
                "take screenshot".into(), "search web".into(), "scrape".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["url".into()],
            weight: GuildWeight::Heavy,
        },

        GuildDescriptor {
            name: "audit".into(),
            description: "audit health security compliance inspect".into(),
            module_path: "guilds.wardens.plugins.audit".into(),
            category: GuildCategory::Watcher,
            trigger_phrases: vec![
                "audit system".into(), "audit the system".into(), "system audit".into(),
                "run audit".into(), "guild health check".into(),
                "inspect guild inventory".into(), "guild inventory".into(),
                "inspect kernel".into(), "kernel health".into(),
                "security audit".into(), "compliance check".into(),
                "inventory tools".into(), "audita".into(),
                "inventario de guilds".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec![], // self-contained
            weight: GuildWeight::Light,
        },
        GuildDescriptor {
            name: "system_metrics".into(),
            description: "system health CPU memory disk metrics".into(),
            module_path: "guilds.watchers.plugins.system_metrics".into(),
            category: GuildCategory::Watcher,
            trigger_phrases: vec![
                "system metrics".into(), "show metrics".into(), "cpu usage".into(),
                "memory usage".into(), "disk space".into(), "disk usage".into(),
                "system health".into(), "system status".into(), "health check".into(),
                "system info".into(), "show cpu".into(), "resource usage".into(),
                "how much memory".into(), "uso de cpu".into(), "uso de memoria".into(),
                "estado del sistema".into(), "métricas del sistema".into(),
                "system uptime".into(), "show uptime".into(), "uptime".into(),
                "show cpu usage".into(), "show cpu".into(),
                "show memory usage".into(), "show ram".into(),
                "show disk usage".into(), "show disk space".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec![], // self-contained
            weight: GuildWeight::Light,
        },
        GuildDescriptor {
            name: "deep_analysis".into(),
            description: "codebase mapping dependency analysis architecture".into(),
            module_path: "guilds.scholars.plugins.deep_analysis".into(),
            category: GuildCategory::Core,
            trigger_phrases: vec![],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["query".into()],
            weight: GuildWeight::Heavy,
        },
        GuildDescriptor {
            name: "sequential_thinking".into(),
            description: "step by step reasoning think plan break down".into(),
            module_path: "guilds.scholars.plugins.sequential_thinking".into(),
            category: GuildCategory::Scholar,
            trigger_phrases: vec![
                "think step by step".into(), "think through".into(),
                "step by step".into(), "break this down".into(), "break down".into(),
                "plan this out".into(), "reason through".into(),
                "analyze step".into(), "walk me through".into(),
                "piensa paso a paso".into(), "desglosa".into(), "planifica".into(),
                "compare options".into(), "compare choices".into(),
                "compare alternatives".into(), "which is better".into(),
                "which should i".into(), "pros and cons".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec![], // self-contained
            weight: GuildWeight::Medium,
        },
        GuildDescriptor {
            name: "ingest".into(),
            description: "ingest import load data documents files".into(),
            module_path: "guilds.scholars.plugins.ingest".into(),
            category: GuildCategory::Builder,
            trigger_phrases: vec![
                "ingest ".into(), "ingest text".into(), "ingest url".into(),
                "ingest file".into(), "import document".into(), "seed knowledge".into(),
                "index content".into(), "load repository".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["url".into()],
            weight: GuildWeight::Heavy,
        },
        GuildDescriptor {
            name: "knowledge".into(),
            description: "extract triples entities knowledge graph NER".into(),
            module_path: "guilds.scholars.plugins.knowledge".into(),
            category: GuildCategory::Scholar,
            trigger_phrases: vec![
                // Technical NLP triggers
                "extract triples".into(),
                "extrae triples".into(),
                "triple extraction".into(),
                "named entity".into(),
                "subject predicate".into(),
                "entity extraction".into(),
                "relation extraction".into(),
                "ner ".into(),
                // Natural language — English
                "knowledge graph".into(),
                "build graph from".into(),
                "link concepts".into(),
                "link this concept".into(),
                "find entities".into(),
                "add to knowledge".into(),
                "add to the graph".into(),
                "populate the graph".into(),
                "store knowledge".into(),
                "learn from this".into(),
                "learn this concept".into(),
                "extract knowledge".into(),
                "extract entities from text".into(),
                "extract relations from text".into(),
                "auto-link".into(),
                // Natural language — Spanish
                "aprende este".into(),
                "aprende que".into(),
                "aprende esto".into(),
                "guarda en el grafo".into(),
                "añade al grafo".into(),
                "añade esto al grafo".into(),
                "poblar el grafo".into(),
                "enlaza conceptos".into(),
                "relaciona conceptos".into(),
                "extrae conceptos".into(),
                "extraer conceptos".into(),
                "extrae el conocimiento".into(),
                "extrae las relaciones".into(),
                "infiere entidades".into(),
                "vincular ideas".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["content".into()],
            weight: GuildWeight::Heavy,
        },
        GuildDescriptor {
            name: "data_tools".into(),
            description: "parse JSON YAML CSV data transform".into(),
            module_path: "guilds.scholars.plugins.data_tools".into(),
            category: GuildCategory::Scholar,
            trigger_phrases: vec![],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["path".into()],
            weight: GuildWeight::Medium,
        },
        GuildDescriptor {
            name: "formatter".into(),
            description: "format code Ruff Prettier Rustfmt lint".into(),
            module_path: "guilds.builders.plugins.formatter".into(),
            category: GuildCategory::Builder,
            trigger_phrases: vec![],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["path".into()],
            weight: GuildWeight::Medium,
        },
        GuildDescriptor {
            name: "mcp_bridge".into(),
            description: "MCP bridge federated remote servers".into(),
            module_path: "guilds.core.mcp_bridge".into(),
            category: GuildCategory::Core,
            trigger_phrases: vec![
                "mcp ping".into(), "mcp call".into(), "mcp list tools".into(),
                "ping mcp".into(), "call remote tool".into(), "federated mcp".into(),
                "mcp bridge".into(), "mcp connectivity".into(), "remote mcp".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["command".into()],
            weight: GuildWeight::Light,
        },
        GuildDescriptor {
            name: "code_reviewer".into(),
            description: "code review analyze quality bugs".into(),
            module_path: "guilds.core.code_reviewer".into(),
            category: GuildCategory::Core,
            trigger_phrases: vec![
                "review code".into(), "code review".into(), "review this".into(),
                "critique code".into(), "find bugs".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["path".into()],
            weight: GuildWeight::Medium,
        },
        GuildDescriptor {
            name: "deep_web_research".into(),
            description: "deep web research fetch scrape online".into(),
            module_path: "guilds.core.deep_web_research".into(),
            category: GuildCategory::Scholar,
            trigger_phrases: vec![
                "fetch page".into(), "fetch url".into(), "fetch http".into(),
                "research topic".into(), "deep research".into(), "crawl page".into(),
                "scrape page".into(), "web scraping".into(),
                "research online about".into(), "look up on the web".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["query".into()],
            weight: GuildWeight::Medium,
        },
        GuildDescriptor {
            name: "comfy_ui".into(),
            description: "generate images video ComfyUI Flux Wan2".into(),
            module_path: "guilds.core.comfy_ui".into(),
            category: GuildCategory::Core,
            trigger_phrases: vec![
                "generate image".into(), "generate an image".into(), "genera imagen".into(),
                "comfy status".into(), "comfyui status".into(), "comfy ui".into(),
                "text to image".into(), "img2img".into(), "image to image".into(),
                "generate video".into(), "genera video".into(), "youtube short".into(),
                "ken burns".into(), "tts narration".into(), "kokoro".into(),
                "list models".into(), "flux schnell".into(), "image generation".into(),
                "create art".into(), "render scene".into(), "generate art".into(),
                "generación de imagen".into(), "genera una imagen".into(),
                // Wan2.2 video
                "generate video wan".into(), "wan2".into(), "wan video".into(),
                "generate wan".into(), "video wan".into(), "genera video wan".into(),
                "landscape video".into(), "shorts video".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["prompt".into()],
            weight: GuildWeight::Heavy,
        },
        GuildDescriptor {
            name: "n8n_bridge".into(),
            description: "n8n workflow automation pipelines orchestration".into(),
            module_path: "guilds.core.n8n_bridge".into(),
            category: GuildCategory::Core,
            trigger_phrases: vec![
                "n8n".into(), "trigger workflow".into(), "run workflow".into(),
                "execute workflow".into(), "list workflows".into(),
                "automation pipeline".into(), "n8n status".into(),
                "ejecuta workflow".into(), "lanza workflow".into(),
                "kernel pulse".into(), "system pulse".into(), "tylluan pulse".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["command".into()],
            weight: GuildWeight::Medium,
        },
        GuildDescriptor {
            name: "coloquio".into(),
            description: "coloquio channel messages chat group".into(),
            module_path: "guilds.core.coloquio".into(),
            category: GuildCategory::Core,
            trigger_phrases: vec![
                "lee el coloquio".into(), "leer coloquio".into(), "ver coloquio".into(),
                "read coloquio".into(), "lee el canal".into(), "ver canal coloquio".into(),
                "publica en coloquio".into(), "post to coloquio".into(),
                "post to coloquio channel".into(), "post to channel".into(),
                "send message to coloquio".into(), "send to coloquio".into(),
                "send message".into(), "envia al canal".into(), "envía al canal".into(),
                "message coloquio".into(),
                "lista canales coloquio".into(), "list coloquio channels".into(),
                "coloquio channel".into(), "canal coloquio".into(),
                "hilo coloquio".into(), "historial coloquio".into(),
                "conversacion grupal".into(), "group chat".into(),
                "publicar en coloquio".into(),
                "publica en canal".into(), "publicar en canal".into(),
                "post to mision".into(),
                "que hay de nuevo".into(), "ponme al dia".into(),
                "what's new".into(), "whats new".into(), "catch up".into(),
                "ponte al dia".into(), "mensajes sin leer".into(),
                "novedades coloquio".into(), "novedades".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["channel_id".into()],
            weight: GuildWeight::Light,
        },
        GuildDescriptor {
            name: "coloquio_digest".into(),
            description: "digest summarize coloquio memory reasoning".into(),
            module_path: "guilds.core.coloquio_digest".into(),
            category: GuildCategory::Core,
            trigger_phrases: vec![
                "digest coloquio".into(), "digest all channels".into(),
                "summarize channels".into(), "coloquio to memory".into(),
                "sincroniza coloquio".into(), "resumen canales".into(),
                "auto reason cycle".into(), "reasoning cycle".into(),
                "flywheel knowledge".into(), "digest status".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["channel_id".into()],
            weight: GuildWeight::Medium,
        },
        GuildDescriptor {
            name: "websearch".into(),
            description: "SearXNG web search meta-engine research".into(),
            module_path: "guilds.core.websearch".into(),
            category: GuildCategory::Scholar,
            trigger_phrases: vec![
                "busca en internet".into(), "search web".into(),
                "busca informacion sobre".into(), "web search".into(),
                "buscar online".into(), "buscar en la web".into(),
                "internet search".into(), "look up".into(),
                "find online".into(), "research topic".into(),
                "buscar informacion".into(), "fact check".into(),
                "online research".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["query".into()],
            weight: GuildWeight::Medium,
        },
        GuildDescriptor {
            name: "scrapling".into(),
            description: "web scraping HTML parse crawl extract".into(),
            module_path: "guilds.core.scrapling_web".into(),
            category: GuildCategory::Scholar,
            trigger_phrases: vec![
                "raspa la pagina".into(), "raspa la página".into(),
                "scrape url".into(), "scrape page".into(),
                "extrae contenido de".into(), "extraer contenido de".into(),
                "fetch webpage".into(), "fetch page".into(),
                "download page".into(), "download url".into(),
                "extract data".into(), "extract structure".into(),
                "scrape website".into(), "scrape search".into(),
                "crawl page".into(), "crawl website".into(),
                "extraer datos".into(), "extraer estructura".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["url".into()],
            weight: GuildWeight::Medium,
        },
        // ─── V1 Port ────────────────────────────────────────────────
        GuildDescriptor {
            name: "audio_tools".into(),
            description: "audio transcribe Whisper ComfyUI".into(),
            module_path: "guilds.builders.plugins.audio_tools".into(),
            category: GuildCategory::Builder,
            trigger_phrases: vec!["transcribe audio".into(), "whisper".into(), "audio transcription".into()],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["path".into()],
            weight: GuildWeight::Heavy,
        },
        GuildDescriptor {
            name: "ffmpeg_tools".into(),
            description: "media probe trim concat resize".into(),
            module_path: "guilds.builders.plugins.ffmpeg_tools".into(),
            category: GuildCategory::Builder,
            trigger_phrases: vec![
                "convert video".into(), "trim video".into(), "concat video".into(), "resize video".into(),
                "extract audio".into(), "media info".into(), "ffmpeg".into(), "ffprobe".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["command".into()],
            weight: GuildWeight::Medium,
        },
        GuildDescriptor {
            name: "screenshot_tools".into(),
            description: "screen capture screenshot image".into(),
            module_path: "guilds.builders.plugins.screenshot_tools".into(),
            category: GuildCategory::Builder,
            trigger_phrases: vec!["screenshot".into(), "capture screen".into(), "take screenshot".into(), "pantallazo".into()],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec![], // self-contained
            weight: GuildWeight::Light,
        },
        GuildDescriptor {
            name: "clipboard_tools".into(),
            description: "clipboard read write system".into(),
            module_path: "guilds.builders.plugins.clipboard_tools".into(),
            category: GuildCategory::Builder,
            trigger_phrases: vec!["clipboard".into(), "copy to clipboard".into(), "paste from clipboard".into(), "portapapeles".into()],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec![], // self-contained
            weight: GuildWeight::Light,
        },
        GuildDescriptor {
            name: "local_llm_proxy".into(),
            description: "local LLM inference Ollama API".into(),
            module_path: "guilds.builders.plugins.local_llm_proxy".into(),
            category: GuildCategory::Builder,
            trigger_phrases: vec![
                "ollama".into(), "lm studio".into(), "local llm".into(), "local model".into(),
                "run inference".into(), "chat with model".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["prompt".into()],
            weight: GuildWeight::Medium,
        },
        GuildDescriptor {
            name: "cron_scheduler".into(),
            description: "schedule recurring tasks jobs".into(),
            module_path: "guilds.watchers.plugins.cron_scheduler".into(),
            category: GuildCategory::Watcher,
            trigger_phrases: vec!["schedule".into(), "cron job".into(), "recurring task".into(), "tarea programada".into()],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["command".into()],
            weight: GuildWeight::Light,
        },
        GuildDescriptor {
            name: "biome_warden".into(),
            description: "format lint code BiomeJS".into(),
            module_path: "guilds.wardens.plugins.biome_warden".into(),
            category: GuildCategory::Watcher,
            trigger_phrases: vec!["biome".into(), "format code".into(), "lint file".into(), "auto-format".into()],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["path".into()],
            weight: GuildWeight::Light,
        },
        GuildDescriptor {
            name: "ast_surgeon".into(),
            description: "AST rename symbol refactor TS".into(),
            module_path: "guilds.scholars.plugins.ast_surgeon".into(),
            category: GuildCategory::Scholar,
            trigger_phrases: vec![
                "ast".into(), "rename symbol".into(), "file outline".into(), "find references".into(),
                "syntax tree".into(), "refactor symbol".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["path".into()],
            weight: GuildWeight::Light,
        },
        // External MCP guild — no Python file, launched via npx codebase-memory-mcp
        GuildDescriptor {
            name: "codebase_memory".into(),
            description: "Code knowledge graph: index repos, search code, trace calls, architecture analysis".into(),
            module_path: "external:codebase_memory".into(),
            category: GuildCategory::Scholar,
            trigger_phrases: vec![
                "index this project".into(),
                "index codebase".into(),
                "index repository".into(),
                "search code".into(),
                "call chain".into(),
                "trace path".into(),
                "trace calls".into(),
                "code architecture".into(),
                "find function".into(),
                "code snippet".into(),
                "detect changes".into(),
                "graph schema".into(),
            ],
            embedding: None,
            negative_keywords: vec![],
            required_args: vec!["repo_path".into()],
            weight: GuildWeight::Heavy,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalog_not_empty() {
        let catalog = builtin_catalog();
        // 28 active guilds
        // Core: bash, filesystem, memory, monitor, mcp_bridge, code_reviewer, deep_web_research, coloquio, websearch (9)
        // Builders: git, docker, database, code (4)
        // Scholars: search, code_analysis, pdf, vision (4)
        // Watchers: system_metrics, deep_analysis, audit, sequential_thinking, ingest, knowledge, data_tools, formatter (8)
        // + browser (1), comfy_ui (1), n8n_bridge (1), code_graph (1)
        assert_eq!(catalog.len(), 40, "Expected 40 active guilds. Update this assertion when adding/removing guilds.");
        // Verify critical guilds are present
        let names: Vec<&str> = catalog.iter().map(|g| g.name.as_str()).collect();
        assert!(names.contains(&"bash"), "bash guild missing");
        assert!(names.contains(&"filesystem"), "filesystem guild missing");
        assert!(names.contains(&"memory"), "memory guild missing");
    }

    #[test]
    fn test_core_guilds_present() {
        let catalog = builtin_catalog();
        let core: Vec<&str> = catalog.iter()
            .filter(|g| g.category == GuildCategory::Core)
            .map(|g| g.name.as_str())
            .collect();
        assert!(core.contains(&"bash"));
        assert!(core.contains(&"filesystem"));
        assert!(core.contains(&"memory"));
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
    fn test_knowledge_guild_has_trigger_phrases() {
        let catalog = builtin_catalog();
        let knowledge = catalog.iter().find(|g| g.name == "knowledge").expect("knowledge guild missing");
        assert!(!knowledge.trigger_phrases.is_empty(), "knowledge guild should have trigger phrases");
        // Technical triggers
        assert!(knowledge.trigger_phrases.contains(&"extract triples".to_string()));
        assert!(knowledge.trigger_phrases.contains(&"knowledge graph".to_string()));
        assert!(knowledge.trigger_phrases.contains(&"ner ".to_string()));
        // Natural language triggers — GIK fix
        assert!(knowledge.trigger_phrases.contains(&"aprende este".to_string()), "missing Spanish natural trigger");
        assert!(knowledge.trigger_phrases.contains(&"añade al grafo".to_string()), "missing Spanish graph trigger");
        assert!(knowledge.trigger_phrases.contains(&"add to knowledge".to_string()), "missing English natural trigger");
        assert!(knowledge.trigger_phrases.contains(&"populate the graph".to_string()), "missing English graph trigger");

        let ingest = catalog.iter().find(|g| g.name == "ingest").expect("ingest guild missing");
        assert!(!ingest.description.contains("knowledge ingestion"), "ingest description should not contain 'knowledge ingestion'");
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
