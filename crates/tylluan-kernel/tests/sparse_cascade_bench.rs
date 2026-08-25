//! Cascade impact benchmark (T210 follow-up, task #4 verification): measures
//! the REAL effect of the two-stage recall cascade on a synthetic-but-shaped
//! corpus before anyone flips the production flags.
//!
//! Answers three questions with numbers, not assumptions:
//!   1. Gate pass rate — how often do independent lexical signals agree enough
//!      to skip the dense embed on natural (paraphrase-heavy) traffic?
//!   2. Latency — what does stage-1 actually save per query vs always-dense?
//!   3. Quality overlap@k — how much do stage-1 results diverge from the full
//!      4-source fusion when the gate passes?
//!
//! Uses REAL local models: BGE-M3 sparse and dense share the same HF repo,
//! already in ~/.fastembed_cache (verified before writing this). Isolated from
//! CI on purpose: #[ignored], following the T289 spike pattern (dbbf910).
//!
//! Run: cargo test --test sparse_cascade_bench -- --ignored --nocapture

use std::sync::Arc;
use std::time::Instant;
use tylluan_kernel::memory::silva::SilvaDB;
use tylluan_kernel::router::embeddings::{EmbeddingEngine, SparseEngine};

const CLUSTERS: &[&str] = &[
    "postgres",
    "kubernetes",
    "rust-borrow",
    "oauth",
    "vector-search",
    "git-rebase",
    "docker-cache",
    "tls-certs",
];

static DENSE_SEED: std::sync::OnceLock<Arc<EmbeddingEngine>> = std::sync::OnceLock::new();

fn db_dense_for_seed() -> Arc<EmbeddingEngine> {
    DENSE_SEED.get().expect("seed dense handle").clone()
}

fn cluster_doc(cluster: &str, variant: usize) -> String {
    match cluster {
        "postgres" => format!("postgres connection pooling pgx pgbouncer variant {variant}: tune max_connections and pool idle timeout for latency"),
        "kubernetes" => format!("kubernetes pod scheduling kubectl variant {variant}: resource requests limits and node affinity rules"),
        "rust-borrow" => format!("rust borrow checker lifetimes variant {variant}: fix E0502 cannot borrow as mutable while borrowed as immutable"),
        "oauth" => format!("oauth token refresh flow variant {variant}: PKCE code verifier exchange and refresh token rotation storage"),
        "vector-search" => format!("vector similarity search HNSW variant {variant}: ef_construction M parameters recall versus latency tradeoff"),
        "git-rebase" => format!("git rebase interactive workflow variant {variant}: squash commits resolve conflicts onto upstream main"),
        "docker-cache" => format!("docker layer caching buildkit variant {variant}: invalidate cache mount target for dependency reinstall speedup"),
        "tls-certs" => format!("tls certificate renewal variant {variant}: acme challenge dns-01 wildcard cert rotate nginx reload"),
        _ => unreachable!(),
    }
}

fn noise_doc(i: usize) -> String {
    let topics = [
        "sourdough starter hydration ratio weekend bake",
        "alpine hiking trail permits season pass",
        "vinyl record cleaning brush antistatic routine",
        "houseplant monstera repotting soil mix chunky perlite",
        "mechanical keyboard lube switch film mod",
        "aquarium shrimp tank parameters gh kh tds",
    ];
    format!("{} entry {i}", topics[i % topics.len()])
}

#[test]
#[ignore]
fn bench_cascade_gate_rate_latency_and_quality_overlap() {
    // ── Setup: real engines from local cache ────────────────────────────
    let t0 = Instant::now();
    let sparse_engine = SparseEngine::try_new(&tylluan_kernel::config::InferenceDevice::Cpu)
        .expect("SparseEngine init failed — is BAAI/bge-m3 in ~/.fastembed_cache?");
    let dense_engine =
        EmbeddingEngine::load_with_device("bge-m3", &tylluan_kernel::config::InferenceDevice::Cpu)
            .expect("dense BGE-M3 load failed");
    let dense_arc = Arc::new(dense_engine);
    println!(
        "[setup] engines loaded in {:.1}s",
        t0.elapsed().as_secs_f32()
    );

    let rt = tokio::runtime::Runtime::new().unwrap();
    let (db, query_set) = rt.block_on(async {
        let db = SilvaDB::in_memory().await.expect("SilvaDB in_memory failed");

        // Install engines BEFORE seeding — mirrors a production deployment
        // where hybrid_sparse_enabled was on from the start. Writes then get
        // sparse signatures at save_embedding time.
        db.install_sparse_engine(Arc::new(sparse_engine));
        db.install_dense_engine(dense_arc.clone());
        DENSE_SEED
            .set(dense_arc.clone())
            .map_err(|_| ())
            .expect("dense seed handle set once");

        for cluster in CLUSTERS {
            for v in 0..30 {
                db.upsert_node(
                    &format!("{cluster}:{v}"),
                    "note",
                    &cluster_doc(cluster, v),
                    "{}",
                )
                .await
                .unwrap();
            }
        }
        for i in 0..60 {
            db.upsert_node(&format!("noise:{i}"), "note", &noise_doc(i), "{}")
                .await
                .unwrap();
        }
        println!("[setup] corpus indexed: {} nodes", db.node_count().await.unwrap());

        // Simulate one Agnostic Reindexer tick (main.rs loop): get_stale_embeddings
        // must flag ALL seeded nodes as missing their sparse signature even though
        // the engines were installed mid-lifecycle — this is exactly the
        // enable-on-existing-DB backfill path the product now guarantees.
        let model_id = "bench-bge-m3";
        let stale = db.get_stale_embeddings(model_id, None).await.unwrap();
        println!("[setup] reindexer would heal {} nodes (dense+sparse)", stale.len());
        assert!(stale.len() >= 300, "stale scan must catch missing sparse signatures");
        for id in &stale {
            if let Ok(Some(node)) = db.get_node(id).await {
                let vec = db_dense_for_seed().embed(&node.content).expect("seed embed failed");
                let _ = db.save_embedding(id, &vec, model_id, None).await;
            }
        }

        // Sparse signatures must exist after the heal tick.
        let sparse_rows: i64 = tokio::task::block_in_place(|| {
            let conn = db.conn_lock();
            let c = conn.blocking_lock();
            c.query_row("SELECT COUNT(*) FROM node_sparse_embeddings", [], |r| r.get(0))
                .unwrap_or(-1)
        });
        println!("[setup] node_sparse_embeddings rows: {sparse_rows}");
        assert_eq!(sparse_rows, 300, "every seeded node must carry a sparse signature");

        // Environment sanity probes — the first run showed ALL sources empty,
        // which contradicts the passing unit tests on the same in_memory path.
        let fts_probe = db.search("postgres", 5, None).await.unwrap_or_default();
        println!(
            "[probe] direct text search 'postgres' -> {} results",
            fts_probe.len()
        );
        let (fts_rows, sample_weight): (i64, Option<f32>) = tokio::task::block_in_place(|| {
            let conn = db.conn_lock();
            let c = conn.blocking_lock();
            let fts: i64 = c
                .query_row("SELECT COUNT(*) FROM nodes_fts", [], |r| r.get(0))
                .unwrap_or(-1);
            let w: Option<f32> = c
                .query_row("SELECT weight FROM nodes LIMIT 1", [], |r| r.get(0))
                .ok();
            (fts, w)
        });
        println!(
            "[probe] nodes_fts rows={fts_rows}, sample node weight={sample_weight:?}"
        );

        // Query set designed against the kernel's strict-AND FTS matcher
        // (sanitize_fts_query joins ALL terms with AND):
        //   - on-cluster: domain terms that literally co-occur in the docs
        //   - partial: 2 of 3 terms co-occur (frontier of the gate)
        //   - off-topic: zero overlap -> must always fall to stage 2
        let mut queries: Vec<String> = Vec::new();
        let on_cluster: &[&str] = &[
            "postgres pgbouncer pooling idle timeout",
            "postgres max_connections latency",
            "kubernetes pod scheduling kubectl",
            "kubernetes node affinity resource limits",
            "rust borrow checker E0502 mutable",
            "rust lifetimes cannot borrow immutable",
            "oauth PKCE refresh token rotation",
            "oauth code verifier exchange flow",
            "vector HNSW recall latency ef_construction",
            "vector similarity search parameters tradeoff",
            "git rebase squash conflicts upstream",
            "git interactive rebase onto main",
            "docker buildkit layer cache invalidate",
            "docker mount dependency reinstall speedup",
            "tls acme dns-01 wildcard certificate",
            "tls certificate renewal nginx reload",
        ];
        for q in on_cluster {
            queries.push(q.to_string());
        }
        for q in [
            "postgres kubernetes together",           // partial: both words exist, never co-occur
            "rust docker debugging",                  // partial
            "best pizza dough fermentation schedule", // off-topic
            "flight upgrade bidding strategies",
            "learn watercolor wet on wet technique",
            "repair cracked phone screen at home",
        ] {
            queries.push(q.to_string());
        }
        (db, queries)
    });

    // Cold single-embed cost (the thing the gate saves): measured BEFORE any
    // path touches these strings, so no LRU can mask it.
    let mut cold_embed_ms_samples: Vec<f64> = Vec::new();
    for q in ["cold-probe alpha one", "cold-probe beta two", "cold-probe gamma three"] {
        let t = Instant::now();
        let _ = dense_arc.embed(q).expect("cold embed failed");
        cold_embed_ms_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let cold_embed_ms =
        cold_embed_ms_samples.iter().sum::<f64>() / cold_embed_ms_samples.len() as f64;
    println!(
        "[setup] cold single-query dense embed: {:.0}ms (mean of {})",
        cold_embed_ms,
        cold_embed_ms_samples.len()
    );

    // ── Warmup (ONNX graph + FTS5) ──────────────────────────────────────
    let (warm_res, _) = rt
        .block_on(async { db.search_recall_cascade(CLUSTERS[0], 20, None, false).await.unwrap() });
    assert!(!warm_res.is_empty(), "warmup returned nothing");

    // Baseline embed handle = the same shared engine (kept outside SilvaDB so
    // baseline timing never benefits from the cascade's internal LRU).
    let baseline_engine = dense_arc.clone();

    struct Row {
        stage1_hit: bool,
        cascade_ms: f64,
        baseline_ms: f64,
        overlap10: f64,
    }
    let mut rows: Vec<Row> = Vec::new();

    for query in &query_set {
        // Diagnostic probe: what does stage 1 see for this query?
        let (agr, total, fts_only, sparse_only) = rt
            .block_on(async { db.cascade_stage1_stats(query, 20).await.unwrap() });

        // Cascade path (the shipped behavior): may or may not pay the embed.
        let t = Instant::now();
        let (casc_res, casc_emb) = rt
            .block_on(async { db.search_recall_cascade(query, 20, None, false).await.unwrap() });
        let cascade_ms = t.elapsed().as_secs_f64() * 1000.0;
        let stage1_hit = casc_emb.is_none();

        // Always-dense baseline: explicit embed + standard legacy fusion.
        let t2 = Instant::now();
        let emb = baseline_engine.embed(query).expect("baseline embed failed");
        let base_res = rt.block_on(async {
            db.search_hybrid_for_recall(query, Some(&emb), 20, None, false, false)
                .await
                .unwrap()
        });
        let baseline_ms = t2.elapsed().as_secs_f64() * 1000.0;

        println!(
            "[q] {:<42} agr={agr:<3} total={total:<3} fts={fts_only:<3} sps={sparse_only:<3} | casc={} base={} | casc_ms={:.0} base_ms={:.0}",
            query,
            casc_res.len(),
            base_res.len(),
            cascade_ms,
            baseline_ms
        );

        let top10 = |v: &[(
            tylluan_kernel::memory::silva::GraphNode,
            f32,
        )]| -> Vec<String> { v.iter().take(10).map(|(n, _)| n.id.clone()).collect() };
        let a = top10(&casc_res);
        let b = top10(&base_res);
        let inter = a.iter().filter(|id| b.contains(id)).count();
        let overlap10 = inter as f64 / a.len().max(1) as f64;

        rows.push(Row { stage1_hit, cascade_ms, baseline_ms, overlap10 });
    }

    // ── Report ──────────────────────────────────────────────────────────
    let hits: Vec<&Row> = rows.iter().filter(|r| r.stage1_hit).collect();
    let misses: Vec<&Row> = rows.iter().filter(|r| !r.stage1_hit).collect();
    let mean = |v: &[f64]| -> f64 { v.iter().sum::<f64>() / v.len().max(1) as f64 };
    let p95 = |mut v: Vec<f64>| -> f64 {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[((v.len() as f64 * 0.95) as usize).saturating_sub(1).min(v.len() - 1)]
    };

    println!("\n========== CASCADE IMPACT REPORT ==========");
    println!("queries measured: {}", rows.len());
    println!("cold dense embed (what the gate saves): {cold_embed_ms:.0}ms");
    println!(
        "stage-1 hit rate: {}/{} ({:.0}%)",
        hits.len(),
        rows.len(),
        100.0 * hits.len() as f64 / rows.len() as f64
    );
    if !hits.is_empty() {
        let h_casc: Vec<f64> = hits.iter().map(|r| r.cascade_ms).collect();
        let h_base: Vec<f64> = hits.iter().map(|r| r.baseline_ms).collect();
        let h_ov: Vec<f64> = hits.iter().map(|r| r.overlap10).collect();
        println!("--- stage-1 hits (dense embed skipped) ---");
        println!(
            "  cascade latency   mean/p95: {:>6.0}ms / {:>6.0}ms",
            mean(&h_casc),
            p95(h_casc.clone())
        );
        println!(
            "  baseline latency  mean/p95: {:>6.0}ms / {:>6.0}ms  (explicit embed + fusion)",
            mean(&h_base),
            p95(h_base.clone())
        );
        println!("  saved per hit ≈ {:.0}ms", mean(&h_base) - mean(&h_casc));
        println!(
            "  quality overlap@10 vs full fusion: mean {:.2} (min {:.2}, max {:.2})",
            mean(&h_ov),
            h_ov.iter().cloned().fold(f64::MAX, f64::min),
            h_ov.iter().cloned().fold(0.0, f64::max)
        );
    }
    if !misses.is_empty() {
        let m_casc: Vec<f64> = misses.iter().map(|r| r.cascade_ms).collect();
        let m_base: Vec<f64> = misses.iter().map(|r| r.baseline_ms).collect();
        let m_ov: Vec<f64> = misses.iter().map(|r| r.overlap10).collect();
        println!("--- stage-2 falls (gate rejected) ---");
        println!("  count: {}", misses.len());
        println!(
            "  cascade latency   mean/p95: {:>6.0}ms / {:>6.0}ms  (gate probe + embed + full fusion)",
            mean(&m_casc),
            p95(m_casc.clone())
        );
        println!(
            "  baseline latency  mean/p95: {:>6.0}ms / {:>6.0}ms",
            mean(&m_base),
            p95(m_base.clone())
        );
        println!(
            "  overhead vs baseline ≈ +{:.0}ms/query (stage-1 probe cost)",
            mean(&m_casc) - mean(&m_base)
        );
        println!("  overlap@10 vs full fusion: mean {:.2} (sanity: should be ~1.00)", mean(&m_ov));
    }
    println!("===========================================\n");
}
