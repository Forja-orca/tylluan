//! D-94: single-instance bookkeeping for `active_port.json` and the
//! "shut down the previous kernel" handshake.
//!
//! Root cause (B-31, 2026-11-01): the file lived in `./data` of the **cwd** while
//! the PID lived in the instance DATA_DIR, so a test kernel with its own DATA_DIR
//! read a stranger's port. When the stale port happened to equal the one about to
//! be bound, the new kernel sent `POST /api/v1/admin/shutdown` to *itself* as soon
//! as its listener came up: a crash-restart loop that exited with code 0 and no
//! error in the log.

use std::path::{Path, PathBuf};

pub const ACTIVE_PORT_FILE: &str = "active_port.json";
pub const PID_FILE: &str = "tylluan-nexus.pid";

/// Same rule as `main::resolve_pid_data_dir`, keyed by the port this process
/// binds. `TYLLUAN_DATA_DIR` wins (that is what test kernels isolate with).
pub fn data_dir(port: u16) -> PathBuf {
    if let Ok(dir) = std::env::var("TYLLUAN_DATA_DIR") {
        return PathBuf::from(dir);
    }
    if port != 47004 {
        return PathBuf::from(format!("./data-port-{port}"));
    }
    PathBuf::from("./data")
}

pub fn active_port_path(data_dir: &Path) -> PathBuf {
    data_dir.join(ACTIVE_PORT_FILE)
}

pub fn read_active_port(path: &Path) -> Option<u16> {
    let content = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&content).ok()?;
    let port = value.get("port")?.as_u64()?;
    u16::try_from(port).ok().filter(|p| *p != 0)
}

pub fn write_active_port(path: &Path, port: u16) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let payload = serde_json::json!({ "port": port });
    let text = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string());
    std::fs::write(path, text)
}

pub fn read_pid(path: &Path) -> Option<u32> {
    let content = std::fs::read_to_string(path).ok()?;
    content.trim().parse::<u32>().ok().filter(|pid| *pid != 0)
}

/// True only when `pid` exists **and** is a tylluan-nexus process: a recycled
/// PID must not be mistaken for a kernel.
pub fn process_is_kernel_alive(pid: u32) -> bool {
    let mut sys = sysinfo::System::new();
    sys.refresh_processes(
        sysinfo::ProcessesToUpdate::Some(&[sysinfo::Pid::from_u32(pid)]),
        true,
    );
    sys.process(sysinfo::Pid::from_u32(pid))
        .map(|p| {
            let name = p.name().to_string_lossy().to_lowercase();
            name.contains("tylluan-nexus") || name.contains("tylluan_nexus")
        })
        .unwrap_or(false)
}

/// D-94: whom (if anyone) to ask for a clean shutdown before binding.
///
/// - no file, zero or unparsable port → nobody;
/// - the recorded port **is ours** → nobody: that is a stale file from our own
///   crashed instance, and the old code ended up asking the new kernel to die
///   (B-31);
/// - no live kernel behind the DATA_DIR PID file → nobody: a dead PID leaves a
///   stale port that must not be trusted;
/// - otherwise → that port.
pub fn shutdown_target(
    old_port: Option<u16>,
    own_port: u16,
    previous_kernel_alive: bool,
) -> Option<u16> {
    let old_port = old_port?;
    if old_port == 0 || old_port == own_port {
        return None;
    }
    if !previous_kernel_alive {
        return None;
    }
    Some(old_port)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The B-31 regression: a stale `active_port.json` pointing at **our own**
    /// port must produce zero shutdown requests, even though the old code asked
    /// because the port was inside `own_port_range`.
    #[test]
    fn d94_stale_file_pointing_at_our_own_port_asks_nobody() {
        assert_eq!(shutdown_target(Some(47099), 47099, true), None);
        assert_eq!(shutdown_target(Some(47099), 47099, false), None);
    }

    #[test]
    fn d94_other_port_without_a_live_kernel_asks_nobody() {
        assert_eq!(shutdown_target(Some(47100), 47099, false), None);
        assert_eq!(shutdown_target(None, 47099, true), None);
        assert_eq!(shutdown_target(Some(0), 47099, true), None);
    }

    #[test]
    fn d94_live_previous_kernel_on_another_port_is_asked() {
        assert_eq!(shutdown_target(Some(47100), 47099, true), Some(47100));
    }

    #[test]
    fn d94_active_port_lives_in_the_instance_data_dir() {
        let dir = PathBuf::from("/tmp/instance-data");
        assert_eq!(
            active_port_path(&dir),
            dir.join("active_port.json"),
            "must not fall back to ./data of the cwd"
        );
        assert_eq!(data_dir(47004), PathBuf::from("./data"));
        assert_eq!(data_dir(47180), PathBuf::from("./data-port-47180"));
    }

    #[test]
    fn d94_active_port_roundtrip() {
        let base = std::env::temp_dir().join(format!("tylluan-d94-{}", std::process::id()));
        let path = active_port_path(&base);
        write_active_port(&path, 47123).expect("write");
        assert_eq!(read_active_port(&path), Some(47123));
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(read_active_port(&path), None);
        std::fs::write(&path, r#"{"port": 70000}"#).unwrap();
        assert_eq!(read_active_port(&path), None, "out of u16 range");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn d94_pid_file_parsing() {
        let base = std::env::temp_dir().join(format!("tylluan-d94pid-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let pid = base.join(PID_FILE);
        std::fs::write(&pid, " 4242 \n").unwrap();
        assert_eq!(read_pid(&pid), Some(4242));
        std::fs::write(&pid, "garbage").unwrap();
        assert_eq!(read_pid(&pid), None);
        std::fs::write(&pid, "0").unwrap();
        assert_eq!(read_pid(&pid), None);
        let _ = std::fs::remove_dir_all(&base);
    }
}
