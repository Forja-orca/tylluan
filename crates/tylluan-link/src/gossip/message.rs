use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareCaps {
    pub ram_mb: u32,
    pub has_gpu: bool,
    pub load_avg: f32,
}

impl Default for HardwareCaps {
    fn default() -> Self {
        Self {
            ram_mb: 0,
            has_gpu: false,
            load_avg: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipEntry {
    pub node_id: String,
    pub addr: String,
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub hardware: HardwareCaps,
    pub clock: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum GossipMessage {
    Push {
        sender_id: String,
        sender_clock: u64,
        entries: Vec<GossipEntry>,
    },
    Pull {
        sender_id: String,
        cursor: u64,
    },
    PullResponse {
        entries: Vec<GossipEntry>,
    },
}

impl GossipMessage {
    pub fn push(sender_id: String, sender_clock: u64, entries: Vec<GossipEntry>) -> Self {
        Self::Push { sender_id, sender_clock, entries }
    }

    pub fn pull(sender_id: String, cursor: u64) -> Self {
        Self::Pull { sender_id, cursor }
    }

    pub fn pull_response(entries: Vec<GossipEntry>) -> Self {
        Self::PullResponse { entries }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gossip_message_serde() {
        let msg = GossipMessage::push(
            "node1".into(),
            42,
            vec![GossipEntry {
                node_id: "peer1".into(),
                addr: "192.168.1.1:3030".into(),
                capabilities: vec!["mesh".into()],
                hardware: HardwareCaps { ram_mb: 4096, has_gpu: false, load_avg: 0.3 },
                clock: 1,
            }],
        );
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: GossipMessage = serde_json::from_str(&json).unwrap();
        match decoded {
            GossipMessage::Push { sender_id, sender_clock, entries } => {
                assert_eq!(sender_id, "node1");
                assert_eq!(sender_clock, 42);
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].node_id, "peer1");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_pull_serde() {
        let msg = GossipMessage::pull("node1".into(), 10);
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: GossipMessage = serde_json::from_str(&json).unwrap();
        match decoded {
            GossipMessage::Pull { sender_id, cursor } => {
                assert_eq!(sender_id, "node1");
                assert_eq!(cursor, 10);
            }
            _ => panic!("wrong variant"),
        }
    }
}
