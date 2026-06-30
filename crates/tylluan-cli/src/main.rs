use clap::{Parser, Subcommand, ValueEnum, ValueHint};
use anyhow::{Result, Context};
use std::process::Command;
use std::path::PathBuf;
use sysinfo::System;

const DEFAULT_PORT: u16 = 3030;

/// Installation profile — determines which embedding model and default settings to use.
#[derive(ValueEnum, Clone, Copy, PartialEq, Debug)]
enum InstallProfile {
    /// BM25-only, no model downloads. Zero dependencies, runs on a potato.
    Portable,
    /// BGE-Small embedding (67MB, 384-dim). Good for ~200K docs on 8GB RAM.
    Clinic,
    /// BGE-M3 embedding (1.2GB, 1024-dim). Full semantic search, production grade.
    Server,
}

impl std::fmt::Display for InstallProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Portable => write!(f, "portable"),
            Self::Clinic => write!(f, "clinic"),
            Self::Server => write!(f, "server"),
        }
    }
}

#[derive(Parser)]
#[command(name = "tylluan")]
#[command(about = "Sovereign Agentic Hub CLI — Manage your TylluanNexus o3 hub", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the TylluanNexus kernel
    Start {
        /// Force headless mode (no TUI)
        #[arg(long)]
        headless: bool,
        /// Specify the hub port
        #[arg(long)]
        port: Option<u16>,
    },
    /// Stop the TylluanNexus kernel
    Stop,
    /// Check the status of the hub
    Status,
    /// Stream kernel logs
    Logs {
        /// Follow log output
        #[arg(short, long)]
        follow: bool,
    },
    /// Download missing models
    DownloadModels,
    /// Connect to a remote Tylluan instance via SSE endpoint
    Connect {
        /// Remote SSE URL (e.g. https://tylluan.example.com/sse)
        #[arg(value_hint = ValueHint::Url)]
        url: Option<String>,
        /// Host (alternative to full URL, e.g. 192.168.1.42:3030)
        #[arg(long, short)]
        host: Option<String>,
        /// Bearer token for authenticated instances
        #[arg(long, short)]
        token: Option<String>,
    },
    /// Generate a tylluan.toml for the given installation profile
    Install {
        /// Installation profile (portable|clinic|server)
        #[arg(long, value_enum)]
        profile: InstallProfile,
        /// Target directory (default: ~/.tylluan/)
        #[arg(long)]
        dir: Option<PathBuf>,
        /// Overwrite existing tylluan.toml if present
        #[arg(long)]
        force: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start { headless, port } => {
            println!("🚀 Starting TylluanNexus kernel...");
            let exe_path = find_kernel_exe()?;
            
            let mut cmd = Command::new(&exe_path);
            if headless {
                cmd.arg("--headless");
            }
            if let Some(p) = port {
                cmd.args(["--port", &p.to_string()]);
            }

            // In start mode we usually want it to be a background-ish experience 
            // if we are using the CLI to "start" it.
            let child = cmd.spawn()
                .with_context(|| format!("Failed to launch kernel at {}", exe_path.display()))?;
            
            println!("✅ Kernel started with PID: {}", child.id());
            println!("🌐 Gateway active. Use 'tylluan status' to verify health.");
        }
        Commands::Stop => {
            let mut sys = System::new();
            sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
            
            let mut found = false;
            for (pid, process) in sys.processes() {
                if process.name().to_string_lossy().contains("tylluan-nexus") {
                    println!("🛑 Stopping kernel process (PID {})...", pid);
                    process.kill();
                    found = true;
                }
            }
            if !found {
                println!("⚠️ No running TylluanNexus kernel found.");
            } else {
                println!("✅ Cleanup completed.");
            }
        }
        Commands::Status => {
            println!("🔍 Checking hub status...");
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()?;
            let url = format!("http://127.0.0.1:{}/health", DEFAULT_PORT);
            match client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let json: serde_json::Value = resp.json().await?;
                    println!("✅ Hub is OPERATIONAL (v{})", json["version"]);
                }
                _ => println!("❌ Hub is OFFLINE or unreachable (http://127.0.0.1:{})", DEFAULT_PORT),
            }
        }
        Commands::Logs { follow } => {
            let log_file = PathBuf::from("logs/kernel.log");
            if !log_file.exists() {
                println!("⚠️ No log file found at {}", log_file.display());
                return Ok(());
            }

            if follow {
                // Simplified tail -f
                let mut cmd = Command::new("powershell");
                cmd.args(["-Command", &format!("Get-Content -Path {} -Wait -Tail 20", log_file.display())]);
                cmd.spawn()?.wait()?;
            } else {
                let content = std::fs::read_to_string(&log_file)?;
                println!("{}", content);
            }
        }
        Commands::DownloadModels => {
            let exe_path = find_kernel_exe()?;
            Command::new(exe_path)
                .arg("--download-models")
                .status()?;
        }
        Commands::Connect { url, host, token } => {
            let base = resolve_url(url, host)?;
            let identity_url = format!("{}/api/v1/federation/identity", base.trim_end_matches('/'));

            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()?;

            let mut req = client.get(&identity_url);
            if let Some(ref t) = token {
                req = req.bearer_auth(t);
            }

            match req.send().await {
                Ok(resp) if resp.status().is_success() => {
                    let json: serde_json::Value = resp.json().await?;
                    println!("✅ Connected to Tylluan at {}", base);
                    println!("   Node ID:    {}", json["node_id"].as_str().unwrap_or("?"));
                    println!("   Public Key: {}", json["public_key"].as_str().unwrap_or("?"));
                    println!("   Version:    {}", json["tylluan_version"].as_str().unwrap_or("?"));
                    if let Some(addr) = json["external_address"].as_str().filter(|a| !a.is_empty()) {
                        println!("   External:   {}", addr);
                    }
                }
                Ok(resp) => {
                    anyhow::bail!("remote returned {} — check URL and auth token", resp.status());
                }
                Err(e) => {
                    anyhow::bail!("could not reach {}: {}", base, e);
                }
            }
        }
        Commands::Install { profile, dir, force } => {
            let install_dir = dir.unwrap_or_else(|| {
                let home = std::env::var_os("HOME")
                    .or_else(|| std::env::var_os("USERPROFILE"))
                    .expect("Cannot determine home directory — use --dir");
                PathBuf::from(home).join(".tylluan")
            });

            std::fs::create_dir_all(&install_dir)
                .with_context(|| format!("Failed to create directory: {}", install_dir.display()))?;

            let config_path = install_dir.join("tylluan.toml");

            if config_path.exists() && !force {
                anyhow::bail!(
                    "{} already exists. Use --force to overwrite.",
                    config_path.display()
                );
            }

            let toml = generate_config(profile);
            std::fs::write(&config_path, &toml)
                .with_context(|| format!("Failed to write {}", config_path.display()))?;

            println!("✅ tylluan.toml written to {}", config_path.display());
            println!("   Profile: {}", profile);

            match profile {
                InstallProfile::Portable => {
                    println!("   BM25-only mode. No downloads needed.");
                    println!("   Start with: tylluan start");
                }
                InstallProfile::Clinic => {
                    println!("   BGE-Small model (67MB). Download with: tylluan download-models");
                }
                InstallProfile::Server => {
                    println!("   BGE-M3 model (1.2GB). Download with: tylluan download-models");
                }
            }

            println!();
            println!("📋 Next steps:");
            println!("   cd {}", install_dir.display());
            println!("   tylluan start");
        }
    }

    Ok(())
}

/// Generate a complete tylluan.toml for the given profile as a string with human-readable comments.
fn generate_config(profile: InstallProfile) -> String {
    let (embedding_model, vector_dimensions) = match profile {
        InstallProfile::Portable => ("none", 0),
        InstallProfile::Clinic => ("bge-small", 384),
        InstallProfile::Server => ("bge-m3", 1024),
    };

    format!(
        r##"# ── TylluanNexus o3 Configuration ──────────────────────────────────
# Generated by: tylluan install --profile={profile}
# Edit this file to tune your hub instance.

# ── Core settings ───────────────────────────────────────────────────
[nexus]
host = "127.0.0.1"       # Listen address (localhost-only for security)
port = 3030               # HTTP/S gateway port
dev_mode = false          # NEVER enable in production — disables auth
transports = ["stdio", "http", "sse"]

# ── Data paths ──────────────────────────────────────────────────────
[memory]
db_path = "./data/tylluan.db"

# ── Embedding model ─────────────────────────────────────────────────
# | profile  | model      | dim  | use case                  |
# |----------|------------|------|---------------------------|
# | portable | none       | 0    | BM25-only, offline-first  |
# | clinic   | bge-small  | 384  | light semantic (~67MB)    |
# | server   | bge-m3     | 1024 | full semantic (~1.2GB)   |
embedding_model = "{embedding_model}"
vector_dimensions = {vector_dimensions}

# ── Vision (SmolVLM2) ───────────────────────────────────────────────
[vision]
model_path = "HuggingFaceTB/SmolVLM2-256M-Instruct"

# ── Timeouts (safe for CPU inference — do not lower) ────────────────
[timeouts]
system_guild_ms = 15_000
analysis_guild_ms = 60_000
heavy_guild_ms = 180_000
tool_call_secs = 3_600
handshake_secs = 120
mcp_heartbeat_ms = 8_000
lazy_timeout_secs = 300

# ── Guilds ──────────────────────────────────────────────────────────
# Always-on guilds that start with the kernel.
[guilds.core]
always_on = ["bash", "memory", "filesystem"]

# ── Monitoring ──────────────────────────────────────────────────────
# Optional: sandbox images for secure code execution.
[sandbox.default]
image = "python:3.12-slim"
memory = "512m"
timeout_secs = 60
"##
    )
}

fn resolve_url(url: Option<String>, host: Option<String>) -> Result<String> {
    if let Some(u) = url {
        // Normalise: strip /sse or /messages or trailing slash to get base
        let base = u
            .trim_end_matches('/')
            .trim_end_matches("/sse")
            .trim_end_matches("/messages")
            .trim_end_matches("/api/v1/federation/identity");
        return Ok(base.to_string());
    }
    if let Some(h) = host {
        let base = if h.contains("://") { h } else { format!("http://{}", h) };
        return Ok(base.trim_end_matches('/').to_string());
    }
    Err(anyhow::anyhow!("Provide a URL or --host"))
}

fn find_kernel_exe() -> Result<PathBuf> {
    let names = ["tylluan-nexus.exe", "tylluan-nexus"];

    // 1. Same directory as the CLI (install.sh/install.ps1 place both here)
    if let Ok(current_exe) = std::env::current_exe()
        && let Some(dir) = current_exe.parent()
    {
        for name in &names {
            let full = dir.join(name);
            if full.exists() {
                return Ok(full);
            }
        }
    }

    // 2. ~/.tylluan/bin/ (install script install path)
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        let install_dir = PathBuf::from(home).join(".tylluan").join("bin");
        for name in &names {
            let full = install_dir.join(name);
            if full.exists() {
                return Ok(full);
            }
        }
    }

    // 3. Search PATH
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            for name in &names {
                if dir.join(name).exists() {
                    return Ok(PathBuf::from(name));
                }
            }
        }
    }

    // 4. Dev/build paths
    for path in &[PathBuf::from("."), PathBuf::from("target/release"), PathBuf::from("target/debug")] {
        for name in &names {
            let full = path.join(name);
            if full.exists() {
                return Ok(full);
            }
        }
    }

    Err(anyhow::anyhow!(
        "Could not find tylluan-nexus binary.\n\
         After installation: Make sure ~/.tylluan/bin/ is in your PATH and open a NEW terminal.\n\
         Build from source: cargo build --release -p tylluan-kernel"
    ))
}
