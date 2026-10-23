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
use tylluan_kernel::router::{matcher::GuildMatcher, catalog::builtin_catalog, embeddings::{EmbeddingEngine, RerankEngine, SparseEngine, EngineLoadOutcome, await_engine_load, apply_onnx_thread_cap}};
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
    // D-4: rotación por tamaño. `kernel.log` había llegado a 1,3 GB sin límite.
    // La configuración se lee best-effort aquí (init_logging corre antes de la
    // carga principal): sin fichero o con error, se usan los defaults seguros.
    let logging = tylluan_kernel::config::TylluanConfig::load()
        .map(|c| c.logging)
        .unwrap_or_default();
    let log_writer = tylluan_kernel::logging::RotatingFileWriter::open(
        std::path::Path::new("./logs"),
        "kernel.log",
        logging.max_file_size_bytes(),
        logging.max_files,
    )
    .or_else(|first_err| {
        let fallback = format!("kernel_{}.log", std::process::id());
        eprintln!("WARN: cannot open ./logs/kernel.log ({first_err}); falling back to ./{fallback}");
        tylluan_kernel::logging::RotatingFileWriter::open(
            std::path::Path::new("."),
            &fallback,
            logging.max_file_size_bytes(),
            logging.max_files,
        )
    })
    .unwrap_or_else(|e| {
        eprintln!("FATAL: cannot create any log file: {e}");
        std::process::exit(1);
    });

    // Final Fix for ANSI/JSON-RPC corruption.
    // In stdio mode, we MUST redirect all logs to stderr so stdout remains pure JSON.
    let is_stdio = args.contains(&"--stdio".to_string());
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::sync::Mutex::new(log_writer))
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
/// Single owner of PID-file directory resolution — used identically at boot
/// (anti_orphan_protection_and_gc) and at shutdown, so a test instance's
/// cleanup always targets the exact directory its own boot used.
///
/// Real incident (2026-09-19), RECURRED (2026-09-20 and 2026-10-11):
/// isolated instances kept killing the live production kernel. 27e2141 scoped
/// the PID file to TYLLUAN_DATA_DIR when set; the 09-20 recurrence added the
/// port-scoped dir for non-default --port. Both failed whenever an operator
/// launched without either var (2026-10-11: `--config test.toml` + no --port
/// shared prod's `./data`), and the 10-11 incident also showed a plain
/// `--version` invocation reaching the PID-file logic at all. This version
/// (1) runs only for server-booting invocations (S0), and (2) is fail-closed
/// on peers: a live sibling kernel is killed only when its listening port
/// equals ours, never when the port is undeterminable (S1/S2), typed as
/// [`PeerDecision`].
fn resolve_pid_data_dir(port_override: Option<u16>) -> PathBuf {
    if let Ok(dir) = std::env::var("TYLLUAN_DATA_DIR") {
        return PathBuf::from(dir);
    }
    match port_override {
        Some(port) if port != 47004 => PathBuf::from(format!("./data-port-{port}")),
        _ => PathBuf::from("./data"),
    }
}

/// How to treat the kernel referenced by a pre-existing PID file.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum PeerDecision {
    /// File missing, unreadable, empty, or garbage → nothing to do.
    NoFile,
    /// PID dead or alive-but-not-a-kernel → stale, run orphan cleanup.
    Stale,
    /// Live OTHER kernel with a different port → do NOT touch it or the file.
    OtherPort,
    /// Live OTHER kernel whose port we cannot determine → do NOT touch it or
    /// the file (fail-closed: unknown must not kill).
    UnknownPeer,
    /// Live kernel whose port equals ours → previous crashed instance of our
    /// own port, safe to kill (legacy behavior).
    SamePort,
}

/// Does a live process have a kernel HTTP port, and what is it?
///
/// `sysinfo` does not expose a TCP table, so use the OS listing tool:
/// `netstat -ano` on Windows, `ss -tlnp` on unix when available. Returns
/// `None` when the process is not listening on any TCP port, when the port
/// cannot be attributed (common when netstat lacks parseable columns) —
/// callers must treat `None` as fail-closed (S1).
fn peer_listen_port(pid: u32) -> Option<u16> {
    if pid == 0 {
        return None;
    }
    let output = if cfg!(target_os = "windows") {
        std::process::Command::new("netstat")
            .args(["-ano", "-p", "TCP"])
            .output()
            .ok()? // netstat missing/nonzero exit → cannot determine
    } else {
        // `ss` may not exist on some unix; try `netstat` fallback there too.
        std::process::Command::new("ss")
            .args(["-tlnp"])
            .output()
            .ok()
            .or_else(|| {
                std::process::Command::new("netstat")
                    .args(["-tlnp"])
                    .output()
                    .ok()
            })?
    };
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    parse_listen_port_from_listing(&text, pid)
}

/// Extract the first listening-on-127.0.0.1-or-any port owned by `pid` from
/// pre-formatted `netstat -ano` / `ss -tlnp` output (tests feed synthetic text).
fn parse_listen_port_from_listing(text: &str, pid: u32) -> Option<u16> {
    let pid = pid.to_string();
    for line in text.lines() {
        let lower = line.to_lowercase();
        // Listen-state markers common to netstat(-ano) and ss(-tlnp).
        if !(lower.contains("listen") || lower.contains("listenin")) {
            continue;
        }
        // The PID is the LAST token on both tools' output lines.
        let toks: Vec<&str> = line.split_whitespace().collect();
        if toks.is_empty() || *toks.last()? != pid.as_str() {
            continue;
        }
        // netstat: `[::]:8080` / `127.0.0.1:47004` in the 'Local Address' col;
        // ss uses `*:47004` / `127.0.0.1:47004` too. Take the LAST token that
        // looks like `addr:port` and parse the port strictly.
        for tok in toks.iter().rev() {
            if let Some((_addr, port)) = tok.rsplit_once(':')
                && let Ok(p) = port.parse::<u16>()
                && p != 0
            {
                return Some(p);
            }
        }
    }
    None
}

/// Live sybl kernel probe: PID exists AND its executable looks like a kernel.
fn process_is_kernel_alive(pid: u32) -> bool {
    let mut sys = sysinfo::System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[sysinfo::Pid::from_u32(pid)]), true);
    sys.process(sysinfo::Pid::from_u32(pid))
        .map(|p| {
            let name = p.name().to_string_lossy().to_lowercase();
            name.contains("tylluan-nexus") || name.contains("tylluan_nexus")
        })
        .unwrap_or(false)
}

/// Decide how to treat the kernel behind the PID file given our effective
/// port. Real-world effects are injected so the full decision table can be
/// unit-tested without spawning or touching real processes:
/// - `read_pid` returns the PID-file contents (None = missing/empty/garbage handling stays upstream)
/// - `peer_alive` reports whether the PID exists and is a tylluan-nexus process
/// - `peer_port` reports the peer's listening TCP port (None = alive but undeterminable)
fn decide_pid_action(
    my_port: Option<u16>,
    read_pid: impl Fn() -> Option<String>,
    peer_alive: impl Fn(u32) -> bool,
    peer_port: impl Fn(u32) -> Option<u16>,
) -> (PeerDecision, Option<u32>) {
    let Some(old_pid_str) = read_pid() else {
        return (PeerDecision::NoFile, None); // missing / unreadable / empty
    };
    let Ok(old_pid) = old_pid_str.trim().parse::<u32>() else {
        return (PeerDecision::NoFile, None); // garbage → nothing sane to act on
    };
    if !peer_alive(old_pid) {
        return (PeerDecision::Stale, Some(old_pid)); // dead PID → real stale
    }
    // Live sibling kernel. Fail-closed: only kill when we are SURE it is
    // "our port left behind by a crash", i.e. its port == ours.
    match (my_port, peer_port(old_pid)) {
        (Some(mine), Some(theirs)) if mine == theirs => (PeerDecision::SamePort, Some(old_pid)),
        (Some(_), Some(_)) => (PeerDecision::OtherPort, None),
        (Some(_), None) => (PeerDecision::UnknownPeer, None), // fail-closed
        (None, _) => (PeerDecision::UnknownPeer, None),       // our own port unknown → don't risk it
    }
}

/// Text for invocations that only ask for information (`--version`/`-V`,
/// `--help`/`-h`). These must print and exit BEFORE logging, config loading or
/// any kernel startup: `invocations_boot_server` only protects the PID file,
/// it does not stop the process from booting a whole kernel afterwards.
fn early_info_output(args: &[String]) -> Option<String> {
    let name = env!("CARGO_BIN_NAME");
    if args.iter().skip(1).any(|a| a == "--version" || a == "-V") {
        return Some(format!("{name} {}", env!("CARGO_PKG_VERSION")));
    }
    if args.iter().skip(1).any(|a| a == "--help" || a == "-h") {
        return Some(format!(
            "{name} {}

USAGE:
  {name} [--config <file>] [--token <token>] [--port <port>]
  {name} --stdio
  {name} --export [file.tar.gz] | --import <file.tar.gz> | --download-models
  {name} --version | --help

With no arguments the kernel boots (see tylluan.toml). Use `tylluan start` for the managed CLI.",
            env!("CARGO_PKG_VERSION")
        ));
    }
    None
}

/// S0: should this invocation be treated as a server boot for anti-orphan
/// purposes? Help/version/maintenance/stdio invocations never bind the port,
/// so they must never touch the PID file or kill anything (incident of
/// 2026-10-11: a bare `--version` from prod's CWD killed the live kernel).
fn invocations_boot_server(args: &[String]) -> bool {
    if args.len() <= 1 {
        return true; // bare `tylluan-nexus` → boot
    }
    if args.iter().any(|a| a == "--version" || a == "--help" || a == "-h" || a == "-V"
        || a == "--export" || a == "--import" || a == "--download-models"
        || a == "--stdio") {
        return false;
    }
    true
}

fn anti_orphan_protection_and_gc(args: &[String], effective_port: Option<u16>) {
    // S0: only server-booting invocations may touch PID files / kill peers.
    if !invocations_boot_server(args) {
        info!("🧹 Non-server invocation (--version/--help/maintenance/stdio): skipping anti-orphan entirely");
        return;
    }
    let port_override = args.iter().position(|r| r == "--port")
        .and_then(|pos| args.get(pos + 1))
        .and_then(|p| p.parse::<u16>().ok());
    let data_dir = resolve_pid_data_dir(port_override.or(effective_port));
    let pid_file = data_dir.join("tylluan-nexus.pid");
    let _ = std::fs::create_dir_all(&data_dir);

    // S1/S2: decide what to do with any pre-existing PID file BEFORE writing.
    let pid_file_for_decide = pid_file.clone();
    let (decision, stale_pid) = decide_pid_action(
        effective_port,
        move || std::fs::read_to_string(&pid_file_for_decide).ok().filter(|s| !s.trim().is_empty()),
        process_is_kernel_alive,
        peer_listen_port,
    );
    match (decision, stale_pid) {
        (PeerDecision::Stale, Some(old)) | (PeerDecision::SamePort, Some(old)) => {
            info!("🧹 PID file for our own port is stale/crashed (PID {}). Cleaning up orphan processes...", old);
            cleanup::cleanup_orphan_guilds(old);
        }
        (PeerDecision::OtherPort, _) => {
            warn!("🛡️ PID file {} belongs to a LIVE kernel listening on a DIFFERENT port — not touching it, not overwriting (fail-closed anti-orphan; use the launcher / TYLLUAN_DATA_DIR for test instances)", pid_file.display());
        }
        (PeerDecision::UnknownPeer, _) => {
            warn!("🛡️ PID file {} belongs to a LIVE tylluan-nexus whose listening port could not be determined — not touching it (fail-closed anti-orphan)", pid_file.display());
        }
        (PeerDecision::NoFile, _) | (PeerDecision::Stale, None) | (PeerDecision::SamePort, None) => {}
    }

    // S2: write our own PID only when the file is not owned by a live sibling.
    let may_write = matches!(decision, PeerDecision::NoFile | PeerDecision::Stale | PeerDecision::SamePort);
    if may_write {
        let _ = std::fs::write(&pid_file, std::process::id().to_string());
    }

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

// Re-exported from tylluan_kernel::router::embeddings for local callers and probe tests
use tylluan_kernel::router::embeddings::onnx_runtime_ort_compatible;

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

    // Information-only invocations never start anything.
    if let Some(text) = early_info_output(&args) {
        println!("{text}");
        return Ok(());
    }

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

    // 1. Initial configuration (load once at startup)
    let mut config = TylluanConfig::load()
        .unwrap_or_else(|e| {
            warn!("⚠️ Failed to load config: {}, using defaults", e);
            TylluanConfig::default()
        });
    // D-42: precedencia de arranque CLI > env > fichero. El fichero (incluido
    // `--config`, cuyo override de ruta se registra arriba) ya se cargó; aquí
    // solo se aplican las capas superiores. NO se vuelve a leer `--config`:
    // esa recarga era la que borraba `--port` (bug de DSH, D-42).
    let env_port = std::env::var("TYLLUAN_PORT").ok();
    let env_host = std::env::var("TYLLUAN_HOST").ok();
    config.apply_startup_overrides(&args, env_port.as_deref(), env_host.as_deref());

    // La config global cacheada (HttpState.config) debe ver los mismos
    // overrides que el arranque, no solo el contenido del fichero.
    if let Ok(cached) = TylluanConfig::load_cached() {
        match cached.try_write() {
            Ok(mut guard) => *guard = config.clone(),
            Err(_) => warn!("⚠️ No pude sincronizar la config cacheada con los overrides de arranque"),
        }
    }

    // Force stdio from CLI if present
    if args.contains(&"--stdio".to_string())
        && !config.nexus.transport.contains(&"stdio".to_string()) {
            config.nexus.transport.push("stdio".to_string());
            info!("📡 Stdio transport forced from CLI");
        }

    let use_stdio = config.nexus.transport.contains(&"stdio".to_string());

    // ─── Anti-orphan protection + GC (after config so the effective port is known).
    // Q2 fail-closed: skip the whole PID/peer logic when the kernel won't bind a
    // real HTTP port (stdio-only) or the port cannot be determined.
    let anti_orphan_port = if use_stdio { None } else { Some(config.nexus.port) };
    anti_orphan_protection_and_gc(&args, anti_orphan_port);

    // Always validate security after all possible overrides (CLI, ENV, File)
    config.validate_security();

    // ─── ONNX Thread Pool Cap ──────────────────────────────────────────
    tylluan_kernel::router::embeddings::apply_onnx_thread_cap(config.inference.max_threads);

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
    // OPT-IN (T206, 2026-08-22): STUN discovery phones home to third-party
    // servers (stun.l.google.com:19302) on every boot — that must not happen
    // on air-gapped / portable / BM25-only installs without explicit consent.
    // Gate the whole block behind config.nat.enabled (default false).
    if config.nat.enabled {
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
    } else {
        info!("🌐 NAT/STUN discovery disabled by default (config.nat.enabled=false) -- set it to true to enable NAT traversal");
    }

    let prefix = db_path.file_stem().unwrap_or_default().to_string_lossy();
    let mailbox_name = if prefix.contains("test") { format!("{prefix}_mailbox.db") } else { "mailbox.db".to_string() };

    let silva_path = PathBuf::from(&config.silva.db_path);
    let mailbox_path = data_dir.join(mailbox_name);

    info!("🧠 Initializing Memory Layer...");
    info!("🌲 SilvaDB path: {}", silva_path.display());
    info!("📬 Mailbox path: {}", mailbox_path.display());

    let silva = Arc::new(SilvaDB::open(&silva_path.to_string_lossy())?);
    let mailbox = Arc::new(Mailbox::open(&mailbox_path.to_string_lossy())?);
    let coloquio = Arc::new(ColoquioDb::new(&mailbox_path.to_string_lossy())?);
    let coloquio_for_shutdown = coloquio.clone();

    silva.init().await?;
    // No direct test coverage for this call site — main()'s boot sequence
    // isn't unit-testable in this codebase's existing patterns. A future
    // refactor of the boot sequence must preserve this call (2026-09-26
    // CPU-runaway fix); see purge_legacy_nested_graphrag_summaries()'s own
    // unit tests in silva/nodes.rs for coverage of the underlying behavior.
    match silva.purge_legacy_nested_graphrag_summaries().await {
        Ok((nodes, edges, summaries)) if nodes > 0 => {
            info!("🧹 Startup cleanup: purged {nodes} legacy nested GraphRAG nodes ({edges} edges, {summaries} cluster_summaries rows)");
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("Startup cleanup: failed to purge legacy nested GraphRAG nodes: {e}"),
    }
    mailbox.init().await?;

    // ─── T-DSH-55: cargar las 3 sesiones ONNX en PARALELO (spawn_blocking) ──
    // Antes se cargaban en serie: SparseEngine (BGE-M3), EmbeddingEngine
    // (mxbai, con su fingerprint) y el reranker. Las tres lecturas de modelo
    // son independientes, así que se lanzan ya y cada una se espera en su
    // punto original de instalación: el ORDEN DE LOS RESULTADOS no cambia
    // (sparse → dense → reranker) y los caminos de degradación son EXACTAMENTE
    // los de antes (una sesión que falla no aborta el arranque). El thread
    // pool/entorno de ONNX se prepara UNA vez aquí para no tocar `set_var`
    // en paralelo desde las cargas.
    apply_onnx_thread_cap(None);
    let engine_device = config.inference.device.clone();

    // 1) Sparse (opt-in por config).
    let sparse_handle = if config.silva.hybrid_sparse_enabled {
        let device = engine_device.clone();
        Some(tokio::task::spawn_blocking(move || SparseEngine::try_new(&device)))
    } else {
        None
    };

    // 2) Dense (mxbai): solo si hay modelo configurado y está cacheado.
    let dense_model_name = config.memory.embedding_model.clone();
    let dense_model_path = EmbeddingEngine::model_path_from_config(&dense_model_name);
    let dense_cached = dense_model_path
        .as_ref()
        .is_some_and(|_| fastembed_model_cached(&dense_model_name));
    let needs_bg_download = dense_model_path.is_some() && !dense_cached;
    if dense_model_path.is_none() {
        info!("🧠 Embedding model disabled — using BM25-only retrieval");
    } else if !dense_cached {
        info!("📥 Model '{}' not cached — fast start with BM25, background download pending", dense_model_name);
    } else {
        info!("🧠 Pre-loading embedding model: {}", dense_model_name);
    }
    let dense_handle = match (dense_model_path.as_ref(), dense_cached) {
        (Some(path), true) => {
            let device = engine_device.clone();
            let path = path.clone();
            Some(tokio::task::spawn_blocking(move || {
                EmbeddingEngine::load_with_device(&path, &device)
            }))
        }
        _ => None,
    };
    let bg_model_name = if needs_bg_download { Some(dense_model_name.clone()) } else { None };

    // 3) Reranker (Jina): misma condición que antes (ORT compatible + config).
    // catch_unwind dentro de la tarea = misma defensa que tenía el `main`
    // antes (no-op bajo panic="abort", pero convierte un panic en Err en debug).
    let rerank_enabled =
        onnx_runtime_ort_compatible() && config.silva.reranker_enabled;
    let rerank_handle = if rerank_enabled {
        let device = engine_device.clone();
        Some(tokio::task::spawn_blocking(move || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                RerankEngine::load_with_device(&device)
            }))
            .unwrap_or_else(|_| Err(anyhow::anyhow!("ort panicó al cargar el reranker")))
        }))
    } else {
        None
    };

    // Sparse: se instala en su punto original (mismo log y misma degradación).
    if let Some(handle) = sparse_handle {
        match await_engine_load(handle).await {
            EngineLoadOutcome::Loaded(engine) => {
                silva.install_sparse_engine(std::sync::Arc::new(engine));
                info!("🧠 Hybrid sparse source: enabled (BGE-M3 sparse fused as 4th RRF signal)");
            }
            EngineLoadOutcome::Failed(e) => {
                warn!("⚠️ SparseEngine failed to load (non-fatal, sparse source disabled): {e}");
            }
        }
    }

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
        "6 herramientas soberanas: tylluan_do (ejecuta cualquier tarea via guilds), ",
        "tylluan_remember (guarda en memoria a largo plazo SilvaDB), ",
        "tylluan_recall (busca en memoria con BGE-M3 hybrid BM25+vector RRF, 82% Recall@5), ",
        "tylluan_think (razonamiento profundo PageRank + GraphRAG), ",
        "tylluan_graph (inspecciona y manipula el grafo SilvaDB), ",
        "tylluan_decide (responde una pregunta acotada y tipada con respuesta de conjunto cerrado o abstencion; nunca una puerta de permisos). ",
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
            "tools": ["tylluan_do", "tylluan_remember", "tylluan_recall", "tylluan_think", "tylluan_graph", "tylluan_decide"]
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

    // T-DSH-55: el motor dense ya se estaba cargando en paralelo; aquí solo se
    // espera y se instala (pre-embebe las descripciones de guilds, el mismo
    // trabajo que hacía `load_model_with_device` pero sin recargar el modelo).
    // El camino de degradación es el de antes: error -> log y BM25-only.
    if let Some(handle) = dense_handle {
        match await_engine_load(handle).await {
            EngineLoadOutcome::Loaded(engine) => {
                if let Err(e) = matcher.install_engine(engine, None) {
                    error!("❌ Embedding model '{}' failed to load: {e:#} — continuing BM25-only (semantic search disabled)", dense_model_name);
                }
            }
            EngineLoadOutcome::Failed(e) => {
                error!("❌ Embedding model '{}' failed to load: {e} — continuing BM25-only (semantic search disabled)", dense_model_name);
            }
        }
    }
    let matcher = matcher.with_curriculum(curriculum.clone());
    // Create shared HormoneSystem before Arc-wrapping so both matcher and server share the same instance
    let hormones_shared = Arc::new(std::sync::Mutex::new(HormoneSystem::new()));
    let matcher = matcher.with_hormones(hormones_shared.clone());
    let matcher = Arc::new(matcher);

    // Share the dense engine handle with SilvaDB for the recall cascade
    // (stage 2 embeds queries itself; re-installed after background hot-swap).
    if let Some(eng) = matcher.engine_arc() {
        silva.install_dense_engine(eng);
    }
    // T1277-D: declare the engine fingerprint every stored embedding row is
    // judged against (observation mode by default — see [silva]
    // embedding_fingerprint_filter).
    silva.set_expected_embedding_from_dense_engine();
    silva.set_fingerprint_filter_enabled(config.silva.embedding_fingerprint_filter);

    // T206 (2026-08-22) wrapped this in catch_unwind believing it made the boot
    // panic-proof when libonnxruntime is missing. Verified WRONG 2026-08-23 by
    // G2's boot-smoke-no-onnx CI job on its first real run: this workspace's
    // release profile sets `panic = "abort"` (Cargo.toml), and under that
    // strategy catch_unwind is a documented no-op -- there is no unwind to
    // catch, so it doesn't even try. `cargo test`/`cargo check` never caught
    // this because the debug profile defaults to `panic = "unwind"`; only a
    // real `--release` boot exercises the code path that actually fails.
    // Researched real alternatives before fixing (Rustonomicon on unwinding;
    // users.rust-lang.org "Catching panic at FFI boundary, iff unwinding
    // enabled"; ort's own linking docs on ORT_DYLIB_PATH): the only reliable
    // fix under panic=abort is to never let `ort` panic in the first place --
    // probe with a real Result before ever calling into `ort`. The first
    // version only checked LOADABILITY (onnx_runtime_available), which proved
    // insufficient on 2026-10-12: a runner's PATH carried a loadable but
    // ancient ONNX Runtime 1.5.2, which passes the loadability probe and
    // then aborts inside ort::api()'s version check. The gate is now
    // onnx_runtime_ort_compatible(), which mirrors every check ort::api()
    // makes (symbol → version → GetApi) and returns false instead of
    // panicking -- check-by-check mirror in router/embeddings.rs.
    // T-DSH-55: el reranker ya se estaba cargando en paralelo (con el mismo
    // catch_unwind dentro de la tarea). Aquí solo se espera: si estaba
    // deshabilitado no hay handle y se conserva el warn original; si falló,
    // el mismo mensaje de fallback a RRF de antes.
    let reranker = if !rerank_enabled {
        warn!("⚠️ Reranker: ONNX Runtime dylib missing or incompatible with the pinned ort (checked before calling into ort) -- fallback a RRF puro");
        None
    } else if let Some(handle) = rerank_handle {
        match await_engine_load(handle).await {
            EngineLoadOutcome::Loaded(r) => {
                info!("🔀 Reranker BGE listo");
                Some(Arc::new(r))
            }
            EngineLoadOutcome::Failed(e) => {
                warn!("⚠️ Reranker no disponible (fallback a RRF puro): {e}");
                None
            }
        }
    } else {
        None
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
                                                match tokio::task::block_in_place(|| engine.embed(&text)) {
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
    ).with_guild_timeouts(config.timeouts.clone());

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
    // T-DSH-16: fuente única de verdad — la lista vive en
    // `registry::guild_process::CPU_INFERENCE_GUILDS` y este crate (binario) la
    // consume por el acceso público; antes había una copia local que podía
    // divergir del registry (deuda declarada en D-19).
    let cpu_inference_guilds = tylluan_kernel::registry::guild_process::cpu_inference_guilds();
    for (name, legacy_module, _) in LAZY_GUILDS.iter().copied() {
        if !registry_raw.guilds.contains_key(name) {
            // Hallazgo MD-3 (2026-09-30): resolver el module path desde el
            // catálogo (que deriva module_path del layout real en disco en cada
            // arranque) en vez de confiar en la ruta hardcodeada de
            // LAZY_GUILDS. Siete entradas seguían apuntando a guilds.core.<name>
            // tras migrar los plugins a los gremios, así que el registro
            // pre-v2 ganaba la carrera de "primera registración gana" sobre
            // [guilds.v2] y todo spawn perezoso moría con "MCP handshake
            // FAILED" (reproducido aislado por el harness MD-3; enmascarado en
            // producción por el registry.json persistido). Mismo fallback que
            // usa el loop always_on de abajo.
            let module = catalog.iter()
                .find(|d| d.name == name)
                .map(|d| d.module_path.clone())
                .unwrap_or_else(|| legacy_module.to_string());
            // cpu_inference_guilds explicitly get None (unlimited); others get system default
            registry_raw.register(name, &module, false, None);
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
        let discoveries = load_guild_config(&config);
        let legacy_path = &v2_config.legacy_fallback;
        for gremioguilds in &v2_config.gremios {
            let base_path = &gremioguilds.path;
            let discovery = discoveries.iter().find(|d| d.name == gremioguilds.name);
            if let Some(disc) = discovery
                && !disc.guild_md_exists {
                    warn!("⚠️ [V2] Gremio '{}' is missing guild.md specification at {}", gremioguilds.name, base_path);
                }

            // Hallazgo MD-3 (2026-09-30): la existencia del plugin se
            // comprobaba contra una ruta RELATIVA al CWD del proceso, así
            // que cualquier kernel lanzado desde otro directorio (kernel de
            // test con tylluan.toml propio, patrón T4; servicio; acceso
            // directo) descartaba TODOS los plugins v2 con "declared in TOML
            // but missing on disk" y sus spawns morían. Anclar al directorio
            // que realmente contiene el árbol guilds/ (en producción es el
            // mismo workspace root; en kernels de test hay que subir).
            let mut plugins_root = workspace_root.clone();
            if !plugins_root.join("guilds").exists() {
                for _ in 0..5 {
                    match plugins_root.parent() {
                        Some(p) => plugins_root = p.to_path_buf(),
                        None => break,
                    }
                    if plugins_root.join("guilds").exists() { break; }
                }
            }
            let mut registered_count = 0;
            for plugin in &gremioguilds.plugins {
                let guild_name = plugin.trim_end_matches(".py");
                let plugin_file = plugins_root.join(base_path).join(plugin);
                if !plugin_file.exists() {
                    warn!("⚠️ [V2] Guild plugin '{}' declared in TOML but missing on disk at {} -- skipping", guild_name, plugin_file.display());
                    continue;
                }

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
                registered_count += 1;
            }
            info!("📦 [V2] Loaded gremio '{}' from {} with {}/{} verified plugins, {} agents",
                  gremioguilds.name, base_path, registered_count, gremioguilds.plugins.len(), gremioguilds.agents.len());
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

    // WS5 — System State Snapshot: one canonical, hash-addressed identity of
    // the running system, computed from LIVE surfaces (loaded config + post-
    // discovery registry + Silva schema version) before anything can observe
    // the process. Exposed in /health (verbose) and stamped into every
    // guild_audit_log row — benchmark/audit ambiguity about "what system
    // produced this result" resolves by citing snapshot ID, not memory.
    let silva_schema_version = std::path::Path::new(&config.silva.db_path)
        .exists()
        .then(|| tylluan_kernel::config::open_db(std::path::Path::new(&config.silva.db_path)).ok())
        .flatten()
        .and_then(|conn| conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i32>(0)).ok());
    tylluan_kernel::router::system_snapshot::set_global(
        tylluan_kernel::router::system_snapshot::SystemSnapshot::compute(
            &config,
            Some(&registry_raw),
            silva_schema_version,
        ),
    );
    if let Some(snap) = tylluan_kernel::router::system_snapshot::global() {
        info!("📸 System snapshot {} (schema {:?}, {} guilds in capability surface)",
            snap.id(), snap.schema_version, registry_raw.guilds.len());
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
    server.recall_cascade_enabled = config.silva.cascade_enabled;
    server.coherence_gate_hybrid_enabled = config.security.coherence_gate_hybrid_enabled;
    server.deep_eval_enabled = config.eval.deep_eval_enabled;
    server.deep_eval_interval_hours = config.eval.deep_eval_interval_hours;
    server.slm_society_eval_enabled = config.eval.slm_society_eval_enabled;
    server.slm_society_eval_interval_hours = config.eval.slm_society_eval_interval_hours;
    // Honest abstention floor (opt-in): scaled x1000 for the atomic.
    silva.set_abstain_floor_x1000((config.silva.recall_abstain_min_score * 1000.0) as i64);
    // MLP scorer: optional ONNX model for learned complexity scoring
    let models_dir = data_dir.join("models");
    std::fs::create_dir_all(&models_dir).ok();
    let mlp_scorer = tylluan_kernel::mlp::MlpScorer::new(&models_dir);
    server.set_mlp(mlp_scorer, data_dir);
    server.expose_guild_tools = config.nexus.expose_guild_tools;
    server.set_decision_config(&config);

    // Wire crash-safe journal from HttpState into TylluanServer
    let journal_path = "./data/journal.db";
    if std::path::Path::new(journal_path).exists() {
        match crate::transport::http::api_v1::api_journal::JournalDb::open(journal_path) {
            Ok(j) => server.journal = Some(Arc::new(j)),
            Err(e) => tracing::warn!("⚠️ Failed to open journal.db: {}", e),
        }
    }

    // Shared budget for heavy background loops (reindexer, HNSW rebuild,
    // memory consensus): max concurrency + bounded wait (latency budget).
    // Prevents the 2026-08-30 GraphRAG failure class — unbounded background
    // jobs stacking and saturating CPU system-wide.
    // Created BEFORE the server is wired: the server gets a read-only handle
    // for the Scheduler's observation path, the loops below clone the same Arc.
    let background_budget = std::sync::Arc::new(
        tylluan_kernel::memory::background_budget::BackgroundBudget::new(
            config.memory.background_concurrency,
            config.memory.background_max_wait_secs,
        )
    );
    server.set_background_budget(background_budget.clone());

    // M31-P6: Wire JobQueue into TylluanServer and spawn background worker
    server.set_jobs(jobs.clone());
    if let Some(state) = crate::transport::server::background_jobs::BackgroundWorkerState::from_server(&server) {
        crate::transport::server::background_jobs::spawn_background_worker(state);
    }

    let server_arc = Arc::new(RwLock::new(server));
    let coloquio_for_fly = coloquio.clone(); // reserve clone for coloquio→SilvaDB flywheel

    // ─── HTTP Server FIRST — before guilds — for <2s /health ─────────
    // T999: reserved before the use_http block, which moves `coloquio`.
    let coloquio_for_version_check = coloquio.clone();
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
            if let Err(e) = transport::http::start_http_server_with_download(&host, port, config.nexus.port_retry_secs, http_token.clone(), config.nexus.dev_mode, Some(server_clone), registry_for_http.clone(), download_clone, tunnel_wsl_url, coloquio.clone(), jobs_for_http, cancel_token_clone, health_for_http, node_identity.clone()).await {
                error!("❌ Universal Gateway: HTTP server error: {}", e);
            }
        });
    }

    // ─── Startup version check (T999) — fire-and-forget, after HTTP ────
    // Checks the llama.cpp pin against the latest GitHub release and posts
    // to Coloquio ONLY on mismatch. Non-blocking, 10s timeout, silent on
    // any failure (no network / rate-limit / missing marker). Never
    // auto-updates — pin changes are a human decision.
    {
        tokio::spawn(tylluan_kernel::version_check::run_version_check(coloquio_for_version_check));
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
        let bg_silva = silva.clone();
        let bg_model = model_name.clone();
        let bg_device = config.inference.device.clone();
        let bg_model_inner = bg_model.clone();
        let hot_swap_target = bg_matcher.clone();
        tokio::spawn(async move {
            info!("📥 [Background] Downloading embedding model '{}'...", bg_model);
            let result = tokio::task::spawn_blocking(move || {
                hot_swap_target.hot_swap_engine(&bg_model_inner, &bg_device)
            }).await;
            match result {
                Ok(Ok(())) => {
                    info!("✅ [Background] Model '{}' loaded — hot-switched to semantic search.", bg_model);
                    // Keep the cascade's dense handle in sync with the hot-swap.
                    if let Some(eng) = bg_matcher.engine_arc() {
                        bg_silva.install_dense_engine(eng);
                    }
                    // T1277-D: refresh the expected fingerprint so the gate
                    // judges rows against the NEW engine after the hot-swap.
                    bg_silva.set_expected_embedding_from_dense_engine();
                    // Cache correction: with the query-embedding cache wired at
                    // the search_hybrid choke point (ff205b1), every recall path
                    // now caches — a stale cache after model swap would serve
                    // old-model vectors to ALL callers, not just handler_recall.
                    bg_silva.query_embed_cache.invalidate();
                }
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

    // DEPRECATED: decay_half_life_hours no longer affects real decay — FSRS-5
    // per-node stability governs retrievability (decay.rs). Kept for config
    // compatibility; warn once at startup if a non-default value is set.
    if config.silva.decay_half_life_hours != tylluan_kernel::config::default_decay_half_life_hours() {
        warn!(
            "⚠️ [Startup] config `silva.decay_half_life_hours` = {} is IGNORED: memory decay is FSRS-driven (per-node stability). Remove the key from tylluan.toml.",
            config.silva.decay_half_life_hours
        );
    }
    let _reaper = lifecycle::start_lifecycle_reaper_with_silva(
        registry_arc.clone(),
        60,
        Some(silva.clone()),
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
                                .and_then(|e| e.embed(&tagged_text).ok()
                                    .map(|emb| (emb, e.engine_id())));
                            // F1 ROADMAP_O3:58: dual-write híbrido cerrado — el job escribe solo en
                            // SilvaDB (nodo vía record_memory + embedding aquí).
                            if let Some((emb, model_id)) = embedding {
                                let _ = silva_clone.save_embedding(&node_id, &emb, &model_id, None).await;
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
        let warm_start_enabled = config.silva.embed_warm_start;
        let budget_warm = background_budget.clone();
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
                        // ADR-017 F1: LRU warm-start de las descripciones de
                        // guild del catálogo (config-gated, detrás del
                        // background budget): puebla el LRU del engine y el
                        // query_embed_cache para que el fallback semántico de
                        // tylluan_do no pague ONNX en caliente. Skip-if-busy:
                        // si otro job pesado arrancó antes, el warm cede su
                        // turno (mismo patrón que reindexer/HNSW/consensus).
                        if warm_start_enabled {
                            let Some(_bg) = budget_warm.acquire().await else {
                                warn!("⚠️ Embed warm-start skipped: background budget busy at boot");
                                return;
                            };
                            let catalog = tylluan_kernel::router::catalog::builtin_catalog();
                            let mut warmed = 0usize;
                            for g in &catalog {
                                let text = format!("{}: {}", g.name, g.description);
                                if tokio::task::block_in_place(|| engine.embed(&text)).is_ok() {
                                    warmed += 1;
                                }
                                let key = text.clone();
                                let text_inner = text.clone();
                                let engine_q = engine.clone();
                                let _ = tokio::task::block_in_place(|| {
                                    silva_warmup
                                        .query_embed_cache
                                        .get_or_embed(&key, move |_| Ok(engine_q.embed(&text_inner).unwrap_or_default()))
                                });
                            }
                            info!("🌱 Embed warm-start: {}/{} guild descriptions embedded", warmed, catalog.len());
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
                silva_inner.apply_decay().await.map(|_| ())?;
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
    let budget_reindex = background_budget.clone();
    tokio::spawn(async move {
        // tokio::time::interval fires its FIRST tick immediately by default --
        // this reindexer would then compete for the shared embedding model
        // (Mutex-protected ONNX session) with real recall traffic right at
        // the moment a fresh kernel is most likely being tested. Measured
        // live (2026-09-07 CPU-only latency baseline): recall p50 across the
        // first ~24 calls after boot was >10s (reindex serializing against
        // real embeds) vs 639ms once past that window. interval_at delays
        // the first fire by the same 600s cadence, same pattern already used
        // for Retrolink Orphans below (10s startup delay).
        let mut reindex_interval = tokio::time::interval_at(
            tokio::time::Instant::now() + Duration::from_secs(600),
            Duration::from_secs(600),
        ); // 10 minutes for toaster-friendly
        loop {
            reindex_interval.tick().await;
            // Latency budget: skip this tick if the background budget is
            // exhausted (another heavy job holding permits > wait budget).
            let Some(_bg) = budget_reindex.acquire().await else { continue };

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
                            // Coalesced entry: when `embed_batching_enabled` is
                            // on, these texts join the shared EmbedBatcher
                            // window with concurrent recall traffic — ONE ONNX
                            // batch for both instead of the reindexer
                            // serializing against recall on the model Mutex
                            // (the contamination measured in the A/B baseline).
                            // When the flag is off it delegates to the
                            // blocking-pool batch (`embed_batch_async`), never
                            // the sync `embed_batch`: 2-8s of ONNX on the async
                            // worker would starve the runtime (live HTTP hang
                            // 2026-09-01 — even /health stopped answering).
                            match engine.embed_batch_coalesced_async(texts.clone()).await {
                                Ok(vectors) => {
                                    for (vector, (nid, mid, mhash_str)) in vectors.into_iter().zip(nodes.drain(..)) {
                                        let mhash_ref = if mhash_str.is_empty() { None } else { Some(mhash_str.as_str()) };
                                        // Await each write in sequence (T842-i):
                                        // the round must only close AFTER its
                                        // writes landed — the old fire-and-forget
                                        // tokio::spawn lost races against
                                        // shutdown/tick and left nodes stale
                                        // forever (the stuck 98/196 counter).
                                        if let Err(e) = silva_inner.save_embedding(&nid, &vector, &mid, mhash_ref).await {
                                            warn!("Re-index save failed for {}: {:?}", nid, e);
                                        }
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
    let budget_hnsw = background_budget.clone();
    tokio::spawn(async move {
        let mut hnsw_interval = tokio::time::interval(Duration::from_secs(600));
        loop {
            hnsw_interval.tick().await;
            let Some(_bg) = budget_hnsw.acquire().await else { continue };
            let silva_inner = silva_hnsw.clone();
            let guard = GuardedTask::new("HNSW Rebuild", Duration::from_secs(120));
            let _ = guard.run(async move {
                silva_inner.rebuild_hnsw_if_needed().await?;
                Ok::<(), anyhow::Error>(())
            }).await;
        }
    });

    // F3 — HybridMemory → SilvaDB one-way content migration (ROADMAP_O3).
    // First fire deferred 600s (same reasoning as the reindexer: a fresh
    // kernel must serve first recall quietly), budget-gated, GuardedTask-
    // protected, idempotent (silva_kv flag + per-doc `hybrid:` skip) so a
    // pass aborted by the guard resumes on a later tick. Deliberately
    // ONNX-free: dense BLOBs are copied byte-identically and the sparse
    // signature backfills through the Agnostic Reindexer afterwards.
    // F4: HybridMemory is no longer a kernel-wide handle — the job reopens
    // the legacy DB itself, and only when one actually exists on disk
    // (fresh installs must not get a brand-new empty tylluan.db created
    // just to migrate nothing).
    let silva_migrate = silva.clone();
    let hybrid_db_path = config.memory.db_path.clone();
    let matcher_migrate = matcher.clone();
    let budget_migrate = background_budget.clone();
    tokio::spawn(async move {
        let mut migrate_interval = tokio::time::interval_at(
            tokio::time::Instant::now() + Duration::from_secs(600),
            Duration::from_secs(600),
        );
        loop {
            migrate_interval.tick().await;
            if tylluan_kernel::memory::hybrid_migration::migration_complete(&silva_migrate).await {
                return;
            }
            if !std::path::Path::new(&hybrid_db_path).exists() {
                return; // no legacy DB → nothing to migrate (stay idempotent)
            }
            let legacy = match HybridMemory::open(&hybrid_db_path) {
                Ok(m) => Arc::new(m),
                Err(e) => {
                    warn!("F3 legacy HybridMemory open failed ({}): {e:?}", hybrid_db_path);
                    return; // corrupt/locked legacy DB: don't spam every tick
                }
            };
            if let Err(e) = legacy.init().await {
                warn!("F3 legacy HybridMemory init failed: {e:?}");
                return;
            }
            let Some(_bg) = budget_migrate.acquire().await else { continue };
            let silva_inner = silva_migrate.clone();
            let memory_inner = legacy.clone();
            let model_name = matcher_migrate
                .engine_arc()
                .map(|e| e.engine_id())
                .unwrap_or_else(|| "unknown-engine".to_string());
            let guard = GuardedTask::new("F3 HYBRID→SILVA MIGRATION", Duration::from_secs(600));
            let result = guard
                .run(async move {
                    let report = tylluan_kernel::memory::hybrid_migration::migrate_hybrid_to_silva(
                        &silva_inner,
                        &memory_inner,
                        &model_name,
                    )
                    .await?;
                    Ok::<_, anyhow::Error>(report)
                })
                .await;
            match result {
                Ok(report) if report.failed == 0 => return, // flag written
                Ok(_) => {}                                 // partial pass → retry next tick
                Err(e) => warn!("F3 hybrid→silva migration pass failed: {e:?}"),
            }
        }
    });

    // Collective memory consensus scheduler (runs every 1 hour)
    // Optimized: Uses 60s tick instead of 1s to save CPU on toaster hardware
    let silva_consensus = silva.clone();
    let matcher_consensus = matcher.clone();
    let budget_consensus = background_budget.clone();
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
                let Some(_bg) = budget_consensus.acquire().await else { continue };
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
        config.night.clone(),
    ));


    // ─── P1: Periodic SQLite Maintenance ───────────────────────────
    // Performs PRAGMA wal_checkpoint(TRUNCATE) every 5 minutes for lean storage.
    let maint_silva = silva.clone();
    let maint_mailbox = mailbox.clone();
    let budget_maint = background_budget.clone();
    tokio::spawn(async move {
        let mut maint_interval = tokio::time::interval(Duration::from_secs(300));
        loop {
            maint_interval.tick().await;
            // Latency budget: skip this tick if the background budget is
            // exhausted (same skip-if-busy pattern as reindexer/HNSW/consensus
            // above). WAL checkpoint can block on busy connections; it must
            // never queue unboundedly behind another heavy job.
            let Some(_bg) = budget_maint.acquire().await else { continue };
            info!("🧹 Running periodic SQLite maintenance (Checkpoints)...");
            
            if let Err(e) = maint_silva.checkpoint().await {
                warn!("⚠️ SilvaDB checkpoint failed: {}", e);
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

    // 3. Clean up PID file (anti-orphan) — resolve_pid_data_dir() is the single
    // owner of this resolution, shared with boot (which now sees the FINAL
    // effective port incl. --config/--port/stdio), so a test instance always
    // removes its own PID file, never a sibling's.
    let pid_file = resolve_pid_data_dir(if use_stdio { None } else { Some(config.nexus.port) })
        .join("tylluan-nexus.pid");
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
    night_config: tylluan_kernel::config::NightConfig,
) {
    use tylluan_kernel::memory::night::{
        PhaseOrchestrator, PhaseContext,
        DreamPhase, OuroborosPhase, AutoLinkPhase, GraphRagPhase,
        DecayPhase, AgentPhase, CurriculumPhase, IdleLabPhase, FeedbackSignalPhase,
        LifecyclePhase, DeepEvalPhase, SlmSocietyPhase,
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
        Box::new(LifecyclePhase),
        // J-14: opt-in (default OFF via [eval] deep_eval_enabled). The phase
        // self-gates on the flag and a 24h marker, so registering it is safe
        // even with eval disabled.
        Box::new(DeepEvalPhase),
        // Phase 0: opt-in (default OFF via [eval] slm_society_eval_enabled).
        // Evaluates 3-arm SLM society benchmark (Arm A/B/C) on a 24h cadence.
        Box::new(SlmSocietyPhase),
    ], night_config.max_parallel_phases);

    let ctx = PhaseContext {
        silva,
        agent_profiles,
        curriculum,
        server,
        data_dir,
        matcher,
    };

    // .max(1): a configured 0 would make tokio::time::interval panic on a
    // zero-duration interval, silently killing this loop for the process
    // lifetime — same guard as max_parallel_override.max(1) applied above.
    let mut interval = tokio::time::interval(Duration::from_secs(night_config.interval_secs.max(1)));
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


#[cfg(test)]
mod anti_orphan_tests {
    use super::*;

    fn decide_with(
        my_port: Option<u16>,
        file: Option<&str>,
        alive: &[u32],
        ports: &[(u32, Option<u16>)],
    ) -> (PeerDecision, Option<u32>) {
        decide_pid_action(
            my_port,
            move || file.map(|s| s.to_string()),
            move |pid| alive.contains(&pid),
            move |pid| ports.iter().find(|(p, _)| *p == pid).and_then(|(_, port)| *port),
        )
    }

    #[test]
    fn missing_file_is_noop() {
        assert_eq!(decide_with(Some(47004), None, &[], &[]), (PeerDecision::NoFile, None));
    }

    #[test]
    fn empty_and_garbage_files_are_noop() {
        assert_eq!(decide_with(Some(47004), Some("   "), &[], &[]), (PeerDecision::NoFile, None));
        assert_eq!(decide_with(Some(47004), Some("not-a-pid"), &[], &[]), (PeerDecision::NoFile, None));
    }

    #[test]
    fn dead_pid_is_stale_and_cleanup_runs() {
        assert_eq!(
            decide_with(Some(47004), Some("1234"), &[], &[]),
            (PeerDecision::Stale, Some(1234))
        );
    }

    #[test]
    fn live_sibling_same_port_is_sameport_kill() {
        assert_eq!(
            decide_with(Some(47004), Some("1111"), &[1111], &[(1111, Some(47004))]),
            (PeerDecision::SamePort, Some(1111))
        );
    }

    #[test]
    fn incident_t1089_other_port_peer_is_untouched() {
        // Exact T1089 shape: test kernel with port 47009 sees prod's PID file
        // pointing at prod (47004) -> peer is live on a different port, so it
        // must not be killed and the file must not be overwritten.
        assert_eq!(
            decide_with(Some(47009), Some("37052"), &[37052], &[(37052, Some(47004))]),
            (PeerDecision::OtherPort, None)
        );
    }

    #[test]
    fn incident_t1089_live_peer_undeterminable_port_is_fail_closed() {
        // The 2026-10-11 --version case shape: we cannot read the peer's port.
        assert_eq!(
            decide_with(Some(47009), Some("37052"), &[37052], &[(37052, None)]),
            (PeerDecision::UnknownPeer, None)
        );
    }

    #[test]
    fn our_port_unknown_is_fail_closed() {
        assert_eq!(
            decide_with(None, Some("37052"), &[37052], &[(37052, Some(47004))]),
            (PeerDecision::UnknownPeer, None)
        );
    }

    #[test]
    fn info_only_invocations_print_and_never_boot() {
        let a = |argv: &[&str]| argv.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let v = early_info_output(&a(&["tylluan-nexus", "--version"])).expect("--version prints");
        assert!(v.contains(env!("CARGO_PKG_VERSION")));
        assert!(early_info_output(&a(&["tylluan-nexus", "-V"])).is_some());
        let h = early_info_output(&a(&["tylluan-nexus", "--help"])).expect("--help prints");
        assert!(h.contains("USAGE"));
        assert!(early_info_output(&a(&["tylluan-nexus", "-h"])).is_some());
        // A normal boot, a --config value or the program name alone must not match.
        assert!(early_info_output(&a(&["tylluan-nexus"])).is_none());
        assert!(early_info_output(&a(&["tylluan-nexus", "--config", "x.toml"])).is_none());
        assert!(early_info_output(&a(&["--version"])).is_none(), "argv[0] is the program name");
    }

    #[test]
    fn invocations_that_never_bind_the_port_are_gated_off() {
        let yes = |argv: &[&str]| invocations_boot_server(
            &argv.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert!(yes(&["tylluan-nexus"]));
        assert!(!yes(&["tylluan-nexus", "--version"]));
        assert!(!yes(&["tylluan-nexus", "--help"]));
        assert!(!yes(&["tylluan-nexus", "-h"]));
        assert!(!yes(&["tylluan-nexus", "--stdio"]));
        assert!(!yes(&["tylluan-nexus", "--export", "out.tar.gz"]));
        assert!(!yes(&["tylluan-nexus", "--import", "in.tar.gz"]));
        assert!(!yes(&["tylluan-nexus", "--download-models"]));
    }

    #[test]
    fn netstat_listen_lines_parse_ports_per_pid() {
        let netstat_out = concat!(
            "  Proto  Local Address          Foreign Address        State           PID\n",
            "  TCP    127.0.0.1:47004        0.0.0.0:0              LISTENING       37052\n",
            "  TCP    127.0.0.1:47009        0.0.0.0:0              LISTENING       43760\n",
            "  TCP    127.0.0.1:50000        127.0.0.1:47004        ESTABLISHED     43760\n",
        );
        assert_eq!(parse_listen_port_from_listing(netstat_out, 37052), Some(47004));
        assert_eq!(parse_listen_port_from_listing(netstat_out, 43760), Some(47009));
        // Established (non-LISTEN) lines must never attribute a port.
        assert_eq!(
            parse_listen_port_from_listing(
                "  TCP    127.0.0.1:50000        127.0.0.1:47004        ESTABLISHED     43760\n",
                43760,
            ),
            None,
        );
        assert_eq!(parse_listen_port_from_listing(netstat_out, 99999), None);
    }
}

#[cfg(test)]
mod onnx_probe_tests {
    use tylluan_kernel::router::embeddings::{onnx_runtime_available, onnx_runtime_ort_compatible};
    use std::sync::{Mutex, MutexGuard, OnceLock};

    // ORT_DYLIB_PATH is process-global env state -- serialize these tests
    // so they can't interleave and clobber each other's override.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    // The ORT_DYLIB_PATH this test PROCESS inherited (User/Machine env --
    // on a dev box that's the machine's real onnxruntime, e.g. an
    // ORT_DYLIB_PATH pointing at a current build; on CI it's absent).
    // Captured ONCE, before any mutation, by the first test to take ENV_LOCK,
    // and restored after every mutating test below: otherwise test ORDER
    // would leak state (a mutator ending in remove_var strips the inherited
    // value for every test that runs after it, including the machine-truth
    // positive test at the bottom, which must see EXACTLY what prod sees).
    static INHERITED: OnceLock<Option<String>> = OnceLock::new();

    fn lock() -> MutexGuard<'static, ()> {
        let guard = ENV_LOCK.lock().unwrap();
        let _ = INHERITED.get_or_init(|| std::env::var("ORT_DYLIB_PATH").ok());
        guard
    }

    fn restore() {
        match INHERITED.get().expect("lock() snapshots INHERITED on first acquisition").as_ref() {
            Some(v) => unsafe { std::env::set_var("ORT_DYLIB_PATH", v) },
            None => unsafe { std::env::remove_var("ORT_DYLIB_PATH") },
        }
    }

    #[test]
    fn missing_library_path_is_reported_unavailable() {
        let _guard = lock();
        unsafe { std::env::set_var("ORT_DYLIB_PATH", "/definitely/does/not/exist/libonnxruntime.so"); }
        assert!(!onnx_runtime_available(), "a nonexistent ORT_DYLIB_PATH must report unavailable, never panic");
        restore();
    }

    #[test]
    fn loadable_system_library_is_reported_available() {
        let _guard = lock();
        // A real, always-present shared library on every platform this
        // project targets, used only to prove the *positive* path of the
        // probe (a real loadable library) without depending on ONNX
        // Runtime actually being installed on the machine running this
        // test -- CI's own docs-* and boot-smoke jobs deliberately don't
        // have it, and this test must pass there too.
        let real_lib = if cfg!(target_os = "windows") {
            "kernel32.dll"
        } else if cfg!(target_os = "macos") {
            "libSystem.dylib"
        } else {
            "libc.so.6"
        };
        unsafe { std::env::set_var("ORT_DYLIB_PATH", real_lib); }
        assert!(onnx_runtime_available(), "a real, always-present system library must report available");
        restore();
    }

    #[test]
    fn loadable_but_not_onnx_library_is_incompatible_never_panics() {
        let _guard = lock();
        // The exact shape of the 2026-10-12 failure (ONNX Runtime 1.5.2 on
        // PATH): the dylib LOADS, so a loadability-only gate waves it through,
        // but it has no usable OrtGetApiBase for the pinned ort -- ort::api()
        // would then panic, which release panic="abort" turns into a hard
        // process abort. The compat probe must return false instead, without
        // ever panicking itself. kernel32/libSystem/libc are loadable and
        // definitively NOT ONNX Runtime, on every platform we target.
        let real_lib = if cfg!(target_os = "windows") {
            "kernel32.dll"
        } else if cfg!(target_os = "macos") {
            "libSystem.dylib"
        } else {
            "libc.so.6"
        };
        unsafe { std::env::set_var("ORT_DYLIB_PATH", real_lib); }
        assert!(onnx_runtime_available(), "sanity: this library IS loadable");
        assert!(
            !onnx_runtime_ort_compatible(),
            "a loadable library without a valid ONNX Runtime API must be rejected by the compat probe"
        );
        restore();
    }

    #[test]
    fn missing_library_is_incompatible_never_panics() {
        let _guard = lock();
        unsafe { std::env::set_var("ORT_DYLIB_PATH", "/definitely/does/not/exist/libonnxruntime.so"); }
        assert!(!onnx_runtime_ort_compatible(), "a nonexistent path must be incompatible, never panic");
        restore();
    }

    #[test]
    #[ignore = "needs a REAL, current ONNX Runtime (CI has none by design) — run: cargo test -p tylluan-kernel --bin tylluan-nexus ort_compatible_with_real_ort -- --ignored"]
    fn ort_compatible_with_real_ort() {
        let _guard = lock();
        // Proves the positive path against THIS machine's real ONNX Runtime,
        // resolved exactly the way prod resolves it (ORT_DYLIB_PATH honored,
        // NOT stripped -- a first version of this test stripped it and failed
        // for that reason alone: System32's 1.17 got correctly rejected, the
        // probe verdict was intact, nothing panicked). Loadable AND passing
        // every ort::api() check (symbol, version >= ort::MINOR_VERSION, GetApi).
        // If this fails locally: either the machine's ONNX Runtime is older
        // than ort::MINOR_VERSION (the probe is RIGHT to reject it -- install
        // a current one or point ORT_DYLIB_PATH at it), or ORT_DYLIB_PATH is
        // unset and PATH has no usable ONNX Runtime, or the probe itself is
        // broken (the 2026-10-12 class bug).
        assert!(
            onnx_runtime_available(),
            "expected this machine to have a loadable ONNX Runtime (check ORT_DYLIB_PATH / PATH)"
        );
        assert!(
            onnx_runtime_ort_compatible(),
            "this machine's ONNX Runtime must pass the compat probe (verdict is logged via the 🔍 tracing lines)"
        );
        restore();
    }
}

#[cfg(test)]
mod ort_entry_contract_tests {
    // T1116/T1122 structural gate, checked PER LINE. The first version was
    // per-file and passed while SparseEngine::try_new sat unguarded, because
    // embeddings.rs defines/uses the probe elsewhere in the same file —
    // exactly the miss T1122 caught (a botched `git reset` had silently
    // dropped that guard from the diff while the report still claimed it).
    // Rule: any code line that ENTERS ort (`Session::builder` / `ort::init`
    // / the fastembed engine constructors) must have
    // `onnx_runtime_ort_compatible` somewhere between the start of its
    // enclosing `fn` and that line. `use ort::` import lines require the
    // probe somewhere in the file. Comment and string-literal lines are
    // skipped (they only mention the markers in prose/data).
    // Rationale: the 2026-10-12 phase-B boot smoke proved unit tests alone
    // miss entries — SparseEngine::try_new panicked after hf_hub and
    // panic="abort" turned it into a hard boot abort. If this fails, gate
    // the new entry (return Err/None and degrade), never weaken the check.
    use std::path::{Path, PathBuf};

    const ENTRY_MARKERS: &[&str] = &[
        "Session::builder",
        "ort::init",
        "TextEmbedding::try_new",
        "TextRerank::try_new",
        "SparseTextEmbedding::try_new",
    ];
    const FILE_MARKERS: &[&str] = &["use ort::"];

    fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_rs(&path, out);
            } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                out.push(path);
            }
        }
    }

    fn starts_fn(line: &str) -> bool {
        let t = line.trim_start();
        t.starts_with("fn ")
            || t.starts_with("pub fn ")
            || t.starts_with("async fn ")
            || t.starts_with("pub async fn ")
            || t.starts_with("pub(crate) fn ")
            || t.starts_with("unsafe fn ")
            || t.starts_with("pub unsafe fn ")
    }

    fn skipped_line(line: &str) -> bool {
        let t = line.trim_start();
        t.starts_with("//") || t.starts_with("\"") || t.starts_with("r#\"")
    }

    #[test]
    fn every_ort_entry_line_is_gated_by_the_compat_probe() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        collect_rs(&root, &mut files);
        assert!(
            files.len() > 50,
            "scan looks broken: only {} .rs files under src/",
            files.len()
        );
        let mut violations = Vec::new();
        for file in &files {
            let Ok(src) = std::fs::read_to_string(file) else {
                continue;
            };
            let rel = file.strip_prefix(&root).unwrap_or(file).display().to_string();
            let file_has_probe = src.contains("onnx_runtime_ort_compatible");
            let lines: Vec<&str> = src.split('\n').collect();
            for (i, line) in lines.iter().enumerate() {
                if skipped_line(line) {
                    continue;
                }
                if FILE_MARKERS.iter().any(|m| line.contains(*m)) {
                    if !file_has_probe {
                        violations.push(format!(
                            "{rel}:{} `use ort::` import but no onnx_runtime_ort_compatible anywhere in the file",
                            i + 1
                        ));
                    }
                    continue;
                }
                if let Some(marker) = ENTRY_MARKERS.iter().find(|m| line.contains(**m)) {
                    let boundary = (0..i).rev().find(|&j| starts_fn(lines[j]));
                    let gated = match boundary {
                        Some(j) => lines[j..=i]
                            .iter()
                            .any(|l| l.contains("onnx_runtime_ort_compatible")),
                        None => file_has_probe,
                    };
                    if !gated {
                        violations.push(format!(
                            "{rel}:{} enters ort via `{marker}` without onnx_runtime_ort_compatible between the enclosing fn start and this line",
                            i + 1
                        ));
                    }
                }
            }
        }
        assert!(
            violations.is_empty(),
            "ort entries NOT gated by the compat probe (gate them: return Err/None and degrade, never panic):\n{}",
            violations.join("\n")
        );
    }
}
