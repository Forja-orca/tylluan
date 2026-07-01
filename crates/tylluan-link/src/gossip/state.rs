use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use crate::dht::RoutingTable;
use crate::transport::{MeshTransport, TransportError};
use super::message::{GossipEntry, GossipMessage, HardwareCaps};
use serde::{Deserialize, Serialize};

pub const DEFAULT_GOSSIP_INTERVAL: Duration = Duration::from_secs(30);
pub const DEFAULT_FANOUT: usize = 3;
pub const DEFAULT_MAX_PEER_CURSORS: usize = 100;
pub const DEFAULT_MAX_ENTRIES: usize = 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipConfig {
    pub enabled: bool,
    pub interval_secs: u64,
    pub fanout: usize,
    pub max_peer_cursors: usize,
    pub max_entries: usize,
}

impl Default for GossipConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_secs: DEFAULT_GOSSIP_INTERVAL.as_secs(),
            fanout: DEFAULT_FANOUT,
            max_peer_cursors: DEFAULT_MAX_PEER_CURSORS,
            max_entries: DEFAULT_MAX_ENTRIES,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipState {
    pub local_node_id: String,
    pub local_clock: u64,
    pub peer_cursors: HashMap<String, u64>,
    #[serde(default)]
    pub seen_by_peers: HashMap<String, u64>,
    pub entries: HashMap<String, GossipEntry>,
    #[serde(default = "default_max_entries")]
    pub max_entries: usize,
    #[serde(default = "default_max_peer_cursors")]
    pub max_peer_cursors: usize,
}

fn default_max_entries() -> usize { DEFAULT_MAX_ENTRIES }
fn default_max_peer_cursors() -> usize { DEFAULT_MAX_PEER_CURSORS }

impl GossipState {
    pub fn new(local_node_id: String) -> Self {
        Self {
            local_node_id,
            local_clock: 0,
            peer_cursors: HashMap::new(),
            seen_by_peers: HashMap::new(),
            entries: HashMap::new(),
            max_entries: DEFAULT_MAX_ENTRIES,
            max_peer_cursors: DEFAULT_MAX_PEER_CURSORS,
        }
    }

    pub fn new_with_limits(local_node_id: String, max_entries: usize, max_peer_cursors: usize) -> Self {
        Self {
            local_node_id,
            local_clock: 0,
            peer_cursors: HashMap::new(),
            seen_by_peers: HashMap::new(),
            entries: HashMap::new(),
            max_entries,
            max_peer_cursors,
        }
    }

    pub fn tick(&mut self) -> u64 {
        self.local_clock += 1;
        self.local_clock
    }

    pub fn update_cursor(&mut self, peer_id: &str, clock: u64) {
        if self.peer_cursors.len() >= self.max_peer_cursors
            && !self.peer_cursors.contains_key(peer_id)
        {
            // Evict the peer with the lowest cursor (least recently synced)
            if let Some(oldest) = self.peer_cursors
                .iter()
                .min_by_key(|(_, c)| *c)
                .map(|(k, _)| k.clone())
            {
                self.peer_cursors.remove(&oldest);
            }
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

    pub fn store_entry(&mut self, entry: GossipEntry) {
        if self.entries.len() >= self.max_entries
            && !self.entries.contains_key(&entry.node_id)
        {
            // Evict the entry with the lowest clock (oldest known state)
            if let Some(oldest) = self.entries
                .iter()
                .min_by_key(|(_, e)| e.clock)
                .map(|(k, _)| k.clone())
            {
                self.entries.remove(&oldest);
            }
        }
        self.entries
            .entry(entry.node_id.clone())
            .and_modify(|e| {
                if entry.clock > e.clock {
                    *e = entry.clone();
                }
            })
            .or_insert(entry);
    }

    pub fn store_entries(&mut self, entries: &[GossipEntry]) {
        for e in entries {
            self.store_entry(e.clone());
        }
    }

    pub fn entries_since(&self, cursor: u64) -> Vec<GossipEntry> {
        self.entries
            .values()
            .filter(|e| e.clock > cursor)
            .cloned()
            .collect()
    }

    pub fn all_entries(&self) -> Vec<GossipEntry> {
        self.entries.values().cloned().collect()
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn local_entry(&self, addr: &str, capabilities: Vec<String>, hardware: HardwareCaps) -> GossipEntry {
        GossipEntry {
            node_id: self.local_node_id.clone(),
            addr: addr.to_string(),
            capabilities,
            hardware,
            clock: self.local_clock,
        }
    }

    pub fn seen_by(&self, peer_id: &str) -> u64 {
        self.seen_by_peers.get(peer_id).copied().unwrap_or(0)
    }

    pub fn record_seen_by(&mut self, peer_id: &str, clock: u64) {
        self.seen_by_peers
            .entry(peer_id.to_string())
            .and_modify(|c| *c = (*c).max(clock))
            .or_insert(clock);
    }

    pub fn save_to(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn load_from(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let json = std::fs::read_to_string(path)?;
        let state: Self = serde_json::from_str(&json)?;
        Ok(state)
    }
}

pub struct GossipEngine {
    pub state: GossipState,
    pub config: GossipConfig,
}

impl GossipEngine {
    pub fn new(local_node_id: String, config: GossipConfig) -> Self {
        let state = GossipState::new_with_limits(
            local_node_id,
            config.max_entries,
            config.max_peer_cursors,
        );
        Self { state, config }
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

    pub fn store_entries(&mut self, entries: &[GossipEntry]) {
        self.state.store_entries(entries);
    }

    pub fn entries_since(&self, cursor: u64) -> Vec<GossipEntry> {
        self.state.entries_since(cursor)
    }

    pub fn local_node_id(&self) -> &str {
        &self.state.local_node_id
    }

    pub fn local_entry(&self, addr: &str, capabilities: Vec<String>, hardware: HardwareCaps) -> GossipEntry {
        self.state.local_entry(addr, capabilities, hardware)
    }

    pub async fn perform_sync<T: MeshTransport + ?Sized>(
        &mut self,
        transport: &mut T,
        peer_id: &str,
    ) -> Result<(), TransportError>
    {
        let cursor = self.state.last_known(peer_id);
        let pull = GossipMessage::pull(self.state.local_node_id.clone(), cursor);
        let data = serde_json::to_vec(&pull)
            .map_err(|e| TransportError::Serialize(e.to_string()))?;
        transport.send(&data).await?;

        let resp = transport.receive().await?;
        let msg: GossipMessage = serde_json::from_slice(&resp)
            .map_err(|e| TransportError::Deserialize(e.to_string()))?;
        let entries = match msg {
            GossipMessage::PullResponse { entries } => entries,
            _ => {
                return Err(TransportError::Protocol("expected PullResponse".into()));
            }
        };

        if let Some(max_clock) = entries.iter().map(|e| e.clock).max() {
            self.state.update_cursor(peer_id, max_clock);
        }
        self.state.store_entries(&entries);

        let their_cursor = self.state.seen_by(peer_id);
        let our_entries = self.state.entries_since(their_cursor);
        if !our_entries.is_empty() {
            let push = GossipMessage::push(
                self.state.local_node_id.clone(),
                self.state.local_clock,
                our_entries,
            );
            let data = serde_json::to_vec(&push)
                .map_err(|e| TransportError::Serialize(e.to_string()))?;
            transport.send(&data).await?;
        }

        Ok(())
    }

    pub async fn handle_incoming_message<T: MeshTransport + ?Sized>(
        &mut self,
        transport: &mut T,
        _peer_id: &str,
        msg: &GossipMessage,
    ) -> Result<(), TransportError>
    {
        match msg {
            GossipMessage::Pull { sender_id, cursor } => {
                let entries = self.state.entries_since(*cursor);
                self.state.record_seen_by(sender_id, *cursor);
                let resp = GossipMessage::pull_response(entries);
                let data = serde_json::to_vec(&resp)
                    .map_err(|e| TransportError::Serialize(e.to_string()))?;
                transport.send(&data).await?;
                Ok(())
            }
            GossipMessage::Push { sender_id, sender_clock, entries } => {
                self.state.store_entries(entries);
                self.state.update_cursor(sender_id, *sender_clock);
                Ok(())
            }
            GossipMessage::PullResponse { .. } => {
                Err(TransportError::Protocol("unexpected PullResponse — initiator already processed".into()))
            }
        }
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

    fn make_entry(node_id: &str, clock: u64) -> GossipEntry {
        GossipEntry {
            node_id: node_id.to_string(),
            addr: format!("127.0.0.1:{}", 3000 + clock),
            capabilities: vec!["mesh".into()],
            hardware: HardwareCaps::default(),
            clock,
        }
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

    #[test]
    fn test_store_and_retrieve_entry() {
        let mut state = GossipState::new("local".into());
        let entry = make_entry("peer1", 1);
        state.store_entry(entry.clone());
        assert_eq!(state.entry_count(), 1);
        let all = state.all_entries();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].node_id, "peer1");
    }

    #[test]
    fn test_store_update_overwrites_if_newer() {
        let mut state = GossipState::new("local".into());
        state.store_entry(make_entry("peer1", 1));
        state.store_entry(make_entry("peer1", 5));
        assert_eq!(state.entry_count(), 1);
        assert_eq!(state.all_entries()[0].clock, 5);
    }

    #[test]
    fn test_store_entry_older_clock_ignored() {
        let mut state = GossipState::new("local".into());
        state.store_entry(make_entry("peer1", 5));
        state.store_entry(make_entry("peer1", 3));
        assert_eq!(state.all_entries()[0].clock, 5);
    }

    #[test]
    fn test_entries_since() {
        let mut state = GossipState::new("local".into());
        state.store_entry(make_entry("peer1", 1));
        state.store_entry(make_entry("peer2", 3));
        state.store_entry(make_entry("peer3", 5));

        let after_2 = state.entries_since(2);
        assert_eq!(after_2.len(), 2);
        assert!(after_2.iter().all(|e| e.clock > 2));

        let after_5 = state.entries_since(5);
        assert!(after_5.is_empty());
    }

    #[test]
    fn test_local_entry() {
        let mut state = GossipState::new("node42".into());
        state.tick();
        let hw = HardwareCaps { ram_mb: 8192, has_gpu: true, load_avg: 0.1 };
        let entry = state.local_entry("1.2.3.4:3030", vec!["mesh".into()], hw);
        assert_eq!(entry.node_id, "node42");
        assert_eq!(entry.addr, "1.2.3.4:3030");
        assert!(entry.hardware.has_gpu);
        assert_eq!(entry.clock, 1);
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let dir = std::env::temp_dir().join("tylluan_gossip_test_saveload");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("gossip_state.json");

        let mut state = GossipState::new("node_save".into());
        state.tick();
        state.update_cursor("peer_x", 7);
        state.store_entry(make_entry("peer_y", 3));
        state.save_to(&path).unwrap();

        let loaded = GossipState::load_from(&path).unwrap();
        assert_eq!(loaded.local_node_id, "node_save");
        assert_eq!(loaded.local_clock, 1);
        assert_eq!(loaded.last_known("peer_x"), 7);
        assert_eq!(loaded.entry_count(), 1);
        assert_eq!(loaded.all_entries()[0].clock, 3);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_engine_store_and_entries_since() {
        let mut engine = GossipEngine::new("local".into(), GossipConfig::default());
        let entries = vec![make_entry("a", 1), make_entry("b", 5)];
        engine.store_entries(&entries);
        assert_eq!(engine.entries_since(2).len(), 1);
        assert_eq!(engine.entries_since(0).len(), 2);
    }
}
