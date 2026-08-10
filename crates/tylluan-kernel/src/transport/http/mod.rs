//! # TylluanNexus HTTP Gateway
//!
//! Provides SSE, JSON-RPC (MCP), and Management API (V1).
//! Orchestrates routing using modular sub-routers.

pub mod auth;
pub mod oauth;
pub mod sse;
pub mod api_v1;
pub mod a2a;
pub mod mcp_apps;

use axum::{
    Router, Json,
    extract::{State, Query},
    http::{StatusCode, header, HeaderValue, Method},
    middleware,
    response::IntoResponse,
    routing::{get, post, any},
};
use axum::http::header::{CONTENT_TYPE, AUTHORIZATION};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::sync::RwLock;
use tracing::{info, error};
use std::time::Instant;
use dashmap::DashMap;
use tower_http::cors::CorsLayer;

use crate::registry::actor::RegistryHandle;
use crate::doctor::Doctor;

/// Wire format discriminators for encrypted gossip payloads.
/// Body format:
///   0x01 + [sender_node_id (32 ascii bytes)] + [Noise NK ciphertext]
///   0x02 + [sender_node_id (32 ascii bytes)] + [ChaCha20-Poly1305 ciphertext]
///   no prefix (legacy) = plaintext JSON
const GOSSIP_DISCR_NOISE: u8 = 0x01;
const GOSSIP_DISCR_CHACHA: u8 = 0x02;
const NODE_ID_BYTES: usize = 32;

/// Decrypt an inbound gossip wire payload.
/// Tries Noise NK (sender_id from prefix → pubkey lookup), then ChaCha20
/// shared_secret, then plaintext backward compat.
pub(crate) async fn gossip_decrypt_plaintext(
    data: &[u8],
    shared_secret: &str,
    identity: &tylluan_link::identity::NodeIdentity,
    routing_table: &tokio::sync::RwLock<tylluan_link::dht::RoutingTable>,
) -> Option<Vec<u8>> {
    if data.is_empty() {
        return None;
    }
    match data[0] {
        GOSSIP_DISCR_NOISE => {
            if data.len() < 1 + NODE_ID_BYTES { return None; }
            let sender_id = std::str::from_utf8(&data[1..1 + NODE_ID_BYTES]).ok()?;
            let ciphertext = &data[1 + NODE_ID_BYTES..];
            let pk = routing_table.read().await
                .all_peers().iter()
                .find(|e| e.node_id == sender_id)
                .and_then(|e| e.ed25519_pubkey.as_deref())
                .map(|s| s.to_string())?;
            tylluan_link::noise::noise_decrypt_payload(ciphertext, identity, &pk).ok()
        }
        GOSSIP_DISCR_CHACHA => {
            if data.len() < 1 + NODE_ID_BYTES { return None; }
            if shared_secret.is_empty() { return None; }
            crate::federation::decrypt_payload(&data[1 + NODE_ID_BYTES..], shared_secret).ok()
        }
        _ => {
            // No discriminator → treat as plaintext JSON (backward compat)
            Some(data.to_vec())
        }
    }
}

#[cfg(feature = "bundled-dashboard")]
#[derive(rust_embed::Embed)]
#[folder = "../../dashboard/dist/"]
struct DashboardAssets;
use crate::memory::hybrid::HybridMemory;
use crate::memory::silva::SilvaDB;
use crate::transport::server::TylluanServer;
use rmcp::model::{CallToolRequestParam, Content};
pub use tylluan_link::dispatch::DispatchQueue;
use tylluan_link::p2p::{P2pSessionPool, P2pHandlerFn, start_p2p_listener_noise};

/// Cached snapshot of guild statuses: (last-refreshed-at, statuses).
type GuildStatusCache = Arc<std::sync::Mutex<Option<(Instant, Vec<crate::registry::guild_process::GuildStatus>)>>>;

/// Shared application state for all HTTP handlers.
pub struct HttpState {
    pub version: String,
    pub auth_token: Option<String>,
    pub dev_mode: Option<bool>,
    pub server: Option<Arc<RwLock<TylluanServer>>>,
    pub registry: RegistryHandle,
    pub doctor: Arc<Doctor>,
    pub memory: Arc<HybridMemory>,
    pub silva: Arc<SilvaDB>,
    pub mailbox: Arc<crate::memory::mailbox::Mailbox>,
    pub coloquio: Arc<crate::memory::coloquio::ColoquioDb>,
    pub matcher: Arc<crate::router::matcher::GuildMatcher>,
    pub start_time: std::time::Instant,
    pub broadcast_tx: tokio::sync::broadcast::Sender<serde_json::Value>,
    pub download_progress_tx: tokio::sync::broadcast::Sender<crate::maintenance::DownloadProgress>,
    pub sessions: Arc<RwLock<HashMap<String, McpSession>>>,
    pub guild_status_cache: GuildStatusCache,
    pub agent_rate_limiter: Arc<dashmap::DashMap<String, (u32, Instant)>>,
    /// Per-IP rate limiting, independent of the client-controlled `X-Agent-Id`
    /// header/query param used by `agent_rate_limiter`. A caller can omit or
    /// rotate agent_id to fully evade the per-agent limiter; this catches
    /// that bypass at the connection level instead.
    pub ip_rate_limiter: Arc<crate::security::rate_limiter::RateLimiter>,
    pub config: Arc<RwLock<crate::config::TylluanConfig>>,
    pub tunnel_wsl_url: Option<String>,
    pub oauth: std::sync::Arc<oauth::OAuthState>,
    pub metrics_ring: Arc<tokio::sync::RwLock<crate::metrics_ring::MetricsRingBuffer>>,
    pub jobs: Arc<crate::memory::jobs::JobQueue>,
    pub cancel_token: tokio_util::sync::CancellationToken,
    pub node_router: Arc<crate::memory::agent_nodes::AgentNodeRouter>,
    pub journal: Arc<crate::transport::http::api_v1::api_journal::JournalDb>,
    pub agent_registry: crate::transport::http::api_v1::api_agents::AgentRegistry,
    pub contract_registry: crate::transport::http::api_v1::api_contracts::ContractRegistry,
    pub contract_db: Arc<crate::transport::http::api_v1::api_contracts::ContractDb>,
    pub peer_db: Arc<crate::federation::PeerDb>,
    pub health_ready: Arc<AtomicBool>,
    pub node_identity: Arc<tylluan_link::identity::NodeIdentity>,
    pub nat_cache: Arc<tokio::sync::RwLock<Option<tylluan_link::nat::ExternalAddr>>>,
    pub dht_routing_table: Arc<tokio::sync::RwLock<tylluan_link::dht::RoutingTable>>,
    pub gossip_engine: Arc<tokio::sync::RwLock<tylluan_link::gossip::GossipEngine>>,
    pub capability_registry: Arc<std::sync::Mutex<tylluan_link::capability::CapabilityRegistry>>,
    pub dispatch_router: Arc<std::sync::Mutex<tylluan_link::dispatch::DispatchRouter>>,
    pub dispatch_queue: Arc<std::sync::Mutex<DispatchQueue>>,
    pub p2p_pool: Arc<tokio::sync::Mutex<P2pSessionPool>>,
    pub repo_map: Arc<crate::repo_map::RepoMap>,
    pub a2a_task_manager: Arc<a2a::A2aTaskManager>,
    /// M19-P5: Declarative agent contract loaded from `.tylluan/agents.toml`.
    /// Empty contract when the file doesn't exist (fully optional feature).
    /// Used as a third-tier role resolution source after explicit token mappings
    /// and static ACL config — never overrides an already-resolved role.
    pub agents_contract: Arc<crate::security::agents_contract::AgentsContract>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct McpSession {
    pub id: String,
    pub client_name: String,
    pub agent_id: Option<String>,
    #[serde(skip, default = "Instant::now")]
    pub created_at: Instant,
    #[serde(skip, default = "Instant::now")]
    pub last_active: Instant,
    pub tool_count: u64,
    pub last_intent: Option<String>,
    pub last_guild: Option<String>,
    #[serde(default)]
    pub mcp_apps: bool,
    #[serde(default)]
    pub created_unix: u64,
    #[serde(default)]
    pub last_active_unix: u64,
}

/// Upsert a session: insert if new, update client_name/agent_id/last_active if existing.
/// Consolidates 3 formerly-duplicated upsert sites (api_v1.rs ×2, sse.rs).
pub async fn create_or_update_session(
    sessions: &Arc<tokio::sync::RwLock<std::collections::HashMap<String, McpSession>>>,
    key: &str,
    client_name: &str,
    agent_id: Option<&str>,
) {
    let mut guard = sessions.write().await;
    let now = Instant::now();
    let now_unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let entry = guard.entry(key.to_string()).or_insert_with(|| McpSession {
        id: key.to_string(),
        client_name: client_name.to_string(),
        agent_id: agent_id.map(|s| s.to_string()),
        created_at: now,
        last_active: now,
        created_unix: now_unix,
        last_active_unix: now_unix,
        tool_count: 0,
        last_intent: None,
        last_guild: None,
        mcp_apps: false,
    });
    entry.last_active = now;
    entry.last_active_unix = now_unix;
    entry.client_name = client_name.to_string();
    entry.agent_id = agent_id.map(|s| s.to_string());
}

// --- Shared Payloads ---

#[derive(Deserialize)]
pub struct EdgePayload {
    pub source: String,
    pub target: String,
    pub edge_type: String,
    #[serde(default = "default_metadata")]
    pub metadata: String,
    pub weight: Option<f64>,
}
fn default_metadata() -> String { "{}".to_string() }

#[derive(Deserialize)]
pub struct CreateNodePayload {
    pub content: String,
    #[serde(default = "default_node_type")]
    pub node_type: String,
    #[serde(default = "default_metadata")]
    pub metadata: String,
    pub weight: Option<f64>,
}
fn default_node_type() -> String { "entity".to_string() }

#[derive(Deserialize)]
pub struct EdgeSearchQuery {
    pub query: String,
    pub limit: Option<usize>,
}

#[derive(Serialize)]
pub struct EdgeSearchResult {
    pub source: String,
    pub target: String,
    pub edge_type: String,
    pub weight: f64,
    pub similarity: f64,
}

#[derive(Deserialize)]
pub struct SilvaQueryParams {
    pub limit: Option<usize>,
    pub min_weight: Option<f64>,
    pub node_type: Option<String>,
    pub cluster: Option<bool>,
}

#[derive(Deserialize, Default)]
pub struct SilvaRecentQuery {
    pub limit: Option<usize>,
}

#[derive(Deserialize, Default)]
pub struct MemorySearchQuery {
    pub q: Option<String>,
    pub query: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Deserialize, Default)]
pub struct DoIntentQuery {
    pub intent: Option<String>,
    pub context: Option<String>,
    pub agent_id: Option<String>,
    pub tool: Option<String>,
    pub query: Option<String>,
    pub guild: Option<String>,
}

#[derive(Deserialize)]
pub struct GuildRequest { pub name: String }

#[derive(Deserialize)]
pub struct GuildRegisterRequest {
    pub name: String,
    pub module_path: String,
    pub always_on: Option<bool>,
    pub timeout_ms: Option<u64>,
}

#[derive(Deserialize)]
pub struct SaveConfigRequest { pub content: String }

#[derive(Deserialize)]
pub struct BashExecuteRequest { pub command: String }

#[derive(Deserialize)]
pub struct ExportQuery {
    #[serde(default = "default_export_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}
fn default_export_limit() -> usize { 5000 }

// --- Initialization ---

pub async fn start_http_server(
    host: &str,
    port: u16,
    state: Arc<HttpState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let app = build_router(state);
    
    // Read the old port before we overwrite it to perform a true hot-swap
    let mut old_port = None;
    let active_port_file = std::path::Path::new("data/active_port.json");
    if active_port_file.exists()
        && let Ok(content) = std::fs::read_to_string(active_port_file)
            && let Ok(val) = serde_json::from_str::<serde_json::Value>(&content)
                && let Some(p) = val.get("port").and_then(|p| p.as_u64()) {
                    old_port = Some(p as u16);
                }

    let listener = match tokio::net::TcpListener::bind(format!("{host}:{port}")).await {
        Ok(l) => l,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            info!("⚠️ Port {} is already in use. Searching for a free port...", port);
            let mut bound_listener = None;
            for candidate_port in (port + 1)..=(port + 100) {
                let candidate_addr = format!("{host}:{candidate_port}");
                match tokio::net::TcpListener::bind(&candidate_addr).await {
                    Ok(l) => {
                        info!("🎯 Found free port to bind: {}", candidate_port);
                        bound_listener = Some(l);
                        break;
                    }
                    Err(_) => continue,
                }
            }
            if let Some(l) = bound_listener {
                l
            } else {
                info!("⚠️ No candidate ports in range free. Letting OS assign free port...");
                tokio::net::TcpListener::bind(format!("{host}:0")).await?
            }
        }
        Err(e) => return Err(e.into()),
    };

    let bound_addr = listener.local_addr()?;
    let bound_port = bound_addr.port();
    info!("\u{1F680} TylluanNexus HTTP Gateway listening on {}", bound_addr);
    
    // Write active port to data/active_port.json
    let _ = std::fs::create_dir_all("data");
    let payload = serde_json::json!({ "port": bound_port });
    let port_json = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string());
    if let Err(e) = std::fs::write("data/active_port.json", port_json) {
        error!("❌ Failed to write active_port.json: {}", e);
    } else {
        info!("📝 Written active port {} to data/active_port.json", bound_port);
    }

    // Now that the new kernel is fully up and running on the new port,
    // gracefully shutdown the old kernel so the proxy starts routing to the new one.
    // Safety: only ever hot-swap-shutdown a port this same kernel could
    // plausibly have bound to previously -- the configured port plus the
    // fallback range used above (port..=port+100) when the configured port
    // was already taken. A stale/corrupted active_port.json must never let
    // this kernel send a shutdown signal to an unrelated process on some
    // other port (e.g. another local service, or anything else on the host).
    //
    // This is deliberately derived from `port` (the configured value) rather
    // than a hardcoded range: Tylluan's real shipped default is :3030 (see
    // tylluan.example.toml) -- a fixed "Tylluan always uses 4000-4099" range was wrong and
    // would have silently broken zero-downtime restarts for any user running
    // the default config, since :3030 fell outside that hardcoded window.
    let own_port_range = port..=(port.saturating_add(100));
    if let Some(op) = old_port
        && op != bound_port
        && own_port_range.contains(&op) {
            tokio::spawn(async move {
                info!("🔌 Sending graceful shutdown signal to previous kernel on port {}...", op);
                let client = reqwest::Client::new();
                let shutdown_url = format!("http://127.0.0.1:{op}/api/v1/admin/shutdown");
                let _ = tokio::time::timeout(
                    std::time::Duration::from_secs(3),
                    client.post(&shutdown_url).header("host", "127.0.0.1").send()
                ).await;
            });
        }

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    ).await?;
    Ok(())
}


/// Compatibility entry point \u{2014} called by main.rs.
/// Constructs the full HttpState (broadcast channels, heartbeat, metrics broadcaster)
/// then delegates to the modular start_http_server.
#[allow(clippy::too_many_arguments)] // one-shot startup wiring called from a single site in main.rs
pub async fn start_http_server_with_download(
    host: &str,
    port: u16,
    auth_token: Option<String>,
    dev_mode: bool,
    server: Option<Arc<tokio::sync::RwLock<TylluanServer>>>,
    registry_handle: crate::registry::actor::RegistryHandle,
    download_tx: tokio::sync::broadcast::Sender<crate::maintenance::DownloadProgress>,
    tunnel_wsl_url: Option<String>,
    coloquio: Arc<crate::memory::coloquio::ColoquioDb>,
    jobs: Arc<crate::memory::jobs::JobQueue>,
    cancel_token: tokio_util::sync::CancellationToken,
    health_ready: Arc<AtomicBool>,
    node_identity: Arc<tylluan_link::identity::NodeIdentity>,
) -> anyhow::Result<()> {
    use tokio::sync::broadcast;

    let (broadcast_tx, _rx) = broadcast::channel(100);

    let (silva, doctor, memory, mailbox, matcher) = if let Some(s) = &server {
        let s_read = s.read().await;
        (s_read.silva(), s_read.doctor(), s_read.memory(), s_read.mailbox.clone(), s_read.matcher.clone())
    } else {
        return Err(anyhow::anyhow!(
            "Cannot initialize HTTP Gateway: Sovereign Server is not available"
        ));
    };

    // Normalize 127.0.0.1 \u{2192} localhost so OAuth issuer matches what clients type
    let canonical_host = if host == "127.0.0.1" || host == "0.0.0.0" { "localhost" } else { host };
    let base_url = format!("http://{canonical_host}:{port}");
    // ─── Metrics Ring Buffer ─────────────────────────────────────────────────
    let metrics_ring = Arc::new(RwLock::new(crate::metrics_ring::MetricsRingBuffer::new()));

    // Read gossip config from TOML (not hardcoded) for engine init
    let mesh_gossip_kernel = {
        let config_lock = crate::config::TylluanConfig::load_cached()
            .unwrap_or_else(|_| Arc::new(RwLock::new(crate::config::TylluanConfig::default())));
        config_lock.read().await.mesh.gossip.clone()
    };
    let gossip_cfg = tylluan_link::gossip::GossipConfig {
        enabled: mesh_gossip_kernel.enabled,
        interval_secs: mesh_gossip_kernel.interval_secs,
        fanout: mesh_gossip_kernel.fanout,
        max_peer_cursors: mesh_gossip_kernel.max_peer_cursors,
        max_entries: mesh_gossip_kernel.max_entries,
    };

let capability_registry: Arc<std::sync::Mutex<tylluan_link::capability::CapabilityRegistry>> =
        Arc::new(std::sync::Mutex::new(
            tylluan_link::capability::CapabilityRegistry::new(std::time::Duration::from_secs(300))
        ));
    let dispatch_router: Arc<std::sync::Mutex<tylluan_link::dispatch::DispatchRouter>> =
        Arc::new(std::sync::Mutex::new(
            tylluan_link::dispatch::DispatchRouter::new(
                capability_registry.clone(),
                std::time::Duration::from_secs(60),
            )
        ));

    let p2p_pool = Arc::new(tokio::sync::Mutex::new(P2pSessionPool::new(16, 300)));

    let repo_map = {
        let root = find_workspace_root();
        tokio::task::spawn_blocking(move || {
            crate::repo_map::RepoMap::build(&root)
        }).await.unwrap_or_else(|_| {
            crate::repo_map::RepoMap::build(&std::path::PathBuf::from("."))
        })
    };

    let a2a_task_manager = Arc::new(a2a::A2aTaskManager::new(silva.clone()));

    // M19-P5: Load declarative agent contract from .tylluan/agents.toml
    // Use find_workspace_root() (walks up from CWD searching for tylluan.toml)
    // instead of current_dir() — otherwise the contract loads silently empty
    // when the kernel process runs from a subdirectory (e.g. crates/tylluan-kernel
    // via tylluan-mcp.bat). Found during 2026-07-26 dogfooding.
    let workspace_root = find_workspace_root();
    let agents_contract = Arc::new(
        crate::security::agents_contract::AgentsContract::load(&workspace_root)
    );

    let state = Arc::new(HttpState {
        version: env!("CARGO_PKG_VERSION").to_string(),
        auth_token,
        dev_mode: Some(dev_mode),
        start_time: std::time::Instant::now(),
        server: server.clone(),
        registry: registry_handle.clone(),
        silva: silva.clone(),
        memory,
        doctor,
        mailbox,
        coloquio,
        matcher,
        broadcast_tx: broadcast_tx.clone(),
        download_progress_tx: download_tx,
        sessions: Arc::new(RwLock::new({
            let mut s = silva.load_sessions().await.unwrap_or_default();
            let now_unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
            // Keep sessions for 24 hours of inactivity (sovereign persistence)
            s.retain(|_, sess| now_unix - sess.last_active_unix < 86400);
            info!("\u{1F332} SilvaDB: Sessions restored: {}", s.len());
            s
        })),
        guild_status_cache: Arc::new(std::sync::Mutex::new(None)),
        agent_rate_limiter: Arc::new(DashMap::new()),
        // Higher ceiling than the per-agent limiter (60/min): a single IP can
        // legitimately host several agents plus the dashboard's own polling.
        // This is a defense-in-depth backstop against agent_id bypass, not
        // the primary limiter.
        ip_rate_limiter: Arc::new(crate::security::rate_limiter::RateLimiter::new(Some(300))),
        config: crate::config::TylluanConfig::load_cached().unwrap_or_else(|_| Arc::new(RwLock::new(crate::config::TylluanConfig::default()))),
        tunnel_wsl_url,
        oauth: std::sync::Arc::new(oauth::OAuthState::new(base_url)),
        metrics_ring: metrics_ring.clone(),
        jobs: jobs.clone(),
        cancel_token,
        node_router: crate::memory::agent_nodes::AgentNodeRouter::new(broadcast_tx.clone()),
        journal: Arc::new(
            crate::transport::http::api_v1::api_journal::JournalDb::open("./data/journal.db")
                .expect("journal.db init failed")
        ),
        agent_registry: crate::transport::http::api_v1::api_agents::AgentRegistry::new(7200),
        contract_registry: crate::transport::http::api_v1::api_contracts::ContractRegistry::new(),
        contract_db: Arc::new(
            crate::transport::http::api_v1::api_contracts::ContractDb::open("./data/contracts.db")
                .expect("contracts.db init failed")
        ),
        peer_db: Arc::new(
            crate::federation::PeerDb::open("./data/peers.db")
                .expect("peers.db init failed")
        ),
        health_ready,
        node_identity: node_identity.clone(),
        nat_cache: Arc::new(tokio::sync::RwLock::new(None)),
        dht_routing_table: Arc::new(tokio::sync::RwLock::new(
            tylluan_link::dht::RoutingTable::new(
                node_identity.node_id().to_string()
            )
        )),
        capability_registry,
        dispatch_router,
        dispatch_queue: Arc::new(std::sync::Mutex::new(DispatchQueue::new(1000))),
        p2p_pool: p2p_pool.clone(),
        repo_map,
        a2a_task_manager: a2a_task_manager.clone(),
        gossip_engine: Arc::new(tokio::sync::RwLock::new(
            tylluan_link::gossip::GossipEngine::new(
                node_identity.node_id().to_string(),
                gossip_cfg.clone(),
            )
        )),
        agents_contract,
    });

    info!("🗺️  Repo map built: {} files, {} dirs, {} lines ({}ms)",
        state.repo_map.total_files, state.repo_map.total_dirs, state.repo_map.total_lines, state.repo_map.build_duration_ms);

    // Bootstrap federation peers: seed DB from TOML if empty, then load DB into config.
    {
        let db_peers = state.peer_db.load_all().unwrap_or_default();
        if db_peers.is_empty() {
            // One-time migration: copy any TOML-seeded peers into DB
            let toml_peers = state.config.read().await.federation_peers.clone();
            for p in &toml_peers {
                let _ = state.peer_db.insert(p);
            }
        }
        // DB is now the source of truth — sync into in-memory config
        let authoritative = state.peer_db.load_all().unwrap_or_default();
        state.config.write().await.federation_peers = authoritative;
    }

    // Bootstrap work contracts from SQLite into the in-memory registry.
    if let Ok(active) = state.contract_db.load_active() {
        for c in active {
            state.contract_registry.contracts.insert(c.id.clone(), c);
        }
    }

    // Spawn background federation auto-sync loop task
    crate::transport::http::api_v1::api_federation::spawn_auto_sync(state.clone());

    // M14-A: DHT mesh bootstrap — async background task
    let dht_state = state.clone();
    tokio::spawn(async move {
        let enabled = dht_state.config.read().await.mesh.bootstrap_enabled;
        if !enabled {
            return;
        }
        let use_mainline = dht_state.config.read().await.mesh.mainline_dht_enabled;
        let use_mdns = {
            let cfg = dht_state.config.read().await;
            cfg.mdns.advertise || cfg.mdns.discover
        };
        let seed_nodes = dht_state.config.read().await.mesh.seed_nodes.clone();
        let listen_port = dht_state.config.read().await.nexus.port;

        let bootstrap_config = tylluan_link::dht::BootstrapConfig {
            local_node_id: dht_state.node_identity.node_id().to_string(),
            local_addr: "0.0.0.0:0".parse().unwrap(),
            use_mdns,
            use_mainline,
            seed_nodes,
            dht_peers_path: std::path::PathBuf::from("data/dht_peers.json"),
            listen_port,
        };

        let mut rt = dht_state.dht_routing_table.write().await;
        match bootstrap_config.bootstrap(&mut rt).await {
            Ok(peers) => tracing::info!("🌐 DHT mesh: discovered {} peer(s)", peers.len()),
            Err(e) => tracing::warn!("🌐 DHT mesh bootstrap: {}", e),
        }
    });

    // M14-B: Gossip Protocol background loop
    let gossip_state = state.clone();
    tokio::spawn(async move {
        let enabled = gossip_state.config.read().await.mesh.gossip.enabled;
        if !enabled { return; }
        let interval = {
            gossip_state.config.read().await.mesh.gossip.interval_secs
        };
        if interval == 0 { return; }
        let gossip_timeout = {
            gossip_state.config.read().await.mesh.gossip.timeout_secs
        };
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(interval));
        loop {
            ticker.tick().await;
            let peers = {
                let rt = gossip_state.dht_routing_table.read().await;
                let engine = gossip_state.gossip_engine.read().await;
                engine.select_gossip_targets(&rt)
            };
            if peers.is_empty() {
                continue;
            }
            let local_id = gossip_state.node_identity.node_id().to_string();
            let local_port = {
                gossip_state.config.read().await.nexus.port
            };
            let local_addr = format!("127.0.0.1:{local_port}");
            let clock = {
                gossip_state.gossip_engine.write().await.advance_clock()
            };
            let local_entry = tylluan_link::gossip::GossipEntry {
                node_id: local_id.clone(),
                addr: local_addr,
                capabilities: vec!["mesh".into()],
                clock,
                hardware: tylluan_link::gossip::HardwareCaps::default(),
                ed25519_pubkey: Some(gossip_state.node_identity.public_key_hex().to_string()),
            };
            // Store our own entry so it's available for Pull responses
            gossip_state.gossip_engine.write().await.store_entries(std::slice::from_ref(&local_entry));

            let secret = gossip_state.config.read().await.mesh.gossip.shared_secret.clone();

            for peer_entry in &peers {
                let peer_pubkey = {
                    let rt = gossip_state.dht_routing_table.read().await;
                    rt.all_peers().iter()
                        .find(|e| e.node_id == peer_entry.node_id)
                        .and_then(|e| e.ed25519_pubkey.as_deref())
                        .map(|s| s.to_string())
                };

                // Helper: build encrypted wire bytes for outbound gossip message
                let build_wire = |body: &[u8], local_id: &str| -> Option<(Vec<u8>, &'static str)> {
                    // Try Noise NK when peer pubkey known
                    if let Some(ref pk) = peer_pubkey
                        && !pk.is_empty()
                        && let Ok(enc) = tylluan_link::noise::noise_encrypt_payload(body, &gossip_state.node_identity, pk) {
                            let mut wire = Vec::with_capacity(1 + NODE_ID_BYTES + enc.len());
                            wire.push(GOSSIP_DISCR_NOISE);
                            wire.extend_from_slice(local_id.as_bytes());
                            wire.extend_from_slice(&enc);
                            return Some((wire, "application/octet-stream"));
                        }
                    // Fallback: ChaCha20 via shared_secret
                    if !secret.is_empty()
                        && let Ok(enc) = crate::federation::encrypt_payload(body, &secret) {
                            let mut wire = Vec::with_capacity(1 + NODE_ID_BYTES + enc.len());
                            wire.push(GOSSIP_DISCR_CHACHA);
                            wire.extend_from_slice(local_id.as_bytes());
                            wire.extend_from_slice(&enc);
                            return Some((wire, "application/octet-stream"));
                        }
                    // Last resort: plaintext JSON
                    Some((body.to_vec(), "application/json"))
                };

                // Process gossip entries from a response and update routing table
                let process_entries = |val: &serde_json::Value| -> std::vec::Vec<tylluan_link::gossip::GossipEntry> {
                    if let Some(entries) = val.get("entries").and_then(|v| v.as_array()) {
                        entries.iter()
                            .filter_map(|e| serde_json::from_value(e.clone()).ok())
                            .collect()
                    } else {
                        Vec::new()
                    }
                };

                // Phase 1: Push our entry
                let push_msg = tylluan_link::gossip::GossipMessage::push(
                    local_id.clone(),
                    clock,
                    vec![local_entry.clone()],
                );
                let push_body = match serde_json::to_vec(&push_msg) {
                    Ok(b) => b,
                    Err(_) => continue,
                };
                let (wire_body, content_type) = match build_wire(&push_body, &local_id) {
                    Some(v) => v,
                    None => continue,
                };
                let url = format!("http://{}/api/v1/gossip", peer_entry.addr);
                let client = reqwest::Client::new();
                match client.post(&url)
                    .header("Content-Type", content_type)
                    .body(wire_body)
                    .timeout(std::time::Duration::from_secs(gossip_timeout))
                    .send()
                    .await
                {
                    Ok(resp) => {
                        gossip_state.gossip_engine.write().await.record_peer_clock(&peer_entry.node_id, clock);
                        let resp_bytes = match resp.bytes().await {
                            Ok(b) => b,
                            Err(_) => continue,
                        };
                        let response_entries = if let Some(plain) = gossip_decrypt_plaintext(
                            &resp_bytes, &secret, &gossip_state.node_identity,
                            &gossip_state.dht_routing_table,
                        ).await {
                            if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&plain) {
                                process_entries(&val)
                            } else { Vec::new() }
                        } else { Vec::new() };
                        gossip_state.gossip_engine.write().await.store_entries(&response_entries);
                        for e in &response_entries {
                            if let Ok(addr) = e.addr.parse::<std::net::SocketAddr>() {
                                gossip_state.dht_routing_table.write().await.insert(
                                    &e.node_id, addr, e.capabilities.clone(),
                                    e.ed25519_pubkey.clone(),
                                );
                            }
                        }
                    }
                    Err(e) => {
                        tracing::trace!("gossip push → {}: {}", peer_entry.addr, e);
                    }
                }

                // Phase 2: Pull entries we might have missed
                let cursor = gossip_state.gossip_engine.read().await.state.last_known(&peer_entry.node_id);
                let pull_msg = tylluan_link::gossip::GossipMessage::pull(local_id.clone(), cursor);
                let pull_body = match serde_json::to_vec(&pull_msg) {
                    Ok(b) => b,
                    Err(_) => continue,
                };
                let (wire_body, content_type) = match build_wire(&pull_body, &local_id) {
                    Some(v) => v,
                    None => continue,
                };
                match client.post(&url)
                    .header("Content-Type", content_type)
                    .body(wire_body)
                    .timeout(std::time::Duration::from_secs(gossip_timeout))
                    .send()
                    .await
                {
                    Ok(resp) => {
                        let resp_bytes = match resp.bytes().await {
                            Ok(b) => b,
                            Err(_) => continue,
                        };
                        let response_entries = if let Some(plain) = gossip_decrypt_plaintext(
                            &resp_bytes, &secret, &gossip_state.node_identity,
                            &gossip_state.dht_routing_table,
                        ).await {
                            if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&plain) {
                                process_entries(&val)
                            } else { Vec::new() }
                        } else { Vec::new() };
                        gossip_state.gossip_engine.write().await.store_entries(&response_entries);
                        for e in &response_entries {
                            if let Ok(addr) = e.addr.parse::<std::net::SocketAddr>() {
                                gossip_state.dht_routing_table.write().await.insert(
                                    &e.node_id, addr, e.capabilities.clone(),
                                    e.ed25519_pubkey.clone(),
                                );
                            }
                        }
                    }
                    Err(e) => {
                        tracing::trace!("gossip pull → {}: {}", peer_entry.addr, e);
                    }
                }
            }

            // Phase 3: CapabilityRegistry — ingest fresh state + prune expired
            // Trust boundary: only capabilities from approved federation peers with a
            // known Ed25519 pubkey ever reach the registry DispatchRouter reads from.
            // An unapproved peer's entries still propagate through gossip to other
            // nodes (fortress, not cage), they just never become routable here.
            let trusted_pubkeys: std::collections::HashSet<String> = gossip_state
                .peer_db
                .load_all()
                .unwrap_or_default()
                .into_iter()
                .filter(|p| p.approved && !p.ed25519_pubkey.is_empty())
                .map(|p| p.ed25519_pubkey)
                .collect();

            // Read engine first (async), then lock registry (sync) — avoids holding
            // MutexGuard across an await boundary.
            let engine_snapshot = gossip_state.gossip_engine.read().await;
            let mut reg = gossip_state.capability_registry.lock().unwrap();
            reg.ingest_from_engine_trusted(&engine_snapshot, &trusted_pubkeys);
            let pruned = reg.prune_expired();
            drop(reg);
            drop(engine_snapshot);
            if pruned > 0 {
                tracing::debug!("CapabilityRegistry: pruned {} expired peers", pruned);
            }
        }
    });

    // Spawn background sampler — fills the ring every 5 seconds.
    crate::metrics_ring::spawn_metrics_sampler(metrics_ring, registry_handle);

    // Wire SSE notifier into the TylluanServer and GrantRegistry
    if let Some(s) = &server {
        s.write().await.set_notifier(broadcast_tx.clone());
    }
    crate::security::grants::set_notifier(broadcast_tx.clone());

    // ─── Global heartbeat + Metrics Broadcaster ──────────────────────────────
    let (decay_enabled, decay_interval_secs, decay_half_life_hours) = {
        let cfg = state.config.read().await;
        (cfg.silva.decay_enabled, cfg.silva.decay_interval_hours * 3600, cfg.silva.decay_half_life_hours)
    };

    sse::spawn_heartbeat(
        broadcast_tx.clone(),
        state.start_time,
        state.sessions.clone(),
        state.mailbox.clone(),
        state.silva.clone(),
        decay_enabled,
        decay_interval_secs,
        decay_half_life_hours,
        Arc::new(state.registry.clone()),
        state.matcher.clone(),
    );

    sse::spawn_metrics_broadcaster(broadcast_tx.clone(), state.doctor.clone());

    // Auto-linking: background task that runs once at startup to connect existing nodes
    let silva_clone = state.silva.clone();
    let registry_clone = state.registry.clone();
    let broadcast_clone = broadcast_tx.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(20)).await;
        let arc = registry_clone.arc();
        let reg = arc.read().await;
        let knowledge_running = reg.guilds.get("knowledge").map(|g| g.is_running()).unwrap_or(false);
        drop(reg);
        if !knowledge_running {
            tracing::warn!("⚠️ Knowledge guild not running — skipping auto-linking");
            return;
        }
        tracing::info!("🕸️ Starting auto-linking from existing nodes...");
        let result = silva_clone.retrograde_extract_triples(50, |snippet: String| {
            let reg = registry_clone.clone();
            async move {
                let params = CallToolRequestParam {
                    name: "extract_triples".into(),
                    arguments: Some(serde_json::json!({"text": snippet, "max_triples": 5}).as_object().cloned().unwrap_or_default()),
                };
                let res = reg.call_tool("knowledge", params).await?;
                // If the call returned a guild error (disconnected), propagate as Err
                // so retrograde_extract_triples stops the loop early
                let is_err = res.is_error == Some(true);
                let text = res.content.into_iter()
                    .filter_map(|c: Content| c.as_text().map(|t| t.text.clone()))
                    .next();
                match text {
                    Some(t) if !is_err => Ok(t),
                    Some(t) if t.contains("disconnected") || t.contains("GUILD_ERROR") =>
                        Err(anyhow::anyhow!("knowledge guild error: {t}")),
                    Some(t) => Ok(t),
                    None => Err(anyhow::anyhow!("no content")),
                }
            }
        }).await;
        match result {
            Ok(count) => {
                tracing::info!("✅ Auto-linking complete: {} edges added from existing nodes", count);
                let _ = broadcast_clone.send(serde_json::json!({
                    "type": "graph_autolinked",
                    "data": { "edges_added": count, "ts": chrono::Utc::now().timestamp() }
                }));
            }
            Err(e) => {
                tracing::error!("❌ Auto-linking failed: {}", e);
            }
        }
    });

    // --- M18-1 AutoResearch Daemon Spawning ---
    let ar_silva = state.silva.clone();
    let data_dir = std::path::PathBuf::from("data");
    let ar_engine = state.matcher.engine_arc();
    let ar_reranker = if let Some(ref s) = server {
        s.read().await.reranker.clone()
    } else {
        None
    };
    tokio::spawn(async move {
        let idle_lab = std::sync::Arc::new(crate::memory::idle_lab::IdleLab::new(ar_silva, &data_dir));
        crate::memory::autoresearch::autoresearch_daemon(idle_lab, ar_engine, ar_reranker).await;
    });

    // M14-F Phase 3: P2P dispatch listener
    let p2p_state = state.clone();
    tokio::spawn(async move {
        let p2p_cfg = p2p_state.config.read().await.p2p.clone();
        if !p2p_cfg.enabled {
            tracing::info!("P2P dispatch listener: disabled by config");
            return;
        }
        use std::net::SocketAddr;
        let addr: SocketAddr = match format!("0.0.0.0:{}", p2p_cfg.listen_port).parse() {
            Ok(a) => a,
            Err(e) => {
                tracing::error!("P2P dispatch listener: invalid addr: {}", e);
                return;
            }
        };
        let identity = p2p_state.node_identity.clone();
        let registry = p2p_state.registry.clone();
        let handler: P2pHandlerFn = Arc::new(move |req: tylluan_link::dispatch::GuildDispatchRequest| {
            let reg = registry.clone();
            Box::pin(async move {
                let tool_req = CallToolRequestParam {
                    name: req.tool.clone().into(),
                    arguments: Some(req.args.as_object().cloned().unwrap_or_default()),
                };
                let start = std::time::Instant::now();
                match reg.call_tool(&req.guild, tool_req).await {
                    Ok(res) => {
                        let dur = start.elapsed().as_millis() as u64;
                        tylluan_link::dispatch::GuildDispatchResponse {
                            request_id: req.request_id,
                            success: !res.is_error.unwrap_or(false),
                            result: serde_json::json!(res.content),
                            error: None,
                            executor_id: "local".to_string(),
                            duration_ms: dur,
                        }
                    }
                    Err(e) => {
                        let dur = start.elapsed().as_millis() as u64;
                        tylluan_link::dispatch::GuildDispatchResponse {
                            request_id: req.request_id,
                            success: false,
                            result: serde_json::Value::Null,
                            error: Some(e.to_string()),
                            executor_id: "local".to_string(),
                            duration_ms: dur,
                        }
                    }
                }
            })
        });
        match start_p2p_listener_noise(addr, identity, handler).await {
            Ok((handle, bound_addr)) => {
                tracing::info!("P2P dispatch listener started on {}", bound_addr);
                if let Err(e) = handle.await {
                    tracing::error!("P2P dispatch listener exited: {:?}", e);
                }
            }
            Err(e) => {
                tracing::error!("Failed to start P2P dispatch listener: {}", e);
            }
        }
    });
    // ------------------------------------------

    start_http_server(host, port, state)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
}

fn build_router(state: Arc<HttpState>) -> Router {
    // CORS: Only known localhost origins (dashboard dev:5173, prod:3030, kernel:3030)
    let cors = CorsLayer::new()
        .allow_origin([
            "http://127.0.0.1:3030".parse::<HeaderValue>().unwrap(),
            "http://localhost:3030".parse::<HeaderValue>().unwrap(),
            "http://127.0.0.1:5173".parse::<HeaderValue>().unwrap(),
            "http://localhost:5173".parse::<HeaderValue>().unwrap(),
            "http://127.0.0.1:3000".parse::<HeaderValue>().unwrap(),
            "http://localhost:3000".parse::<HeaderValue>().unwrap(),
        ])
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
            Method::PATCH,
        ])
        .allow_headers([CONTENT_TYPE, AUTHORIZATION]);

    // 1. Public Routes
    let oauth_state = state.oauth.clone();
    let oauth_routes = Router::new()
        .route("/.well-known/oauth-authorization-server", get(oauth::metadata_handler))
        .route("/oauth/authorize", get(oauth::authorize_handler))
        .route("/oauth/token", post(oauth::token_handler))
        .route("/oauth/revoke", post(oauth::revoke_handler))
        .with_state(oauth_state);

    let public_routes = Router::new()
        .route("/health", get(health_handler))
        .route("/discovery", get(discovery_handler))
        .route("/api/v1/mcp/probe", get(api_v1::probe_handler))
        .route("/.well-known/agent-card.json", get(a2a::agent_card_handler))
        .merge(oauth_routes);

    #[cfg(feature = "observability")]
    let public_routes = public_routes.route("/metrics", get(crate::metrics_exporter::metrics_handler));

    // 2. Protected Routes (API v1 + MCP + SSE)
    let protected_routes = api_v1::api_v1_routes()
        .merge(sse::sse_routes())
        .route("/messages", any(api_v1::mcp_handler))
        .route("/mcp", any(api_v1::mcp_handler))
        .route("/api/v1/mcp", any(api_v1::mcp_handler))
        .route("/a2a", post(a2a::a2a_jsonrpc_handler))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::bearer_auth_middleware,
        ));

    // 3. Assemble and Static Assets
    // IMPORTANT: In Axum, .fallback_service() overwrites .fallback(). Using both means the
    // last one wins. We use only .fallback() with smart routing:
    //   - /api/* routes that weren't matched → 404 (never index.html, would hide auth errors)
    //   - Real static assets (JS/CSS) → read from disk via ServeDir tower service
    //   - SPA client-side routes → index.html
    let static_dir = find_workspace_root().join("dashboard/dist");

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        // index.html never cached — assets use content-hash and can cache forever
        .route("/", axum::routing::get(serve_index))
        .route("/index.html", axum::routing::get(serve_index))
        .fallback_service(
            tower::service_fn(move |req: axum::extract::Request| {
                let path = req.uri().path().to_owned();
                let static_dir_inner = static_dir.clone();
                async move {
                    // API routes not matched by registered handlers → 404
                    // (never fall through to index.html, that would hide auth errors)
                    if path.starts_with("/api/") {
                        let resp = axum::http::Response::builder()
                            .status(404)
                            .header("Content-Type", "application/json")
                            .body(axum::body::Body::from("{\"error\":\"not_found\"}"))
                            .unwrap();
                        return Ok::<_, std::convert::Infallible>(resp);
                    }
                    // Try embedded assets first (bundled-dashboard feature)
                    #[cfg(feature = "bundled-dashboard")]
                    {
                        let asset_path = path.trim_start_matches('/');
                        if let Some(asset) = DashboardAssets::get(asset_path) {
                            let mime = mime_guess::from_path(asset_path)
                                .first_or_octet_stream()
                                .to_string();
                            let resp = axum::http::Response::builder()
                                .status(200)
                                .header("Content-Type", mime)
                                .header("Cache-Control", "public, max-age=31536000, immutable")
                                .body(axum::body::Body::from(asset.data.to_vec()))
                                .unwrap();
                            return Ok(resp);
                        }
                    }
                    // Static assets (JS/CSS/fonts) — serve from disk
                    let file_path = static_dir_inner.join(path.trim_start_matches('/'));
                    if file_path.is_file()
                        && let Ok(bytes) = tokio::fs::read(&file_path).await {
                            let mime = match file_path.extension().and_then(|e| e.to_str()) {
                                Some("js")   => "application/javascript; charset=utf-8",
                                Some("css")  => "text/css; charset=utf-8",
                                Some("html") => "text/html; charset=utf-8",
                                Some("svg")  => "image/svg+xml",
                                Some("png")  => "image/png",
                                Some("ico")  => "image/x-icon",
                                Some("woff2")=> "font/woff2",
                                Some("woff") => "font/woff",
                                _            => "application/octet-stream",
                            };
                            let resp = axum::http::Response::builder()
                                .status(200)
                                .header("Content-Type", mime)
                                .header("Cache-Control", "public, max-age=31536000, immutable")
                                .body(axum::body::Body::from(bytes))
                                .unwrap();
                            return Ok(resp);
                        }
                    // SPA client-side routes → index.html
                    #[cfg(feature = "bundled-dashboard")]
                    {
                        if let Some(asset) = DashboardAssets::get("index.html") {
                            let resp = axum::http::Response::builder()
                                .status(200)
                                .header("Content-Type", "text/html; charset=utf-8")
                                .header("Cache-Control", "no-cache, no-store, must-revalidate")
                                .body(axum::body::Body::from(asset.data.to_vec()))
                                .unwrap();
                            return Ok(resp);
                        }
                    }
                    let index_path = static_dir_inner.join("index.html");
                    match tokio::fs::read(&index_path).await {
                        Ok(bytes) => {
                            let resp = axum::http::Response::builder()
                                .status(200)
                                .header("Content-Type", "text/html; charset=utf-8")
                                .header("Cache-Control", "no-cache, no-store, must-revalidate")
                                .body(axum::body::Body::from(bytes))
                                .unwrap();
                            Ok(resp)
                        }
                        Err(_) => {
                            let resp = axum::http::Response::builder()
                                .status(404)
                                .body(axum::body::Body::empty())
                                .unwrap();
                            Ok(resp)
                        }
                    }
                }
            })
        )
        .layer(tower_http::compression::CompressionLayer::new())
        .layer(cors)
        .layer(middleware::from_fn(force_utf8_middleware))
        .layer(axum::extract::DefaultBodyLimit::max(50 * 1024 * 1024))
        .with_state(state)
}


async fn serve_index() -> impl IntoResponse {
    #[cfg(feature = "bundled-dashboard")]
    {
        if let Some(asset) = DashboardAssets::get("index.html") {
            return axum::response::Response::builder()
                .status(200)
                .header("Content-Type", "text/html; charset=utf-8")
                .header("Cache-Control", "no-cache, no-store, must-revalidate")
                .body(axum::body::Body::from(asset.data.to_vec()))
                .expect("valid index response builder");
        }
    }
    let index_path = find_workspace_root().join("dashboard/dist/index.html");
    match tokio::fs::read(&index_path).await {
        Ok(bytes) => axum::response::Response::builder()
            .status(200)
            .header("Content-Type", "text/html; charset=utf-8")
            .header("Cache-Control", "no-cache, no-store, must-revalidate")
            .body(axum::body::Body::from(bytes))
            .expect("valid index response builder"),
        Err(_) => axum::response::Response::builder()
            .status(404)
            .body(axum::body::Body::from("Dashboard not built. Run: cd dashboard && pnpm build"))
            .expect("valid 404 response builder"),
    }
}

#[derive(Deserialize)]
pub struct HealthQuery {
    pub verbose: Option<bool>,
}

async fn health_handler(
    State(state): State<Arc<HttpState>>,
    Query(query): Query<HealthQuery>,
) -> impl IntoResponse {
    let ready = state.health_ready.load(std::sync::atomic::Ordering::Acquire);

    if query.verbose.unwrap_or(false) {
        let node_count = state.silva.node_count().await.unwrap_or(0);
        let edge_count = state.silva.edge_count().await.unwrap_or(0);
        let (total_guilds, active_guilds) = state.registry.guild_stats().await.unwrap_or((0, 0));

        let embeddings_loaded = state.server.as_ref()
            .and_then(|s| s.try_read().ok())
            .map(|s| s.matcher.engine().is_some())
            .unwrap_or(false);
        let reranker_loaded = state.server.as_ref()
            .and_then(|s| s.try_read().ok())
            .map(|s| s.reranker.is_some())
            .unwrap_or(false);

        // Mesh status: active P2P sessions + DHT peers
        let p2p_sessions = state.p2p_pool.try_lock().map(|p| p.len()).unwrap_or(0);
        let dht_peers = state.dht_routing_table.try_read().map(|t| t.peer_count()).unwrap_or(0);

        let overall = if !ready { "warming_up" }
            else if embeddings_loaded && active_guilds > 0 { "healthy" }
            else if embeddings_loaded || active_guilds > 0 { "degraded" }
            else { "critical" };

        return (StatusCode::OK, Json(serde_json::json!({
            "status": overall,
            "version": env!("CARGO_PKG_VERSION"),
            "commit": env!("TYLLUAN_GIT_COMMIT"),
            "boot_ready": ready,
            "components": {
                "kernel": { "ok": ready },
                "embeddings": { "ok": embeddings_loaded, "model": "bge-m3" },
                "reranker":   { "ok": reranker_loaded, "model": "jina-reranker-v1-turbo-en" },
                "silva":      { "ok": node_count > 0, "nodes": node_count, "edges": edge_count },
                "guilds":     { "ok": active_guilds > 0, "active": active_guilds, "total": total_guilds },
                "mesh":       { "ok": p2p_sessions > 0 || dht_peers > 0,
                                "p2p_sessions": p2p_sessions, "dht_peers": dht_peers }
            }
        })));
    }

    let status = if ready { "ok" } else { "warming_up" };
    (StatusCode::OK, Json(serde_json::json!({
        "status": status,
        "version": env!("CARGO_PKG_VERSION"),
        "commit": env!("TYLLUAN_GIT_COMMIT"),
    })))
}

/// Returns the 5 sovereign MCP tools for agent discovery.
async fn discovery_handler() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({
        "server": "tylluan-nexus-sovereign",
        "version": env!("CARGO_PKG_VERSION"),
        "protocol": "MCP",
        "tools": [
            { "name": "tylluan_do",       "description": "Execute tasks via natural language routing to Python guilds" },
            { "name": "tylluan_remember", "description": "Store knowledge in SilvaDB long-term memory" },
            { "name": "tylluan_recall",   "description": "Retrieve knowledge from SilvaDB via hybrid BM25+vector search" },
            { "name": "tylluan_think",    "description": "Structured multi-step reasoning with hypothesis tracking" },
            { "name": "tylluan_graph",    "description": "Query and traverse the knowledge graph (PPR, BFS, edges)" }
        ],
        "endpoints": {
            "sse":      "/sse",
            "messages": "/messages",
            "health":   "/health"
        }
    })))
}

pub(crate) fn find_workspace_root() -> std::path::PathBuf {
    let mut root = std::env::current_dir().unwrap_or_default();
    for _ in 0..5 {
        if root.join("tylluan.toml").exists() { return root; }
        if let Some(parent) = root.parent() { root = parent.to_path_buf(); } else { break; }
    }
    std::env::current_dir().unwrap_or_default()
}

async fn force_utf8_middleware(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> impl IntoResponse {
    let mut response = next.run(req).await;
    let headers = response.headers_mut();
    if let Some(ct) = headers.get(header::CONTENT_TYPE)
        && let Ok(ct_str) = ct.to_str()
            && ct_str.contains("application/json") && !ct_str.contains("charset") {
                let new_ct = format!("{ct_str}; charset=utf-8");
                if let Ok(hv) = header::HeaderValue::from_str(&new_ct) {
                    headers.insert(header::CONTENT_TYPE, hv);
                }
            }
    response
}

/// JSON response with UTF-8 charset forced (fixes Windows client encoding issues)
#[derive(Debug)]
pub struct Utf8Json<T: Serialize>(pub T);

impl<T: Serialize> IntoResponse for Utf8Json<T> {
    fn into_response(self) -> axum::response::Response {
        let json = serde_json::to_string(&self.0).unwrap_or_default();
        let mut response = json.into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json; charset=utf-8"),
        );
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_workspace_root_from_nested_cwd() {
        let temp = tempfile::tempdir().unwrap();
        let root_path = temp.path();

        // 1. Create root markers: tylluan.toml and .tylluan/agents.toml
        std::fs::write(root_path.join("tylluan.toml"), "[silva]\n").unwrap();
        let tylluan_dir = root_path.join(".tylluan");
        std::fs::create_dir_all(&tylluan_dir).unwrap();
        std::fs::write(
            tylluan_dir.join("agents.toml"),
            "[agents.claude-code]\nrole = \"tech-lead\"\n",
        ).unwrap();

        // 2. Create nested CWD simulating tylluan-mcp.bat spawn (crates/tylluan-kernel)
        let nested = root_path.join("crates").join("tylluan-kernel");
        std::fs::create_dir_all(&nested).unwrap();

        // 3. Save original CWD and switch to nested CWD
        let orig_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&nested).unwrap();

        // 4. Test find_workspace_root() traverses up to root_path
        let found_root = find_workspace_root();
        
        // 5. Test AgentsContract::load() successfully loads .tylluan/agents.toml from found_root
        let contract = crate::security::agents_contract::AgentsContract::load(&found_root);

        // Restore original CWD before assertions
        let _ = std::env::set_current_dir(&orig_cwd);

        assert_eq!(found_root.canonicalize().unwrap(), root_path.canonicalize().unwrap());
        assert_eq!(contract.agents.len(), 1);
        assert!(contract.agents.contains_key("claude-code"));
    }

    fn test_identity() -> tylluan_link::identity::NodeIdentity {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("identity.key");
        // Leak the tempdir so the file stays alive for the identity's lifetime in the test.
        std::mem::forget(temp);
        tylluan_link::identity::NodeIdentity::load_or_create(&path).unwrap()
    }

    #[tokio::test]
    async fn test_gossip_decrypt_plaintext_passthrough_no_prefix() {
        // Regression guard: a legacy/no-discriminator payload (plain JSON) must be
        // returned unchanged -- this is the backward-compat path for peers that
        // don't have this fix yet.
        let identity = test_identity();
        let rt = tokio::sync::RwLock::new(tylluan_link::dht::RoutingTable::new("local".into()));
        let plain = br#"{"type":"Push","sender_id":"x"}"#.to_vec();

        let result = gossip_decrypt_plaintext(&plain, "", &identity, &rt).await;
        assert_eq!(result, Some(plain));
    }

    #[tokio::test]
    async fn test_gossip_decrypt_plaintext_chacha_fallback_roundtrip() {
        let identity = test_identity();
        let rt = tokio::sync::RwLock::new(tylluan_link::dht::RoutingTable::new("local".into()));
        let secret = "test-shared-secret";
        let plain = b"hello gossip";

        let enc = crate::federation::encrypt_payload(plain, secret).unwrap();
        let mut wire = vec![GOSSIP_DISCR_CHACHA];
        wire.extend_from_slice(&[b'0'; NODE_ID_BYTES]);
        wire.extend_from_slice(&enc);

        let result = gossip_decrypt_plaintext(&wire, secret, &identity, &rt).await;
        assert_eq!(result, Some(plain.to_vec()));
    }

    #[tokio::test]
    async fn test_gossip_decrypt_plaintext_chacha_wrong_secret_fails() {
        let identity = test_identity();
        let rt = tokio::sync::RwLock::new(tylluan_link::dht::RoutingTable::new("local".into()));
        let plain = b"hello gossip";
        let enc = crate::federation::encrypt_payload(plain, "real-secret").unwrap();
        let mut wire = vec![GOSSIP_DISCR_CHACHA];
        wire.extend_from_slice(&[b'0'; NODE_ID_BYTES]);
        wire.extend_from_slice(&enc);

        let result = gossip_decrypt_plaintext(&wire, "wrong-secret", &identity, &rt).await;
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_gossip_decrypt_plaintext_noise_roundtrip_with_known_pubkey() {
        // Two distinct identities: A encrypts to B, B decrypts using its own
        // identity + A's pubkey looked up from the routing table.
        let identity_a = test_identity();
        let identity_b = test_identity();
        let sender_id = "sender-node-aaaaaaaaaaaaaaaaaaaa"; // 32 ascii chars

        let rt = tokio::sync::RwLock::new(tylluan_link::dht::RoutingTable::new("local-b".into()));
        rt.write().await.insert(
            sender_id,
            "127.0.0.1:1".parse().unwrap(),
            vec!["mesh".into()],
            Some(identity_a.public_key_hex().to_string()),
        );

        let plain = b"secret gossip payload";
        let enc = tylluan_link::noise::noise_encrypt_payload(plain, &identity_a, identity_b.public_key_hex()).unwrap();
        let mut wire = vec![GOSSIP_DISCR_NOISE];
        wire.extend_from_slice(sender_id.as_bytes());
        wire.extend_from_slice(&enc);

        let result = gossip_decrypt_plaintext(&wire, "", &identity_b, &rt).await;
        assert_eq!(result, Some(plain.to_vec()));
    }

    #[tokio::test]
    async fn test_gossip_decrypt_plaintext_noise_unknown_pubkey_fails() {
        // Sender not in the routing table -- Noise decrypt must not be attempted
        // (no pubkey to associate), so this returns None rather than panicking
        // or falling through silently.
        let identity_b = test_identity();
        let rt = tokio::sync::RwLock::new(tylluan_link::dht::RoutingTable::new("local-b".into()));
        let mut wire = vec![GOSSIP_DISCR_NOISE];
        wire.extend_from_slice(&[b'z'; NODE_ID_BYTES]);
        wire.extend_from_slice(&[0u8; 60]); // dummy ciphertext, never reached

        let result = gossip_decrypt_plaintext(&wire, "", &identity_b, &rt).await;
        assert_eq!(result, None);
    }

    #[test]
    fn test_gossip_entry_without_pubkey_field_deserializes() {
        // Backward compat: a GossipEntry JSON from an old peer that predates the
        // ed25519_pubkey field must still deserialize (serde default = None).
        let json = r#"{"node_id":"x","addr":"127.0.0.1:1","capabilities":[],"clock":1}"#;
        let entry: tylluan_link::gossip::GossipEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.ed25519_pubkey, None);
    }
}
