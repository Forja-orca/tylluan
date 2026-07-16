


#[derive(Debug, Clone, serde::Serialize)]
#[allow(dead_code)]
pub struct QueryResult {
    pub correct_in_top1: bool,
    pub correct_in_top3: bool,
    pub correct_in_top5: bool,
    pub correct_in_top10: bool,
    pub precision_at_1: f64,
    pub precision_at_3: f64,
    pub precision_at_5: f64,
    pub precision_at_10: f64,
    pub recall_at_5: f64,
    pub latency_ms: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BenchmarkReport {
    pub suite_name: String,
    pub num_queries: usize,
    pub num_nodes: usize,
    pub num_edges: usize,
    pub recall_at_1: f64,
    pub recall_at_3: f64,
    pub recall_at_5: f64,
    pub recall_at_10: f64,
    pub precision_at_5: f64,
    pub latency_p50_ms: f64,
    pub latency_p95_ms: f64,
    pub latency_p99_ms: f64,
    pub embedding_available: bool,
    pub reranker_available: bool,
    pub per_query: Vec<QueryResult>,
    pub contradiction_accuracy: Option<ContradictionAccuracy>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ContradictionAccuracy {
    pub total: usize,
    pub correct_version_outranks_wrong: usize,
    pub both_in_top5: usize,
}

pub fn compute_query_result(
    retrieved: &[(String, f32)],
    relevant_ids: &[String],
) -> QueryResult {
    let retrieved_ids: Vec<&str> = retrieved.iter().map(|(id, _)| id.as_str()).collect();
    let top1 = retrieved_ids.first().copied();
    let top3: Vec<&str> = retrieved_ids.iter().take(3).copied().collect();
    let top5: Vec<&str> = retrieved_ids.iter().take(5).copied().collect();
    let top10: Vec<&str> = retrieved_ids.iter().take(10).copied().collect();

    let correct_in_top1 = top1.is_some_and(|id| relevant_ids.contains(&id.to_string()));
    let correct_in_top3 = top3.iter().any(|id| relevant_ids.contains(&id.to_string()));
    let correct_in_top5 = top5.iter().any(|id| relevant_ids.contains(&id.to_string()));
    let correct_in_top10 = top10.iter().any(|id| relevant_ids.contains(&id.to_string()));

    let hits_at_1 = if correct_in_top1 { 1.0 } else { 0.0 };
    let hits_at_3 = top3.iter().filter(|id| relevant_ids.contains(&id.to_string())).count() as f64;
    let hits_at_5 = top5.iter().filter(|id| relevant_ids.contains(&id.to_string())).count() as f64;
    let hits_at_10 = top10.iter().filter(|id| relevant_ids.contains(&id.to_string())).count() as f64;

    let num_relevant = relevant_ids.len() as f64;

    QueryResult {
        correct_in_top1,
        correct_in_top3,
        correct_in_top5,
        correct_in_top10,
        precision_at_1: hits_at_1 / 1.0,
        precision_at_3: hits_at_3 / 3.0,
        precision_at_5: hits_at_5 / 5.0,
        precision_at_10: hits_at_10 / 10.0,
        recall_at_5: if num_relevant > 0.0 { hits_at_5 / num_relevant } else { 0.0 },
        latency_ms: 0.0,
    }
}

pub fn compute_report(
    suite_name: &str,
    num_nodes: usize,
    num_edges: usize,
    results: Vec<QueryResult>,
    embedding_available: bool,
    reranker_available: bool,
    contradiction_accuracy: Option<ContradictionAccuracy>,
) -> BenchmarkReport {
    let n = results.len() as f64;
    if n == 0.0 {
        return BenchmarkReport {
            suite_name: suite_name.to_string(),
            num_queries: 0,
            num_nodes,
            num_edges,
            recall_at_1: 0.0,
            recall_at_3: 0.0,
            recall_at_5: 0.0,
            recall_at_10: 0.0,
            precision_at_5: 0.0,
            latency_p50_ms: 0.0,
            latency_p95_ms: 0.0,
            latency_p99_ms: 0.0,
            embedding_available,
            reranker_available,
            per_query: results,
            contradiction_accuracy,
        };
    }

    let recall_at_1 = results.iter().filter(|r| r.correct_in_top1).count() as f64 / n * 100.0;
    let recall_at_3 = results.iter().filter(|r| r.correct_in_top3).count() as f64 / n * 100.0;
    let recall_at_5 = results.iter().filter(|r| r.correct_in_top5).count() as f64 / n * 100.0;
    let recall_at_10 = results.iter().filter(|r| r.correct_in_top10).count() as f64 / n * 100.0;
    let precision_at_5 = results.iter().map(|r| r.precision_at_5).sum::<f64>() / n * 100.0;

    let mut latencies: Vec<f64> = results.iter().map(|r| r.latency_ms).collect();
    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let latency_p50 = percentile(&latencies, 50.0);
    let latency_p95 = percentile(&latencies, 95.0);
    let latency_p99 = percentile(&latencies, 99.0);

    BenchmarkReport {
        suite_name: suite_name.to_string(),
        num_queries: results.len(),
        num_nodes,
        num_edges,
        recall_at_1,
        recall_at_3,
        recall_at_5,
        recall_at_10,
        precision_at_5,
        latency_p50_ms: latency_p50,
        latency_p95_ms: latency_p95,
        latency_p99_ms: latency_p99,
        embedding_available,
        reranker_available,
        per_query: results,
        contradiction_accuracy,
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((p / 100.0) * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[idx.clamp(0, sorted.len() - 1)]
}

pub fn print_report(report: &BenchmarkReport) {
    println!();
    println!("═══════════════════════════════════════════════════════");
    println!("  Tylluan-Evals: {} Benchmark Report", report.suite_name);
    println!("═══════════════════════════════════════════════════════");
    println!();
    println!("  Corpus:    {} nodes, {} edges, {} queries",
        report.num_nodes, report.num_edges, report.num_queries);
    println!("  Embeddings: {}  |  Reranker: {}",
        if report.embedding_available { "YES" } else { "NO (text-only)" },
        if report.reranker_available { "YES" } else { "NO" });
    println!();
    println!("  ┌─────────────────────┬──────────┐");
    println!("  │ Metric              │  Value   │");
    println!("  ├─────────────────────┼──────────┤");
    println!("  │ Recall@1            │  {:>5.1}%  │", report.recall_at_1);
    println!("  │ Recall@3            │  {:>5.1}%  │", report.recall_at_3);
    println!("  │ Recall@5            │  {:>5.1}%  │", report.recall_at_5);
    println!("  │ Recall@10           │  {:>5.1}%  │", report.recall_at_10);
    println!("  │ Precision@5         │  {:>5.1}%  │", report.precision_at_5);
    println!("  ├─────────────────────┼──────────┤");
    println!("  │ Latency p50 (ms)    │  {:>7.1} │", report.latency_p50_ms);
    println!("  │ Latency p95 (ms)    │  {:>7.1} │", report.latency_p95_ms);
    println!("  │ Latency p99 (ms)    │  {:>7.1} │", report.latency_p99_ms);
    println!("  └─────────────────────┴──────────┘");
    println!();

    if let Some(ref ca) = report.contradiction_accuracy {
        println!("  ⚔ Contradiction Resolution:");
        println!("    Total contradictions: {}", ca.total);
        println!("    Correct outranks wrong: {}/{} ({:.1}%)",
            ca.correct_version_outranks_wrong, ca.total,
            (ca.correct_version_outranks_wrong as f64 / ca.total as f64 * 100.0));
        if ca.both_in_top5 > 0 {
            println!("    ⚠  Both versions in top-5: {}", ca.both_in_top5);
        }
        println!();
    }

    println!("  Per-query breakdown:");
    println!();
    for (i, qr) in report.per_query.iter().enumerate() {
        let icon = if qr.correct_in_top5 { "✓" } else { "✗" };
        println!("  {}  Q{}: Recall@5={}  Latency={:.1}ms  Precision@5={:.0}%",
            icon, i + 1,
            if qr.correct_in_top5 { "YES" } else { "no" },
            qr.latency_ms,
            qr.precision_at_5 * 100.0);
    }
    println!();
    println!("═══════════════════════════════════════════════════════");
    println!();
}

/// Prints Tylluan's own measured score plus published third-party numbers,
/// with an explicit metric-mismatch warning instead of a false apples-to-
/// apples table.
///
/// M28-P0 finding (2026-07-13): the previous version of this function put
/// every number in one "R@5" column as if they were the same metric. They
/// are not, verified against primary/independent sources:
///   - Hindsight's 91.4% is END-TO-END QA ACCURACY (LLM judge, e.g. Gemini
///     3 Pro) -- not retrieval Recall@5 at all.
///   - MemPalace's 96.6% IS "recall_any@5" (pure retrieval, same metric
///     family as Tylluan's own number) -- but independent testers report
///     only 82.6% end-to-end QA accuracy running the same pipeline, and
///     multiple sources explicitly flag the 96.6% figure as "incomparable
///     to anything on the [QA-accuracy] leaderboard."
///   - Mem0's published number is contested: their own blog claims 94.4,
///     an independent measurement reports 49.0% for the same system.
///
/// None of these are verified by us against our own harness/dataset subset,
/// so this prints them as unverified third-party claims with sources, never
/// as a ranked comparison.
pub fn print_comparison(report: &BenchmarkReport) {
    println!("  📊 Tylluan retrieval Recall@5 (LongMemEval-S, own measurement):");
    println!("     {:.1}% -- reproducible via `cargo run -p tylluan-evals -- --suite longmemeval`", report.recall_at_5);
    println!();
    println!("  ⚠️  Third-party numbers below are self-reported by each vendor, NOT");
    println!("      independently reproduced against the same dataset/harness, and use");
    println!("      DIFFERENT metrics that are not directly comparable to the R@5 above:");
    println!("  ┌─────────────────────┬──────────┬────────────────────────────────────┐");
    println!("  │ System              │ Claimed  │ Metric actually being reported       │");
    println!("  ├─────────────────────┼──────────┼────────────────────────────────────┤");
    println!("  │ MemPalace           │  96.6%   │ recall_any@5 (retrieval-only; indep. │");
    println!("  │                     │          │ testers report 82.6% QA accuracy)    │");
    println!("  │ Hindsight           │  91.4%   │ end-to-end QA accuracy (LLM judge)   │");
    println!("  │ Mem0                │ 94.4/49.0│ contested -- vendor vs indep. differ │");
    println!("  │ Zep + Graphiti      │  71.2%   │ unverified, source not re-checked    │");
    println!("  └─────────────────────┴──────────┴────────────────────────────────────┘");
    println!("  → See benchmarks/COMPARISON.md for sources and full caveats.");

    if report.suite_name.contains("Jina") {
        println!("  ✓ Mode: BGE-M3 + Jina Reranker (best config)");
    } else {
        println!("  → Tip: add --reranker flag for BGE-M3 + Jina (typically +3-5pp)");
    }
    println!();
}
