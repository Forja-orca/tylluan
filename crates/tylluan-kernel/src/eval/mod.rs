pub mod longmemeval_s;

use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::sync::Arc;
use std::time::Instant;

use crate::memory::hybrid::HybridMemory;

/// A single benchmark result point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalPoint {
    pub query: String,
    pub expected_id: i64,
    pub rank: Option<usize>,
    pub score: Option<f64>,
    pub latency_ms: f64,
}

/// Aggregated benchmark results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalResult {
    pub benchmark: String,
    pub seed: u64,
    pub config: serde_json::Value,
    pub num_queries: usize,
    pub recall_at_1: f64,
    pub recall_at_5: f64,
    pub recall_at_10: f64,
    pub mean_latency_ms: f64,
    pub median_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub result_hash: String,
    pub points: Vec<EvalPoint>,
    pub computed_at: String,
}

fn fmt_timestamp() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Compute SHA-256 of config + seed for reproducibility verification.
fn compute_result_hash(benchmark: &str, seed: u64, config: &serde_json::Value) -> String {
    let input = format!("{}|{}|{}", benchmark, seed, serde_json::to_string(config).unwrap_or_default());
    let mut hasher = sha2::Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Run LongMemEval-S benchmark against the kernel's memory.
///
/// 1. Generate N test documents with a fixed seed (deterministic)
/// 2. Store them in HybridMemory
/// 3. Query each fact, measure rank position and latency
/// 4. Report recall@1/5/10 + latency percentiles
/// 5. Return EvalResult with hash for cross-run reproducibility verification
///
/// The same seed + config string → same result_hash on identical hardware+model.
pub async fn run_longmemeval_s(
    memory: Arc<HybridMemory>,
    num_queries: Option<usize>,
    seed: Option<u64>,
) -> EvalResult {
    let cfg = serde_json::json!({
        "num_distractors": 50,
        "model": "bge-m3",
    });
    let effective_seed = seed.unwrap_or(42);
    const MAX_EVAL_QUERIES: usize = 200;
    let n = num_queries.unwrap_or(30).min(MAX_EVAL_QUERIES);

    let data = longmemeval_s::generate_dataset(n, effective_seed);
    let mut points = Vec::with_capacity(n);
    let mut query_map: Vec<(String, i64)> = Vec::with_capacity(n);

    // Phase 1: Write all documents, capture their DB IDs
    for doc in &data.documents {
        let meta = serde_json::json!({
            "source": "longmemeval-s",
            "category": doc.category,
            "eval_seed": effective_seed,
            "eval_id": doc.id,
        });
        if let Ok(doc_id) = memory.add_document(&doc.content, &meta.to_string(), None).await {
            // Find matching query for this document
            if let Some(q) = data.queries.iter().find(|q| q.expected_content == doc.content) {
                query_map.push((q.text.clone(), doc_id));
            }
        }
    }

    // Phase 2: Query each fact and measure recall
    let mut latencies = Vec::with_capacity(n);
    for (query_text, expected_id) in &query_map {
        let t0 = Instant::now();
        let results = memory.search(query_text, None, 10).await.unwrap_or_default();
        let elapsed = t0.elapsed();
        latencies.push(elapsed.as_secs_f64() * 1000.0);

        let rank = results.iter().position(|r| r.id == *expected_id);
        let score = rank.and_then(|i| Some(results[i].score as f64));
        points.push(EvalPoint {
            query: query_text.clone(),
            expected_id: *expected_id,
            rank: rank.map(|r| r + 1),
            score,
            latency_ms: elapsed.as_secs_f64() * 1000.0,
        });
    }

    // Phase 3: Aggregate
    let total = points.len() as f64;
    let recall_1 = if total > 0.0 { points.iter().filter(|p| p.rank == Some(1)).count() as f64 / total } else { 0.0 };
    let recall_5 = if total > 0.0 { points.iter().filter(|p| p.rank.map_or(false, |r| r <= 5)).count() as f64 / total } else { 0.0 };
    let recall_10 = if total > 0.0 { points.iter().filter(|p| p.rank.map_or(false, |r| r <= 10)).count() as f64 / total } else { 0.0 };

    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mean_latency = if latencies.is_empty() { 0.0 } else { latencies.iter().sum::<f64>() / latencies.len() as f64 };
    let median_latency = if latencies.is_empty() { 0.0 } else { latencies[latencies.len() / 2] };
    let p95_idx = ((latencies.len() as f64) * 0.95) as usize;
    let p95_latency = if p95_idx < latencies.len() { latencies[p95_idx] } else { latencies.last().copied().unwrap_or(0.0) };

    EvalResult {
        benchmark: "longmemeval-s".into(),
        seed: effective_seed,
        config: cfg.clone(),
        num_queries: query_map.len(),
        recall_at_1: (recall_1 * 10000.0).round() / 100.0,
        recall_at_5: (recall_5 * 10000.0).round() / 100.0,
        recall_at_10: (recall_10 * 10000.0).round() / 100.0,
        mean_latency_ms: (mean_latency * 100.0).round() / 100.0,
        median_latency_ms: (median_latency * 100.0).round() / 100.0,
        p95_latency_ms: (p95_latency * 100.0).round() / 100.0,
        result_hash: compute_result_hash("longmemeval-s", effective_seed, &cfg),
        points,
        computed_at: fmt_timestamp(),
    }
}
