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
            .search_hybrid(query, Some(&emb), 10, None, false)
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

// ── v0.10.0 Extended Benchmark ──────────────────────────────────────────────────
// 50 nodes, 40 edges, 10 queries (5 original + 5 multi-hop).
// Runs with skip_graph=false (P3 on) and skip_graph=true (P3 off) to measure
// the quality delta of LightRAG graph traversal.

const EXTRA_NODES: &[(&str, &str, &str, &str)] = &[
    ("entity:llm", "entity", "Large Language Models power the reasoning behind agent tool calls. The kernel does not bundle an LLM — it connects to external MCP clients.", "{\"topic\":\"ai\"}"),
    ("entity:docker", "entity", "Docker containers isolate guild subprocesses from the host system, providing reproducible execution environments.", "{\"topic\":\"infra\"}"),
    ("entity:linux", "entity", "Linux is the primary deployment target. The kernel is tested on x86_64 and aarch64 (RPi4) architectures.", "{\"topic\":\"infra\"}"),
    ("entity:raspberrypi", "entity", "Raspberry Pi 4 with 4GB RAM is the reference portable deployment target. The kernel must run within 2GB idle.", "{\"topic\":\"infra\"}"),
    ("entity:github", "entity", "GitHub hosts the source repository. CI workflows run cargo test, cargo clippy, and integration checks on each PR.", "{\"topic\":\"devops\"}"),
    ("concept:noise_protocol", "concept", "Noise Protocol Framework provides P2P encryption for tylluan-link. Supports XK and NK handshake patterns with snow crate.", "{\"topic\":\"networking\"}"),
    ("concept:gossip_protocol", "concept", "Gossip protocol disseminates peer information across the mesh using push-pull sync with LWW conflict resolution by clock.", "{\"topic\":\"networking\"}"),
    ("concept:dht", "concept", "Kademlia DHT provides decentralized peer discovery without a central bootstrap server. Uses XOR distance routing.", "{\"topic\":\"networking\"}"),
    ("concept:partition_tolerance", "concept", "PartitionableTransport injects faults (drop, latency, partition, error) to test gossip resilience under network degradation.", "{\"topic\":\"testing\"}"),
    ("concept:mcp_streaming", "concept", "HTTP Streamable MCP allows real-time tool responses. The kernel negotiates protocol version automatically.", "{\"topic\":\"protocol\"}"),
    ("lesson:portable_deployment", "lesson", "The kernel must fit in 2GB RAM idle and 3GB under load to run on RPi4. Quantization and mmap reduce the footprint.", "{\"topic\":\"infra\"}"),
    ("lesson:fault_tolerance_first", "lesson", "Fault tolerance validation must precede new features. Network partitions and message loss are expected in rural deployments.", "{\"topic\":\"reliability\"}"),
    ("lesson:deterministic_tests", "lesson", "InMemoryTransport with mpsc channels provides deterministic simulation testing without turmoil. Avoid runtime simulation frameworks.", "{\"topic\":\"testing\"}"),
    ("episodic:session_006", "episodic", "@episode agent=opencode intent='P2 batch embeddings' result='embed_batch + chunked reindex 268 tests' date=2026-07-01", "{\"agent\":\"opencode\"}"),
    ("episodic:session_007", "episodic", "@episode agent=antigravity intent='P3 LightRAG' result='local_query_graph + PPR traversal integrated' date=2026-07-01", "{\"agent\":\"antigravity\"}"),
    ("episodic:session_008", "episodic", "@episode agent=opencode intent='M6-full DST' result='PartitionableTransport 5 modes 61 link tests' date=2026-07-01", "{\"agent\":\"opencode\"}"),
    ("episodic:session_009", "episodic", "@episode agent=qwen intent='v0.10.0 research' result='M14-D protocol research backlog' date=2026-07-01", "{\"agent\":\"qwen\"}"),
    ("episodic:session_010", "episodic", "@episode agent=antigravity intent='T427 gossip DST' result='3 DST tests + concurrent LWW convergence' date=2026-07-01", "{\"agent\":\"antigravity\"}"),
    ("decision:offline_first", "decision", "All critical paths must work without internet access. Embeddings and inference are local via ONNX runtime.", "{\"topic\":\"architecture\"}"),
    ("decision:deterministic_testing", "decision", "Deterministic simulation via InMemoryTransport is preferred over turmoil for testing gossip and DHT behavior.", "{\"topic\":\"testing\"}"),
    ("decision:rpi4_target", "decision", "RPi4 4GB is the minimum deployment target. All dependencies must compile for aarch64-unknown-linux-gnu.", "{\"topic\":\"infra\"}"),
];

const EXTRA_EDGES: &[(&str, &str, &str)] = &[
    ("concept:tylluan", "contains", "concept:noise_protocol"),
    ("concept:tylluan", "contains", "concept:gossip_protocol"),
    ("concept:tylluan", "contains", "concept:dht"),
    ("concept:tylluan", "contains", "concept:partition_tolerance"),
    ("concept:tylluan", "implements", "concept:mcp_streaming"),
    ("concept:tylluan", "deploys_on", "entity:raspberrypi"),
    ("concept:tylluan", "deploys_on", "entity:linux"),
    ("concept:silvadb", "uses", "entity:sqlite"),
    ("concept:silvadb", "uses", "entity:llm"),
    ("concept:silvadb", "provides", "concept:hybrid_search"),
    ("concept:guilds", "uses", "entity:docker"),
    ("concept:noise_protocol", "encrypts", "concept:gossip_protocol"),
    ("concept:gossip_protocol", "uses", "concept:dht"),
    ("concept:gossip_protocol", "tested_by", "concept:partition_tolerance"),
    ("concept:dht", "replaces", "concept:gossip_protocol"),
    ("concept:partition_tolerance", "validates", "lesson:fault_tolerance_first"),
    ("concept:partition_tolerance", "uses", "lesson:deterministic_tests"),
    ("entity:raspberrypi", "constrains", "lesson:portable_deployment"),
    ("entity:raspberrypi", "motivates", "decision:rpi4_target"),
    ("entity:linux", "hosts", "entity:docker"),
    ("entity:docker", "isolates", "concept:guilds"),
    ("entity:github", "hosts", "concept:tylluan"),
    ("entity:fastembed", "enables", "decision:offline_first"),
    ("lesson:portable_deployment", "depends_on", "lesson:no_cloud"),
    ("lesson:deterministic_tests", "requires", "decision:deterministic_testing"),
    ("concept:episodic_memory", "stores", "episodic:session_006"),
    ("concept:episodic_memory", "stores", "episodic:session_007"),
    ("concept:episodic_memory", "stores", "episodic:session_008"),
    ("concept:episodic_memory", "stores", "episodic:session_009"),
    ("concept:episodic_memory", "stores", "episodic:session_010"),
    ("lesson:tool_design", "governs", "decision:tools_count"),
    ("lesson:storage_choice", "motivates", "decision:storage"),
    ("lesson:port_choice", "secures", "entity:mcp"),
    ("concept:hybrid_search", "uses", "concept:silvadb"),
    ("concept:hybrid_search", "ranks_by", "lesson:bge_dims"),
    ("concept:guilds", "implemented_in", "entity:python"),
    ("entity:rust", "implements", "concept:tylluan"),
    ("entity:python", "implements", "concept:guilds"),
    ("concept:silvadb", "stores", "decision:storage"),
    ("concept:tylluan", "licensed_under", "lesson:license"),
];



const ALL_QUERIES: &[(&str, &[&str])] = &[
    ("how many sovereign tools does tylluan provide", &["decision:tools_count", "lesson:tool_design"]),
    ("what database engine does silvadb use", &["entity:sqlite", "lesson:storage_choice", "decision:storage", "concept:silvadb"]),
    ("what port does the kernel listen on", &["lesson:port_choice"]),
    ("what is episodic memory", &["concept:episodic_memory"]),
    ("how does hybrid search work in tylluan", &["concept:hybrid_search", "concept:silvadb"]),
    ("what networking protocols does tylluan mesh use", &["concept:noise_protocol", "concept:gossip_protocol", "concept:dht", "concept:partition_tolerance"]),
    ("how is the mesh tested for reliability", &["concept:partition_tolerance", "lesson:fault_tolerance_first", "lesson:deterministic_tests", "decision:deterministic_testing"]),
    ("what hardware does tylluan target", &["entity:raspberrypi", "entity:linux", "lesson:portable_deployment", "decision:rpi4_target"]),
    ("how does p2p encryption work", &["concept:noise_protocol", "concept:gossip_protocol"]),
    ("what infra dependencies does the project have", &["entity:docker", "entity:github", "entity:linux", "entity:sqlite"]),
];

fn compute_mrr(results: &[(String, f32)], relevant: &[&str]) -> f64 {
    for (rank, (id, _)) in results.iter().enumerate() {
        if relevant.contains(&id.as_str()) {
            return 1.0 / (rank as f64 + 1.0);
        }
    }
    0.0
}

fn compute_recall(results: &[(String, f32)], relevant: &[&str], k: usize) -> f64 {
    let top_k: Vec<&str> = results.iter().take(k).map(|(n, _)| n.as_str()).collect();
    let hits = relevant.iter().filter(|r| top_k.contains(r)).count();
    hits as f64 / relevant.len() as f64
}

async fn seed_extended_dataset(db: &SilvaDB) {
    // Insert all 50 nodes (29 original + 21 extra)
    for (id, node_type, content, metadata) in NODES {
        db.upsert_node(id, node_type, content, metadata).await.unwrap();
        let emb = generate_embedding(content, 12);
        db.save_embedding(id, &emb, "test-fake", None).await.unwrap();
    }
    for (id, node_type, content, metadata) in EXTRA_NODES {
        db.upsert_node(id, node_type, content, metadata).await.unwrap();
        let emb = generate_embedding(content, 12);
        db.save_embedding(id, &emb, "test-fake", None).await.unwrap();
    }
    // Insert 40 edges
    for (subject, predicate, object) in EXTRA_EDGES {
        let _ = db.add_edge(subject, object, predicate, 1.0, "").await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn benchmark_v0_10_0_quality_delta() {
    let db = SilvaDB::in_memory().await.expect("Failed to create in-memory SilvaDB");
    seed_extended_dataset(&db).await;

    // ── Run with graph traversal (P3 enabled) ──
    let mut total_recall5 = 0.0f64;
    let mut total_recall10 = 0.0f64;
    let mut total_mrr = 0.0f64;
    let mut all_lat_ms: Vec<f64> = Vec::new();

    for (_i, (query, relevant_ids)) in ALL_QUERIES.iter().enumerate() {
        let emb = generate_embedding(query, 12);
        let start = Instant::now();
        let retrieved = db.search_hybrid(query, Some(&emb), 10, None, false)
            .await
            .unwrap_or_default();
        let elapsed = start.elapsed();
        all_lat_ms.push(elapsed.as_secs_f64() * 1000.0);
        let results: Vec<(String, f32)> = retrieved.iter().map(|(n, s)| (n.id.clone(), *s)).collect();
        let r5 = compute_recall(&results, relevant_ids, 5);
        let r10 = compute_recall(&results, relevant_ids, 10);
        let mrr = compute_mrr(&results, relevant_ids);
        total_recall5 += r5;
        total_recall10 += r10;
        total_mrr += mrr;
    }

    let n = ALL_QUERIES.len() as f64;
    let recall5_graph = (total_recall5 / n) * 100.0;
    let recall10_graph = (total_recall10 / n) * 100.0;
    let mrr_graph = (total_mrr / n) * 100.0;

    all_lat_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = all_lat_ms[((50.0 / 100.0) * (all_lat_ms.len() - 1) as f64).round() as usize];
    let p95 = all_lat_ms[((95.0 / 100.0) * (all_lat_ms.len() - 1) as f64).round() as usize];
    let p99 = all_lat_ms[((99.0 / 100.0) * (all_lat_ms.len() - 1) as f64).round() as usize];

    // ── Run without graph traversal (skip_graph=true) ──
    // We need a fresh DB to avoid cross-contamination from previous run's PPR state
    let db_off = SilvaDB::in_memory().await.expect("Failed to create in-memory SilvaDB");
    seed_extended_dataset(&db_off).await;

    let mut total_recall5_off = 0.0f64;
    let mut total_recall10_off = 0.0f64;
    let mut total_mrr_off = 0.0f64;
    let mut all_lat_ms_off: Vec<f64> = Vec::new();

    for (_i, (query, relevant_ids)) in ALL_QUERIES.iter().enumerate() {
        let emb = generate_embedding(query, 12);
        let start = Instant::now();
        let retrieved = db_off.search_hybrid(query, Some(&emb), 10, None, true)
            .await
            .unwrap_or_default();
        let elapsed = start.elapsed();
        all_lat_ms_off.push(elapsed.as_secs_f64() * 1000.0);

        let results: Vec<(String, f32)> = retrieved.iter().map(|(n, s)| (n.id.clone(), *s)).collect();
        let r5 = compute_recall(&results, relevant_ids, 5);
        let r10 = compute_recall(&results, relevant_ids, 10);
        let mrr = compute_mrr(&results, relevant_ids);
        total_recall5_off += r5;
        total_recall10_off += r10;
        total_mrr_off += mrr;
    }

    let recall5_off = (total_recall5_off / n) * 100.0;
    let recall10_off = (total_recall10_off / n) * 100.0;
    let mrr_off = (total_mrr_off / n) * 100.0;

    all_lat_ms_off.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50_off = all_lat_ms_off[((50.0 / 100.0) * (all_lat_ms_off.len() - 1) as f64).round() as usize];
    let p95_off = all_lat_ms_off[((95.0 / 100.0) * (all_lat_ms_off.len() - 1) as f64).round() as usize];
    let p99_off = all_lat_ms_off[((99.0 / 100.0) * (all_lat_ms_off.len() - 1) as f64).round() as usize];

    let delta_recall5 = recall5_graph - recall5_off;
    let delta_recall10 = recall10_graph - recall10_off;
    let delta_mrr = mrr_graph - mrr_off;

    let report = serde_json::json!({
        "version": "v0.10.0-quality-delta",
        "date": "2026-07-01",
        "dataset": {
            "num_nodes": NODES.len() + EXTRA_NODES.len(),
            "num_edges": EXTRA_EDGES.len(),
            "num_queries": ALL_QUERIES.len(),
            "embedding_dims": 12,
            "embedding_source": "deterministic-fake",
        },
        "graph_enabled": {
            "recall_at_5": (recall5_graph * 100.0).round() / 100.0,
            "recall_at_10": (recall10_graph * 100.0).round() / 100.0,
            "mrr": (mrr_graph * 100.0).round() / 100.0,
            "latency_p50_ms": (p50 * 100.0).round() / 100.0,
            "latency_p95_ms": (p95 * 100.0).round() / 100.0,
            "latency_p99_ms": (p99 * 100.0).round() / 100.0,
        },
        "graph_disabled": {
            "recall_at_5": (recall5_off * 100.0).round() / 100.0,
            "recall_at_10": (recall10_off * 100.0).round() / 100.0,
            "mrr": (mrr_off * 100.0).round() / 100.0,
            "latency_p50_ms": (p50_off * 100.0).round() / 100.0,
            "latency_p95_ms": (p95_off * 100.0).round() / 100.0,
            "latency_p99_ms": (p99_off * 100.0).round() / 100.0,
        },
        "delta_graph_minus_baseline": {
            "recall_at_5": (delta_recall5 * 100.0).round() / 100.0,
            "recall_at_10": (delta_recall10 * 100.0).round() / 100.0,
            "mrr": (delta_mrr * 100.0).round() / 100.0,
        },
        "notes": "P3 LightRAG quality delta. Deterministic fake embeddings. Graph traversal enabled via local_query_graph (PPR + degree centrality). skip_graph=true disables the graph path entirely.",
    });

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().and_then(|p| p.parent()).unwrap_or(manifest_dir);
    let bench_dir = workspace_root.join("benchmarks");
    if !bench_dir.exists() {
        std::fs::create_dir_all(&bench_dir).expect("Failed to create benchmarks dir");
    }
    let json_path = bench_dir.join("benchmark_v0.10.0.json");
    let json_str = serde_json::to_string_pretty(&report).expect("Failed to serialize JSON");
    std::fs::write(&json_path, &json_str).expect("Failed to write benchmark JSON");

    println!("\n  Benchmark saved to: {:?}", json_path);
    println!("  Dataset: {} nodes, {} edges, {} queries",
        NODES.len() + EXTRA_NODES.len(), EXTRA_EDGES.len(), ALL_QUERIES.len());
    println!("  ── Graph ON (P3 enabled) ──");
    println!("  Recall@5:  {:.1}%", recall5_graph);
    println!("  Recall@10: {:.1}%", recall10_graph);
    println!("  MRR:       {:.1}%", mrr_graph);
    println!("  Latency p50/p95/p99: {:.1}/{:.1}/{:.1}ms", p50, p95, p99);
    println!("  ── Graph OFF (skip_graph=true) ──");
    println!("  Recall@5:  {:.1}%", recall5_off);
    println!("  Recall@10: {:.1}%", recall10_off);
    println!("  MRR:       {:.1}%", mrr_off);
    println!("  Latency p50/p95/p99: {:.1}/{:.1}/{:.1}ms", p50_off, p95_off, p99_off);
    println!("  ── Delta ──");
    println!("  ΔRecall@5:  {:+.1}%", delta_recall5);
    println!("  ΔRecall@10: {:+.1}%", delta_recall10);
    println!("  ΔMRR:       {:+.1}%", delta_mrr);

    assert!(
        recall5_graph > 0.0 || recall5_off > 0.0,
        "At least one benchmark variant must achieve Recall@5 > 0%"
    );
}
