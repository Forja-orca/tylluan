//! # Memory Module
//!
//! Persistent memory layer for TylluanNexus:
//! - **HybridMemory**: FTS5 keyword + vector cosine hybrid search (RRF fusion)
//! - **SilvaDB**: Knowledge graph with nodes, edges, BFS traversal, and decay
//! - **ConsensusEngine**: Sovereign conflict resolution for multi-agent truth
//! - **Mailbox**: Agent-to-agent async messaging
//! - **IdentityManager**: Agent biographical identity with context injection
//! - **BackupManager**: Automatic backup and integrity verification
//! - **MigrationRunner**: Schema versioning for forward compatibility
//!
//! All backed by SQLite (rusqlite bundled) — zero external dependencies, cross-platform.

pub mod backup;
pub mod hybrid;
pub mod silva;
pub mod consensus;
pub mod mailbox;
pub mod cosine;
pub mod identity;
pub mod migrations;
pub mod graph_rag;
pub mod louvain;
pub mod agent_memory;
pub mod agent_profile;
pub mod triple_extractor;
pub mod coloquio;
pub mod jobs;
pub mod dream_cycle;
pub mod dual_retrieval;
pub mod auto_link;
pub mod idle_lab;
pub mod idle_lab_oracle;
pub mod autoresearch;
pub mod mmap_store;
pub mod ivf_index;
pub mod agent_nodes;
