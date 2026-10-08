//! Safe instance resolution for `tylluan-cli stop`.
//!
//! Auditoria externa hallazgo #3 (ALTO): the previous implementation swept every
//! process on the system and killed any whose name contained "tylluan-nexus" —
//! no PID file, no port/instance verification. With production + test + Docker
//! running simultaneously it could kill the wrong instance (a real incident
//! class in this project; see the kernel's anti_orphan_protection docstring,
//! which documents a test instance killing the live production kernel twice).
//!
//! This module resolves the SINGLE target instance the same way the kernel
//! resolves its own PID file at boot (resolve_pid_data_dir in
//! tylluan-kernel/src/main.rs) and NEVER kills by name-sweep: an ambiguous
//! multi-instance environment must be disambiguated explicitly with
//! --port / --data-dir, otherwise `stop` reports and refuses.

use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

/// Substring that identifies kernel processes. Kept for PID verification only —
/// never used to enumerate-and-kill.
pub const KERNEL_PROCESS_MARKER: &str = "tylluan-nexus";
/// Must match the file name the kernel writes at boot (anti_orphan_protection).
pub const PID_FILE_NAME: &str = "tylluan-nexus.pid";

/// Mirror of the kernel's `resolve_pid_data_dir` (tylluan-kernel/src/main.rs).
/// CLI precedence: explicit `--data-dir` > `TYLLUAN_DATA_DIR` env >
/// port-scoped `./data-port-{port}` (when --port != 47004) > `./data`.
/// MUST stay in sync with the kernel: boot writes `{dir}/tylluan-nexus.pid`.
pub fn resolve_pid_data_dir(data_dir_override: Option<&Path>, port: Option<u16>) -> PathBuf {
    if let Some(dir) = data_dir_override {
        return dir.to_path_buf();
    }
    if let Ok(dir) = std::env::var("TYLLUAN_DATA_DIR") {
        return PathBuf::from(dir);
    }
    match port {
        // 47004 is the kernel's default port (contract: single port).
        Some(port) if port != 47004 => PathBuf::from(format!("./data-port-{port}")),
        _ => PathBuf::from("./data"),
    }
}

/// Reads and parses a PID file. Returns None if missing, unreadable or garbage.
pub fn read_pid_file(path: &Path) -> Option<u32> {
    let raw = std::fs::read_to_string(path).ok()?;
    raw.trim().parse::<u32>().ok()
}

/// True if a process name identifies a TylluanNexus kernel.
pub fn is_kernel_process_name(name: &str) -> bool {
    name.contains(KERNEL_PROCESS_MARKER)
}

/// What `stop` should do, resolved as a pure decision so it is unit-testable
/// without real OS processes.
#[derive(Debug, PartialEq)]
pub enum StopAction {
    /// Stop exactly this PID (graceful shutdown first, hard kill as fallback).
    Target(u32),
    /// Multiple kernels running and no way to pick one — report, never kill.
    Ambiguous(Vec<u32>),
    /// No kernel running (or only a stale PID file).
    Nothing,
}

/// Resolve the stop target from the PID file contents and the process table.
/// `pid_alive` / `pid_is_kernel` abstract the process table for testing.
pub fn resolve_stop_target(
    pid_from_file: Option<u32>,
    pid_alive: impl Fn(u32) -> bool,
    pid_is_kernel: impl Fn(u32) -> bool,
    candidates: &[u32],
) -> Result<StopAction> {
    if let Some(pid) = pid_from_file {
        if !pid_alive(pid) {
            // Stale PID file (previous crash): fall back to candidate resolution.
            return resolve_stop_target(None, pid_alive, pid_is_kernel, candidates);
        }
        if !pid_is_kernel(pid) {
            // The PID file points at a LIVE process that is not a kernel —
            // most likely a recycled PID. Never kill it; refuse loudly.
            return Err(anyhow!(
                "PID file points to PID {pid}, which is alive but is not a {KERNEL_PROCESS_MARKER} process. Refusing to kill it. If this is a stale file from a recycled PID, remove it or pass --data-dir/--port explicitly."
            ));
        }
        return Ok(StopAction::Target(pid));
    }
    match candidates {
        [] => Ok(StopAction::Nothing),
        [only] => Ok(StopAction::Target(*only)),
        many => Ok(StopAction::Ambiguous(many.to_vec())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// env vars are process-global (edition 2024: set_var/remove_var unsafe);
    /// serialize every test that touches TYLLUAN_DATA_DIR (T999 lesson).
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    // ---- resolve_pid_data_dir ----

    #[test]
    fn default_data_dir_without_port_or_env() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::remove_var("TYLLUAN_DATA_DIR") };
        assert_eq!(resolve_pid_data_dir(None, None), PathBuf::from("./data"));
    }

    #[test]
    fn port_scoped_data_dir_for_non_default_port() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::remove_var("TYLLUAN_DATA_DIR") };
        assert_eq!(
            resolve_pid_data_dir(None, Some(47100)),
            PathBuf::from("./data-port-47100")
        );
        assert_eq!(
            resolve_pid_data_dir(None, Some(47004)),
            PathBuf::from("./data")
        );
    }

    #[test]
    fn env_var_wins_over_port() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("TYLLUAN_DATA_DIR", "/tmp/tylluan-test") };
        assert_eq!(
            resolve_pid_data_dir(None, Some(47100)),
            PathBuf::from("/tmp/tylluan-test")
        );
        unsafe { std::env::remove_var("TYLLUAN_DATA_DIR") };
    }

    #[test]
    fn explicit_flag_wins_over_env_and_port() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("TYLLUAN_DATA_DIR", "/tmp/tylluan-test") };
        assert_eq!(
            resolve_pid_data_dir(Some(Path::new("./other")), Some(47100)),
            PathBuf::from("./other")
        );
        unsafe { std::env::remove_var("TYLLUAN_DATA_DIR") };
    }

    // ---- read_pid_file ----

    #[test]
    fn reads_valid_pid() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("p.pid");
        std::fs::write(&f, "12345\n").unwrap();
        assert_eq!(read_pid_file(&f), Some(12345));
    }

    #[test]
    fn missing_or_garbled_pid_file_is_none() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(read_pid_file(&tmp.path().join("nope.pid")), None);
        let f = tmp.path().join("bad.pid");
        std::fs::write(&f, "not-a-pid").unwrap();
        assert_eq!(read_pid_file(&f), None);
    }

    // ---- is_kernel_process_name ----

    #[test]
    fn kernel_name_matching() {
        assert!(is_kernel_process_name("tylluan-nexus"));
        assert!(is_kernel_process_name("tylluan-nexus.exe"));
        assert!(!is_kernel_process_name("tylluan-cli.exe"));
        assert!(!is_kernel_process_name("notepad.exe"));
    }

    // ---- resolve_stop_target (pure decision) ----

    fn alive(p: u32) -> bool {
        p == 100 || p == 200
    }
    fn kernel(p: u32) -> bool {
        p == 100 || p == 200
    }

    #[test]
    fn pid_file_resolves_exact_target_even_with_siblings() {
        let action = resolve_stop_target(Some(100), alive, kernel, &[100, 200]).unwrap();
        assert_eq!(action, StopAction::Target(100));
    }

    #[test]
    fn pid_file_pointing_at_live_non_kernel_is_refused() {
        let alive_foreign = |p: u32| p == 300;
        let r = resolve_stop_target(Some(300), alive_foreign, kernel, &[]);
        assert!(r.is_err(), "must never kill a live non-kernel PID");
    }

    #[test]
    fn stale_pid_file_falls_back_to_candidates() {
        let r = resolve_stop_target(Some(999), alive, kernel, &[100]).unwrap();
        assert_eq!(r, StopAction::Target(100));
    }

    #[test]
    fn stale_pid_file_with_multiple_candidates_is_ambiguous() {
        let r = resolve_stop_target(Some(999), alive, kernel, &[100, 200]).unwrap();
        assert_eq!(r, StopAction::Ambiguous(vec![100, 200]));
    }

    #[test]
    fn no_pid_no_candidates_is_nothing() {
        let r = resolve_stop_target(None, alive, kernel, &[]).unwrap();
        assert_eq!(r, StopAction::Nothing);
    }

    #[test]
    fn single_candidate_without_pid_file_is_target() {
        let r = resolve_stop_target(None, alive, kernel, &[200]).unwrap();
        assert_eq!(r, StopAction::Target(200));
    }

    #[test]
    fn multiple_candidates_without_pid_file_never_kill() {
        let r = resolve_stop_target(None, alive, kernel, &[100, 200]).unwrap();
        assert_eq!(r, StopAction::Ambiguous(vec![100, 200]));
    }

    // ---- real process-table glue (Windows) ----
    // Spawns a real process NAMED like the kernel (a copy of cmd.exe placed as
    // tylluan-nexus.exe) and verifies the sysinfo glue end-to-end: detection by
    // name, pid_alive, pid_is_kernel, and the resolve decision against the real
    // table. Only ever kills its own spawned child.

    #[cfg(windows)]
    #[test]
    fn real_process_table_glue_windows() {
        use std::process::Stdio;
        use std::time::Duration;
        use sysinfo::System;

        let tmp = tempfile::tempdir().unwrap();
        let fake = tmp.path().join("tylluan-nexus.exe");
        std::fs::copy(r"C:\Windows\System32\cmd.exe", &fake).unwrap();

        let mut child = std::process::Command::new(&fake)
            .args(["/c", "ping", "-n", "15", "127.0.0.1"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id();

        let mut sys = System::new();
        let mut visible = false;
        for _ in 0..20 {
            sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
            if sys.process(sysinfo::Pid::from_u32(pid)).is_some() {
                visible = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(150));
        }
        assert!(visible, "spawned process must be visible to sysinfo");

        let proc = sys.process(sysinfo::Pid::from_u32(pid)).unwrap();
        assert!(is_kernel_process_name(&proc.name().to_string_lossy()));

        let pid_alive =
            |p: u32| sys.process(sysinfo::Pid::from_u32(p)).is_some();
        let pid_is_kernel = |p: u32| {
            sys.process(sysinfo::Pid::from_u32(p))
                .map(|pr| is_kernel_process_name(&pr.name().to_string_lossy()))
                .unwrap_or(false)
        };
        let action = resolve_stop_target(Some(pid), pid_alive, pid_is_kernel, &[pid]).unwrap();
        assert_eq!(action, StopAction::Target(pid));

        child.kill().expect("cleanup kill");
        let _ = child.wait();
    }
}
