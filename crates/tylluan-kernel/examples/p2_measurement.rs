//! P2 measurement harness (Frente 2, T891) — MEDICIÓN ONLY, no production changes.
//!
//! Reads REAL vectors from `data/silva.db` (read-only) and measures the three
//! code-level P2 items from `docs/roadmap/ROADMAP_O3.md`:
//!
//!   1. HNSW distance: current full-norm cosine per comparison
//!      (`silva/hnsw.rs::EmbPoint::distance`, verbatim) vs `1 - dot` for
//!      normalized vectors. Reports ns/eval, max |Δsim| and R@10 identity on
//!      20 real queries × 3,543 real vectors. Plus a REAL HnswMap built at the
//!      `HNSW_THRESHOLD` (12k) activation point, current vs optimized distance.
//!   2. IVF candidate scan: current per-candidate `get_vector()` (int8→f32
//!      dequantize + 4KB alloc) + `cosine::cosine_similarity` (3 passes) vs a
//!      direct int8×f32 scaled dot (no alloc). Bit-exactness asserted against
//!      the real `MmapEmbeddingStore::get_vector`, R@10 identity, ms/query.
//!   3. GraphRAG hub selection: `Vec<String>::contains` (graph_rag.rs:119)
//!      vs HashSet on the real largest component from silva.db.
//!
//! Run: cargo run --release -p tylluan-kernel --example p2_measurement
//! Output: human-readable table + a JSON block at the end.
#![allow(unknown_lints, clippy::chunks_exact_to_as_chunks)]

use std::collections::{HashMap, HashSet, VecDeque};
use std::hint::black_box;
use std::path::Path;
use std::time::Instant;

use instant_distance::{Builder, Search};
use rusqlite::{Connection, OpenFlags};

use tylluan_kernel::memory::cosine::cosine_similarity;
use tylluan_kernel::memory::ivf_index::{kmeans_plus_plus, IVFSearcher};
use tylluan_kernel::memory::mmap_store::MmapEmbeddingStore;
use tylluan_kernel::memory::silva::hnsw::EmbPoint;

const N_QUERIES: usize = 20;
const DIM: usize = 1024;
const BLOB_BYTES: usize = DIM * 4;
const TOP_K: usize = 10;

// ─── verbatim copies (cited) ────────────────────────────────────────────────

/// VERBATIM copy of `silva/hnsw.rs::EmbPoint::distance` (lines 11-20, main).
fn distance_current(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        return 1.0;
    }
    1.0 - (dot / (na * nb))
}

/// Optimized candidate: `distance = 1 - dot`, valid only if both vectors are
/// normalized (verified against production data: norms in [0.999999, 1.000001]).
fn distance_normalized_dot(a: &[f32], b: &[f32]) -> f32 {
    1.0 - a.iter().zip(b.iter()).map(|(x, y)| x * y).sum::<f32>()
}

/// HNSW point with the optimized distance, for the 12k end-to-end build.
#[derive(Clone)]
struct OptPoint(Vec<f32>);
impl instant_distance::Point for OptPoint {
    fn distance(&self, other: &Self) -> f32 {
        distance_normalized_dot(&self.0, &other.0)
    }
}

// ─── data loading ───────────────────────────────────────────────────────────

struct RealData {
    ids: Vec<String>,
    vectors: Vec<Vec<f32>>,
    edges: Vec<(String, String)>,
    all_node_ids: Vec<String>,
}

fn open_ro() -> Connection {
    Connection::open_with_flags(
        Path::new("data/silva.db"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("cannot open data/silva.db (run from repo root)")
}

fn load_real() -> RealData {
    let conn = open_ro();
    let mut stmt = conn
        .prepare("SELECT node_id, embedding FROM node_embeddings WHERE length(embedding) = ?1")
        .expect("query failed");
    let rows = stmt
        .query_map([BLOB_BYTES as i64], |row| {
            let id: String = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            Ok((id, blob))
        })
        .expect("query failed");
    let mut ids = Vec::new();
    let mut vectors = Vec::new();
    for r in rows.flatten() {
        let (id, blob) = r;
        let v: Vec<f32> = blob
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes(c.try_into().expect("4-byte chunk")))
            .collect();
        ids.push(id);
        vectors.push(v);
    }
    let mut e_stmt = conn
        .prepare("SELECT source, target FROM edges")
        .expect("edges query failed");
    let edges: Vec<(String, String)> = e_stmt
        .query_map(rusqlite::params![], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .expect("edges query failed")
        .flatten()
        .collect();
    let all_node_ids: Vec<String> = conn
        .prepare("SELECT id FROM nodes")
        .expect("nodes query failed")
        .query_map(rusqlite::params![], |r| r.get::<_, String>(0))
        .expect("nodes query failed")
        .flatten()
        .collect();
    println!(
        "loaded {} real vectors (dim {DIM}) + {} edges + {} nodes from data/silva.db",
        vectors.len(),
        edges.len(),
        all_node_ids.len()
    );
    RealData { ids, vectors, edges, all_node_ids }
}

fn select_queries(vectors: &[Vec<f32>]) -> Vec<Vec<f32>> {
    (0..N_QUERIES)
        .map(|i| vectors[i * vectors.len() / N_QUERIES].clone())
        .collect()
}

fn median(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    xs[xs.len() / 2]
}

// ─── item 1: HNSW distance ──────────────────────────────────────────────────

fn bench_hnsw(data: &RealData) {
    let queries = select_queries(&data.vectors);
    let corpus: Vec<&[f32]> = data.vectors.iter().map(|v| v.as_slice()).collect();

    // Norm invariant on real data (the [SIN VERIFICAR] precondition).
    let mut max_norm_dev = 0.0f32;
    for v in &data.vectors {
        let n: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        max_norm_dev = max_norm_dev.max((n - 1.0).abs());
    }

    // Micro: per-eval cost, 5 iterations of the full sweep.
    let evals = queries.len() * corpus.len();
    let mut t_cur = Vec::new();
    let mut t_opt = Vec::new();
    let mut max_delta = 0.0f32;
    let mut r10_mismatches = 0usize;
    for _ in 0..5 {
        let t0 = Instant::now();
        for q in &queries {
            for c in &corpus {
                black_box(distance_current(black_box(q), black_box(c)));
            }
        }
        t_cur.push(t0.elapsed().as_secs_f64() * 1e3);

        let t0 = Instant::now();
        for q in &queries {
            for c in &corpus {
                black_box(distance_normalized_dot(black_box(q), black_box(c)));
            }
        }
        t_opt.push(t0.elapsed().as_secs_f64() * 1e3);
    }
    // Correctness on one pass (not timed).
    for q in &queries {
        let mut ranks_cur: Vec<(usize, f32)> = corpus
            .iter()
            .enumerate()
            .map(|(i, c)| (i, distance_current(q, c)))
            .collect();
        let mut ranks_opt = ranks_cur
            .iter()
            .map(|&(i, _)| (i, distance_normalized_dot(q, corpus[i])))
            .collect::<Vec<_>>();
        max_delta = max_delta.max(
            ranks_cur
                .iter()
                .zip(&ranks_opt)
                .map(|((_, a), (_, b))| (a - b).abs())
                .fold(0.0f32, f32::max),
        );
        ranks_cur.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        ranks_opt.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        let cur: HashSet<usize> = ranks_cur.iter().take(TOP_K).map(|&(i, _)| i).collect();
        let opt: HashSet<usize> = ranks_opt.iter().take(TOP_K).map(|&(i, _)| i).collect();
        if cur != opt {
            r10_mismatches += 1;
        }
    }
    let cur_ms = median(t_cur);
    let opt_ms = median(t_opt);
    println!("\n[1] HNSW distance (micro, {evals} evals/pass, 5 passes, median)");
    println!(
        "    current full-norm: {:8.3} ms/pass  ({:6.1} ns/eval)",
        cur_ms,
        cur_ms * 1e6 / evals as f64
    );
    println!(
        "    1-dot optimized:   {:8.3} ms/pass  ({:6.1} ns/eval)  speedup {:.2}x",
        opt_ms,
        opt_ms * 1e6 / evals as f64,
        cur_ms / opt_ms
    );
    println!("    max |Δsim| {max_delta:.3e}   R@10 mismatches {r10_mismatches}/{}   max |‖v‖-1| {max_norm_dev:.3e}", queries.len());

    // End-to-end at the HNSW_THRESHOLD activation point (12k).
    let threshold = tylluan_kernel::memory::silva::hnsw::HNSW_THRESHOLD;
    let mut big: Vec<Vec<f32>> = Vec::with_capacity(threshold);
    let mut big_ids: Vec<String> = Vec::with_capacity(threshold);
    let mut rep = 0usize;
    while big.len() < threshold {
        for (i, v) in data.vectors.iter().enumerate() {
            if big.len() == threshold {
                break;
            }
            big.push(v.clone());
            big_ids.push(format!("{}#r{rep}", data.ids[i]));
        }
        rep += 1;
    }
    let t0 = Instant::now();
    let points_cur: Vec<EmbPoint> = big.iter().cloned().map(EmbPoint).collect();
    let map_cur = Builder::default().build(points_cur, big_ids.clone());
    let build_cur = t0.elapsed().as_secs_f64();
    let t0 = Instant::now();
    let points_opt: Vec<OptPoint> = big.iter().cloned().map(OptPoint).collect();
    let map_opt = Builder::default().build(points_opt, big_ids);
    let build_opt = t0.elapsed().as_secs_f64();

    let mut cur_q = Vec::new();
    let mut opt_q = Vec::new();
    for _ in 0..20 {
        for q in &queries {
            let t0 = Instant::now();
            let mut s = Search::default();
            black_box(map_cur.search(&EmbPoint(q.clone()), &mut s).take(TOP_K).count());
            cur_q.push(t0.elapsed().as_secs_f64() * 1e3);
            let t0 = Instant::now();
            let mut s = Search::default();
            black_box(map_opt.search(&OptPoint(q.clone()), &mut s).take(TOP_K).count());
            opt_q.push(t0.elapsed().as_secs_f64() * 1e3);
        }
    }
    let cur = median(cur_q);
    let opt = median(opt_q);
    println!("\n[1b] HNSW end-to-end search @ {threshold} points (20 queries × 20 iters, median)");
    println!(
        "    current {:7.3} ms/query   optimized {:7.3} ms/query   speedup {:.2}x   (build {:.2}s / {:.2}s)",
        cur,
        opt,
        cur / opt,
        build_cur,
        build_opt
    );
    println!(
        "    context: production has {} embeddings — HNSW is INACTIVE below {threshold} (silva/mod.rs:431,479)",
        data.vectors.len()
    );
}

// ─── item 2: IVF candidate scan ─────────────────────────────────────────────

fn bench_ivf(data: &RealData) {
    let n = data.vectors.len();
    let nlist = ((n as f64).sqrt() as u32).clamp(1, 100); // graph.rs:608-612
    let (centroids, assignments) = kmeans_plus_plus(&data.vectors, nlist, 10);

    let store_path = Path::new("target/p2_bench_store.fjv1");
    let store = MmapEmbeddingStore::create(
        store_path,
        &data.ids,
        &data.vectors,
        DIM,
        nlist,
        &centroids,
        &assignments,
    )
    .expect("store create failed");

    // Bit-exactness: my quantized copy must equal the real store's dequantized
    // output (same calibrate_scales + quantize as production).
    let scales = {
        // calibrate_scales is pub; compute on the same input.
        tylluan_kernel::memory::mmap_store::calibrate_scales(&data.vectors, DIM)
    };
    let quantized: Vec<Vec<i8>> = data
        .vectors
        .iter()
        .map(|v| tylluan_kernel::memory::mmap_store::quantize(v, &scales))
        .collect();
    let mut dequant_max_err = 0.0f32;
    for i in (0..n).step_by((n / 100).max(1)) {
        let real = store.get_vector(i as u32);
        let mine = tylluan_kernel::memory::mmap_store::dequantize(&quantized[i], &scales);
        for (a, b) in real.iter().zip(&mine) {
            dequant_max_err = dequant_max_err.max((a - b).abs());
        }
    }
    assert!(dequant_max_err == 0.0, "quantized copy diverges from store: {dequant_max_err}");

    let searcher = IVFSearcher::new(store.centroids().to_vec(), store.assignments(), 10);
    let queries = select_queries(&data.vectors);
    let nprobe = 20.min(store.centroids().len()); // search.rs:148

    // Dequantized-norm deviation (decides whether |v| can be skipped).
    let mut max_dnorm = 0.0f32;
    for i in 0..n.min(500) {
        let v = store.get_vector(i as u32);
        let nn: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        max_dnorm = max_dnorm.max((nn - 1.0).abs());
    }

    let mut t_cur = Vec::new();
    let mut t_opt = Vec::new();
    let mut t_opt_norm = Vec::new();
    let mut n_candidates = 0usize;
    let mut max_delta = 0.0f32;
    let mut r10_mismatches = 0usize;
    let mut normed_mismatches = 0usize;
    for _ in 0..20 {
        for q in &queries {
            let nearest = searcher.find_nearest_centroids(q, nprobe);
            let mut cand: Vec<u32> = Vec::new();
            for c in &nearest {
                cand.extend(searcher.inverted_lists()[*c].iter().copied());
            }
            n_candidates += cand.len();

            let t0 = Instant::now();
            let mut cur_scores: Vec<(u32, f32)> = Vec::with_capacity(cand.len());
            for &idx in &cand {
                let v = store.get_vector(idx);
                if v.len() != q.len() {
                    continue;
                }
                cur_scores.push((idx, cosine_similarity(q, &v)));
            }
            t_cur.push(t0.elapsed().as_secs_f64() * 1e3);

            let t0 = Instant::now();
            let mut opt_scores: Vec<(u32, f32)> = Vec::with_capacity(cand.len());
            for &idx in &cand {
                let qi = &quantized[idx as usize];
                // Scaled int8×f32 dot (the roadmap's proposal): same scales the
                // store used at quantization time, no per-candidate alloc.
                let dot: f32 = q
                    .iter()
                    .enumerate()
                    .map(|(d, x)| x * (qi[d] as f32) * scales[d])
                    .sum();
                opt_scores.push((idx, dot));
            }
            t_opt.push(t0.elapsed().as_secs_f64() * 1e3);

            // Variant with cheap per-candidate norm (no alloc): dequantized
            // norms deviate from 1 by quantization error, so skip-|v| ranking
            // must be compared against the current ranking.
            let t0 = Instant::now();
            let mut normed: Vec<(u32, f32)> = Vec::with_capacity(cand.len());
            for &idx in &cand {
                let qi = &quantized[idx as usize];
                let mut dot = 0.0f32;
                let mut sq = 0.0f32;
                for (d, &y) in qi.iter().enumerate() {
                    let yv = y as f32 * scales[d];
                    dot += q[d] * yv;
                    sq += yv * yv;
                }
                normed.push((idx, dot / sq.sqrt()));
            }
            t_opt_norm.push(t0.elapsed().as_secs_f64() * 1e3);

            max_delta = max_delta.max(
                cur_scores
                    .iter()
                    .zip(&opt_scores)
                    .map(|((_, a), (_, b))| (a - b).abs())
                    .fold(0.0f32, f32::max),
            );
            let rank = |v: &[(u32, f32)]| -> Vec<u32> {
                let mut w: Vec<(u32, f32)> = v.to_vec();
                w.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                w.truncate(TOP_K);
                w.into_iter().map(|(i, _)| i).collect()
            };
            let cur_top = rank(&cur_scores);
            if cur_top != rank(&opt_scores) {
                r10_mismatches += 1;
            }
            if cur_top != rank(&normed) {
                normed_mismatches += 1;
            }
            black_box(cur_top.len());
        }
    }
    let total_runs = t_cur.len();
    let cur = median(t_cur.clone());
    let opt = median(t_opt.clone());
    let optn = median(t_opt_norm.clone());
    println!("\n[2] IVF candidate scan (20 queries × 20 iters, nlist {nlist}, nprobe {nprobe}, avg candidates {}/query)", n_candidates / total_runs);
    println!("    current (dequant+alloc+cosine): {cur:8.3} ms/query");
    println!(
        "    optimized (scaled int8×f32 dot, skip |v|): {:8.3} ms/query  speedup {:.2}x   max|Δsim| {max_delta:.3e}  R@10 mismatches {r10_mismatches}/{total_runs}",
        opt,
        cur / opt
    );
    println!(
        "    optimized (dot + cheap |v|):               {:8.3} ms/query  speedup {:.2}x   R@10 mismatches {normed_mismatches}/{total_runs}",
        optn,
        cur / optn
    );
    println!("    dequantized max |‖v‖-1| (500 samples): {max_dnorm:.3e}");
}

// ─── item 3: GraphRAG hub selection ─────────────────────────────────────────

fn bench_graphrag(data: &RealData) {
    // graph_rag.rs:57-60 builds adjacency over ALL node ids (SELECT id FROM
    // nodes), not just ids with embeddings — reproduce that population.
    let idset: HashSet<&String> = data.all_node_ids.iter().collect();
    let mut index_of: HashMap<&str, u32> = HashMap::with_capacity(data.all_node_ids.len());
    for (i, id) in data.all_node_ids.iter().enumerate() {
        index_of.insert(id.as_str(), i as u32);
    }
    let mut adj: Vec<Vec<u32>> = vec![Vec::new(); data.all_node_ids.len()];
    for (s, t) in &data.edges {
        if idset.contains(s) && idset.contains(t) {
            let (a, b) = (index_of[s.as_str()], index_of[t.as_str()]);
            adj[a as usize].push(b);
            adj[b as usize].push(a);
        }
    }
    // BFS to find the largest component (matches graph_rag.rs:81-107).
    let mut visited = vec![false; data.all_node_ids.len()];
    let mut largest: Vec<u32> = Vec::new();
    for start in 0..data.all_node_ids.len() as u32 {
        if visited[start as usize] {
            continue;
        }
        let mut comp = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(start);
        visited[start as usize] = true;
        while let Some(n) = queue.pop_front() {
            comp.push(n);
            for &nb in &adj[n as usize] {
                if !visited[nb as usize] {
                    visited[nb as usize] = true;
                    queue.push_back(nb);
                }
            }
        }
        if comp.len() > largest.len() {
            largest = comp;
        }
    }
    let comp_ids: Vec<String> = largest.iter().map(|&i| data.all_node_ids[i as usize].clone()).collect();
    let deg: f64 = largest
        .iter()
        .map(|&i| adj[i as usize].len() as f64)
        .sum::<f64>()
        / largest.len() as f64;

    // Current: Vec<String>::contains per neighbor (graph_rag.rs:117-120).
    let t0 = Instant::now();
    let hub_vec = comp_ids
        .iter()
        .max_by_key(|id| {
            index_of
                .get(id.as_str())
                .map(|&i| adj[i as usize].iter().filter(|&&n| comp_ids.contains(&data.all_node_ids[n as usize])).count())
                .unwrap_or(0)
        })
        .cloned()
        .unwrap_or_default();
    let t_vec = t0.elapsed().as_secs_f64();

    // Optimized: HashSet<u32> membership.
    let comp_set: HashSet<u32> = largest.iter().copied().collect();
    let t0 = Instant::now();
    let hub_set = largest
        .iter()
        .max_by_key(|&&i| adj[i as usize].iter().filter(|n| comp_set.contains(n)).count())
        .copied();
    let t_set = t0.elapsed().as_secs_f64();
    assert_eq!(
        hub_vec,
        data.all_node_ids[hub_set.expect("non-empty comp") as usize],
        "hub selection diverged"
    );
    println!("\n[3] GraphRAG hub selection on the real largest component");
    println!(
        "    component: {} nodes, avg deg {deg:.1}   (nightly job, graph_rag.rs:117-120)",
        comp_ids.len()
    );
    println!("    Vec<String>::contains (current): {t_vec:8.3} s");
    println!("    HashSet<u32> (optimized):        {t_set:8.3} s   speedup {:.0}x", t_vec / t_set.max(1e-9));
}

fn main() {
    println!("=== P2 measurement harness (Frente 2, T891) — real production data ===\n");
    let data = load_real();
    bench_hnsw(&data);
    bench_ivf(&data);
    bench_graphrag(&data);
    println!("\n=== done ===");
}
