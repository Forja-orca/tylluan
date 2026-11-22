

/// Kill orphan Python guild processes from a previous kernel session that crashed.
/// Uses `sysinfo` to find Python processes whose command line contains "guilds.core."
/// and whose parent PID matches the stale kernel PID (or parent is already dead).
pub fn cleanup_orphan_guilds(stale_kernel_pid: u32) {
    use sysinfo::{System, Pid};

    let mut sys = System::new();
    // D-119: con el refresh normal `cmd()` llega vacía y esta limpieza de huérfanos
    // no encontraba NUNCA un huérfano (el `cmd.contains("guilds.")` era siempre falso).
    sys.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        true,
        sysinfo::ProcessRefreshKind::everything(),
    );

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

/// Directorios residuales a barrer, dentro del **DATA_DIR efectivo** (D4981).
///
/// Antes eran `data/...` relativos al cwd: una instancia con `TYLLUAN_DATA_DIR`
/// barría (o dejaba de barrer) carpetas del directorio equivocado. Misma clase que
/// D4978/D4979/D4980.
pub fn residual_dirs() -> Vec<std::path::PathBuf> {
    ["test_stress", "uploads", "bench"]
        .iter()
        .map(|n| crate::config::data_file(n))
        .collect()
}

/// Clean up residual data directories from previous sessions or stress tests.
/// This prevents accumulation of "trash" nodes or temp files in sovereign hardware.
pub fn cleanup_residual_data() {
    for path in residual_dirs() {
        if path.exists() {
            tracing::info!("🧹 Garbage Collection: Removing residual directory: {}", path.display());
            let _ = std::fs::remove_dir_all(&path);
        }
    }
}

#[cfg(test)]
mod residual_dirs_tests {

    /// D4981: el barrido mira el DATA_DIR efectivo, no el cwd.
    /// MUTACION: volver a `["data/…"]` deja este test en rojo.
    #[test]
    fn residual_dirs_siguen_al_data_dir() {
        let _g = crate::test_env::serial();
        let previo = std::env::var("TYLLUAN_DATA_DIR").ok();
        unsafe { std::env::set_var("TYLLUAN_DATA_DIR", "D:/tmp/instancia-clean") };
        let dirs = super::residual_dirs();
        match previo {
            Some(p) => unsafe { std::env::set_var("TYLLUAN_DATA_DIR", p) },
            None => unsafe { std::env::remove_var("TYLLUAN_DATA_DIR") },
        }
        assert_eq!(dirs.len(), 3);
        for d in &dirs {
            assert!(
                d.starts_with("D:/tmp/instancia-clean"),
                "tiene que colgar del DATA_DIR: {}",
                d.display()
            );
        }
    }
}