use clap::{Parser, Subcommand};
use anyhow::{Result, Context};
use std::process::Command;
use std::path::PathBuf;
use sysinfo::System;

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
            // Simple ping to health endpoint
            let client = reqwest::Client::new();
            match client.get("http://127.0.0.1:3000/health").send().await {
                Ok(resp) if resp.status().is_success() => {
                    let json: serde_json::Value = resp.json().await?;
                    println!("✅ Hub is OPERATIONAL (v{})", json["version"]);
                    println!("📊 Uptime: {}s", json["uptime_seconds"]);
                }
                _ => println!("❌ Hub is OFFLINE or unreachable on port 3000."),
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
    }

    Ok(())
}

fn find_kernel_exe() -> Result<PathBuf> {
    // Strategy: Look in current dir, then target/release
    let names = vec!["tylluan-nexus.exe", "tylluan-nexus"];
    let paths = vec![
        PathBuf::from("."),
        PathBuf::from("target/release"),
        PathBuf::from("target/debug"),
    ];

    for path in paths {
        for name in &names {
            let full = path.join(name);
            if full.exists() {
                return Ok(full);
            }
        }
    }

    // Try to find it in the same directory as the CLI
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            for name in &names {
                let full = dir.join(name);
                if full.exists() {
                    return Ok(full);
                }
            }
        }
    }

    Err(anyhow::anyhow!("Could not find tylluan-nexus binary. Ensure you have built the kernel first."))
}
