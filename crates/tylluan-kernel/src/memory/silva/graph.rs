use anyhow::Result;
use rusqlite::params;
use std::collections::HashMap;
use tracing::{info, warn};

use super::{GraphNode, ThinkAnalysis};

impl super::SilvaDB {
    /// Find clusters of connected nodes (communities) using the Semantic Louvain algorithm.
    /// This combines explicit edges with semantic proximity from embeddings.
    pub async fn find_clusters(&self, min_size: usize) -> Result<Vec<Vec<String>>> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();

            // 1. Fetch only relevant nodes with embeddings (Sovereign Relevance Window)
            let mut stmt = conn.prepare(
                "SELECT n.id, e.embedding 
                 FROM nodes n
                 JOIN node_embeddings e ON n.id = e.node_id
                 ORDER BY n.weight DESC, n.updated_at DESC
                 LIMIT 500"
            )?;

            let mut embeddings = HashMap::new();
            let mut active_node_ids = Vec::new();

            let emb_rows = stmt.query_map([], |row| {
                let id: String = row.get(0)?;
                let data: Vec<u8> = row.get(1)?;
                let f32_vec: Vec<f32> = data.chunks_exact(4)
                    .map(|c| f32::from_le_bytes(c.try_into().expect("chunk should be exactly 4 bytes")))
                    .collect();
                Ok((id, f32_vec))
            })?;

            for row in emb_rows {
                let (id, vec) = row?;
                active_node_ids.push(id.clone());
                embeddings.insert(id, vec);
            }

            if active_node_ids.is_empty() { return Ok(Vec::new()); }

            // 2. Build Weighted Graph with ALL existing node IDs
            let mut stmt_all = conn.prepare("SELECT id FROM nodes")?;
            let all_node_ids: Vec<String> = stmt_all.query_map([], |row| row.get(0))?
                .collect::<Result<Vec<String>, _>>()?;
            let mut graph = crate::memory::louvain::WeightedGraph::new(all_node_ids);

            // 3. Add Explicit Edges (from SilvaDB edges table)
            let mut stmt_edges = conn.prepare("SELECT source, target, weight FROM edges")?;
            let edges = stmt_edges.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, f64>(2)?))
            })?;
            for edge_res in edges {
                let (s, t, w) = edge_res?;
                graph.add_edge(&s, &t, w);
            }

            // 4. Add Semantic Edges (Similarity Enrichment)
            for i in 0..active_node_ids.len() {
                for j in i + 1..active_node_ids.len() {
                    let id_a = &active_node_ids[i];
                    let id_b = &active_node_ids[j];

                    let vec_a = &embeddings[id_a];
                    let vec_b = &embeddings[id_b];

                    let sim = crate::memory::cosine::cosine_similarity(vec_a, vec_b);
                    if sim > 0.85 {
                        graph.add_edge(id_a, id_b, sim as f64);
                    }
                }
            }

            // 5. Run Louvain Discovery
            let partition = graph.find_communities();

            // 6. Persist results in node_communities
            conn.execute("DELETE FROM node_communities", [])?;
            let mut stmt = conn.prepare("INSERT INTO node_communities (node_id, cluster_id) VALUES (?, ?)")?;
            for (id, cluster_id) in &partition {
                stmt.execute(params![id, *cluster_id as i64])?;
            }

            // 7. Update nodes.cluster_id for IVF index
            for (node_id, &comm) in &partition {
                conn.execute(
                    "UPDATE nodes SET cluster_id = ?1 WHERE id = ?2",
                    params![comm.to_string(), node_id],
                ).map_err(|e| warn!("🌲 Failed to set cluster_id for '{}': {}", node_id, e)).ok();
            }

            // 8. Compute and store cluster centroids (IVF Materialization)
            let mut cluster_sums: HashMap<usize, Vec<f32>> = HashMap::new();
            let mut cluster_counts: HashMap<usize, usize> = HashMap::new();

            for (node_id, &comm) in &partition {
                if let Some(vec) = embeddings.get(node_id) {
                    let sum = cluster_sums.entry(comm).or_insert_with(|| vec![0.0; vec.len()]);
                    for (i, val) in vec.iter().enumerate() {
                        sum[i] += val;
                    }
                    *cluster_counts.entry(comm).or_insert(0) += 1;
                }
            }

            let mut stmt_centroid = conn.prepare(
                "INSERT OR REPLACE INTO cluster_centroids (cluster_id, centroid_vector, model_id) VALUES (?, ?, ?)"
            )?;
            let model_id = "default";

            for (comm, sum) in cluster_sums.iter() {
                let count = *cluster_counts.get(comm).unwrap_or(&1) as f32;
                let centroid: Vec<u8> = sum.iter()
                    .flat_map(|v| (v / count).to_le_bytes())
                    .collect();
                stmt_centroid.execute(params![comm.to_string(), centroid, model_id])
                    .map_err(|e| warn!("🌲 Failed to store centroid for cluster '{}': {}", comm, e)).ok();
            }

            info!("🔬 IVF Index materialized: {} clusters, {} centroids stored", 
                  cluster_sums.len(), cluster_sums.len());

            // 9. Group by cluster ID and filter by min_size
            let mut groups: HashMap<usize, Vec<String>> = HashMap::new();
            for (id, &comm) in &partition {
                groups.entry(comm).or_default().push(id.clone());
            }

            let results: Vec<Vec<String>> = groups.into_values()
                .filter(|v| v.len() >= min_size)
                .collect();

            Ok(results)
        })
    }

    /// BLOQUE C: Deep graph analysis (Communities, Hubs, Bridges)
    pub async fn analyze_graph_deep(&self) -> Result<serde_json::Value> {
        let nodes = self.get_all_nodes().await?;
        let edges_json = self.get_all_edges().await?;

        let node_ids: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();
        let edges: Vec<(String, String)> = edges_json.iter().map(|e| {
            (e["source"].as_str().unwrap_or("").to_string(), 
             e["target"].as_str().unwrap_or("").to_string())
        }).collect();

        // 1. Communities (Louvain)
        let mut graph = crate::memory::louvain::WeightedGraph::new(node_ids.clone());
        for (s, t) in &edges {
            graph.add_edge(s, t, 1.0);
        }
        let communities = graph.find_communities();

        // 2. Hubs (Local PageRank)
        let pr = self.calculate_pagerank_internal(&node_ids, &edges, 15, 0.85);
        let mut hub_list: Vec<_> = pr.into_iter().collect();
        hub_list.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let hubs: Vec<_> = hub_list.into_iter().take(10).map(|(id, score)| {
            serde_json::json!({ "id": id, "score": score })
        }).collect();

        Ok(serde_json::json!({
            "communities": communities,
            "hubs": hubs,
            "stats": {
                "nodes": nodes.len(),
                "edges": edges.len()
            }
        }))
    }

    /// TAREA 1 — Implementar silva.rs:personalized_pagerank_local() (R11-4)
    pub async fn personalized_pagerank_local(
        &self,
        seed_ids: &[String],
        alpha: f64,
        iterations: u32,
        top_k: usize,
    ) -> Result<Vec<(String, f64)>> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();

            // 1. BFS to load local subgraph up to 2 hops
            let mut visited = std::collections::HashSet::new();
            for seed in seed_ids {
                visited.insert(seed.clone());
                if visited.len() >= 200 {
                    break;
                }
            }
            let mut current_frontier = seed_ids.to_vec();

            for _depth in 0..2 {
                if current_frontier.is_empty() || visited.len() >= 200 {
                    break;
                }

                let mut next_frontier = Vec::new();
                for chunk in current_frontier.chunks(50) {
                    let placeholders = std::iter::repeat_n("?", chunk.len())
                        .collect::<Vec<_>>()
                        .join(",");
                    let edges_query = format!(
                        "SELECT source, target FROM edges WHERE source IN ({}) OR target IN ({})",
                        placeholders, placeholders
                    );
                    let mut stmt = conn.prepare(&edges_query)?;
                    let mut rows = stmt.query(rusqlite::params_from_iter(chunk.iter().chain(chunk.iter())))?;
                    while let Some(row) = rows.next()? {
                        let src: String = row.get(0)?;
                        let tgt: String = row.get(1)?;

                        for node in [&src, &tgt] {
                            if !visited.contains(node)
                                && visited.len() < 200 {
                                    visited.insert(node.clone());
                                    next_frontier.push(node.clone());
                                }
                        }
                    }
                }
                current_frontier = next_frontier;
            }

            if visited.is_empty() {
                return Ok(Vec::new());
            }

            let nodes_vec: Vec<String> = visited.into_iter().collect();
            let nodes_set: std::collections::HashSet<String> = nodes_vec.iter().cloned().collect();

            // 2. Fetch edges between nodes in the subgraph
            let mut edges = Vec::new();
            for chunk in nodes_vec.chunks(50) {
                let placeholders = std::iter::repeat_n("?", chunk.len())
                    .collect::<Vec<_>>()
                    .join(",");
                let query = format!("SELECT source, target FROM edges WHERE source IN ({})", placeholders);
                let mut stmt = conn.prepare(&query)?;
                let mut rows = stmt.query(rusqlite::params_from_iter(chunk))?;
                while let Some(row) = rows.next()? {
                    let src: String = row.get(0)?;
                    let tgt: String = row.get(1)?;
                    if nodes_set.contains(&tgt) {
                        edges.push((src, tgt));
                    }
                }
            }

            // Find active seeds present in subgraph
            let seeds_in_subgraph: Vec<String> = seed_ids
                .iter()
                .filter(|id| nodes_set.contains(*id))
                .cloned()
                .collect();

            let seeds = if seeds_in_subgraph.is_empty() {
                nodes_vec.clone()
            } else {
                seeds_in_subgraph
            };

            // 3. Initialize scores
            let mut scores: HashMap<String, f64> = nodes_vec.iter().map(|id| (id.clone(), 0.0)).collect();
            let initial_score = 1.0 / seeds.len() as f64;
            for seed in &seeds {
                if let Some(score) = scores.get_mut(seed) {
                    *score = initial_score;
                }
            }

            // 4. Calculate out-degrees and incoming neighbors
            let mut out_degree: HashMap<String, usize> = HashMap::new();
            let mut incoming_neighbors: HashMap<String, Vec<String>> = HashMap::new();
            for id in &nodes_vec {
                out_degree.insert(id.clone(), 0);
                incoming_neighbors.insert(id.clone(), Vec::new());
            }
            for (s, t) in &edges {
                *out_degree.entry(s.clone()).or_insert(0) += 1;
                incoming_neighbors.entry(t.clone()).or_default().push(s.clone());
            }

            // 5. Power iterations
            for _ in 0..iterations {
                let mut next_scores = HashMap::new();
                let mut dangling_sum = 0.0;
                for id in &nodes_vec {
                    if *out_degree.get(id).unwrap_or(&0) == 0 {
                        dangling_sum += scores[id];
                    }
                }

                for v in &nodes_vec {
                    let mut incoming_sum = 0.0;
                    if let Some(neighbors) = incoming_neighbors.get(v) {
                        for u in neighbors {
                            let deg = out_degree[u];
                            if deg > 0 {
                                incoming_sum += scores[u] / deg as f64;
                            }
                        }
                    }

                    let is_seed = seeds.contains(v);
                    let base = if is_seed {
                        (1.0 - alpha) / seeds.len() as f64
                    } else {
                        0.0
                    };

                    let dangling_share = if is_seed {
                        alpha * dangling_sum / seeds.len() as f64
                    } else {
                        0.0
                    };

                    let score_v = base + dangling_share + alpha * incoming_sum;
                    next_scores.insert(v.clone(), score_v);
                }
                scores = next_scores;
            }

            // 6. Exclude seeds and return sorted desc top_k
            let mut results: Vec<(String, f64)> = scores
                .into_iter()
                .filter(|(id, _)| !seed_ids.contains(id))
                .collect();

            results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            results.truncate(top_k);

            Ok(results)
        })
    }

    fn calculate_pagerank_internal(
        &self,
        node_ids: &[String],
        edges: &[(String, String)],
        iterations: usize,
        damping: f64,
    ) -> HashMap<String, f64> {
        let n = node_ids.len();
        if n == 0 { return HashMap::new(); }

        let mut pr: HashMap<String, f64> = node_ids.iter().map(|id| (id.clone(), 1.0 / n as f64)).collect();
        let mut out_degree: HashMap<String, usize> = HashMap::new();
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();

        for (s, t) in edges {
            if node_ids.contains(s) && node_ids.contains(t) {
                adj.entry(s.clone()).or_default().push(t.clone());
                *out_degree.entry(s.clone()).or_insert(0) += 1;
            }
        }

        for _ in 0..iterations {
            let mut next_pr: HashMap<String, f64> = node_ids.iter().map(|id| (id.clone(), (1.0 - damping) / n as f64)).collect();
            let mut dangling_sum = 0.0;

            for id in node_ids {
                if *out_degree.get(id).unwrap_or(&0) == 0 {
                    dangling_sum += pr[id];
                } else {
                    if let Some(neighbors) = adj.get(id) {
                        for neighbor in neighbors {
                            if let Some(score) = next_pr.get_mut(neighbor) {
                                *score += damping * pr[id] / out_degree[id] as f64;
                            }
                        }
                    }
                }
            }

            for id in node_ids {
                if let Some(score) = next_pr.get_mut(id) {
                    *score += damping * dangling_sum / n as f64;
                }
            }

            pr = next_pr;
        }

        pr
    }

    /// Generate a summary for a cluster of nodes.
    pub async fn generate_cluster_summary(&self, node_ids: &[String]) -> Result<String> {
        if node_ids.is_empty() {
            return Ok("No nodes provided.".into());
        }

        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let placeholders: Vec<String> = node_ids.iter().map(|_| "?".to_string()).collect();
            let sql = format!(
                "SELECT type, content FROM nodes WHERE id IN ({})",
                placeholders.join(",")
            );

            let mut stmt = conn.prepare(&sql)?;
            let refs: Vec<&dyn rusqlite::types::ToSql> = node_ids.iter().map(|n| n as &dyn rusqlite::types::ToSql).collect();
            let rows = stmt.query_map(refs.as_slice(), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;

            let mut by_type: HashMap<String, Vec<String>> = HashMap::new();
            for row in rows.flatten() {
                by_type.entry(row.0).or_default().push(row.1);
            }

            let mut summary = String::new();
            summary.push_str("## Cluster Summary\n\n");

            for (node_type, contents) in by_type.iter() {
                summary.push_str(&format!("### {} ({} items)\n", node_type, contents.len()));
                for content in contents.iter().take(3) {
                    let preview = if content.len() > 80 {
                        format!("{}...", content.chars().take(80).collect::<String>())
                    } else {
                        content.clone()
                    };
                    summary.push_str(&format!("- {}\n", preview));
                }
                if contents.len() > 3 {
                    summary.push_str(&format!("  ... and {} more\n", contents.len() - 3));
                }
                summary.push('\n');
            }

            let edge_placeholders: Vec<String> = node_ids.iter().map(|_| "?".to_string()).collect();
            let edge_sql = format!(
                "SELECT type, COUNT(*) as cnt FROM edges WHERE source IN ({}) OR target IN ({}) GROUP BY type",
                edge_placeholders.join(","), edge_placeholders.join(",")
            );

            if let Ok(mut edge_stmt) = conn.prepare(&edge_sql) {
                let edge_refs: Vec<&dyn rusqlite::types::ToSql> = node_ids.iter()
                    .chain(node_ids.iter())
                    .map(|n| n as &dyn rusqlite::types::ToSql)
                    .collect();
                if let Ok(edge_rows) = edge_stmt.query_map(edge_refs.as_slice(), |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                }) {
                    summary.push_str("### Relationships\n");
                    for row in edge_rows.flatten() {
                        summary.push_str(&format!("- {}: {} connections\n", row.0, row.1));
                    }
                }
            }
            Ok(summary)
        })
    }

    /// IVF Autobuild: builds cluster centroids from node_embeddings when:
    ///   - embedding count > IVF_MIN_VECTORS (50)
    ///   - cluster_centroids table is empty
    /// Returns IvfBuildResult with skipped=true if conditions are not met.
    pub async fn consolidate_ivf_index(&self) -> anyhow::Result<IvfBuildResult> {
        use crate::memory::ivf_index::{kmeans_plus_plus, IVFOptions};

        let start = std::time::Instant::now();
        const IVF_MIN_VECTORS: usize = 50;

        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();

            // 1. Check if centroids already exist AND .fjv1 file already exists → skip
            let centroid_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM cluster_centroids",
                [],
                |r| r.get(0),
            ).unwrap_or(0);

            let fjv1_exists = self.mmap_path.as_ref().map(|p| p.exists()).unwrap_or(false);

            if centroid_count > 0 && fjv1_exists {
                let current_embeddings: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM node_embeddings",
                    [],
                    |r| r.get(0),
                ).unwrap_or(0);
                let last_build_count: i64 = conn.query_row(
                    "SELECT CAST(value AS INTEGER) FROM silva_kv WHERE key = 'ivf_last_build_count'",
                    [],
                    |r| r.get(0),
                ).unwrap_or(0);

                let is_stale = last_build_count == 0
                    || current_embeddings > last_build_count + (last_build_count / 10);

                if !is_stale {
                    return Ok(IvfBuildResult {
                        n_centroids: centroid_count as usize,
                        elapsed_ms: start.elapsed().as_millis() as u64,
                        skipped: true,
                    });
                }

                info!("🔬 IVF stale: {} embeddings vs {} at last build — forcing rebuild",
                      current_embeddings, last_build_count);
                let _ = conn.execute("DELETE FROM cluster_centroids", []);
                if let Some(ref p) = self.mmap_path {
                    let _ = std::fs::remove_file(p);
                }
            }

            if fjv1_exists {
                info!("🔬 IVF: .fjv1 missing but centroids in SQLite — regenerating mmap store");
            }

            // 2. Load all embeddings
            let mut stmt = conn.prepare(
                "SELECT node_id, embedding FROM node_embeddings LIMIT 100000"
            )?;
            let rows = stmt.query_map([], |row| {
                let id: String = row.get(0)?;
                let blob: Vec<u8> = row.get(1)?;
                Ok((id, blob))
            })?;

            let mut node_ids: Vec<String> = Vec::new();
            let mut vectors: Vec<Vec<f32>> = Vec::new();
            for row in rows.flatten() {
                let (id, blob) = row;
                if blob.len() < 4 { continue; }
                let v: Vec<f32> = blob.chunks_exact(4)
                    .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect();
                node_ids.push(id);
                vectors.push(v);
            }

            if vectors.len() < IVF_MIN_VECTORS {
                return Ok(IvfBuildResult {
                    n_centroids: 0,
                    elapsed_ms: start.elapsed().as_millis() as u64,
                    skipped: true,
                });
            }

            // 3. Run K-Means++ to compute centroids
            let opts = IVFOptions::default();
            // nlist = sqrt(n_vectors), capped at default nlist and at n_vectors
            let nlist = ((vectors.len() as f64).sqrt() as u32)
                .min(opts.nlist)
                .min(vectors.len() as u32)
                .max(1);
            let (centroids, assignments) = kmeans_plus_plus(&vectors, nlist, 10);

            let n_centroids = centroids.len();

            // 4. Persist centroids
            let mut insert_stmt = conn.prepare(
                "INSERT OR REPLACE INTO cluster_centroids (cluster_id, centroid_vector, model_id) VALUES (?1, ?2, ?3)"
            )?;
            for (i, centroid) in centroids.iter().enumerate() {
                let blob: Vec<u8> = centroid.iter()
                    .flat_map(|v| v.to_le_bytes())
                    .collect();
                insert_stmt.execute(params![
                    format!("ivf_auto_{}", i),
                    blob,
                    "bge-m3"
                ])?;
            }

            let elapsed_ms = start.elapsed().as_millis() as u64;
            info!("🔬 IVF Autobuild complete: {} centroids from {} vectors in {}ms",
                  n_centroids, vectors.len(), elapsed_ms);

            let _ = conn.execute(
                "INSERT OR REPLACE INTO silva_kv (key, value) VALUES ('ivf_last_build_count', ?1)",
                params![vectors.len().to_string()],
            );

            // 5. Create .fjv1 mmap file and populate in-memory store + IVF searcher
            if let Some(ref mmap_path) = self.mmap_path {
                let dim = vectors[0].len();
                match crate::memory::mmap_store::MmapEmbeddingStore::create(
                    mmap_path,
                    &node_ids,
                    &vectors,
                    dim,
                    nlist as u32,
                    &centroids,
                    &assignments,
                ) {
                    Ok(store) => {
                        let searcher = crate::memory::ivf_index::IVFSearcher::new(
                            store.centroids().to_vec(),
                            store.assignments(),
                            10,
                        );
                        *self.mmap_store.write().unwrap() = Some(store);
                        *self.ivf_searcher.write().unwrap() = Some(searcher);
                        info!("🔬 IVF mmap store written to {}", mmap_path.display());
                    }
                    Err(e) => warn!("🔬 Failed to create IVF mmap store: {}", e),
                }
            }

            Ok(IvfBuildResult {
                n_centroids,
                elapsed_ms,
                skipped: false,
            })
        })
    }

    /// Graph analysis for tylluan_think: finds hubs, paths, and contradictions
    /// among a set of candidate node IDs.
    pub async fn analyze_subgraph(
        &self,
        node_ids: &[String],
        _query: &str,
    ) -> Result<ThinkAnalysis> {
        if node_ids.is_empty() {
            return Ok(ThinkAnalysis::default());
        }

        let mut degree_map: HashMap<String, usize> = HashMap::new();
        for id in node_ids {
            let neighbors: Vec<GraphNode> = self.get_context(id, 1).await.unwrap_or_default();
            let relevant_neighbors = neighbors.iter()
                .filter(|n| node_ids.contains(&n.id) && &n.id != id)
                .count();
            degree_map.insert(id.clone(), relevant_neighbors);
        }
        let hub = degree_map.iter()
            .max_by_key(|entry| *entry.1)
            .map(|(k, v)| (k.clone(), *v));

        let contradictions = self.find_contradictions_in(node_ids).await
            .unwrap_or_default();

        let path_summary = if node_ids.len() >= 2 {
            let path = self.get_context(&node_ids[0], 2).await.unwrap_or_default();
            path.iter()
                .filter(|n| node_ids.contains(&n.id) && n.id != node_ids[0])
                .take(3)
                .map(|n| {
                    if n.content.len() > 60 {
                        format!("{}...", n.content.chars().take(60).collect::<String>())
                    } else {
                        n.content.clone()
                    }
                })
                .collect()
        } else {
            vec![]
        };

        Ok(ThinkAnalysis {
            hub_node: hub,
            contradictions,
            connected_path: path_summary,
            node_count: node_ids.len(),
        })
    }

    /// Calculates the degree centrality (incoming + outgoing edge count) of a set of node IDs
    pub async fn degree_centrality(&self, node_ids: &[String]) -> Result<HashMap<String, usize>> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let mut degree_map = HashMap::new();
            if node_ids.is_empty() {
                return Ok(degree_map);
            }

            // Initialize all IDs with 0
            for id in node_ids {
                degree_map.insert(id.clone(), 0);
            }

            // Query in chunks of 50 to avoid SQLite parameter limits
            for chunk in node_ids.chunks(50) {
                let placeholders = std::iter::repeat_n("?", chunk.len())
                    .collect::<Vec<_>>()
                    .join(",");
                let sql = format!(
                    "SELECT source, target FROM edges WHERE source IN ({}) OR target IN ({})",
                    placeholders, placeholders
                );
                let mut stmt = conn.prepare(&sql)?;
                let mut rows = stmt.query(rusqlite::params_from_iter(chunk.iter().chain(chunk.iter())))?;

                while let Some(row) = rows.next()? {
                    let s: String = row.get(0)?;
                    let t: String = row.get(1)?;
                    if node_ids.contains(&s) {
                        *degree_map.entry(s).or_insert(0) += 1;
                    }
                    if node_ids.contains(&t) {
                        *degree_map.entry(t).or_insert(0) += 1;
                    }
                }
            }

            Ok(degree_map)
        })
    }

    /// Performs a local query over the graph using seed node IDs (activated via vector search)
    /// to return a ranked list of neighboring context nodes based on Personalized PageRank and degree centrality.
    pub async fn local_query_graph(
        &self,
        seed_ids: &[String],
        limit: usize,
    ) -> Result<Vec<(GraphNode, f32)>> {
        if seed_ids.is_empty() {
            return Ok(vec![]);
        }

        // 1. Personalized PageRank local around seeds
        let pagerank_results = self.personalized_pagerank_local(seed_ids, 0.85, 10, limit * 2).await?;
        if pagerank_results.is_empty() {
            return Ok(vec![]);
        }

        // 2. Compute degree centrality for candidate nodes
        let candidate_ids: Vec<String> = pagerank_results.iter().map(|(id, _)| id.clone()).collect();
        let degree_map = self.degree_centrality(&candidate_ids).await?;

        // 3. Retrieve GraphNodes and combine PageRank with Degree Centrality
        // final_score = pr_score / (1.0 + degree * 0.1)
        // Inverted bias: high-degree hub nodes are penalized, low-degree specific nodes are preferred.
        let results = tokio::task::block_in_place(|| -> Result<Vec<(GraphNode, f32)>> {
            let conn = self.conn.blocking_lock();
            let mut scored_nodes = Vec::new();

            for (id, pr_score) in pagerank_results {
                if let Ok(Some(node)) = self.get_node_sync(&id, &conn) {
                    let deg = *degree_map.get(&id).unwrap_or(&0) as f64;
                    let final_score = pr_score / (1.0 + deg * 0.1);
                    scored_nodes.push((node, final_score as f32));
                }
            }

            scored_nodes.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            scored_nodes.truncate(limit);
            Ok(scored_nodes)
        })?;

        Ok(results)
    }

    async fn find_contradictions_in(&self, _node_ids: &[String]) -> Result<Vec<String>> {
        Ok(vec![])
    }
}

/// Result of IVF index build.
#[derive(Debug, Clone)]
pub struct IvfBuildResult {
    /// Number of centroids built (0 if skipped).
    pub n_centroids: usize,
    /// Elapsed build time in milliseconds.
    pub elapsed_ms: u64,
    /// True if build was skipped (not enough vectors or centroids already exist).
    pub skipped: bool,
}

#[cfg(test)]
mod tests {
    use super::super::SilvaDB;
    use std::time::Instant;

    // Contract that consolidate_ivf_index() must fulfill:
    //   pub async fn consolidate_ivf_index(&self) -> anyhow::Result<IvfBuildResult>
    //   pub struct IvfBuildResult { pub n_centroids: usize, pub elapsed_ms: u64, pub skipped: bool }
    //   skipped=true si embeddings < IVF_MIN_VECTORS (50) o centroids ya existen

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ivf_autobuild_skips_when_empty() {
        let db = SilvaDB::in_memory().await.unwrap();
        let result = db.consolidate_ivf_index().await.unwrap();
        assert!(result.skipped, "should skip build if no embeddings exist");
        assert_eq!(result.n_centroids, 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ivf_autobuild_skips_when_centroids_already_exist() {
        let db = SilvaDB::in_memory().await.unwrap();

        // Insert fake centroid directly
        tokio::task::block_in_place(|| {
            let conn = db.conn.blocking_lock();
            let fake_vec: Vec<u8> = vec![0u8; 1024 * 4]; // 1024-dim f32
            conn.execute(
                "INSERT OR REPLACE INTO cluster_centroids (cluster_id, centroid_vector, model_id) VALUES (?1, ?2, ?3)",
                rusqlite::params!["c0", fake_vec, "test"],
            ).unwrap();
        });

        let result = db.consolidate_ivf_index().await.unwrap();
        assert!(result.skipped, "debe saltarse si ya hay centroides");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ivf_autobuild_completes_within_5s_for_200_vectors() {
        let db = SilvaDB::in_memory().await.unwrap();

        // Insertar 200 nodos con embeddings sintéticos (1024-dim)
        for i in 0..200usize {
            let node_id = format!("bench_node_{}", i);
            db.upsert_node(&node_id, "bench", &format!("contenido bench {}", i), "{}").await.unwrap();
            // Embedding sintético: vector con varianza real para que Louvain forme clusters
            let mut emb = vec![0.0f32; 1024];
            let cluster = i % 8;
            emb[cluster * 128] = 1.0 + (i as f32 * 0.01);
            emb[cluster * 128 + 1] = 0.5 + (i as f32 * 0.005);
            let emb_bytes: Vec<u8> = emb.iter().flat_map(|v| v.to_le_bytes()).collect();
            tokio::task::block_in_place(|| {
                let conn = db.conn.blocking_lock();
                conn.execute(
                    "INSERT OR REPLACE INTO node_embeddings (node_id, embedding, model_id) VALUES (?1, ?2, ?3)",
                    rusqlite::params![node_id, emb_bytes, "test-bge"],
                ).unwrap();
            });
        }

        let start = Instant::now();
        let result = db.consolidate_ivf_index().await.unwrap();
        let elapsed = start.elapsed();

        assert!(!result.skipped, "no debe saltar con 200 embeddings");
        assert!(result.n_centroids > 0, "must have at least 1 centroid");
        // In debug (unoptimized) builds allow more time; release target is <5s
        let time_limit_secs = if cfg!(debug_assertions) { 30 } else { 5 };
        assert!(
            elapsed.as_secs() < time_limit_secs,
            "warm-up excede {}s: {}ms", time_limit_secs, elapsed.as_millis()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ivf_autobuild_produces_queryable_centroids() {
        let db = SilvaDB::in_memory().await.unwrap();

        // 60 nodos con embeddings (supera el umbral de 50)
        for i in 0..60usize {
            let node_id = format!("qtest_node_{}", i);
            db.upsert_node(&node_id, "qtest", &format!("query test {}", i), "{}").await.unwrap();
            let mut emb = vec![0.0f32; 64];
            emb[i % 64] = 1.0;
            let emb_bytes: Vec<u8> = emb.iter().flat_map(|v| v.to_le_bytes()).collect();
            tokio::task::block_in_place(|| {
                let conn = db.conn.blocking_lock();
                conn.execute(
                    "INSERT OR REPLACE INTO node_embeddings (node_id, embedding, model_id) VALUES (?1, ?2, ?3)",
                    rusqlite::params![node_id, emb_bytes, "test-64"],
                ).unwrap();
            });
        }

        db.consolidate_ivf_index().await.unwrap();

        // Verificar que search_vector_ivf funciona tras el build
        let query = vec![1.0f32; 64];
        let results = db.search_vector_ivf(&query, 5).await;
        assert!(results.is_ok(), "IVF search debe funcionar tras autobuild: {:?}", results.err());
    }

}
