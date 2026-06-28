//! Configuration system for TylluanNexus.
//!
//! Reads from `tylluan.toml` in the current directory or the default config path.
//! Auto-generates a random auth token on first run if none is set.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error};
use std::fs;

#[derive(Debug, Clone)]
pub struct GremioDiscovery {
    pub name: String,
    pub path: String,
    pub guild_md_exists: bool,
    pub plugins: Vec<String>,
    pub agents: Vec<String>,
}

pub fn load_guild_config(config: &TylluanConfig) -> Vec<GremioDiscovery> {
    let mut discoveries = Vec::new();

    if let Some(v2_config) = &config.guilds.v2 {
        for gremio in &v2_config.gremios {
            let guild_md_path = Path::new(&gremio.path).join("guild.md");
            let plugins_dir = Path::new(&gremio.path).join("plugins");
            let agents_dir = Path::new(&gremio.path).join("agents");

            let guild_md_exists = guild_md_path.exists();
            let plugins = if plugins_dir.exists() {
                fs::read_dir(&plugins_dir)
                    .map(|entries| {
                        entries.filter_map(|e| e.ok())
                            .map(|e| e.path())
                            .filter(|p| p.extension().is_some_and(|ext| ext == "py"))
                            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                vec![]
            };
            let agents = if agents_dir.exists() {
                fs::read_dir(&agents_dir)
                    .map(|entries| {
                        entries.filter_map(|e| e.ok())
                            .map(|e| e.path())
                            .filter(|p| p.is_dir())
                            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                vec![]
            };

            if guild_md_exists {
                info!("📦 [V2] Discovered gremio '{}' at {} with {} plugins, {} agents",
                      gremio.name, gremio.path, plugins.len(), agents.len());
            } else {
                warn!("⚠️ [V2] Gremio '{}' missing guild.md at {}", gremio.name, guild_md_path.display());
            }

            discoveries.push(GremioDiscovery {
                name: gremio.name.clone(),
                path: gremio.path.clone(),
                guild_md_exists,
                plugins,
                agents,
            });
        }

        let legacy_path = &v2_config.legacy_fallback;
        if Path::new(legacy_path).exists() {
            if let Ok(entries) = fs::read_dir(legacy_path) {
                let mut legacy_plugins: Vec<String> = entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|ext| ext == "py"))
                    .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                    .collect();
                legacy_plugins.sort();
                info!("📦 [V2] Legacy fallback: {} guilds from {}", legacy_plugins.len(), legacy_path);
            }
        } else {
            warn!("⚠️ [V2] Legacy fallback path not found: {}", legacy_path);
        }
    }

    discoveries
}

/// Root configuration structure, parsed from `tylluan.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct TylluanConfig {
    #[serde(default)]
    pub nexus: NexusConfig,

    #[serde(default)]
    pub memory: MemoryConfig,

    #[serde(default)]
    pub vision: VisionConfig,

    #[serde(default)]
    pub tui: TuiConfig,

    #[serde(default)]
    pub guilds: GuildsConfig,

    #[serde(default)]
    pub services: HashMap<String, ServiceConfig>,

    #[serde(default)]
    pub external_mcp: Vec<ExternalMcpConfig>,

    #[serde(default)]
    pub federation_peers: Vec<crate::federation::FederationPeer>,

    #[serde(default)]
    pub proxy: ProxyConfig,

    #[serde(default)]
    pub inference: InferenceConfig,

    #[serde(default)]
    pub silva: SilvaConfig,

    #[serde(default)]
    pub limits: LimitsConfig,

    #[serde(default)]
    pub tunnel: TunnelConfig,

    #[serde(default)]
    pub security: SecurityConfig,

    /// Category-specific guild timeouts and low-memory mode.
    #[serde(default)]
    pub timeouts: GuildTimeoutsConfig,

    /// Low memory mode: reduces all guild timeouts by 50%.
    /// On Windows, auto-detected if available; otherwise defaults to false.
    #[serde(default)]
    pub low_memory_mode: bool,

    #[serde(default)]
    pub sharing: SharingConfig,

    #[serde(default)]
    pub mdns: MdnsConfig,

    #[serde(default)]
    pub federation: FederationConfig,

    #[serde(default)]
    pub nat: NatConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationConfig {
    #[serde(default = "default_auto_sync_interval")]
    pub auto_sync_interval_secs: u64,
    #[serde(default = "default_auto_sync_mode")]
    pub auto_sync_mode: String,
}
fn default_auto_sync_interval() -> u64 { 3600 }
fn default_auto_sync_mode() -> String { "push".to_string() }

impl Default for FederationConfig {
    fn default() -> Self {
        Self {
            auto_sync_interval_secs: 3600,
            auto_sync_mode: "push".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatConfig {
    /// STUN servers to try for NAT traversal (ordered: first success wins).
    #[serde(default = "default_stun_servers")]
    pub stun_servers: Vec<String>,
    /// Timeout per STUN attempt in seconds.
    #[serde(default = "default_stun_timeout")]
    pub stun_timeout_secs: u64,
    /// Number of retries per server before trying the next.
    #[serde(default = "default_stun_retries")]
    pub stun_retries: u32,
}

fn default_stun_servers() -> Vec<String> {
    vec![
        "stun.l.google.com:19302".to_string(),
        "stun.cloudflare.com:3478".to_string(),
    ]
}
fn default_stun_timeout() -> u64 { 5 }
fn default_stun_retries() -> u32 { 2 }

impl Default for NatConfig {
    fn default() -> Self {
        Self {
            stun_servers: default_stun_servers(),
            stun_timeout_secs: default_stun_timeout(),
            stun_retries: default_stun_retries(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MdnsConfig {
    /// Advertise this instance on the LAN as `tylluan-nexus-o3.local`.
    /// Disabled by default — enable only in trusted LAN environments.
    #[serde(default)]
    pub advertise: bool,
    /// Scan the LAN for other TylluanNexus instances and auto-register them
    /// as federation peers (requires human approval before any sync).
    /// Disabled by default — enable only in trusted LAN environments.
    #[serde(default)]
    pub discover: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharingConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_node_types")]
    pub node_types: Vec<String>,
    #[serde(default = "default_min_weight")]
    pub min_weight: f64,
    #[serde(default = "default_min_activity_hours")]
    pub min_activity_hours: u64,
}

fn default_true() -> bool { true }
fn default_node_types() -> Vec<String> { vec![] }
fn default_min_weight() -> f64 { 0.5 }
fn default_min_activity_hours() -> u64 { 24 }

impl Default for SharingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            node_types: default_node_types(),
            min_weight: default_min_weight(),
            min_activity_hours: default_min_activity_hours(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusConfig {
    #[serde(default = "default_host")]
    pub host: String,

    #[serde(default = "default_port")]
    pub port: u16,

    #[serde(default)]
    pub transport: Vec<String>,

    #[serde(default)]
    pub timeouts: TimeoutsConfig,

    /// Dev mode: disables authentication for local development
    /// Set to true while prototyping, false for production
    #[serde(default)]
    pub dev_mode: bool,

    /// Expose all kernel utility tools via MCP (not just 5 sovereign).
    /// Enables agents without native tools to call
    /// health, doctor, memory_search, graph ops directly without tylluan_do routing.
    #[serde(default)]
    pub expose_guild_tools: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutsConfig {
    #[serde(default = "default_handshake_secs")]
    pub handshake_secs: u64,

    #[serde(default = "default_tool_call_secs")]
    pub tool_call_secs: u64,
}

impl Default for TimeoutsConfig {
    fn default() -> Self {
        Self {
            handshake_secs: default_handshake_secs(),
            tool_call_secs: default_tool_call_secs(),
        }
    }
}

impl Default for NexusConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            transport: default_transports(),
            timeouts: TimeoutsConfig::default(),
            dev_mode: false,
            expose_guild_tools: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    #[serde(default = "default_db_path")]
    pub db_path: String,

    #[serde(default = "default_embedding_model")]
    pub embedding_model: String,

    #[serde(default = "default_dimensions")]
    pub vector_dimensions: u32,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            db_path: default_db_path(),
            embedding_model: default_embedding_model(),
            vector_dimensions: default_dimensions(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionConfig {
    #[serde(default = "default_vision_model_path")]
    pub model_path: String,
}

impl Default for VisionConfig {
    fn default() -> Self {
        Self {
            model_path: default_vision_model_path(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiConfig {
    #[serde(default = "default_tui_enabled")]
    pub enabled: bool,

    #[serde(default = "default_refresh_ms")]
    pub refresh_ms: u64,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            enabled: default_tui_enabled(),
            refresh_ms: default_refresh_ms(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuildsConfig {
    #[serde(default)]
    pub core: CoreGuildsConfig,
    #[serde(default)]
    pub v2: Option<V2GuildsConfig>,
    /// Max simultaneous calls per guild. Tune for CPU vs GPU concurrency.
    #[serde(default = "default_max_concurrent")]
    pub guild_max_concurrent_calls: usize,
}

fn default_max_concurrent() -> usize { 3 }

impl Default for GuildsConfig {
    fn default() -> Self {
        Self {
            core: CoreGuildsConfig::default(),
            v2: None,
            guild_max_concurrent_calls: default_max_concurrent(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreGuildsConfig {
    #[serde(default = "default_always_on")]
    pub always_on: Vec<String>,

    #[serde(default = "default_lazy_timeout")]
    pub lazy_load_timeout_secs: u64,
}

impl Default for CoreGuildsConfig {
    fn default() -> Self {
        Self {
            always_on: default_always_on(),
            lazy_load_timeout_secs: default_lazy_timeout(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V2GuildsConfig {
    #[serde(default = "default_legacy_fallback")]
    pub legacy_fallback: String,
    #[serde(default)]
    pub gremios: Vec<GremioConfig>,
}

impl Default for V2GuildsConfig {
    fn default() -> Self {
        Self {
            legacy_fallback: default_legacy_fallback(),
            gremios: vec![],
        }
    }
}

fn default_legacy_fallback() -> String {
    "guilds/core".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GremioConfig {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub always_on: bool,
    #[serde(default)]
    pub plugins: Vec<String>,
    #[serde(default)]
    pub agents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub always_on: bool,
    pub url: Option<String>,
    pub env: Option<HashMap<String, String>>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalMcpConfig {
    pub name: String,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub cwd: Option<String>,
    /// HTTP Streamable MCP endpoint (POST /messages)
    pub url: Option<String>,
    /// Classic SSE MCP: GET endpoint (persistent event stream)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sse_url: Option<String>,
    /// Classic SSE MCP: POST endpoint for sending requests
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_url: Option<String>,
    pub env: Option<HashMap<String, String>>,
    pub headers: Option<HashMap<String, String>>,
    pub timeout_ms: Option<u64>,
    /// Whether this server is active (spawned). False = registered but dormant (e.g. auto-discovered).
    #[serde(default = "default_true")]
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct ProxyConfig {
    #[serde(default)]
    pub wsl: WslProxyConfig,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WslProxyConfig {
    #[serde(default = "default_bool_false")]
    pub enabled: bool,
    #[serde(default = "default_bool_true")]
    pub auto_detect: bool,
    #[serde(default = "default_wsl_port")]
    pub fallback_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum InferenceDevice {
    #[default]
    Cpu,
    Directml,
    Cuda,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceConfig {
    #[serde(default)]
    pub providers: Vec<InferenceProvider>,
    #[serde(default = "default_model")]
    pub primary_model: String,
    #[serde(default)]
    pub device: InferenceDevice,
}

impl Default for InferenceConfig {
    fn default() -> Self {
        Self {
            providers: Vec::new(),
            primary_model: default_model(),
            device: InferenceDevice::Cpu,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceProvider {
    pub name: String,
    pub mcp_server: String, // Name of the MCP server that provides this model
    pub model_id: String,
    pub capability: Vec<String>, // ["chat", "vision", "thinking"]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SilvaConfig {
    #[serde(default = "default_silva_db_path")]
    pub db_path: String,

    #[serde(default = "default_bool_true")]
    pub decay_enabled: bool,

    #[serde(default = "default_decay_interval_hours")]
    pub decay_interval_hours: u64,

    #[serde(default = "default_decay_prune_threshold")]
    pub decay_prune_threshold: f64,

    #[serde(default = "default_sync_interval")]
    pub sync_interval_ms: u64,
}

impl Default for SilvaConfig {
    fn default() -> Self {
        Self {
            db_path: default_silva_db_path(),
            decay_enabled: true,
            decay_interval_hours: default_decay_interval_hours(),
            decay_prune_threshold: default_decay_prune_threshold(),
            sync_interval_ms: default_sync_interval(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitsConfig {
    #[serde(default = "default_max_requests_per_agent")]
    pub max_requests_per_agent_per_min: u32,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_requests_per_agent_per_min: default_max_requests_per_agent(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuildTimeoutsConfig {
    /// Timeout for system guilds (bash, git, filesystem, monitor)
    #[serde(default = "default_system_guild_ms")]
    pub system_guild_ms: u64,
    /// Timeout for analysis guilds (code_analysis, data_tools, search)
    #[serde(default = "default_analysis_guild_ms")]
    pub analysis_guild_ms: u64,
    /// Timeout for heavy guilds (docker, database, pdf, vision)
    #[serde(default = "default_heavy_guild_ms")]
    pub heavy_guild_ms: u64,
    /// Heartbeat interval for SSE progress during long guild calls
    #[serde(default = "default_mcp_heartbeat_ms")]
    pub mcp_client_heartbeat_ms: u64,
}

impl Default for GuildTimeoutsConfig {
    fn default() -> Self {
        Self {
            system_guild_ms: default_system_guild_ms(),
            analysis_guild_ms: default_analysis_guild_ms(),
            heavy_guild_ms: default_heavy_guild_ms(),
            mcp_client_heartbeat_ms: default_mcp_heartbeat_ms(),
        }
    }
}

/// Get the effective timeout for a guild weight, adjusted for low memory mode.
pub fn effective_timeout_ms(weight: GuildWeight, low_memory_mode: bool) -> u64 {
    let base = weight.default_timeout_ms();
    if low_memory_mode {
        base / 2
    } else {
        base
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelConfig {
    /// Enable tunnel module on startup
    #[serde(default)]
    pub enabled: bool,
    /// Auto-configure Windows netsh portproxy for WSL2 access
    #[serde(default)]
    pub wsl_bridge: bool,
    /// Port to expose for WSL2 (proxied to kernel's main port)
    #[serde(default = "default_wsl_bridge_port")]
    pub wsl_bridge_port: u16,
    /// Cleanup portproxy rules on shutdown
    #[serde(default = "default_bool_true")]
    pub wsl_bridge_cleanup: bool,
}

impl Default for TunnelConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            wsl_bridge: false,
            wsl_bridge_port: default_wsl_bridge_port(),
            wsl_bridge_cleanup: true,
        }
    }
}

fn default_wsl_bridge_port() -> u16 { 3031 }

fn default_max_requests_per_agent() -> u32 { 60 }

fn default_model() -> String { "local-v3".into() }

impl Default for WslProxyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_detect: true,
            fallback_port: 3031,
        }
    }
}

// ─── Security Configuration (Sandbox + ACL) ─────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default)]
    pub intent_filter: bool,
    #[serde(default)]
    pub sandbox: SandboxConfig,
    #[serde(default)]
    pub acl: AclConfig,
    /// Enable SQLCipher encryption at rest. Reads key from TYLLUAN_DB_KEY env var.
    #[serde(default)]
    pub encrypt_at_rest: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            intent_filter: false,
            sandbox: SandboxConfig::default(),
            acl: AclConfig::default(),
            encrypt_at_rest: false,
        }
    }
}

/// Open a SQLite connection with optional SQLCipher encryption.
/// If [security] encrypt_at_rest = true, reads TYLLUAN_DB_KEY from env.
/// The key MUST be a 64-character lowercase hex string (32 bytes).
/// Generate with: openssl rand -hex 32
///
/// Encryption requires the `encryption` Cargo feature:
///   cargo build --features encryption
///
/// Security note: uses PRAGMA hexkey (not PRAGMA key) — hexkey only accepts
/// [0-9a-f] so string interpolation cannot produce SQL injection.
/// The key is applied BEFORE any other PRAGMA to avoid reading an encrypted
/// DB with WAL mode before it is unlocked.
pub fn open_db(path: &std::path::Path) -> anyhow::Result<rusqlite::Connection> {
    let conn = rusqlite::Connection::open(path)?;

    // Encryption key MUST be the very first operation on the connection.
    // Applying PRAGMA journal_mode=WAL before the key would fail on an
    // already-encrypted database.
    if let Ok(cfg_lock) = TylluanConfig::load_cached() {
        if let Ok(cfg) = cfg_lock.try_read() {
            if cfg.security.encrypt_at_rest {
                #[cfg(feature = "encryption")]
                {
                    match std::env::var("TYLLUAN_DB_KEY") {
                        Ok(key_hex) => {
                            if !key_hex.chars().all(|c| c.is_ascii_hexdigit()) || key_hex.len() != 64 {
                                return Err(anyhow::anyhow!(
                                    "TYLLUAN_DB_KEY must be a 64-character hex string \
                                     (generate with: openssl rand -hex 32)"
                                ));
                            }
                            conn.pragma_update(None, "hexkey", &key_hex)?;
                            // Verify the key is correct before proceeding
                            conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))
                                .map_err(|_| anyhow::anyhow!(
                                    "Encryption key rejected for {}: wrong TYLLUAN_DB_KEY or \
                                     database was not encrypted with SQLCipher",
                                    path.display()
                                ))?;
                            tracing::info!("🔐 SQLCipher encryption active: {}", path.display());
                        }
                        Err(_) => {
                            tracing::warn!(
                                "⚠️ encrypt_at_rest=true but TYLLUAN_DB_KEY not set \
                                 — {} is NOT encrypted",
                                path.display()
                            );
                        }
                    }
                }
                #[cfg(not(feature = "encryption"))]
                {
                    tracing::error!(
                        "encrypt_at_rest=true but binary was not compiled with encryption support. \
                         Rebuild with: cargo build --features encryption"
                    );
                }
            }
        }
    }

    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
    Ok(conn)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_sandbox_image")]
    pub image: String,
    #[serde(default = "default_sandbox_memory")]
    pub memory: String,
    #[serde(default)]
    pub network: bool,
    #[serde(default = "default_sandbox_timeout_secs")]
    pub timeout_secs: u64,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            image: default_sandbox_image(),
            memory: default_sandbox_memory(),
            network: false,
            timeout_secs: default_sandbox_timeout_secs(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclConfig {
    #[serde(default = "default_acl_default_role")]
    pub default_role: String,
    #[serde(default)]
    pub roles: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub tokens: HashMap<String, String>,
}

impl Default for AclConfig {
    fn default() -> Self {
        Self {
            default_role: default_acl_default_role(),
            roles: HashMap::new(),
            tokens: HashMap::new(),
        }
    }
}

/// Load sandbox config from global cache if enabled.
pub fn load_sandbox_config() -> Option<SandboxConfig> {
    if let Ok(cached) = TylluanConfig::load_cached() {
        if let Ok(cfg) = cached.try_read() {
            if cfg.security.sandbox.enabled {
                return Some(cfg.security.sandbox.clone());
            }
        }
    }
    None
}

// ─── Defaults ───────────────────────────────────────────────────────

fn default_host() -> String { "0.0.0.0".into() }
fn default_port() -> u16 { 3030 }
fn default_transports() -> Vec<String> { vec!["stdio".into(), "http".into(), "sse".into()] }
fn default_db_path() -> String { "./data/tylluan.db".into() }
fn default_embedding_model() -> String { "Nomic-Embed-v2".into() }
fn default_dimensions() -> u32 { 1024 }
fn default_vision_model_path() -> String { "HuggingFaceTB/SmolVLM2-256M-Instruct".into() }
fn default_tui_enabled() -> bool { true }
fn default_refresh_ms() -> u64 { 1000 }
fn default_always_on() -> Vec<String> { vec!["bash".into(), "memory".into(), "filesystem".into()] }
fn default_lazy_timeout() -> u64 { 300 }
fn default_handshake_secs() -> u64 { 120 }     // 2 mins default
fn default_tool_call_secs() -> u64 { 3600 }   // 1 hour default (for slow models)
fn default_bool_false() -> bool { false }
fn default_bool_true() -> bool { true }
fn default_wsl_port() -> u16 { 3031 }
fn default_silva_db_path() -> String { "./data/silva.db".into() }
fn default_sync_interval() -> u64 { 5000 }
fn default_decay_interval_hours() -> u64 { 6 }
fn default_decay_prune_threshold() -> f64 { 0.15 }
fn default_system_guild_ms() -> u64 { 15_000 }
fn default_analysis_guild_ms() -> u64 { 60_000 }
fn default_heavy_guild_ms() -> u64 { 180_000 }
fn default_mcp_heartbeat_ms() -> u64 { 8_000 }
fn default_sandbox_image() -> String { "python:3.12-slim".to_string() }
fn default_sandbox_memory() -> String { "512m".to_string() }
fn default_sandbox_timeout_secs() -> u64 { 30 }
fn default_acl_default_role() -> String { "admin".to_string() }

/// Guild category for timeout assignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum GuildWeight {
    #[default]
    Light,
    Medium,
    Heavy,
}

impl GuildWeight {
    /// Return the default timeout in milliseconds for this guild weight.
    pub fn default_timeout_ms(&self) -> u64 {
        match self {
            GuildWeight::Light => 15_000,
            GuildWeight::Medium => 60_000,
            GuildWeight::Heavy => 180_000,
        }
    }
}

// ─── Config Caching + Watcher ─────────────────────────────────────────

static CONFIG_CACHE: std::sync::OnceLock<Arc<RwLock<TylluanConfig>>> = std::sync::OnceLock::new();

impl TylluanConfig {
    pub fn security_intent_filter_enabled(&self) -> bool {
        self.security.intent_filter
    }

    /// Load config once and cache it. Returns cached config if already loaded.
    pub fn load_cached() -> anyhow::Result<Arc<RwLock<TylluanConfig>>> {
        if let Some(cached) = CONFIG_CACHE.get() {
            return Ok(cached.clone());
        }
        
        let config = Self::load()?;
        let shared = Arc::new(RwLock::new(config));
        CONFIG_CACHE.set(shared.clone()).ok();
        
        info!("📁 Config loaded and cached");
        Ok(shared)
    }

    /// Manual reload from file (for API endpoint or manual trigger)
    pub async fn reload() -> anyhow::Result<()> {
        if let Some(cached) = CONFIG_CACHE.get() {
            let new_config = Self::load()?;
            let mut guard = cached.write().await;
            *guard = new_config;
            info!("🔄 Config reloaded manually");
        }
        Ok(())
    }
}

// ─── Config Loading ─────────────────────────────────────────────────

impl TylluanConfig {
    /// Load configuration from `tylluan.toml` in the current directory,
    /// or fall back to sensible defaults if no config file exists.
    pub fn load() -> anyhow::Result<Self> {
        let config_path = Self::find_config_file();

        let mut config = if let Some(path) = &config_path {
            info!("📄 Loading config from: {}", path.display());
            let content = std::fs::read_to_string(path)?;
            toml::from_str(&content)?
        } else {
            info!("📄 No tylluan.toml found, using defaults.");
            Self::default()
        };

        config.validate_security();

        Ok(config)
    }

    /// Validates security constraints and normalizes dangerous configurations.
    pub fn validate_security(&mut self) {
        if self.nexus.dev_mode && self.nexus.host != "127.0.0.1" && self.nexus.host != "localhost" {
            warn!(
                "CRITICAL_SECURITY_TRIGGER: dev_mode is enabled but host is set to '{}'. Forcing host to '127.0.0.1' for safety.",
                self.nexus.host
            );
            self.nexus.host = "127.0.0.1".to_string();
        }
    }

    pub fn find_config_file() -> Option<PathBuf> {
        // Check current directory first
        let local = Path::new("tylluan.toml");
        if local.exists() {
            return Some(local.to_path_buf());
        }

        // Check user config directory
        if let Some(config_dir) = dirs::config_dir() {
            let global = config_dir.join("tylluan-nexus").join("tylluan.toml");
            if global.exists() {
                return Some(global);
            }
        }

        None
    }

    /// Ensure an auth token exists. Priority:
    /// 1. Environment variable TYLLUAN_TOKEN
    /// 2. Local file .tylluan-token
    /// 3. Randomly generated (and saved to .tylluan-token) if dev_mode is false
    pub fn ensure_auth_token(&self) -> anyhow::Result<Option<String>> {
        // 0. Dev mode bypass
        if self.nexus.dev_mode {
            info!("🔓 Dev mode enabled: authentication disabled");
            return Ok(None);
        }

        // 1. Check environment variable (highest priority)
        if let Ok(token) = std::env::var("TYLLUAN_TOKEN") {
            let trimmed = token.trim();
            if !trimmed.is_empty() {
                info!("🔐 Auth: Using token from TYLLUAN_TOKEN environment variable");
                return Ok(Some(trimmed.to_string()));
            }
        }

        // 2. Check .tylluan-token file
        let token_path = Path::new(".tylluan-token");
        if token_path.exists() {
            let content = std::fs::read_to_string(token_path)?;
            let trimmed = content.trim();
            if !trimmed.is_empty() {
                info!("🔐 Auth: Using token from .tylluan-token file");
                return Ok(Some(trimmed.to_string()));
            }
        }

        // 3. Generate random token if missing (Sovereign Auto-Security)
        warn!("⚠️ No authentication token found (TYLLUAN_TOKEN or .tylluan-token).");
        info!("🔐 Generating a new secure Master Token...");
        
        use rand::{Rng, distributions::Alphanumeric};
        let new_token: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();

        if let Err(e) = std::fs::write(token_path, &new_token) {
            error!("❌ Failed to write .tylluan-token: {}. Security compromised.", e);
            anyhow::bail!("Security violation: cannot persist auth token");
        }

        info!("✅ New Master Token saved to .tylluan-token");
        info!("💡 TIP: Add this token to your Dashboard or set it as TYLLUAN_TOKEN env var.");

        Ok(Some(new_token))
    }
}

/// Persist federation_peers to tylluan.toml, preserving all other config.
pub fn persist_federation_peers(config: &TylluanConfig, config_path: &std::path::Path) -> anyhow::Result<()> {
    let content = toml::to_string_pretty(config)?;
    std::fs::write(config_path, content)?;
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = TylluanConfig::default();
        assert_eq!(config.nexus.port, 3030);
        assert_eq!(config.memory.vector_dimensions, 1024); // BGE-M3 nativo
        assert_eq!(config.guilds.core.always_on, vec!["bash", "memory", "filesystem"]);
        assert_eq!(config.guilds.core.lazy_load_timeout_secs, 300);
    }

    #[test]
    fn test_parse_minimal_toml() {
        let toml_str = r#"
[nexus]
port = 4000
"#;
        let config: TylluanConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.nexus.port, 4000);
        // Defaults should still work
        assert_eq!(config.memory.embedding_model, "Nomic-Embed-v2"); // Updated to Nomic v2
    }

    #[test]
    fn test_parse_external_mcps() {
        let toml_str = r#"
[[external_mcp]]
name = "github"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[[external_mcp]]
name = "slack"
url = "https://slack.example.com/sse"
"#;
        let config: TylluanConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.external_mcp.len(), 2);
        assert_eq!(config.external_mcp[0].name, "github");
        assert_eq!(config.external_mcp[1].url, Some("https://slack.example.com/sse".into()));
    }

    #[test]
    fn test_security_validation() {
        let mut config = TylluanConfig::default();
        config.nexus.dev_mode = true;
        config.nexus.host = "0.0.0.0".to_string();
        config.validate_security();
        assert_eq!(config.nexus.host, "127.0.0.1");
        
        config.nexus.host = "192.168.1.50".to_string();
        config.validate_security();
        assert_eq!(config.nexus.host, "127.0.0.1");

        config.nexus.host = "127.0.0.1".to_string();
        config.validate_security();
        assert_eq!(config.nexus.host, "127.0.0.1");
    }
}
