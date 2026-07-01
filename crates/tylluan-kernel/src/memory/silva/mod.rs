//! # SilvaDB: Sovereign Knowledge Graph
//!
//! Ported from `TylluanMCP/src/brain/SilvaDB.ts`.
//!
//! Provides:
//! - **Nodes**: Typed knowledge items (experience, lesson, concept, entity)
//! - **Edges**: Typed relationships (semantic triples: S→P→O)
//! - **BFS traversal**: Multi-hop context retrieval
//! - **Decay**: Weight decay for old nodes/edges (recency bias)
//! - **Search**: LIKE keyword + weight-ranked results

use anyhow::{Context, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use std::sync::{Arc, RwLock};
use std::path::PathBuf;
use chrono::{DateTime, Utc};

use crate::security::circuit_breaker::CircuitBreaker;

pub mod decay;
pub mod edges;
pub mod graph;
pub mod nodes;
pub mod search;
pub mod anchors;
pub mod autolink;
pub mod hnsw;
pub mod maintenance;
pub mod schema;
pub mod sharing;

/// A node in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub node_type: String,
    pub content: String,
    pub metadata: String,
    pub weight: f64,
    pub protected: bool,
    pub conflicted: bool,
    pub topic_key: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub last_touched: DateTime<Utc>,
    pub valid_from: Option<i64>,
    pub valid_until: Option<i64>,
    pub shareable: bool,
}

/// An edge in the knowledge graph.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub edge_type: String,
    pub metadata: String,
    pub weight: f64,
}

/// Node trace for stigmergy: records when an agent touched a node.
#[derive(Debug, Clone)]
pub struct NodeTrace {
    pub node_id: String,
    pub agent_id: String,
    pub touched_at: i64,
    pub trace_type: String,
}

/// SilvaDB statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SilvaStats {
    pub page_count: i64,
    pub page_size: i64,
    pub total_bytes: i64,
    pub node_count: i64,
    pub edge_count: i64,
}

/// Graph analysis result for tylluan_think
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ThinkAnalysis {
    pub hub_node: Option<(String, usize)>,     // (node_id, degree)
    pub contradictions: Vec<String>,
    pub connected_path: Vec<String>,
    pub node_count: usize,
}

/// The SilvaDB knowledge graph engine.
pub struct SilvaDB {
    pub(crate) conn: Arc<Mutex<Connection>>,
    pub(crate) cb_vector: CircuitBreaker,
    /// Memory-mapped int8 quantized embedding store (loaded on init if .fjv1 exists)
    pub(crate) mmap_store: Arc<RwLock<Option<crate::memory::mmap_store::MmapEmbeddingStore>>>,
    /// IVF searcher built from centroids + assignments in the mmap file
    pub(crate) ivf_searcher: Arc<RwLock<Option<crate::memory::ivf_index::IVFSearcher>>>,
    /// Path to the .fjv1 mmap file (derived from SQLite db_path)
    pub(crate) mmap_path: Option<PathBuf>,
    /// HNSW index for approximate nearest neighbor search (built at >= 12k nodes)
    pub(crate) hnsw: tokio::sync::RwLock<Option<crate::memory::silva::hnsw::HnswIndex>>,
}

impl SilvaDB {
    /// Access to the internal connection for testing/advanced manipulation.
    pub fn conn_lock(&self) -> Arc<Mutex<Connection>> {
        Arc::clone(&self.conn)
    }

    /// Open or create a SilvaDB database at the given path.
    /// Ensures parent directories exist.
    pub fn open(db_path: &str) -> Result<Self> {
        if let Some(parent) = std::path::Path::new(db_path).parent() {
            std::fs::create_dir_all(parent).ok();
        }

        // Derive mmap path: same dir, same stem, .fjv1 extension
        let mmap_path = std::path::Path::new(db_path)
            .with_extension("fjv1");

        let conn = crate::config::open_db(std::path::Path::new(db_path))
            .with_context(|| format!("Failed to open SilvaDB: {}", db_path))?;

        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
            cb_vector: CircuitBreaker::new(),
            mmap_store: Arc::new(RwLock::new(None)),
            ivf_searcher: Arc::new(RwLock::new(None)),
            mmap_path: Some(mmap_path),
            hnsw: tokio::sync::RwLock::new(None),
        };
        Ok(db)
    }

    /// Complete initialization, ensuring the schema is up to date and performance optimizations (WAL) are applied.
    /// Also loads the existing .fjv1 mmap embedding store if present.
    pub async fn init(&self) -> Result<()> {
        tokio::task::block_in_place(|| {
            let mut conn = self.conn.blocking_lock();
            crate::memory::migrations::MigrationRunner::run(&mut conn, "silva")
        })?;
        self.init_schema().await?;
        self.load_mmap_store().await;
        self.load_hnsw_from_db().await;
        Ok(())
    }

    /// Try to load an existing .fjv1 mmap embedding store.
    /// Silently returns if file doesn't exist or is corrupt.
    async fn load_mmap_store(&self) {
        if let Some(ref path) = self.mmap_path {
            if path.exists() {
                match crate::memory::mmap_store::MmapEmbeddingStore::load(path) {
                    Ok(store) => {
                        let searcher = crate::memory::ivf_index::IVFSearcher::new(
                            store.centroids().to_vec(),
                            store.assignments(),
                            10,
                        );
                        *self.mmap_store.write().unwrap() = Some(store);
                        *self.ivf_searcher.write().unwrap() = Some(searcher);
                        tracing::info!("🌲 Loaded .fjv1 mmap embedding store from {}", path.display());
                    }
                    Err(e) => tracing::warn!("🌲 Failed to load .fjv1 mmap store (will rebuild on next consolidate): {}", e),
                }
            }
        }
    }

    /// Create an in-memory instance (for testing).
    #[allow(dead_code)]
    pub async fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
            cb_vector: CircuitBreaker::new(),
            mmap_store: Arc::new(RwLock::new(None)),
            ivf_searcher: Arc::new(RwLock::new(None)),
            mmap_path: None,
            hnsw: tokio::sync::RwLock::new(None),
        };
        db.init_schema().await?;
        tokio::task::block_in_place(|| {
            let mut conn = db.conn.blocking_lock();
            crate::memory::migrations::MigrationRunner::run(&mut conn, "silva")
        })?;
        Ok(db)
    }

    /// Perform a WAL checkpoint to merge transaction logs into the main database file.
    /// This prevents the -wal file from growing indefinitely.
    pub async fn checkpoint(&self) -> Result<()> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
                .with_context(|| "Failed to checkpoint SilvaDB")
        })?;
        Ok(())
    }

    pub async fn health_check(&self) -> Result<()> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let _: i64 = conn.query_row("SELECT COUNT(*) FROM nodes", [], |row| row.get(0))?;
            let _: i64 = conn.query_row("SELECT COUNT(*) FROM edges", [], |row| row.get(0))?;
            Ok::<(), anyhow::Error>(())
        })?;
        Ok(())
    }

    /// Load the HNSW index from the hnsw_index table.
    /// Silently returns None if no index is stored or count < threshold.
    pub async fn load_hnsw_from_db(&self) {
        let result: Option<crate::memory::silva::hnsw::HnswIndex> = tokio::task::block_in_place(|| -> Option<crate::memory::silva::hnsw::HnswIndex> {
            let conn = self.conn.blocking_lock();
            let row: (Vec<u8>, i32) = conn.query_row(
                "SELECT index_blob, node_count FROM hnsw_index WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            ).ok()?;
            if (row.1 as usize) < crate::memory::silva::hnsw::HNSW_THRESHOLD {
                return None;
            }
            crate::memory::silva::hnsw::deserialize_hnsw_rebuild(&row.0).ok()
        });
        if let Some(state) = result {
            *self.hnsw.write().await = Some(state);
            tracing::info!("🌲 HNSW index loaded from DB");
        }
    }

    /// Serialize and save the current HNSW index to the hnsw_index table.
    pub async fn save_hnsw_to_db(&self) -> Result<()> {
        let state = self.hnsw.read().await;
        let Some(ref state) = *state else {
            return Ok(());
        };
        let bytes = crate::memory::silva::hnsw::serialize_hnsw_data(state)?;
        let node_count = state.node_ids.len() as i32;
        let _ = state;

        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            conn.execute(
                "INSERT OR REPLACE INTO hnsw_index(id, index_blob, node_count, built_at) VALUES(1, ?1, ?2, datetime('now'))",
                rusqlite::params![bytes, node_count],
            )?;
            Ok::<(), anyhow::Error>(())
        })?;
        tracing::info!("🌲 HNSW index saved to DB ({} nodes)", node_count);
        Ok(())
    }

    /// Rebuild the HNSW index if the embedding count >= threshold and no index exists.
    pub async fn rebuild_hnsw_if_needed(&self) -> Result<()> {
        {
            let guard = self.hnsw.read().await;
            if guard.is_some() {
                return Ok(());
            }
        }

        let count: i64 = tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            conn.query_row("SELECT COUNT(*) FROM node_embeddings", [], |row| row.get(0))
                .unwrap_or(0)
        });

        if (count as usize) < crate::memory::silva::hnsw::HNSW_THRESHOLD {
            return Ok(());
        }

        let entries: Vec<(String, Vec<u8>)> = tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let mut stmt = conn
                .prepare("SELECT node_id, embedding FROM node_embeddings ORDER BY rowid DESC")
                .ok()?;
            let rows = stmt
                .query_map([], |row| {
                    let id: String = row.get(0)?;
                    let blob: Vec<u8> = row.get(1)?;
                    Ok((id, blob))
                })
                .ok()?;
            let mut result = Vec::new();
            for row in rows.flatten() {
                result.push(row);
            }
            Some(result)
        })
        .unwrap_or_default();

        if let Some(state) = crate::memory::silva::hnsw::build_hnsw(entries) {
            *self.hnsw.write().await = Some(state);
            self.save_hnsw_to_db().await?;
            tracing::info!("🌲 HNSW index rebuilt with {} nodes", count);
        }

        Ok(())
    }

}

fn jaccard_similarity(a: &str, b: &str) -> f64 {
    let set_a: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let set_b: std::collections::HashSet<&str> = b.split_whitespace().collect();
    if set_a.is_empty() && set_b.is_empty() { return 0.0; }
    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();
    if union == 0 { return 0.0; }
    intersection as f64 / union as f64
}

/// Cosine similarity between two raw f32-bytes blobs (used by Dream Cycle dedup).
pub(crate) fn dream_cosine(a: &[u8], b: &[u8]) -> f64 {
    cosine_similarity(a, b)
}

pub(crate) fn cosine_similarity(a: &[u8], b: &[u8]) -> f64 {
    let a_f: Vec<f32> = a.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().expect("chunk should be exactly 4 bytes"))).collect();
    let b_f: Vec<f32> = b.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().expect("chunk should be exactly 4 bytes"))).collect();
    if a_f.len() != b_f.len() || a_f.is_empty() { return 0.0; }
    let dot: f32 = a_f.iter().zip(&b_f).map(|(x, y)| x * y).sum();
    let na: f32 = a_f.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b_f.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 { return 0.0; }
    (dot / (na * nb)) as f64
}

#[cfg(test)]
mod tests;
