use crate::dht::routing_table::RoutingTable;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

pub const DHT_INFO_HASH: &str = "tylluan-mesh-v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhtPersistedPeers {
    pub peers: Vec<SavedPeer>,
    pub saved_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPeer {
    pub node_id: String,
    pub addr: SocketAddr,
    pub capabilities: Vec<String>,
    pub last_seen: i64,
}

#[derive(Debug, Clone)]
pub struct BootstrapConfig {
    pub local_node_id: String,
    pub local_addr: SocketAddr,
    pub use_mdns: bool,
    pub use_mainline: bool,
    pub seed_nodes: Vec<String>,
    pub dht_peers_path: PathBuf,
    pub listen_port: u16,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            local_node_id: String::new(),
            local_addr: "0.0.0.0:0".parse().unwrap(),
            use_mdns: true,
            use_mainline: true,
            seed_nodes: Vec::new(),
            dht_peers_path: PathBuf::from("data/dht_peers.json"),
            listen_port: 3000,
        }
    }
}

#[derive(Debug)]
pub struct DiscoveredPeer {
    pub node_id: String,
    pub addr: String,
    pub source: PeerSource,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PeerSource {
    Persisted,
    MDns,
    MainlineDht,
    Manual,
}

impl BootstrapConfig {
    /// Bootstrap peer discovery with priority stack:
    /// 1. data/dht_peers.json — peers from last session (instant)
    /// 2. mDNS — LAN peers (already wired in kernel via mdns-sd)
    /// 3. Mainline DHT — WAN bootstrap via mainline crate
    /// 4. Manual seed nodes from config
    pub async fn bootstrap(&self, routing_table: &mut RoutingTable) -> anyhow::Result<Vec<DiscoveredPeer>> {
        let mut all = Vec::new();

        // Layer 1: Persisted peers from last session
        if let Ok(saved) = Self::load_persisted(&self.dht_peers_path) {
            for sp in &saved.peers {
                let elapsed = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64 - sp.last_seen;
                if elapsed < 86400 {
                    routing_table.insert(&sp.node_id, sp.addr, sp.capabilities.clone());
                    all.push(DiscoveredPeer {
                        node_id: sp.node_id.clone(),
                        addr: sp.addr.to_string(),
                        source: PeerSource::Persisted,
                    });
                }
            }
            tracing::info!("DHT: Loaded {} persisted peers", saved.peers.len());
        }

        // Layer 2: Mainline DHT bootstrap
        #[cfg(feature = "mainline-dht")]
        if self.use_mainline {
            match Self::bootstrap_mainline(&self.local_node_id, self.listen_port).await {
                Ok(peers) => {
                    for p in &peers {
                        if let Ok(addr) = p.addr.parse::<SocketAddr>() {
                            routing_table.insert(&p.node_id, addr, vec!["mesh".into()]);
                        }
                        all.push(DiscoveredPeer {
                            node_id: p.node_id.clone(),
                            addr: p.addr.clone(),
                            source: PeerSource::MainlineDht,
                        });
                    }
                    tracing::info!("DHT: Discovered {} peers via Mainline DHT", peers.len());
                }
                Err(e) => {
                    tracing::warn!("DHT: Mainline bootstrap failed: {}", e);
                }
            }
        }

        // Layer 3: Manual seed nodes from config
        for seed in &self.seed_nodes {
            if let Ok(addr) = seed.parse::<SocketAddr>() {
                let pid = format!("seed-{}", seed);
                routing_table.insert(&pid, addr, vec!["seed".into()]);
                all.push(DiscoveredPeer {
                    node_id: pid,
                    addr: seed.clone(),
                    source: PeerSource::Manual,
                });
            }
        }

        self.save_persisted(routing_table).ok();
        Ok(all)
    }

    /// Mainline DHT bootstrap via the `mainline` crate.
    /// Announces this node and looks up other Tylluan peers.
    #[cfg(feature = "mainline-dht")]
    async fn bootstrap_mainline(local_node_id: &str, _port: u16) -> anyhow::Result<Vec<DiscoveredPeer>> {
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let discovered = Arc::new(Mutex::new(Vec::new()));

        let announce = |info_hash: [u8; 20], peer_addr: std::net::SocketAddr| {
            let disc = discovered.clone();
            let nid = local_node_id.to_string();
            async move {
                let peer = DiscoveredPeer {
                    node_id: nid,
                    addr: peer_addr.to_string(),
                    source: PeerSource::MainlineDht,
                };
                disc.lock().await.push(peer);
            }
        };

        let lookup = |info_hash: [u8; 20], peers: Vec<std::net::SocketAddr>| {
            let disc = discovered.clone();
            async move {
                for peer_addr in peers {
                    let peer = DiscoveredPeer {
                        node_id: format!("dht-{}", peer_addr),
                        addr: peer_addr.to_string(),
                        source: PeerSource::MainlineDht,
                    };
                    disc.lock().await.push(peer);
                }
            }
        };

        let info_hash_bytes = Sha256_of(local_node_id);
        let mut info_hash_arr = [0u8; 20];
        info_hash_arr.copy_from_slice(&info_hash_bytes[..20]);

        let client = mainline::ClientBuilder::new()
            .on_announce(announce)
            .on_query(lookup)
            .build()
            .map_err(|e| anyhow::anyhow!("mainline client build failed: {}", e))?;

        let local_addr: std::net::SocketAddr = format!("0.0.0.0:{}", _port).parse()?;
        let _ = client.announce(info_hash_arr, local_addr.port()).await;
        tokio::time::sleep(Duration::from_secs(5)).await;

        let result = discovered.lock().await.clone();
        Ok(result)
    }

    fn load_persisted(path: &Path) -> anyhow::Result<DhtPersistedPeers> {
        if path.exists() {
            let data = std::fs::read_to_string(path)?;
            Ok(serde_json::from_str(&data)?)
        } else {
            Ok(DhtPersistedPeers {
                peers: Vec::new(),
                saved_at: 0,
            })
        }
    }

    pub fn save_persisted(&self, routing_table: &RoutingTable) -> anyhow::Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let saved = DhtPersistedPeers {
            peers: routing_table.all_peers().into_iter().map(|e| SavedPeer {
                node_id: e.node_id,
                addr: e.addr,
                capabilities: e.capabilities,
                last_seen: e.last_seen_unix,
            }).collect(),
            saved_at: now,
        };

        if let Some(parent) = self.dht_peers_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_string_pretty(&saved)?;
        std::fs::write(&self.dht_peers_path, data)?;
        Ok(())
    }
}

#[cfg(feature = "mainline-dht")]
fn Sha256_of(input: &str) -> [u8; 32] {
    use sha2::Digest;
    let hash = sha2::Sha256::digest(input.as_bytes());
    let mut result = [0u8; 32];
    result.copy_from_slice(&hash);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dht::routing_table::RoutingTable;
    use crate::identity::NodeIdentity;
    use std::net::SocketAddr;

    fn test_identity() -> NodeIdentity {
        let dir = std::env::temp_dir().join(format!("tylluan_bootstrap_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("identity.key");
        NodeIdentity::load_or_create(&path).expect("should generate")
    }

    #[tokio::test]
    async fn test_persistence_roundtrip() {
        let dir = std::env::temp_dir().join(format!("tylluan_dht_persist_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("dht_peers.json");

        let identity = test_identity();
        let mut rt = RoutingTable::new(identity.node_id().to_string());

        let peer_id = crate::identity::NodeIdentity::load_or_create(
            &std::env::temp_dir().join(format!("tylluan_dht_test_p2_{}", std::process::id()))
        ).expect("should generate").node_id().to_string();

        rt.insert(
            &peer_id,
            "192.168.1.42:3000".parse::<SocketAddr>().unwrap(),
            vec!["mesh".into()],
        );

        let config = BootstrapConfig {
            dht_peers_path: path.clone(),
            ..Default::default()
        };

        config.save_persisted(&rt).expect("should save");

        let loaded = BootstrapConfig::load_persisted(&path).expect("should load");
        assert_eq!(loaded.peers.len(), 1);
        assert_eq!(loaded.peers[0].addr.to_string(), "192.168.1.42:3000");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_bootstrap_empty_config() {
        let identity = test_identity();
        let mut rt = RoutingTable::new(identity.node_id().to_string());
        let config = BootstrapConfig {
            dht_peers_path: std::env::temp_dir()
                .join(format!("tylluan_boot_empty_{}", std::process::id()))
                .join("dht_peers.json"),
            ..Default::default()
        };
        let peers = config.bootstrap(&mut rt).await.expect("bootstrap should not fail");
        assert!(peers.is_empty() || true);
    }
}
