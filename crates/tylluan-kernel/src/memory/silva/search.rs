use anyhow::Result;
use chrono::Utc;
use rusqlite::params;
use std::collections::HashMap;

use super::GraphNode;

impl super::SilvaDB {
    /// Pure Rust vector cosine similarity search on the graph.
    /// Fast path: HNSW → IVF → linear fallback.
    pub async fn search_vector(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<(GraphNode, f32)>> {
        // Fast path: HNSW if index is built (approximate, best for large datasets)
        let hnsw_result = self.search_vector_hnsw(query_embedding, limit).await;
        if let Ok(ref results) = hnsw_result
            && !results.is_empty() {
                return hnsw_result;
            }
        // Try IVF next (optimized path)
        if let Ok(results) = self.search_vector_ivf(query_embedding, limit).await
            && !results.is_empty() {
                return Ok(results);
            }
        // Fallback to linear search
        self.search_vector_linear(query_embedding, limit).await
    }

    /// HNSW approximate nearest neighbor search via instant-distance.
    /// Returns empty results if no HNSW index is loaded or if it returns nothing.
    async fn search_vector_hnsw(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<(GraphNode, f32)>> {
        let hnsw_results = {
            let guard = self.hnsw.read().await;
            let Some(ref state) = *guard else { return Ok(vec![]); };
            let results = crate::memory::silva::hnsw::search_hnsw(state, query_embedding, limit * 3);
            // Collect owned data before guard is dropped
            results.into_iter().map(|(id, dist)| (id.to_string(), dist)).collect::<Vec<_>>()
        };

        if hnsw_results.is_empty() {
            return Ok(vec![]);
        }

        let results: Vec<(GraphNode, f32)> = tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let mut results = Vec::new();
            for (id, dist) in &hnsw_results {
                if let Ok(Some(node)) = self.get_node_sync(id, &conn) {
                    let score = 1.0 - dist;
                    results.push((node, score));
                }
            }
            results
        });
        let truncated = results.into_iter().take(limit).collect();
        Ok(truncated)
    }

    /// Linear vector search (fallback when IVF not available)
    /// Protected by circuit breaker — records success/failure for resilience.
    async fn search_vector_linear(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<(GraphNode, f32)>> {
        if self.cb_vector.check("vector_search").open {
            return Err(anyhow::anyhow!("Vector search circuit breaker is open"));
        }

        let result = tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT node_id, embedding FROM node_embeddings ORDER BY rowid DESC LIMIT 5000"
            )?;

            let mut scored: Vec<(String, f32)> = Vec::new();

            let rows = stmt.query_map([], |row| {
                let id: String = row.get(0)?;
                let blob: Vec<u8> = row.get(1)?;
                Ok((id, blob))
            })?;

            for row in rows.flatten() {
                let (id, blob) = row;
                if blob.is_empty() { continue; }

                // Deserialize f32 LE blob
                let stored: Vec<f32> = blob
                    .chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect();

                if stored.len() != query_embedding.len() { continue; }

                let sim = crate::memory::cosine::cosine_similarity(query_embedding, &stored);
                if sim > 0.05 { // Lower threshold for "light semantic search"
                    scored.push((id, sim));
                }
            }

            // Sort by similarity descending
            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            scored.truncate(limit);

            let mut results = Vec::new();
            for (id, score) in scored {
                if let Ok(Some(node)) = self.get_node_sync(&id, &conn) {
                    results.push((node, score));
                }
            }

            Ok(results)
        });

        match &result {
            Ok(_) => self.cb_vector.record_success("vector_search"),
            Err(_) => self.cb_vector.record_error("vector_search"),
        }
        result
    }

    /// Optimized IVF (Inverted File Index) search using the in-memory mmap store.
    /// Protected by circuit breaker — falls back to linear search on open, and
    /// records success/failure to prevent cascading ONNX/search failures.
    pub async fn search_vector_ivf(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<(GraphNode, f32)>> {
        if self.cb_vector.check("vector_search").open {
            return Err(anyhow::anyhow!("Vector search circuit breaker is open"));
        }

        // Fast path: use in-memory mmap store + IVF searcher
        // Scoped block ensures RwLockReadGuards are dropped before any .await
        let scored_opt: Option<Vec<(String, f32)>> = {
            let ivf_searcher = self.ivf_searcher.read().unwrap();
            let mmap_store = self.mmap_store.read().unwrap();
            match (&*ivf_searcher, &*mmap_store) {
                (Some(searcher), Some(store)) => {
                    let nprobe = 20.min(store.centroids().len());
                    let nearest = searcher.find_nearest_centroids(query_embedding, nprobe);

                    let mut candidate_idxs: Vec<u32> = Vec::new();
                    for centroid_idx in &nearest {
                        if let Some(list) = searcher.inverted_lists().get(*centroid_idx) {
                            candidate_idxs.extend(list);
                        }
                    }

                    let mut scored: Vec<(String, f32)> = Vec::with_capacity(candidate_idxs.len());
                    for &idx in &candidate_idxs {
                        let v = store.get_vector(idx);
                        if v.len() != query_embedding.len() { continue; }
                        let sim = crate::memory::cosine::cosine_similarity(query_embedding, &v);
                        if let Some(nid) = store.index_to_node(idx) {
                            scored.push((nid.to_string(), sim));
                        }
                    }
                    Some(scored)
                }
                _ => None,
            }
        };

        if let Some(mut scored) = scored_opt {
            let result: std::result::Result<Vec<(GraphNode, f32)>, anyhow::Error> = tokio::task::block_in_place(|| {
                let conn = self.conn.blocking_lock();
                scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                scored.truncate(limit);

                let mut results = Vec::new();
                for (id, score) in scored {
                    if let Ok(Some(node)) = self.get_node_sync(&id, &conn) {
                        results.push((node, score));
                    }
                }
                Ok(results)
            });

            match &result {
                Ok(_) => self.cb_vector.record_success("vector_search"),
                Err(_) => self.cb_vector.record_error("vector_search"),
            }
            return result;
        }

        // Fallback: linear search (no IVF store loaded)
        self.search_vector_linear(query_embedding, limit).await
    }

    /// Hybrid search for SilvaDB: Semantic (vector) + Weight-Ranked (topic/text) + Graph-Traversal (LightRAG).
    pub async fn search_hybrid(
        &self,
        query: &str,
        query_embedding: Option<&[f32]>,
        limit: usize,
    ) -> Result<Vec<(GraphNode, f32)>> {
        // Reciprocal Rank Fusion (RRF): score(d) = Σ 1/(k + rank)
        // k=60 is the standard constant (Cormack et al. 2009).
        // Fuses by rank position, not raw score — no normalization needed.
        const K: f32 = 60.0;
        let mut rrf_scores: HashMap<String, (GraphNode, f32)> = HashMap::new();

        let mut vector_results = Vec::new();
        if let Some(emb) = query_embedding {
            vector_results = self.search_vector_ivf(emb, limit).await.unwrap_or_default();
            for (rank, (node, _score)) in vector_results.iter().enumerate() {
                let rrf = 1.0 / (K + rank as f32 + 1.0);
                rrf_scores.entry(node.id.clone())
                    .and_modify(|e| e.1 += rrf)
                    .or_insert((node.clone(), rrf));
            }
        }

        // LightRAG local graph query: use vector search results as seeds for Personalized PageRank local traversal
        if !vector_results.is_empty() {
            let seed_ids: Vec<String> = vector_results.iter().map(|(node, _)| node.id.clone()).collect();
            if let Ok(graph_results) = self.local_query_graph(&seed_ids, limit).await {
                for (rank, (node, _score)) in graph_results.into_iter().enumerate() {
                    let rrf = 1.0 / (K + rank as f32 + 1.0);
                    rrf_scores.entry(node.id.clone())
                        .and_modify(|e| e.1 += rrf)
                        .or_insert((node, rrf));
                }
            }
        }

        let text_results = self.search(query, limit, None).await.unwrap_or_default();
        for (rank, node) in text_results.into_iter().enumerate() {
            let rrf = 1.0 / (K + rank as f32 + 1.0);
            rrf_scores.entry(node.id.clone())
                .and_modify(|e| e.1 += rrf)
                .or_insert((node, rrf));
        }

        // Entity boost: entity/concept nodes get +25% score (more relevant for knowledge graph)
        for entry in rrf_scores.values_mut() {
            let nt = entry.0.node_type.to_lowercase();
            if nt == "entity" || nt == "concept" || nt.starts_with("entity_") {
                entry.1 *= 1.25;
            }
        }

        // Temporal validity penalty: expired nodes lose 90% of their score
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
        for entry in rrf_scores.values_mut() {
            if let Some(until) = entry.0.valid_until
                && until < now { entry.1 *= 0.1; }
        }

        let mut final_results: Vec<(GraphNode, f32)> = rrf_scores.into_values().collect();
        final_results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        final_results.truncate(limit);

        Ok(final_results)
    }

    /// RRF + cross-encoder reranking. Fetches limit*4 candidates via RRF then reorders
    /// with BGE cross-encoder for higher precision. Falls back to RRF order on reranker error.
    pub async fn search_hybrid_reranked(
        &self,
        query: &str,
        query_embedding: Option<&[f32]>,
        limit: usize,
        reranker: &crate::router::embeddings::RerankEngine,
    ) -> Result<Vec<(GraphNode, f32)>> {
        let candidates = self.search_hybrid(query, query_embedding, (limit * 4).min(20)).await?;
        if candidates.is_empty() { return Ok(candidates); }
        let docs: Vec<&str> = candidates.iter().map(|(n, _)| n.content.as_str()).collect();
        let ranked = reranker.rerank(query, &docs).unwrap_or_else(|_| {
            (0..candidates.len()).map(|i| (i, 0.0f32)).collect()
        });
        Ok(ranked.into_iter()
            .take(limit)
            .filter_map(|(idx, score)| candidates.get(idx).map(|(n, _)| (n.clone(), score)))
            .collect())
    }

    fn map_node_row(row: &rusqlite::Row) -> rusqlite::Result<GraphNode> {
        Ok(GraphNode {
            id: row.get(0)?,
            node_type: row.get(1)?,
            content: row.get(2)?,
            metadata: row.get(3)?,
            weight: row.get(4)?,
            protected: row.get::<_, i32>(5)? != 0,
            conflicted: row.get::<_, i32>(6)? != 0,
            topic_key: row.get(7)?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
            valid_from: row.get(10)?,
            valid_until: row.get(11)?,
            shareable: row.get::<_, i32>(12)? != 0,
            last_touched: Utc::now(),
        })
    }

    fn sanitize_fts_query(query: &str) -> String {
        let sanitized: String = query.chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '\'')
            .collect();
        let terms: Vec<String> = sanitized.split_whitespace()
            .filter(|w| !w.is_empty())
            .map(|w| format!("\"{}\"", w))
            .collect();
        if terms.is_empty() { String::new() } else { terms.join(" AND ") }
    }

    pub async fn search(
        &self,
        query: &str,
        max_results: usize,
        types: Option<&[&str]>,
    ) -> Result<Vec<GraphNode>> {
        let fts_query = Self::sanitize_fts_query(query);

        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();

            // Try FTS5 BM25 first, fallback to LIKE
            let results = (|| -> Result<Vec<GraphNode>> {
                if fts_query.is_empty() { return Ok(Vec::new()); }
                let (sql, has_types) = if let Some(type_filter) = types {
                    let placeholders: Vec<String> = type_filter.iter()
                        .enumerate()
                        .map(|(i, _)| format!("?{}", i + 2))
                        .collect();
                    let type_clause = placeholders.join(",");
                    (format!(
                        "SELECT n.id, n.type, n.content, n.metadata, n.weight, n.protected, n.conflicted, n.topic_key, n.created_at, n.updated_at, n.valid_from, n.valid_until, n.shareable
                         FROM nodes_fts f
                         JOIN nodes n ON n.rowid = f.rowid
                         WHERE nodes_fts MATCH ?1
                           AND n.type IN ({})
                         ORDER BY bm25(nodes_fts, 10.0, 5.0, 5.0)
                         LIMIT {}",
                        type_clause, max_results
                    ), true)
                } else {
                    (format!(
                        "SELECT n.id, n.type, n.content, n.metadata, n.weight, n.protected, n.conflicted, n.topic_key, n.created_at, n.updated_at, n.valid_from, n.valid_until, n.shareable
                         FROM nodes_fts f
                         JOIN nodes n ON n.rowid = f.rowid
                         WHERE nodes_fts MATCH ?1
                         ORDER BY bm25(nodes_fts, 10.0, 5.0, 5.0)
                         LIMIT {}",
                        max_results
                    ), false)
                };

                let mut stmt = conn.prepare(&sql)?;
                let results = if has_types {
                    let type_filter = types.unwrap();
                    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
                    param_values.push(Box::new(fts_query));
                    for t in type_filter {
                        param_values.push(Box::new(t.to_string()));
                    }
                    let refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();
                    let rows = stmt.query_map(refs.as_slice(), Self::map_node_row)?;
                    rows.filter_map(|r| r.ok()).collect()
                } else {
                    let rows = stmt.query_map(params![fts_query], Self::map_node_row)?;
                    rows.filter_map(|r| r.ok()).collect()
                };
                Ok(results)
            })();

            if let Ok(r) = results {
                if !r.is_empty() { return Ok(r); }
            }

            // Fallback: LIKE search (original behavior)
            let pattern = format!("%{}%", query.to_lowercase());
            let results = if let Some(type_filter) = types {
                let placeholders: Vec<String> = type_filter.iter()
                    .enumerate()
                    .map(|(i, _)| format!("?{}", i + 2))
                    .collect();
                let type_clause = placeholders.join(",");
                let sql = format!(
                    "SELECT id, type, content, metadata, weight, protected, conflicted, topic_key, created_at, updated_at, valid_from, valid_until, shareable FROM nodes
                     WHERE (LOWER(content) LIKE ?1 OR LOWER(metadata) LIKE ?1)
                     AND type IN ({})
                     ORDER BY weight DESC
                     LIMIT {}",
                    type_clause, max_results
                );
                let mut stmt = conn.prepare(&sql)?;
                let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
                param_values.push(Box::new(pattern));
                for t in type_filter {
                    param_values.push(Box::new(t.to_string()));
                }
                let refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();
                let rows = stmt.query_map(refs.as_slice(), |row| {
                    Ok(GraphNode {
                        id: row.get(0)?,
                        node_type: row.get(1)?,
                        content: row.get(2)?,
                        metadata: row.get(3)?,
                        weight: row.get(4)?,
                        protected: row.get::<_, i32>(5)? != 0,
                        conflicted: row.get::<_, i32>(6)? != 0,
                        topic_key: row.get(7)?,
                        created_at: row.get(8)?,
                        updated_at: row.get(9)?,
                        valid_from: row.get(10)?,
                        valid_until: row.get(11)?,
                        shareable: row.get::<_, i32>(12)? != 0,
                        last_touched: Utc::now(),
                    })
                })?;
                rows.filter_map(|r| r.ok()).collect()
            } else {
                let sql = format!(
                    "SELECT id, type, content, metadata, weight, protected, conflicted, topic_key, created_at, updated_at, valid_from, valid_until, shareable FROM nodes
                     WHERE (LOWER(content) LIKE ?1 OR LOWER(metadata) LIKE ?1)
                     ORDER BY weight DESC
                     LIMIT {}",
                    max_results
                );
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(params![pattern], |row| {
                    Ok(GraphNode {
                        id: row.get(0)?,
                        node_type: row.get(1)?,
                        content: row.get(2)?,
                        metadata: row.get(3)?,
                        weight: row.get(4)?,
                        protected: row.get::<_, i32>(5)? != 0,
                        conflicted: row.get::<_, i32>(6)? != 0,
                        topic_key: row.get(7)?,
                        created_at: row.get(8)?,
                        updated_at: row.get(9)?,
                        valid_from: row.get(10)?,
                        valid_until: row.get(11)?,
                        shareable: row.get::<_, i32>(12)? != 0,
                        last_touched: Utc::now(),
                    })
                })?;
                rows.filter_map(|r| r.ok()).collect()
            };
            Ok(results)
        })
    }
    pub async fn search_content(&self, query: &str, limit: usize) -> Result<Vec<String>> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let pattern = format!("%{}%", query);
            let mut stmt = conn.prepare(
                "SELECT id FROM nodes WHERE content LIKE ?1 LIMIT ?2"
            )?;
            let rows = stmt.query_map(params![pattern, limit as i64], |row| {
                row.get::<_, String>(0)
            })?;
            let mut results = Vec::new();
            for row in rows { results.push(row?); }
            Ok(results)
        })
    }
}
