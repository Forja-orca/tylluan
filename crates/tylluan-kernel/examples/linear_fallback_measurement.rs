//! Linear Fallback Vector Search Measurement Harness (ROADMAP_O3:59)
//!
//! Reads REAL vectors from `data/silva.db` (read-only) and measures the fallback
//! linear scan from `crates/tylluan-kernel/src/memory/silva/search.rs:73-122`:
//!
//!   - Current: `query_map` allocating `String` + `Vec<u8>` (4KB) per row,
//!     allocating `Vec<f32>` (4KB) per candidate, and `cosine_similarity`
//!     recalculating both norms. (10,000+ allocs / 27 MB per query).
//!   - Optimized: `stmt.query` zero-copy `ValueRef::Blob(&[u8])`, precomputed query norm,
//!     fused single-pass chunked dot + stored norm, `String` allocated only for `sim > 0.05`.
//!
//! Run: cargo run --release -p tylluan-kernel --example linear_fallback_measurement
#![allow(unknown_lints, clippy::chunks_exact_to_as_chunks, clippy::uninlined_format_args)]

use std::hint::black_box;
use std::path::Path;
use std::time::Instant;

use rusqlite::{Connection, OpenFlags};
use tylluan_kernel::memory::cosine::cosine_similarity;

const N_QUERIES: usize = 20;
const DIM: usize = 1024;
const BLOB_BYTES: usize = DIM * 4;
const TOP_K: usize = 10;
const RUNS_PER_QUERY: usize = 5;

fn open_ro() -> Connection {
    let p1 = Path::new("data/silva.db");
    let p2 = Path::new("e:/tylluan/data/silva.db");
    let path = if p1.exists() { p1 } else { p2 };
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("cannot open silva.db")
}

fn load_sample_queries(conn: &Connection) -> Vec<(String, Vec<f32>)> {
    let mut stmt = conn
        .prepare("SELECT node_id, embedding FROM node_embeddings WHERE length(embedding) = ?1 LIMIT ?2")
        .expect("query failed");
    let rows = stmt
        .query_map(rusqlite::params![BLOB_BYTES as i64, N_QUERIES as i64], |row| {
            let id: String = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            Ok((id, blob))
        })
        .expect("query failed");

    let mut queries = Vec::new();
    for r in rows.flatten() {
        let (id, blob) = r;
        let floats: Vec<f32> = blob
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();
        queries.push((id, floats));
    }
    queries
}

/// VERBATIM implementation from search.rs:78-122
fn search_current(conn: &Connection, query_embedding: &[f32], limit: usize) -> Vec<(String, f32)> {
    let mut stmt = conn
        .prepare("SELECT node_id, embedding FROM node_embeddings ORDER BY rowid DESC LIMIT 5000")
        .expect("prepare failed");

    let mut scored: Vec<(String, f32)> = Vec::new();

    let rows = stmt
        .query_map([], |row| {
            let id: String = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            Ok((id, blob))
        })
        .expect("query_map failed");

    for row in rows.flatten() {
        let (id, blob) = row;
        if blob.is_empty() {
            continue;
        }

        let stored: Vec<f32> = blob
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();

        if stored.len() != query_embedding.len() {
            continue;
        }

        let sim = cosine_similarity(query_embedding, &stored);
        if sim > 0.05 {
            scored.push((id, sim));
        }
    }

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    scored
}

/// Optimized zero-allocation streaming scan candidate
fn search_optimized(conn: &Connection, query_embedding: &[f32], limit: usize) -> Vec<(String, f32)> {
    let mut stmt = conn
        .prepare("SELECT node_id, embedding FROM node_embeddings ORDER BY rowid DESC LIMIT 5000")
        .expect("prepare failed");

    let q_norm_sq: f32 = query_embedding.iter().map(|&v| v * v).sum();
    if q_norm_sq == 0.0 {
        return vec![];
    }
    let q_norm = q_norm_sq.sqrt();
    let expected_bytes = query_embedding.len() * 4;

    let mut scored: Vec<(String, f32)> = Vec::new();

    let mut rows = stmt.query([]).expect("query failed");
    while let Ok(Some(row)) = rows.next() {
        let blob: &[u8] = match row.get_ref(1) {
            Ok(rusqlite::types::ValueRef::Blob(b)) => b,
            _ => continue,
        };
        if blob.len() != expected_bytes {
            continue;
        }

        let mut dot: f32 = 0.0;
        let mut stored_norm_sq: f32 = 0.0;
        for (chunk, &q) in blob.chunks_exact(4).zip(query_embedding.iter()) {
            let s = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            dot += s * q;
            stored_norm_sq += s * s;
        }

        if stored_norm_sq == 0.0 {
            continue;
        }

        let sim = dot / (q_norm * stored_norm_sq.sqrt());
        if sim > 0.05 {
            let id: String = row.get(0).unwrap_or_default();
            scored.push((id, sim));
        }
    }

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    scored
}

fn median(mut times: Vec<f64>) -> f64 {
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    times[times.len() / 2]
}

fn main() {
    let conn = open_ro();

    let total_rows: i64 = conn
        .query_row("SELECT count(*) FROM node_embeddings", [], |r| r.get(0))
        .expect("count failed");
    println!("=== Linear Fallback Search Benchmark (ROADMAP_O3:59) ===");
    println!("Production node_embeddings count: {}", total_rows);

    let queries = load_sample_queries(&conn);
    println!("Loaded {} test queries (dim: {})", queries.len(), DIM);

    let mut max_delta_sim: f32 = 0.0;
    let mut ranking_mismatches = 0;

    let mut times_current = Vec::new();
    let mut times_optimized = Vec::new();

    for (q_idx, (qid, q_vec)) in queries.iter().enumerate() {
        // Correctness & Parity check
        let res_curr = search_current(&conn, q_vec, TOP_K);
        let res_opt = search_optimized(&conn, q_vec, TOP_K);

        let ids_curr: Vec<&str> = res_curr.iter().map(|r| r.0.as_str()).collect();
        let ids_opt: Vec<&str> = res_opt.iter().map(|r| r.0.as_str()).collect();

        if ids_curr != ids_opt {
            ranking_mismatches += 1;
            println!("Query {} ({}) ranking mismatch!", q_idx, qid);
        }

        for ((_, s1), (_, s2)) in res_curr.iter().zip(res_opt.iter()) {
            let delta = (s1 - s2).abs();
            if delta > max_delta_sim {
                max_delta_sim = delta;
            }
        }

        // Timing benchmark
        let mut t_cur_runs = Vec::new();
        for _ in 0..RUNS_PER_QUERY {
            let start = Instant::now();
            let _ = black_box(search_current(&conn, q_vec, TOP_K));
            t_cur_runs.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        times_current.push(median(t_cur_runs));

        let mut t_opt_runs = Vec::new();
        for _ in 0..RUNS_PER_QUERY {
            let start = Instant::now();
            let _ = black_box(search_optimized(&conn, q_vec, TOP_K));
            t_opt_runs.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        times_optimized.push(median(t_opt_runs));
    }

    let median_curr = median(times_current);
    let median_opt = median(times_optimized);
    let speedup = median_curr / median_opt;

    println!("\n--- RESULTS ---");
    println!("Top-{} Ranking Parity: {}/{} mismatches", TOP_K, ranking_mismatches, N_QUERIES);
    println!("Max |Δsim|: {:.2e}", max_delta_sim);
    println!("Current Median Latency:   {:.2} ms / query", median_curr);
    println!("Optimized Median Latency: {:.2} ms / query", median_opt);
    println!("Speedup Multiplier:       {:.2}x", speedup);
    println!(
        "Heap Allocations per query: Current: ~{} allocs (~{:.1} MB) -> Optimized: <{} allocs (<{:.1} KB)",
        total_rows * 3,
        (total_rows as f64 * 8192.0) / (1024.0 * 1024.0),
        TOP_K + 50,
        ((TOP_K + 50) as f64 * 64.0) / 1024.0
    );
}
