//! Network tunnel manager.
//!
//! On Windows: auto-configures netsh portproxy so WSL2 clients can reach
//! the kernel at wsl_bridge_port → kernel_port.
//! On non-Windows: no-op (future: rathole sidecar).

#[cfg(target_os = "windows")]
use anyhow::Result;
#[cfg(target_os = "windows")]
use tracing::{info, warn};
use crate::config::TunnelConfig;

pub struct TunnelManager {
    config: TunnelConfig,
    kernel_port: u16,
    wsl_bridge_active: bool,
    windows_ip: Option<String>,
}

impl TunnelManager {
    pub fn new(config: TunnelConfig, kernel_port: u16) -> Self {
        Self { config, kernel_port, wsl_bridge_active: false, windows_ip: None }
    }

    /// Start all configured tunnels. Called once at kernel startup.
    pub fn start(&mut self) {
        if !self.config.enabled {
            return;
        }

        #[cfg(target_os = "windows")]
        if self.config.wsl_bridge {
            match self.setup_wsl_bridge() {
                Ok(()) => self.wsl_bridge_active = true,
                Err(e) => warn!("⚠️ WSL bridge setup failed: {}", e),
            }
        }
    }

    /// Remove all tunnel rules. Called on graceful shutdown.
    pub fn stop(&self) {
        if !self.wsl_bridge_active { return; }
        if !self.config.wsl_bridge_cleanup { return; }

        #[cfg(target_os = "windows")]
        self.teardown_wsl_bridge();
    }

    #[cfg(target_os = "windows")]
    fn setup_wsl_bridge(&mut self) -> Result<()> {
        let bridge_port = self.config.wsl_bridge_port;
        let kernel_port = self.kernel_port;

        info!(
            "🌉 WSL bridge: setting up portproxy {} → 127.0.0.1:{}",
            bridge_port, kernel_port
        );

        // Remove stale rule if it exists (ignore error — may not exist)
        let _ = std::process::Command::new("netsh")
            .args([
                "interface", "portproxy", "delete", "v4tov4",
                &format!("listenport={}", bridge_port),
                "listenaddress=0.0.0.0",
            ])
            .output();

        // Add the portproxy rule
        let out = std::process::Command::new("netsh")
            .args([
                "interface", "portproxy", "add", "v4tov4",
                &format!("listenport={}", bridge_port),
                "listenaddress=0.0.0.0",
                &format!("connectport={}", kernel_port),
                "connectaddress=127.0.0.1",
            ])
            .output()
            .map_err(|e| anyhow::anyhow!("netsh portproxy add failed: {}", e))?;

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            // Check if it failed due to missing admin rights
            if stderr.contains("acceso") || stderr.contains("access") 
                || stderr.contains("denied") || stderr.contains("denegado") {
                warn!(
                    "⚠️ WSL bridge requires admin rights. \
                     Run the kernel as Administrator or add the rule manually:\n  \
                     netsh interface portproxy add v4tov4 \
                     listenport={} listenaddress=0.0.0.0 \
                     connectport={} connectaddress=127.0.0.1",
                    bridge_port, kernel_port
                );
                return Ok(()); // Soft failure — don't crash the kernel
            }
            return Err(anyhow::anyhow!(
                "netsh portproxy add error: {}", stderr
            ));
        }

        // Add firewall rule for the bridge port (also needs admin)
        let fw_out = std::process::Command::new("netsh")
            .args([
                "advfirewall", "firewall", "add", "rule",
                "name=TylluanNexus-WSL",
                "dir=in",
                "action=allow",
                "protocol=TCP",
                &format!("localport={}", bridge_port),
            ])
            .output();

        // Detect the real Windows LAN IP for the banner
        let detected_ip = Self::detect_windows_lan_ip();
        let display_ip = detected_ip.as_deref().unwrap_or("WINDOWS_HOST_IP");

        match fw_out {
            Ok(o) if o.status.success() => {
                info!(
                    "🌉 WSL bridge active — from WSL use: \
                     http://{}:{}/messages",
                    display_ip, bridge_port
                );
            }
            _ => {
                info!(
                    "🌉 WSL bridge portproxy set (firewall rule skipped). \
                     From WSL: http://{}:{}/messages",
                    display_ip, bridge_port
                );
            }
        }

        self.windows_ip = detected_ip;

        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn detect_windows_lan_ip() -> Option<String> {
        let out = std::process::Command::new("ipconfig")
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);

        let mut fallback: Option<String> = None;

        for line in text.lines() {
            let line = line.trim();
            if (line.starts_with("Direcci") || line.starts_with("IPv4") 
                || line.contains("IPv4 Address"))
                && let Some(ip_part) = line.split(':').next_back() {
                    let ip = ip_part.trim().trim_end_matches('(').trim();
                    let ip = ip.split('(').next().unwrap_or(ip).trim();
                    if ip.starts_with("192.168.") {
                        return Some(ip.to_string());
                    }
                    if !ip.starts_with("127.") 
                        && !ip.starts_with("169.254.")
                        && !ip.is_empty()
                        && ip.contains('.') {
                        fallback = Some(ip.to_string());
                    }
                }
        }
        fallback
    }

    #[cfg(not(target_os = "windows"))]
    fn detect_windows_lan_ip() -> Option<String> {
        None
    }

    #[cfg(target_os = "windows")]
    fn teardown_wsl_bridge(&self) {
        let bridge_port = self.config.wsl_bridge_port;

        let _ = std::process::Command::new("netsh")
            .args([
                "interface", "portproxy", "delete", "v4tov4",
                &format!("listenport={}", bridge_port),
                "listenaddress=0.0.0.0",
            ])
            .output();

        let _ = std::process::Command::new("netsh")
            .args([
                "advfirewall", "firewall", "delete", "rule",
                "name=TylluanNexus-WSL",
            ])
            .output();

        info!("🌉 WSL bridge removed (port {})", bridge_port);
    }

    /// Returns the WSL-accessible URL for this kernel, if bridge is active.
    pub fn wsl_url(&self) -> Option<String> {
        if self.wsl_bridge_active {
            let ip = self.windows_ip.as_deref()
                .unwrap_or("WINDOWS_HOST_IP");
            Some(format!(
                "http://{}:{}/messages",
                ip, self.config.wsl_bridge_port
            ))
        } else {
            None
        }
    }
}

impl Drop for TunnelManager {
    fn drop(&mut self) {
        self.stop();
    }
}
