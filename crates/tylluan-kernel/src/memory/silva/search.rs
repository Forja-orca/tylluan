use anyhow::Result;
use chrono::Utc;
use rusqlite::params;
use std::collections::HashMap;

use super::{ExpectedEmbedding, GraphNode};

/// T1277-D: per-candidate mirror of the `get_stale_embeddings` freshness
/// predicate. A stored row is compatible with THIS process's engine iff its
/// `model_name` matches and its `model_hash` is either unknown (NULL or the
/// `unknown-pre-hash` sentinel — pre-traceability rows, never invalidated just
/// for lacking a fingerprint) or equal to the expected fingerprint. An expected
/// `None` hash accepts any known stored hash (nothing to compare against).
pub(crate) fn embedding_row_matches_engine(
    model_name: &str,
    model_hash: Option<&str>,
    expected: &ExpectedEmbedding,
) -> bool {
    if model_name != expected.model_name {
        return false;
    }
    match model_hash {
        None => true,
        Some(h) if h == super::nodes::MODEL_HASH_UNKNOWN => true,
        Some(_) if expected.model_hash.is_none() => true,
        Some(h) => Some(h) == expected.model_hash.as_deref(),
    }
}

/// ADR-011 §Fase 3: per-recall retrieval-quality features, recorded alongside
/// `rank_position` in `recall_feedback` so the LightReranker trains on real
/// signals instead of the constant proxies (0.0 / 0.5) that carried zero
/// variance. `None` = genuinely not computable (e.g. graph skip, or the
/// recall_cache path that never passes through `search_hybrid`) — never a
/// fabricated value.
#[derive(Debug, Clone, Default)]
pub struct SearchMeta {
    /// Raw RRF contribution of the LightRAG local graph traversal (Personalized
    /// PageRank) per node id, BEFORE the entity boost / temporal penalty.
    /// Empty map = graph step skipped (`skip_graph=true`) or no vector seeds.
    pub graph_contrib: HashMap<String, f32>,
}

impl super::SilvaDB {
    /// Pure Rust vector cosine similarity search on the graph.
    /// Fast path: HNSW â†’ IVF â†’ linear fallback.
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

    /// T1277-E: fingerprint gate for index-backed candidate sets (HNSW / IVF).
    ///
    /// Neither in-memory index stores engine metadata per node (HNSW keeps
    /// `node_ids + points`, IVF/mmap keeps ids + quantized vectors), so a
    /// candidate's provenance can only come from `node_embeddings`. This
    /// resolves it with ONE bounded query over the candidate ids (top-k, never
    /// a table scan) and returns the ids the gate DROPS, feeding the same three
    /// counters as the linear path.
    ///
    /// Inert (no expected fingerprint declared) => returns empty without
    /// touching the DB at all. Ids with no `node_embeddings` row are never
    /// judged (kept): absence is not evidence of a foreign engine.
    fn fingerprint_dropped_ids(
        &self,
        conn: &rusqlite::Connection,
        ids: &[String],
        expected_bytes: usize,
    ) -> Vec<String> {
        let Some(expected) = self.expected_embedding_snapshot() else {
            return Vec::new();
        };
        if ids.is_empty() {
            return Vec::new();
        }
        let filter = self.fingerprint_filter_enabled();

        // Bounded IN(...) lookup, chunked under SQLite's bound-parameter cap.
        const CHUNK: usize = 400;
        let mut verdicts: HashMap<String, (Option<String>, Option<String>, i64)> = HashMap::new();
        for chunk in ids.chunks(CHUNK) {
            let placeholders = vec!["?"; chunk.len()].join(",");
            let sql = format!(
                "SELECT node_id, model_name, model_hash, length(embedding) \
                 FROM node_embeddings WHERE node_id IN ({placeholders})"
            );
            let Ok(mut stmt) = conn.prepare(&sql) else { continue };
            let Ok(rows) = stmt.query_map(rusqlite::params_from_iter(chunk.iter()), |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1).unwrap_or(None),
                    r.get::<_, Option<String>>(2).unwrap_or(None),
                    r.get::<_, Option<i64>>(3).unwrap_or(None).unwrap_or(-1),
                ))
            }) else {
                continue;
            };
            for row in rows.flatten() {
                verdicts.insert(row.0, (row.1, row.2, row.3));
            }
        }

        let mut dropped = Vec::new();
        for id in ids {
            let Some((model_name, model_hash, bytes)) = verdicts.get(id) else {
                continue;
            };
            // Dimension precedence mirrors the linear path (dim check first).
            if *bytes != expected_bytes as i64 {
                self.fp_would_exclude_dim
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            } else if !embedding_row_matches_engine(
                model_name.as_deref().unwrap_or(""),
                model_hash.as_deref(),
                &expected,
            ) {
                self.fp_would_exclude_model
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            } else {
                continue;
            }
            // Observation keeps the candidate; only the flag actually drops it.
            if filter {
                self.fp_excluded
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                dropped.push(id.clone());
            }
        }
        dropped
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
            let conn = self.conn_timed();
            // T1277-E: judge the index-backed candidates by provenance. Inert
            // (no declared fingerprint) => no extra query at all.
            let dropped: std::collections::HashSet<String> =
                if self.expected_embedding_snapshot().is_some() {
                    let ids: Vec<String> =
                        hnsw_results.iter().map(|(id, _)| id.clone()).collect();
                    self.fingerprint_dropped_ids(&conn, &ids, query_embedding.len() * 4)
                        .into_iter()
                        .collect()
                } else {
                    std::collections::HashSet::new()
                };
            let mut results = Vec::new();
            for (id, dist) in &hnsw_results {
                if dropped.contains(id) {
                    continue;
                }
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
    /// Protected by circuit breaker â€” records success/failure for resilience.
    async fn search_vector_linear(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<(GraphNode, f32)>> {
        if self.cb_vector.check("vector_search").open {
            return Err(anyhow::anyhow!("Vector search circuit breaker is open"));
        }

        let expected = self.expected_embedding_snapshot();
        let result = tokio::task::block_in_place(|| {
            let conn = self.conn_timed();
            let mut stmt = conn.prepare(
                "SELECT node_id, embedding, model_name, model_hash FROM node_embeddings ORDER BY rowid DESC LIMIT 5000"
            )?;

            let q_norm_sq: f32 = query_embedding.iter().map(|&v| v * v).sum();
            if q_norm_sq == 0.0 {
                return Ok(vec![]);
            }
            let q_norm = q_norm_sq.sqrt();
            let expected_bytes = query_embedding.len() * 4;

            let mut scored: Vec<(String, f32)> = Vec::new();

            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let blob: &[u8] = match row.get_ref(1) {
                    Ok(rusqlite::types::ValueRef::Blob(b)) => b,
                    _ => continue,
                };
                if blob.len() != expected_bytes {
                    // T1277-D: this drop was silent until now — the only way
                    // to see how many rows carry a foreign dimension. Counted
                    // only when the gate is declared, so "no expected
                    // fingerprint" stays a fully inert no-op.
                    if expected.is_some() {
                        self.fp_would_exclude_dim
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    continue;
                }

                // T1277-D observation gate: a row embedded by a different
                // engine is not comparable with this query vector. Default
                // (flag OFF) counts it and lets it through — no ranking change.
                if let Some(expected) = &expected {
                    let row_model: Option<String> = row.get(2).ok().flatten();
                    let row_hash: Option<String> = row.get(3).ok().flatten();
                    if !embedding_row_matches_engine(
                        row_model.as_deref().unwrap_or(""),
                        row_hash.as_deref(),
                        expected,
                    ) {
                        self.fp_would_exclude_model
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if self.fingerprint_filter_enabled() {
                            self.fp_excluded
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            continue;
                        }
                    }
                }

                // Single-pass dot product + stored norm without allocating Vec<f32>
                let mut dot: f32 = 0.0;
                let mut stored_norm_sq: f32 = 0.0;
                for (chunk, &q) in blob.chunks_exact(4).zip(query_embedding.iter()) {
                    let s = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                    dot += s * q;
                    stored_norm_sq += s * s;
                }

                if stored_norm_sq == 0.0 {
                    continue;
                }

                let sim = dot / (q_norm * stored_norm_sq.sqrt());
                if sim > 0.05 {
                    // Only allocate String if candidate exceeds threshold
                    let id: String = row.get(0)?;
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
    /// Protected by circuit breaker â€” falls back to linear search on open, and
    /// records success/failure to prevent cascading ONNX/search failures.
    pub async fn search_vector_ivf(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<(GraphNode, f32)>> {
        if self.cb_vector.check("vector_search").open {
            return Err(anyhow::anyhow!("Vector search circuit breaker is open"));
        }

        // Fast path: use in-memory mmap store + IVF searcher
        // Scoped block ensures RwLockReadGuards are dropped before any .await
        let scored_opt: Option<Vec<(String, f32)>> = {
            // Category (b): recover a poisoned read lock so one failed writer
            // does not turn every subsequent recall into a process panic.
            let ivf_searcher = self.ivf_searcher.read().unwrap_or_else(|poisoned| poisoned.into_inner());
            let mmap_store = self.mmap_store.read().unwrap_or_else(|poisoned| poisoned.into_inner());
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

                    // Query norm computed ONCE per query (Frente 2 P2 fix):
                    // the old path materialized a dequantized Vec per
                    // candidate AND recomputed both norms per candidate
                    // (3 ndarray passes) — the fused single-pass
                    // store.cosine_query keeps the exact ranking (measured
                    // 0/400 R@10 mismatches) at ~1.73x on the IVF path.
                    let query_norm = (query_embedding.iter().map(|v| v * v).sum::<f32>()).sqrt();
                    let mut scored_idxs: Vec<(u32, f32)> = Vec::with_capacity(candidate_idxs.len());
                    for &idx in &candidate_idxs {
                        if let Some(sim) = store.cosine_query(idx, query_embedding, query_norm) {
                            scored_idxs.push((idx, sim));
                        }
                    }
                    scored_idxs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                    scored_idxs.truncate(limit);

                    let scored: Vec<(String, f32)> = scored_idxs
                        .into_iter()
                        .filter_map(|(idx, sim)| {
                            store.index_to_node(idx).map(|nid| (nid.to_string(), sim))
                        })
                        .collect();
                    Some(scored)
                }
                _ => None,
            }
        };

        if let Some(scored) = scored_opt {
            let result: std::result::Result<Vec<(GraphNode, f32)>, anyhow::Error> = tokio::task::block_in_place(|| {
                let conn = self.conn_timed();
                // T1277-E: same provenance gate as HNSW — the IVF index stores
                // no engine metadata either. Inert => no extra query.
                let dropped: std::collections::HashSet<String> =
                    if self.expected_embedding_snapshot().is_some() {
                        let ids: Vec<String> = scored.iter().map(|(id, _)| id.clone()).collect();
                        self.fingerprint_dropped_ids(&conn, &ids, query_embedding.len() * 4)
                            .into_iter()
                            .collect()
                    } else {
                        std::collections::HashSet::new()
                    };

                let mut results = Vec::new();
                for (id, score) in scored {
                    if dropped.contains(&id) {
                        continue;
                    }
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
    /// When `skip_graph` is true, the LightRAG local graph traversal (Personalized PageRank) is skipped entirely.
    pub async fn search_hybrid(
        &self,
        query: &str,
        query_embedding: Option<&[f32]>,
        limit: usize,
        type_filter: Option<&str>,
        skip_graph: bool,
    ) -> Result<Vec<(GraphNode, f32)>> {
        self.search_hybrid_detailed(query, query_embedding, limit, type_filter, skip_graph)
            .await
            .map(|(results, _meta)| results)
    }

    /// Same fusion as `search_hybrid` plus the [`SearchMeta`]
    /// telemetry (per-node graph contribution). Signature-stable variant: the
    /// ~14 existing `search_hybrid` call sites stay untouched; only the recall
    /// path consumes the extra features (ADR-011 §Fase 3).
    pub async fn search_hybrid_detailed(
        &self,
        query: &str,
        query_embedding: Option<&[f32]>,
        limit: usize,
        type_filter: Option<&str>,
        skip_graph: bool,
    ) -> Result<(Vec<(GraphNode, f32)>, SearchMeta)> {
        const K: f32 = 60.0;
        let mut rrf_scores: HashMap<String, (GraphNode, f32)> = HashMap::new();
        // Always-on in the detailed variant (its whole point is the telemetry);
        // the plain `search_hybrid` wrapper discards it. Inlining this fn for
        // the plain path would hard-code None here and keep the hot path at
        // zero allocation for the telemetry map.
        let mut graph_meta: Option<SearchMeta> = Some(SearchMeta::default());

        let mut vector_results = Vec::new();
        if let Some(emb) = query_embedding {
            // Embedding caching lives at the callers (api_memory, think,
            // autolink...) via query_embed_cache.get_or_embed BEFORE embed() â€”
            // a post-hoc cache here can't skip the ONNX cost (see ff205b1
            // history). search_hybrid just consumes the passed embedding.
            vector_results = self.search_vector_ivf(emb, limit).await.unwrap_or_default();
            for (rank, (node, _score)) in vector_results.iter().enumerate() {
                let rrf = 1.0 / (K + rank as f32 + 1.0);
                rrf_scores.entry(node.id.clone())
                    .and_modify(|e| e.1 += rrf)
                    .or_insert((node.clone(), rrf));
            }
        }

        // LightRAG local graph query: use vector search results as seeds for Personalized PageRank local traversal
        if !skip_graph && !vector_results.is_empty() {
            let seed_ids: Vec<String> = vector_results.iter().map(|(node, _)| node.id.clone()).collect();
            if let Ok(graph_results) = self.local_query_graph(&seed_ids, limit).await {
                for (rank, (node, _score)) in graph_results.into_iter().enumerate() {
                    let rrf = 1.0 / (K + rank as f32 + 1.0);
                    // ADR-011 §Fase 3: record the graph's own contribution so
                    // recall_feedback can store score_graph per row. Additive
                    // telemetry — the fusion below is untouched.
                    if let Some(meta) = graph_meta.as_mut() {
                        meta.graph_contrib.entry(node.id.clone())
                            .and_modify(|v| *v += rrf)
                            .or_insert(rrf);
                    }
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

        // Learned-sparse source (BGE-M3 sparse head, opt-in via hybrid_sparse_enabled).
        // Works even without a query embedding (text-only mode) since it needs only
        // the raw query. Nodes with no stored sparse vector are simply absent â€”
        // graceful degradation to the pre-existing 3-source fusion.
        if let Some(sparse_engine) = self.sparse_engine_ref()
            && let Ok(qsv) = tokio::task::block_in_place(|| sparse_engine.embed(query))
            && let Ok(sparse_results) = self.search_sparse_candidates(&qsv, limit).await
        {
            for (rank, node) in sparse_results.into_iter().enumerate() {
                let rrf = 1.0 / (K + rank as f32 + 1.0);
                rrf_scores.entry(node.id.clone())
                    .and_modify(|e| e.1 += rrf)
                    .or_insert((node, rrf));
            }
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

        // ASI06: exclude quarantined nodes from recall. Filtered here (post-fusion,
        // single query) rather than in each of the 4 upstream sources (vector/graph/
        // BM25-LIKE/sparse) -- one choke point, can't be missed by adding a 5th
        // source later. Quarantined content stays in SilvaDB for manual review, it
        // just never surfaces through tylluan_recall.
        if !final_results.is_empty() {
            let quarantined_ids = self.quarantined_ids_among(
                &final_results.iter().map(|(n, _)| n.id.clone()).collect::<Vec<_>>()
            ).await.unwrap_or_default();
            if !quarantined_ids.is_empty() {
                final_results.retain(|(node, _)| !quarantined_ids.contains(&node.id));
            }
        }

        // Apply post-RRF type filter if provided
        if let Some(filter) = type_filter {
            final_results.retain(|(node, _)| node.node_type.to_lowercase() == filter.to_lowercase());
        }

        final_results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        final_results.truncate(limit);

        let meta = graph_meta.unwrap_or_default();
        Ok((final_results, meta))
    }

    /// Agent-facing hybrid recall with an explicit archived-node policy.
    /// The existing internal search remains unchanged; this choke point applies
    /// the lifecycle filter after all retrieval sources have been fused.
    pub async fn search_hybrid_for_recall(
        &self,
        query: &str,
        query_embedding: Option<&[f32]>,
        limit: usize,
        type_filter: Option<&str>,
        skip_graph: bool,
        include_archived: bool,
    ) -> Result<Vec<(GraphNode, f32)>> {
        self.search_hybrid_for_recall_detailed(query, query_embedding, limit, type_filter, skip_graph, include_archived)
            .await
            .map(|(results, _meta)| results)
    }

    /// Same recall policy as `search_hybrid_for_recall` plus the
    /// [`SearchMeta`] telemetry (per-node LightRAG graph contribution) so the
    /// ADR-011 Signal Loop can persist real per-row features. Signature-stable
    /// variant: the existing `search_hybrid_for_recall` call sites stay
    /// untouched.
    pub async fn search_hybrid_for_recall_detailed(
        &self,
        query: &str,
        query_embedding: Option<&[f32]>,
        limit: usize,
        type_filter: Option<&str>,
        skip_graph: bool,
        include_archived: bool,
    ) -> Result<(Vec<(GraphNode, f32)>, SearchMeta)> {
        let source_limit = if include_archived {
            limit
        } else {
            limit.saturating_mul(3).max(limit)
        };
        let (mut results, meta) = self
            .search_hybrid_detailed(query, query_embedding, source_limit, type_filter, skip_graph)
            .await?;
        if !include_archived {
            let archived_ids = self
                .archived_lifecycle_ids_among(
                    &results.iter().map(|(node, _)| node.id.clone()).collect::<Vec<_>>(),
                )
                .await
                .unwrap_or_default();
            results.retain(|(node, _)| !archived_ids.contains(&node.id));
        }
        results.truncate(limit);
        // Honest abstention (opt-in): if the floor is set and even the top
        // fused score is below it, return "no sufficiently relevant memory"
        // instead of forcing noise through as memory. Logged with its own
        // reason so negative memory distinguishes empty from weak â€” guarded
        // below so the generic 'empty' insert doesn't also fire and blur
        // that distinction.
        let mut abstained = false;
        if !results.is_empty() {
            let floor = self.abstain_floor_x1000.load(std::sync::atomic::Ordering::Relaxed);
            if floor > 0 {
                let top = results[0].1;
                if (top as f64) < (floor as f64 / 1000.0) {
                    use std::collections::hash_map::DefaultHasher;
                    use std::hash::{Hash, Hasher};
                    let mut h = DefaultHasher::new();
                    query.hash(&mut h);
                    let qhash = format!("{:016x}", h.finish());
                    let _ = tokio::task::block_in_place(|| {
                        let conn = self.conn_timed();
                        conn.execute(
                            "INSERT INTO recall_misses (ts, query_hash, reason) VALUES (?1, ?2, 'below_relevance_floor')",
                            params![std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs() as i64, qhash],
                        )
                    });
                    tracing::info!(gen_ai.operation.name = "retrieval",
                        "recall: honest abstention â€” top fused score {top:.4} below floor {:.3}", floor as f64 / 1000.0);
                    results.clear();
                    abstained = true;
                }
            }
        }
        // Memoria negativa: record that this query found nothing. Hash only â€”
        // raw queries may carry personal data (soberanÃ­a). This is the seed of
        // "what was searched and not found" so learning isn't biased toward
        // what survived the pipeline. Skipped when abstention already logged
        // its own more specific reason above, so one recall never writes two
        // recall_misses rows with contradictory reasons.
        if results.is_empty() && !abstained {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut h = DefaultHasher::new();
            query.hash(&mut h);
            let qhash = format!("{:016x}", h.finish());
            let _ = tokio::task::block_in_place(|| {
                let conn = self.conn_timed();
                conn.execute(
                    "INSERT INTO recall_misses (ts, query_hash, reason) VALUES (?1, ?2, 'empty')",
                    params![std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64, qhash],
                )
            });
        }
        Ok((results, meta))
    }

    /// Stage-1 body shared by the cascade and the diagnostic probe: fuse
    /// FTS5 + learned-sparse lexically with per-source bits (1=fts, 2=sparse,
    /// 3=both), sorted by fused score desc, truncated to 2Ã—limit.
    async fn lexical_stage1_fuse(
        &self,
        query: &str,
        qsv: Option<&crate::router::embeddings::SparseVec>,
        limit: usize,
    ) -> Result<Vec<(GraphNode, f32, u8)>> {
        const K: f32 = 60.0;
        let mut fused: HashMap<String, (GraphNode, f32, u8)> = HashMap::new();

        let text_results = self.search(query, limit * 2, None).await.unwrap_or_default();
        for (rank, node) in text_results.into_iter().enumerate() {
            let rrf = 1.0 / (K + rank as f32 + 1.0);
            fused.entry(node.id.clone())
                .and_modify(|e| { e.1 += rrf; })
                .or_insert((node.clone(), rrf, 1));
        }
        if let Some(qsv) = qsv
            && let Ok(sparse_nodes) = self.search_sparse_candidates(qsv, limit * 2).await
        {
            for (rank, node) in sparse_nodes.into_iter().enumerate() {
                let rrf = 1.0 / (K + rank as f32 + 1.0);
                fused.entry(node.id.clone())
                    .and_modify(|e| {
                        e.1 += rrf;
                        e.2 |= 2;
                    })
                    .or_insert((node.clone(), rrf, 2));
            }
        }

        let mut v: Vec<(String, (GraphNode, f32, u8))> = fused.into_iter().collect();
        v.sort_by(|a, b| b.1 .1.partial_cmp(&a.1 .1).unwrap_or(std::cmp::Ordering::Equal));
        v.truncate(limit * 2);
        Ok(v.into_iter().map(|(_, t)| t).collect())
    }

    /// Diagnostic probe for benchmarks/ops: what would stage 1 see for this
    /// query right now? Returns (agreement, lexical_total, fts_only,
    /// sparse_only). Cheap (no dense embed).
    pub async fn cascade_stage1_stats(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<(usize, usize, usize, usize)> {
        let qsv = self.sparse_engine_ref().and_then(|e| e.embed(query).ok());
        let lexical = self.lexical_stage1_fuse(query, qsv.as_ref(), limit).await?;
        let agreement = lexical.iter().filter(|(_, _, s)| *s == 3).count();
        let fts_only = lexical.iter().filter(|(_, _, s)| *s == 1).count();
        let sparse_only = lexical.iter().filter(|(_, _, s)| *s == 2).count();
        Ok((agreement, lexical.len(), fts_only, sparse_only))
    }

    /// Two-stage retrieval cascade (arXiv:2404.13357 Two-Step SPLADE pattern),
    /// opt-in via `[silva] cascade_enabled`.
    ///
    /// Stage 1 (cheap): FTS5 + learned-sparse lexical fusion. If enough results
    /// are backed by BOTH independent lexical signals (`cascade_gate`), return
    /// them WITHOUT paying the dense query embed (2-8s CPU per query).
    /// Stage 2 (full): embed via the installed dense engine (query_embed_cache-
    /// backed) and run the standard 4-source `search_hybrid_for_recall`.
    ///
    /// Returns (results, Option<embedding>) â€” callers forward the embedding to
    /// downstream consumers (CoherenceGate) so a stage-2 hit keeps full parity.
    /// A stage-1 hit returns None there; that matches BM25-only-mode behavior.
    ///
    /// Degraded mode: cascade enabled but dense engine never installed â†’ warn +
    /// lexical-only results (resilient; visible in logs, opt-in feature).
    pub async fn search_recall_cascade(
        &self,
        query: &str,
        limit: usize,
        type_filter: Option<&str>,
        include_archived: bool,
    ) -> Result<(Vec<(GraphNode, f32)>, Option<Vec<f32>>)> {
        // Stage 1: sparse-embed the query when possible; FTS5 always available.
        let qsv = match self.sparse_engine_ref() {
            Some(engine) => tokio::task::block_in_place(|| engine.embed(query)).ok(),
            None => None,
        };
        self.search_recall_cascade_inner(query, qsv.as_ref(), limit, type_filter, include_archived)
            .await
    }

    /// Cascade body with injectable query-sparse-vector (testability without
    /// downloading ONNX models). Production entry is `search_recall_cascade`.
    pub(crate) async fn search_recall_cascade_inner(
        &self,
        query: &str,
        qsv: Option<&crate::router::embeddings::SparseVec>,
        limit: usize,
        type_filter: Option<&str>,
        include_archived: bool,
    ) -> Result<(Vec<(GraphNode, f32)>, Option<Vec<f32>>)> {
        // â”€â”€ Stage 1: lexical-only fusion with per-source agreement tracking â”€â”€
        let lexical = self.lexical_stage1_fuse(query, qsv, limit).await?;

        let agreement = lexical.iter().filter(|(_, _, src)| *src == 3).count();
        if cascade_gate(agreement, lexical.len(), limit) {
            let mut results: Vec<(GraphNode, f32)> =
                lexical.into_iter().map(|(n, s, _)| (n, s)).collect();
            self.apply_recall_filters(&mut results, type_filter, include_archived).await?;
            results.truncate(limit);
            tracing::info!(
                gen_ai.operation.name = "retrieval",
                "cascade: stage-1 hit (agreement={agreement}, total={}) â€” dense embed skipped", results.len()
            );
            return Ok((results, None));
        }

        // â”€â”€ Stage 2: full fusion with a freshly-obtained dense embedding â”€â”€
        let Some(engine) = self.dense_engine_ref() else {
            tracing::warn!("cascade enabled but no dense engine installed â€” degrading to lexical-only results");
            let mut results: Vec<(GraphNode, f32)> =
                lexical.into_iter().map(|(n, s, _)| (n, s)).collect();
            self.apply_recall_filters(&mut results, type_filter, include_archived).await?;
            results.truncate(limit);
            return Ok((results, None));
        };
        let emb = self.query_embed_cache.get_or_embed(query, |q| engine.embed_batch_coalesced(q))?;
        let results = self
            .search_hybrid_for_recall(query, Some(&emb), limit, type_filter, false, include_archived)
            .await?;
        tracing::info!(gen_ai.operation.name = "retrieval", "cascade: stage-2 full fusion (agreement={agreement})");
        Ok((results, Some(emb)))
    }

    /// Shared post-fusion filters for the cascade paths (quarantine ASI06 +
    /// lifecycle archived + optional type filter) â€” same policy as the
    /// standard recall path.
    async fn apply_recall_filters(
        &self,
        results: &mut Vec<(GraphNode, f32)>,
        type_filter: Option<&str>,
        include_archived: bool,
    ) -> Result<()> {
        if !results.is_empty() {
            let quarantined_ids = self.quarantined_ids_among(
                &results.iter().map(|(n, _)| n.id.clone()).collect::<Vec<_>>()
            ).await.unwrap_or_default();
            if !quarantined_ids.is_empty() {
                results.retain(|(node, _)| !quarantined_ids.contains(&node.id));
            }
        }
        if !include_archived {
            let archived_ids = self.archived_lifecycle_ids_among(
                &results.iter().map(|(n, _)| n.id.clone()).collect::<Vec<_>>()
            ).await.unwrap_or_default();
            results.retain(|(node, _)| !archived_ids.contains(&node.id));
        }
        if let Some(filter) = type_filter {
            results.retain(|(node, _)| node.node_type.to_lowercase() == filter.to_lowercase());
        }
        Ok(())
    }

    /// RRF + cross-encoder reranking. Fetches limit*4 candidates via RRF then reorders
    /// with BGE cross-encoder for higher precision. Falls back to RRF order on reranker error.
    pub async fn search_hybrid_reranked(
        &self,
        query: &str,
        query_embedding: Option<&[f32]>,
        limit: usize,
        reranker: &crate::router::embeddings::RerankEngine,
        skip_graph: bool,
    ) -> Result<Vec<(GraphNode, f32)>> {
        let candidates = self.search_hybrid(query, query_embedding, (limit * 4).min(20), None, skip_graph).await?;
        if candidates.is_empty() { return Ok(candidates); }
        let docs: Vec<&str> = candidates.iter().map(|(n, _)| n.content.as_str()).collect();
        let ranked = tokio::task::block_in_place(|| reranker.rerank(query, &docs)).unwrap_or_else(|_| {
            (0..candidates.len()).map(|i| (i, 0.0f32)).collect()
        });
        Ok(ranked.into_iter()
            .take(limit)
            .filter_map(|(idx, score)| candidates.get(idx).map(|(n, _)| (n.clone(), score)))
            .collect())
    }

    /// Rank nodes by learned-sparse lexical match against the query vector.
    /// Single JOINed query (no N+1); dot product over shared indices; returns
    /// only nodes with a positive score, best first. Lifecycle/quarantine
    /// filtering stays post-fusion in search_hybrid (same as the other sources).
    pub(crate) async fn search_sparse_candidates(
        &self,
        query: &crate::router::embeddings::SparseVec,
        limit: usize,
    ) -> Result<Vec<GraphNode>> {
        use crate::router::embeddings::SparseVec;
        let mut scored: Vec<(GraphNode, f32)> = tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT n.id, n.type, n.content, n.metadata, n.weight, n.protected, n.conflicted,
                        n.topic_key, n.created_at, n.updated_at, n.valid_from, n.valid_until,
                        n.shareable, n.content_hash, n.provenance,
                        s.indices, s.vals
                 FROM node_sparse_embeddings s
                 JOIN nodes n ON n.id = s.node_id
                 WHERE n.weight > 0.005 LIMIT 20000"
            )?;
            let mapped = stmt.query_map([], |row| {
                let node = Self::map_node_row(row)?;
                let ib: Vec<u8> = row.get(15)?;
                let vb: Vec<u8> = row.get(16)?;
                Ok((node, ib, vb))
            })?;
            let mut out: Vec<(GraphNode, f32)> = Vec::new();
            for r in mapped {
                let (node, ib, vb) = r?;
                let Ok(sv) = SparseVec::from_bytes(&ib, &vb) else { continue };
                let score = query.dot(&sv);
                if score > 0.0 {
                    out.push((node, score));
                }
            }
            Ok::<_, anyhow::Error>(out)
        })?;
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);
        Ok(scored.into_iter().map(|(n, _)| n).collect())
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
            provenance: row.get::<_, String>(13).unwrap_or_default(),
            content_hash: "".to_string(),
            last_touched: Utc::now(),
        })
    }

    /// FTS5 query sanitizer. Non-alphanumeric characters become SPACES — never
    /// deleted — so hyphenated terms split into their real tokens
    /// (`portability-roundtrip-marker` → "portability" AND "roundtrip" AND
    /// "marker"). Deleting them glued words into single tokens that matched
    /// nothing in the document side of the FTS index (unicode61 splits on the
    /// same punctuation), silently zeroing the silva leg of recall for any
    /// hyphenated query — masked for months by HybridMemory's dual-write, whose
    /// sanitizer already mapped punctuation to spaces. Exposed as a CI failure
    /// ("Portability round-trip") once F1 closed the dual-write.
    fn sanitize_fts_query(query: &str) -> String {
        let sanitized: String = query.chars()
            .map(|c| if c.is_alphanumeric() { c } else { ' ' })
            .collect();
        let terms: Vec<String> = sanitized.split_whitespace()
            .map(|w| format!("\"{w}\""))
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
                        "SELECT n.id, n.type, n.content, n.metadata, n.weight, n.protected, n.conflicted, n.topic_key, n.created_at, n.updated_at, n.valid_from, n.valid_until, n.shareable, n.provenance
                         FROM nodes_fts f
                         JOIN nodes n ON n.rowid = f.rowid
                         WHERE nodes_fts MATCH ?1
                           AND n.type IN ({type_clause})
                         ORDER BY bm25(nodes_fts, 10.0, 5.0, 5.0)
                         LIMIT {max_results}"
                    ), true)
                } else {
                    (format!(
                        "SELECT n.id, n.type, n.content, n.metadata, n.weight, n.protected, n.conflicted, n.topic_key, n.created_at, n.updated_at, n.valid_from, n.valid_until, n.shareable, n.provenance
                         FROM nodes_fts f
                         JOIN nodes n ON n.rowid = f.rowid
                         WHERE nodes_fts MATCH ?1
                         ORDER BY bm25(nodes_fts, 10.0, 5.0, 5.0)
                         LIMIT {max_results}"
                    ), false)
                };

                let mut stmt = conn.prepare(&sql)?;
                let results = if has_types {
                    // `has_types` is derived from `types.is_some()` above; use
                    // the checked branch directly instead of relying on unwrap.
                    let Some(type_filter) = types else { return Ok(Vec::new()); };
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

            if let Ok(r) = results
                && !r.is_empty() { return Ok(r); }

            // Fallback: LIKE search (original behavior)
            let pattern = format!("%{}%", query.to_lowercase());
            let results = if let Some(type_filter) = types {
                let placeholders: Vec<String> = type_filter.iter()
                    .enumerate()
                    .map(|(i, _)| format!("?{}", i + 2))
                    .collect();
                let type_clause = placeholders.join(",");
                let sql = format!(
                    "SELECT id, type, content, metadata, weight, protected, conflicted, topic_key, created_at, updated_at, valid_from, valid_until, shareable, provenance FROM nodes
                     WHERE (LOWER(content) LIKE ?1 OR LOWER(metadata) LIKE ?1)
                     AND type IN ({type_clause})
                     ORDER BY weight DESC
                     LIMIT {max_results}"
                );
                let mut stmt = conn.prepare(&sql)?;
                let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
                param_values.push(Box::new(pattern));
                for t in type_filter {
                    param_values.push(Box::new(t.to_string()));
                }
                let refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();
                let rows = stmt.query_map(refs.as_slice(), Self::map_node_row)?;
                rows.filter_map(|r| r.ok()).collect()
            } else {
                let sql = format!(
                    "SELECT id, type, content, metadata, weight, protected, conflicted, topic_key, created_at, updated_at, valid_from, valid_until, shareable, provenance FROM nodes
                     WHERE (LOWER(content) LIKE ?1 OR LOWER(metadata) LIKE ?1)
                     ORDER BY weight DESC
                     LIMIT {max_results}"
                );
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(params![pattern], Self::map_node_row)?;
                rows.filter_map(|r| r.ok()).collect()
            };
            Ok(results)
        })
    }
    pub async fn search_content(&self, query: &str, limit: usize) -> Result<Vec<String>> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let pattern = format!("%{query}%");
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

    /// Return lifecycle-archived IDs among a candidate set.
    pub(crate) async fn archived_lifecycle_ids_among(
        &self,
        ids: &[String],
    ) -> Result<std::collections::HashSet<String>> {
        if ids.is_empty() {
            return Ok(std::collections::HashSet::new());
        }
        let ids = ids.to_vec();
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT id FROM nodes WHERE lifecycle_state = 'archived' AND id IN ({placeholders})"
            );
            let mut stmt = conn.prepare(&sql)?;
            let params_slice: Vec<&dyn rusqlite::ToSql> =
                ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
            let rows = stmt.query_map(params_slice.as_slice(), |row| row.get::<_, String>(0))?;
            let mut out = std::collections::HashSet::new();
            for row in rows {
                out.insert(row?);
            }
            Ok(out)
        })
    }

    /// ASI06: given a set of node ids, return which of them are currently
    /// quarantined. Used by `search_hybrid` as a single post-fusion filter
    /// point instead of threading the check through every candidate source.
    pub(crate) async fn quarantined_ids_among(&self, ids: &[String]) -> Result<std::collections::HashSet<String>> {
        if ids.is_empty() {
            return Ok(std::collections::HashSet::new());
        }
        let ids = ids.to_vec();
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!("SELECT id FROM nodes WHERE quarantined = 1 AND id IN ({placeholders})");
            let mut stmt = conn.prepare(&sql)?;
            let params_slice: Vec<&dyn rusqlite::ToSql> = ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
            let rows = stmt.query_map(params_slice.as_slice(), |row| row.get::<_, String>(0))?;
            let mut out = std::collections::HashSet::new();
            for row in rows { out.insert(row?); }
            Ok(out)
        })
    }
}

/// Stage-1 gate for the recall cascade: pass to lexical-only results iff the
/// two independent lexical signals (FTS5 + learned-sparse) agree on at least
/// `CASCADE_MIN_AGREEMENT` nodes AND stage 1 filled at least half the budget.
/// Agreement of independent signals is the relevance proxy â€” this is why RRF
/// works at all; a single-source hit is never trusted to skip the dense path.
pub(crate) const CASCADE_MIN_AGREEMENT: usize = 3;

pub(crate) fn cascade_gate(agreement_count: usize, lexical_total: usize, limit: usize) -> bool {
    agreement_count >= CASCADE_MIN_AGREEMENT && lexical_total >= (limit / 2).max(1)
}

#[cfg(test)]
mod asi06_tests {
    use super::super::SilvaDB;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn quarantined_ids_among_returns_only_quarantined() {
        let db = SilvaDB::in_memory().await.unwrap();
        db.upsert_node("n1", "note", "safe content", "{}").await.unwrap();
        db.upsert_node("n2", "note", "flagged content", "{}").await.unwrap();
        tokio::task::block_in_place(|| {
            let conn = db.conn.blocking_lock();
            conn.execute("UPDATE nodes SET quarantined = 1 WHERE id = 'n2'", []).unwrap();
        });

        let ids = db.quarantined_ids_among(&["n1".to_string(), "n2".to_string()]).await.unwrap();
        assert!(!ids.contains("n1"));
        assert!(ids.contains("n2"));
        assert_eq!(ids.len(), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn quarantined_ids_among_empty_input_returns_empty() {
        let db = SilvaDB::in_memory().await.unwrap();
        let ids = db.quarantined_ids_among(&[]).await.unwrap();
        assert!(ids.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn search_hybrid_excludes_quarantined_nodes_from_bm25_path() {
        let db = SilvaDB::in_memory().await.unwrap();
        db.upsert_node("keep", "note", "tylluan sovereign memory design", "{}").await.unwrap();
        db.upsert_node("quarantined", "note", "tylluan sovereign memory design flagged", "{}").await.unwrap();
        tokio::task::block_in_place(|| {
            let conn = db.conn.blocking_lock();
            conn.execute("UPDATE nodes SET quarantined = 1 WHERE id = 'quarantined'", []).unwrap();
        });

        let results = db.search_hybrid("tylluan sovereign memory design", None, 10, None, true).await.unwrap();
        assert!(results.iter().any(|(n, _)| n.id == "keep"));
        assert!(!results.iter().any(|(n, _)| n.id == "quarantined"), "quarantined node must not appear in recall results");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn archived_lifecycle_is_opt_in_and_reactivation_is_counted() {
        let db = SilvaDB::in_memory().await.unwrap();
        db.upsert_node("active", "note", "lifecycle archive policy test", "{}").await.unwrap();
        db.upsert_node("archived", "note", "lifecycle archive policy test", "{}").await.unwrap();
        tokio::task::block_in_place(|| {
            let conn = db.conn.blocking_lock();
            conn.execute("UPDATE nodes SET lifecycle_state = 'archived' WHERE id = 'archived'", []).unwrap();
        });

        let default_results = db.search_hybrid_for_recall(
            "lifecycle archive policy test", None, 10, None, true, false,
        ).await.unwrap();
        assert!(!default_results.iter().any(|(node, _)| node.id == "archived"));

        let explicit_results = db.search_hybrid_for_recall(
            "lifecycle archive policy test", None, 10, None, true, true,
        ).await.unwrap();
        assert!(explicit_results.iter().any(|(node, _)| node.id == "archived"));

        db.record_agent_access("archived", 1_700_000_000).await.unwrap();
        let (state, count, access): (String, i64, i64) = tokio::task::block_in_place(|| {
            let conn = db.conn.blocking_lock();
            conn.query_row(
                "SELECT lifecycle_state, reactivation_count, last_agent_access FROM nodes WHERE id = 'archived'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            ).unwrap()
        });
        assert_eq!(state, "active");
        assert_eq!(count, 1);
        assert_eq!(access, 1_700_000_000);

        db.record_agent_access("archived", 1_700_000_001).await.unwrap();
        let count: i64 = tokio::task::block_in_place(|| {
            let conn = db.conn.blocking_lock();
            conn.query_row("SELECT reactivation_count FROM nodes WHERE id = 'archived'", [], |row| row.get(0)).unwrap()
        });
        assert_eq!(count, 1, "active follow-up access must not increment reactivation_count");
    }
}

#[cfg(test)]
mod cascade_tests {
    use super::{super::SilvaDB, cascade_gate};
    use crate::router::embeddings::SparseVec;

    #[test]
    fn cascade_gate_requires_both_conditions() {
        // Fewer than CASCADE_MIN_AGREEMENT dual-source hits â†’ never pass.
        assert!(!cascade_gate(0, 20, 10));
        assert!(!cascade_gate(2, 20, 10));
        // Enough agreement but stage 1 did not fill half the budget â†’ no pass.
        assert!(!cascade_gate(3, 2, 10));
        assert!(!cascade_gate(4, 4, 10));
        // Both conditions met â†’ pass.
        assert!(cascade_gate(3, 5, 10));
        assert!(cascade_gate(4, 10, 8));
        // Tiny limit: total >= max(limit/2, 1).
        assert!(cascade_gate(3, 1, 1));
        assert!(!cascade_gate(3, 0, 1));
    }

    async fn seed_agreeing_nodes(db: &SilvaDB) {
        for i in 0..4 {
            db.upsert_node(
                &format!("q{i}"),
                "note",
                &format!("quantumflux oscillator calibration run {i}"),
                "{}",
            ).await.unwrap();
        }
    }

    fn overlapping_qsv() -> SparseVec {
        SparseVec { indices: vec![10, 11, 12], values: vec![1.0, 1.0, 1.0] }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cascade_stage1_hit_skips_dense_and_returns_none_embedding() {
        let db = SilvaDB::in_memory().await.unwrap();
        seed_agreeing_nodes(&db).await;
        // Manual sparse signatures that all overlap the canned query vector â€”
        // no ONNX model needed: agreement comes from injected vectors.
        let sv = SparseVec { indices: vec![10, 11, 12], values: vec![1.0, 1.0, 1.0] };
        for i in 0..4 {
            db.save_sparse_embedding(&format!("q{i}"), &sv).await.unwrap();
        }

        let (results, emb) = db
            .search_recall_cascade_inner("quantumflux oscillator", Some(&overlapping_qsv()), 8, None, false)
            .await
            .unwrap();

        assert!(emb.is_none(), "stage-1 hit must not return an embedding (dense was skipped)");
        assert!(results.len() >= 3, "expected the agreeing nodes back, got {}", results.len());
        for i in 0..4 {
            assert!(results.iter().any(|(n, _)| n.id == format!("q{i}")), "q{i} missing from stage-1 results");
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cascade_without_sparse_agreement_degrades_gracefully_without_dense_engine() {
        let db = SilvaDB::in_memory().await.unwrap();
        seed_agreeing_nodes(&db).await;
        // Disjoint sparse signature: FTS matches but sparse never agrees â†’ gate fails.
        let disjoint = SparseVec { indices: vec![777], values: vec![1.0] };
        for i in 0..4 {
            db.save_sparse_embedding(&format!("q{i}"), &disjoint).await.unwrap();
        }

        let (results, emb) = db
            .search_recall_cascade_inner("quantumflux oscillator", Some(&overlapping_qsv()), 8, None, false)
            .await
            .unwrap();

        assert!(emb.is_none());
        assert!(!results.is_empty(), "degraded path still returns lexical (FTS-only) results");
        assert!(results.iter().any(|(n, _)| n.id.starts_with("q")));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cascade_stage1_respects_quarantine_filter() {
        let db = SilvaDB::in_memory().await.unwrap();
        seed_agreeing_nodes(&db).await;
        let sv = SparseVec { indices: vec![10, 11, 12], values: vec![1.0, 1.0, 1.0] };
        for i in 0..4 {
            db.save_sparse_embedding(&format!("q{i}"), &sv).await.unwrap();
        }
        tokio::task::block_in_place(|| {
            let conn = db.conn.blocking_lock();
            conn.execute("UPDATE nodes SET quarantined = 1 WHERE id = 'q0'", []).unwrap();
        });

        let (results, _) = db
            .search_recall_cascade_inner("quantumflux oscillator", Some(&overlapping_qsv()), 8, None, false)
            .await
            .unwrap();

        assert!(!results.iter().any(|(n, _)| n.id == "q0"), "quarantined node must not surface via cascade stage-1");
    }
}

/// Regresión de CI "Portability round-trip" (run 37388000510, rojo en 2bc090a,
/// verde en 5b731c3): el leg silva de recall no encontraba el marcador con
/// `embedding_model="none"` porque `sanitize_fts_query` BORRABA los guiones
/// (uniendo `portability-roundtrip-marker` en un token inexistente) mientras el
/// sanitizador de HybridMemory los convertía en espacios. El dual-write (F1)
/// tapaba el hueco: con la escritura híbrida retirada, el leg silva pasó a ser
/// el único y el bug quedó expuesto.
#[cfg(test)]
mod ci_roundtrip_regression {
    use super::super::SilvaDB;

    #[test]
    fn sanitize_fts_query_keeps_word_boundaries_for_hyphenated_terms() {
        let q = SilvaDB::sanitize_fts_query("agent: ci-portability-test portability-roundtrip-marker");
        assert_eq!(
            q,
            "\"agent\" AND \"ci\" AND \"portability\" AND \"test\" AND \"portability\" AND \"roundtrip\" AND \"marker\"",
            "hyphens/colons must split words, not glue them into a single impossible token"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn recall_finds_agent_memory_marker_without_dense_embedding() {
        let db = SilvaDB::in_memory().await.unwrap();
        // Exactamente lo que AgentMemoryManager::record_memory escribe
        // (agent_memory.rs:187-201): tipo agent_memory, contenido etiquetado
        // [agent_id], metadata con agent_id. Marcador del job de CI.
        db.upsert_node_with_validity(
            "agent_memory:ci-portability-test:8997ff6d398e417cae3c7ab866134cc3",
            "agent_memory",
            "[ci-portability-test] portability-roundtrip-marker-f6ee419a-39ef-4848-9eb7-49e401475924",
            r#"{"agent_id":"ci-portability-test","importance":0.7}"#,
            super::super::NodeWriteOptions::new("agent_generated"),
        )
        .await
        .unwrap();

        // effective_query exacto que construye handler_recall.rs:511-513 cuando
        // viene agent_id; embedding None = modo embedding_model="none" de CI.
        let (results, _meta) = db
            .search_hybrid_for_recall_detailed(
                "agent: ci-portability-test portability-roundtrip-marker",
                None,
                100,
                None,
                false,
                false,
            )
            .await
            .unwrap();

        assert!(
            results.iter().any(|(n, _)| n.id.ends_with("8997ff6d398e417cae3c7ab866134cc3")),
            "el leg silva solo debe encontrar el marcador sin embedding denso (F1 cerró el dual-write híbrido); resultados={}",
            results.len()
        );
    }
}

/// T1277-D: the embedding-fingerprint gate, observation by default.
/// Corpus written by one engine, queried by another: the foreign-model row is
/// counted (and, flag OFF, still returned); the sentinel-hash row is never a
/// mismatch; the foreign-dimension row is dropped as it already was.
#[cfg(test)]
mod embedding_fingerprint_tests {
    use super::super::SilvaDB;

    async fn seeded() -> SilvaDB {
        let db = SilvaDB::in_memory().await.unwrap();
        for id in ["same_engine", "other_model", "sentinel_hash", "other_dim"] {
            db.upsert_node(id, "note", "embedding fingerprint gate", "{}").await.unwrap();
        }
        // 4-dim query is [1,0,0,0] => cosine 1.0, far above the 0.05 cut.
        db.save_embedding("same_engine", &[1.0, 0.0, 0.0, 0.0], "engA", Some("hash1")).await.unwrap();
        db.save_embedding("other_model", &[1.0, 0.0, 0.0, 0.0], "engB", Some("hash1")).await.unwrap();
        db.save_embedding("sentinel_hash", &[1.0, 0.0, 0.0, 0.0], "engA", Some("unknown-pre-hash")).await.unwrap();
        db.save_embedding("other_dim", &[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], "engA", None).await.unwrap();
        db
    }

    fn ids(results: &[(super::super::GraphNode, f32)]) -> Vec<String> {
        results.iter().map(|(n, _)| n.id.clone()).collect()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn observation_mode_counts_without_excluding() {
        let db = seeded().await;
        let q = [1.0f32, 0.0, 0.0, 0.0];

        // No expected fingerprint declared => gate fully inert.
        assert!(db.expected_embedding_snapshot().is_none());
        let _ = db.search_vector(&q, 10).await.unwrap();
        assert_eq!(db.fingerprint_observation(), (0, 0, 0), "no expected fingerprint => inert");

        db.set_expected_embedding("engA".to_string(), Some("hash1".to_string()));
        let got = ids(&db.search_vector(&q, 10).await.unwrap());

        assert!(got.contains(&"same_engine".to_string()), "own-engine row present: {got:?}");
        assert!(
            got.contains(&"other_model".to_string()),
            "observation mode must NOT exclude the foreign model: {got:?}"
        );
        assert!(
            got.contains(&"sentinel_hash".to_string()),
            "unknown-pre-hash sentinel is never a mismatch: {got:?}"
        );
        assert!(
            !got.contains(&"other_dim".to_string()),
            "foreign-dimension row keeps its pre-existing drop: {got:?}"
        );

        let (by_model, by_dim, excluded) = db.fingerprint_observation();
        assert_eq!(by_model, 1, "exactly one foreign-model candidate");
        assert_eq!(by_dim, 1, "exactly one foreign-dimension row");
        assert_eq!(excluded, 0, "flag OFF must never exclude");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn enabled_filter_excludes_foreign_model_only() {
        let db = seeded().await;
        db.set_expected_embedding("engA".to_string(), Some("hash1".to_string()));
        db.set_fingerprint_filter_enabled(true);
        assert!(db.fingerprint_filter_enabled());

        let got = ids(&db.search_vector(&[1.0f32, 0.0, 0.0, 0.0], 10).await.unwrap());
        assert!(
            !got.contains(&"other_model".to_string()),
            "flag ON must drop the foreign model: {got:?}"
        );
        assert!(got.contains(&"same_engine".to_string()));
        assert!(got.contains(&"sentinel_hash".to_string()));

        let (by_model, by_dim, excluded) = db.fingerprint_observation();
        assert_eq!(by_model, 1);
        assert_eq!(by_dim, 1);
        assert_eq!(excluded, 1, "the dropped candidate must be counted");
    }

    #[test]
    fn predicate_mirrors_get_stale_embeddings_semantics() {
        use super::embedding_row_matches_engine;
        use super::super::ExpectedEmbedding;

        let expected = ExpectedEmbedding {
            model_name: "engA".to_string(),
            model_hash: Some("hash1".to_string()),
        };
        assert!(embedding_row_matches_engine("engA", Some("hash1"), &expected));
        assert!(embedding_row_matches_engine("engA", None, &expected));
        assert!(embedding_row_matches_engine("engA", Some("unknown-pre-hash"), &expected));
        assert!(!embedding_row_matches_engine("engB", Some("hash1"), &expected));
        assert!(!embedding_row_matches_engine("engA", Some("hash2"), &expected));

        // No expected hash => nothing to compare, any known stored hash is fine.
        let no_hash = ExpectedEmbedding { model_name: "engA".to_string(), model_hash: None };
        assert!(embedding_row_matches_engine("engA", Some("whatever"), &no_hash));
        assert!(!embedding_row_matches_engine("engB", Some("whatever"), &no_hash));
    }
}

/// T1277-E: the same fingerprint gate on the index-backed candidate paths
/// (HNSW and IVF). Neither index stores engine metadata — HNSW keeps
/// `node_ids + points`, the IVF/mmap store keeps ids + quantized vectors — so
/// candidate provenance comes from ONE bounded `node_embeddings` lookup over
/// the candidate ids. Inert (no declared fingerprint) => no query at all.
#[cfg(test)]
mod embedding_fingerprint_index_tests {
    use super::super::hnsw::{EmbPoint, HnswIndex};
    use super::super::{GraphNode, SilvaDB};
    use crate::memory::ivf_index::IVFSearcher;
    use crate::memory::mmap_store::MmapEmbeddingStore;

    const IDS: [&str; 4] = ["same_engine", "other_model", "sentinel_hash", "other_dim"];

    /// 4-dim query `[1,0,0,0]`. `other_dim` carries an 8-float row in
    /// `node_embeddings` (foreign dimension) while the index vectors stay 4-dim.
    async fn seeded() -> SilvaDB {
        let db = SilvaDB::in_memory().await.unwrap();
        for id in IDS {
            db.upsert_node(id, "note", "index fingerprint gate", "{}").await.unwrap();
        }
        db.save_embedding("same_engine", &[1.0, 0.0, 0.0, 0.0], "engA", Some("hash1"))
            .await
            .unwrap();
        db.save_embedding("other_model", &[0.9, 0.1, 0.0, 0.0], "engB", Some("hash1"))
            .await
            .unwrap();
        db.save_embedding("sentinel_hash", &[0.95, 0.05, 0.0, 0.0], "engA", Some("unknown-pre-hash"))
            .await
            .unwrap();
        db.save_embedding("other_dim", &[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], "engA", None)
            .await
            .unwrap();
        db
    }

    fn ids(results: &[(GraphNode, f32)]) -> Vec<String> {
        results.iter().map(|(n, _)| n.id.clone()).collect()
    }

    fn index_vectors() -> Vec<Vec<f32>> {
        vec![
            vec![1.0, 0.0, 0.0, 0.0],
            vec![0.9, 0.1, 0.0, 0.0],
            vec![0.95, 0.05, 0.0, 0.0],
            vec![1.0, 0.0, 0.0, 0.0],
        ]
    }

    /// Real `HnswIndex` built directly: the 12_000 guard lives in
    /// `build_hnsw()`, not in the type, so a 4-point index is valid here.
    async fn install_hnsw(db: &SilvaDB) {
        let node_ids: Vec<String> = IDS.iter().map(|s| s.to_string()).collect();
        let points: Vec<EmbPoint> =
            index_vectors().iter().map(|v| EmbPoint(v.clone())).collect();
        let map = instant_distance::Builder::default().build(points.clone(), node_ids.clone());
        *db.hnsw.write().await = Some(HnswIndex { map, node_ids, points });
    }

    /// Installs a real IVF store + searcher over a temp file. Returns the path
    /// so the test can clean it up (Windows rejects deleting a mapped file).
    fn install_ivf(db: &SilvaDB) -> std::path::PathBuf {
        let node_ids: Vec<String> = IDS.iter().map(|s| s.to_string()).collect();
        let centroids = vec![vec![1.0f32, 0.0, 0.0, 0.0]];
        let assignments = vec![0u32, 0, 0, 0];
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("t1277e_ivf_{}_{nonce}.fjv1", std::process::id()));
        let store =
            MmapEmbeddingStore::create(&path, &node_ids, &index_vectors(), 4, 1, &centroids, &assignments)
                .unwrap();
        *db.mmap_store.write().unwrap_or_else(|p| p.into_inner()) = Some(store);
        *db.ivf_searcher.write().unwrap_or_else(|p| p.into_inner()) =
            Some(IVFSearcher::new(centroids, &assignments, 1));
        path
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn hnsw_observation_counts_without_changing_results() {
        let db = seeded().await;
        install_hnsw(&db).await;
        let q = [1.0f32, 0.0, 0.0, 0.0];

        // No expected fingerprint => gate fully inert on the HNSW path too.
        let before = ids(&db.search_vector(&q, 10).await.unwrap());
        assert_eq!(before.len(), 4, "all four index candidates: {before:?}");
        assert_eq!(db.fingerprint_observation(), (0, 0, 0), "inert => no counters");

        db.set_expected_embedding("engA".to_string(), Some("hash1".to_string()));
        let after = ids(&db.search_vector(&q, 10).await.unwrap());
        assert_eq!(
            before.len(),
            after.len(),
            "observation mode must not change HNSW results: before={before:?} after={after:?}"
        );
        assert!(
            after.contains(&"other_model".to_string()),
            "observer counts the foreign model but still returns it: {after:?}"
        );
        assert_eq!(db.fingerprint_observation(), (1, 1, 0), "(model, dim, excluded)");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn hnsw_enabled_filter_drops_foreign_candidates() {
        let db = seeded().await;
        install_hnsw(&db).await;
        db.set_expected_embedding("engA".to_string(), Some("hash1".to_string()));
        db.set_fingerprint_filter_enabled(true);

        let got = ids(&db.search_vector(&[1.0f32, 0.0, 0.0, 0.0], 10).await.unwrap());
        assert!(got.contains(&"same_engine".to_string()));
        assert!(got.contains(&"sentinel_hash".to_string()), "sentinel never a mismatch: {got:?}");
        assert!(!got.contains(&"other_model".to_string()), "flag ON drops foreign model: {got:?}");
        assert!(!got.contains(&"other_dim".to_string()), "flag ON drops foreign dim: {got:?}");
        assert_eq!(db.fingerprint_observation(), (1, 1, 2));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ivf_observation_counts_without_changing_results() {
        let db = seeded().await;
        let path = install_ivf(&db);
        let q = [1.0f32, 0.0, 0.0, 0.0];

        let before = ids(&db.search_vector(&q, 10).await.unwrap());
        assert_eq!(before.len(), 4, "all four IVF candidates: {before:?}");
        assert_eq!(db.fingerprint_observation(), (0, 0, 0), "inert => no counters");

        db.set_expected_embedding("engA".to_string(), Some("hash1".to_string()));
        let after = ids(&db.search_vector(&q, 10).await.unwrap());
        assert_eq!(
            before, after,
            "observation mode must not change IVF results (order included)"
        );
        assert_eq!(db.fingerprint_observation(), (1, 1, 0), "(model, dim, excluded)");

        drop(db);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ivf_enabled_filter_drops_foreign_candidates() {
        let db = seeded().await;
        let path = install_ivf(&db);
        db.set_expected_embedding("engA".to_string(), Some("hash1".to_string()));
        db.set_fingerprint_filter_enabled(true);

        let got = ids(&db.search_vector(&[1.0f32, 0.0, 0.0, 0.0], 10).await.unwrap());
        assert!(got.contains(&"same_engine".to_string()));
        assert!(got.contains(&"sentinel_hash".to_string()));
        assert!(!got.contains(&"other_model".to_string()), "flag ON drops foreign model: {got:?}");
        assert!(!got.contains(&"other_dim".to_string()), "flag ON drops foreign dim: {got:?}");
        assert_eq!(db.fingerprint_observation(), (1, 1, 2));

        drop(db);
        let _ = std::fs::remove_file(&path);
    }

    /// Cost probe for the added bounded lookup. Run with:
    ///   cargo test -p tylluan-kernel --lib fingerprint_gate_cost -- --nocapture
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn fingerprint_gate_cost() {
        const ROWS: usize = 1000;
        const CANDIDATES: usize = 300; // HNSW window at limit=100 (limit * 3)
        const ITERS: usize = 100;

        let db = SilvaDB::in_memory().await.unwrap();
        for i in 0..ROWS {
            let id = format!("row_{i:05}");
            db.upsert_node(&id, "note", "cost probe", "{}").await.unwrap();
            db.save_embedding(&id, &[1.0, 0.0, 0.0, 0.0], "engA", Some("hash1"))
                .await
                .unwrap();
        }
        db.set_expected_embedding("engA".to_string(), Some("hash1".to_string()));
        let candidate_ids: Vec<String> = (0..CANDIDATES).map(|i| format!("row_{i:05}")).collect();

        let mut samples = Vec::with_capacity(ITERS);
        tokio::task::block_in_place(|| {
            let conn = db.conn_timed();
            for _ in 0..ITERS {
                let t0 = std::time::Instant::now();
                let dropped = db.fingerprint_dropped_ids(&conn, &candidate_ids, 16);
                samples.push(t0.elapsed().as_micros() as u64);
                assert!(dropped.is_empty(), "all candidates share the declared engine");
            }
        });
        samples.sort_unstable();
        let p50 = samples[ITERS / 2];
        let p95 = samples[ITERS * 95 / 100];

        // Reference point: the pre-existing per-candidate resolution the gate
        // sits next to in the HNSW/IVF loops.
        let mut baseline = Vec::with_capacity(ITERS);
        tokio::task::block_in_place(|| {
            let conn = db.conn_timed();
            for _ in 0..ITERS {
                let t0 = std::time::Instant::now();
                for id in &candidate_ids {
                    let _ = db.get_node_sync(id, &conn);
                }
                baseline.push(t0.elapsed().as_micros() as u64);
            }
        });
        baseline.sort_unstable();
        let base_p50 = baseline[ITERS / 2];

        println!(
            "T1277-E fingerprint_gate_cost: rows={ROWS} candidates={CANDIDATES} iters={ITERS} \
             gate_p50={p50}us gate_p95={p95}us (min={}us max={}us) — \
             reference get_node_sync loop p50={base_p50}us",
            samples[0],
            samples[ITERS - 1]
        );
        assert!(p95 < 50_000, "bounded candidate lookup should stay sub-50ms (got {p95}us)");
    }
}

/// T1277-F (cola D-1): escala del filtro de huella por encima de HNSW_THRESHOLD.
///
/// Corpus sintético de 12.500 nodos en una base en memoria (ningún dato de
/// producción): 20 % con huella de motor B, 5 % con dimensión distinta (16 en
/// vez de 12), disjuntos por construcción (`i % 5 == 0` y `i % 20 == 2` no
/// pueden solaparse). Construye el HNSW REAL por la ruta de producción
/// (`rebuild_hnsw_if_needed`, que exige >= 12.000 filas) y comprueba tres cosas
/// con el flag en false y en true:
///   1. ranking con flag=false IDÉNTICO al de antes de la puerta (sin huella
///      declarada), top-20 y en el mismo orden, en 50 consultas;
///   2. contadores observados == composición EXACTA de la ventana de candidatos
///      (el top-`limit*3` que devuelve el propio índice), consulta a consulta;
///   3. con flag=true, `fp_excluded` == (modelo ajeno + dimensión ajena) de esa
///      misma consulta, y el resultado es la ventana sin los ajenos, truncada.
///
/// Además mide latencia p50/p95 de `search_vector` con la puerta inerte y activa.
///
/// Es `#[ignore]`: tarda minutos y no debe estar en la suite por defecto.
/// Reproducir con:
///   cargo test -p tylluan-kernel --lib t1277f_scale -- --ignored --nocapture
#[cfg(test)]
mod embedding_fingerprint_scale_tests {
    use super::super::SilvaDB;

    const N: usize = 12_500;
    const DIM: usize = 12;
    const DIM_FOREIGN: usize = 16;
    const QUERIES: usize = 50;
    const TOP: usize = 20;

    fn node_index(id: &str) -> usize {
        id[1..].parse().unwrap_or(usize::MAX)
    }
    /// 20 % del corpus: `i % 5 == 0`.
    fn is_engine_b(id: &str) -> bool {
        node_index(id).is_multiple_of(5)
    }
    /// 5 % del corpus: `i % 20 == 2` (nunca divisible por 5, así que disjunto).
    fn is_foreign_dim(id: &str) -> bool {
        node_index(id) % 20 == 2
    }

    fn vec_of(i: usize, dim: usize) -> Vec<f32> {
        (0..dim)
            .map(|j| (((i * 31 + j * 7) % 101) as f32) / 101.0 + 0.01)
            .collect()
    }

    /// Vector de un nodo con dimensión ajena: las mismas `DIM` componentes que
    /// su variante válida, rellenadas con ceros hasta `DIM_FOREIGN`. El relleno
    /// en ceros no altera ni la norma ni el producto escalar, así que el nodo
    /// compite en igualdad dentro del HNSW. Sin ese relleno la métrica coseno
    /// (`hnsw.rs`: producto sobre el mínimo de longitudes, norma sobre la
    /// longitud completa) penaliza sistemáticamente a los nodos de 16 dims y
    /// jamás aparecerían como candidatos — que es exactamente lo que hay que
    /// poder medir aquí.
    fn vec_of_foreign(i: usize) -> Vec<f32> {
        let mut v = vec_of(i, DIM);
        v.resize(DIM_FOREIGN, 0.0);
        v
    }

    fn ids(results: &[(super::super::GraphNode, f32)]) -> Vec<String> {
        results.iter().map(|(n, _)| n.id.clone()).collect()
    }

    fn pct(sorted: &[u64], p: usize) -> u64 {
        sorted[(sorted.len() * p / 100).min(sorted.len() - 1)]
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "T1277-F: corpus de 12.5k nodos con HNSW real; minutos. Correr con --ignored"]
    async fn t1277f_scale_ranking_and_counters() {
        let db = SilvaDB::in_memory().await.unwrap();

        // ── Corpus ────────────────────────────────────────────────────────
        let t_corpus = std::time::Instant::now();
        for i in 0..N {
            let id = format!("n{i:05}");
            db.upsert_node(&id, "note", "escala del filtro de huella", "{}")
                .await
                .unwrap();
            if is_engine_b(&id) {
                db.save_embedding(&id, &vec_of(i, DIM), "engB", Some("hashB"))
                    .await
                    .unwrap();
            } else if is_foreign_dim(&id) {
                db.save_embedding(&id, &vec_of_foreign(i), "engA", Some("hashA"))
                    .await
                    .unwrap();
            } else {
                db.save_embedding(&id, &vec_of(i, DIM), "engA", Some("hashA"))
                    .await
                    .unwrap();
            }
        }
        let corpus_secs = t_corpus.elapsed().as_secs_f32();

        // ── HNSW real, por la ruta de producción ──────────────────────────
        let t_hnsw = std::time::Instant::now();
        db.rebuild_hnsw_if_needed().await.unwrap();
        let hnsw_secs = t_hnsw.elapsed().as_secs_f32();
        assert!(
            db.hnsw.read().await.is_some(),
            "con {N} filas (> HNSW_THRESHOLD) rebuild_hnsw_if_needed debe construir el índice"
        );

        // ── Fase 1: sin huella declarada (comportamiento pre-T1277-D) ──────
        assert!(db.expected_embedding_snapshot().is_none());
        let mut base_ids: Vec<Vec<String>> = Vec::with_capacity(QUERIES);
        let mut base_us: Vec<u64> = Vec::with_capacity(QUERIES);
        for q in 0..QUERIES {
            let query = vec_of(q, DIM);
            let t0 = std::time::Instant::now();
            let res = db.search_vector(&query, TOP).await.unwrap();
            base_us.push(t0.elapsed().as_micros() as u64);
            let got = ids(&res);
            assert!(!got.is_empty(), "query {q}: el HNSW debe devolver candidatos");
            base_ids.push(got);
        }
        assert_eq!(
            db.fingerprint_observation(),
            (0, 0, 0),
            "sin huella declarada la puerta es inerte"
        );

        // ── Fase 2: huella declarada, flag false (observación) ───────────
        db.set_expected_embedding("engA".to_string(), Some("hashA".to_string()));
        let mut gate_us: Vec<u64> = Vec::with_capacity(QUERIES);
        let mut identical = 0usize;
        let mut counters_exact = 0usize;
        let mut excluded_exact = 0usize;
        let (mut total_model, mut total_dim, mut total_window) = (0u64, 0u64, 0u64);

        for (q, base) in base_ids.iter().enumerate() {
            let query = vec_of(q, DIM);

            // Ventana real de candidatos: el mismo top-`TOP*3` que usa la ruta
            // HNSW (llamada determinista al mismo índice).
            let win: Vec<String> = {
                let guard = db.hnsw.read().await;
                let state = guard.as_ref().expect("HNSW construido arriba");
                crate::memory::silva::hnsw::search_hnsw(state, &query, TOP * 3)
                    .into_iter()
                    .map(|(id, _)| id)
                    .collect()
            };
            assert!(win.len() >= TOP, "ventana demasiado corta para juzgar: {}", win.len());
            let exp_model = win.iter().filter(|id| is_engine_b(id)).count() as u64;
            let exp_dim = win.iter().filter(|id| is_foreign_dim(id)).count() as u64;

            let before = db.fingerprint_observation();
            let t0 = std::time::Instant::now();
            let res = db.search_vector(&query, TOP).await.unwrap();
            gate_us.push(t0.elapsed().as_micros() as u64);
            let after = db.fingerprint_observation();
            let got = ids(&res);

            // (1) ranking idéntico al de antes de la puerta, mismo orden.
            if got == *base {
                identical += 1;
            }
            // (2) contadores == composición exacta de la ventana.
            if after.0 - before.0 == exp_model && after.1 - before.1 == exp_dim {
                counters_exact += 1;
            }
            total_model += after.0 - before.0;
            total_dim += after.1 - before.1;
            total_window += win.len() as u64;
            assert_eq!(after.2, before.2, "flag false no debe excluir nada");

            // (3) flag true: excluye exactamente los ajenos y devuelve la
            // ventana sin ellos truncada a TOP.
            db.set_fingerprint_filter_enabled(true);
            let before_t = db.fingerprint_observation();
            let filtered = ids(&db.search_vector(&query, TOP).await.unwrap());
            let after_t = db.fingerprint_observation();
            db.set_fingerprint_filter_enabled(false);

            let expected_filtered: Vec<String> = win
                .iter()
                .filter(|id| !is_engine_b(id) && !is_foreign_dim(id))
                .take(TOP)
                .cloned()
                .collect();
            let dropped_foreign = (exp_model + exp_dim) as u64;
            if after_t.2 - before_t.2 == dropped_foreign && filtered == expected_filtered {
                excluded_exact += 1;
            }
        }

        base_us.sort_unstable();
        gate_us.sort_unstable();
        let (b50, b95) = (pct(&base_us, 50), pct(&base_us, 95));
        let (g50, g95) = (pct(&gate_us, 50), pct(&gate_us, 95));

        let mix_b = (0..N).filter(|i| i % 5 == 0).count();
        let mix_dim = (0..N).filter(|i| i % 20 == 2).count();
        println!(
            "T1277-F escala: nodes={N} dim={DIM} (ajenos {DIM_FOREIGN}) window={} queries={QUERIES} top={TOP}\n  \
             corpus: motor B={mix_b} ({:.1}%) · dim ajena={mix_dim} ({:.1}%) · build {corpus_secs:.1}s + HNSW {hnsw_secs:.1}s\n  \
             ranking flag=false idéntico a pre-puerta: {identical}/{QUERIES}\n  \
             contadores == ventana exacta: {counters_exact}/{QUERIES} (modelo {total_model} esperado, dim {total_dim} — sobre {total_window} candidatos juzgados)\n  \
             flag=true excluded == modelo+dim y resultado = ventana filtrada: {excluded_exact}/{QUERIES}\n  \
             latencia search_vector p50/p95: inerte {b50}us/{b95}us · puerta activa {g50}us/{g95}us (delta p50 {}us)",
            TOP * 3,
            100.0 * mix_b as f32 / N as f32,
            100.0 * mix_dim as f32 / N as f32,
            g50 as i64 - b50 as i64
        );

        assert_eq!(identical, QUERIES, "flag=false no debe alterar el ranking ni el orden");
        assert_eq!(
            counters_exact, QUERIES,
            "los contadores deben clavar la composición de la ventana, consulta a consulta"
        );
        assert_eq!(
            excluded_exact, QUERIES,
            "con flag=true se excluye exactamente lo ajeno: (modelo + dimensión)"
        );
    }
}
