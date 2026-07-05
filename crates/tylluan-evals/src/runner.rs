use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use tylluan_kernel::memory::silva::SilvaDB;
use tylluan_kernel::router::embeddings::EmbeddingEngine;

use crate::corpus::{SyntheticCorpus, TestQuery};
use crate::metrics::{self, QueryResult, BenchmarkReport, ContradictionAccuracy};

// ── IdleLab report types ──────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize)]
pub struct IdleLabReport {
    pub db_path: String,
    pub oracle_path: String,
    pub oracle_pairs: usize,
    pub experiments_run: usize,
    pub baseline: IdleLabMetrics,
    pub final_metrics: IdleLabMetrics,
    pub best_params: IdleLabParams,
    pub delta_score_pp: f64,
    pub improvement_found: bool,
    pub adr007_verdict: String,
}

#[derive(Debug, serde::Serialize)]
pub struct IdleLabMetrics {
    pub recall_at_1: f64,
    pub recall_at_5: f64,
    pub composite_score: f64,
}

#[derive(Debug, serde::Serialize)]
pub struct IdleLabParams {
    pub candidate_pool_mult: usize,
    pub rerank_window: usize,
    pub semantic_weight: u32,
    pub dedup_cosine: u32,
}

impl IdleLabReport {
    fn empty(db_path: &str, oracle_path: &str) -> Self {
        let zero = IdleLabMetrics { recall_at_1: 0.0, recall_at_5: 0.0, composite_score: 0.0 };
        let def = IdleLabParams { candidate_pool_mult: 20, rerank_window: 50, semantic_weight: 70, dedup_cosine: 92 };
        Self {
            db_path: db_path.to_string(),
            oracle_path: oracle_path.to_string(),
            oracle_pairs: 0,
            experiments_run: 0,
            baseline: zero,
            final_metrics: IdleLabMetrics { recall_at_1: 0.0, recall_at_5: 0.0, composite_score: 0.0 },
            best_params: def,
            delta_score_pp: 0.0,
            improvement_found: false,
            adr007_verdict: "ABORTED — empty oracle".to_string(),
        }
    }
}

pub fn print_idle_lab_report(report: &IdleLabReport) {
    println!();
    println!("═══════════════════════════════════════════════════════");
    println!("  Tylluan-Evals: IdleLab Hill-Climbing — ADR-007");
    println!("═══════════════════════════════════════════════════════");
    println!();
    println!("  DB:           {}", report.db_path);
    println!("  Oracle:       {} pairs  ({})", report.oracle_pairs, report.oracle_path);
    println!("  Experiments:  {}", report.experiments_run);
    println!();
    println!("  ┌──────────────────────────┬──────────┬──────────┬──────────┐");
    println!("  │ Metric                   │ Baseline │  Final   │  Delta   │");
    println!("  ├──────────────────────────┼──────────┼──────────┼──────────┤");
    println!("  │ Recall@1                 │  {:>5.1}%  │  {:>5.1}%  │  {:>+5.1}pp │",
        report.baseline.recall_at_1, report.final_metrics.recall_at_1,
        report.final_metrics.recall_at_1 - report.baseline.recall_at_1);
    println!("  │ Recall@5                 │  {:>5.1}%  │  {:>5.1}%  │  {:>+5.1}pp │",
        report.baseline.recall_at_5, report.final_metrics.recall_at_5,
        report.final_metrics.recall_at_5 - report.baseline.recall_at_5);
    println!("  │ Composite (0.6R1+0.4R5)  │  {:>5.1}%  │  {:>5.1}%  │  {:>+5.1}pp │",
        report.baseline.composite_score, report.final_metrics.composite_score,
        report.delta_score_pp);
    println!("  └──────────────────────────┴──────────┴──────────┴──────────┘");
    println!();
    println!("  Best params found:");
    println!("    candidate_pool_mult = {}", report.best_params.candidate_pool_mult);
    println!("    rerank_window       = {}", report.best_params.rerank_window);
    println!("    semantic_weight     = {}%", report.best_params.semantic_weight);
    println!("    dedup_cosine        = {}%", report.best_params.dedup_cosine);
    println!();
    let icon = if report.improvement_found { "✓" } else { "✗" };
    println!("  {} ADR-007: {}", icon, report.adr007_verdict);
    println!();
    println!("═══════════════════════════════════════════════════════");
    println!();
}

pub async fn run_idle_lab(
    db_path: &str,
    oracle_path: &str,
    experiments: usize,
    engine: Option<&EmbeddingEngine>,
) -> IdleLabReport {
    use std::sync::atomic::Ordering;
    use tylluan_kernel::memory::idle_lab::{
        IdleLab, CANDIDATE_POOL_MULT, RERANK_WINDOW, SEMANTIC_WEIGHT, DEDUP_COSINE,
    };
    use tylluan_kernel::memory::idle_lab_oracle;

    println!("  Opening SilvaDB: {}", db_path);
    let silva = match SilvaDB::open(db_path) {
        Ok(db) => Arc::new(db),
        Err(e) => { eprintln!("  ERROR: cannot open {}: {}", db_path, e); return IdleLabReport::empty(db_path, oracle_path); }
    };
    silva.init().await.expect("init schema");

    let node_count = silva.node_count().await.unwrap_or(0);
    let edge_count = silva.edge_count().await.unwrap_or(0);
    println!("  Nodes: {}  Edges: {}", node_count, edge_count);

    let oracle = idle_lab_oracle::load_oracle(Path::new(oracle_path));
    println!("  Oracle: {} pairs from {}", oracle.len(), oracle_path);

    if oracle.is_empty() {
        eprintln!("  ERROR: oracle is empty — run --generate-oracle --oracle-output {} first", oracle_path);
        return IdleLabReport::empty(db_path, oracle_path);
    }

    // Reset to defaults for a clean baseline
    CANDIDATE_POOL_MULT.store(20, Ordering::SeqCst);
    RERANK_WINDOW.store(50, Ordering::SeqCst);
    SEMANTIC_WEIGHT.store(70, Ordering::SeqCst);
    DEDUP_COSINE.store(92, Ordering::SeqCst);

    println!();
    println!("  [1/3] Baseline (pool_mult=20, rerank_win=50, sw=70, dedup=92)...");
    let (bl_r1, bl_r5) = measure_oracle_recall(&silva, &oracle, engine, 20, 50).await;
    let bl_score = 0.6 * bl_r1 + 0.4 * bl_r5;
    println!("        R@1={:.1}%  R@5={:.1}%  composite={:.1}%",
        bl_r1 * 100.0, bl_r5 * 100.0, bl_score * 100.0);

    // Write oracle to IdleLab output dir so it loads the right oracle
    let lab_dir = Path::new("benchmarks/idle_lab_run");
    std::fs::create_dir_all(lab_dir).ok();
    let lab_oracle = lab_dir.join("idle_lab_oracle.json");
    if let Ok(json) = serde_json::to_string_pretty(&oracle) {
        std::fs::write(&lab_oracle, json).ok();
    }

    println!("  [2/3] Running {} hill-climbing experiments...", experiments);
    let lab = IdleLab::new(silva.clone(), lab_dir);
    lab.run_experiments(engine, None, experiments).await;

    let fin_pool = CANDIDATE_POOL_MULT.load(Ordering::SeqCst);
    let fin_win  = RERANK_WINDOW.load(Ordering::SeqCst);
    let fin_sw   = SEMANTIC_WEIGHT.load(Ordering::SeqCst);
    let fin_dc   = DEDUP_COSINE.load(Ordering::SeqCst);

    println!("  [3/3] Final recall (pool_mult={}, rerank_win={}, sw={}, dedup={})...",
        fin_pool, fin_win, fin_sw, fin_dc);
    let (fin_r1, fin_r5) = measure_oracle_recall(&silva, &oracle, engine, fin_pool, fin_win).await;
    let fin_score = 0.6 * fin_r1 + 0.4 * fin_r5;
    println!("        R@1={:.1}%  R@5={:.1}%  composite={:.1}%",
        fin_r1 * 100.0, fin_r5 * 100.0, fin_score * 100.0);

    let delta_score_pp = (fin_score - bl_score) * 100.0;
    let improvement_found = delta_score_pp >= 5.0;

    IdleLabReport {
        db_path: db_path.to_string(),
        oracle_path: oracle_path.to_string(),
        oracle_pairs: oracle.len(),
        experiments_run: experiments,
        baseline: IdleLabMetrics {
            recall_at_1: bl_r1 * 100.0,
            recall_at_5: bl_r5 * 100.0,
            composite_score: bl_score * 100.0,
        },
        final_metrics: IdleLabMetrics {
            recall_at_1: fin_r1 * 100.0,
            recall_at_5: fin_r5 * 100.0,
            composite_score: fin_score * 100.0,
        },
        best_params: IdleLabParams {
            candidate_pool_mult: fin_pool,
            rerank_window: fin_win,
            semantic_weight: fin_sw,
            dedup_cosine: fin_dc,
        },
        delta_score_pp,
        improvement_found,
        adr007_verdict: if improvement_found {
            format!("ÚTIL — delta={:.1}pp ≥ 5pp. Adopt hill-climbing in NightConsolidation.", delta_score_pp)
        } else {
            format!("INNECESARIO — delta={:.1}pp < 5pp. Disable Idle Lab to save CPU.", delta_score_pp)
        },
    }
}

async fn measure_oracle_recall(
    silva: &Arc<SilvaDB>,
    oracle: &[tylluan_kernel::memory::idle_lab_oracle::OraclePair],
    engine: Option<&EmbeddingEngine>,
    pool_mult: usize,
    _rerank_window: usize,
) -> (f64, f64) {
    const EVAL_LIMIT: usize = 5;
    let n = oracle.len();
    if n == 0 { return (0.0, 0.0); }
    let mut hit1 = 0usize;
    let mut hit5 = 0usize;

    for pair in oracle {
        let pool_size = (EVAL_LIMIT * pool_mult).max(50);
        let embedding = engine.and_then(|e| e.embed(&pair.query).ok());

        let results = match silva.search_hybrid(
            &pair.query,
            embedding.as_deref(),
            pool_size,
            None,
            false,
        ).await {
            Ok(r) => r,
            Err(_) => continue,
        };

        let expected = pair.expected_id.to_lowercase();
        let top5: Vec<_> = results.iter().take(EVAL_LIMIT).collect();

        if top5.first().map_or(false, |(node, _)| node.id.to_lowercase() == expected) {
            hit1 += 1;
        }
        if top5.iter().any(|(node, _)| node.id.to_lowercase() == expected) {
            hit5 += 1;
        }
    }

    (hit1 as f64 / n as f64, hit5 as f64 / n as f64)
}

pub async fn run_real_benchmark(db_path: &str, engine: Option<&EmbeddingEngine>) -> BenchmarkReport {
    println!("  Opening real SilvaDB: {}", db_path);
    let db = SilvaDB::open(db_path).expect("Failed to open real SilvaDB");
    db.init().await.expect("Failed to init schema");

    // Count nodes and edges
    let node_count = db.node_count().await.unwrap_or(0) as usize;
    let edge_count = db.edge_count().await.unwrap_or(0) as usize;
    println!("  Found {} nodes, {} edges", node_count, edge_count);

    // Sample 20 nodes as query targets by type (routing_anchors dominate weight ranking)
    let sample = match db.get_nodes_by_types(
        &["episode", "document", "agent_memory", "code_entity"],
        20,
    ).await {
        Ok(nodes) => nodes,
        Err(e) => {
            println!("  ERROR loading nodes by type: {}", e);
            return metrics::compute_report("Real SilvaDB", node_count, edge_count, vec![], engine.is_some(), false, None);
        }
    };
    let targets: Vec<_> = sample.iter()
        .filter(|n| n.content.len() >= 20)
        .collect();

    if targets.is_empty() {
        println!("  No queryable nodes found — aborting");
        return metrics::compute_report("Real SilvaDB", node_count, edge_count, vec![], engine.is_some(), false, None);
    }

    println!("  Sampling {} nodes as query targets", targets.len());
    println!("  Running benchmark queries...\n");

    let mut results = Vec::new();
    for (i, node) in targets.iter().enumerate() {
        // Use first 80 chars of content as query, relevant = [node.id]
        let query: String = node.content.chars().take(80).collect();
        let query_embedding = engine.and_then(|e| e.embed(&query).ok());

        let start = Instant::now();
        let retrieved = db.search_hybrid(&query, query_embedding.as_deref(), 10, None, false)
            .await
            .unwrap_or_default();
        let elapsed = start.elapsed();

        let paired: Vec<(String, f32)> = retrieved.iter()
            .map(|(n, s)| (n.id.clone(), *s))
            .collect();

        let relevant = vec![node.id.clone()];
        let mut qr = metrics::compute_query_result(&paired, &relevant);
        qr.latency_ms = elapsed.as_secs_f64() * 1000.0;

        let icon = if qr.correct_in_top5 { "+" } else { "x" };
        println!("  [{}]  Q{:02}: {} — {:.1}ms",
            icon, i + 1,
            node.id.chars().take(50).collect::<String>(),
            qr.latency_ms);

        results.push(qr);
    }

    metrics::compute_report(
        "Real SilvaDB",
        node_count,
        edge_count,
        results,
        engine.is_some(),
        false,
        None,
    )
}

pub async fn run_synthetic_benchmark(corpus: &SyntheticCorpus, engine: Option<&EmbeddingEngine>) -> BenchmarkReport {
    println!("  Creating in-memory SilvaDB...");
    let db = SilvaDB::in_memory().await.expect("Failed to create in-memory SilvaDB");

    println!("  Inserting {} nodes...", corpus.nodes.len());
    for node in &corpus.nodes {
        db.upsert_node(&node.id, &node.node_type, &node.content, &node.metadata)
            .await
            .unwrap_or_else(|e| panic!("Failed to insert node {}: {:?}", node.id, e));
    }

    if let Some(ref engine) = engine {
        println!("  Computing embeddings for {} nodes...", corpus.nodes.len());
        for node in &corpus.nodes {
            match engine.embed(&node.content) {
                Ok(emb) => {
                    db.save_embedding(&node.id, &emb, "bge-m3", None)
                        .await
                        .unwrap_or_else(|e| panic!("Failed to save embedding for {}: {:?}", node.id, e));
                }
                Err(e) => {
                    println!("  ⚠ Embedding failed for {}: {:?}", node.id, e);
                }
            }
        }
        println!("  ✓ Embeddings computed and saved");
    }

    println!("  Inserting {} edges...", corpus.nodes.len());
    for edge in &corpus.edges {
        db.add_edge(&edge.source, &edge.target, &edge.edge_type, 1.0, "{}")
            .await
            .unwrap_or_else(|e| panic!("Failed to insert edge {:?}: {:?}", edge, e));
    }

    println!("  Running {} benchmark queries...\n", corpus.queries.len());
    let mut results = Vec::with_capacity(corpus.queries.len());

    for (i, query) in corpus.queries.iter().enumerate() {
        let result = run_single_query(&db, engine, query).await;
        let icon = if result.correct_in_top5 { "✓" } else { "✗" };
        println!("  {}  Q{}: {} — Recall@5={}", icon, i + 1, query.description,
            if result.correct_in_top5 { "YES" } else { "no" });
        results.push(result);
    }

    let contradiction_accuracy = if engine.is_some() {
        Some(test_contradictions(&db, engine, corpus).await)
    } else {
        None
    };

    let report = metrics::compute_report(
        "Synthetic Corpus",
        corpus.nodes.len(),
        corpus.edges.len(),
        results,
        engine.is_some(),
        false,
        contradiction_accuracy,
    );

    report
}

async fn run_single_query(
    db: &SilvaDB,
    engine: Option<&EmbeddingEngine>,
    query: &TestQuery,
) -> QueryResult {
    let query_embedding = engine.and_then(|e| e.embed(&query.query).ok());

    let start = Instant::now();
    let retrieved = db.search_hybrid(&query.query, query_embedding.as_deref(), 10, None, false)
        .await
        .unwrap_or_default();
    let elapsed = start.elapsed();

    let paired: Vec<(String, f32)> = retrieved.iter()
        .map(|(node, score)| (node.id.clone(), *score))
        .collect();

    let mut qr = metrics::compute_query_result(&paired, &query.relevant_ids);
    qr.latency_ms = elapsed.as_secs_f64() * 1000.0;
    qr
}

async fn test_contradictions(
    db: &SilvaDB,
    engine: Option<&EmbeddingEngine>,
    corpus: &SyntheticCorpus,
) -> ContradictionAccuracy {
    let mut correct_outranks = 0;
    let mut both_in_top5 = 0;

    for [correct_id, wrong_id] in &corpus.contradiction_pairs {
        let correct_node = db.get_node(correct_id).await.ok().flatten();
        let wrong_node = db.get_node(wrong_id).await.ok().flatten();

        if correct_node.is_none() || wrong_node.is_none() {
            continue;
        }

        let query = correct_node.as_ref().unwrap().content.chars().take(60).collect::<String>();
        let query_embedding = engine.and_then(|e| e.embed(&query).ok());

        let results = db.search_hybrid(&query, query_embedding.as_deref(), 10, None, false)
            .await
            .unwrap_or_default();

        let correct_rank = results.iter().position(|(n, _)| n.id == *correct_id);
        let wrong_rank = results.iter().position(|(n, _)| n.id == *wrong_id);

        if let (Some(cr), Some(wr)) = (correct_rank, wrong_rank) {
            if cr < wr {
                correct_outranks += 1;
            }
            if cr < 5 && wr < 5 {
                both_in_top5 += 1;
            }
        }
    }

    ContradictionAccuracy {
        total: corpus.contradiction_pairs.len(),
        correct_version_outranks_wrong: correct_outranks,
        both_in_top5,
    }
}

pub async fn run_auto_link(db_path: &str) {
    use tylluan_kernel::memory::auto_link::AutoLinker;
    use std::sync::Arc;

    println!("  Opening SilvaDB: {}", db_path);
    let db = match SilvaDB::open(db_path) {
        Ok(d) => { Arc::new(d) }
        Err(e) => { eprintln!("  ERROR: Failed to open {}: {}", db_path, e); return; }
    };
    if let Err(e) = db.init().await {
        eprintln!("  ERROR: Failed to init schema: {}", e);
        return;
    }

    let node_count = db.node_count().await.unwrap_or(0);
    let edge_count = db.edge_count().await.unwrap_or(0);
    println!("  Found {} nodes, {} edges before linking", node_count, edge_count);

    let linker = AutoLinker::new(db.clone());
    let report = linker.run(None).await;

    let edges_after = db.edge_count().await.unwrap_or(0);
    println!();
    println!("  =============================================");
    println!("  AutoLink CERO-LLM Report");
    println!("  =============================================");
    println!("  Nodes:               {}", report.nodes_total);
    println!("  Edges before:        {}", report.edges_before);
    println!("  Edges after:         {}", edges_after as usize);
    println!("  Edges created:       {}", edges_after as usize - report.edges_before);
    println!("  File ref links:      {}", report.file_ref_edges);
    println!("  Tool ref links:      {}", report.tool_ref_edges);
    println!("  Topic links:         {}", report.topic_edges);
    println!("  Orphan links:        {}", report.orphan_edges);
    println!();
}

/// Generate an IdleLab oracle file by sampling 20 real nodes from SilvaDB.
/// Each oracle pair: query = first 80 chars of node content, expected_id = node.id.
/// Writes JSON to `output_path`.
pub async fn generate_oracle(db_path: &str, output_path: &Path) {
    println!("  Opening SilvaDB: {}", db_path);
    let db = SilvaDB::open(db_path).expect("Failed to open SilvaDB");
    db.init().await.expect("Failed to init schema");

    let nodes = match db.get_nodes_by_types(
        &["episode", "document", "agent_memory", "code_entity", "experience", "concept"],
        20,
    ).await {
        Ok(nodes) => nodes,
        Err(e) => {
            println!("  ERROR: failed to sample nodes: {}", e);
            return;
        }
    };

    println!("  Sampled {} nodes", nodes.len());

    let oracle: Vec<tylluan_kernel::memory::idle_lab_oracle::OraclePair> = nodes.iter()
        .filter(|n| n.content.len() >= 20)
        .map(|n| tylluan_kernel::memory::idle_lab_oracle::OraclePair {
            query: n.content.chars().take(80).collect(),
            expected_id: n.id.clone(),
        })
        .collect();

    let json = serde_json::to_string_pretty(&oracle).expect("serialize oracle");
    if let Err(e) = std::fs::write(output_path, &json) {
        println!("  ERROR writing oracle to {}: {}", output_path.display(), e);
        return;
    }

    println!("  Wrote {} oracle pairs to {}", oracle.len(), output_path.display());
    println!("  Done.");
}
