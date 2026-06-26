use std::sync::Arc;
use std::collections::HashMap;
use tracing::{info, warn};
use crate::memory::cosine::cosine_similarity;
use crate::memory::silva::SilvaDB;
use crate::router::embeddings::EmbeddingEngine;

pub struct AutoLinker {
    pub silva: Arc<SilvaDB>,
}

#[derive(Debug, Default)]
pub struct AutoLinkReport {
    pub file_ref_edges: usize,
    pub tool_ref_edges: usize,
    pub topic_edges: usize,
    pub orphan_edges: usize,
    pub doc_similar_edges: usize,
    pub low_threshold_edges: usize,
    pub triple_edges: usize,
    pub nodes_total: usize,
    pub edges_before: usize,
    pub edges_after: usize,
}

impl AutoLinker {
    pub fn new(silva: Arc<SilvaDB>) -> Self {
        Self { silva }
    }

    pub async fn run(&self, engine: Option<&EmbeddingEngine>) -> AutoLinkReport {
        let mut report = AutoLinkReport::default();
        report.edges_before = self.silva.edge_count().await.unwrap_or(0) as usize;
        report.nodes_total = self.silva.node_count().await.unwrap_or(0);

        info!("[AutoLink] Starting CERO-LLM pass on {} nodes ({} edges before)", report.nodes_total, report.edges_before);

        report.file_ref_edges = self.link_file_references().await;
        report.tool_ref_edges = self.link_tool_references().await;
        report.topic_edges = self.link_by_topic().await;
        report.orphan_edges = self.link_orphans(engine).await;
        report.doc_similar_edges = self.link_similar_nodes(0.70, 5).await;
        report.low_threshold_edges = self.link_low_threshold_nodes(0.15, 3).await;

        report.edges_after = self.silva.edge_count().await.unwrap_or(0) as usize;
        info!("[AutoLink] CERO-LLM complete: +{} edges ({} before → {} after) | file_ref={} tool_ref={} topic={} orphan={} doc_sim={} low_thresh={}",
            report.edges_after - report.edges_before, report.edges_before, report.edges_after,
            report.file_ref_edges, report.tool_ref_edges, report.topic_edges,
            report.orphan_edges, report.doc_similar_edges, report.low_threshold_edges);
        report
    }

    /// Detect file references in node content and create `references` edges
    /// e.g., a CLAUDE.md chunk mentioning "STATUS.md" → edge to STATUS.md nodes
    async fn link_file_references(&self) -> usize {
        let nodes = match self.silva.get_nodes_limited(5000, 0.0).await {
            Ok(n) => n,
            Err(e) => { warn!("[AutoLink] get_nodes failed: {}", e); return 0; }
        };

        // Build index: filename stem → list of node IDs from that file
        let mut file_index: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
        let mut source_file_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();

        for node in &nodes {
            let meta: serde_json::Value = serde_json::from_str(&node.metadata).unwrap_or(serde_json::Value::Null);
            let source = meta.get("source_file")
                .and_then(|v| v.as_str())
                .or_else(|| meta.get("filename").and_then(|v| v.as_str()));
            if let Some(fname) = source {
                let stem = fname.to_lowercase();
                file_index.entry(stem.clone()).or_default().push(node.id.clone());
                source_file_map.insert(node.id.clone(), stem);
            }
        }

        // Known markdown files to detect in content
        let known_files: Vec<&str> = vec![
            "CLAUDE.md", "STATUS.md", "WORKFLOW.md", "ROADMAP.md",
            "ROADMAP-CICLOS.md", "VETERAN_REVIEW.md", "COLMENA_OLEADA1",
            "SUMMARY.md", "ARCHITECTURE_DECISIONS.md",
        ];
        let known_lower: Vec<String> = known_files.iter().map(|f| f.to_lowercase()).collect();

        let mut count = 0usize;
        for node in &nodes {
            let src_stem = match source_file_map.get(&node.id) {
                Some(s) => s.clone(),
                None => continue,
            };
            let lower = node.content.to_lowercase();
            for (i, _fname) in known_files.iter().enumerate() {
                let stem = &known_lower[i];
                if stem == &src_stem { continue; } // skip self-references
                if !lower.contains(stem) { continue; }
                // Found a reference — link to up to 5 nodes from target file
                if let Some(targets) = file_index.get(stem) {
                    for target_id in targets.iter().take(5) {
                        if let Err(e) = self.silva.add_edge(&node.id, target_id, "references", 0.6, "{\"method\":\"file_ref\"}").await {
                            warn!("[AutoLink] file_ref edge failed: {}", e);
                        } else {
                            count += 1;
                        }
                    }
                }
            }
        }
        info!("[AutoLink] file_references: {} edges created", count);
        count
    }

    /// Detect tool/API name mentions and create `uses_tool` edges
    async fn link_tool_references(&self) -> usize {
        let tool_names = [
            ("tylluan_do", "tylluan_do"),
            ("tylluan_remember", "tylluan_remember"),
            ("tylluan_recall", "tylluan_recall"),
            ("tylluan_think", "tylluan_think"),
            ("tylluan_graph", "tylluan_graph"),
            ("tylluan_nexus", "tylluan-nexus"),
            ("silvadb", "silvadb"),
            ("fastmcp", "fastmcp"),
            ("bge-m3", "bge-m3"),
            ("sse", "sse"),
        ];

        let nodes = match self.silva.get_nodes_limited(5000, 0.0).await {
            Ok(n) => n,
            Err(e) => { warn!("[AutoLink] get_nodes failed: {}", e); return 0; }
        };

        let mut count = 0usize;
        for node in &nodes {
            let lower = node.content.to_lowercase();
            for (keyword, tool_id) in &tool_names {
                if !lower.contains(keyword) { continue; }
                if let Err(e) = self.silva.add_edge(&node.id, tool_id, "uses_tool", 0.5, "{\"method\":\"tool_ref\"}").await {
                    warn!("[AutoLink] tool_ref edge failed: {}", e);
                } else {
                    count += 1;
                }
            }
        }
        info!("[AutoLink] tool_references: {} edges created", count);
        count
    }

    /// Connect nodes sharing the same topic_key with `same_topic` edges.
    /// Uses direct SQL query (not get_nodes_limited) to avoid dependency on
    /// weight-based filtering and ensure all topic_key-bearing nodes are found.
    async fn link_by_topic(&self) -> usize {
        use std::collections::HashSet;

        let topic_groups: std::collections::HashMap<String, Vec<String>> = tokio::task::block_in_place(|| {
            let lock = self.silva.conn_lock();
            let conn = lock.blocking_lock();
            let mut stmt = match conn.prepare(
                "SELECT id, topic_key FROM nodes WHERE topic_key IS NOT NULL AND topic_key != ''"
            ) {
                Ok(s) => s,
                Err(e) => {
                    warn!("[AutoLink] link_by_topic prepare failed: {}", e);
                    return std::collections::HashMap::new();
                }
            };
            let rows = match stmt.query_map([], |row| {
                let id: String = row.get(0)?;
                let topic: String = row.get(1)?;
                Ok((id, topic))
            }) {
                Ok(r) => r,
                Err(e) => {
                    warn!("[AutoLink] link_by_topic query failed: {}", e);
                    return std::collections::HashMap::new();
                }
            };
            let mut groups: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
            for row in rows.flatten() {
                let (id, topic) = row;
                groups.entry(topic).or_default().push(id);
            }
            groups
        });

        if topic_groups.is_empty() {
            info!("[AutoLink] topic links: no topic_key nodes found");
            return 0;
        }

        info!("[AutoLink] topic links: {} groups found", topic_groups.len());
        for (topic, ids) in &topic_groups {
            info!("[AutoLink]   topic '{}': {} nodes", topic, ids.len());
        }

        let mut count = 0usize;
        let mut seen: HashSet<(String, String)> = HashSet::new();

        for ids in topic_groups.values() {
            if ids.len() < 2 { continue; }
            for i in 0..ids.len().min(20) {
                for j in (i + 1)..ids.len().min(20) {
                    let key = if ids[i] < ids[j] {
                        (ids[i].clone(), ids[j].clone())
                    } else {
                        (ids[j].clone(), ids[i].clone())
                    };
                    if seen.contains(&key) { continue; }
                    seen.insert(key);
                    if let Err(e) = self.silva.add_edge(&ids[i], &ids[j], "same_topic", 0.7, "{\"method\":\"topic\"}").await {
                        warn!("[AutoLink] topic edge failed: {}", e);
                    } else {
                        count += 1;
                    }
                }
            }
        }
        info!("[AutoLink] topic links: {} edges created across {} groups", count, topic_groups.len());
        count
    }

    /// Doc-to-doc strategy: link content-bearing nodes by embedding cosine similarity.
    /// Compares all embeddings of `episode`, `document`, `agent_memory`, `memory`,
    /// `summary`, `lesson`, `concept`, `synthesis` types and creates `related_to`
    /// edges for pairs with similarity >= threshold.
    async fn link_similar_nodes(&self, threshold: f32, max_per_node: usize) -> usize {
        let content_types = [
            "episode", "document", "agent_memory", "memory",
            "summary", "lesson", "concept", "synthesis",
        ];

        // Build placeholders: (?1,?2,...,?N)
        let placeholders: Vec<String> = (1..=content_types.len())
            .map(|i| format!("?{}", i))
            .collect();
        let placeholder_str = placeholders.join(",");

        // Load node IDs, content, and embeddings for content-bearing types
        #[allow(clippy::type_complexity)]
        let all_data: Vec<(String, Vec<f32>)> = tokio::task::block_in_place(|| {
            let lock = self.silva.conn_lock();
            let conn = lock.blocking_lock();
            let mut stmt = match conn.prepare(
                &format!(
                    "SELECT DISTINCT n.id, e.embedding FROM node_embeddings e
                     JOIN nodes n ON n.id = e.node_id
                     WHERE n.type IN ({}) AND e.embedding IS NOT NULL
                     LIMIT 5000",
                    placeholder_str
                ),
            ) {
                Ok(s) => s,
                Err(e) => {
                    warn!("[AutoLink] prepare failed: {}", e);
                    return Vec::new();
                }
            };

            let params: Vec<&dyn rusqlite::types::ToSql> = content_types
                .iter().map(|t| t as &dyn rusqlite::types::ToSql).collect();

            let rows = match stmt.query_map(params.as_slice(), |row| {
                let id: String = row.get(0)?;
                let blob: Vec<u8> = row.get(1)?;
                // Decode f32 little-endian blob
                let emb: Vec<f32> = blob.chunks_exact(4)
                    .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect();
                Ok((id, emb))
            }) {
                Ok(r) => r,
                Err(e) => {
                    warn!("[AutoLink] query_map failed: {}", e);
                    return Vec::new();
                }
            };

            let mut data = Vec::new();
            for row in rows.flatten() {
                data.push(row);
            }
            data
        });

        let n = all_data.len();
        if n < 2 {
            info!("[AutoLink] doc similarity: only {} content nodes, skipping", n);
            return 0;
        }

        info!("[AutoLink] doc similarity: comparing {} content nodes", n);

        // Precompute L2 magnitudes for cosine normalization
        let magnitudes: Vec<f32> = all_data.iter()
            .map(|(_, emb)| emb.iter().map(|x| x * x).sum::<f32>().sqrt())
            .collect();

        let mut count = 0usize;
        let mut per_node: HashMap<String, usize> = HashMap::new();

        for i in 0..n {
            let id_i = &all_data[i].0;
            let emb_i = &all_data[i].1;
            let mag_i = magnitudes[i];
            if mag_i < 1e-8 { continue; }

            for j in (i + 1)..n {
                let mag_j = magnitudes[j];
                if mag_j < 1e-8 { continue; }

                let sim = cosine_similarity(emb_i, &all_data[j].1);
                if sim.is_nan() || sim < threshold { continue; }

                let id_j = &all_data[j].0;

                // Enforce per-node edge cap (avoid double mutable borrow)
                let ci = per_node.get(id_i).copied().unwrap_or(0);
                let cj = per_node.get(id_j).copied().unwrap_or(0);
                if ci >= max_per_node || cj >= max_per_node { continue; }
                per_node.insert(id_i.clone(), ci + 1);
                per_node.insert(id_j.clone(), cj + 1);

                let weight = (sim as f64).min(0.95);
                if let Err(e) = self.silva.add_edge(
                    id_i, id_j, "related_to", weight,
                    "{\"method\":\"doc_similarity\"}",
                ).await {
                    warn!("[AutoLink] doc-similar edge failed: {}", e);
                } else {
                    count += 1;
                }
            }
        }

        info!(
            "[AutoLink] doc-to-doc similarity: {} edges created (threshold={}, max_per_node={})",
            count, threshold, max_per_node
        );
        count
    }

    /// Low-threshold linking for lesson/plan nodes: connect them to any content-bearing
    /// node with similarity >= low_threshold. Uses keyword overlap (Jaccard) over content.
    async fn link_low_threshold_nodes(&self, low_threshold: f32, max_per_node: usize) -> usize {
        let nodes = match self.silva.get_nodes_limited(2000, 0.0).await {
            Ok(n) => n,
            Err(e) => { warn!("[AutoLink] low_threshold get_nodes failed: {}", e); return 0; }
        };

        // Collect lesson/plan target nodes
        let targets: Vec<(String, String)> = nodes.iter()
            .filter(|n| (n.node_type == "lesson" || n.node_type == "plan") && !n.protected)
            .map(|n| (n.id.clone(), n.content.clone()))
            .collect();

        if targets.is_empty() {
            info!("[AutoLink] low_threshold: no lesson/plan nodes found");
            return 0;
        }

        info!("[AutoLink] low_threshold: linking {} lesson/plan nodes (threshold={})", targets.len(), low_threshold);

        let mut count = 0usize;
        let mut per_node: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

        for (target_id, target_content) in &targets {
            let target_lower = target_content.to_lowercase();
            let target_words: std::collections::HashSet<&str> =
                target_lower.split_whitespace().collect();
            if target_words.is_empty() { continue; }

            let mut linked = 0usize;
            for neighbor in &nodes {
                if neighbor.id == *target_id { continue; }
                if neighbor.node_type == "routing_anchor" { continue; }
                let ncount = per_node.get(&neighbor.id).copied().unwrap_or(0);
                if ncount >= max_per_node { continue; }

                let neighbor_lower = neighbor.content.to_lowercase();
                let neighbor_words: std::collections::HashSet<&str> =
                    neighbor_lower.split_whitespace().collect();
                let intersection = target_words.intersection(&neighbor_words).count();
                let union = target_words.union(&neighbor_words).count();
                let score = if union == 0 { 0.0 } else { intersection as f32 / union as f32 };
                if score < low_threshold { continue; }

                let meta = serde_json::json!({"score": score, "method": "low_threshold"}).to_string();
                if self.silva.add_edge(target_id, &neighbor.id, "related_to", score as f64, &meta).await.is_ok() {
                    count += 1;
                    linked += 1;
                    *per_node.entry(neighbor.id.clone()).or_insert(0) += 1;
                    if linked >= max_per_node { break; }
                }
            }
        }

        info!("[AutoLink] low_threshold: {} edges created for {} lesson/plan nodes", count, targets.len());
        count
    }

    /// Link orphan nodes (no outgoing edges) using keyword overlap or hybrid search
    async fn link_orphans(&self, engine: Option<&EmbeddingEngine>) -> usize {
        let n = if let Some(eng) = engine {
            self.silva.retrolink_orphans_with_engine(500, 0.15, Some(eng)).await
        } else {
            self.silva.retrolink_orphans(500, 0.15).await
        };
        match n {
            Ok(n) => {
                info!("[AutoLink] orphan links: {} edges created", n);
                n
            }
            Err(e) => {
                warn!("[AutoLink] retrolink_orphans failed: {}", e);
                0
            }
        }
    }
}
