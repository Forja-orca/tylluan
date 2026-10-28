use clap::{Parser, Subcommand, ValueEnum, ValueHint};
use anyhow::{Result, Context};
use std::process::Command;
use std::path::{Path, PathBuf};
use sysinfo::System;
use sha2::{Digest, Sha256};

mod backup;
mod stop;

use stop::StopAction;

const DEFAULT_PORT: u16 = 47004;

/// Installation profile — determines which embedding model and default settings to use.
#[derive(ValueEnum, Clone, Copy, PartialEq, Debug)]
enum InstallProfile {
    /// BM25-only, no model downloads. Zero dependencies, runs on a potato.
    Portable,
    /// BGE-Small embedding (67MB, 384-dim). Good for ~200K docs on 8GB RAM.
    Clinic,
    /// mxbai-embed-large embedding (1024-dim native). Full semantic search, production grade.
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
    /// Run benchmarks or list past results
    Eval {
        #[command(subcommand)]
        action: EvalAction,
    },
    /// Start the TylluanNexus kernel
    Start {
        /// Force headless mode (no TUI)
        #[arg(long)]
        headless: bool,
        /// Specify the hub port
        #[arg(long)]
        port: Option<u16>,
        /// Detect hardware (RAM/CPU/GPU) and generate tylluan.toml automatically
        /// if none exists in the current directory. Never edit an existing TOML.
        #[arg(long)]
        setup: bool,
    },
    /// Stop the TylluanNexus kernel (resolves the exact instance via PID file;
    /// never kills by process-name sweep)
    Stop {
        /// Port of the instance to stop (matches the kernel's --port)
        #[arg(long)]
        port: Option<u16>,
        /// Data directory of the instance to stop (matches TYLLUAN_DATA_DIR)
        #[arg(long)]
        data_dir: Option<PathBuf>,
        /// Bearer token for authenticated instances (graceful shutdown)
        #[arg(long, short)]
        token: Option<String>,
    },
    /// Check the status of the hub
    Status,
    /// Run a full diagnostic scan (guilds, storage, system resources, config)
    Doctor,
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
        /// Host (alternative to full URL, e.g. 192.168.1.42:47004)
        #[arg(long, short)]
        host: Option<String>,
        /// Bearer token for authenticated instances
        #[arg(long, short)]
        token: Option<String>,
    },
    /// Audit chain verification (tamper detection)
    Audit {
        /// Show last N audit entries (default: 10)
        #[arg(short, long, default_value = "10")]
        entries: usize,
    },
    /// Export LLM decision examples (CoherenceGate dataset) to JSONL
    ExportExamples {
        /// Output file (default: llm_decision_examples.jsonl)
        #[arg(short, long)]
        out: Option<std::path::PathBuf>,
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
        /// Write the config but do not start the kernel
        #[arg(long)]
        no_start: bool,
    },
    /// Check for updates and (solo con orden explicita) instalar el binario
    Update {
        /// Only check for updates, don't download
        #[arg(long)]
        check: bool,
        /// Instalar la release nueva (orden explicita; nunca silencioso)
        #[arg(long)]
        install: bool,
    },
    /// Create a new guild or other scaffold
    New {
        #[command(subcommand)]
        what: NewWhat,
    },
    /// Manage the sandbox capability policy (global / per-guild / per-session)
    Sandbox {
        #[command(subcommand)]
        action: SandboxCommand,
        /// Bearer token for authenticated instances (or set TYLLUAN_TOKEN)
        #[arg(long, short)]
        token: Option<String>,
    },
    /// Resume a previous agent session — retrieve the last session summary from memory
    Resume {
        /// Agent identity to resume (e.g. 'claude-code', 'qwen')
        agent_id: String,
    },
    /// Create a hot-consistent backup of the hub databases (kernel may keep running)
    Backup {
        /// Directory to write the backup into (created if missing; .db files + manifest)
        dir: PathBuf,
        /// Extra database to include (file name relative to the data dir, or a full path)
        #[arg(long)]
        db: Vec<String>,
    },
    /// Restore hub databases from a directory produced by `tylluan-cli backup`
    /// (refuses to run while the kernel is up; snapshots current state first)
    Restore {
        /// Backup directory containing backup-manifest.json + .db files
        dir: PathBuf,
    },
    /// Inference points registry (T-DSH-116): read or change a point (operator)
    Inference {
        #[command(subcommand)]
        action: InferenceAction,
        /// Port of the kernel instance (default 47004)
        #[arg(long)]
        port: Option<u16>,
        /// Bearer token for authenticated instances (or set TYLLUAN_TOKEN)
        #[arg(long, short)]
        token: Option<String>,
    },
    /// Sentinel (write_gate L2 judge) control: status / mode
    Sentinel {
        #[command(subcommand)]
        action: SentinelAction,
        /// Port of the kernel instance (default 47004)
        #[arg(long)]
        port: Option<u16>,
        /// Bearer token for authenticated instances (or set TYLLUAN_TOKEN)
        #[arg(long, short)]
        token: Option<String>,
    },
}

/// T-DSH-116: subcomandos del registro de puntos de inferencia.
#[derive(Subcommand)]
enum InferenceAction {
    /// Show the configured inference points (read; never returns key values)
    Points,
    /// Change one point (operator class; REST only, never MCP)
    SetPoint {
        /// decision | grammar_judge | sentinel | chat
        point: String,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        endpoint: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long = "api-key-env")]
        api_key_env: Option<String>,
        #[arg(long = "cloud-enabled")]
        cloud_enabled: Option<bool>,
        /// Persistir el cambio en tylluan.toml (atomico, solo esa seccion)
        #[arg(long)]
        persist: bool,
    },
}

/// T-DSH-114: subcomandos del centinela.
#[derive(Subcommand)]
enum SentinelAction {
    /// Show the sentinel state (mode, loaded, verdict counters, fallbacks, cpu_skips)
    Status,
    /// Change the sentinel mode in hot: off | observe | enforce
    Mode {
        /// off | observe | enforce
        mode: String,
    },
}

/// T-DSH-114: valida el modo del centinela (mismo criterio que kernel/REST).
fn valid_sentinel_mode(mode: &str) -> bool {
    matches!(mode.trim().to_ascii_lowercase().as_str(), "off" | "observe" | "enforce")
}

#[derive(Subcommand)]
enum SandboxCommand {
    /// Show the current effective sandbox profile and any active overrides
    Status,
    /// Set the global sandbox profile (persisted to tylluan.toml)
    Set {
        /// strict, balanced, or permissive
        profile: String,
    },
    /// Set a guild-level override (persisted to tylluan.toml)
    Guild {
        /// Target guild name
        name: String,
        /// strict, balanced, or permissive
        profile: String,
    },
    /// Set a session-level override (in-memory only, lost on kernel restart)
    Session {
        /// Target agent_id
        agent_id: String,
        /// strict, balanced, or permissive
        profile: String,
    },
    /// Remove a guild-level override, falling back to the global profile
    UnsetGuild {
        /// Target guild name
        name: String,
    },
    /// Clear a session-level override
    UnsetSession {
        /// Target agent_id
        agent_id: String,
    },
}

#[derive(Subcommand)]
enum NewWhat {
    /// Scaffold a new guild from a template
    Guild {
        /// Name of the guild (snake_case, e.g. 'my_tool')
        name: String,
    },
}

/// How long `start` keeps polling /health before declaring the kernel
/// un-ready. The kernel can legitimately take ~52s to boot when it preloads
/// its default embedding model (measured 2026-10-26, H-35: cold `start` with
/// no installer profile, Xeon E5-2680 v4). The old fixed 30s window declared
/// such kernels failed while they were still booting normally, and the CLI
/// returned as if it had given up on a healthy boot.
/// Raising the CLI-side window is safe by design: it never lowers any
/// kernel-side or guild-side timeout — polling stays 1s apart with a 2s
/// per-request HTTP timeout, so a kernel that answers is still reported
/// ready on the first successful poll.
const STARTUP_WAIT_SECS: u64 = 180;

// Compile-time guard (H-37 regression): the window must exceed the measured
// cold-preload boot of ~52s or `start` gives up on a healthy kernel again.
const _: () = assert!(STARTUP_WAIT_SECS > 60);

/// Poll `url` every second until it answers 2xx or `cap_secs` elapses.
/// Returns true on ready. Pure in its timing contract so it can be tested
/// against a port with nothing listening.
async fn wait_for_health(client: &reqwest::Client, url: &str, cap_secs: u64) -> bool {
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_secs(cap_secs);
    let mut next_report = std::time::Instant::now()
        + std::time::Duration::from_secs(15);
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        match client.get(url).send().await {
            Ok(resp) if resp.status().is_success() => return true,
            _ => {
                let now = std::time::Instant::now();
                if now >= next_report && now < deadline {
                    let remaining =
                        deadline.checked_duration_since(now).map(|d| d.as_secs());
                    if let Some(rem) = remaining {
                        println!("   … still waiting for /health ({rem}s left)");
                    }
                    next_report = now + std::time::Duration::from_secs(15);
                }
            }
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start { headless, port, setup } => {
            if setup {
                run_setup_wizard()?;
            }
            println!("🚀 Starting TylluanNexus kernel...");
            let exe_path = find_kernel_exe()?;
            
            let mut cmd = Command::new(&exe_path);
            if headless {
                cmd.arg("--headless");
            }
            if let Some(p) = port {
                cmd.args(["--port", &p.to_string()]);
            }

            let child = cmd.spawn()
                .with_context(|| format!("Failed to launch kernel at {}", exe_path.display()))?;
            
            println!("✅ Kernel started with PID: {}", child.id());

            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(2))
                .build()?;
            let port = port.unwrap_or(DEFAULT_PORT);
            let url = format!("http://127.0.0.1:{port}/health");

            if wait_for_health(&client, &url, STARTUP_WAIT_SECS).await {
                println!("✅ Kernel is ready at http://127.0.0.1:{port}");
                println!("🌐 Connect your MCP client to http://127.0.0.1:{port}/sse");
            } else {
                println!(
                    "⚠️ Kernel started but not ready within {STARTUP_WAIT_SECS}s. Check logs \
                     (a cold preload of the embedding model can legitimately exceed a minute — \
                     the kernel may still be booting; `tylluan-cli status` re-checks /health)."
                );
            }
        }
        Commands::Stop { port, data_dir, token } => {
            run_stop(port, data_dir.as_deref(), token).await?;
        }
        Commands::Status => {
            println!("🔍 Checking hub status...");
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()?;
            let url = format!("http://127.0.0.1:{DEFAULT_PORT}/health");
            match client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let json: serde_json::Value = resp.json().await?;
                    println!("✅ Hub is OPERATIONAL (v{})", json["version"]);
                }
                _ => println!("❌ Hub is OFFLINE or unreachable (http://127.0.0.1:{DEFAULT_PORT})"),
            }
        }
        Commands::Doctor => {
            println!("🩺 Tylluan Diagnostic Scan (v{})", env!("CARGO_PKG_VERSION"));
            println!("{}\n", "─".repeat(48));

            let mut all_ok = true;

            // ── 1. Binary & version ──
            print!("[1/8] Binary version: {} ... ", env!("CARGO_PKG_VERSION"));
            println!("✅");

            // ── 2. Config file ──
            let config_paths = [
                PathBuf::from("tylluan.toml"),
                std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))
                    .map(|h| PathBuf::from(h).join(".tylluan").join("tylluan.toml"))
                    .unwrap_or_default(),
            ];
            let config_ok = config_paths.iter().any(|p| p.exists() && p.is_file());
            if config_ok {
                let p = config_paths.iter().find(|p| p.exists()).unwrap();
                let raw = match std::fs::read_to_string(p) {
                    Ok(c) => c,
                    Err(e) => { println!("❌ (read error: {e})"); all_ok = false; String::new() }
                };
                if !raw.is_empty() {
                    match raw.parse::<toml::Value>() {
                        Ok(_) => println!("[2/8] Config file: {} ... ✅", p.display()),
                        Err(e) => { println!("[2/8] Config file: {} ... ❌ (invalid TOML: {})", p.display(), e); all_ok = false; }
                    }
                }
            } else {
                println!("[2/8] Config file: not found (tylluan.toml) ... ⚠️");
                println!("       Run 'tylluan install --profile portable' to create one.");
            }

            // ── 3. Python version ──
            let python_check = || -> Option<String> {
                for bin in &["python3", "python"] {
                    if let Ok(out) = Command::new(bin).arg("--version").output()
                        && out.status.success() {
                            let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
                            return Some(v);
                        }
                }
                None
            };
            match python_check() {
                Some(v) => {
                    // Check major.minor >= 3.12 (matches README/guilds/requirements.txt's
                    // published requirement -- was stale at 3.11, found during audit
                    // finding scope "unify Python version requirement", 2026-10-09).
                    let major_minor = v.split_whitespace().nth(1).unwrap_or("0.0")
                        .split('.').take(2).map(|n| n.parse::<u32>().unwrap_or(0)).collect::<Vec<_>>();
                    if major_minor.len() == 2 && major_minor[0] >= 3 && major_minor[1] >= 12 {
                        println!("[3/8] Python: {v} ... ✅");
                    } else {
                        println!("[3/8] Python: {v} ... ❌ (need 3.12+)");
                        all_ok = false;
                    }
                }
                None => {
                    println!("[3/8] Python: not found ... ❌");
                    println!("       Install Python 3.12+: https://python.org/downloads");
                    all_ok = false;
                }
            }

            // ── 4. Guilds installed ──
            let guild_dir = PathBuf::from("guilds").join("core");
            if guild_dir.exists() {
                let count = match std::fs::read_dir(&guild_dir) {
                    Ok(entries) => entries.filter_map(|e| e.ok()).filter(|e| e.path().extension().is_some_and(|x| x == "py" || x == "rs")).count(),
                    Err(_) => 0,
                };
                println!("[4/8] Guilds core: {} guilds at {} ... ✅", count, guild_dir.display());
            } else {
                println!("[4/8] Guilds core: {} not found ... ⚠️", guild_dir.display());
                println!("       Guilds will be loaded from ~/.tylluan/guilds/ at runtime.");
            }

            // ── 5. Embedding model cache ──
            let models_dir = PathBuf::from("models");
            let model_cache_ok = models_dir.exists() && std::fs::read_dir(&models_dir).is_ok_and(|mut e| e.next().is_some());
            if model_cache_ok {
                let size = models_dir_approx_size(&models_dir);
                println!("[5/8] Embedding model: cached (~{} MB) ... ✅", size / 1024 / 1024);
            } else {
                println!("[5/8] Embedding model: not cached ... ⚠️");
                println!("       BM25-only mode active. Run 'tylluan download-models' for semantic search.");
            }

            // ── 6. Port free ──
            let port_free = std::net::TcpListener::bind(("127.0.0.1", DEFAULT_PORT)).is_ok();
            if port_free {
                println!("[6/8] Port {DEFAULT_PORT}: available ... ✅");
            } else {
                println!("[6/8] Port {DEFAULT_PORT}: in use (kernel likely running) ... ✅");
            }

            // ── 7. Kernel health (online check) ──
            let kernel_running = !port_free;
            if kernel_running {
                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(5))
                    .build()?;
                let url = format!("http://127.0.0.1:{DEFAULT_PORT}/health");
                match client.get(&url).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        let json: serde_json::Value = resp.json().await?;
                        let status = json.get("status").and_then(|s| s.as_str()).unwrap_or("ok");
                        println!("[7/8] Kernel: running ({status}) ... ✅");
                        // Also fetch detailed doctor report
                        let doctor_url = format!("http://127.0.0.1:{DEFAULT_PORT}/api/v1/doctor");
                        if let Ok(resp) = client.get(&doctor_url).send().await
                            && let Ok(report) = resp.json::<serde_json::Value>().await {
                                if let Some(guilds) = report["guilds"].as_array() {
                                    let down: Vec<_> = guilds.iter()
                                        .filter(|g| g["running"].as_bool() == Some(false))
                                        .filter_map(|g| g["name"].as_str())
                                        .collect();
                                    if !down.is_empty() {
                                        println!("       Guilds DOWN: {}", down.join(", "));
                                    }
                                }
                                if let Some(suggestions) = report["suggestions"].as_array()
                                    && !suggestions.is_empty() {
                                        println!("\n   Suggestions:");
                                        for s in suggestions {
                                            if let Some(s) = s.as_str() {
                                                println!("   - {s}");
                                            }
                                        }
                                    }
                            }
                    }
                    Ok(resp) => println!("[7/8] Kernel: error {} ... ❌", resp.status()),
                    Err(_) => println!("[7/8] Kernel: unreachable despite port open ... ❌"),
                }
            } else {
                println!("[7/8] Kernel: not running ... ⚪");
                println!("       Start with 'tylluan start'");
            }

            println!("\n{}", "─".repeat(48));
            if all_ok {
                println!("✅ All checks passed — Tylluan is ready.");
            } else {
                println!("⚠️ Some checks failed — review items marked ❌ above.");
                println!("   Run 'tylluan doctor' again after resolving issues.");
            }
        }
        Commands::Inference { action, port, token } => {
            run_inference(action, port.unwrap_or(DEFAULT_PORT), token).await?;
        }
        Commands::Sentinel { action, port, token } => {
            run_sentinel(action, port.unwrap_or(DEFAULT_PORT), token).await?;
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
                println!("{content}");
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
                    println!("✅ Connected to Tylluan at {base}");
                    println!("   Node ID:    {}", json["node_id"].as_str().unwrap_or("?"));
                    println!("   Public Key: {}", json["public_key"].as_str().unwrap_or("?"));
                    println!("   Version:    {}", json["tylluan_version"].as_str().unwrap_or("?"));
                    if let Some(addr) = json["external_address"].as_str().filter(|a| !a.is_empty()) {
                        println!("   External:   {addr}");
                    }
                }
                Ok(resp) => {
                    anyhow::bail!("remote returned {} — check URL and auth token", resp.status());
                }
                Err(e) => {
                    anyhow::bail!("could not reach {base}: {e}");
                }
            }
        }
        Commands::Audit { .. } => {
            println!("🔍 Audit chain verification...");
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()?;
            let verify_url = format!("http://127.0.0.1:{DEFAULT_PORT}/api/v1/audit/verify");
            match client.get(&verify_url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let json: serde_json::Value = resp.json().await?;
                    let status = json["status"].as_str().unwrap_or("unknown");
                    let icon = match status {
                        "clean" => "✅",
                        "tampered" => "🚨",
                        _ => "⚠️",
                    };
                    println!("{icon} Chain integrity: {status}");
                    println!("   Valid entries:   {}", json["valid_count"]);
                    println!("   Tampered entries: {}", json["tampered_count"]);
                }
                Ok(resp) => println!("❌ Hub returned error status: {}", resp.status()),
                Err(_) => println!("❌ Hub is OFFLINE — start it with 'tylluan start'"),
            }
        }
        Commands::ExportExamples { out } => {
            println!("📦 Exporting LLM decision examples (CoherenceGate dataset)...");
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()?;
            let url = format!("http://127.0.0.1:{DEFAULT_PORT}/api/v1/llm-examples/export");
            match client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let stats_json = resp
                        .headers()
                        .get("X-Export-Stats")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("{}")
                        .to_string();
                    let body = resp.text().await?;
                    let path = out.unwrap_or_else(|| {
                        std::path::PathBuf::from("llm_decision_examples.jsonl")
                    });
                    std::fs::write(&path, &body)?;
                    let stats: serde_json::Value = serde_json::from_str(&stats_json).unwrap_or_default();
                    let total = stats["total"].as_u64().unwrap_or(0);
                    let train = stats["train"].as_u64().unwrap_or(0);
                    let heldout = stats["heldout"].as_u64().unwrap_or(0);
                    let agreement = stats["gate_llm_agreement"].as_f64().unwrap_or(0.0);
                    println!("✅ Exported {total} examples to {}", path.display());
                    println!("   Split:        {train} train / {heldout} heldout (determinista por node_id, sin leak)");
                    println!("   Acuerdo gate↔LLM: {:.1}%", agreement * 100.0);
                }
                Ok(resp) => println!("❌ Hub returned error status: {}", resp.status()),
                Err(_) => println!("❌ Hub is OFFLINE — start it with 'tylluan start'"),
            }
        }
        Commands::Eval { action } => match action {            EvalAction::Longmemevals { num_queries, seed } => {
                println!("🧪 Running LongMemEval-S benchmark...");
                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(300))
                    .build()?;
                let url = format!("http://127.0.0.1:{DEFAULT_PORT}/api/v1/eval/run");
                let body = serde_json::json!({
                    "benchmark": "longmemeval-s",
                    "num_queries": num_queries.unwrap_or(30),
                    "seed": seed.unwrap_or(42),
                });
                match client.post(&url).json(&body).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        let json: serde_json::Value = resp.json().await?;
                        if json["ok"] == false {
                            println!("❌ Benchmark failed: {}", json["error"]);
                        } else if let Some(r) = json["result"].as_object() {
                            let recall_1 = r["recall_at_1"].as_f64().unwrap_or(0.0);
                            let recall_5 = r["recall_at_5"].as_f64().unwrap_or(0.0);
                            let recall_10 = r["recall_at_10"].as_f64().unwrap_or(0.0);
                            let mean_lat = r["mean_latency_ms"].as_f64().unwrap_or(0.0);
                            let p95_lat = r["p95_latency_ms"].as_f64().unwrap_or(0.0);
                            let hash = r["result_hash"].as_str().unwrap_or("?");
                            let n = r["num_queries"].as_u64().unwrap_or(0);
                            println!("✅ LongMemEval-S complete ({n} queries):");
                            println!("   Recall@1:  {recall_1:.1}%");
                            println!("   Recall@5:  {recall_5:.1}%");
                            println!("   Recall@10: {recall_10:.1}%");
                            println!("   Latency:   mean={mean_lat:.0}ms p95={p95_lat:.0}ms");
                            println!("   Hash:      {hash}");
                            println!("   (Run 'tylluan eval list' to see all results)");
                        }
                    }
                    Ok(resp) => println!("❌ Hub returned error status: {}", resp.status()),
                    Err(_) => println!("❌ Hub is OFFLINE — start it with 'tylluan start'"),
                }
            }
            EvalAction::List => {
                println!("📊 Past benchmark results...");
                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(10))
                    .build()?;
                let url = format!("http://127.0.0.1:{DEFAULT_PORT}/api/v1/eval/results");
                match client.get(&url).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        let json: serde_json::Value = resp.json().await?;
                        let empty_vec = vec![];
                        let results = json["results"].as_array().unwrap_or(&empty_vec);
                        if results.is_empty() {
                            println!("   No results yet. Run 'tylluan eval longmemeval-s' first.");
                        } else {
                            for (i, r) in results.iter().enumerate() {
                                let recall_1 = r["recall_at_1"].as_f64().unwrap_or(0.0);
                                let recall_5 = r["recall_at_5"].as_f64().unwrap_or(0.0);
                                let recall_10 = r["recall_at_10"].as_f64().unwrap_or(0.0);
                                let mean_lat = r["mean_latency_ms"].as_f64().unwrap_or(0.0);
                                let hash = r["result_hash"].as_str().unwrap_or("?");
                                let n = r["num_queries"].as_u64().unwrap_or(0);
                                println!("   {}. {} ({} queries)", i + 1, r["benchmark"], n);
                                println!("      R@1={recall_1:.1}% R@5={recall_5:.1}% R@10={recall_10:.1}%  lat={mean_lat:.0}ms");
                                println!("      hash: {hash}");
                            }
                        }
                    }
                    Ok(resp) => println!("❌ Hub returned error status: {}", resp.status()),
                    Err(_) => println!("❌ Hub is OFFLINE — start it with 'tylluan start'"),
                }
            }
        },
        Commands::Install { profile, dir, force, no_start } => {
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
            println!("   Profile: {profile}");

            if profile != InstallProfile::Portable {
                let model_name = match profile {
                    InstallProfile::Clinic => "BGE-Small (67MB)",
                    _ => "mxbai-embed-large (1024d)",
                };
                println!("📥 Downloading {model_name} embedding model...");
                let exe_path = find_kernel_exe()?;
                let status = Command::new(exe_path)
                    .arg("--download-models")
                    .status()
                    .with_context(|| "Failed to run model download")?;
                if !status.success() {
                    anyhow::bail!("Model download failed (exit code: {:?})", status.code());
                }
                println!("✅ Model download complete.");
            } else {
                println!("   BM25-only mode. No downloads needed.");
            }

            if no_start {
                println!("Kernel not started (--no-start).");
                return Ok(());
            }

            println!("🚀 Starting Tylluan kernel...");
            let original_dir = std::env::current_dir()?;
            std::env::set_current_dir(&install_dir)?;

            let exe_path = find_kernel_exe()?;
            let child = Command::new(&exe_path)
                .spawn()
                .with_context(|| format!("Failed to launch kernel at {}", exe_path.display()))?;

            println!("✅ Kernel started with PID: {}", child.id());

            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(2))
                .build()?;
            let url = format!("http://127.0.0.1:{DEFAULT_PORT}/health");

            for i in 1..=30 {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                match client.get(&url).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        println!();
                        println!("🎉 Tylluan is running at http://127.0.0.1:{DEFAULT_PORT}");
                        println!();
                        println!("   Connect your MCP client:");
                        println!("     Claude Desktop -> http://127.0.0.1:{DEFAULT_PORT}/sse");
                        println!("     Claude Code   -> /mcp add tylluan sse http://127.0.0.1:{DEFAULT_PORT}/sse");
                        println!("     curl          -> curl http://127.0.0.1:{DEFAULT_PORT}/health");
                        break;
                    }
                    _ if i == 30 => {
                        println!("⚠️ Kernel started but not ready within 30s.");
                        println!("   Check logs: {}", install_dir.join("logs").join("kernel.log").display());
                    }
                    _ => {}
                }
            }

            let _ = std::env::set_current_dir(&original_dir);
        }
        Commands::Update { check, install } => {
            println!("🔍 Checking for updates...");
            let repo = "Forja-orca/tylluan";
            let current_ver = env!("CARGO_PKG_VERSION");
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .user_agent("tylluan-update/1.0")
                .build()?;

            let release_url = format!("https://api.github.com/repos/{repo}/releases/latest");
            let resp = client.get(&release_url).send().await?;

            if !resp.status().is_success() {
                println!("❌ Failed to check updates: HTTP {}", resp.status());
                return Ok(());
            }

            let release: serde_json::Value = resp.json().await?;
            let latest_tag = release["tag_name"]
                .as_str()
                .filter(|tag| !tag.trim().is_empty())
                .ok_or_else(|| anyhow::anyhow!("Latest release has no usable tag_name; refusing to update"))?;
            let latest_ver = latest_tag.trim_start_matches('v');

            match compare_versions(current_ver, latest_ver)? {
                std::cmp::Ordering::Equal => {
                    println!("✅ Tylluan v{current_ver} is up to date.");
                    return Ok(());
                }
                std::cmp::Ordering::Less => {
                    println!("✅ Tylluan v{current_ver} is already newer than v{latest_ver}; not downgrading.");
                    return Ok(());
                }
                std::cmp::Ordering::Greater => {
                    println!("📦 Update available: v{current_ver} → v{latest_ver}");
                }
            }

            match update_action(check, install) {
                "check" => {
                    println!("   Run 'tylluan update --install' para instalar (orden explicita).");
                    return Ok(());
                }
                "notice" => {
                    println!("   Aviso: hay una version nueva. NO se instala nada sin orden.");
                    println!("   Para instalar: tylluan update --install");
                    return Ok(());
                }
                _ => {} // "install": continua con la descarga
            }

            // Detect current platform.
            let target = detect_update_target();
            let archive_name = format!("tylluan-{target}.tar.gz");
            let base_url = format!("https://github.com/{repo}/releases/download/{latest_tag}");
            let download_url = format!("{base_url}/{archive_name}");
            let checksums_url = format!("{base_url}/checksums.txt");

            // E-03: the checksum must come from a different transport object than
            // the archive it authenticates. A checksum embedded in the same tar
            // only detects accidental corruption, not substitution.
            println!("🔐 Verifying release checksum from {checksums_url} ...");
            let checksums_resp = client.get(&checksums_url).send().await.with_context(|| {
                format!("Could not download checksums.txt for {latest_tag}")
            })?;
            if !checksums_resp.status().is_success() {
                anyhow::bail!(
                    "Release {latest_tag} does not publish a separate checksums.txt (HTTP {}). \
                     Refusing to update without an independent checksum.",
                    checksums_resp.status()
                );
            }
            let checksums_text = checksums_resp
                .text()
                .await
                .context("Could not read checksums.txt")?;
            let expected_hash = checksum_for_asset(&checksums_text, &archive_name)
                .ok_or_else(|| anyhow::anyhow!(
                    "checksums.txt has no entry for {archive_name}; refusing to update"
                ))?;

            println!("📦 Downloading {archive_name} ...");
            let download_resp = client.get(&download_url).send().await?;
            if !download_resp.status().is_success() {
                println!("❌ Download failed: HTTP {} — unsupported platform: {}", download_resp.status(), target);
                println!("   Manual download: https://github.com/{repo}/releases");
                return Ok(());
            }

            let bytes = download_resp.bytes().await?;
            verify_sha256(&bytes, &expected_hash)?;

            let current_exe = std::env::current_exe()?;
            let parent = current_exe.parent().unwrap_or(std::path::Path::new("."));
            let stage = parent.join(format!(".tylluan-update-{}.stage", std::process::id()));
            if stage.exists() {
                std::fs::remove_dir_all(&stage)
                    .with_context(|| format!("Could not clear stale staging dir {}", stage.display()))?;
            }
            std::fs::create_dir_all(&stage)
                .with_context(|| format!("Could not create staging dir {}", stage.display()))?;

            let result = (|| -> Result<Vec<PathBuf>> {
                let staged = extract_update_binaries(&bytes, &stage)?;
                install_staged_binaries(&staged, parent)
            })();
            let _ = std::fs::remove_dir_all(&stage);
            let installed = result?;
            for path in &installed {
                println!("✅ Installed {}", path.display());
            }
            println!("✅ Updated to Tylluan v{latest_ver}");
            println!("   Restart the kernel with 'tylluan start' for changes to take effect.");
        }
        Commands::New { what } => match what {
            NewWhat::Guild { name } => {
                let snake = name.trim().to_lowercase().replace(' ', "_");
                if !snake.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    anyhow::bail!("Guild name must be snake_case (letters, numbers, underscores only). Got: '{name}'");
                }

                let guild_dir = std::path::Path::new("guilds").join("core");
                let tests_dir = std::path::Path::new("tests").join("guilds");
                let guild_path = guild_dir.join(format!("{snake}.py"));
                let test_path = tests_dir.join(format!("test_{snake}.py"));

                if guild_path.exists() {
                    anyhow::bail!("Guild already exists at {}", guild_path.display());
                }

                std::fs::create_dir_all(&guild_dir)
                    .with_context(|| format!("Failed to create {}", guild_dir.display()))?;
                std::fs::create_dir_all(&tests_dir)
                    .with_context(|| format!("Failed to create {}", tests_dir.display()))?;

                let display_name = snake.replace('_', " ").split(' ').map(|w| {
                    let mut c = w.chars();
                    match c.next() {
                        None => String::new(),
                        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    }
                }).collect::<Vec<_>>().join(" ");

                let guild_src = format!(
                    r##"""TylluanNexus {display_name} Guild — <description>

Status: draft
"""

CAPABILITIES = {{
    "process_execution": False,
    "filesystem_scope": ["/"],
    "network_hosts": [],
}}

from mcp.server.fastmcp import FastMCP
from guilds.core import utils

mcp = FastMCP("tylluan-{snake}")


@mcp.tool()
async def my_tool(query: str) -> str:
    """Describe what this tool does.

    Args:
        query: Input parameter description.
    """
    return f"Hello from {snake}! You said: {{query}}"
"##
                );

                let test_src = format!(
                    r##"""Tests for the {snake} guild."""

import pytest
from guilds.core.{snake} import mcp


def test_{snake}_tool_registered():
    """Verify the tool is registered."""
    tools = mcp._tool_manager.list_tools()
    assert len(tools) >= 1
    names = [t.name for t in tools]
    assert "my_tool" in names
"##
                );

                std::fs::write(&guild_path, &guild_src)
                    .with_context(|| format!("Failed to write {}", guild_path.display()))?;
                std::fs::write(&test_path, &test_src)
                    .with_context(|| format!("Failed to write {}", test_path.display()))?;

                println!("✅ Created guild: {}", guild_path.display());
                println!("✅ Created test:  {}", test_path.display());
                println!();
                println!("📋 Next steps:");
                println!("   1. Edit  {}  to add your tool logic", guild_path.display());
                println!("   2. Run   pytest {}  to verify", test_path.display());
                println!("   3. Your guild auto-registers on next kernel restart");
            }
        },
        Commands::Sandbox { action, token } => {
            // T-DSH-83: same bearer pattern as `stop` (main.rs stop_instance),
            // but with the env/file fallback the operator asked for. No token
            // resolved => no Authorization header (dev_mode=true behaviour).
            let token = resolve_cli_token(token);
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()?;
            let base = format!("http://127.0.0.1:{DEFAULT_PORT}");
            match action {
                SandboxCommand::Status => {
                    let url = format!("{base}/api/v1/config");
                    match with_optional_bearer(client.get(&url), &token).send().await {
                        Ok(resp) if resp.status().is_success() => {
                            let config: serde_json::Value = resp.json().await?;
                            let sb = &config["security"]["sandbox"];
                            println!("🔒 Sandbox Profile");
                            println!("   Enabled: {}", sb["enabled"].as_bool().unwrap_or(false));
                            println!("   Global:  {}", sb["profile"].as_str().unwrap_or("balanced"));
                            if let Some(overrides) = sb["guild_overrides"].as_object()
                                && !overrides.is_empty() {
                                    println!("   Guild overrides:");
                                    for (guild, profile) in overrides {
                                        println!("     {} → {}", guild, profile.as_str().unwrap_or("?"));
                                    }
                                }
                        }
                        Ok(resp) => println!("❌ Hub returned error status: {}", resp.status()),
                        Err(_) => println!("❌ Hub is OFFLINE — start it with 'tylluan start'"),
                    }
                }
                SandboxCommand::Set { profile } => {
                    let url = format!("{base}/api/v1/config/sandbox-profile");
                    match with_optional_bearer(client.post(&url).json(&serde_json::json!({ "profile": profile })), &token).send().await {
                        Ok(resp) if resp.status().is_success() => {
                            let json: serde_json::Value = resp.json().await?;
                            println!("✅ Global sandbox profile set to: {}", json["profile"]);
                        }
                        Ok(resp) => {
                            let json: serde_json::Value = resp.json().await.unwrap_or_default();
                            println!("❌ {}", json["error"].as_str().unwrap_or("unknown error"));
                        }
                        Err(_) => println!("❌ Hub is OFFLINE — start it with 'tylluan start'"),
                    }
                }
                SandboxCommand::Guild { name, profile } => {
                    let url = format!("{base}/api/v1/config/sandbox-profile/guild");
                    match with_optional_bearer(client.post(&url).json(&serde_json::json!({ "guild": name, "profile": profile })), &token).send().await {
                        Ok(resp) if resp.status().is_success() => {
                            let json: serde_json::Value = resp.json().await?;
                            println!("✅ Guild '{}' sandbox override set to: {}", name, json["profile"]);
                        }
                        Ok(resp) => {
                            let json: serde_json::Value = resp.json().await.unwrap_or_default();
                            println!("❌ {}", json["error"].as_str().unwrap_or("unknown error"));
                        }
                        Err(_) => println!("❌ Hub is OFFLINE — start it with 'tylluan start'"),
                    }
                }
                SandboxCommand::Session { agent_id, profile } => {
                    let url = format!("{base}/api/v1/config/sandbox-profile/session");
                    match with_optional_bearer(client.post(&url).json(&serde_json::json!({ "agent_id": agent_id, "profile": profile })), &token).send().await {
                        Ok(resp) if resp.status().is_success() => {
                            println!("✅ Session override for agent '{agent_id}' set to: {profile} (in-memory, lost on restart)");
                        }
                        Ok(resp) => {
                            let json: serde_json::Value = resp.json().await.unwrap_or_default();
                            println!("❌ {}", json["error"].as_str().unwrap_or("unknown error"));
                        }
                        Err(_) => println!("❌ Hub is OFFLINE — start it with 'tylluan start'"),
                    }
                }
                SandboxCommand::UnsetGuild { name } => {
                    let url = format!("{base}/api/v1/config/sandbox-profile/guild/{name}");
                    match with_optional_bearer(client.delete(&url), &token).send().await {
                        Ok(resp) if resp.status().is_success() => {
                            println!("✅ Guild '{name}' override removed — falls back to global profile");
                        }
                        Ok(resp) if resp.status() == reqwest::StatusCode::NOT_FOUND => {
                            println!("ℹ️ Guild '{name}' had no override set");
                        }
                        Ok(resp) => println!("❌ Hub returned error status: {}", resp.status()),
                        Err(_) => println!("❌ Hub is OFFLINE — start it with 'tylluan start'"),
                    }
                }
                SandboxCommand::UnsetSession { agent_id } => {
                    let url = format!("{base}/api/v1/config/sandbox-profile/session/{agent_id}");
                    match with_optional_bearer(client.delete(&url), &token).send().await {
                        Ok(resp) if resp.status().is_success() => {
                            println!("✅ Session override for '{agent_id}' cleared");
                        }
                        Ok(resp) => println!("❌ Hub returned error status: {}", resp.status()),
                        Err(_) => println!("❌ Hub is OFFLINE — start it with 'tylluan start'"),
                    }
                }
            }
        }
        Commands::Resume { agent_id } => {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()?;
            let url = format!("http://127.0.0.1:{DEFAULT_PORT}/api/v1/sessions/resume");
            match client.get(&url).query(&[("agent_id", &agent_id)]).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let json: serde_json::Value = resp.json().await?;
                    println!("📋 Resume session for agent: {}", json["agent_id"]);
                    if json["found"].as_bool().unwrap_or(false) {
                        println!("   Summary: {}", json["summary"].as_str().unwrap_or("(empty)"));
                        println!("   Node:    {} ({})", json["node_id"].as_str().unwrap_or("?"), json["node_type"].as_str().unwrap_or("?"));
                        println!("   Created: {}", json["created_at"].as_str().unwrap_or("?"));
                    } else {
                        println!("   Summary: (no session digest yet)");
                    }
                    if let Some(lt) = json.get("last_task").filter(|v| !v.is_null()) {
                        let stale = lt["stale"].as_bool().unwrap_or(false);
                        let stale_mark = if stale { " ⚠️ stale" } else { "" };
                        println!("   Last task: {}{}", lt["task"].as_str().unwrap_or("?"), stale_mark);
                    }
                    let pending = json["pending_actions_for_me"].as_array().map(|a| a.len()).unwrap_or(0);
                    if pending > 0 {
                        println!("   Pending actions: {pending}");
                    }
                    if json["identity"]["registered"].as_bool().unwrap_or(false) {
                        println!("   Identity: registered");
                    }
                }
                Ok(resp) => println!("❌ Hub returned error status: {}", resp.status()),
                Err(_) => println!("❌ Hub is OFFLINE — start it with 'tylluan start'"),
            }
        }
        Commands::Backup { dir, db } => {
            backup::run_backup(&dir, &db)?;
        }
        Commands::Restore { dir } => {
            backup::run_restore(&dir, DEFAULT_PORT).await?;
        }
    }

    Ok(())
}

/// Detects RAM and (best-effort) GPU presence and returns the recommended
/// InstallProfile. Never fabricates a signal it can't check: RAM comes from
/// `sysinfo` (real), GPU detection is a real `nvidia-smi` PATH probe (finds
/// an actual NVIDIA driver install) -- if that check is inconclusive on a
/// platform, this defaults to "no GPU assumed" rather than guessing yes.
///
/// Mapping (deliberately not "≤8GB->clinic" as an early draft of the roadmap
/// item phrased it -- that would recommend a 67MB model download for a
/// machine that can't spare the RAM; Portable/BM25-only is the correct
/// bottom tier, matching its own doc comment "runs on a potato"):
///   RAM < 8GB            -> Portable (BM25-only, zero downloads)
///   RAM >= 8GB, no GPU   -> Clinic   (BGE-Small, light semantic)
///   RAM >= 8GB, GPU seen -> Server   (mxbai-embed-large, full semantic)
fn detect_recommended_profile() -> (InstallProfile, String) {
    let mut sys = System::new();
    sys.refresh_memory();
    let total_ram_gb = sys.total_memory() as f64 / (1024.0 * 1024.0 * 1024.0);

    let has_gpu = find_executable_in_path("nvidia-smi").is_some();

    let profile = if total_ram_gb < 8.0 {
        InstallProfile::Portable
    } else if has_gpu {
        InstallProfile::Server
    } else {
        InstallProfile::Clinic
    };

    let reason = format!(
        "{:.1} GB RAM detected, {} -> recommending '{}'",
        total_ram_gb,
        if has_gpu { "NVIDIA GPU detected (nvidia-smi found)" } else { "no GPU detected" },
        profile,
    );
    (profile, reason)
}

/// Best-effort executable lookup on PATH -- does not run the binary, only
/// checks presence, so this can't hang or fail on a system without a GPU.
fn find_executable_in_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    let exe_name = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(&exe_name))
        .find(|candidate| candidate.is_file())
}

/// `tylluan start --setup`: detect hardware, print the recommendation, and
/// write tylluan.toml to the current directory -- but ONLY if none exists
/// there yet. Never overwrites an existing config; "sin editar TOML" is the
/// M19-P2 closure criterion, not "silently replace whatever the user has."
fn run_setup_wizard() -> Result<()> {
    let config_path = PathBuf::from("tylluan.toml");
    if config_path.exists() {
        println!("ℹ️  tylluan.toml already exists in this directory -- --setup does not overwrite it.");
        println!("   Delete it first (or run in an empty directory) to re-run the wizard.");
        return Ok(());
    }

    println!("🔍 Detecting hardware...");
    let (profile, reason) = detect_recommended_profile();
    println!("   {reason}");

    let toml = generate_config(profile);
    std::fs::write(&config_path, &toml)
        .with_context(|| format!("Failed to write {}", config_path.display()))?;

    println!("✅ tylluan.toml written ({profile} profile) -- no manual editing needed.");
    Ok(())
}

/// Generate a complete tylluan.toml for the given profile as a string with human-readable comments.
fn generate_config(profile: InstallProfile) -> String {
    let (embedding_model, vector_dimensions) = match profile {
        InstallProfile::Portable => ("none", 0),
        InstallProfile::Clinic => ("bge-small", 384),
        InstallProfile::Server => ("mxbai-embed-large", 1024),
    };

    // T1102(a): portable ships with the reranker explicitly OFF. Portable's
    // promise is "boots offline with zero external dependencies", and the
    // cross-encoder needs an ONNX Runtime new enough for the pinned ort
    // (GetVersionString 1.22.x): a machine whose PATH carries an older
    // system dylib (1.5.2 reproduced on a Windows runner, 2026-10-12) would
    // otherwise reach ort on the boot path. Release builds use
    // panic="abort", so ort's version panic is a hard process kill — no
    // catch_unwind exists to soften it. The kernel ALSO re-verifies the
    // dylib version at every boot (onnx_runtime_ort_compatible) and falls
    // back to pure RRF if it is too old, so flipping this to `true` after
    // installing a current onnxruntime is safe and self-checked.
    // Clinic/Server keep SilvaConfig's default (true): they already ship
    // with ONNX models, and the boot-time compat probe is their safety net.
    let silva_block = match profile {
        InstallProfile::Portable => r#"# ── Silva graph search ─────────────────────────────────────────────
[silva]
# Portable assumes NO compatible ONNX Runtime on the machine: reranker OFF.
# Needs ONNX Runtime >= 1.22 for the pinned ort; an older system dylib
# would abort a release boot (panic="abort") before any fallback could run.
# Enable only after installing a current onnxruntime or setting
# ORT_DYLIB_PATH — the kernel re-checks the version at every boot anyway.
reranker_enabled = false
"#,
        InstallProfile::Clinic | InstallProfile::Server => "",
    };

    format!(
        r##"# ── TylluanNexus o3 Configuration ──────────────────────────────────
# Generated by: tylluan install --profile={profile}
# Edit this file to tune your hub instance.

# ── Core settings ───────────────────────────────────────────────────
[nexus]
host = "127.0.0.1"       # Listen address (localhost-only for security)
port = 47004               # HTTP/MCP gateway port (architecture invariant)
dev_mode = false          # NEVER enable in production — disables auth
# http+sse only: `tylluan install`/`tylluan start` spawn the kernel detached from
# any terminal. With "stdio" in this list the kernel reads EOF on its closed stdin
# and shuts down (HTTP included) a millisecond after "operational" -- seen on every
# fresh install in the v0.17.1 release smoke. Stdio is for MCP clients that launch
# the kernel themselves (--stdio).
transport = ["http", "sse"]

# ── Security ─────────────────────────────────────────────────────────
[security]
# encrypt_at_rest left unset on purpose: defaults to true only on binaries built
# with --features encryption (bundles SQLCipher+OpenSSL; not supported on Windows
# native). Setting it to `true` here would be misleading on standard builds,
# which log a warning and run unencrypted instead of silently failing.
# Key resolved from: TYLLUAN_DB_KEY env var > OS keychain > file fallback.

# ── Data paths ──────────────────────────────────────────────────────
[memory]
db_path = "./data/tylluan.db"

# ── Embedding model ─────────────────────────────────────────────────
# | profile  | model             | dim  | use case                  |
# |----------|-------------------|------|---------------------------|
# | portable | none              | 0    | BM25-only, offline-first  |
# | clinic   | bge-small         | 384  | light semantic (~67MB)    |
# | server   | mxbai-embed-large | 1024 | full semantic (1024d)     |
embedding_model = "{embedding_model}"
vector_dimensions = {vector_dimensions}
{silva_block}
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

#[derive(Subcommand)]
enum EvalAction {
    /// Run the LongMemEval-S benchmark (tests memory recall accuracy)
    Longmemevals {
        /// Number of query-document pairs (default: 30, max: 200)
        #[arg(short, long)]
        num_queries: Option<usize>,
        /// Random seed for reproducible results (default: 42)
        #[arg(short, long)]
        seed: Option<u64>,
    },
    /// List past benchmark results with hashes for reproducibility verification
    List,
}

async fn run_stop(
    port: Option<u16>,
    data_dir: Option<&std::path::Path>,
    token: Option<String>,
) -> Result<()> {
    // Hallazgo #3 fix: resolve the EXACT instance instead of sweeping every
    // process whose name contains "tylluan-nexus". The PID file here is the
    // same one the kernel writes at boot (anti_orphan_protection_and_gc) and
    // removes at shutdown, resolved with the same precedence the kernel uses.
    let data_dir_resolved = stop::resolve_pid_data_dir(data_dir, port);
    let pid_file = data_dir_resolved.join(stop::PID_FILE_NAME);
    let pid_from_file = stop::read_pid_file(&pid_file);

    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let mut candidates: Vec<u32> = Vec::new();
    for (pid, process) in sys.processes() {
        if stop::is_kernel_process_name(&process.name().to_string_lossy()) {
            candidates.push(pid.as_u32());
        }
    }

    let pid_alive = |pid: u32| sys.process(sysinfo::Pid::from_u32(pid)).is_some();
    let pid_is_kernel = |pid: u32| {
        sys.process(sysinfo::Pid::from_u32(pid))
            .map(|p| stop::is_kernel_process_name(&p.name().to_string_lossy()))
            .unwrap_or(false)
    };

    match stop::resolve_stop_target(pid_from_file, pid_alive, pid_is_kernel, &candidates)? {
        StopAction::Nothing => {
            println!("⚠️ No running TylluanNexus kernel found.");
            if pid_from_file.is_some() {
                println!(
                    "   (PID file {} is stale — the kernel it described is already gone)",
                    pid_file.display()
                );
            }
        }
        StopAction::Ambiguous(ids) => {
            return Err(anyhow::anyhow!(
                "Multiple TylluanNexus kernels are running (PIDs: {ids:?}) and none was specified. Refusing to kill blindly. Disambiguate with: tylluan-cli stop --port <port> (matches the kernel's --port), or tylluan-cli stop --data-dir <dir> (matches that instance's TYLLUAN_DATA_DIR)."
            ));
        }
        StopAction::Target(pid) => {
            // Graceful shutdown (HTTP) is only attempted when we can be sure the
            // target listens on the port we hit: an explicit --port, or the
            // single-candidate case where any kernel answering on the default
            // port IS that candidate.
            let graceful_port = if port.is_some() || candidates.len() == 1 {
                Some(port.unwrap_or(DEFAULT_PORT))
            } else {
                None
            };
            stop_instance(pid, graceful_port, token, &pid_file, &mut sys).await?;
        }
    }
    Ok(())
}

async fn stop_instance(
    pid: u32,
    graceful_port: Option<u16>,
    token: Option<String>,
    pid_file: &std::path::Path,
    sys: &mut System,
) -> Result<()> {
    println!("🛑 Stopping TylluanNexus kernel (PID {pid})...");

    // 1. Graceful shutdown via the kernel's localhost-only admin endpoint so
    //    guilds and auxiliary services shut down cleanly (same endpoint the
    //    kernel's own hot-swap path uses).
    if let Some(p) = graceful_port {
        let url = format!("http://127.0.0.1:{p}/api/v1/admin/shutdown");
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()?;
        let mut req = client.post(&url).header("host", "127.0.0.1");
        if let Some(t) = token.as_deref().filter(|t| !t.is_empty()) {
            req = req.bearer_auth(t);
        }
        match req.send().await {
            Ok(resp) if resp.status().is_success() => {
                println!("   ✅ graceful shutdown accepted on port {p} — waiting for exit...");
                // The kernel's graceful path announces to coloquio, tears down
                // tunnels/guilds/services — measured live it can take >15s, so
                // wait up to 30s before falling back to a hard kill.
                for _ in 0..30 {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    sys.refresh_processes(
                        sysinfo::ProcessesToUpdate::Some(&[sysinfo::Pid::from_u32(pid)]),
                        true,
                    );
                    if sys.process(sysinfo::Pid::from_u32(pid)).is_none() {
                        println!("✅ Kernel exited cleanly.");
                        let _ = std::fs::remove_file(pid_file);
                        return Ok(());
                    }
                }
                println!("   ⚠️ still running after graceful shutdown — falling back to hard kill.");
            }
            Ok(resp)
                if resp.status() == reqwest::StatusCode::UNAUTHORIZED
                    || resp.status() == reqwest::StatusCode::FORBIDDEN =>
            {
                println!(
                    "   ⚠️ graceful shutdown refused ({}) — this instance requires a bearer token; pass --token.",
                    resp.status()
                );
            }
            _ => {
                println!("   (no kernel answered on port {p} — stopping PID directly)");
            }
        }
    }

    // 2. Hard kill of the VERIFIED PID only — never a name sweep.
    if let Some(process) = sys.process(sysinfo::Pid::from_u32(pid)) {
        process.kill();
        for _ in 0..10 {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            sys.refresh_processes(
                sysinfo::ProcessesToUpdate::Some(&[sysinfo::Pid::from_u32(pid)]),
                true,
            );
            if sys.process(sysinfo::Pid::from_u32(pid)).is_none() {
                break;
            }
        }
    }
    let _ = std::fs::remove_file(pid_file);
    println!("✅ Kernel stopped (PID {pid}).");
    Ok(())
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
        let base = if h.contains("://") { h } else { format!("http://{h}") };
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

fn models_dir_approx_size(dir: &std::path::Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                total += std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            } else if path.is_dir() {
                total += models_dir_approx_size(&path);
            }
        }
    }
    total
}

/// Parse a release version, accepting the optional `v` prefix.
fn parse_release_version(raw: &str) -> Result<semver::Version> {
    let trimmed = raw.trim().trim_start_matches('v');
    semver::Version::parse(trimmed)
        .with_context(|| format!("Invalid release version '{raw}' (expected semver)"))
}

/// Compare the installed and published versions without ever treating a
/// downgrade as an update. Returns `Greater` when `latest` is newer.
fn compare_versions(current: &str, latest: &str) -> Result<std::cmp::Ordering> {
    Ok(parse_release_version(latest)?.cmp(&parse_release_version(current)?))
}

/// Extract the SHA-256 for `asset` from a release-level checksums.txt.
fn checksum_for_asset(text: &str, asset: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(hash) = parts.next() else { continue };
        let Some(name) = parts.next() else { continue };
        if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        let name = name.trim_start_matches('*');
        let name = name.strip_prefix("./").unwrap_or(name);
        if name == asset {
            return Some(hash.to_ascii_lowercase());
        }
    }
    None
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn verify_sha256(bytes: &[u8], expected: &str) -> Result<()> {
    let actual = sha256_hex(bytes);
    if !actual.eq_ignore_ascii_case(expected.trim()) {
        anyhow::bail!(
            "SHA-256 mismatch for release archive: expected {expected}, got {actual}"
        );
    }
    Ok(())
}

/// The two binaries every release archive must contain. The suffix is empty on
/// Unix and `.exe` on Windows, matching the release workflow.
fn update_binary_names() -> [String; 2] {
    let suffix = std::env::consts::EXE_SUFFIX;
    [
        format!("tylluan-nexus{suffix}"),
        format!("tylluan-cli{suffix}"),
    ]
}

/// Extract exactly the two expected binaries, each into its own path. Entry
/// paths from the archive are never used as destinations, so a malicious `../`
/// member cannot escape the staging directory.
fn extract_update_binaries(bytes: &[u8], stage: &Path) -> Result<Vec<(String, PathBuf)>> {
    use std::collections::HashMap;

    let names = update_binary_names();
    let mut found: HashMap<String, PathBuf> = HashMap::new();
    let decoder = flate2::read::GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(decoder);

    for entry in archive.entries().context("Could not read release archive")? {
        let mut entry = entry.context("Could not read archive entry")?;
        let path = entry.path().context("Could not read archive entry path")?;
        let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_owned) else {
            continue;
        };
        if !names.contains(&name) {
            continue;
        }
        if found.contains_key(&name) {
            anyhow::bail!("Release archive contains duplicate {name}");
        }
        let destination = stage.join(&name);
        entry
            .unpack(&destination)
            .with_context(|| format!("Could not extract {name}"))?;
        found.insert(name, destination);
    }

    let mut binaries = Vec::with_capacity(names.len());
    for name in names {
        let path = found
            .remove(&name)
            .ok_or_else(|| anyhow::anyhow!("Release archive is missing {name}"))?;
        binaries.push((name, path));
    }
    Ok(binaries)
}

/// Install each staged binary under its own name. On Windows, rename the old
/// file aside before moving the new one into place; if that is impossible
/// because the old binary is running, leave the new binary beside it and fail
/// instead of overwriting a live executable.
fn install_staged_binaries(
    staged: &[(String, PathBuf)],
    destination_dir: &Path,
) -> Result<Vec<PathBuf>> {
    let mut installed = Vec::with_capacity(staged.len());
    for (name, staged_path) in staged {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(staged_path, std::fs::Permissions::from_mode(0o755))
                .with_context(|| format!("Could not make {name} executable"))?;
        }
        let destination = destination_dir.join(name);
        replace_update_binary(staged_path, &destination)?;
        installed.push(destination);
    }
    Ok(installed)
}

#[cfg(not(windows))]
fn replace_update_binary(new_path: &Path, destination: &Path) -> Result<()> {
    std::fs::rename(new_path, destination).with_context(|| {
        format!(
            "Could not replace {} with {}",
            destination.display(),
            new_path.display()
        )
    })?;
    Ok(())
}

#[cfg(windows)]
fn replace_update_binary(new_path: &Path, destination: &Path) -> Result<()> {
    if !destination.exists() {
        std::fs::rename(new_path, destination).with_context(|| {
            format!("Could not install {}", destination.display())
        })?;
        return Ok(());
    }

    let file_name = destination
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    let aside = destination.with_file_name(format!("{file_name}.old-{}", std::process::id()));

    match std::fs::rename(destination, &aside) {
        Ok(()) => match std::fs::rename(new_path, destination) {
            Ok(()) => {
                let _ = std::fs::remove_file(&aside);
                Ok(())
            }
            Err(error) => {
                let _ = std::fs::rename(&aside, destination);
                Err(error).with_context(|| {
                    format!("Could not install {}", destination.display())
                })
            }
        },
        Err(error) => {
            let fallback = destination.with_file_name(format!("{file_name}.new"));
            std::fs::rename(new_path, &fallback).with_context(|| {
                format!("Could not save replacement for {}", destination.display())
            })?;
            anyhow::bail!(
                "Could not replace {} because the old binary is in use; \
                 new binary saved at {} (rename-aside failed: {error})",
                destination.display(),
                fallback.display()
            )
        }
    }
}

/// Detect the current platform target triple for update downloads.
/// Must match the release artifact naming in .github/workflows/release.yml.
fn detect_update_target() -> String {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    match (os, arch) {
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu".into(),
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu".into(),
        ("macos", "x86_64") => "x86_64-apple-darwin".into(),
        ("macos", "aarch64") => "aarch64-apple-darwin".into(),
        ("windows", "x86_64") => "x86_64-pc-windows-msvc".into(),
        ("windows", "aarch64") => "aarch64-pc-windows-msvc".into(),
        _ => format!("{arch}-{os}"),
    }
}

#[cfg(test)]
mod installer_flag_tests {
    use super::*;
    use clap::CommandFactory;
    use std::collections::BTreeSet;

    const INSTALL_SH: &str = include_str!("../../../install.sh");
    const INSTALL_PS1: &str = include_str!("../../../install.ps1");

    fn accepted_flags(subcommand: &str) -> BTreeSet<String> {
        // No cmd.build(): building the root walks every subcommand, and the
        // pre-existing `connect -h`/auto-help conflict debug-asserts panic
        // (separate latent bug, reported, not #1's scope). find_subcommand
        // only materializes the requested subcommand's derive-defined args.
        let cmd = Cli::command();
        cmd.find_subcommand(subcommand)
            .unwrap_or_else(|| panic!("CLI lost its `{subcommand}` subcommand"))
            .get_arguments()
            .filter_map(|a| a.get_long().map(|l| l.to_string()))
            .collect()
    }

    /// Every `<cli-binary> <subcommand> [--flag ...]` invocation found in an
    /// installer script, as (1-based line number, flags used). Covers both
    /// shapes present in the repo:
    ///   `"${BIN_DIR}/tylluan" start --profile portable &`   (install.sh)
    ///   `-ArgumentList "start --profile portable"`          (install.ps1)
    fn invocations_with_flags(script: &str, subcommand: &str) -> Vec<(usize, Vec<String>)> {
        let mut found = Vec::new();
        for (idx, line) in script.lines().enumerate() {
            if !line.contains("tylluan") {
                continue;
            }
            let mut search_from = 0;
            while let Some(rel) = line[search_from..].find(subcommand) {
                let start = search_from + rel;
                let end = start + subcommand.len();
                let before_ok = matches!(
                    line[..start].chars().next_back(),
                    None | Some(' ') | Some('\t') | Some('"') | Some('\'') | Some('(')
                );
                let after_ok = matches!(
                    line[end..].chars().next(),
                    None | Some(' ')
                        | Some('\t')
                        | Some('"')
                        | Some('\'')
                        | Some(')')
                        | Some('&')
                        | Some(';')
                        | Some('|')
                );
                if before_ok && after_ok {
                    let rest = &line[end..];
                    let stop = rest
                        .find(['&', ';', '|', '`'])
                        .unwrap_or(rest.len());
                    let flags: Vec<String> = rest[..stop]
                        .split_whitespace()
                        .filter_map(|tok| tok.strip_prefix("--"))
                        .map(|flag| {
                            // Strip trailing shell/paren/quote noise so a
                            // token like `--flag)` still compares as `--flag`.
                            flag.trim_end_matches(|c: char| {
                                !(c.is_ascii_alphanumeric() || c == '-' || c == '_')
                            })
                                .split('=')
                                .next()
                                .unwrap_or("")
                                .to_string()
                        })
                        .filter(|f| !f.is_empty())
                        .collect();
                    found.push((idx + 1, flags));
                    break;
                }
                search_from = end;
            }
        }
        found
    }

    #[test]
    fn installer_scripts_only_use_flags_the_cli_actually_accepts() {
        for (name, script) in [("install.sh", INSTALL_SH), ("install.ps1", INSTALL_PS1)] {
            for subcommand in ["start", "install"] {
                let accepted = accepted_flags(subcommand);
                let invocations = invocations_with_flags(script, subcommand);
                assert!(
                    !invocations.is_empty(),
                    "{name}: no `{subcommand}` invocation found — installer changed shape; \
                     update this test to match"
                );
                for (line_no, flags) in invocations {
                    for flag in flags {
                        assert!(
                            accepted.contains(&flag),
                            "{name}:{line_no}: `{subcommand} --{flag}` — clap rejects --{flag} \
                             on `tylluan {subcommand}` (accepted flags: {accepted:?})"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_generated_config_never_enables_stdio_transport() {
        // A kernel spawned by the installer has no usable stdin: stdio in the
        // transport list makes it exit on EOF and take HTTP down with it.
        for profile in [
            InstallProfile::Portable,
            InstallProfile::Clinic,
            InstallProfile::Server,
        ] {
            let cfg = generate_config(profile);
            let line = cfg
                .lines()
                .find(|l| l.trim_start().starts_with("transport"))
                .expect("generated config must declare a transport");
            assert!(
                !line.contains("stdio"),
                "generated config must not enable stdio, got: {line}"
            );
            assert!(line.contains("http"), "generated config must serve http, got: {line}");
        }
    }

    #[test]
    fn test_generate_config_profiles() {
        let portable = generate_config(InstallProfile::Portable);
        assert!(portable.contains(r#"embedding_model = "none""#));
        assert!(portable.contains("vector_dimensions = 0"));
        // T1102(a): portable must ship with the reranker explicitly off so a
        // machine without (or with a too-old) ONNX Runtime still boots — its
        // whole promise is zero external dependencies.
        assert!(
            portable.contains("[silva]"),
            "portable must emit a [silva] section to carry the reranker override"
        );
        assert!(
            portable.contains("reranker_enabled = false"),
            "portable must disable the reranker (needs ONNX Runtime >= 1.22)"
        );

        let clinic = generate_config(InstallProfile::Clinic);
        assert!(clinic.contains(r#"embedding_model = "bge-small""#));
        assert!(clinic.contains("vector_dimensions = 384"));
        // Clinic/Server keep SilvaConfig's default (reranker_enabled = true);
        // the boot-time onnx_runtime_ort_compatible() probe is their safety
        // net — they must NOT inherit portable's override.
        assert!(
            !clinic.contains("reranker_enabled"),
            "clinic must not override the SilvaConfig reranker default"
        );

        let server = generate_config(InstallProfile::Server);
        assert!(server.contains(r#"embedding_model = "mxbai-embed-large""#));
        assert!(server.contains("vector_dimensions = 1024"));
        assert!(
            !server.contains("reranker_enabled"),
            "server must not override the SilvaConfig reranker default"
        );
    }
}

/// T-DSH-116: `tylluan inference points|set-point`.
async fn run_inference(action: InferenceAction, port: u16, token: Option<String>) -> Result<()> {
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let token = resolve_cli_token(token);
    match action {
        InferenceAction::Points => {
            let req = with_optional_bearer(
                client.get(format!("{base}/api/v1/inference/points")),
                &token,
            );
            let resp = req.send().await.context("no pude contactar el kernel")?;
            if !resp.status().is_success() {
                anyhow::bail!("kernel respondió {} (¿token?)", resp.status());
            }
            let json: serde_json::Value = resp.json().await?;
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
        InferenceAction::SetPoint {
            point,
            provider,
            endpoint,
            model,
            api_key_env,
            cloud_enabled,
            persist,
        } => {
            let mut body = serde_json::Map::new();
            if let Some(v) = provider { body.insert("provider".into(), v.into()); }
            if let Some(v) = endpoint { body.insert("endpoint".into(), v.into()); }
            if let Some(v) = model { body.insert("model".into(), v.into()); }
            if let Some(v) = api_key_env { body.insert("api_key_env".into(), v.into()); }
            if let Some(v) = cloud_enabled { body.insert("cloud_enabled".into(), v.into()); }
            if persist { body.insert("persist".into(), true.into()); }
            if body.is_empty() {
                anyhow::bail!("set-point sin cambios: pasa --provider/--endpoint/--model/--api-key-env/--cloud-enabled");
            }
            let req = with_optional_bearer(
                client
                    .post(format!("{base}/api/v1/inference/points/{point}"))
                    .json(&serde_json::Value::Object(body)),
                &token,
            );
            let resp = req.send().await.context("no pude contactar el kernel")?;
            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("kernel respondió {status}: {text}");
            }
            let json: serde_json::Value = resp.json().await?;
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
    }
    Ok(())
}

/// T-DSH-114: `tylluan sentinel status|mode <off|observe|enforce>`.
async fn run_sentinel(action: SentinelAction, port: u16, token: Option<String>) -> Result<()> {
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let token = resolve_cli_token(token);
    match action {
        SentinelAction::Status => {
            let req = with_optional_bearer(
                client.get(format!("{base}/api/v1/security/sentinel")),
                &token,
            );
            let resp = req
                .send()
                .await
                .context("no pude contactar el kernel (¿está arrancado?)")?;
            if !resp.status().is_success() {
                anyhow::bail!(
                    "kernel respondió {} (¿token? usa --token o TYLLUAN_TOKEN)",
                    resp.status()
                );
            }
            let json: serde_json::Value = resp.json().await?;
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
        SentinelAction::Mode { mode } => {
            if !valid_sentinel_mode(&mode) {
                anyhow::bail!("modo inválido '{mode}': usa off|observe|enforce");
            }
            let body = serde_json::json!({ "mode": mode.trim().to_ascii_lowercase() });
            let req = with_optional_bearer(
                client
                    .post(format!("{base}/api/v1/security/sentinel/mode"))
                    .json(&body),
                &token,
            );
            let resp = req
                .send()
                .await
                .context("no pude contactar el kernel (¿está arrancado?)")?;
            if !resp.status().is_success() {
                anyhow::bail!(
                    "kernel respondió {} (¿token? usa --token o TYLLUAN_TOKEN)",
                    resp.status()
                );
            }
            let json: serde_json::Value = resp.json().await?;
            println!(
                "sentinel mode: {} (antes {})",
                json["mode"], json["previous"]
            );
            println!("{}", serde_json::to_string_pretty(&json["state"])?);
        }
    }
    Ok(())
}

/// T-DSH-115: accion de `tylluan update` — sin orden explicita solo avisa.
fn update_action(check: bool, install: bool) -> &'static str {
    if check {
        "check"
    } else if install {
        "install"
    } else {
        "notice"
    }
}

/// Resolve the bearer token for a CLI HTTP call: explicit `--token` flag wins
/// over `TYLLUAN_TOKEN`, which wins over `.tylluan-token` in the current
/// directory (the same file the kernel reads at startup). `None` means "send
/// no Authorization header", preserving the dev_mode=true behaviour.
fn resolve_cli_token(flag: Option<String>) -> Option<String> {
    let env = std::env::var("TYLLUAN_TOKEN").ok();
    let file = std::fs::read_to_string(".tylluan-token").ok();
    pick_token(flag, env, file)
}

/// Pure precedence helper (unit-tested): first non-empty value wins after trim.
fn pick_token(flag: Option<String>, env: Option<String>, file: Option<String>) -> Option<String> {
    [flag, env, file]
        .into_iter()
        .flatten()
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty())
}

/// Attach `Authorization: Bearer <token>` only when a token was resolved.
fn with_optional_bearer(
    req: reqwest::RequestBuilder,
    token: &Option<String>,
) -> reqwest::RequestBuilder {
    match token {
        Some(t) => req.bearer_auth(t),
        None => req,
    }
}

#[cfg(test)]
mod sandbox_token_tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn update_without_order_only_notifies() {
        assert_eq!(update_action(false, false), "notice");
        assert_eq!(update_action(true, false), "check");
        assert_eq!(update_action(false, true), "install");
        assert_eq!(update_action(true, true), "check");
    }

    #[test]
    fn sentinel_mode_validation_matches_kernel() {
        for ok in ["off", "observe", "enforce", " ENFORCE "] {
            assert!(valid_sentinel_mode(ok), "{ok}");
        }
        for bad in ["", "on", "basura", "true"] {
            assert!(!valid_sentinel_mode(bad), "{bad}");
        }
    }

    #[test]
    fn pick_token_prefers_flag_then_env_then_file() {
        assert_eq!(
            pick_token(Some("flag".into()), Some("env".into()), Some("file".into())).as_deref(),
            Some("flag")
        );
        assert_eq!(
            pick_token(None, Some("env".into()), Some("file".into())).as_deref(),
            Some("env"),
            "el env debe ganar al fichero"
        );
        assert_eq!(
            pick_token(None, None, Some("file".into())).as_deref(),
            Some("file")
        );
        assert_eq!(pick_token(None, None, None), None, "sin fichero ni env => None");
    }

    #[test]
    fn pick_token_ignores_empty_and_whitespace() {
        assert_eq!(
            pick_token(Some("   ".into()), Some(" env \n".into()), None).as_deref(),
            Some("env")
        );
        assert_eq!(pick_token(Some(String::new()), None, None), None);
        assert_eq!(pick_token(None, Some("\t\n".into()), None), None);
    }

    #[test]
    fn sandbox_subcommand_accepts_token_flag() {
        let cmd = Cli::command();
        let sandbox = cmd
            .find_subcommand("sandbox")
            .expect("el CLI debe conservar el subcomando sandbox");
        let has_token = sandbox
            .get_arguments()
            .any(|arg| arg.get_long() == Some("token"));
        assert!(has_token, "`tylluan sandbox` debe aceptar --token");
    }
}

#[cfg(test)]
mod startup_wait_tests {
    use super::*;

    #[tokio::test]
    async fn wait_for_health_honors_cap_when_nothing_listens() {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .build()
            .unwrap();
        let started = std::time::Instant::now();
        let ready = wait_for_health(&client, "http://127.0.0.1:9/health", 2).await;
        let elapsed = started.elapsed();
        assert!(!ready, "no listener => never ready");
        assert!(elapsed.as_secs() >= 2, "must wait the full cap (got {elapsed:?})");
        assert!(elapsed.as_secs() <= 5, "must stop at the cap (got {elapsed:?})");
    }

    #[tokio::test]
    async fn wait_for_health_turns_ready_once_the_server_answers() {
        // One-shot std server: answers a single GET with 200, then done.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            if let Ok((mut sock, _)) = listener.accept() {
                // Drain the request first: closing with unread request bytes
                // pending makes Windows send RST and the response is lost.
                let mut req_buf = [0u8; 1024];
                let _ = sock.read(&mut req_buf);
                let mut resp = Vec::new();
                for part in [b"HTTP/1.1 200 OK".as_slice(), b"content-length: 0",
                    b"connection: close", b""]
                {
                    resp.extend_from_slice(part);
                    resp.extend_from_slice(&[13u8, 10u8]);
                }
                let _ = sock.write_all(&resp);
                let _ = sock.flush();
                // Stay alive briefly so the client can read before FIN.
                std::thread::sleep(std::time::Duration::from_millis(400));
            }
        });
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .build()
            .unwrap();
        let started = std::time::Instant::now();
        let ready = wait_for_health(&client, &format!("http://{addr}/health"), 30).await;
        assert!(ready, "a server answering 200 must flip the wait to ready");
        let elapsed = started.elapsed();
        assert!(elapsed.as_secs() >= 1, "first poll happens after the 1s sleep");
        assert!(elapsed.as_secs() <= 3, "ready must be reported on the first 200, got {elapsed:?}");
    }
}

#[cfg(test)]
mod update_tests {
    use super::*;
    use std::io::Write;

    fn gzip_tar(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (name, bytes) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder
                .append_data(&mut header, name, *bytes)
                .expect("append test tar entry");
        }
        let tar_bytes = builder.into_inner().expect("finish test tar");
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&tar_bytes).expect("gzip test tar");
        encoder.finish().expect("finish test gzip")
    }

    #[test]
    fn semver_never_downgrades_and_rejects_invalid_versions() {
        assert_eq!(
            compare_versions("0.17.2", "0.17.1").unwrap(),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            compare_versions("0.17.2", "0.17.2").unwrap(),
            std::cmp::Ordering::Equal
        );
        assert_eq!(
            compare_versions("0.17.2", "v0.17.3").unwrap(),
            std::cmp::Ordering::Greater
        );
        assert!(compare_versions("not-a-version", "0.17.3").is_err());
    }

    #[test]
    fn checksum_parser_selects_asset_and_detects_mismatch() {
        let archive = b"release-archive-bytes";
        let hash = sha256_hex(archive);
        let text = format!(
            "{}  unrelated.tar.gz\n{} *tylluan-x86_64-unknown-linux-gnu.tar.gz\n",
            "0".repeat(64),
            hash
        );
        let selected = checksum_for_asset(&text, "tylluan-x86_64-unknown-linux-gnu.tar.gz")
            .expect("archive entry must be found");
        assert_eq!(selected, hash);
        assert!(verify_sha256(archive, &selected).is_ok());
        assert!(verify_sha256(b"tampered", &selected).is_err());
        assert!(checksum_for_asset(&text, "missing.tar.gz").is_none());
    }

    #[test]
    fn archive_extraction_keeps_each_binary_under_its_own_name() {
        let names = update_binary_names();
        let entries: Vec<(&str, &[u8])> = vec![
            (names[0].as_str(), b"nexus-payload"),
            (names[1].as_str(), b"cli-payload"),
        ];
        let archive = gzip_tar(&entries);
        let stage = tempfile::tempdir().expect("tempdir");
        let found = extract_update_binaries(&archive, stage.path()).expect("extract both binaries");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].0, names[0]);
        assert_eq!(found[1].0, names[1]);
        assert_eq!(std::fs::read(&found[0].1).unwrap(), b"nexus-payload");
        assert_eq!(std::fs::read(&found[1].1).unwrap(), b"cli-payload");
    }

    #[test]
    fn archive_extraction_rejects_a_missing_binary() {
        let names = update_binary_names();
        let archive = gzip_tar(&[(names[0].as_str(), b"only-nexus")]);
        let stage = tempfile::tempdir().expect("tempdir");
        let error = extract_update_binaries(&archive, stage.path()).unwrap_err();
        assert!(
            error.to_string().contains(&names[1]),
            "error must name the missing binary, got: {error}"
        );
    }
}
