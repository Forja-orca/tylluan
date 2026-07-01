use crate::memory::silva::{GraphNode, SilvaDB};
use std::collections::HashMap;
use std::sync::Arc;

pub struct DualRetrievalResult {
    pub low_level: Vec<(GraphNode, f32)>,
    pub high_level: Vec<(GraphNode, f32)>,
    pub merged: Vec<(GraphNode, f32)>,
}

pub async fn dual_retrieve(
    silva: &Arc<SilvaDB>,
    query: &str,
    query_embedding: Option<&[f32]>,
    limit: usize,
) -> anyhow::Result<DualRetrievalResult> {
    let low_level = silva.search_hybrid(query, query_embedding, limit * 2, None).await?;

    let seed_ids: Vec<String> = low_level
        .iter()
        .take(3)
        .map(|(n, _)| n.id.clone())
        .collect();

    let mut high_map: HashMap<String, (GraphNode, f32)> = HashMap::new();
    for seed_id in &seed_ids {
        if let Ok(context) = silva.get_context(seed_id, 1).await {
            for neighbor in context {
                let degree = compute_degree(silva, &neighbor.id);
                let hs = 0.3 * (1.0 + (degree as f32 + 1.0).log2() / 10.0);
                high_map
                    .entry(neighbor.id.clone())
                    .and_modify(|e| {
                        if hs > e.1 {
                            e.1 = hs;
                        }
                    })
                    .or_insert((neighbor, hs));
            }
        }
    }
    let high_level: Vec<(GraphNode, f32)> = high_map.into_values().collect();

    let mut merge_map: HashMap<String, (GraphNode, f32)> = HashMap::new();
    for (node, score) in &low_level {
        merge_map.insert(node.id.clone(), (node.clone(), *score));
    }
    for (node, hs) in &high_level {
        merge_map
            .entry(node.id.clone())
            .and_modify(|(_, score)| {
                *score = *score * 0.7 + hs * 0.3;
            })
            .or_insert((node.clone(), *hs));
    }
    let mut merged: Vec<(GraphNode, f32)> = merge_map.into_values().collect();

    merged.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let top_k = merged.len().min(20);
    let top_ids: Vec<String> = merged.iter().take(top_k).map(|(n, _)| n.id.clone()).collect();

    let degrees: HashMap<String, i64> = top_ids
        .iter()
        .map(|id| (id.clone(), compute_degree(silva, id)))
        .collect();

    for (node, score) in &mut merged {
        let degree = degrees.get(&node.id).copied().unwrap_or(0);
        let boost = 1.0 + (degree as f32 + 1.0).log2() * 0.05;
        *score *= boost;
    }

    merged.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    merged.truncate(limit);

    Ok(DualRetrievalResult {
        low_level,
        high_level,
        merged,
    })
}

fn compute_degree(silva: &Arc<SilvaDB>, node_id: &str) -> i64 {
    if let Ok(guard) = silva.conn_lock().try_lock() {
        if let Ok(count) = guard.query_row(
            "SELECT COUNT(*) FROM edges WHERE source = ?1 OR target = ?1",
            rusqlite::params![node_id],
            |row| row.get::<_, i64>(0),
        ) {
            return count;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::silva::GraphNode;

    fn boost(degree: i64) -> f32 {
        1.0 + (degree as f32 + 1.0).log2() * 0.05
    }

    #[test]
    fn test_centralidad_boost_monotonica() {
        let b0 = boost(0);
        let b1 = boost(1);
        let b100 = boost(100);

        assert!((b0 - 1.0).abs() < 1e-6, "degree 0 should give boost 1.0, got {b0}");
        assert!(b1 > 1.0, "degree 1 should give boost > 1.0, got {b1}");
        assert!(b100 < 2.0, "degree 100 should give boost < 2.0, got {b100}");
        assert!(b1 < b100, "boost should be monotonically increasing");
    }

    #[test]
    fn test_merge_deduplica_por_node_id() {
        let node_a = GraphNode {
            id: "a".into(),
            node_type: "test".into(),
            content: String::new(),
            metadata: String::new(),
            weight: 1.0,
            protected: false,
            conflicted: false,
            topic_key: None,
            created_at: None,
            updated_at: None,
            last_touched: chrono::Utc::now(),
            valid_from: None,
            valid_until: None,
            shareable: false,
        };
        let node_b = GraphNode {
            id: "b".into(),
            node_type: "test".into(),
            content: String::new(),
            metadata: String::new(),
            weight: 1.0,
            protected: false,
            conflicted: false,
            topic_key: None,
            created_at: None,
            updated_at: None,
            last_touched: chrono::Utc::now(),
            valid_from: None,
            valid_until: None,
            shareable: false,
        };

        let low: Vec<(GraphNode, f32)> = vec![(node_a.clone(), 0.8), (node_b.clone(), 0.3)];
        let high: Vec<(GraphNode, f32)> = vec![(node_a.clone(), 0.4)];

        let mut map: HashMap<String, (GraphNode, f32)> = HashMap::new();
        for (n, s) in &low {
            map.insert(n.id.clone(), (n.clone(), *s));
        }
        for (n, hs) in &high {
            map.entry(n.id.clone())
                .and_modify(|(_, score)| {
                    *score = *score * 0.7 + hs * 0.3;
                })
                .or_insert((n.clone(), *hs));
        }

        assert_eq!(map.len(), 2, "merged should deduplicate node_a — expected 2 entries");

        let expected = 0.8 * 0.7 + 0.4 * 0.3;
        let (_, score_a) = map.get("a").unwrap();
        assert!(
            (score_a - expected).abs() < 1e-6,
            "combined score should be {expected}, got {score_a}"
        );
    }
}
