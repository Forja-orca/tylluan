

/// Kill orphan Python guild processes from a previous kernel session that crashed.
/// Uses `sysinfo` to find Python processes whose command line contains "guilds.core."
/// and whose parent PID matches the stale kernel PID (or parent is already dead).
pub fn cleanup_orphan_guilds(stale_kernel_pid: u32) {
    use sysinfo::{System, Pid};

    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let stale_pid = Pid::from_u32(stale_kernel_pid);
    let stale_is_kernel = sys.process(stale_pid)
        .map(|p| {
            let name = p.name().to_string_lossy().to_lowercase();
            name.contains("tylluan-nexus") || name.contains("tylluan_nexus")
        })
        .unwrap_or(false);

    // Only skip if the process is genuinely another kernel instance.
    // A plain `is_some()` check gets tricked by recycled PIDs (Windows reuses PIDs fast).
    if stale_is_kernel {
        tracing::warn!(
            "🧹 Previous kernel (PID {}) is still running — killing it to reclaim resources.",
            stale_kernel_pid
        );
        if let Some(p) = sys.process(stale_pid) {
            p.kill();
        }
        // Brief pause so the OS can release the TCP port before we try to bind.
        std::thread::sleep(std::time::Duration::from_millis(500));
    } else if sys.process(stale_pid).is_some() {
        tracing::info!(
            "🧹 PID {} is a different process (recycled PID) — ignoring stale PID file.",
            stale_kernel_pid
        );
    }

    let my_pid = sysinfo::Pid::from_u32(std::process::id());
    let mut killed = 0;

    for (pid, process) in sys.processes() {
        let cmd = process.cmd().iter()
            .map(|s| s.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(" ");

        // Strategy: Only target Python processes that look like TylluanNexus guilds
        if cmd.contains("guilds.") && (cmd.contains("python") || cmd.contains("Python")) {
            let parent_pid = process.parent();

            // Kill if: parent is the stale PID OR (parent is not me AND parent no longer exists)
            let is_orphan = if let Some(ppid) = parent_pid {
                ppid == stale_pid || (ppid != my_pid && sys.process(ppid).is_none())
            } else {
                true // No parent -> definitely orphan
            };

            if is_orphan {
                tracing::warn!("🧹 Killing orphan guild process: PID {} ({})", pid, cmd);
                process.kill();
                killed += 1;
            }
        }
    }

    if killed > 0 {
        tracing::info!("🧹 Cleaned up {} orphan guild processes.", killed);
    } else {
        tracing::info!("🧹 No orphan processes found. System clean.");
    }
}

/// Clean up residual data directories from previous sessions or stress tests.
/// This prevents accumulation of "trash" nodes or temp files in sovereign hardware.
pub fn cleanup_residual_data() {
    let targets = ["data/test_stress", "data/uploads", "data/bench"];
    for target in targets {
        let path = std::path::Path::new(target);
        if path.exists() {
            tracing::info!("🧹 Garbage Collection: Removing residual directory: {}", target);
            let _ = std::fs::remove_dir_all(path);
        }
    }
}
