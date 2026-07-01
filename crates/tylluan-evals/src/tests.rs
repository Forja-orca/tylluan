use std::path::Path;
use std::time::Instant;
use tylluan_kernel::memory::silva::SilvaDB;

const NODES: &[(&str, &str, &str, &str)] = &[
    ("concept:tylluan", "concept", "TylluanNexus is an open-source sovereign MCP kernel written in Rust. It provides 5 sovereign tools to AI agents via the Model Context Protocol.", "{\"topic\":\"tylluan\"}"),
    ("concept:silvadb", "concept", "SilvaDB is the vector-graph memory engine of TylluanNexus. It stores nodes as rows in SQLite with BGE-M3 embeddings and edges as triple-based relationships.", "{\"topic\":\"silvadb\"}"),
    ("concept:guilds", "concept", "The Guild system routes agent intents to specialized Python FastMCP subprocesses via semantic matching. Each guild is an independent fastmcp server.", "{\"topic\":\"guilds\"}"),
    ("concept:hybrid_search", "concept", "tylluan_recall uses hybrid search combining vector similarity with text search via Reciprocal Rank Fusion with k=60.", "{\"topic\":\"search\"}"),
    ("concept:episodic_memory", "concept", "Episodic memory stores agent actions chronologically with @episode tags enabling temporal recall across sessions.", "{\"topic\":\"memory\"}"),
    ("entity:rust", "entity", "Rust is a systems programming language focused on safety and performance, used for the Tylluan kernel.", "{\"topic\":\"tech\"}"),
    ("entity:sqlite", "entity", "SQLite is an embedded relational database engine used by SilvaDB for persistent storage of nodes and edges.", "{\"topic\":\"tech\"}"),
    ("entity:fastembed", "entity", "Fastembed is a Rust ONNX runtime for generating BGE-M3 embeddings locally without cloud dependencies.", "{\"topic\":\"tech\"}"),
    ("entity:mcp", "entity", "Model Context Protocol is a standardized protocol for AI agent tool communication, used by Tylluan as its transport layer.", "{\"topic\":\"protocol\"}"),
    ("entity:python", "entity", "Python is used for implementing guild subprocesses that run as independent FastMCP servers.", "{\"topic\":\"tech\"}"),
    ("lesson:tool_design", "lesson", "Five sovereign tools is the correct count. Extra tools must be routed through tylluan_do to maintain client compatibility.", "{\"topic\":\"protocol\"}"),
    ("lesson:storage_choice", "lesson", "SQLite is the right storage backend for a local-first memory system. PostgreSQL adds latency and cloud dependency.", "{\"topic\":\"architecture\"}"),
    ("lesson:port_choice", "lesson", "Port 3030 bound to 127.0.0.1 only is the correct configuration. No external access to the kernel.", "{\"topic\":\"architecture\"}"),
    ("lesson:bge_dims", "lesson", "BGE-M3 produces 1024-dimensional embeddings, not 2048. The lower dimension keeps memory usage manageable on RPi4.", "{\"topic\":\"embeddings\"}"),
    ("lesson:license", "lesson", "AGPL v3 protects the project from proprietary forking while allowing free use and modification.", "{\"topic\":\"legal\"}"),
    ("episodic:session_001", "episodic", "@episode agent=opencode intent='implement P0 benchmark' result='created baseline test' date=2026-07-01", "{\"agent\":\"opencode\"}"),
    ("episodic:session_002", "episodic", "@episode agent=qwen intent='research LinearRAG' result='validated candidate 2 consensus' date=2026-07-01", "{\"agent\":\"qwen\"}"),
    ("episodic:session_003", "episodic", "@episode agent=antigravity intent='test MCP SSE' result='found streamable HTTP issue with Qwen' date=2026-07-01", "{\"agent\":\"antigravity\"}"),
    ("episodic:session_004", "episodic", "@episode agent=claude-code intent='verify episodic bug' result='false positive - search_hybrid does not filter episodic' date=2026-07-01", "{\"agent\":\"claude-code\"}"),
    ("episodic:session_005", "episodic", "@episode agent=jose intent='triage v0.9.0 lanes' result='P0-P6 ordered' date=2026-07-01", "{\"agent\":\"jose\"}"),
    ("decision:tools_count", "decision", "MCP clients see exactly 5 sovereign tools. all_tools() in server.rs MUST filter to these 5 and nothing else.", "{\"topic\":\"protocol\"}"),
    ("decision:storage", "decision", "All persistent data is stored in local SQLite databases. No PostgreSQL or cloud databases.", "{\"topic\":\"architecture\"}"),
    ("decision:local_first", "decision", "The system uses local-first architecture with no cloud dependencies in the critical path.", "{\"topic\":\"architecture\"}"),
];

const QUERIES: &[(&str, &[&str])] = &[
    ("how many sovereign tools does tylluan provide", &["decision:tools_count", "lesson:tool_design"]),
    ("what database engine does silvadb use", &["entity:sqlite", "lesson:storage_choice", "decision:storage", "concept:silvadb"]),
    ("what port does the kernel listen on", &["lesson:port_choice"]),
    ("what is episodic memory", &["concept:episodic_memory"]),
    ("how does hybrid search work in tylluan", &["concept:hybrid_search", "concept:silvadb"]),
];

fn generate_embedding(text: &str, dims: usize) -> Vec<f32> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut result = Vec::with_capacity(dims);
    for i in 0..dims {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        (i * 7).hash(&mut hasher);
        let h = hasher.finish();
        result.push((h as f32) / (u64::MAX as f32));
    }
    result
}

#[tokio::test(flavor = "multi_thread")]
async fn baseline_v090_benchmark() {
    let db = SilvaDB::in_memory().await.expect("Failed to create in-memory SilvaDB");

    for (id, node_type, content, metadata) in NODES {
        db.upsert_node(id, node_type, content, metadata)
            .await
            .unwrap_or_else(|e| panic!("Failed to insert node {}: {:?}", id, e));
        let emb = generate_embedding(content, 12);
        db.save_embedding(id, &emb, "test-fake", None)
            .await
            .unwrap_or_else(|e| panic!("Failed to save embedding for {}: {:?}", id, e));
    }

    let mut latencies_ms: Vec<f64> = Vec::new();
    let mut correct_in_top5_count = 0;
    let mut total_precision_at_5 = 0.0f64;

    for (_i, (query, relevant_ids)) in QUERIES.iter().enumerate() {
        let emb = generate_embedding(query, 12);
        let start = Instant::now();
        let retrieved = db
            .search_hybrid(query, Some(&emb), 10)
            .await
            .unwrap_or_default();
        let elapsed = start.elapsed();
        latencies_ms.push(elapsed.as_secs_f64() * 1000.0);

        let top5_ids: Vec<&str> = retrieved.iter().take(5).map(|(n, _)| n.id.as_str()).collect();
        let hits_in_top5 = top5_ids.iter().filter(|id| relevant_ids.contains(id)).count();
        if hits_in_top5 > 0 {
            correct_in_top5_count += 1;
        }
        total_precision_at_5 += hits_in_top5 as f64 / 5.0;
    }

    let n = QUERIES.len() as f64;
    let recall_at_5 = (correct_in_top5_count as f64 / n) * 100.0;
    let precision_at_5 = (total_precision_at_5 / n) * 100.0;

    latencies_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let latency_p50 = if latencies_ms.len() >= 2 {
        let idx = ((50.0 / 100.0) * (latencies_ms.len() - 1) as f64).round() as usize;
        latencies_ms[idx]
    } else {
        latencies_ms.first().copied().unwrap_or(0.0)
    };
    let latency_p95 = if latencies_ms.len() >= 2 {
        let idx = ((95.0 / 100.0) * (latencies_ms.len() - 1) as f64).round() as usize;
        latencies_ms[idx]
    } else {
        latencies_ms.last().copied().unwrap_or(0.0)
    };

    let baseline = serde_json::json!({
        "version": "v0.9.0-baseline",
        "date": "2026-07-01",
        "search_backend": "BM25+FTS5+RRF",
        "num_nodes": NODES.len(),
        "num_queries": QUERIES.len(),
        "recall_at_5": (recall_at_5 * 100.0).round() / 100.0,
        "precision_at_5": (precision_at_5 * 100.0).round() / 100.0,
        "latency_p50_ms": (latency_p50 * 100.0).round() / 100.0,
        "latency_p95_ms": (latency_p95 * 100.0).round() / 100.0,
        "notes": "baseline before LightRAG P3. Uses deterministic fake embeddings for vector path. Real BGE-M3 not available in test.",
        "per_query": QUERIES.iter().zip(latencies_ms.iter()).enumerate().map(|(i, (q, _))| {
            serde_json::json!({
                "query": q.0,
                "latency_ms": latencies_ms[i],
            })
        }).collect::<Vec<_>>(),
    });

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().and_then(|p| p.parent()).unwrap_or(manifest_dir);
    let bench_dir = workspace_root.join("benchmarks");
    if !bench_dir.exists() {
        std::fs::create_dir_all(&bench_dir).expect("Failed to create benchmarks dir");
    }
    let json_path = bench_dir.join("baseline_v0.9.0.json");
    let json_str = serde_json::to_string_pretty(&baseline).expect("Failed to serialize JSON");
    std::fs::write(&json_path, &json_str).expect("Failed to write baseline JSON");

    println!("\n  Baseline saved to: {:?}", json_path);
    println!("  Recall@5: {:.1}%", recall_at_5);
    println!("  Precision@5: {:.1}%", precision_at_5);
    println!("  Latency p50: {:.1}ms", latency_p50);
    println!("  Latency p95: {:.1}ms", latency_p95);

    assert!(
        recall_at_5 > 0.0,
        "Baseline Recall@5 must be > 0% — got {:.1}%",
        recall_at_5
    );
}
