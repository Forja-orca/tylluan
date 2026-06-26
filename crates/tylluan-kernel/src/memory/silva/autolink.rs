use anyhow::Result;

impl super::SilvaDB {
    /// Link a new node to similar existing nodes using keyword overlap (Jaccard similarity).
    /// Returns number of edges created.
    pub async fn auto_link_similar(
        &self,
        new_node_id: &str,
        content: &str,
        max_links: usize,
        min_score: f32,
    ) -> Result<usize> {
        let candidates = self.search(content, 20, None).await?;
        let query_lower = content.to_lowercase();
        let query_words: std::collections::HashSet<&str> =
            query_lower.split_whitespace().collect();
        let mut count = 0usize;

        for node in &candidates {
            if node.id == new_node_id {
                continue;
            }
            if node.node_type == "routing_anchor" { continue; }
            let node_lower = node.content.to_lowercase();
            let content_words: std::collections::HashSet<&str> =
                node_lower.split_whitespace().collect();
            let intersection = query_words.intersection(&content_words).count();
            let union = query_words.union(&content_words).count();
            let score = if union == 0 { 0.0 } else { intersection as f32 / union as f32 };

            if score < min_score {
                continue;
            }

            let meta = serde_json::json!({"score": score, "method": "keyword_overlap"}).to_string();
            self.add_edge(new_node_id, &node.id, "related_to", 1.0, &meta).await?;
            count += 1;
            if count >= max_links {
                break;
            }
        }
        Ok(count)
    }

    /// Retroactively extract triples from existing nodes using an external extractor (LLM).
    /// This connects the scientific reasoning layer (triples) to nodes created via legacy ingest.
    pub async fn retrograde_extract_triples<F, Fut>(
        &self,
        limit: usize,
        extractor: F,
    ) -> Result<usize>
    where
        F: Fn(String) -> Fut + Send + Sync,
        Fut: std::future::Future<Output = Result<String>> + Send,
    {
        let nodes = self.get_nodes_limited(limit, 0.0).await?;
        let eligible: Vec<_> = nodes.into_iter().filter(|n| n.content.len() > 100).collect();
        tracing::info!("🔄 SilvaDB: processing {} nodes for retrograde extraction", eligible.len());

        let mut edges_added: usize = 0;
        for node in eligible {
            let snippet: String = node.content.chars().take(500).collect();
            let extract_result = extractor(snippet).await;
            // Abort entire loop on guild disconnection — avoids log spam
            if let Err(ref e) = extract_result {
                let msg = e.to_string();
                if msg.contains("disconnected") || msg.contains("not found") || msg.contains("not available") {
                    tracing::warn!("🕸️ Retrograde extract aborted: {}", msg);
                    break;
                }
            }
            if let Ok(triple_json) = extract_result
                && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&triple_json)
                    && let Some(triples) = parsed["triples"].as_array() {
                        for triple in triples {
                            let confidence = triple["confidence"].as_f64().unwrap_or(0.0);
                            if confidence >= 0.45 {
                                let subj = triple["subject"].as_str().unwrap_or("");
                                let pred = triple["predicate"].as_str().unwrap_or("relates_to");
                                let obj = triple["object"].as_str().unwrap_or("");
                                if !subj.is_empty() && !obj.is_empty() {
                                    let meta = serde_json::json!({"source": "retrograde_extract", "confidence": confidence}).to_string();
                                    if self.add_edge(subj, obj, pred, confidence, &meta).await.is_ok() {
                                        edges_added += 1;
                                    }
                                }
                            }
                        }
                    }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        Ok(edges_added)
    }

    /// Retroactively link existing orphan nodes (no outgoing edges) using keyword overlap.
    ///
    /// Called once at startup (after 10s delay) to connect nodes created before
    /// `auto_link_similar` was introduced. Processes up to `max_nodes` orphans.
    /// When `engine` is provided, uses hybrid semantic search for more aggressive linking.
    pub async fn retrolink_orphans(&self, max_nodes: usize, min_score: f32) -> Result<usize> {
        self.retrolink_orphans_with_engine(max_nodes, min_score, None).await
    }

    /// Same as retrolink_orphans but with optional embedding engine for hybrid semantic linking.
    pub async fn retrolink_orphans_with_engine(&self, max_nodes: usize, min_score: f32, engine: Option<&crate::router::embeddings::EmbeddingEngine>) -> Result<usize> {
        let all = self.get_nodes_limited(max_nodes * 4, 0.01).await?;
        let mut total_links = 0usize;
        let mut processed = 0usize;

        for node in &all {
            if processed >= max_nodes { break; }
            if node.node_type == "routing_anchor" { continue; }

            let is_orphan: bool = tokio::task::block_in_place(|| {
                let conn = self.conn.blocking_lock();
                conn.query_row(
                    "SELECT COUNT(*) FROM edges WHERE source = ?1",
                    rusqlite::params![node.id],
                    |row: &rusqlite::Row| row.get::<_, i64>(0),
                ).unwrap_or(0) == 0
            });
            if !is_orphan { continue; }

            let links = if let Some(eng) = engine {
                // Aggressive hybrid: semantic + keyword via search_hybrid
                let emb = eng.embed(&node.content).ok();
                let candidates = self.search_hybrid(&node.content, emb.as_deref(), 10).await.unwrap_or_default();
                let mut count = 0usize;
                for (neighbor, score) in &candidates {
                    if neighbor.id == node.id { continue; }
                    if *score < min_score * 0.1 { continue; } // hybrid RRF scores are ~0.005-0.05
                    let meta = serde_json::json!({"score": score, "method": "orphan_hybrid"}).to_string();
                    if self.add_edge(&node.id, &neighbor.id, "related_to", *score as f64, &meta).await.is_ok() {
                        count += 1;
                    }
                    if count >= 3 { break; }
                }
                count
            } else {
                self.auto_link_similar(&node.id, &node.content, 3, min_score).await.unwrap_or(0)
            };
            total_links += links;
            processed += 1;
        }

        tracing::info!("retrolink_orphans: processed {} orphans, created {} edges", processed, total_links);
        Ok(total_links)
    }
}
