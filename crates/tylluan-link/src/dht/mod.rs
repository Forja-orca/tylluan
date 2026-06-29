//! # TylluanLink DHT — Kademlia DHT for Mesh Discovery (M14-A)
//!
//! Two-layer architecture:
//! - Bootstrap via Mainline DHT (WAN, no infrastructure)
//! - Routing table with K-buckets over Ed25519 node IDs

pub mod routing_table;
pub mod announce;
pub mod bootstrap;

pub use routing_table::{RoutingTable, KBucketEntry, K};
pub use announce::PeerAnnouncement;
pub use bootstrap::{BootstrapConfig, DiscoveredPeer, PeerSource, DHT_INFO_HASH};
