use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};
use crate::memory::silva::SilvaDB;
use crate::config::TylluanConfig;

pub struct DreamCycle {
    pub silva: Arc<SilvaDB>,
    config: Option<Arc<TylluanConfig>>,
}

#[derive(Debug, Default)]
pub struct DreamCycleReport {
    pub duplicates_merged: usize,
    pub nodes_decayed: usize,
    pub contradictions_flagged: usize,
    pub nodes_processed: usize,
    pub pair_comparisons: usize,
    pub exact_content_groups: usize,
    pub graph_nodes_total: usize,
    pub graph_edges_total: usize,
    pub salience_pruned: usize,
}

impl DreamCycle {
    pub fn new(silva: Arc<SilvaDB>) -> Self {
        Self { silva, config: None }
    }

    pub fn with_config(silva: Arc<SilvaDB>, config: Arc<TylluanConfig>) -> Self {
        Self { silva, config: Some(config) }
    }

    pub fn start_background_scheduler(&self) {
        let config = match &self.config {
            Some(c) => Arc::clone(c),
            None => return,
        };
        if !config.silva.decay_enabled {
            info!("🌙 Decay scheduler disabled (decay_enabled = false)");
            return;
        }
        let silva = Arc::clone(&self.silva);
        let interval_secs = (config.silva.decay_interval_hours * 3600) as u64;
        let prune_threshold = config.silva.decay_prune_threshold;
        let half_life = config.silva.decay_half_life_hours;

        tokio::spawn(async move {
            info!("🌙 Decay background scheduler started (interval={}s, prune_threshold={})", interval_secs, prune_threshold);
            loop {
                tokio::time::sleep(Duration::from_secs(interval_secs)).await;
                if let Err(e) = Self::run_decay_cycle(&silva, prune_threshold, half_life).await {
                    warn!("🌙 Decay cycle error: {}", e);
                }
            }
        });
    }

    async fn run_decay_cycle(silva: &Arc<SilvaDB>, prune_threshold: f64, half_life: u64) -> Result<(), anyhow::Error> {
        let decayed = silva.apply_decay(half_life).await?;
        let pruned = silva.prune_by_salience(prune_threshold).await?;
        if decayed > 0 || pruned > 0 {
            info!("🌙 Decay cycle: {} affected, {} pruned (salience < {})", decayed, pruned, prune_threshold);
        }
        Ok(())
    }

    pub async fn run(&self) -> DreamCycleReport {
        let mut report = DreamCycleReport::default();

        report.graph_nodes_total = self.silva.node_count().await.unwrap_or(0);
        report.graph_edges_total = self.silva.edge_count().await.unwrap_or(0) as usize;

        report.duplicates_merged = self.deduplicate(&mut report).await;
        report.nodes_decayed = self.apply_decay().await;
        report.contradictions_flagged = self.flag_contradictions().await;

        let prune_threshold = self.config.as_ref()
            .map(|c| c.silva.decay_prune_threshold)
            .unwrap_or(0.15);
        report.salience_pruned = self.silva.prune_by_salience(prune_threshold).await.unwrap_or(0);

        info!(
            "🌙 DreamCycle complete: {} merged, {} decayed, {} contradictions, {} salience-pruned \
             (processed {} nodes, {} pairs, {} exact-content groups | graph: {} nodes, {} edges)",
            report.duplicates_merged, report.nodes_decayed, report.contradictions_flagged,
            report.salience_pruned,
            report.nodes_processed, report.pair_comparisons, report.exact_content_groups,
            report.graph_nodes_total, report.graph_edges_total,
        );
        report
    }

    // Step 1: find node pairs with cosine similarity > 0.92, merge the lighter into the heavier
    async fn deduplicate(&self, report: &mut DreamCycleReport) -> usize {
        let nodes = match self.silva.get_nodes_limited(1000, 0.1).await {
            Ok(n) => n,
            Err(e) => { warn!("DreamCycle deduplicate: {}", e); return 0; }
        };
        report.nodes_processed = nodes.len();

        let mut merged = 0usize;
        let mut skip_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Phase 1: fast exact-content dedup — group identical content strings and merge
        // into the heaviest node. Catches graphrag_summary clones efficiently.
        // Build index: content -> Vec<(id, weight)>
        let mut content_groups: std::collections::HashMap<&str, Vec<(String, f64)>> =
            std::collections::HashMap::new();
        for node in &nodes {
            if node.protected || node.node_type == "identity" { continue; }
            if skip_ids.contains(&node.id) { continue; }
            content_groups.entry(node.content.as_str()).or_default().push((node.id.clone(), node.weight));
        }
        for group in content_groups.values() {
            if group.len() < 2 { continue; }
            report.exact_content_groups += 1;
            // Find heaviest node
            let keep_idx = group.iter().enumerate()
                .max_by(|(_, a), (_, b)| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(i, _)| i).unwrap_or(0);
            let keep_id = &group[keep_idx].0;
            for (i, node_data) in group.iter().enumerate() {
                if i == keep_idx { continue; }
                if skip_ids.contains(&node_data.0) { continue; }
                if let Ok(()) = self.silva.merge_node_into(&node_data.0, keep_id).await {
                    skip_ids.insert(node_data.0.clone());
                    merged += 1;
                }
            }
        }

        // Phase 2: cosine-similarity dedup for non-identical but similar nodes
        for i in 0..nodes.len() {
            if skip_ids.contains(&nodes[i].id) { continue; }
            if nodes[i].protected || nodes[i].node_type == "identity" { continue; }

            for j in (i + 1)..nodes.len() {
                if skip_ids.contains(&nodes[j].id) { continue; }
                if nodes[j].protected || nodes[j].node_type == "identity" { continue; }
                if nodes[i].node_type != nodes[j].node_type { continue; }
                report.pair_comparisons += 1;

                // Jaccard on content words as fast pre-filter before embedding cosine
                let sim = jaccard_words(&nodes[i].content, &nodes[j].content);
                if sim < 0.5 { continue; }

                // True cosine via SilvaDB embedding lookup (threshold from IdleLab atomic)
                let dedup_threshold = (crate::memory::idle_lab::DEDUP_COSINE.load(std::sync::atomic::Ordering::Relaxed) as f64) / 100.0;
                if let Ok(true) = self.silva.nodes_are_similar(&nodes[i].id, &nodes[j].id, dedup_threshold).await {
                    // Keep the heavier node, merge the lighter into it
                    let (keep_id, drop_id) = if nodes[i].weight >= nodes[j].weight {
                        (nodes[i].id.clone(), nodes[j].id.clone())
                    } else {
                        (nodes[j].id.clone(), nodes[i].id.clone())
                    };

                    if let Ok(()) = self.silva.merge_node_into(&drop_id, &keep_id).await {
                        skip_ids.insert(drop_id);
                        merged += 1;
                    }
                }
            }
        }
        merged
    }

    // Step 2: decay nodes using half-life exponential (14-day T½)
    async fn apply_decay(&self) -> usize {
        let nodes = match self.silva.get_nodes_limited(2000, 0.15).await {
            Ok(n) => n,
            Err(e) => { warn!("DreamCycle decay: {}", e); return 0; }
        };

        let now = chrono::Utc::now().timestamp();
        let mut decayed = 0usize;

        for node in &nodes {
            if node.protected || node.node_type == "identity" || node.node_type == "agent_summary" {
                continue;
            }
            let last_update = parse_timestamp(node.updated_at.as_deref().unwrap_or(""));
            let elapsed_secs = now - last_update;
            if elapsed_secs >= 2592000 {
                let _ = self.silva.decay_node(&node.id, elapsed_secs).await;
                decayed += 1;
            }
        }
        decayed
    }

    // Step 3: find conflicting edges (same source+predicate, different target) and mark nodes
    async fn flag_contradictions(&self) -> usize {
        match self.silva.flag_contradiction_nodes().await {
            Ok(n) => n,
            Err(e) => { warn!("DreamCycle contradictions: {}", e); 0 }
        }
    }
}

// Fast word-level Jaccard for pre-filtering before cosine
fn jaccard_words(a: &str, b: &str) -> f64 {
    let set_a: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let set_b: std::collections::HashSet<&str> = b.split_whitespace().collect();
    if set_a.is_empty() && set_b.is_empty() { return 1.0; }
    let inter = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();
    if union == 0 { 0.0 } else { inter as f64 / union as f64 }
}

fn parse_timestamp(s: &str) -> i64 {
    // Try unix integer first
    if let Ok(n) = s.parse::<i64>() { return n; }
    // Try ISO 8601 with timezone (RFC3339)
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) { return dt.timestamp(); }
    // Try SQLite format: "2026-06-07 22:41:00" (space separator, no tz — assume UTC)
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return dt.and_utc().timestamp();
    }
    // Fallback: treat as old
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_half_life_math() {
        let lambda = std::f64::consts::LN_2 / 1_209_600.0;
        let w0 = 1.0_f64;
        let w_half = w0 * (-lambda * 1_209_600.0_f64).exp();
        assert!((w_half - 0.5).abs() < 0.001, "Half-life math incorrect: {}", w_half);
        let w_zero = w0 * (-lambda * 0.0_f64).exp();
        assert_eq!(w_zero, 1.0);
    }

    #[test]
    fn test_salience_increases_with_access() {
        let s0 = 1.0_f32 * (1.0 + (0.0_f32).ln_1p() * 0.1);
        let s10 = 1.0_f32 * (1.0 + (10.0_f32).ln_1p() * 0.1);
        assert!(s10 > s0, "Salience should increase with access count");
    }

    #[test]
    fn test_jaccard_identical() {
        assert_eq!(jaccard_words("hello world", "hello world"), 1.0);
    }

    #[test]
    fn test_jaccard_disjoint() {
        assert_eq!(jaccard_words("foo bar", "baz qux"), 0.0);
    }

    #[test]
    fn test_jaccard_partial() {
        let sim = jaccard_words("rust async await", "rust sync await");
        assert!(sim > 0.0 && sim < 1.0);
    }

    #[test]
    fn test_parse_timestamp_unix() {
        assert_eq!(parse_timestamp("1700000000"), 1700000000);
    }

    #[test]
    fn test_parse_timestamp_invalid_returns_zero() {
        assert_eq!(parse_timestamp("not-a-date"), 0);
    }

    #[test]
    fn test_parse_timestamp_sqlite_format() {
        // SQLite CURRENT_TIMESTAMP produces "YYYY-MM-DD HH:MM:SS" without T or timezone
        let ts = parse_timestamp("2026-06-07 22:41:00");
        assert!(ts > 0, "SQLite timestamp format must parse to non-zero");
        assert_eq!(ts, 1780872060);
    }
}
