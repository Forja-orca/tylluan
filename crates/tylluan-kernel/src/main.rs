//! # TylluanNexus Kernel
//!
//! Sovereign Agentic MCP Hub — the Rust microkernel that powers TylluanMCP v3.

mod setup;
mod cleanup;

use tylluan_kernel::config::{TylluanConfig, load_guild_config};
use tylluan_kernel::transport::server::TylluanServer;
use tylluan_kernel::transport;
use tylluan_kernel::registry::{guild_process::GuildRegistry, guild_list::LAZY_GUILDS, lifecycle, service_manager::ServiceManager, actor::RegistryActor};
use tylluan_kernel::memory::{hybrid::HybridMemory, silva::SilvaDB, mailbox::Mailbox, coloquio::ColoquioDb, consensus::ConsensusEngine, agent_profile::AgentProfileStore, agent_memory::AgentMemoryManager};
use tylluan_kernel::memory::agent_nodes::AgentNodeRouter;
use tylluan_kernel::memory::silva::nodes::build_contextual_text;
use tylluan_kernel::router::{matcher::GuildMatcher, catalog::builtin_catalog, embeddings::{EmbeddingEngine, RerankEngine}};
use tylluan_kernel::doctor::Doctor;
use tylluan_kernel::guard::GuardedTask;
use tylluan_kernel::curriculum::CurriculumLearner;
use tylluan_kernel::hormones::HormoneSystem;
use std::sync::Mutex;

use rmcp::transport::io;
use rmcp::ServiceExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::{info, error, warn};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;
use std::time::Duration;

/// Initializes the tracing subscriber (file + stderr/stdout layers depending on
/// stdio-transport mode) and returns whether the kernel is running in `--stdio` mode.
fn init_logging(args: &[String]) -> bool {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    // Always initialize logging - keep stderr clean for human readability
    let _ = std::fs::create_dir_all("./logs");
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("./logs/kernel.log")
        .unwrap_or_else(|_| {
            let fallback = format!("./kernel_{}.log", std::process::id());
            std::fs::File::create(&fallback).unwrap_or_else(|e| {
                eprintln!("FATAL: cannot create log file '{fallback}': {e}");
                std::process::exit(1);
            })
        });

    // Final Fix for ANSI/JSON-RPC corruption.
    // In stdio mode, we MUST redirect all logs to stderr so stdout remains pure JSON.
    let is_stdio = args.contains(&"--stdio".to_string());
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(log_file)
        .with_ansi(false);

    if is_stdio {
        let stderr_layer = tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(false);
        tracing_subscriber::registry()
            .with(filter)
            .with(file_layer)
            .with(stderr_layer)
            .init();
    } else {
        let stdout_layer = tracing_subscriber::fmt::layer()
            .with_writer(std::io::stdout)
            .with_ansi(true);
        tracing_subscriber::registry()
            .with(filter)
            .with(file_layer)
            .with(stdout_layer)
            .init();
    }

    #[cfg(target_os = "windows")]
    setup::setup_windows_job_object();

    // Use eprintln for the startup message in stdio mode to stay off stdout
    if is_stdio {
        eprintln!("INFO: TylluanNexus kernel started (stdio mode)");
    } else {
        info!("TylluanNexus kernel started (headless mode) - logs written to ./logs/kernel.log");
    }

    is_stdio
}

/// Handles one-shot maintenance CLI commands (`--export`, `--import`,
/// `--download-models`) that exit immediately without starting the kernel.
/// Returns `Ok(true)` if a maintenance command was handled (caller should exit).
async fn handle_maintenance_commands(args: &[String]) -> anyhow::Result<bool> {
    if args.contains(&"--export".to_string()) {
        let output = setup::get_cli_arg(args, "--export")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("tylluan-sovereign-state.tar.gz"));

        let config = tylluan_kernel::config::TylluanConfig::load()?;
        let data_dir = PathBuf::from(&config.memory.db_path).parent().unwrap_or(Path::new("data")).to_path_buf();
        let config_dir = PathBuf::from("config");
        let models_dir = PathBuf::from("models");
        let venv_dir = PathBuf::from(".venv");

        let venv_arg = if venv_dir.exists() { Some(venv_dir.as_path()) } else { None };

        tylluan_kernel::maintenance::export_state(&output, &data_dir, &config_dir, &models_dir, venv_arg)?;
        return Ok(true);
    }

    if args.contains(&"--import".to_string()) {
        let input = setup::get_cli_arg(args, "--import")
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("Usage: --import <file.tar.gz>"))?;

        tylluan_kernel::maintenance::import_state(&input, Path::new("."))?;
        return Ok(true);
    }

    // Download missing models command
    if args.contains(&"--download-models".to_string()) {
        info!("📥 Downloading missing AI models...");
        let models_dir = PathBuf::from("models");
        let downloader = tylluan_kernel::maintenance::ModelDownloader::new();
        match downloader.download_missing(&models_dir).await {
            Ok(downloaded) => {
                if downloaded.is_empty() {
                    info!("✅ All models already present.");
                } else {
                    info!("✅ Downloaded models: {:?}", downloaded);
                }
            }
            Err(e) => {
                error!("❌ Model download failed: {}", e);
            }
        }
        return Ok(true);
    }

    Ok(false)
}

/// Anti-Orphan Protection, for non-technical users ("huérfanos de la informática"):
/// 1. Write PID file so we can detect stale kernels
/// 2. Kill any orphan Python guild processes from previous crashed sessions
///
/// Also runs P4 garbage collection of residual data from previous sessions/stress tests.
fn anti_orphan_protection_and_gc() {
    let pid_file = PathBuf::from("./data/tylluan-nexus.pid");
    let _ = std::fs::create_dir_all("./data");

    // Check for stale PID file (previous crash)
    if pid_file.exists()
        && let Ok(old_pid_str) = std::fs::read_to_string(&pid_file)
            && let Ok(old_pid) = old_pid_str.trim().parse::<u32>() {
                info!("🧹 Found stale PID file ({}). Cleaning up orphan processes...", old_pid);
                cleanup::cleanup_orphan_guilds(old_pid);
            }

    // Write current PID
    let _ = std::fs::write(&pid_file, std::process::id().to_string());

    cleanup::cleanup_residual_data();
}

/// Detects low-memory environments (< 2 GB total RAM) via sysinfo when not
/// already explicitly configured, so timeouts can be reduced downstream.
fn detect_low_memory_mode(configured: bool) -> bool {
    if configured {
        return true;
    }
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let total_mem = sys.total_memory();
    if total_mem > 0 {
        let total_gb = total_mem as f64 / (1024.0 * 1024.0 * 1024.0);
        if total_gb < 2.0 {
            warn!("⚠️ RAM baja detectada: {:.1} GB. Timeouts reducidos al 50%.", total_gb);
            return true;
        }
    }
    false
}

/// Refuses to boot with an insecure LAN-exposed, unauthenticated configuration.
/// See briefing 003 + memory/security_invariant_bind.md.
fn enforce_security_guard(config: &TylluanConfig, cli_token: &Option<String>) {
    let host = config.nexus.host.as_str();
    let lan_exposed = host == "0.0.0.0" || host == "::" || host == "[::]";
    let no_token = cli_token.as_ref().is_none_or(|t| t.is_empty())
        && std::env::var("TYLLUAN_TOKEN").ok().filter(|s| !s.is_empty()).is_none();
    let has_override = std::env::var("TYLLUAN_ALLOW_INSECURE").ok().as_deref() == Some("1");

    if lan_exposed && config.nexus.dev_mode && no_token && !has_override {
        eprintln!();
        eprintln!("⛔ INSECURE CONFIG REFUSED");
        eprintln!("   host = \"{host}\"  →  exposes the kernel on every network interface");
        eprintln!("   dev_mode = true   →  bearer auth is disabled");
        eprintln!("   TYLLUAN_TOKEN env   →  not set");
        eprintln!();
        eprintln!("   Combined, this is unauthenticated remote code execution on your LAN/WiFi.");
        eprintln!();
        eprintln!("   Choose ONE:");
        eprintln!("     (a) Edit tylluan.toml: host = \"127.0.0.1\"   (recommended)");
        eprintln!("     (b) Set env TYLLUAN_TOKEN=<strong-token> AND set dev_mode = false");
        eprintln!("     (c) Set env TYLLUAN_ALLOW_INSECURE=1         (you accept the risk)");
        eprintln!();
        std::process::exit(1);
    }

    if lan_exposed && (config.nexus.dev_mode || no_token) && has_override {
        warn!("⚠️ TYLLUAN_ALLOW_INSECURE=1 set — kernel running LAN-exposed without auth. You accepted the risk.");
    }

    // Additional guard: dev_mode=true on any non-localhost host (Hallazgo #7)
    if config.nexus.dev_mode && host != "127.0.0.1" && host != "localhost" && !has_override {
        panic!(
            "UNSAFE CONFIG: dev_mode=true con host={host} expone el kernel sin autenticacion en la red. \
             Usa host=\"127.0.0.1\" o desactiva dev_mode."
        );
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file if present
    dotenvy::dotenv().ok();

    let cancel_token = tokio_util::sync::CancellationToken::new();
    
    let workspace_root = setup::find_workspace_root();
    
    // SOVEREIGN FIX: Always operate from workspace root
    if let Err(e) = std::env::set_current_dir(&workspace_root) {
        eprintln!("⚠️ Failed to set current dir to workspace root: {e}");
    }

    // 0. Parse CLI arguments
    let args: Vec<String> = std::env::args().collect();

    // Register --config's path override BEFORE any TylluanConfig::load()/
    // load_cached() call below -- CONFIG_PATH_OVERRIDE is a OnceLock, so
    // whichever call reaches find_config_file() first wins permanently.
    // This used to run after the pre-cache call further down, meaning
    // --config only ever affected this file's local `config` variable and
    // never HttpState.config (a separate global cache read by every sandbox/
    // grants/setup-hint handler) -- the cache had already locked in the
    // cwd/default-discovered file by the time the override was registered.
    if let Some(pos) = args.iter().position(|r| r == "--config")
        && let Some(config_path_str) = args.get(pos + 1) {
            tylluan_kernel::config::TylluanConfig::set_config_path_override(
                std::path::PathBuf::from(config_path_str)
            );
        }

    let cli_token = setup::get_cli_arg(&args, "--token");

    // Logging must come up before anything else logs; is_stdio also gates stdout usage below.
    let is_stdio = init_logging(&args);

    // One-shot maintenance commands exit immediately without starting the kernel.
    if handle_maintenance_commands(&args).await? {
        return Ok(());
    }

    anti_orphan_protection_and_gc();

    // 1. Initial configuration (load once at startup)
    let mut config = TylluanConfig::load()
        .unwrap_or_else(|e| {
            warn!("⚠️ Failed to load config: {}, using defaults", e);
            TylluanConfig::default()
        });
    // Pre-cache for hot-reload API (optional)
    let _ = TylluanConfig::load_cached();
    
    // Override port from CLI if present
    if let Some(pos) = args.iter().position(|r| r == "--port")
        && let Some(port_str) = args.get(pos + 1)
            && let Ok(port) = port_str.parse::<u16>() {
                config.nexus.port = port;
                info!("🔌 Port override from CLI: {}", port);
            }

    // Override host from CLI if present (Essential for WSL/External access)
    if let Some(pos) = args.iter().position(|r| r == "--host")
        && let Some(host_str) = args.get(pos + 1) {
            config.nexus.host = host_str.clone();
            info!("🌐 Host override from CLI: {}", host_str);
        }

    // Override config path from CLI if present. The path override itself is
    // already registered earlier (before the pre-cache load_cached() call) --
    // this just applies it to this file's local `config` variable too.
    if let Some(pos) = args.iter().position(|r| r == "--config")
        && let Some(config_path_str) = args.get(pos + 1) {
            info!("📄 Config path override from CLI: {}", config_path_str);
            if let Ok(content) = std::fs::read_to_string(config_path_str) {
                config = toml::from_str(&content)?;
            } else {
                warn!("⚠️ Config file from CLI not found or invalid: {}", config_path_str);
            }
        }

    // Force stdio from CLI if present
    if args.contains(&"--stdio".to_string())
        && !config.nexus.transport.contains(&"stdio".to_string()) {
            config.nexus.transport.push("stdio".to_string());
            info!("📡 Stdio transport forced from CLI");
        }

    let use_stdio = config.nexus.transport.contains(&"stdio".to_string());
    
    // Always validate security after all possible overrides (CLI, ENV, File)
    config.validate_security();

    // ─── Low Memory Detection ──────────────────────────────────────────
    let low_memory_mode = detect_low_memory_mode(config.low_memory_mode);

    // ─── Tunnel Manager ────────────────────────────────────────────────────
    let mut tunnel_manager = tylluan_kernel::tunnel::TunnelManager::new(
        config.tunnel.clone(),
        config.nexus.port,
    );
    tunnel_manager.start();

    // ─── Security guard: refuse LAN-exposed unauthenticated boot ───────────
    enforce_security_guard(&config, &cli_token);

    info!("--------------------------------------------------");
    info!("🛡️ TYLLUANNEXUS o3 KERNEL — Sovereignty Activated (HEADLESS)");
    info!("--------------------------------------------------");

    // Startup banner: always visible on stderr regardless of log level.
    // In stdio mode stderr is safe (stdout reserved for JSON-RPC).
    // In HTTP mode this tells the user the kernel is alive.
    if !is_stdio {
        eprintln!();
        eprintln!("TylluanNexus o3 — Sovereign Kernel");
        eprintln!("  Version : {}", env!("CARGO_PKG_VERSION"));
        eprintln!("  Logs    : ./logs/kernel.log");
        eprintln!("  Config  : tylluan.toml");
        eprintln!("  Ready   : waiting for transports...");
        eprintln!();
    }

    // 0. Startup Integrity Check (T10)
    if let Err(e) = tylluan_kernel::memory::backup::run_startup_integrity_check().await {
        warn!("⚠️ [Integrity] Startup check failed (continuing anyway): {}", e);
    }

    // ─── Secret/Token handling ──────────────────────────────────────
    // SECURITY: Get auth token from config/env only - NO HARDCODED FALLBACK
    let mut auth_token_opt = config.ensure_auth_token()?;
    if let Some(t) = &cli_token
        && !t.is_empty() {
            auth_token_opt = Some(t.clone());
        }
    
    // Final token verification
    if auth_token_opt.is_none() && !config.nexus.dev_mode {
        error!("❌ [FATAL] No authentication token could be established and dev_mode=false.");
        error!("   Set TYLLUAN_TOKEN environment variable or check .tylluan-token file.");
        std::process::exit(1);
    }

    let token = auth_token_opt.clone();
    if let Some(ref _t) = token
        && !is_stdio {
            eprintln!("🔐 Auth: Master Token is active (see .tylluan-token for the full key)");
        }

    // Capture binary downloads channel for the TUI and SSE
    let (download_tx, _) = tokio::sync::broadcast::channel::<tylluan_kernel::maintenance::DownloadProgress>(100);
    

    // ─── Initialize Core Subsystems ────────────────────────────────
    // Allow TYLLUAN_DATA_DIR env var to override db paths (useful for Docker/WSL)
    if let Ok(data_override) = std::env::var("TYLLUAN_DATA_DIR") {
        let base = PathBuf::from(&data_override);
        config.memory.db_path = base.join("tylluan.db").to_string_lossy().into_owned();
        config.silva.db_path = base.join("silva.db").to_string_lossy().into_owned();
    }
    let db_path = Path::new(&config.memory.db_path);
    let data_dir = db_path.parent().unwrap_or_else(|| Path::new("data"));
    std::fs::create_dir_all(data_dir).ok();

    let jobs_path = data_dir.join("jobs.db");
    let jobs = Arc::new(tylluan_kernel::memory::jobs::JobQueue::open(&jobs_path)?);
    let resumed = jobs.resume_pending()?;
    if resumed > 0 {
        info!("Resumed {} pending jobs from previous session", resumed);
    }

    // ─── Initialize Cryptographic Node Identity (M12-A) ───────────
    let identity_path = data_dir.join("identity.key");
    info!("🔑 Loading cryptographic node identity: {}", identity_path.display());
    let node_identity = match tylluan_link::identity::NodeIdentity::load_or_create(&identity_path) {
        Ok(id) => {
            info!("🔑 Node public key (Ed25519): {}", id.public_key_hex());
            Arc::new(id)
        }
        Err(e) => {
            error!("❌ [FATAL] Node identity key is corrupted or inaccessible: {}. Boot aborted.", e);
            std::process::exit(1);
        }
    };

    // ─── M12-C: Boot-time NAT external address discovery ──────────
    // REAL BUG FIX (2026-08-19, found live via the security-claims-gate CI
    // job hanging for minutes before the HTTP port was ever bound): this used
    // to `.await` synchronously HERE, before the HTTP server starts (see the
    // "HTTP Server FIRST — before guilds" comment below) -- but
    // discover_external_addr()'s result is never consumed anywhere else in
    // main(), it's pure fire-and-log. Its underlying stun_binding_request()
    // (tylluan-link/src/nat.rs) uses blocking std::net::UdpSocket with DNS
    // resolution (to_socket_addrs()) that has NO timeout of its own -- only
    // the UDP recv is bounded by stun_timeout_secs. On a network that drops
    // (rather than fast-rejects) this traffic -- a GitHub Actions runner,
    // or any real operator behind a restrictive firewall/corporate network
    // -- 2 servers x 3 attempts each meant up to 6 unbounded DNS lookups
    // blocking kernel startup for minutes, delaying /health and the HTTP
    // listener bind for a purely optional, best-effort P2P convenience
    // feature. Moved to a background tokio::spawn, matching the same
    // "start in background, don't block boot" pattern already used for
    // guild spawning just below.
    {
        let nat_config = tylluan_link::nat::NatConfig {
            stun_servers: config.nat.stun_servers.clone(),
            stun_timeout_secs: config.nat.stun_timeout_secs,
            stun_retries: config.nat.stun_retries,
        };
        tokio::spawn(async move {
            match tylluan_link::nat::discover_external_addr(&nat_config).await {
                Ok(addr) => info!("🌐 NAT external IP: {}:{} (via {})", addr.ip, addr.port, addr.stun_server),
                Err(e) => info!("🌐 NAT external address not available (fallback to LAN): {e}"),
            }
        });
    }

    let prefix = db_path.file_stem().unwrap_or_default().to_string_lossy();
    let mailbox_name = if prefix.contains("test") { format!("{prefix}_mailbox.db") } else { "mailbox.db".to_string() };

    let silva_path = PathBuf::from(&config.silva.db_path);
    let mailbox_path = data_dir.join(mailbox_name);

    info!("🧠 Initializing Memory Layer...");
    info!("🌲 SilvaDB path: {}", silva_path.display());
    info!("📬 Mailbox path: {}", mailbox_path.display());

    let memory = Arc::new(HybridMemory::open(&config.memory.db_path)?);
    let silva = Arc::new(SilvaDB::open(&silva_path.to_string_lossy())?);
    let mailbox = Arc::new(Mailbox::open(&mailbox_path.to_string_lossy())?);
    let coloquio = Arc::new(ColoquioDb::new(&mailbox_path.to_string_lossy())?);
    let coloquio_for_shutdown = coloquio.clone();

    memory.init().await?;
    silva.init().await?;
    mailbox.init().await?;

    // ─── Agent Profile Store ────────────────────────────────────────
    let agent_profiles_path = data_dir.join("agent_profiles.db");
    let agent_profiles = match AgentProfileStore::new(&agent_profiles_path.to_string_lossy()) {
        Ok(store) => {
            info!("🧬 AgentProfileStore: online at {}", agent_profiles_path.display());
            Some(Arc::new(Mutex::new(store)))
        }
        Err(e) => {
            warn!("⚠️ AgentProfileStore failed to initialize (non-fatal): {}", e);
            None
        }
    };

    // ─── Register Kernel Identity ──────────────────────────────────
    let kernel_id = "agent:tylluan-nexus-o3";
    let _ = silva.upsert_node(
        kernel_id, 
        "identity", 
        "TylluanNexus o3 Sovereign Kernel", 
        &serde_json::json!({
            "agent": "tylluan-nexus-o3",
            "type": "kernel",
            "version": env!("CARGO_PKG_VERSION"),
            "status": "online"
        }).to_string()
    ).await;
    let _ = silva.set_protected(kernel_id, true).await;

    // ─── Seed Universal Skill Node ─────────────────────────────────
    let skill_id = "skill:universal:tylluan-nexus-o3";
    let skill_content = concat!(
        "TylluanNexus o3 Universal Skill — qué puedo hacer en este sistema. ",
        "5 herramientas soberanas: tylluan_do (ejecuta cualquier tarea via guilds), ",
        "tylluan_remember (guarda en memoria a largo plazo SilvaDB), ",
        "tylluan_recall (busca en memoria con BGE-M3 hybrid BM25+vector RRF, 82% Recall@5), ",
        "tylluan_think (razonamiento profundo PageRank + GraphRAG), ",
        "tylluan_graph (inspecciona y manipula el grafo SilvaDB). ",
        "Guilds activos: bash git filesystem monitor docker code coloquio memory knowledge web sequential_thinking vision deep_analysis. ",
        "Endpoints clave: GET /api/v1/skill (este documento completo), GET /health, GET /api/v1/guilds, POST /api/v1/do. ",
        "Protocolo: memoria compartida via SilvaDB, coordinacion via coloquio canal mision-activa, ",
        "NightConsolidation horario, soberania total sin cloud ni GPU. ",
        "Quick start: tylluan_recall('project status') -> tylluan_do('lee canal mision-activa') -> tylluan_remember('[AGENT] ready'). ",
        "Benchmark: LongMemEval-S 50q Recall@5=82% CPU-only (Recall@10=90%), sin Neo4j ni dependencias cloud."
    );
    let _ = silva.upsert_node(
        skill_id,
        "agent_skill",
        skill_content,
        &serde_json::json!({
            "type": "universal_skill",
            "endpoint": "/api/v1/skill",
            "version": env!("CARGO_PKG_VERSION"),
            "tools": ["tylluan_do", "tylluan_remember", "tylluan_recall", "tylluan_think", "tylluan_graph"]
        }).to_string()
    ).await;
    let _ = silva.set_protected(skill_id, true).await;
    info!("🎯 Universal skill node seeded: {}", skill_id);

    // IdleLab: restore best retrieval params from previous experiments
    {
        use tylluan_kernel::memory::idle_lab::IdleLab;
        let idle = IdleLab::new(silva.clone(), data_dir);
        idle.load_best_params();
    }

    // ─── Initialize Sovereign AI Engines ───────────────────────────
    info!("🧭 Initializing Sovereign AI Engines (Toaster-friendly)...");

    // 1. Semantic Matcher (embed ALL guilds for lazy discovery, spawn only always_on)
    // ─── Curriculum Learner (Thompson sampling router) ─────────────
    let curriculum_path = data_dir.join("curriculum.db");
    let mut curriculum_raw = CurriculumLearner::new(&curriculum_path.to_string_lossy(), 5)
        .unwrap_or_else(|e| {
            warn!("Curriculum DB failed: {}, using in-memory fallback", e);
            CurriculumLearner::new_in_memory(5).expect("In-memory curriculum must work")
        });
    let catalog = builtin_catalog();
    
    // SEEDING: Use a generalist role as base seed if no agent is yet known
    let seeded = curriculum_raw.seed_from_catalog(&catalog, "generalist").unwrap_or(0);
    if seeded > 0 {
        info!("🌱 Curriculum seeded with {} a-priori entries from catalog", seeded);
    }
    
    if let Ok(Some(saved_curriculum)) = CurriculumLearner::restore(&silva).await {
        info!("📚 Curriculum restaurado desde SilvaDB");
        curriculum_raw = saved_curriculum;
    }
    let curriculum = Arc::new(Mutex::new(curriculum_raw));

    let mut matcher = GuildMatcher::new(builtin_catalog());
    let _always_on = config.guilds.core.always_on.clone();
    let needs_bg_download = if let Some(ref _model) = tylluan_kernel::router::embeddings::EmbeddingEngine::model_path_from_config(&config.memory.embedding_model) {
        if !fastembed_model_cached(&config.memory.embedding_model) {
            info!("📥 Model '{}' not cached — fast start with BM25, background download pending", config.memory.embedding_model);
            true
        } else {
            let model_name = &config.memory.embedding_model;
            info!("🧠 Pre-loading embedding model: {}", model_name);
            let _ = tokio::task::block_in_place(|| matcher.load_model_with_device(None, model_name, &config.inference.device));
            false
        }
    } else {
        info!("🧠 Embedding model disabled — using BM25-only retrieval");
        false
    };
    let bg_model_name = if needs_bg_download { Some(config.memory.embedding_model.clone()) } else { None };
    let matcher = matcher.with_curriculum(curriculum.clone());
    // Create shared HormoneSystem before Arc-wrapping so both matcher and server share the same instance
    let hormones_shared = Arc::new(std::sync::Mutex::new(HormoneSystem::new()));
    let matcher = matcher.with_hormones(hormones_shared.clone());
    let matcher = Arc::new(matcher);

    let reranker = match tokio::task::block_in_place(|| RerankEngine::load_with_device(&config.inference.device)) {
        Ok(r) => {
            info!("🔀 Reranker BGE listo");
            Some(Arc::new(r))
        }
        Err(e) => {
            warn!("⚠️ Reranker no disponible (fallback a RRF puro): {}", e);
            None
        }
    };

    // ─── Hub Mailbox Listener (processes incoming agent proposals) ──────────
    let mailbox_hub = mailbox.clone();
    let silva_hub = silva.clone();
    let matcher_hub_clone = matcher.clone();

    info!("🚀 Hub: Spawning Mailbox Listener thread...");
    tokio::spawn(async move {
        info!("🚀 Hub: Listener thread started - event-driven mode (Sovereign Efficiency)");
        loop {
            match mailbox_hub.check_mail("hub", true, 5).await {
                Ok(messages) if !messages.is_empty() => {
                    info!("📬 Hub: Found {} unread messages in 'hub' mailbox.", messages.len());
                    for msg in messages {
                        let trace_id = format!("tx_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
                        info!("[{}] 📬 Hub: Raw mail payload from {}: {}", trace_id, msg.sender_id, msg.payload);
                        if let Ok(mut proposal) = serde_json::from_str::<serde_json::Value>(&msg.payload) {
                            if let Some(s) = proposal.as_str() {
                                if let Ok(inner) = serde_json::from_str::<serde_json::Value>(s) {
                                    proposal = inner;
                                } else {
                                    warn!("⚠️ Hub: Falló doble parse del payload como string, usando raw");
                                }
                            }
                            info!("[{}] 📬 Hub: Parsed JSON proposal: {:?}", trace_id, proposal);
                            if proposal["type"] == "lesson_proposal" {
                                let topic = proposal["topic"].as_str().unwrap_or("unknown_topic");
                                let content = proposal["content"].as_str().unwrap_or("");
                                
                                info!("[{}] 📬 Hub: Processing proposal (Topic: {}, Len: {})", trace_id, topic, content.len());
                                
                                if !content.is_empty() {
                                    let p_id = format!("proposal_{}_{}", msg.sender_id, &uuid::Uuid::new_v4().simple().to_string()[..8]);

                                    // 1. Persist node
                                    info!("[{}] 🧠 SilvaDB: Attempting to upsert node '{}'...", trace_id, p_id);
                                    let upsert_res = silva_hub.upsert_node(
                                        &p_id,
                                        "lesson",
                                        content,
                                        &msg.payload
                                    ).await;
                                    
                                    match upsert_res {
                                        Ok(_) => {
                                            info!("[{}] 🧠 SilvaDB: Node '{}' persisted successfully.", trace_id, p_id);
                                            // 2.b Handle protected status if specified in proposal (Sovereign Authority)
                                            if proposal["protected"].as_bool() == Some(true) {
                                                info!("[{}] 🛡️ Hub: Marking node '{}' as PROTECTED in SilvaDB", trace_id, p_id);
                                                let _ = silva_hub.set_protected(&p_id, true).await;
                                            }
                                        },
                                        Err(e) => error!("[{}] ❌ SilvaDB: Failed to persist node '{}': {}", trace_id, p_id, e),
                                    }

                                    // 2. Mark as conflicted
                                    if let Err(e) = silva_hub.mark_conflicted(&p_id, true).await {
                                        error!("[{}] ❌ SilvaDB: Failed to mark node '{}' conflicted: {}", trace_id, p_id, e);
                                    }

                                    // 3. Generate embedding (if matcher ready)
                                    let silva_inner = silva_hub.clone();
                                    let m_hub = matcher_hub_clone.clone();
                                    let node_id = p_id.clone();
                                    let text = content.to_string();

                                    tokio::spawn(async move {
                                        let guard = GuardedTask::new("Proposal Embedding", Duration::from_secs(30));
                                        let _ = guard.run(async move {
                                            if let Some(engine) = m_hub.engine() {
                                                match engine.embed(&text) {
                                                    Ok(embedding) => {
                                                        let _ = silva_inner.save_embedding(
                                                            &node_id,
                                                            &embedding,
                                                            &engine.engine_id(),
                                                            engine.engine_hash().as_deref()
                                                        ).await;
                                                        info!("🧠 SilvaDB: Embedding for '{}' saved successfully.", node_id);
                                                    },
                                                    Err(e) => warn!("Embedding failed: {}", e),
                                                }
                                            }
                                            Ok::<(), anyhow::Error>(())
                                        }).await;
                                    });
                                }
                            }
                        }
                    }
                }
                Ok(_) => {}
                Err(e) => warn!("⚠️ Mailbox Hub Listener error: {}", e),
            }
            mailbox_hub.notifier.notified().await;
        }
    });

    // ─── Registry Actor ───────────────────────────────────────────
    let mut registry_raw = GuildRegistry::new(
        workspace_root.clone(),
        config.guilds.core.lazy_load_timeout_secs,
        config.nexus.timeouts.clone(),
        config.guilds.guild_max_concurrent_calls,
    );

    if let Err(e) = registry_raw.load() {
        warn!("⚠️ [T27] Failed to load registry: {}", e);
    }

    // Initialize metrics database for guild metrics persistence
    if let Err(e) = registry_raw.init_metrics_db(&config.memory.db_path) {
        warn!("⚠️ Failed to initialize guild metrics database: {}", e);
    } else {
        // Load previously persisted metrics on startup
        if let Err(e) = registry_raw.load_metrics().await {
            warn!("⚠️ Failed to load persisted guild metrics: {}", e);
        }
    }
    
    // Lazy guilds: on-demand only, never in tylluan.toml always_on list.
    // Always-on guilds are handled by the loop below — don't duplicate them here.
    //
    // This list lives in `registry::guild_list::LAZY_GUILDS` (not inline) so that
    // router::catalog's structural drift test can assert every catalog guild is
    // reachable from either this list or tylluan.toml's always_on list. See that
    // constant's doc comment for the bug history this closes.
    // CPU inference guilds (vision, deep_analysis): tool_timeout = None → wait indefinitely.
    // Killing ONNX inference mid-run wastes all prior computation. Patience is correct on CPU.
    // Network/fast guilds: tool_timeout = None falls back to 120s default in guild_process.rs.
    let cpu_inference_guilds = ["vision", "deep_analysis", "knowledge", "comfy_ui", "n8n_bridge"];
    for (name, module, _) in LAZY_GUILDS.iter().copied() {
        if !registry_raw.guilds.contains_key(name) {
            // cpu_inference_guilds explicitly get None (unlimited); others get system default
            registry_raw.register(name, module, false, None);
        }
    }

    for name in config.guilds.core.always_on.iter() {
        let module = catalog.iter()
            .find(|d| &d.name == name)
            .map(|d| d.module_path.clone())
            .unwrap_or_else(|| format!("guilds.core.{name}"));
        if let Some(g) = registry_raw.guilds.get_mut(name) {
            g.always_on = true;
            if cpu_inference_guilds.contains(&name.as_str()) {
                g.tool_timeout = None; // unlimited patience for CPU inference
            }
        } else {
            registry_raw.register(name, &module, true, None);
        }
    }

    if let Some(v2_config) = &config.guilds.v2 {
        let _discoveries = load_guild_config(&config);
        let legacy_path = &v2_config.legacy_fallback;
        for gremioguilds in &v2_config.gremios {
            let base_path = &gremioguilds.path;
            for plugin in &gremioguilds.plugins {
                let guild_name = plugin.trim_end_matches(".py");
                let module_path = format!("{}.{}", base_path.replace("/", "."), guild_name);
                if !registry_raw.guilds.contains_key(guild_name) {
                    registry_raw.register_v2(guild_name, &module_path, false, None, &gremioguilds.name, gremioguilds.agents.clone());
                } else if let Some(g) = registry_raw.guilds.get_mut(guild_name) {
                    g.guild_id = Some(gremioguilds.name.clone());
                    g.agent_roles = gremioguilds.agents.clone();
                }
                if gremioguilds.always_on
                    && let Some(g) = registry_raw.guilds.get_mut(guild_name) {
                        g.always_on = true;
                    }
            }
            info!("📦 [V2] Loaded gremio '{}' from {} with {} plugins, {} agents",
                  gremioguilds.name, base_path, gremioguilds.plugins.len(), gremioguilds.agents.len());
        }
        info!("📦 [V2] Legacy fallback path: {}", legacy_path);
    }

    for ext in config.external_mcp.iter() {
        if !ext.active {
            info!("⏭️ [external_mcp] '{}' skipped (active=false)", ext.name);
            continue;
        }
        if ext.command.is_none()
            && let Some(url) = &ext.url {
                // URL-only entry → remote HTTP/SSE MCP server
                registry_raw.register_http_mcp(
                    &ext.name,
                    url,
                    ext.headers.clone().unwrap_or_default(),
                    ext.timeout_ms,
                );
                continue;
            }
        // Has a command → stdio-based external MCP
        registry_raw.register_external(
            &ext.name,
            ext.command.as_deref().unwrap_or(""),
            ext.args.clone().unwrap_or_default(),
            ext.cwd.clone().map(PathBuf::from),
            ext.env.clone(),
            ext.timeout_ms,
        );
    }

    if let Err(e) = registry_raw.discover_guilds() {
        warn!("⚠️ Guild discovery failed during startup: {}", e);
    }

    // Start Registry Actor
    // Wrap registry in Arc<RwLock<>> so it can be SHARED between the actor
    // (which serializes mutations via messages) and TylluanServer (which still
    // uses direct .read().await/.write().await for legacy paths).
    let registry_arc = Arc::new(RwLock::new(registry_raw));
    let (registry_actor, registry) = RegistryActor::new(registry_arc.clone());
    tokio::spawn(async move {
        registry_actor.run().await;
    });

    // Periodic guild metrics persistence (every 60 seconds)
    let metrics_reg = registry_arc.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            if let Err(e) = metrics_reg.read().await.persist_metrics().await {
                tracing::warn!("⚠️ Failed to persist guild metrics: {}", e);
            }
        }
    });

    // ─── Initialize Kernel Doctor ───────────────────────────────────
    let doctor = Arc::new(Doctor::new(
        registry_arc.clone(),
        memory.clone(),
        silva.clone(),
        curriculum.clone(),
    ));

    // Doctor background task: run diagnostic every 60s (reduce CPU from spam)
    let doctor_bg = doctor.clone();
    let doc_token = CancellationToken::new();
    let doc_token_inner = doc_token.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            tokio::select! {
                _ = doc_token_inner.cancelled() => {
                    info!("🩺 Doctor background task stopping...");
                    break;
                }
                _ = interval.tick() => {
                    let _ = doctor_bg.diagnose().await;
                }
            }
        }
    });

    // Keep doc_token alive until shutdown (moved to shutdown scope below)
    let _doc_token_guard = doc_token;

    // ─── Initialize Auxiliary Service Manager ────────────────────────
    let service_manager = Arc::new(ServiceManager::new());
    if let Err(e) = service_manager.spawn_configured_services(&config.services).await {
        warn!("⚠️ Some auxiliary services failed to start: {}", e);
    }
    service_manager.start_watchdog(config.services.clone());

    // IVF warm-up: build centroids if >50 embeddings and table empty (non-blocking)
    {
        let silva_ivf = silva.clone();
        tokio::spawn(async move {
            match silva_ivf.consolidate_ivf_index().await {
                Ok(r) if !r.skipped => info!("🔬 IVF warm-up: {} centroids in {}ms", r.n_centroids, r.elapsed_ms),
                Ok(_) => {},
                Err(e) => warn!("🔬 IVF warm-up failed: {}", e),
            }
        });
    }

    // IVF staleness watcher: rebuild every 6h if graph has grown >10% since last build
    {
        let silva_ivf = silva.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(6 * 3600));
            interval.tick().await;
            loop {
                interval.tick().await;
                match silva_ivf.consolidate_ivf_index().await {
                    Ok(r) if !r.skipped => info!("🔬 IVF periodic rebuild: {} centroids in {}ms", r.n_centroids, r.elapsed_ms),
                    Ok(_) => {},
                    Err(e) => warn!("🔬 IVF periodic rebuild failed: {}", e),
                }
            }
        });
    }

    // Shared health signal: false → /health returns warming_up, true → ok
    let health_ready = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    // ─── Grant Registry (M30-P3) ──────────────────────────────────
    tylluan_kernel::security::grants::init();
    tylluan_kernel::security::grants::init_plan_store();
    tylluan_kernel::security::grants::spawn_reaper();

    // ─── Initialize MCP Server ──────────────────────────────────────
    // TylluanServer uses the shared Arc<RwLock<GuildRegistry>> for legacy direct
    // access; the actor-pattern handle (`registry`) flows separately to HTTP.
    let node_router = AgentNodeRouter::new(tokio::sync::broadcast::channel(1).0);
    let mut server = TylluanServer::new(
        registry_arc.clone(),
        matcher.clone(),
        memory.clone(),
        silva.clone(),
        mailbox.clone(),
        doctor.clone(),
        node_router.clone(),
    );
    if let Some(ref ap) = agent_profiles {
        server.agent_profiles = Some(ap.clone());
    }
    // Share the same HormoneSystem instance between matcher and server
    server.hormones = hormones_shared.clone();
    server.reranker = reranker;
    server.agent_memory = Some(Arc::new(AgentMemoryManager::new(silva.clone(), 20)));
    server.coloquio = Some(coloquio.clone());
    server.low_memory_mode = low_memory_mode;
    // MLP scorer: optional ONNX model for learned complexity scoring
    let models_dir = data_dir.join("models");
    std::fs::create_dir_all(&models_dir).ok();
    let mlp_scorer = tylluan_kernel::mlp::MlpScorer::new(&models_dir);
    server.set_mlp(mlp_scorer, data_dir);
    server.expose_guild_tools = config.nexus.expose_guild_tools;

    // Wire crash-safe journal from HttpState into TylluanServer
    let journal_path = "./data/journal.db";
    if std::path::Path::new(journal_path).exists() {
        match crate::transport::http::api_v1::api_journal::JournalDb::open(journal_path) {
            Ok(j) => server.journal = Some(Arc::new(j)),
            Err(e) => tracing::warn!("⚠️ Failed to open journal.db: {}", e),
        }
    }

    // M31-P6: Wire JobQueue into TylluanServer and spawn background worker
    server.set_jobs(jobs.clone());
    if let Some(state) = crate::transport::server::background_jobs::BackgroundWorkerState::from_server(&server) {
        crate::transport::server::background_jobs::spawn_background_worker(state);
    }

    let server_arc = Arc::new(RwLock::new(server));
    let coloquio_for_fly = coloquio.clone(); // reserve clone for coloquio→SilvaDB flywheel

    // ─── HTTP Server FIRST — before guilds — for <2s /health ─────────
    let use_http = config.nexus.transport.contains(&"http".to_string())
        || config.nexus.transport.contains(&"sse".to_string());
    if use_http {
        let host = config.nexus.host.clone();
        let port = config.nexus.port;
        let http_token = auth_token_opt.clone();
        let server_clone = server_arc.clone();
        let registry_for_http = registry.clone();
        let download_clone = download_tx.clone();
        let tunnel_wsl_url = tunnel_manager.wsl_url();
        let jobs_for_http = jobs.clone();
        let cancel_token_clone = cancel_token.clone();
        let health_for_http = health_ready.clone();
        tokio::spawn(async move {
            if let Err(e) = transport::http::start_http_server_with_download(&host, port, http_token.clone(), config.nexus.dev_mode, Some(server_clone), registry_for_http.clone(), download_clone, tunnel_wsl_url, coloquio.clone(), jobs_for_http, cancel_token_clone, health_for_http, node_identity.clone()).await {
                error!("❌ Universal Gateway: HTTP server error: {}", e);
            }
        });
    }

    // ─── Core Guild Spawning (after HTTP) ────────────────────────────
    // Guilds start in background; /health returns warming_up until they finish.
    let core_reg = registry_arc.clone();
    let always_on_names: Vec<String> = config.guilds.core.always_on.clone();
    let health_guilds = health_ready.clone();
    tokio::spawn(async move {
        for name in always_on_names {
            info!("🚀 [Startup] Spawning always-on guild: {}", name);
            if let Err(e) = core_reg.write().await.ensure_guild_running(&name).await {
                error!("❌ [Startup] Failed to spawn guild '{}': {}", name, e);
            }
        }
        health_guilds.store(true, std::sync::atomic::Ordering::Release);
        info!("✅ [Startup] Always-on guilds loaded — /health now returns ok");
    });

    // ─── Warm Pool: pre-warm frequently-used lazy guilds ─────────────
    // These are spawned at boot (faster first-use) but CAN still be killed
    // by idle timeout — unlike always_on which is kept alive by the watchdog.
    let warm_names: Vec<String> = config.guilds.core.warm_pool.clone();
    if !warm_names.is_empty() {
        let warm_reg = registry_arc.clone();
        tokio::spawn(async move {
            for name in warm_names {
                info!("🔥 [WarmPool] Pre-warming guild: {}", name);
                if let Err(e) = warm_reg.write().await.ensure_guild_running(&name).await {
                    warn!("⚠️ [WarmPool] Guild '{}' failed to pre-warm: {}", name, e);
                }
            }
        });
    }

    // ─── Background Model Download (instant start) ──────────────────
    if let Some(ref model_name) = bg_model_name {
        let bg_matcher = matcher.clone();
        let bg_model = model_name.clone();
        let bg_device = config.inference.device.clone();
        let bg_model_inner = bg_model.clone();
        tokio::spawn(async move {
            info!("📥 [Background] Downloading embedding model '{}'...", bg_model);
            let result = tokio::task::spawn_blocking(move || {
                bg_matcher.hot_swap_engine(&bg_model_inner, &bg_device)
            }).await;
            match result {
                Ok(Ok(())) => info!("✅ [Background] Model '{}' loaded — hot-switched to semantic search.", bg_model),
                Ok(Err(e)) => warn!("⚠️ [Background] Model '{}' failed to load: {}", bg_model, e),
                Err(e) => warn!("⚠️ [Background] Spawn error for '{}': {}", bg_model, e),
            }
        });
    }

    // Try to spawn external MCP servers (non-blocking)
    let ext_mcps = config.external_mcp.clone();
    let ext_reg = registry_arc.clone();
    tokio::spawn(async move {
        for ext in ext_mcps {
            if !ext.active {
                info!("⏭️ [Startup] External MCP '{}' skipped (active=false)", ext.name);
                continue;
            }
            let ext_name = ext.name.clone();
            info!("🚀 [Startup] Spawning external MCP: {}", ext_name);
            if let Err(e) = ext_reg.write().await.ensure_guild_running(&ext_name).await {
                warn!("⚠️ External MCP '{}' failed to spawn: {}", ext_name, e);
            }
        }
    });

    let _reaper = lifecycle::start_lifecycle_reaper_with_silva(
        registry_arc.clone(),
        60,
        Some(silva.clone()),
        config.silva.decay_half_life_hours,
    );
    let _supervisor = tylluan_kernel::registry::supervisor::start_supervisor(registry_arc.clone(), 10);

    info!("🛠️ Starting transports...");
    // ─── Start Transports ───────────────────────────────────────────
    let use_http = config.nexus.transport.contains(&"http".to_string())
        || config.nexus.transport.contains(&"sse".to_string());

    if use_http {
        let host = config.nexus.host.clone();
        let port = config.nexus.port;
        let _ = host.clone(); // used by mDNS and sharing policy below

        info!("🌐 UNIVERSAL GATEWAY: Initializing HTTP/SSE transport...");
        info!("   📍 Standard Endpoints: /health, /sse, /sse/download, /messages, /discovery");
        
        if host == "0.0.0.0" {
            info!("   💡 WSL/External Access: Enabled. Connect from WSL using your Windows Host IP.");
            info!("   👉 Tip: Run 'grep -m 1 nameserver /etc/resolv.conf' in WSL to find the IP.");
        } else {
            info!("   🔒 Local-Only: Gateway restricted to localhost ({}).", host);
        }

        // mDNS is opt-in — disabled by default (config.mdns.advertise / discover)
        if config.mdns.advertise {
            tylluan_kernel::transport::mdns::start_mdns_advertiser(port);
        } else {
            info!("📢 mDNS advertiser disabled (set mdns.advertise = true to enable)");
        }

        if config.mdns.discover {
            let mdns_config = Arc::new(RwLock::new(config.clone()));
            let mdns_config_path = if Path::new("tylluan.toml").exists() {
                Some(PathBuf::from("tylluan.toml"))
            } else {
                None
            };
            tylluan_kernel::transport::mdns::start_mdns_discovery(port, mdns_config, mdns_config_path);
        } else {
            info!("🔍 mDNS discovery disabled (set mdns.discover = true to enable)");
        }

        // Sharing policy background task (M6)
        {
            let config = Arc::new(RwLock::new(config.clone()));
            let silva = silva.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(300)).await;
                    let cfg = config.read().await;
                    if !cfg.sharing.enabled {
                        let _ = silva.reset_all_shareable().await;
                        continue;
                    }
                    let min_w = cfg.sharing.min_weight;
                    let min_h = cfg.sharing.min_activity_hours;
                    let types = cfg.sharing.node_types.clone();
                    drop(cfg);
                    let _ = silva.apply_sharing_policy(min_w, min_h, &types).await;
                }
            });
        }

        // Background job processor for episodic_index
        let jobs_clone = jobs.clone();
        let silva_clone = silva.clone();
        let memory_clone = memory.clone();
        let matcher_clone = matcher.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                if let Ok(Some(job)) = jobs_clone.claim_next("episodic_index") {
                    info!("Processing job: {} ({})", job.id, job.task_type);
                    if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&job.payload) {
                        let channel_id = payload.get("channel_id").and_then(|v| v.as_str()).unwrap_or("");
                        let author = payload.get("author_id").and_then(|v| v.as_str()).unwrap_or("unknown");
                        let content = payload.get("content").and_then(|v| v.as_str()).unwrap_or("");
                        let content_trimmed = content.trim();
                        let is_meaningful = content_trimmed.len() > 50 && content_trimmed.contains(char::is_alphanumeric);
                        let is_mision_activa = channel_id == "mision-activa";
                        let is_trazas_tareas = channel_id == "trazas-tareas";
                        let is_meta = channel_id == "meta" || channel_id == "general";
                        let has_target_prefix = content_trimmed.starts_with('[')
                            || content_trimmed.starts_with("TRAZA")
                            || content_trimmed.starts_with("M17-");

                        if is_meaningful && (is_mision_activa || is_trazas_tareas || is_meta || has_target_prefix) {
                            let local_date = chrono::Local::now().format("%Y-%m-%d").to_string();
                            let mgr = tylluan_kernel::memory::agent_memory::AgentMemoryManager::new(silva_clone.clone(), 20);
                            let formatted_content = format!("el {local_date}: {content}");
                            let node_id = mgr.record_memory(author, &formatted_content, 1.2).await;
                            let tagged_text = format!("[{author}] {formatted_content}");
                            let embedding = matcher_clone.engine()
                                .and_then(|e| e.embed(&tagged_text).ok());
                            if let Some(emb) = embedding {
                                let metadata = serde_json::json!({
                                    "agent_id": author,
                                    "importance": 1.2,
                                    "channel_id": channel_id,
                                }).to_string();
                                let _ = silva_clone.save_embedding(&node_id, &emb, "nomic", None).await;
                                let _ = memory_clone.add_document(&tagged_text, &metadata, Some(&emb)).await;
                            } else {
                                let metadata = serde_json::json!({
                                    "agent_id": author,
                                    "importance": 1.2,
                                    "channel_id": channel_id,
                                }).to_string();
                                let _ = memory_clone.add_document(&tagged_text, &metadata, None).await;
                            }
                            let agent_node_id = format!("agent:{author}");
                            let _ = silva_clone.add_edge(&agent_node_id, &node_id, "remembers", 0.9, "{}").await;
                        }
                    }
                    let _ = jobs_clone.mark_done(&job.id);
                }
            }
        });
    }

    let mut stdio_handle = None;
    if use_stdio {
        info!("📡 Starting MCP Stdio transport...");
        let server_stdio = server_arc.read().await.clone();
        let handle = tokio::spawn(async move {
            let (stdin, stdout) = io::stdio();
            if let Ok(service) = server_stdio.serve((stdin, stdout)).await {
                info!("📡 Stdio: client connected, waiting...");
                let _ = service.waiting().await;
                info!("📡 Stdio: client disconnected");
            }
        });
        stdio_handle = Some(handle);
    }

    info!("✅ TylluanNexus Kernel operational.");
    if !is_stdio {
        let display_host = if config.nexus.host == "0.0.0.0" { "127.0.0.1" } else { &config.nexus.host };
        eprintln!("  [OK] Kernel operational — http://{}:{}", display_host, config.nexus.port);
        eprintln!("  [OK] Health: http://{}:{}/health", display_host, config.nexus.port);
        eprintln!("  [OK] Discovery: http://{}:{}/discovery", display_host, config.nexus.port);
        if config.nexus.dev_mode {
            eprintln!("  [!] dev_mode=true — authentication disabled");
        }
        eprintln!("  Press Ctrl+C to stop.");
        if let Some(wsl_url) = tunnel_manager.wsl_url() {
            eprintln!("  🌉 WSL clients: {wsl_url}");
        }
        eprintln!();
    }

    // ─── Routing Anchor Auto-Seed + Warmup ──────────────────────────
    // Phase 1 (immediate): reads routing_anchors_seed.jsonl and upserts any
    //   new/changed anchors into SilvaDB — idempotent, safe on every restart.
    // Phase 2 (after BGE-M3 ready): re-generates embeddings for any anchor
    //   nodes that are missing them. Zero manual steps required.
    {
        let silva_warmup = silva.clone();
        let matcher_warmup = matcher.clone();
        let workspace_for_seed = workspace_root.clone();
        let embedding_model = config.memory.embedding_model.clone();
        tokio::spawn(async move {
            // ── Phase 1: upsert seed corpus (no engine needed) ────────
            let seed_path = workspace_for_seed.join("data").join("routing_anchors_seed.jsonl");
            if seed_path.exists() {
                match std::fs::read_to_string(&seed_path) {
                    Ok(content) => {
                        let mut upserted = 0usize;
                        for line in content.lines().filter(|l| !l.trim().is_empty()) {
                            if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
                                let guild  = entry.get("guild").and_then(|v| v.as_str()).unwrap_or_default();
                                let intent = entry.get("intent").and_then(|v| v.as_str()).unwrap_or_default();
                                let source = entry.get("source").and_then(|v| v.as_str()).unwrap_or("seed");
                                if !guild.is_empty() && !intent.is_empty() {
                                    let _ = silva_warmup.upsert_routing_anchor(guild, intent, source, None).await;
                                    upserted += 1;
                                }
                            }
                        }
                        info!("🌱 Anchor seed: {} entries synced from corpus", upserted);
                    }
                    Err(e) => warn!("⚠️ Could not read anchor seed file: {}", e),
                }
            } else {
                info!("🌱 Anchor seed: no seed file at {} — skipping", seed_path.display());
            }

            // ── Phase 2: wait for BGE-M3, then reembed missing nodes ──
            let no_embedding = embedding_model == "none"
                || embedding_model.is_empty();
            if no_embedding {
                info!("🌱 Embedding model disabled — skipping anchor warmup");
            } else {
                let mut engine_arc: Option<Arc<tylluan_kernel::router::embeddings::EmbeddingEngine>> = None;
                for attempt in 0..90u32 {
                    if let Some(arc) = matcher_warmup.engine_arc() {
                        engine_arc = Some(arc.clone());
                        break;
                    }
                    if attempt % 15 == 0 && attempt > 0 {
                        info!("🌱 Anchor warmup: waiting for BGE-M3... ({}s elapsed)", attempt * 2);
                    }
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
                match engine_arc {
                    None => warn!("⚠️ Anchor warmup: BGE-M3 not available after 3min — anchor routing degraded"),
                    Some(engine) => {
                        match silva_warmup.reembed_anchors(&engine).await {
                            Ok(0) => info!("🌱 Routing anchors: all embeddings present"),
                            Ok(n) => info!("🌱 Routing anchors warmed: {} embeddings generated", n),
                            Err(e) => warn!("⚠️ Anchor reembed failed: {}", e),
                        }
                    }
                }
            }
        });
    }

    // ─── M22-3: Re-embed legacy episode/lesson nodes with distilled embeddings ──
    {
        let silva_re = Arc::clone(&silva);
        let matcher_re = Arc::clone(&matcher);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(5)).await;
            match crate::transport::server::handler_do::re_embed_legacy_nodes(&silva_re, &matcher_re).await {
                Ok(n) => info!("📀 M22 re-embed: {} episode/lesson nodes distilled", n),
                Err(e) => warn!("⚠️ M22 re-embed failed: {}", e),
            }
        });
    }

    // ─── M24/A1-bis: one-shot collapse of legacy duplicate summaries ──
    {
        let silva_cl = Arc::clone(&silva);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(20)).await;
            let rag = tylluan_kernel::memory::graph_rag::GraphRagManager::new(silva_cl);
            match rag.collapse_legacy_summaries().await {
                Ok(n) => info!("🧹 M24 collapse: {} legacy duplicate summaries merged", n),
                Err(e) => warn!("⚠️ M24 collapse failed: {}", e),
            }
        });
    }

    // ─── Hormone Tick (exponential decay every 5s) ──────────────────
    let hormone_server = server_arc.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        static TICK_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        loop {
            interval.tick().await;
            let s = hormone_server.read().await;
            if let Ok(mut h) = s.hormones.lock() {
                h.tick();
            }
            
            let tick = TICK_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if tick.is_multiple_of(60) && tick > 0
                && let Some(c_mutex) = s.matcher.curriculum() {
                    let state_json = if let Ok(c) = c_mutex.lock() {
                        Some(c.serialize_state().to_string())
                    } else {
                        None
                    };
                    if let Some(json_str) = state_json {
                        let _ = s.silva.upsert_node(
                            "__curriculum_state__",
                            "system",
                            "Curriculum Thompson sampling state",
                            &json_str,
                        ).await;
                        let _ = s.silva.set_weight("__curriculum_state__", 999.0).await;
                    }
                }
        }
    });

    // Biological decay scheduler (respects config)
    let silva_decay = silva.clone();
    let decay_enabled = config.silva.decay_enabled;
    let decay_half_life_hours = config.silva.decay_half_life_hours;
    tokio::spawn(async move {
        if !decay_enabled {
            info!("🌲 SilvaDB: Biological decay is DISABLED by config.");
            return;
        }
        let mut decay_interval = tokio::time::interval(Duration::from_secs(300));
        loop {
            decay_interval.tick().await;
            let silva_inner = silva_decay.clone();
            let guard = GuardedTask::new("Memory Decay", Duration::from_secs(60));
            let _ = guard.run(async move {
                silva_inner.apply_decay(decay_half_life_hours).await.map(|_| ())?;
                Ok::<(), anyhow::Error>(())
            }).await;
        }
    });

    // Coloquio→SilvaDB flywheel: background ingestion of coloquio turns into long-term memory
    let coloquio_fly = coloquio_for_fly.clone();
    let silva_fly = silva.clone();
    tokio::spawn(async move {
        let mut fly_interval = tokio::time::interval(Duration::from_secs(60));
        let mut watermarks: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        loop {
            fly_interval.tick().await;
            let channels = match coloquio_fly.list_channels().await {
                Ok(c) => c,
                Err(e) => { tracing::warn!("⚠️ Coloquio flywheel: list_channels failed: {}", e); continue; }
            };
            for ch in &channels {
                let last_turn = watermarks.get(&ch.channel_id).copied().unwrap_or(0);
                let msgs = match coloquio_fly.get_thread(&ch.channel_id, 50, last_turn).await {
                    Ok(m) => m,
                    Err(e) => { tracing::warn!("⚠️ Coloquio flywheel: get_thread({}) failed: {}", ch.channel_id, e); continue; }
                };
                let mut max_turn = last_turn;
                for msg in &msgs {
                    if msg.turn <= last_turn { continue; }
                    let node_id = format!("coloquio_{}_{}", ch.channel_id, msg.turn);
                    let metadata = serde_json::json!({
                        "channel_id": ch.channel_id,
                        "turn": msg.turn,
                        "author_id": msg.author_id,
                        "source": "coloquio_flywheel",
                    });
                    let content = build_contextual_text(&metadata.to_string(), &msg.content);
                    if let Err(e) = silva_fly.upsert_node(&node_id, "coloquio_memory", &content, &metadata.to_string()).await {
                        tracing::warn!("⚠️ Coloquio flywheel: upsert_node({}) failed: {}", node_id, e);
                    }
                    if msg.turn > max_turn { max_turn = msg.turn; }
                }
                if max_turn > last_turn {
                    watermarks.insert(ch.channel_id.clone(), max_turn);
                }
            }
        }
    });

    // Sovereign Agnostic Reindexer (detects model upgrades and re-indexes background)
    let silva_reindex = silva.clone();
    let matcher_reindex = matcher.clone();
    tokio::spawn(async move {
        let mut reindex_interval = tokio::time::interval(Duration::from_secs(600)); // 10 minutes for toaster-friendly
        loop {
            reindex_interval.tick().await;

            let silva_inner = silva_reindex.clone();
            let matcher_inner = matcher_reindex.clone();

            let guard = GuardedTask::new("Agnostic RE-INDEXING", Duration::from_secs(120));
            let _ = guard.run(async move {
                if let Ok(_count) = silva_inner.node_count().await {
                    // TUI update removed
                }

                if let Ok(_protected) = silva_inner.get_protected_nodes(20).await {
                    // TUI update removed
                }

                let maybe_engine: Option<Arc<EmbeddingEngine>> = matcher_inner.engine_arc();

                if let Some(engine) = maybe_engine {
                    let model_id = engine.engine_id();
                    let model_hash = engine.engine_hash();

                    let stale_nodes = silva_inner.get_stale_embeddings(&model_id, model_hash.as_deref()).await?;
                    if !stale_nodes.is_empty() {
                        info!("🧠 Agnostic Indexer: Found {} stale nodes. Re-indexing...", stale_nodes.len());
                        let batch_size: usize = 32;
                        let nodes_vec: Vec<&String> = stale_nodes.iter().take(100).collect();
                        for chunk in nodes_vec.chunks(batch_size) {
                            let mut texts: Vec<String> = Vec::with_capacity(chunk.len());
                            let mut nodes: Vec<(String, String, String)> = Vec::with_capacity(chunk.len());
                            for node_id in chunk {
                                if let Ok(Some(node)) = silva_inner.get_node(node_id).await {
                                    let contextual = build_contextual_text(&node.metadata, &node.content);
                                    texts.push(contextual);
                                    nodes.push((node_id.to_string(), model_id.clone(), model_hash.clone().unwrap_or_default()));
                                }
                            }
                            if texts.is_empty() {
                                continue;
                            }
                            let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
                            match engine.embed_batch(&text_refs) {
                                Ok(vectors) => {
                                    for (vector, (nid, mid, mhash_str)) in vectors.into_iter().zip(nodes.drain(..)) {
                                        let sid = silva_inner.clone();
                                        tokio::spawn(async move {
                                            let mhash_ref = if mhash_str.is_empty() { None } else { Some(mhash_str.as_str()) };
                                            let _ = sid.save_embedding(&nid, &vector, &mid, mhash_ref).await;
                                        });
                                    }
                                }
                                Err(e) => warn!("Batch re-index error for {} nodes: {:?}", chunk.len(), e),
                            }
                            tokio::time::sleep(Duration::from_millis(500)).await;
                        }
                    }
                }
                Ok::<(), anyhow::Error>(())
            }).await;
        }
    });

    // HNSW index rebuild scheduler (runs every 10 minutes, only if >= threshold)
    let silva_hnsw = silva.clone();
    tokio::spawn(async move {
        let mut hnsw_interval = tokio::time::interval(Duration::from_secs(600));
        loop {
            hnsw_interval.tick().await;
            let silva_inner = silva_hnsw.clone();
            let guard = GuardedTask::new("HNSW Rebuild", Duration::from_secs(120));
            let _ = guard.run(async move {
                silva_inner.rebuild_hnsw_if_needed().await?;
                Ok::<(), anyhow::Error>(())
            }).await;
        }
    });

    // Collective memory consensus scheduler (runs every 1 hour)
    // Optimized: Uses 60s tick instead of 1s to save CPU on toaster hardware
    let silva_consensus = silva.clone();
    let matcher_consensus = matcher.clone();
    tokio::spawn(async move {
        let mut consensus_interval = tokio::time::interval(Duration::from_secs(60));
        let mut secs_to_consensus: u64 = 3600;
        let silva_hub_consensus = silva_consensus;
        loop {
            consensus_interval.tick().await;
            
            if secs_to_consensus > 60 {
                secs_to_consensus -= 60;
            } else {
                secs_to_consensus = 3600;
                let silva_inner = silva_hub_consensus.clone();
                let embedding_engine = matcher_consensus.engine();
                let guard = GuardedTask::new("Memory Consensus", Duration::from_secs(120));
                let _ = guard.run(async move {
                    let engine = ConsensusEngine::new(silva_inner);
                    engine.consolidate_with_engine(None, embedding_engine.as_deref()).await.map(|_| ())?;
                    Ok::<(), anyhow::Error>(())
                }).await;
            }

        }
    });
    
    // ─── Retrolink Orphans (one-shot, 10s after startup) ──────────
    {
        let silva_retro = silva.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(10)).await;
            match silva_retro.retrolink_orphans(150, 0.25).await {
                Ok(n) => info!("🌿 retrolink_orphans: created {} new edges from orphan nodes", n),
                Err(e) => warn!("⚠️ retrolink_orphans failed: {}", e),
            }
        });
    }

    // ─── NightConsolidation (hourly: selective decay + agent memory consolidation) ──
    tokio::spawn(run_night_consolidation_loop(
        silva.clone(),
        agent_profiles.clone(),
        curriculum.clone(),
        server_arc.clone(),
        data_dir.to_path_buf(),
        matcher.clone(),
    ));


    // ─── P1: Periodic SQLite Maintenance ───────────────────────────
    // Performs PRAGMA wal_checkpoint(TRUNCATE) every 5 minutes for lean storage.
    let maint_silva = silva.clone();
    let maint_memory = memory.clone();
    let maint_mailbox = mailbox.clone();
    tokio::spawn(async move {
        let mut maint_interval = tokio::time::interval(Duration::from_secs(300));
        loop {
            maint_interval.tick().await;
            info!("🧹 Running periodic SQLite maintenance (Checkpoints)...");
            
            if let Err(e) = maint_silva.checkpoint().await {
                warn!("⚠️ SilvaDB checkpoint failed: {}", e);
            }
            if let Err(e) = maint_memory.checkpoint().await {
                warn!("⚠️ HybridMemory checkpoint failed: {}", e);
            }
            if let Err(e) = maint_mailbox.checkpoint().await {
                warn!("⚠️ Mailbox checkpoint failed: {}", e);
            }
            // Prune dead nodes (weight < 0.05, excludes identity & agent_summary)
            let prune_silva = maint_silva.clone();
            tokio::spawn(async move {
                match prune_silva.prune_dead_nodes(0.05).await {
                    Ok(n) => { if n > 0 { info!("🧹 Pruned {} dead nodes", n); } }
                    Err(e) => warn!("⚠️ prune_dead_nodes failed: {}", e),
                }
            });
        }
    });

    // ─── P2: Sovereign Routine Subsystem (DISABLED - needs async_trait) ──────────
    // let mut routine_manager = tylluan_kernel::routine::RoutineManager::new();
    // routine_manager.register(Box::new(tylluan_kernel::routine::HealthCheckRoutine::new(registry.clone())));
    // routine_manager.register(Box::new(tylluan_kernel::routine::SummarizationRoutine::new(silva.clone())));
    // routine_manager.register(Box::new(tylluan_kernel::routine::CleanupRoutine::new(silva.clone())));
    // let routine_fut = routine_manager.start();
    // tokio::spawn(routine_fut);

    // ─── BLOCK 2: Main Application Loop (Headless Only) ──────────────────
    if let Some(handle) = stdio_handle {
        info!("📡 Kernel: Operating in Stdio mode. Waiting for MCP client...");
        tokio::select! {
            _ = handle => {}
            _ = cancel_token.cancelled() => {
                info!("Shutdown request received during Stdio session.");
            }
        }
    } else {
        info!("   Press Ctrl+C to shutdown gracefully.");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("Ctrl+C received, shutting down...");
            }
            _ = cancel_token.cancelled() => {
                info!("Shutdown request received, shutting down...");
            }
        }
    }

    // ─── Graceful Shutdown ──────────────────────────────────────────
    info!("🛑 Shutting down gracefully...");

    // Announce shutdown to coloquio so agents know the kernel is going down
    let commit = env!("TYLLUAN_GIT_COMMIT");
    let shutdown_msg = format!(
        "🛑 **TylluanNexus kernel apagándose** (v{} commit `{}`). Rebuild en progreso — reconectar en ~2 min.",
        env!("CARGO_PKG_VERSION"),
        &commit[..commit.len().min(8)]
    );
    let _ = coloquio_for_shutdown.post_message("mision-activa", "kernel", "system", &shutdown_msg, "{}").await;

    // 0a. Announce shutdown to the coloquio so agents know the service is going down
    let _ = coloquio_for_shutdown.post_message(
        "general", "system", "system",
        &format!(
            "[SHUTDOWN] Kernel apagandose limpiamente (v{} commit {}). Si era una ventana de despliegue, reintentad en ~60s.",
            env!("CARGO_PKG_VERSION"), env!("TYLLUAN_GIT_COMMIT")
        ),
        "{}",
    ).await;

    // 0b. Tear down tunnel rules (WSL portproxy cleanup)
    tunnel_manager.stop();

    // 1. Kill all active guilds
    registry.reap_idle().await;

    // 2. Shut down auxiliary services 
    service_manager.shutdown_all().await;

    // 3. Clean up PID file (anti-orphan)
    let pid_file = PathBuf::from("./data/tylluan-nexus.pid");
    let _ = std::fs::remove_file(&pid_file);

    info!("👋 TylluanNexus stopped. No orphan processes left.");
    Ok(())
}

async fn run_night_consolidation_loop(
    silva: Arc<SilvaDB>,
    agent_profiles: Option<Arc<Mutex<AgentProfileStore>>>,
    curriculum: Arc<Mutex<CurriculumLearner>>,
    server: Arc<RwLock<TylluanServer>>,
    data_dir: PathBuf,
    matcher: Arc<GuildMatcher>,
) {
    use tylluan_kernel::memory::night::{
        PhaseOrchestrator, PhaseContext,
        DreamPhase, OuroborosPhase, AutoLinkPhase, GraphRagPhase,
        DecayPhase, AgentPhase, CurriculumPhase, IdleLabPhase, FeedbackSignalPhase,
    };

    let orchestrator = PhaseOrchestrator::new(vec![
        Box::new(DreamPhase),
        Box::new(OuroborosPhase),
        Box::new(AutoLinkPhase),
        Box::new(GraphRagPhase),
        Box::new(DecayPhase),
        Box::new(AgentPhase),
        Box::new(CurriculumPhase),
        Box::new(IdleLabPhase),
        Box::new(FeedbackSignalPhase),
    ]);

    let ctx = PhaseContext {
        silva,
        agent_profiles,
        curriculum,
        server,
        data_dir,
        matcher,
    };

    let mut interval = tokio::time::interval(Duration::from_secs(1800));
    loop {
        interval.tick().await;
        orchestrator.run_all(&ctx).await;
    }
}

/// Check if a fastembed model is already cached locally (avoids blocking startup on download).
fn fastembed_model_cached(model_name: &str) -> bool {
    if model_name.is_empty() || model_name == "none" {
        return true;
    }
    let repo_id = match model_name {
        m if m.contains("bge-m3") => "BAAI/bge-m3",
        m if m.contains("bge-small") => "BAAI/bge-small-en-v1.5",
        m if m.contains("minilm") => "Xenova/all-MiniLM-L6-v2",
        _ => "BAAI/bge-m3",
    };
    // HF hub cache dir: $HF_HOME/hub/models--REPO--NAME/  or ~/.cache/huggingface/hub/
    let cache_dir = std::env::var("HF_HOME")
        .map(|h| std::path::PathBuf::from(h).join("hub"))
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .unwrap_or_else(|_| ".".into());
            std::path::PathBuf::from(home).join(".cache").join("huggingface").join("hub")
        });
    let model_dir_name = format!("models--{}", repo_id.replace('/', "--"));
    let model_dir = cache_dir.join(&model_dir_name);
    let cached = model_dir.exists();
    if !cached {
        // Also check the local models/ directory
        let local_path = std::path::PathBuf::from("models").join(model_name);
        return local_path.exists();
    }
    cached
}

