use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::gossip::{GossipEngine, HardwareCaps};

/// Hardware + software capabilities reported by a peer in the mesh.
#[derive(Debug, Clone)]
pub struct CapabilityRecord {
    /// Reachable address (IP:port or .onion).
    pub addr: String,
    /// Hardware specs (RAM, GPU, load).
    pub hardware: HardwareCaps,
    /// Guild capabilities (e.g. "bash", "git", "vision").
    pub capabilities: Vec<String>,
    /// Gossip clock — higher = newer.
    pub clock: u64,
}

/// Tracks peer capabilities with TTL expiry.
///
/// Ingested from GossipEngine entries. Expired peers are removed by
/// `prune_expired()` which should be called periodically (e.g. every 60s)
/// alongside the gossip background task.
pub struct CapabilityRegistry {
    peers: HashMap<String, (CapabilityRecord, Instant)>,
    ttl: Duration,
}

impl CapabilityRegistry {
    /// Create a new registry with the given TTL.
    /// Peers that haven't been heard from in `ttl` are pruned by `prune_expired()`.
    pub fn new(ttl: Duration) -> Self {
        Self {
            peers: HashMap::new(),
            ttl,
        }
    }

    /// Ingest a single peer entry (typically from a GossipEntry or sync result).
    /// Only updates if the clock is newer than the stored record.
    pub fn ingest(&mut self, node_id: &str, addr: &str, hardware: &HardwareCaps, capabilities: &[String], clock: u64) {
        let now = Instant::now();
        match self.peers.get(node_id) {
            Some((existing, _)) if existing.clock >= clock => return,
            _ => {}
        }
        self.peers.insert(
            node_id.to_string(),
            (
                CapabilityRecord {
                    addr: addr.to_string(),
                    hardware: hardware.clone(),
                    capabilities: capabilities.to_vec(),
                    clock,
                },
                now,
            ),
        );
    }

    /// Bulk-ingest all entries currently stored in a GossipEngine.
    pub fn ingest_from_engine(&mut self, engine: &GossipEngine) {
        for entry in engine.state.all_entries() {
            self.ingest(
                &entry.node_id,
                &entry.addr,
                &entry.hardware,
                &entry.capabilities,
                entry.clock,
            );
        }
    }

    /// Remove all peers whose last update is older than TTL.
    /// Returns the number of pruned peers.
    pub fn prune_expired(&mut self) -> usize {
        let cutoff = Instant::now() - self.ttl;
        let before = self.peers.len();
        self.peers.retain(|_, (_, last_seen)| *last_seen >= cutoff);
        before - self.peers.len()
    }

    /// Look up a peer by node_id.
    pub fn get_peer(&self, node_id: &str) -> Option<&(CapabilityRecord, Instant)> {
        self.peers.get(node_id)
    }

    /// Iterate over all known peers.
    pub fn all_peers(&self) -> impl Iterator<Item = (&String, &(CapabilityRecord, Instant))> {
        self.peers.iter()
    }

    /// Number of peers currently tracked.
    pub fn len(&self) -> usize {
        self.peers.len()
    }

    /// Returns true if no peers are tracked.
    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gossip::{GossipConfig, GossipEngine};

    #[test]
    fn test_capability_registry_new_is_empty() {
        let reg = CapabilityRegistry::new(Duration::from_secs(300));
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn test_capability_registry_ingest_and_lookup() {
        let mut reg = CapabilityRegistry::new(Duration::from_secs(300));
        let hw = HardwareCaps { ram_mb: 4096, has_gpu: false, load_avg: 0.5, supports_p2p: false, tcp_port: None };

        reg.ingest("node-a", "10.0.0.1:9000", &hw, &["bash".into(), "git".into()], 5);
        assert_eq!(reg.len(), 1);

        let (record, _) = reg.get_peer("node-a").unwrap();
        assert_eq!(record.addr, "10.0.0.1:9000");
        assert_eq!(record.hardware.ram_mb, 4096);
        assert_eq!(record.capabilities, vec!["bash", "git"]);
    }

    #[test]
    fn test_capability_registry_stale_clock_ignored() {
        let mut reg = CapabilityRegistry::new(Duration::from_secs(300));
        let hw = HardwareCaps { ram_mb: 4096, has_gpu: false, load_avg: 0.5, supports_p2p: false, tcp_port: None };

        reg.ingest("node-a", "10.0.0.1:9000", &hw, &["bash".into()], 10);
        // stale write with lower clock
        reg.ingest("node-a", "10.0.0.1:9001", &hw, &["old".into()], 5);

        let (record, _) = reg.get_peer("node-a").unwrap();
        assert_eq!(record.addr, "10.0.0.1:9000", "stale write must not overwrite");
        assert_eq!(record.clock, 10);
    }

    #[test]
    fn test_capability_registry_prune_expired() {
        // Use a TTL of 0 so entries are immediately expired
        let mut reg = CapabilityRegistry::new(Duration::from_secs(0));
        let hw = HardwareCaps::default();

        reg.ingest("node-a", "10.0.0.1:9000", &hw, &[], 1);
        reg.ingest("node-b", "10.0.0.2:9000", &hw, &[], 2);

        // Sleep a tiny bit so Instant::now advances past TTL=0
        std::thread::sleep(std::time::Duration::from_millis(10));
        let pruned = reg.prune_expired();
        assert!(pruned >= 1, "should prune at least 1 expired peer, got {pruned}");
        assert!(reg.is_empty(), "all peers should be pruned with TTL=0");
    }

    #[tokio::test]
    async fn test_capability_registry_ingest_from_engine() {
        let mut engine = GossipEngine::new("self".into(), GossipConfig::default());
        engine.advance_clock();

        let entry = engine.local_entry("127.0.0.1:9000", vec!["bash".into()], HardwareCaps { ram_mb: 2048, has_gpu: false, load_avg: 0.3, supports_p2p: false, tcp_port: None });
        engine.store_entries(&[entry]);

        let mut reg = CapabilityRegistry::new(Duration::from_secs(300));
        reg.ingest_from_engine(&engine);
        assert!(!reg.is_empty(), "registry should have entries after ingestion");
        let (record, _) = reg.get_peer("self").unwrap();
        assert_eq!(record.hardware.ram_mb, 2048);
    }

    #[test]
    fn test_capability_registry_default_ttl() {
        let reg = CapabilityRegistry::new(Duration::from_secs(300));
        assert_eq!(reg.ttl, Duration::from_secs(300));
    }
}
