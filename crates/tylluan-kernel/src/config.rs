//! Configuration system for TylluanNexus.
//!
//! Reads from `tylluan.toml` in the current directory or the default config path.
//! Auto-generates a random auth token on first run if none is set.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use tokio::sync::RwLock;
use tracing::{info, warn, error};
use std::fs;
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::Digest;

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

    /// M31-P0: deterministic pre/post hooks around the 5 sovereign tools.
    /// `[[hooks]]` array-of-tables in TOML, see security::hooks::HookRule.
    #[serde(default)]
    pub hooks: Vec<crate::security::hooks::HookRule>,

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
    pub mesh: MeshConfig,

    #[serde(default)]
    pub p2p: P2pConfig,

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshConfig {
    #[serde(default = "default_mesh_bootstrap_enabled")]
    pub bootstrap_enabled: bool,
    #[serde(default = "default_mainline_dht_enabled")]
    pub mainline_dht_enabled: bool,
    #[serde(default)]
    pub seed_nodes: Vec<String>,
    #[serde(default)]
    pub gossip: GossipConfig,
}
fn default_mesh_bootstrap_enabled() -> bool { true }
fn default_mainline_dht_enabled() -> bool { true }

impl Default for MeshConfig {
    fn default() -> Self {
        Self {
            bootstrap_enabled: true,
            mainline_dht_enabled: true,
            seed_nodes: Vec::new(),
            gossip: GossipConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipConfig {
    #[serde(default = "default_gossip_enabled")]
    pub enabled: bool,
    #[serde(default = "default_gossip_interval")]
    pub interval_secs: u64,
    #[serde(default = "default_gossip_fanout")]
    pub fanout: usize,
    #[serde(default = "default_gossip_max_entries")]
    pub max_entries: usize,
    #[serde(default = "default_gossip_max_peer_cursors")]
    pub max_peer_cursors: usize,
    #[serde(default = "default_gossip_timeout_secs")]
    pub timeout_secs: u64,
}
fn default_gossip_enabled() -> bool { true }
fn default_gossip_interval() -> u64 { 30 }
fn default_gossip_fanout() -> usize { 3 }
fn default_gossip_max_entries() -> usize { 1000 }
fn default_gossip_max_peer_cursors() -> usize { 100 }
fn default_gossip_timeout_secs() -> u64 { 5 }

impl Default for GossipConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_secs: 30,
            fanout: 3,
            max_entries: 1000,
            max_peer_cursors: 100,
            timeout_secs: 5,
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
pub struct P2pConfig {
    /// Enable the P2P dispatch listener for direct Noise XK connections.
    #[serde(default = "default_p2p_enabled")]
    pub enabled: bool,
    /// TCP port for the P2P dispatch listener.
    #[serde(default = "default_p2p_listen_port")]
    pub listen_port: u16,
}
fn default_p2p_enabled() -> bool { true }
fn default_p2p_listen_port() -> u16 { 9123 }
impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            listen_port: 9123,
        }
    }
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
    /// When true, destructive guilds (bash, filesystem write, docker, etc.)
    /// simulate execution and return output marked [DRY-RUN].
    /// Useful for safely previewing workflows.
    #[serde(default)]
    pub dry_run: bool,
}

fn default_max_concurrent() -> usize { 3 }

impl Default for GuildsConfig {
    fn default() -> Self {
        Self {
            core: CoreGuildsConfig::default(),
            v2: None,
            guild_max_concurrent_calls: default_max_concurrent(),
            dry_run: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreGuildsConfig {
    #[serde(default = "default_always_on")]
    pub always_on: Vec<String>,

    /// Guilds to pre-warm at boot (started once, NOT kept alive by idle watchdog).
    /// Unlike always_on, these CAN be killed by idle timeout after inactivity.
    /// Useful for frequently-used lazy guilds that shouldn't cold-start on first call.
    #[serde(default)]
    pub warm_pool: Vec<String>,

    #[serde(default = "default_lazy_timeout")]
    pub lazy_load_timeout_secs: u64,
}

impl Default for CoreGuildsConfig {
    fn default() -> Self {
        Self {
            always_on: default_always_on(),
            warm_pool: Vec::new(),
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
    Coreml,
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

    #[serde(default = "default_decay_half_life_hours")]
    pub decay_half_life_hours: u64,

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
            decay_half_life_hours: default_decay_half_life_hours(),
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
    /// Blocks destructive intents (rm -rf /, DROP TABLE, fork bombs, etc.)
    /// across the whole tylluan_do dispatch path, not just the bash guild.
    /// Defaults to true (safe-by-default): a fresh install with no config
    /// key set should not silently run unprotected. Was `false` until
    /// 2026-07-12 -- the filter existed with 13 passing tests (including
    /// explicit allow-cases for common safe intents to guard against false
    /// positives) but was opt-in, so a new user following
    /// tylluan.example.toml (which left it commented out) got zero
    /// protection from this layer by default.
    #[serde(default = "default_intent_filter")]
    pub intent_filter: bool,
    /// Opt-in runtime enforcement for guild capability declarations.
    /// When true, guilds with declared CAPABILITIES are blocked from
    /// performing operations outside their declared scope (process_execution
    /// and filesystem_scope only — network_hosts is advisory-only).
    /// Defaults to false: maintaining existing advisory-only behavior.
    /// Guilds without capabilities (null) are never affected.
    #[serde(default = "default_capabilities_enforce")]
    pub capabilities_enforce: bool,
    #[serde(default)]
    pub sandbox: SandboxConfig,
    #[serde(default)]
    pub acl: AclConfig,
    /// Enable SQLCipher encryption at rest. Defaults to true ONLY when the binary
    /// was compiled with the `encryption` feature — that feature is not in the
    /// default feature set (bundles SQLCipher+OpenSSL from source, unsupported on
    /// Windows native). Defaulting this to true unconditionally would silently
    /// report "encrypted" on standard builds that cannot actually encrypt.
    /// Key resolved from: TYLLUAN_DB_KEY env var > OS keychain > file fallback.
    #[serde(default = "default_encrypt_at_rest")]
    pub encrypt_at_rest: bool,
}

fn default_encrypt_at_rest() -> bool {
    cfg!(feature = "encryption")
}

fn default_intent_filter() -> bool { true }
fn default_capabilities_enforce() -> bool { false }

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            intent_filter: default_intent_filter(),
            capabilities_enforce: default_capabilities_enforce(),
            sandbox: SandboxConfig::default(),
            acl: AclConfig::default(),
            encrypt_at_rest: default_encrypt_at_rest(),
        }
    }
}

/// Open a SQLite connection with optional SQLCipher encryption.
/// When [security] encrypt_at_rest = true (default), the encryption key is
/// resolved via: TYLLUAN_DB_KEY env var > .tylluan-db-key file > auto-generate.
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
    if let Ok(cfg_lock) = TylluanConfig::load_cached()
        && let Ok(cfg) = cfg_lock.try_read()
            && cfg.security.encrypt_at_rest {
                let data_dir = path.parent().unwrap_or(Path::new("."));
                let key_hex = ensure_db_key(data_dir)?;

                #[cfg(feature = "encryption")]
                {
                    conn.pragma_update(None, "hexkey", &key_hex)?;
                    conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))
                        .map_err(|_| anyhow::anyhow!(
                            "Encryption key rejected for {}: wrong key or \
                             database was not encrypted with SQLCipher",
                            path.display()
                        ))?;
                    tracing::info!("🔐 SQLCipher encryption active: {}", path.display());
                }
                #[cfg(not(feature = "encryption"))]
                {
                    tracing::error!(
                        "encrypt_at_rest=true but binary was not compiled with encryption support. \
                         Rebuild with: cargo build --features encryption"
                    );
                    let _ = key_hex;
                }
            }

    // M21-P1: cache_size/mmap_size/synchronous tuning applied here so every
    // caller of open_db() benefits uniformly (15+ call sites: jobs, mailbox,
    // agent_profiles, curriculum, federation peers, coloquio, registry,
    // contracts, journal, audit log) instead of only the two DBs (hybrid.rs,
    // silva/schema.rs) that happened to add the same PRAGMAs manually after
    // their own open_db() call. Setting these PRAGMAs twice on those two is
    // harmless (idempotent) -- left as-is rather than touched, to avoid
    // colliding with concurrent M21-P0 work on the same files.
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; \
         PRAGMA busy_timeout=5000; \
         PRAGMA synchronous=NORMAL; \
         PRAGMA cache_size=-65536; \
         PRAGMA mmap_size=268435456;"
    )?;
    Ok(conn)
}

// Resolve the DB encryption key with priority:
// 1. `TYLLUAN_DB_KEY` env var (64-char hex) — explicit operator override, e.g. injected
//    from a vault/secrets manager in server/Docker deployments.
// 2. OS keychain (Windows Credential Manager / macOS Keychain / Linux Secret Service).
//    The key never touches the data directory — it is tied to the OS user account,
//    so copying the DB file or the data directory alone does not leak the key.
// 3. File-based fallback (`.tylluan-db-key`, derived with Argon2id) — ONLY used when
//    no keychain backend is available (e.g. headless Linux/Docker without a Secret
//    Service daemon). This mode does NOT protect against filesystem/disk access —
//    the seed lives next to the encrypted DB. Operators on server/Docker profiles
//    should set `TYLLUAN_DB_KEY` explicitly for real at-rest protection.

/// Quick check for DBus availability on Linux (fails fast in Docker/headless).
/// Prevents keyring from hanging for ~25s on zbus connection timeout.
fn dbus_is_available() -> bool {
    // Session bus: $XDG_RUNTIME_DIR/bus  (e.g. /run/user/1000/bus)
    if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
        let session_bus = std::path::Path::new(&runtime).join("bus");
        if session_bus.exists() {
            return true;
        }
    }
    // System bus: /run/dbus/system_bus_socket
    if std::path::Path::new("/run/dbus/system_bus_socket").exists() {
        return true;
    }
    // Explicit env var override
    if std::env::var("DBUS_SESSION_BUS_ADDRESS").is_ok() {
        return true;
    }
    false
}

fn ensure_db_key(data_dir: &Path) -> anyhow::Result<String> {
    if let Ok(key_hex) = std::env::var("TYLLUAN_DB_KEY") {
        if key_hex.chars().all(|c| c.is_ascii_hexdigit()) && key_hex.len() == 64 {
            return Ok(key_hex);
        }
        anyhow::bail!(
            "TYLLUAN_DB_KEY must be a 64-character hex string \
             (generate with: openssl rand -hex 32)"
        );
    }

    // On Linux, detect DBus presence EARLY to avoid a ~25s zbus timeout
    // inside keyring::get_password() when no DBus daemon is running
    // (common in Docker / headless CI environments).
    if cfg!(target_os = "linux") && !dbus_is_available() {
        tracing::warn!("No DBus detected on Linux — skipping OS keychain. \
             File-based key fallback does NOT protect against filesystem access. \
             Set TYLLUAN_DB_KEY explicitly for real at-rest protection.");
        return file_based_key_fallback(data_dir);
    }

    let account = data_dir.to_string_lossy().to_string();
    match keyring::Entry::new("tylluan-nexus-db", &account) {
        Ok(entry) => match entry.get_password() {
            Ok(key_hex) if key_hex.chars().all(|c| c.is_ascii_hexdigit()) && key_hex.len() == 64 => {
                tracing::info!("🔐 DB encryption key loaded from OS keychain");
                return Ok(key_hex);
            }
            Ok(_) => {
                tracing::warn!("OS keychain entry for tylluan-nexus-db is corrupt — regenerating");
            }
            Err(keyring::Error::NoEntry) => {
                let mut raw = [0u8; 32];
                OsRng.fill_bytes(&mut raw);
                let key_hex: String = raw.iter().map(|b| format!("{b:02x}")).collect();
                match entry.set_password(&key_hex) {
                    Ok(()) => {
                        tracing::info!("🔑 Generated DB encryption key, stored in OS keychain (never written to disk)");
                        return Ok(key_hex);
                    }
                    Err(e) => {
                        tracing::warn!("Could not store key in OS keychain ({e}) — falling back to file-based key. \
                             This does NOT protect against filesystem/disk access. \
                             Set TYLLUAN_DB_KEY explicitly for real at-rest protection.");
                    }
                }
            }
            Err(e) => {
                tracing::warn!("OS keychain unavailable ({e}) — falling back to file-based key. \
                     This does NOT protect against filesystem/disk access. \
                     Set TYLLUAN_DB_KEY explicitly for real at-rest protection.");
            }
        },
        Err(e) => {
            tracing::warn!("OS keychain unavailable ({e}) — falling back to file-based key. \
                 This does NOT protect against filesystem/disk access. \
                 Set TYLLUAN_DB_KEY explicitly for real at-rest protection.");
        }
    }

    file_based_key_fallback(data_dir)
}

/// Last-resort key storage for environments without an OS keychain (headless
/// Linux/Docker without Secret Service). The seed sits next to the encrypted
/// DB, so this does NOT protect against an attacker with filesystem access —
/// it only guards against e.g. accidentally syncing just the `.db` file
/// without its sibling key file.
fn file_based_key_fallback(data_dir: &Path) -> anyhow::Result<String> {
    let key_path = data_dir.join(".tylluan-db-key");

    if key_path.exists() {
        let seed = fs::read(&key_path)
            .map_err(|e| anyhow::anyhow!("Cannot read {}: {}", key_path.display(), e))?;
        if seed.len() != 32 {
            anyhow::bail!(
                "{} must be exactly 32 bytes (got {}). Delete it to auto-regenerate.",
                key_path.display(),
                seed.len()
            );
        }
        return derive_key_argon2(&seed, data_dir);
    }

    let mut seed = [0u8; 32];
    OsRng.fill_bytes(&mut seed);
    fs::write(&key_path, seed)
        .map_err(|e| anyhow::anyhow!("Cannot write {}: {}", key_path.display(), e))?;
    tracing::info!("🔑 Generated file-based DB encryption key at {} (no keychain available)", key_path.display());

    derive_key_argon2(&seed, data_dir)
}

/// Derive a 64-char hex SQLCipher key from a 32-byte seed using Argon2id.
/// Uses SHA-256(seed + data_dir) as the deterministic salt (16 bytes).
fn derive_key_argon2(seed: &[u8], data_dir: &Path) -> anyhow::Result<String> {
    let mut hasher = sha2::Sha256::new();
    hasher.update(seed);
    hasher.update(data_dir.to_string_lossy().as_bytes());
    let hash = hasher.finalize();

    let mut salt = [0u8; 16];
    salt.copy_from_slice(&hash[..16]);

    let argon2 = argon2::Argon2::default();
    let mut key = [0u8; 32];
    argon2
        .hash_password_into(seed, &salt, &mut key)
        .map_err(|e| anyhow::anyhow!("Argon2id key derivation failed: {e}"))?;

    Ok(key.iter().map(|b| format!("{b:02x}")).collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum SandboxProfile {
    /// Full isolation: Docker for all guilds, process_execution=false,
    /// network=none, filesystem=read-only workspace.
    Strict,
    /// Moderate isolation: Docker for bash/code only, enforcement per
    /// declared capabilities, network/filesystem per guild declaration.
    /// This is the DEFAULT — backward compatible with pre-M30-P1 behavior.
    #[default]
    Balanced,
    /// No isolation: no Docker, process_execution allowed, full network
    /// and filesystem access. Advisory-only capability declarations.
    Permissive,
}

impl SandboxProfile {
    pub fn is_strict(&self) -> bool { matches!(self, SandboxProfile::Strict) }
    pub fn is_balanced(&self) -> bool { matches!(self, SandboxProfile::Balanced) }
    pub fn is_permissive(&self) -> bool { matches!(self, SandboxProfile::Permissive) }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub profile: SandboxProfile,
    /// Per-guild overrides (level 2 in cascade).
    /// Example: `{ "bash": "strict" }` overrides the global profile for the bash guild.
    /// Stored in TOML under `[security.sandbox.guild_overrides]`.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub guild_overrides: HashMap<String, SandboxProfile>,
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
            profile: SandboxProfile::default(),
            guild_overrides: HashMap::new(),
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
    /// M31-P1: Optional binding of bearer tokens to a fixed agent_id.
    /// When set, requests using this token MUST supply the matching agent_id
    /// in tool call arguments; cross-agent impersonation is denied.
    #[serde(default)]
    pub token_agent_bindings: HashMap<String, String>,
    /// M31-P1: Per-agent_id permission rules, keyed by agent_id.
    /// scope: "read-only", "read-write", or "admin"
    /// denied_tools: tools this agent cannot call (by name, e.g. "tylluan_graph")
    /// memory_isolation: if true, tylluan_recall only returns this agent's own episodes
    #[serde(default)]
    pub agent_permissions: HashMap<String, AgentPermission>,
}

impl Default for AclConfig {
    fn default() -> Self {
        Self {
            default_role: default_acl_default_role(),
            roles: HashMap::new(),
            tokens: HashMap::new(),
            token_agent_bindings: HashMap::new(),
            agent_permissions: HashMap::new(),
        }
    }
}

/// M31-P1: Per-agent permission rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPermission {
    #[serde(default = "default_agent_scope")]
    pub scope: String,
    #[serde(default)]
    pub denied_tools: Vec<String>,
    #[serde(default)]
    pub memory_isolation: bool,
}

fn default_agent_scope() -> String {
    "read-write".to_string()
}

/// Load sandbox config from global cache if enabled.
pub fn load_sandbox_config() -> Option<SandboxConfig> {
    if let Ok(cached) = TylluanConfig::load_cached()
        && let Ok(cfg) = cached.try_read()
            && cfg.security.sandbox.enabled {
                return Some(cfg.security.sandbox.clone());
            }
    None
}

/// Load the active sandbox profile, or default if sandbox is disabled.
pub fn load_sandbox_profile() -> SandboxProfile {
    if let Ok(cached) = TylluanConfig::load_cached()
        && let Ok(cfg) = cached.try_read()
            && cfg.security.sandbox.enabled {
                return cfg.security.sandbox.profile;
            }
    SandboxProfile::Balanced
}

// ─── M30-P2: Hierarchical profile override (session > guild > global) ───

/// In-memory session-level profile overrides, keyed by agent_id.
/// NOT persisted — lives only while the kernel runs.
static SESSION_OVERRIDES: OnceLock<RwLock<HashMap<String, SandboxProfile>>> = OnceLock::new();

fn session_overrides() -> &'static RwLock<HashMap<String, SandboxProfile>> {
    SESSION_OVERRIDES.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Set a session-level override for a given agent_id.
pub async fn set_session_override(agent_id: &str, profile: SandboxProfile) {
    session_overrides().write().await.insert(agent_id.to_string(), profile);
}

/// Remove a session-level override for a given agent_id.
pub async fn clear_session_override(agent_id: &str) {
    session_overrides().write().await.remove(agent_id);
}

/// Load the effective guild-level override from TOML config.
fn load_guild_override(guild_name: &str) -> Option<SandboxProfile> {
    if let Ok(cached) = TylluanConfig::load_cached()
        && let Ok(cfg) = cached.try_read()
            && cfg.security.sandbox.enabled {
                let o = cfg.security.sandbox.guild_overrides.get(guild_name).copied();
                if o.is_some() {
                    return o;
                }
            }
    None
}

/// Resolve the effective sandbox profile using the hierarchical cascade:
///   session (agent_id) > guild > global
///
/// Returns `(SandboxProfile, origin)` where origin is one of
/// `"session"`, `"guild"`, or `"global"`.
///
/// ## Asymmetry (documented):
/// Session-level overrides only affect enforcement (check_capabilities)
/// and dry-run classification. They do NOT affect Docker spawn decisions —
/// a guild is launched once per kernel start, not per-agent, so the guild
/// and global levels are the only ones that can decide Docker isolation.
pub async fn resolve_effective_profile(guild_name: &str, agent_id: &str) -> (SandboxProfile, &'static str) {
    // 1. Session override (highest precedence)
    {
        let overrides = session_overrides().read().await;
        if let Some(profile) = overrides.get(agent_id) {
            return (*profile, "session");
        }
    }

    // 2. Guild override
    if let Some(profile) = load_guild_override(guild_name) {
        return (profile, "guild");
    }

    // 3. Global default
    let global = load_sandbox_profile();
    (global, "global")
}

/// Load the effective Docker-scope profile for a guild.
/// Session overrides are intentionally excluded — Docker isolation is a
/// per-guild concern, not per-agent. See `resolve_effective_profile` docs.
pub async fn resolve_docker_profile(guild_name: &str) -> (SandboxProfile, &'static str) {
    // 1. Guild override
    if let Some(profile) = load_guild_override(guild_name) {
        return (profile, "guild");
    }

    // 2. Global default
    let global = load_sandbox_profile();
    (global, "global")
}

/// Persist a guild override to `permissive` in tylluan.toml via targeted edit.
/// Used by the grant engine (M30-P3) when a user chooses "always_for_guild".
/// Returns an error message string on failure, Ok(()) on success.
pub async fn persist_guild_override(guild_name: &str) -> Result<(), String> {
    let config_path = TylluanConfig::find_config_file()
        .unwrap_or_else(|| std::path::PathBuf::from("tylluan.toml"));
    let raw = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("cannot read config: {e}"))?;

    let guild_key = format!("guild_overrides.\"{}\"", guild_name.trim());
    let target_line = format!("{guild_key} = \"permissive\"");

    let mut in_guild_overrides = false;
    let mut replaced = false;
    let mut saw_sandbox_section = false;
    let mut saw_guild_overrides = false;

    let new_raw: String = raw.lines().map(|l| {
        let trimmed = l.trim_start();
        if trimmed.starts_with('[') {
            in_guild_overrides = trimmed.starts_with("[security.sandbox.guild_overrides]");
            if trimmed.starts_with("[security.sandbox]") { saw_sandbox_section = true; }
            if trimmed.starts_with("[security.sandbox.guild_overrides]") { saw_guild_overrides = true; }
        } else if in_guild_overrides && !replaced
            && trimmed.starts_with(&format!("\"{}\"", guild_name.trim()))
            && trimmed.contains('=')
        {
            replaced = true;
            return format!("{guild_key} = \"permissive\"");
        }
        l.to_string()
    }).collect::<Vec<_>>().join("\n");

    let new_raw = if replaced {
        new_raw
    } else if saw_guild_overrides {
        let mut out = String::new();
        let mut inserted = false;
        let mut in_override_section = false;
        for l in new_raw.lines() {
            let trimmed = l.trim_start();
            if trimmed.starts_with("[security.sandbox.guild_overrides]") {
                in_override_section = true;
            } else if in_override_section && trimmed.starts_with('[') {
                if !inserted {
                    out.push_str(&format!("{target_line}\n"));
                    inserted = true;
                }
                in_override_section = false;
            }
            out.push_str(l);
            out.push('\n');
        }
        if !inserted { out.push_str(&format!("{target_line}\n")); }
        out
    } else if saw_sandbox_section {
        // Append new section at the end
        format!("{}\n[security.sandbox.guild_overrides]\n{}\n", new_raw.trim_end(), target_line)
    } else {
        format!("{}\n\n[security.sandbox]\n[security.sandbox.guild_overrides]\n{}\n", new_raw.trim_end(), target_line)
    };

    if let Err(e) = toml::from_str::<TylluanConfig>(&new_raw) {
        return Err(format!("refusing to write invalid TOML: {e}"));
    }
    let tmp_path = config_path.with_extension("toml.tmp");
    std::fs::write(&tmp_path, &new_raw)
        .map_err(|e| format!("cannot write temp file: {e}"))?;
    std::fs::rename(&tmp_path, &config_path)
        .map_err(|e| format!("cannot rename temp file: {e}"))?;
    TylluanConfig::reload().await.map_err(|e| format!("reload failed: {e}"))?;
    Ok(())
}

fn default_host() -> String { "0.0.0.0".into() }
fn default_port() -> u16 { 3030 }
fn default_transports() -> Vec<String> { vec!["stdio".into(), "http".into(), "sse".into()] }
fn default_db_path() -> String { "./data/tylluan.db".into() }
fn default_embedding_model() -> String { "bge-m3".into() }
fn default_dimensions() -> u32 {
    crate::router::embeddings::resolve_dimension(&default_embedding_model())
}
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
fn default_decay_half_life_hours() -> u64 { 336 }  // 14 días
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
static CONFIG_PATH_OVERRIDE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

impl TylluanConfig {
    pub fn security_intent_filter_enabled(&self) -> bool {
        self.security.intent_filter
    }

    pub fn security_capabilities_enforce_enabled(&self) -> bool {
        self.security.capabilities_enforce
    }

    pub fn guilds_dry_run(&self) -> bool {
        self.guilds.dry_run
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

    /// Set by main.rs when `--config <path>` is passed on the CLI. Checked
    /// first by find_config_file() so that both the initial boot AND any
    /// later TylluanConfig::reload() call (e.g. from the sandbox-profile
    /// admin endpoints) keep reading from the CLI-specified file instead of
    /// silently falling back to cwd/default discovery. Without this, a
    /// process started with `--config /etc/tylluan/tylluan.toml` (the Docker
    /// image's setup) never actually applied that file to HttpState.config --
    /// load_cached() is a separate global cache from main.rs's local,
    /// CLI-overridden config variable, and would independently re-discover
    /// (or default) via find_config_file() with no knowledge of --config.
    pub fn set_config_path_override(path: PathBuf) {
        CONFIG_PATH_OVERRIDE.set(path).ok();
    }

    pub fn find_config_file() -> Option<PathBuf> {
        if let Some(overridden) = CONFIG_PATH_OVERRIDE.get() {
            return Some(overridden.clone());
        }

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

        info!("✅ New Master Token saved to {}.", token_path.display());

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
        assert_eq!(config.memory.embedding_model, "bge-m3");
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

    #[test]
    fn test_sandbox_profile_default_is_balanced() {
        let cfg = SandboxConfig::default();
        assert_eq!(cfg.profile, SandboxProfile::Balanced);
    }

    #[test]
    fn test_sandbox_profile_serde_roundtrip() {
        for profile in &[SandboxProfile::Strict, SandboxProfile::Balanced, SandboxProfile::Permissive] {
            let json = serde_json::to_string(profile).unwrap();
            let deserialized: SandboxProfile = serde_json::from_str(&json).unwrap();
            assert_eq!(*profile, deserialized);
        }
    }

    #[test]
    fn test_sandbox_profile_from_toml() {
        let toml_str = r#"
[security.sandbox]
enabled = true
profile = "strict"
"#;
        let config: TylluanConfig = toml::from_str(toml_str).unwrap();
        assert!(config.security.sandbox.enabled);
        assert_eq!(config.security.sandbox.profile, SandboxProfile::Strict);
    }

    #[test]
    fn test_sandbox_profile_from_toml_permissive() {
        let toml_str = r#"
[security.sandbox]
enabled = true
profile = "permissive"
"#;
        let config: TylluanConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.security.sandbox.profile, SandboxProfile::Permissive);
    }

    #[test]
    fn test_sandbox_profile_helpers() {
        assert!(SandboxProfile::Strict.is_strict());
        assert!(!SandboxProfile::Strict.is_balanced());
        assert!(!SandboxProfile::Strict.is_permissive());

        assert!(!SandboxProfile::Balanced.is_strict());
        assert!(SandboxProfile::Balanced.is_balanced());
        assert!(!SandboxProfile::Balanced.is_permissive());

        assert!(!SandboxProfile::Permissive.is_strict());
        assert!(!SandboxProfile::Permissive.is_balanced());
        assert!(SandboxProfile::Permissive.is_permissive());
    }

    // ─── M30-P2: Hierarchical cascade tests ─────────────────────────

    #[test]
    fn test_guild_overrides_deserialize_from_toml() {
        let toml_str = r#"
[security.sandbox]
enabled = true
profile = "balanced"

[security.sandbox.guild_overrides]
bash = "strict"
code = "permissive"
"#;
        let config: TylluanConfig = toml::from_str(toml_str).unwrap();
        assert!(config.security.sandbox.enabled);
        assert_eq!(config.security.sandbox.profile, SandboxProfile::Balanced);
        assert_eq!(config.security.sandbox.guild_overrides.get("bash"), Some(&SandboxProfile::Strict));
        assert_eq!(config.security.sandbox.guild_overrides.get("code"), Some(&SandboxProfile::Permissive));
        assert_eq!(config.security.sandbox.guild_overrides.len(), 2);
    }

    #[test]
    fn test_guild_overrides_roundtrip_skipped_when_empty() {
        let cfg = SandboxConfig::default();
        let toml_str = toml::to_string(&cfg).unwrap();
        // Empty guild_overrides should be skipped by skip_serializing_if
        assert!(!toml_str.contains("guild_overrides"));
    }

    #[test]
    fn test_guild_overrides_present_when_not_empty() {
        let mut cfg = SandboxConfig::default();
        cfg.guild_overrides.insert("bash".into(), SandboxProfile::Strict);
        let toml_str = toml::to_string(&cfg).unwrap();
        assert!(toml_str.contains("guild_overrides"));
    }

    #[tokio::test]
    async fn test_session_override_set_and_clear() {
        // Start clean
        clear_session_override("test-agent").await;
        let (_profile, origin) = resolve_effective_profile("bash", "test-agent").await;
        assert_eq!(origin, "global");

        set_session_override("test-agent", SandboxProfile::Permissive).await;
        let (profile, origin) = resolve_effective_profile("bash", "test-agent").await;
        assert_eq!(profile, SandboxProfile::Permissive);
        assert_eq!(origin, "session");

        clear_session_override("test-agent").await;
        let (_profile, origin) = resolve_effective_profile("bash", "test-agent").await;
        assert_eq!(origin, "global");
    }

    #[tokio::test]
    async fn test_session_precedence_over_guild_and_global() {
        clear_session_override("boss-agent").await;

        // With no override, falls back to global
        let (_profile, origin) = resolve_effective_profile("nonexistent", "boss-agent").await;
        assert_eq!(origin, "global");

        // Session wins over everything
        set_session_override("boss-agent", SandboxProfile::Permissive).await;
        // Even with global = strict, session takes priority
        let (profile, origin) = resolve_effective_profile("any-guild", "boss-agent").await;
        assert_eq!(profile, SandboxProfile::Permissive);
        assert_eq!(origin, "session");
    }

    #[tokio::test]
    async fn test_resolve_docker_profile_excludes_session() {
        clear_session_override("docker-test").await;
        set_session_override("docker-test", SandboxProfile::Permissive).await;

        // resolve_docker_profile should return global, not session
        let (_profile, origin) = resolve_docker_profile("bash").await;
        assert_eq!(origin, "global");
    }
}
