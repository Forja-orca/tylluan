use std::time::Instant;

use tylluan_kernel::memory::silva::SilvaDB;
use tylluan_kernel::router::embeddings::EmbeddingEngine;

use crate::corpus::{SyntheticCorpus, TestQuery};
use crate::metrics::{self, QueryResult, BenchmarkReport, ContradictionAccuracy};

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
        let retrieved = db.search_hybrid(&query, query_embedding.as_deref(), 10)
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
    let retrieved = db.search_hybrid(&query.query, query_embedding.as_deref(), 10)
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

        let results = db.search_hybrid(&query, query_embedding.as_deref(), 10)
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
