//! # TylluanNexus HTTP Gateway
//!
//! Provides SSE, JSON-RPC (MCP), and Management API (V1).
//! Orchestrates routing using modular sub-routers.

pub mod auth;
pub mod oauth;
pub mod sse;
pub mod api_v1;

use axum::{
    Router, Json,
    extract::State,
    http::{StatusCode, header, HeaderValue, Method},
    middleware,
    response::IntoResponse,
    routing::{get, post, any},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::sync::RwLock;
use tracing::{info, error};
use std::time::Instant;
use dashmap::DashMap;
use tower_http::cors::{CorsLayer, Any};

use crate::registry::actor::RegistryHandle;
use crate::doctor::Doctor;
use crate::memory::hybrid::HybridMemory;
use crate::memory::silva::SilvaDB;
use crate::transport::server::TylluanServer;
use rmcp::model::{CallToolRequestParam, Content};

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
    pub guild_status_cache: Arc<std::sync::Mutex<Option<(Instant, Vec<crate::registry::guild_process::GuildStatus>)>>>,
    pub agent_rate_limiter: Arc<dashmap::DashMap<String, (u32, Instant)>>,
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

    let listener = match tokio::net::TcpListener::bind(format!("{}:{}", host, port)).await {
        Ok(l) => l,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            info!("⚠️ Port {} is already in use. Searching for a free port...", port);
            let mut bound_listener = None;
            for candidate_port in (port + 1)..=(port + 100) {
                let candidate_addr = format!("{}:{}", host, candidate_port);
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
                tokio::net::TcpListener::bind(format!("{}:0", host)).await?
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
    if let Some(op) = old_port
        && op != bound_port {
            tokio::spawn(async move {
                info!("🔌 Sending graceful shutdown signal to previous kernel on port {}...", op);
                let client = reqwest::Client::new();
                let shutdown_url = format!("http://127.0.0.1:{}/api/v1/admin/shutdown", op);
                let _ = tokio::time::timeout(
                    std::time::Duration::from_secs(3),
                    client.post(&shutdown_url).header("host", "127.0.0.1").send()
                ).await;
            });
        }

    axum::serve(listener, app).await?;
    Ok(())
}


/// Compatibility entry point \u{2014} called by main.rs.
/// Constructs the full HttpState (broadcast channels, heartbeat, metrics broadcaster)
/// then delegates to the modular start_http_server.
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
    let base_url = format!("http://{}:{}", canonical_host, port);
    // ─── Metrics Ring Buffer ─────────────────────────────────────────────────
    let metrics_ring = Arc::new(RwLock::new(crate::metrics_ring::MetricsRingBuffer::new()));

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
        node_identity,
        nat_cache: Arc::new(tokio::sync::RwLock::new(None)),
    });

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

    // Spawn background sampler — fills the ring every 5 seconds.
    crate::metrics_ring::spawn_metrics_sampler(metrics_ring, registry_handle);

    // Wire SSE notifier into the TylluanServer
    if let Some(s) = &server {
        s.write().await.set_notifier(broadcast_tx.clone());
    }

    // ─── Global heartbeat + Metrics Broadcaster ──────────────────────────────
    sse::spawn_heartbeat(broadcast_tx.clone(), state.start_time, state.sessions.clone(), state.mailbox.clone(), state.silva.clone(), true, 21600, Arc::new(state.registry.clone()), state.matcher.clone());

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
                        Err(anyhow::anyhow!("knowledge guild error: {}", t)),
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
    let ar_engine = state.matcher.engine_arc().cloned();
    let ar_reranker = if let Some(ref s) = server {
        s.read().await.reranker.clone()
    } else {
        None
    };
    tokio::spawn(async move {
        let idle_lab = std::sync::Arc::new(crate::memory::idle_lab::IdleLab::new(ar_silva, &data_dir));
        crate::memory::autoresearch::autoresearch_daemon(idle_lab, ar_engine, ar_reranker).await;
    });
    // ------------------------------------------

    start_http_server(host, port, state)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))
}

fn build_router(state: Arc<HttpState>) -> Router {
    // CORS: Only allow localhost:3000 and localhost:5173 (Vite/React dev)
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
            Method::PATCH,
        ])
        .allow_headers(Any);

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
        .merge(oauth_routes);

    // 2. Protected Routes (API v1 + MCP + SSE)
    let protected_routes = api_v1::api_v1_routes()
        .merge(sse::sse_routes())
        .route("/messages", any(api_v1::mcp_handler))
        .route("/mcp", any(api_v1::mcp_handler))
        .route("/api/v1/mcp", any(api_v1::mcp_handler))
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
                    // Static assets (JS/CSS/fonts) — serve from disk
                    let file_path = static_dir_inner.join(path.trim_start_matches('/'));
                    if file_path.is_file() {
                        if let Ok(bytes) = tokio::fs::read(&file_path).await {
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
                    }
                    // SPA client-side routes → index.html
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
    let index_path = find_workspace_root().join("dashboard/dist/index.html");
    match tokio::fs::read(&index_path).await {
        Ok(bytes) => axum::response::Response::builder()
            .status(200)
            .header("Content-Type", "text/html; charset=utf-8")
            .header("Cache-Control", "no-cache, no-store, must-revalidate")
            .header("Pragma", "no-cache")
            .header("Expires", "0")
            .body(axum::body::Body::from(bytes))
            .expect("valid index response builder"),
        Err(_) => axum::response::Response::builder()
            .status(404)
            .body(axum::body::Body::from("index.html not found"))
            .expect("valid 404 response builder"),
    }
}

async fn health_handler(
    State(state): State<Arc<HttpState>>,
) -> impl IntoResponse {
    let ready = state.health_ready.load(std::sync::atomic::Ordering::Acquire);
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

fn find_workspace_root() -> std::path::PathBuf {
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
                let new_ct = format!("{}; charset=utf-8", ct_str);
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
