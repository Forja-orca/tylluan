//! # HybridMemory: FTS5 + Vector Search + RRF Fusion
//!
//! Ported from `TylluanMCP/src/memory/HybridMemory.ts`.
//!
//! ## Search Strategy
//!
//! 1. **FTS5 (BM25)**: Keyword search, always available
//! 2. **Vector cosine similarity**: Scans stored embeddings (pure Rust, no native deps)
//! 3. **RRF fusion** (k=60): Combines both result sets using Reciprocal Rank Fusion
//!
//! Embeddings are stored as BLOB (f32 little-endian). When ONNX feature is enabled,
//! the kernel generates embeddings at write time. Without it, FTS5-only mode works fine.

use crate::memory::cosine::cosine_similarity;
use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use std::collections::HashMap;
use std::path::Path;
use tracing::info;

/// Retrieval budget tier for adaptive RRF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetrievalTier { Fast, Balanced, Deep }

/// Classify a query into a retrieval tier based on token count + character entropy.
///   Fast:     ≤3 tokens and entropy < 3.0  → BM25 only, top-5
///   Balanced: ≤8 tokens and entropy < 4.5  → BM25 + vector + adaptive RRF
///   Deep:     anything else                 → full pipeline (with future reranker)
pub fn classify_query_tier(query: &str) -> RetrievalTier {
    let tokens = query.split_whitespace().count();
    let entropy = shannon_entropy_text(query);
    match (tokens, entropy) {
        (t, e) if t <= 3 && e < 3.0 => RetrievalTier::Balanced,
        (t, e) if t <= 8 && e < 4.5 => RetrievalTier::Balanced,
        _ => RetrievalTier::Deep,
    }
}

/// Shannon entropy of a score distribution H(s) = -Σ p_i·log₂(p_i).
/// Used by adaptive RRF to modulate the k parameter.
fn shannon_entropy(scores: &[f32]) -> f32 {
    let total: f32 = scores.iter().sum();
    if total <= 0.0 { return 0.0; }
    scores.iter()
        .map(|s| { let p = s / total; if p > 0.0 { -p * p.log2() } else { 0.0 } })
        .sum()
}

/// Character-level Shannon entropy of a text string.
/// Used by the budget router to classify query complexity.
fn shannon_entropy_text(text: &str) -> f32 {
    let len = text.len();
    if len == 0 { return 0.0; }
    let mut freq: HashMap<char, usize> = HashMap::new();
    for c in text.chars() {
        *freq.entry(c).or_default() += 1;
    }
    freq.values()
        .map(|&count| { let p = count as f32 / len as f32; -p * p.log2() })
        .sum()
}

/// A document stored in HybridMemory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: i64,
    pub content: String,
    pub metadata: String,
    pub score: f32,
}

/// Database statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbStats {
    pub page_count: i64,
    pub page_size: i64,
    pub total_bytes: i64,
    pub document_count: i64,
}

/// The HybridMemory engine — FTS5 + vector + RRF.
pub struct HybridMemory {
    conn: Mutex<Connection>,
}

impl HybridMemory {
    /// Open or create a HybridMemory database at the given path.
    pub fn open(db_path: &str) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = Path::new(db_path).parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let conn = crate::config::open_db(std::path::Path::new(db_path))
            .with_context(|| format!("Failed to open HybridMemory DB: {:?}", db_path))?;

        let memory = Self { conn: Mutex::new(conn) };
        // We do a sync-ish initialization here since it's the constructor
        // But since init_schema needs a lock, we'll handle it carefully or just use a sync block.
        // For simplicity in the constructor, we can use a temporary sync call if we can, 
        // but better to just do it in an async-ready way.
        Ok(memory)
    }

    /// Complete initialization. Ensures the FTS5 virtual tables are created and maintenance tasks are performed. 
    /// Must be called once before use.
    pub async fn init(&self) -> Result<()> {
        tokio::task::block_in_place(|| {
            let mut conn = self.conn.blocking_lock();
            crate::memory::migrations::MigrationRunner::run(&mut conn, "hybrid")
        })?;
        self.init_schema().await
    }

    /// Create an in-memory instance (for testing).
    #[allow(dead_code)]
    pub async fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let memory = Self { conn: Mutex::new(conn) };
        memory.init_schema().await?;
        Ok(memory)
    }

    /// Perform a WAL checkpoint to merge transaction logs into the main database file.
    /// This prevents the -wal file from growing indefinitely.
    pub async fn checkpoint(&self) -> Result<()> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
                .with_context(|| "Failed to checkpoint HybridMemory")
        })?;
        Ok(())
    }

    async fn init_schema(&self) -> Result<()> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            conn.execute_batch("PRAGMA journal_mode = WAL;")?;
            
            // Performance optimizations
            conn.execute_batch(
                "PRAGMA synchronous = NORMAL;      -- Faster writes
                 PRAGMA cache_size = -64000;        -- 64MB cache
                 PRAGMA temp_store = MEMORY;        -- Temp in RAM
                 PRAGMA mmap_size = 268435456;      -- 256MB mmap
                 PRAGMA page_size = 4096;"
            )?;

            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS documents (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    content TEXT NOT NULL,
                    metadata TEXT DEFAULT '{}',
                    embedding BLOB,
                    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
                );

                CREATE VIRTUAL TABLE IF NOT EXISTS fts_documents USING fts5(
                    content,
                    content='documents',
                    content_rowid='id'
                );"
            )?;

            Ok::<(), anyhow::Error>(())
        })?;

        info!("🧠 HybridMemory schema initialized (FTS5 + vector).");
        Ok(())
    }

    /// Add a document to the hybrid memory system.
    /// If an embedding is provided, it is stored alongside the content for vector search.
    pub async fn add_document(
        &self,
        content: &str,
        metadata: &str,
        embedding: Option<&[f32]>,
    ) -> Result<i64> {
        let embedding_blob: Option<Vec<u8>> = embedding.map(|emb| {
            emb.iter().flat_map(|f| f.to_le_bytes()).collect()
        });

        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            conn.execute(
                "INSERT INTO documents (content, metadata, embedding) VALUES (?1, ?2, ?3)",
                params![content, metadata, embedding_blob],
            )?;

            let id = conn.last_insert_rowid();

            // Insert into FTS index
            conn.execute(
                "INSERT INTO fts_documents (rowid, content) VALUES (?1, ?2)",
                params![id, content],
            )?;

            Ok(id)
        })
    }

    /// Search documents using adaptive RRF with budget-aware tier routing.
    /// 
    /// Tier-based dispatch:
    ///   Fast:     BM25 only, top-5, no embeddings (budget saver)
    ///   Balanced: BM25 + vector + adaptive RRF with dynamic k
    ///   Deep:     full pipeline (same as Balanced for now; future reranker slot)
    /// 
    /// Adaptive k (balanced/deep only):
    ///   k = 60 + (entropy_vec - entropy_bm25) * 20, clamped [40, 80]
    pub async fn search(
        &self,
        query: &str,
        query_embedding: Option<&[f32]>,
        limit: usize,
    ) -> Result<Vec<Document>> {
        let safe_query = query.chars().take(1000).collect::<String>();
        let sanitized = sanitize_fts_query(&safe_query);
        let tier = classify_query_tier(&sanitized);

        match tier {
            RetrievalTier::Fast => self.search_fast(&sanitized).await,
            RetrievalTier::Balanced => self.search_balanced(&sanitized, query_embedding, limit).await,
            RetrievalTier::Deep => self.search_deep(&sanitized, query_embedding, limit).await,
        }
    }

    /// Fast tier: BM25 only, top-5 results. No embedding scan.
    async fn search_fast(&self, sanitized: &str) -> Result<Vec<Document>> {
        let fts_results = self.fts_search(sanitized, 5).await?;
        let mut results = Vec::new();
        for (id, _score) in fts_results.into_iter().take(5) {
            if let Ok(doc) = self.get_document(id).await {
                results.push(Document { id, content: doc.0, metadata: doc.1, score: 1.0 });
            }
        }
        Ok(results)
    }

    /// Balanced tier: BM25 + vector + adaptive RRF with dynamic k.
    async fn search_balanced(
        &self,
        sanitized: &str,
        query_embedding: Option<&[f32]>,
        limit: usize,
    ) -> Result<Vec<Document>> {
        let fts_results = if !sanitized.is_empty() {
            self.fts_search(sanitized, limit * 2).await?
        } else {
            Vec::new()
        };

        let vector_results = if let Some(q_emb) = query_embedding {
            self.vector_search(q_emb, limit * 2).await?
        } else {
            Vec::new()
        };

        // Adaptive k: Shannon entropy of both score distributions
        let bm25_scores: Vec<f32> = fts_results.iter().map(|(_, s)| *s).collect();
        let vec_scores: Vec<f32> = vector_results.iter().map(|(_, s)| *s).collect();
        let e_bm25 = shannon_entropy(&bm25_scores);
        let e_vec = shannon_entropy(&vec_scores);
        let k = (60.0 + (e_vec - e_bm25) * 20.0).clamp(40.0, 80.0);

        Ok(self.fuse_rrf(fts_results, vector_results, k, limit).await)
    }

    /// Deep tier: full pipeline (same fusion as balanced; reserved for future reranker).
    async fn search_deep(
        &self,
        sanitized: &str,
        query_embedding: Option<&[f32]>,
        limit: usize,
    ) -> Result<Vec<Document>> {
        self.search_balanced(sanitized, query_embedding, limit).await
    }

    /// Reciprocal Rank Fusion with configurable k.
    /// Weights read from IdleLab atomics (SEMANTIC_WEIGHT), BM25 derived as 1.0 - semantic.
    async fn fuse_rrf(
        &self,
        fts_results: Vec<(i64, f32)>,
        vector_results: Vec<(i64, f32)>,
        k: f32,
        limit: usize,
    ) -> Vec<Document> {
        let semantic_weight = (crate::memory::idle_lab::SEMANTIC_WEIGHT.load(std::sync::atomic::Ordering::Relaxed) as f32) / 100.0;
        let bm25_weight = 1.0 - semantic_weight;

        let mut fusion_map: HashMap<i64, f32> = HashMap::new();

        for (rank, (id, _score)) in vector_results.iter().enumerate() {
            let score = semantic_weight / (k + rank as f32 + 1.0);
            *fusion_map.entry(*id).or_default() += score;
        }

        for (rank, (id, _bm25)) in fts_results.iter().enumerate() {
            let score = bm25_weight / (k + rank as f32 + 1.0);
            *fusion_map.entry(*id).or_default() += score;
        }

        let mut ranked: Vec<(i64, f32)> = fusion_map.into_iter().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        ranked.truncate(limit);

        let mut results = Vec::new();
        for (id, score) in ranked {
            if let Ok(doc) = self.get_document(id).await {
                results.push(Document { id, content: doc.0, metadata: doc.1, score });
            }
        }
        results
    }

    /// FTS5 BM25 search. Returns (id, bm25_score) pairs.
    async fn fts_search(&self, query: &str, limit: usize) -> Result<Vec<(i64, f32)>> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT f.rowid, bm25(fts_documents) as score
                 FROM fts_documents f
                 WHERE fts_documents MATCH ?1
                 ORDER BY bm25(fts_documents)
                 LIMIT ?2"
            )?;

            let rows = stmt.query_map(params![query, limit as i64], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, f32>(1)?))
            })?;

            Ok(rows.filter_map(|r| r.ok()).collect())
        })
    }

    /// T12: Tiered vector search (hot + cold).
    /// Hot tier: recent docs (last 30 days) — scanned first for speed.
    /// Cold tier: older docs — only scanned if hot doesn't yield enough results.
    /// Returns (id, cosine_similarity) pairs for entropy computation + RRF.
    async fn vector_search(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<(i64, f32)>> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            
            // Hot tier: recent docs (last 30 days)
            let mut scored = self.scan_embeddings(
                &conn, query_embedding,
                "SELECT id, embedding FROM documents WHERE embedding IS NOT NULL AND created_at > datetime('now', '-30 days') ORDER BY created_at DESC LIMIT 2000"
            )?;

            // Cold tier: only if hot didn't yield enough
            if scored.len() < limit {
                let cold = self.scan_embeddings(
                    &conn, query_embedding,
                    "SELECT id, embedding FROM documents WHERE embedding IS NOT NULL AND created_at <= datetime('now', '-30 days') ORDER BY created_at DESC LIMIT 3000"
                )?;
                scored.extend(cold);
            }

            // Sort by similarity descending
            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            scored.dedup_by_key(|item| item.0);
            scored.truncate(limit);

            Ok(scored)
        })
    }

    /// Scan embeddings from a SQL query and score with cosine similarity.
    fn scan_embeddings(
        &self,
        conn: &rusqlite::Connection,
        query_embedding: &[f32],
        sql: &str,
    ) -> Result<Vec<(i64, f32)>> {
        let mut stmt = conn.prepare(sql)?;
        let mut scored: Vec<(i64, f32)> = Vec::new();

        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            Ok((id, blob))
        })?;

        for row in rows.flatten() {
            let (id, blob) = row;
            if blob.is_empty() { continue; }

            let stored: Vec<f32> = blob
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();

            if stored.len() != query_embedding.len() { continue; }

            let sim = cosine_similarity(query_embedding, &stored);
            if sim > 0.2 {
                scored.push((id, sim));
            }
        }

        Ok(scored)
    }

    /// Fetch a document's content and metadata by id.
    async fn get_document(&self, id: i64) -> Result<(String, String)> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT content, metadata FROM documents WHERE id = ?1"
            )?;
            let result = stmt.query_row(params![id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            Ok(result)
        })
    }

    /// Get total document count.
    pub async fn document_count(&self) -> Result<i64> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM documents", [], |row| row.get(0)
            )?;
            Ok(count)
        })
    }
    
    /// Get database stats (size, page count).
    pub async fn stats(&self) -> Result<DbStats> {
        let (page_count, page_size) = tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let page_count: i64 = conn.query_row(
                "PRAGMA page_count", [], |row| row.get(0)
            )?;
            let page_size: i64 = conn.query_row(
                "PRAGMA page_size", [], |row| row.get(0)
            )?;
            Ok::<(i64, i64), anyhow::Error>((page_count, page_size))
        })?;
        
        Ok(DbStats {
            page_count,
            page_size,
            total_bytes: page_count * page_size,
            document_count: self.document_count().await?,
        })
    }
    
    /// Vacuum database to reclaim space.
    pub async fn vacuum(&self) -> Result<()> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            conn.execute_batch("VACUUM")?;
            Ok::<(), anyhow::Error>(())
        })?;
        info!("🧠 HybridMemory vacuumed.");
        Ok(())
    }

    /// Health check: verify database integrity via quick query.
    pub async fn health_check(&self) -> Result<()> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let _: i64 = conn.query_row("SELECT COUNT(*) FROM documents", [], |row| row.get(0))?;
            Ok::<(), anyhow::Error>(())
        })?;
        Ok(())
    }
}

/// Remove FTS5 special characters from a query to prevent syntax errors.
fn sanitize_fts_query(query: &str) -> String {
    query
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_memory() -> HybridMemory {
        HybridMemory::in_memory().await.unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_add_and_count() {
        let mem = test_memory().await;
        mem.add_document("hello world", "{}", None).await.unwrap();
        mem.add_document("goodbye world", "{}", None).await.unwrap();
        assert_eq!(mem.document_count().await.unwrap(), 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fts_search() {
        let mem = test_memory().await;
        mem.add_document("rust programming language", "{}", None).await.unwrap();
        mem.add_document("python scripting language", "{}", None).await.unwrap();
        mem.add_document("javascript web framework", "{}", None).await.unwrap();

        let results = mem.search("rust programming", None, 5).await.unwrap();
        assert!(!results.is_empty());
        assert!(results[0].content.contains("rust"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_vector_search() {
        let mem = test_memory().await;

        // Fake embeddings (3-dim)
        let emb1 = vec![1.0_f32, 0.0, 0.0];
        let emb2 = vec![0.0_f32, 1.0, 0.0];
        let emb3 = vec![0.9_f32, 0.1, 0.0]; // Similar to emb1

        mem.add_document("doc about A", "{}", Some(&emb1)).await.unwrap();
        mem.add_document("doc about B", "{}", Some(&emb2)).await.unwrap();
        mem.add_document("doc about C", "{}", Some(&emb3)).await.unwrap();

        let query_emb = vec![1.0_f32, 0.0, 0.0];
        let results = mem.search("doc", Some(&query_emb), 5).await.unwrap();

        // Should find docs — FTS matches all, vector boosts A and C
        assert!(results.len() >= 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_hybrid_rrf_fusion() {
        let mem = test_memory().await;

        // Doc that matches both FTS and vector
        let emb = vec![1.0_f32, 0.0, 0.0];
        mem.add_document("machine learning AI", "{}", Some(&emb)).await.unwrap();

        // Doc that only matches FTS
        mem.add_document("machine learning deep", "{}", None).await.unwrap();

        // Doc that only matches vector (different text)
        let emb2 = vec![0.95_f32, 0.05, 0.0];
        mem.add_document("unrelated text xyz", "{}", Some(&emb2)).await.unwrap();

        let query_emb = vec![1.0_f32, 0.0, 0.0];
        let results = mem.search("machine learning", Some(&query_emb), 5).await.unwrap();

        // The first result should be "machine learning AI" (matches both)
        assert!(!results.is_empty());
        assert!(results[0].content.contains("machine learning AI"));
        // It should have a higher score than others
        if results.len() > 1 {
            assert!(results[0].score >= results[1].score);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_empty_search() {
        let mem = test_memory().await;
        let results = mem.search("nothing here", None, 5).await.unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_sanitize_fts() {
        assert_eq!(sanitize_fts_query("hello AND world"), "hello AND world");
        assert_eq!(sanitize_fts_query("test(){}*"), "test");
        assert_eq!(sanitize_fts_query("  spaces  "), "spaces");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_metadata_preserved() {
        let mem = test_memory().await;
        let meta = r#"{"source":"test","type":"experiment"}"#;
        let id = mem.add_document("content here", meta, None).await.unwrap();
        let (_, stored_meta) = mem.get_document(id).await.unwrap();
        assert_eq!(stored_meta, meta);
    }
}
