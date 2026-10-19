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

#[derive(Debug, Clone)]
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
                    routing_table.insert(&sp.node_id, sp.addr, sp.capabilities.clone(), None);
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
                            routing_table.insert(&p.node_id, addr, vec!["mesh".into()], None);
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
                let pid = format!("seed-{seed}");
                routing_table.insert(&pid, addr, vec!["seed".into()], None);
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
    ///
    /// Written against mainline 2.x's low-level synchronous API (`Dht::client`,
    /// `announce_peer`, `get_peers`) — the 0.x `ClientBuilder` with
    /// `on_announce`/`on_query` callbacks this code originally used no longer
    /// exists. mainline 2.x iteration is blocking, so the whole exchange runs
    /// in `spawn_blocking`: peers are drained within a 5 s window (the query's
    /// channel disconnects when the DHT actor completes it, and `shutdown()`
    /// forces that disconnect if the window closes first), and the whole call
    /// is hard-capped at 10 s in case the actor stalls on an unreachable
    /// network — then bootstrap degrades to the manual seed layer instead of
    /// hanging.
    #[cfg(feature = "mainline-dht")]
    async fn bootstrap_mainline(local_node_id: &str, listen_port: u16) -> anyhow::Result<Vec<DiscoveredPeer>> {
        use std::sync::mpsc;
        use std::time::{Duration, Instant};

        let node_id = local_node_id.to_string();
        let joined = tokio::time::timeout(
            Duration::from_secs(10),
            tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<std::net::SocketAddr>> {
                // Client mode starts bootstrapped from the crate's default
                // mainline nodes on its own.
                let mut dht = mainline::Dht::client()
                    .map_err(|e| anyhow::anyhow!("mainline DHT client: {e}"))?;

                let hash = sha256_of(&node_id);
                let info_hash = mainline::Id::from_bytes(&hash[..20])
                    .map_err(|e| anyhow::anyhow!("mainline info hash: {e}"))?;

                // Advertise ourselves so other Tylluan nodes can find us (BEP 5).
                if let Err(e) = dht.announce_peer(info_hash, Some(listen_port)) {
                    tracing::warn!("DHT: mainline announce failed: {e}");
                }

                // Collect peers found for our info hash. The iterator blocks
                // per batch, so it runs on a drain thread whose sender is
                // dropped either when the query completes or when our window
                // ends and `rx` goes out of scope below.
                let mut peers: Vec<std::net::SocketAddr> = Vec::new();
                match dht.get_peers(info_hash) {
                    Ok(pending) => {
                        let (tx, rx) = mpsc::channel();
                        std::thread::spawn(move || {
                            for batch in pending {
                                if tx.send(batch).is_err() {
                                    break;
                                }
                            }
                        });
                        let deadline = Instant::now() + Duration::from_secs(5);
                        loop {
                            let remaining = deadline.saturating_duration_since(Instant::now());
                            if remaining.is_zero() {
                                break;
                            }
                            match rx.recv_timeout(remaining) {
                                Ok(batch) => peers.extend(batch),
                                Err(mpsc::RecvTimeoutError::Timeout) => break,
                                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                            }
                        }
                    }
                    Err(e) => tracing::warn!("DHT: mainline get_peers failed: {e}"),
                }

                // Stops the DHT actor; also disconnects the query channel so
                // the drain thread above always terminates.
                let _ = dht.shutdown();
                Ok(peers)
            }),
        )
        .await;

        // joined nests three Results: Elapsed (timeout) / JoinError (task) /
        // the closure's own anyhow result — destructure all of them.
        let peers = match joined {
            Ok(Ok(Ok(peers))) => peers,
            Ok(Ok(Err(e))) => return Err(e),
            Ok(Err(join_err)) => {
                return Err(anyhow::anyhow!("mainline bootstrap task join failed: {join_err}"));
            }
            Err(_) => {
                return Err(anyhow::anyhow!(
                    "mainline bootstrap exceeded its 10 s budget (unreachable network?)"
                ));
            }
        };

        Ok(peers
            .into_iter()
            .map(|addr| DiscoveredPeer {
                node_id: format!("dht-{addr}"),
                addr: addr.to_string(),
                source: PeerSource::MainlineDht,
            })
            .collect())
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
fn sha256_of(input: &str) -> [u8; 32] {
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
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn tmp_dir(label: &str) -> std::path::PathBuf {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("tylluan_{label}_{id}"))
    }

    fn test_identity() -> NodeIdentity {
        let dir = tmp_dir("bootstrap_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("identity.key");
        NodeIdentity::load_or_create(&path).expect("should generate")
    }

    #[tokio::test]
    async fn test_persistence_roundtrip() {
        let dir = tmp_dir("dht_persist");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("dht_peers.json");

        let identity = test_identity();
        let mut rt = RoutingTable::new(identity.node_id().to_string());

        let peer_path = tmp_dir("dht_test_p2");
        let peer_id = crate::identity::NodeIdentity::load_or_create(
            &peer_path
        ).expect("should generate").node_id().to_string();

        rt.insert(
            &peer_id,
            "192.168.1.42:3000".parse::<SocketAddr>().unwrap(),
            vec!["mesh".into()],
            None,
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

    // With `mainline-dht` the bootstrap reaches the public BitTorrent DHT, so the
    // "no peers" assertion depends on the machine's internet access.
    #[cfg_attr(feature = "mainline-dht", ignore = "contacts the public mainline DHT")]
    #[tokio::test]
    async fn test_bootstrap_empty_config() {
        let identity = test_identity();
        let mut rt = RoutingTable::new(identity.node_id().to_string());
        let config = BootstrapConfig {
            dht_peers_path: tmp_dir("boot_empty").join("dht_peers.json"),
            ..Default::default()
        };
        let peers = config.bootstrap(&mut rt).await.expect("bootstrap should not fail");
        assert!(peers.is_empty());
    }
}
