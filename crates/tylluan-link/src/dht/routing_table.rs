use std::net::SocketAddr;
use std::time::{Duration, Instant};

pub const K: usize = 16;
pub const BUCKET_REFRESH: Duration = Duration::from_secs(3600);
pub const PEER_TIMEOUT: Duration = Duration::from_secs(7200);

#[derive(Debug, Clone)]
pub struct KBucketEntry {
    pub node_id: String,
    pub addr: SocketAddr,
    pub capabilities: Vec<String>,
    pub last_seen_unix: i64,
    pub last_seen: Instant,
}

impl KBucketEntry {
    pub fn new(node_id: String, addr: SocketAddr, capabilities: Vec<String>) -> Self {
        Self {
            node_id,
            addr,
            capabilities,
            last_seen_unix: now_unix(),
            last_seen: Instant::now(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct KBucket {
    pub entries: Vec<KBucketEntry>,
    pub last_refresh_unix: i64,
    pub last_refresh: Instant,
}

impl KBucket {
    pub fn new() -> Self {
        Self {
            entries: Vec::with_capacity(K),
            last_refresh_unix: now_unix(),
            last_refresh: Instant::now(),
        }
    }

    fn remove_dead(&mut self) {
        self.entries.retain(|e| e.last_seen.elapsed() < PEER_TIMEOUT);
    }

    fn insert_internal(&mut self, entry: KBucketEntry) {
        self.remove_dead();
        if let Some(pos) = self.entries.iter().position(|e| e.node_id == entry.node_id) {
            self.entries[pos].addr = entry.addr;
            self.entries[pos].capabilities = entry.capabilities;
            self.entries[pos].last_seen = Instant::now();
            self.entries[pos].last_seen_unix = now_unix();
        } else if self.entries.len() < K {
            self.entries.push(entry);
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.entries.len() >= K
    }
}

impl Default for KBucket {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct RoutingTable {
    pub local_node_id: String,
    pub buckets: Vec<KBucket>,
}

impl RoutingTable {
    pub fn new(local_node_id: String) -> Self {
        Self {
            local_node_id,
            buckets: (0..256).map(|_| KBucket::new()).collect(),
        }
    }


    fn bucket_index(local: &str, target: &str) -> usize {
        let a_bytes = hex::decode(local).unwrap_or_default();
        let b_bytes = hex::decode(target).unwrap_or_default();
        if a_bytes.len() != 16 || b_bytes.len() != 16 {
            return 0;
        }
        let xor: Vec<u8> = a_bytes.iter().zip(b_bytes.iter()).map(|(x, y)| x ^ y).collect();
        for (i, &byte) in xor.iter().enumerate() {
            if byte != 0 {
                let leading = byte.leading_zeros() as usize;
                return (i * 8) + leading;
            }
        }
        255
    }

    pub fn insert(&mut self, node_id: &str, addr: SocketAddr, capabilities: Vec<String>) {
        if node_id == self.local_node_id {
            return;
        }
        let idx = Self::bucket_index(&self.local_node_id, node_id);
        if idx < self.buckets.len() {
            let entry = KBucketEntry::new(node_id.to_string(), addr, capabilities);
            self.buckets[idx].insert_internal(entry);
        }
    }

    pub fn find_closest(&self, target: &str, count: usize) -> Vec<&KBucketEntry> {
        let mut candidates: Vec<&KBucketEntry> = self.buckets.iter()
            .flat_map(|b| b.entries.iter().filter(|e| e.last_seen.elapsed() < PEER_TIMEOUT))
            .collect();

        let local_bytes = hex::decode(&self.local_node_id).unwrap_or_default();
        let target_bytes = hex::decode(target).unwrap_or_default();

        candidates.sort_by(|a, b| {
            let a_dist = Self::xor_distance_ordered(&local_bytes, &target_bytes, &hex::decode(&a.node_id).unwrap_or_default());
            let b_dist = Self::xor_distance_ordered(&local_bytes, &target_bytes, &hex::decode(&b.node_id).unwrap_or_default());
            a_dist.cmp(&b_dist)
        });

        candidates.truncate(count);
        candidates
    }

    fn xor_distance_ordered(local: &[u8], target: &[u8], node: &[u8]) -> Vec<u8> {
        let xor_lt: Vec<u8> = local.iter().zip(target.iter()).map(|(x, y)| x ^ y).collect();
        let xor_ln: Vec<u8> = local.iter().zip(node.iter()).map(|(x, y)| x ^ y).collect();
        xor_lt.iter().zip(xor_ln.iter()).map(|(a, b)| a ^ b).collect()
    }

    pub fn all_peers(&self) -> Vec<KBucketEntry> {
        let mut peers: Vec<KBucketEntry> = self.buckets.iter()
            .flat_map(|b| {
                b.entries.iter()
                    .filter(|e| e.last_seen.elapsed() < PEER_TIMEOUT)
                    .cloned()
                    .map(|mut e| {
                        e.last_seen = Instant::now();
                        e
                    })
            })
            .collect();
        peers.sort_by(|a, b| b.last_seen_unix.cmp(&a.last_seen_unix));
        peers.dedup_by(|a, b| a.node_id == b.node_id);
        peers
    }

    pub fn peer_count(&self) -> usize {
        self.buckets.iter().map(|b| b.entries.len()).sum()
    }

    pub fn refresh_buckets(&mut self) {
        for bucket in &mut self.buckets {
            bucket.remove_dead();
        }
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Sha256, Digest};

    fn node_id_from_hex(hex_str: &str) -> String {
        let hash = Sha256::digest(hex_str.as_bytes());
        hex::encode(&hash[..16])
    }

    #[test]
    fn test_routing_table_insert() {
        let local = node_id_from_hex("local");
        let mut rt = RoutingTable::new(local.clone());

        let peer1 = node_id_from_hex("peer1");
        let peer2 = node_id_from_hex("peer2");

        rt.insert(&peer1, "192.168.1.1:3000".parse().unwrap(), vec!["mesh".into()]);
        rt.insert(&peer2, "192.168.1.2:3000".parse().unwrap(), vec!["mesh".into(), "federation".into()]);

        assert_eq!(rt.peer_count(), 2);
    }

    #[test]
    fn test_find_closest() {
        let local = node_id_from_hex("local");
        let mut rt = RoutingTable::new(local.clone());

        for i in 0..10 {
            let pid = node_id_from_hex(&format!("peer{}", i));
            rt.insert(&pid, format!("192.168.1.{}:3000", i + 1).parse().unwrap(), vec![]);
        }

        let target = node_id_from_hex("search-target");
        let closest = rt.find_closest(&target, 5);
        assert_eq!(closest.len(), 5);
    }

    #[test]
    fn test_self_insert_ignored() {
        let local = node_id_from_hex("self");
        let mut rt = RoutingTable::new(local.clone());
        rt.insert(&local, "127.0.0.1:3000".parse().unwrap(), vec![]);
        assert_eq!(rt.peer_count(), 0);
    }

    #[test]
    fn test_all_peers_dedup() {
        let local = node_id_from_hex("local");
        let mut rt = RoutingTable::new(local.clone());
        let pid = node_id_from_hex("peer");
        rt.insert(&pid, "192.168.1.1:3000".parse().unwrap(), vec!["a".into()]);
        rt.insert(&pid, "192.168.1.1:3000".parse().unwrap(), vec!["a".into(), "b".into()]);
        assert_eq!(rt.all_peers().len(), 1);
    }

    #[test]
    fn test_bucket_index_ranges() {
        let a = node_id_from_hex("aaaa");
        let b = node_id_from_hex("bbbb");
        let idx = RoutingTable::bucket_index(&a, &b);
        assert!(idx < 256, "bucket index out of range: {}", idx);
    }
}
