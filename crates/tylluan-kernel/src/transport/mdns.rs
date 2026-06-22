//! # mDNS ZeroConf Discovery
//!
//! Advertises the TylluanNexus Universal Gateway on the local network.
//! Allows IDEs to discover the gateway as `tylluan-nexus-o3.local`.

use mdns_sd::{ServiceDaemon, ServiceInfo, ServiceEvent};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, error};
use crate::config::TylluanConfig;
use crate::federation::FederationPeer;

/// Start the mDNS advertiser background task.
pub fn start_mdns_advertiser(port: u16) {
    let daemon = match ServiceDaemon::new() {
        Ok(d) => d,
        Err(e) => {
            error!("❌ mDNS: Failed to create daemon: {}", e);
            return;
        }
    };

    let service_type = "_mcp-gateway._tcp.local.";
    let instance_name = "tylluan-nexus-o3";
    let host_name = "tylluan-nexus-o3.local.";
    
    let mut properties = HashMap::new();
    properties.insert("version".to_string(), env!("CARGO_PKG_VERSION").to_string());
    properties.insert("path".to_string(), "/sse".to_string());
    properties.insert("mesh".to_string(), "sovereign".to_string());

    let my_service = ServiceInfo::new(
        service_type,
        instance_name,
        host_name,
        "0.0.0.0", // Let it bind to all
        port,
        Some(properties),
    ).expect("Failed to create service info");

    if let Err(e) = daemon.register(my_service) {
        error!("❌ mDNS: Failed to register service: {}", e);
    } else {
        info!("📢 mDNS: Gateway discovery active: 'tylluan-nexus-o3.local' on port {}", port);
    }
}

/// Start the mDNS discovery background task.
/// Browses for `_mcp-gateway._tcp.local.` services on the LAN and auto-registers
/// any discovered TylluanNexus peers (mesh=sovereign) as federation peers.
pub fn start_mdns_discovery(
    own_port: u16,
    config: Arc<RwLock<TylluanConfig>>,
    config_path: Option<PathBuf>,
) {
    let daemon = match ServiceDaemon::new() {
        Ok(d) => d,
        Err(e) => {
            error!("❌ mDNS Discovery: Failed to create daemon: {}", e);
            return;
        }
    };

    let service_type = "_mcp-gateway._tcp.local.";
    let receiver = match daemon.browse(service_type) {
        Ok(r) => r,
        Err(e) => {
            error!("❌ mDNS Discovery: Failed to start browse: {}", e);
            return;
        }
    };

    std::thread::spawn(move || {
        info!("🔍 mDNS Discovery: Scanning for TylluanNexus peers on {}...", service_type);
        for event in receiver {
            if let ServiceEvent::ServiceResolved(info) = event {
                let mesh = info.get_property_val_str("mesh").unwrap_or("");
                if mesh != "sovereign" {
                    continue;
                }

                let port = info.get_port();
                if port == own_port {
                    continue;
                }

                let ip = info.get_addresses_v4().iter().next()
                    .map(|a| a.to_string())
                    .unwrap_or_else(|| info.get_hostname().to_string());
                let name = info.get_fullname().trim_end_matches('.');
                let path = info.get_property_val_str("path").unwrap_or("/sse");

                info!("🔍 mDNS: Discovered peer {} at {}:{}{}", name, ip, port, path);

                let peer_name = format!("mdns-auto-{}", name.replace('.', "-"));
                let url = if port == 443 {
                    format!("https://{}:{}{}", ip, port, path)
                } else {
                    format!("http://{}:{}{}", ip, port, path)
                };

                let mut cfg = config.blocking_write();
                let exists = cfg.federation_peers.iter().any(|p| p.url == url);
                if !exists {
                    cfg.federation_peers.push(FederationPeer {
                        name: peer_name.clone(),
                        url,
                        token: String::new(), // no shared secret — must be set manually before sync
                        last_sync: None,
                        approved: false, // requires human approval + token before any sync
                    });
                    info!("🔍 mDNS: Discovered peer '{}' — PENDING APPROVAL (set token + approve before sync)", peer_name);
                    if let Some(ref path_buf) = config_path
                        && let Err(e) = crate::config::persist_federation_peers(&cfg, path_buf) {
                            error!("❌ mDNS: Failed to persist federation peers: {}", e);
                        }
                }
            }
        }
    });
}
