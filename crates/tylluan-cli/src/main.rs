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
    },
    /// Stop the TylluanNexus kernel
    Stop,
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
        /// Host (alternative to full URL, e.g. 192.168.1.42:3030)
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
    /// Check for updates and auto-update Tylluan binary
    Update {
        /// Only check for updates, don't download
        #[arg(long)]
        check: bool,
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

            let child = cmd.spawn()
                .with_context(|| format!("Failed to launch kernel at {}", exe_path.display()))?;
            
            println!("✅ Kernel started with PID: {}", child.id());

            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(2))
                .build()?;
            let port = port.unwrap_or(DEFAULT_PORT);
            let url = format!("http://127.0.0.1:{}/health", port);

            for i in 1..=30 {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                match client.get(&url).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        println!("✅ Kernel is ready at http://127.0.0.1:{}", port);
                        println!("🌐 Connect your MCP client to http://127.0.0.1:{}/sse", port);
                        break;
                    }
                    _ if i == 30 => {
                        println!("⚠️ Kernel started but not ready within 30s. Check logs.");
                    }
                    _ => {}
                }
            }
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
        Commands::Doctor => {
            println!("🩺 Tylluan Diagnostic Scan (v{})", env!("CARGO_PKG_VERSION"));
            println!("{}\n", "─".repeat(48));

            let mut all_ok = true;

            // ── 1. Binary & version ──
            print!("[1/7] Binary version: {} ... ", env!("CARGO_PKG_VERSION"));
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
                    Err(e) => { println!("❌ (read error: {})", e); all_ok = false; String::new() }
                };
                if !raw.is_empty() {
                    match raw.parse::<toml::Value>() {
                        Ok(_) => println!("[2/7] Config file: {} ... ✅", p.display()),
                        Err(e) => { println!("[2/7] Config file: {} ... ❌ (invalid TOML: {})", p.display(), e); all_ok = false; }
                    }
                }
            } else {
                println!("[2/7] Config file: not found (tylluan.toml) ... ⚠️");
                println!("       Run 'tylluan install --profile portable' to create one.");
            }

            // ── 3. Python version ──
            let python_check = || -> Option<String> {
                for bin in &["python3", "python"] {
                    if let Ok(out) = Command::new(bin).arg("--version").output() {
                        if out.status.success() {
                            let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
                            return Some(v);
                        }
                    }
                }
                None
            };
            match python_check() {
                Some(v) => {
                    // Check major.minor >= 3.11
                    let major_minor = v.split_whitespace().nth(1).unwrap_or("0.0")
                        .split('.').take(2).map(|n| n.parse::<u32>().unwrap_or(0)).collect::<Vec<_>>();
                    if major_minor.len() == 2 && major_minor[0] >= 3 && major_minor[1] >= 11 {
                        println!("[3/7] Python: {} ... ✅", v);
                    } else {
                        println!("[3/7] Python: {} ... ❌ (need 3.11+)", v);
                        all_ok = false;
                    }
                }
                None => {
                    println!("[3/7] Python: not found ... ❌");
                    println!("       Install Python 3.11+: https://python.org/downloads");
                    all_ok = false;
                }
            }

            // ── 4. Guilds installed ──
            let guild_dir = PathBuf::from("guilds").join("core");
            if guild_dir.exists() {
                let count = match std::fs::read_dir(&guild_dir) {
                    Ok(entries) => entries.filter_map(|e| e.ok()).filter(|e| e.path().extension().map_or(false, |x| x == "py" || x == "rs")).count(),
                    Err(_) => 0,
                };
                println!("[4/7] Guilds core: {} guilds at {} ... ✅", count, guild_dir.display());
            } else {
                println!("[4/7] Guilds core: {} not found ... ⚠️", guild_dir.display());
                println!("       Guilds will be loaded from ~/.tylluan/guilds/ at runtime.");
            }

            // ── 5. Embedding model cache ──
            let models_dir = PathBuf::from("models");
            let model_cache_ok = models_dir.exists() && std::fs::read_dir(&models_dir).map_or(false, |mut e| e.next().is_some());
            if model_cache_ok {
                let size = models_dir_approx_size(&models_dir);
                println!("[5/7] Embedding model: cached (~{} MB) ... ✅", size / 1024 / 1024);
            } else {
                println!("[5/7] Embedding model: not cached ... ⚠️");
                println!("       BM25-only mode active. Run 'tylluan download-models' for semantic search.");
            }

            // ── 6. Port free ──
            let port_free = std::net::TcpListener::bind(("127.0.0.1", DEFAULT_PORT)).is_ok();
            if port_free {
                println!("[6/7] Port {}: available ... ✅", DEFAULT_PORT);
            } else {
                println!("[6/7] Port {}: in use (kernel likely running) ... ✅", DEFAULT_PORT);
            }

            // ── 7. Kernel health (online check) ──
            let kernel_running = !port_free;
            if kernel_running {
                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(5))
                    .build()?;
                let url = format!("http://127.0.0.1:{}/health", DEFAULT_PORT);
                match client.get(&url).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        let json: serde_json::Value = resp.json().await?;
                        let status = json.get("status").and_then(|s| s.as_str()).unwrap_or("ok");
                        println!("[7/7] Kernel: running ({}) ... ✅", status);
                        // Also fetch detailed doctor report
                        let doctor_url = format!("http://127.0.0.1:{}/api/v1/doctor", DEFAULT_PORT);
                        if let Ok(resp) = client.get(&doctor_url).send().await {
                            if let Ok(report) = resp.json::<serde_json::Value>().await {
                                if let Some(guilds) = report["guilds"].as_array() {
                                    let down: Vec<_> = guilds.iter()
                                        .filter(|g| g["running"].as_bool() == Some(false))
                                        .filter_map(|g| g["name"].as_str())
                                        .collect();
                                    if !down.is_empty() {
                                        println!("       Guilds DOWN: {}", down.join(", "));
                                    }
                                }
                                if let Some(suggestions) = report["suggestions"].as_array() {
                                    if !suggestions.is_empty() {
                                        println!("\n   Suggestions:");
                                        for s in suggestions {
                                            if let Some(s) = s.as_str() {
                                                println!("   - {}", s);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Ok(resp) => println!("[7/7] Kernel: error {} ... ❌", resp.status()),
                    Err(_) => println!("[7/7] Kernel: unreachable despite port open ... ❌"),
                }
            } else {
                println!("[7/7] Kernel: not running ... ⚪");
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
        Commands::Audit { .. } => {
            println!("🔍 Audit chain verification...");
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()?;
            let verify_url = format!("http://127.0.0.1:{}/api/v1/audit/verify", DEFAULT_PORT);
            match client.get(&verify_url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let json: serde_json::Value = resp.json().await?;
                    let status = json["status"].as_str().unwrap_or("unknown");
                    let icon = match status {
                        "clean" => "✅",
                        "tampered" => "🚨",
                        _ => "⚠️",
                    };
                    println!("{} Chain integrity: {}", icon, status);
                    println!("   Valid entries:   {}", json["valid_count"]);
                    println!("   Tampered entries: {}", json["tampered_count"]);
                }
                Ok(resp) => println!("❌ Hub returned error status: {}", resp.status()),
                Err(_) => println!("❌ Hub is OFFLINE — start it with 'tylluan start'"),
            }
        }
        Commands::Eval { action } => match action {
            EvalAction::Longmemevals { num_queries, seed } => {
                println!("🧪 Running LongMemEval-S benchmark...");
                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(300))
                    .build()?;
                let url = format!("http://127.0.0.1:{}/api/v1/eval/run", DEFAULT_PORT);
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
                            println!("✅ LongMemEval-S complete ({} queries):", n);
                            println!("   Recall@1:  {:.1}%", recall_1);
                            println!("   Recall@5:  {:.1}%", recall_5);
                            println!("   Recall@10: {:.1}%", recall_10);
                            println!("   Latency:   mean={:.0}ms p95={:.0}ms", mean_lat, p95_lat);
                            println!("   Hash:      {}", hash);
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
                let url = format!("http://127.0.0.1:{}/api/v1/eval/results", DEFAULT_PORT);
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
                                println!("      R@1={:.1}% R@5={:.1}% R@10={:.1}%  lat={:.0}ms",
                                    recall_1, recall_5, recall_10, mean_lat);
                                println!("      hash: {}", hash);
                            }
                        }
                    }
                    Ok(resp) => println!("❌ Hub returned error status: {}", resp.status()),
                    Err(_) => println!("❌ Hub is OFFLINE — start it with 'tylluan start'"),
                }
            }
        },
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

            if profile != InstallProfile::Portable {
                let model_name = match profile {
                    InstallProfile::Clinic => "BGE-Small (67MB)",
                    _ => "BGE-M3 (1.2GB)",
                };
                println!("📥 Downloading {} embedding model...", model_name);
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
            let url = format!("http://127.0.0.1:{}/health", DEFAULT_PORT);

            for i in 1..=30 {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                match client.get(&url).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        println!();
                        println!("🎉 Tylluan is running at http://127.0.0.1:{}", DEFAULT_PORT);
                        println!();
                        println!("   Connect your MCP client:");
                        println!("     Claude Desktop -> http://127.0.0.1:{}/sse", DEFAULT_PORT);
                        println!("     Claude Code   -> /mcp add tylluan sse http://127.0.0.1:{}/sse", DEFAULT_PORT);
                        println!("     curl          -> curl http://127.0.0.1:{}/health", DEFAULT_PORT);
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
        Commands::Update { check } => {
            println!("🔍 Checking for updates...");
            let repo = "Forja-orca/tylluan";
            let current_ver = env!("CARGO_PKG_VERSION");
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .user_agent("tylluan-update/1.0")
                .build()?;

            let release_url = format!("https://api.github.com/repos/{}/releases/latest", repo);
            let resp = client.get(&release_url).send().await?;

            if !resp.status().is_success() {
                println!("❌ Failed to check updates: HTTP {}", resp.status());
                return Ok(());
            }

            let release: serde_json::Value = resp.json().await?;
            let latest_tag = release["tag_name"].as_str().unwrap_or("v0.0.0");
            let latest_ver = latest_tag.trim_start_matches('v');

            if latest_ver == current_ver {
                println!("✅ Tylluan v{} is up to date.", current_ver);
                return Ok(());
            }

            println!("📦 Update available: v{} → v{}", current_ver, latest_ver);

            if check {
                println!("   Run 'tylluan update' without --check to download.");
                return Ok(());
            }

            // Detect current platform
            let target = detect_update_target();
            let archive_name = format!("tylluan-{}.tar.gz", target);
            let download_url = format!(
                "https://github.com/{}/releases/download/{}/{}",
                repo, latest_tag, archive_name
            );

            println!("📥 Downloading {} ...", archive_name);
            let download_resp = client.get(&download_url).send().await?;
            if !download_resp.status().is_success() {
                println!("❌ Download failed: HTTP {} — unsupported platform: {}", download_resp.status(), target);
                println!("   Manual download: https://github.com/{}/releases", repo);
                return Ok(());
            }

            let bytes = download_resp.bytes().await?;
            let current_exe = std::env::current_exe()?;
            let parent = current_exe.parent().unwrap_or(std::path::Path::new("."));
            let temp_path = parent.join(format!(".tylluan-update-{}", std::process::id()));

            // Extract binary from tarball to temp, then atomic rename
            {
                let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(&bytes[..]));
                let exe_name = current_exe.file_name().and_then(|n| n.to_str()).unwrap_or("tylluan");
                let found = archive.entries()?.find_map(|entry| {
                    let mut entry = entry.ok()?;
                    let name = entry.path().ok()?;
                    let fname = name.file_name()?.to_str()?;
                    // Match: same filename, or tylluan updating from tylluan-cli, or tylluan-cli/tylluan-nexus
                    if fname == exe_name || (exe_name == "tylluan" && fname == "tylluan-cli") || fname == "tylluan-nexus" {
                        entry.unpack(&temp_path).ok()?;
                        Some(())
                    } else {
                        None
                    }
                });
                if found.is_none() {
                    println!("❌ Could not find '{}' in archive.", exe_name);
                    println!("   Manual download: https://github.com/{}/releases", repo);
                    return Ok(());
                }
            }

            if !temp_path.exists() {
                println!("❌ Could not extract binary from archive.");
                println!("   Manual download: https://github.com/{}/releases", repo);
                return Ok(());
            }

            // Atomic replace: rename temp -> target
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&temp_path, std::fs::Permissions::from_mode(0o755))?;
            }
            match std::fs::rename(&temp_path, &current_exe) {
                Ok(()) => {
                    println!("✅ Updated to Tylluan v{}", latest_ver);
                }
                Err(e) => {
                    // Windows: can't rename over running exe. Place beside it.
                    let fallback = parent.join(format!("tylluan-v{}{}", latest_ver, std::env::consts::EXE_SUFFIX));
                    std::fs::rename(&temp_path, &fallback)?;
                    println!("✅ Downloaded Tylluan v{} to {}", latest_ver, fallback.display());
                    println!("   Replace {} manually with the new binary.", current_exe.display());
                    println!("   Error was: {}", e);
                }
            }
            println!("   Restart the kernel with 'tylluan start' for changes to take effect.");
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
        _ => format!("{}-{}", arch, os),
    }
}
