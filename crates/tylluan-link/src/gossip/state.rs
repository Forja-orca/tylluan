use std::collections::HashMap;
use std::time::Duration;
use crate::dht::RoutingTable;

pub const DEFAULT_GOSSIP_INTERVAL: Duration = Duration::from_secs(30);
pub const DEFAULT_FANOUT: usize = 3;
pub const DEFAULT_MAX_PEER_CURSORS: usize = 100;

#[derive(Debug, Clone)]
pub struct GossipConfig {
    pub enabled: bool,
    pub interval_secs: u64,
    pub fanout: usize,
    pub max_peer_cursors: usize,
}

impl Default for GossipConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_secs: DEFAULT_GOSSIP_INTERVAL.as_secs(),
            fanout: DEFAULT_FANOUT,
            max_peer_cursors: DEFAULT_MAX_PEER_CURSORS,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GossipState {
    pub local_node_id: String,
    pub local_clock: u64,
    pub peer_cursors: HashMap<String, u64>,
}

impl GossipState {
    pub fn new(local_node_id: String) -> Self {
        Self {
            local_node_id,
            local_clock: 0,
            peer_cursors: HashMap::new(),
        }
    }

    pub fn tick(&mut self) -> u64 {
        self.local_clock += 1;
        self.local_clock
    }

    pub fn update_cursor(&mut self, peer_id: &str, clock: u64) {
        if self.peer_cursors.len() >= DEFAULT_MAX_PEER_CURSORS {
            self.peer_cursors.clear();
        }
        self.peer_cursors
            .entry(peer_id.to_string())
            .and_modify(|c| *c = (*c).max(clock))
            .or_insert(clock);
    }

    pub fn last_known(&self, peer_id: &str) -> u64 {
        self.peer_cursors.get(peer_id).copied().unwrap_or(0)
    }

    pub fn select_peers(&self, routing_table: &RoutingTable, fanout: usize) -> Vec<crate::dht::KBucketEntry> {
        let mut candidates: Vec<crate::dht::KBucketEntry> = routing_table
            .all_peers()
            .into_iter()
            .filter(|e| e.node_id != self.local_node_id)
            .collect();

        if candidates.len() <= fanout {
            return candidates;
        }

        use rand::seq::SliceRandom;
        let mut rng = rand::thread_rng();
        candidates.shuffle(&mut rng);
        candidates.truncate(fanout);
        candidates
    }
}

pub struct GossipEngine {
    pub state: GossipState,
    pub config: GossipConfig,
}

impl GossipEngine {
    pub fn new(local_node_id: String, config: GossipConfig) -> Self {
        Self {
            state: GossipState::new(local_node_id),
            config,
        }
    }

    pub fn select_gossip_targets(&self, routing_table: &RoutingTable) -> Vec<crate::dht::KBucketEntry> {
        self.state.select_peers(routing_table, self.config.fanout)
    }

    pub fn record_peer_clock(&mut self, peer_id: &str, clock: u64) {
        self.state.update_cursor(peer_id, clock);
    }

    pub fn advance_clock(&mut self) -> u64 {
        self.state.tick()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dht::RoutingTable;

    fn node_id_from_bytes(input: &[u8]) -> String {
        use sha2::{Sha256, Digest};
        let hash = Sha256::digest(input);
        hex::encode(&hash[..16])
    }

    #[test]
    fn test_gossip_state_tick() {
        let mut state = GossipState::new("local".into());
        assert_eq!(state.local_clock, 0);
        assert_eq!(state.tick(), 1);
        assert_eq!(state.tick(), 2);
    }

    #[test]
    fn test_update_cursor() {
        let mut state = GossipState::new("local".into());
        state.update_cursor("peer1", 5);
        assert_eq!(state.last_known("peer1"), 5);
        state.update_cursor("peer1", 3);
        assert_eq!(state.last_known("peer1"), 5);
    }

    #[test]
    fn test_select_peers_excludes_self() {
        let local = node_id_from_bytes(b"local");
        let state = GossipState::new(local.clone());
        let rt = RoutingTable::new(local.clone());
        let peers = state.select_peers(&rt, 5);
        assert!(peers.is_empty());
    }

    #[test]
    fn test_select_peers_respects_fanout() {
        let local = node_id_from_bytes(b"local");
        let state = GossipState::new(local.clone());
        let mut rt = RoutingTable::new(local.clone());
        for i in 0..10 {
            let pid = node_id_from_bytes(format!("peer{}", i).as_bytes());
            rt.insert(&pid, format!("192.168.1.{}:3000", i + 1).parse().unwrap(), vec!["mesh".into()]);
        }
        let peers = state.select_peers(&rt, 3);
        assert_eq!(peers.len(), 3);
    }

    #[test]
    fn test_gossip_engine_new() {
        let engine = GossipEngine::new("local".into(), GossipConfig::default());
        assert_eq!(engine.state.local_node_id, "local");
        assert_eq!(engine.config.fanout, 3);
    }
}
