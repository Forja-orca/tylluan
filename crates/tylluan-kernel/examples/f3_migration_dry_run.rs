//! F3 dry-run harness — hybrid → silva content migration measured over
//! CONSISTENT COPIES of the production databases (ROADMAP_O3:58, design
//! announced in Coloquio T963).
//!
//! Production files are never opened for write: each is snapshotted with
//! `VACUUM INTO` (a consistent read-only snapshot even while the kernel is
//! running), and every migration/search runs against the copies in `<workdir>`.
//!
//! Usage:
//!   cargo run -p tylluan-kernel --example f3_migration_dry_run -- \
//!       <prod-hybrid.db> <prod-silva.db> <workdir> [model_name]
//!
//! Prints:
//!   1. Snapshot of both production files (untouched)
//!   2. Baseline counts (hybrid docs, silva nodes, pre-existing hybrid:* ids)
//!   3. MigrationReport JSON + conservation invariant check
//!   4. DB-level conservation (migrated nodes, dense embedding rows) + created_at
//!      parity for a sampled subset
//!   5. Recall parity probe: content-prefix queries must retrieve `hybrid:{id}`
//!      in the silva top-10 (FTS leg, embedding = None)
//!
//! Exit code: 0 = all invariants held, 1 = at least one invariant violated.

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;
use tylluan_kernel::memory::hybrid::HybridMemory;
use tylluan_kernel::memory::hybrid_migration::migrate_hybrid_to_silva;
use tylluan_kernel::memory::silva::SilvaDB;

const TIMESTAMP_SAMPLES: usize = 20;
const PROBE_TARGET: usize = 5;

fn open_ro(path: &Path) -> Result<Connection> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("cannot open read-only: {}", path.display()))
}

fn vacuum_into(src: &Path, dst: &Path) -> Result<()> {
    if dst.exists() {
        std::fs::remove_file(dst)
            .with_context(|| format!("cannot remove stale copy {}", dst.display()))?;
    }
    let src_conn = open_ro(src)?;
    let dst_str = dst.to_string_lossy().into_owned();
    src_conn
        .execute("VACUUM INTO ?1", [&dst_str])
        .with_context(|| format!("VACUUM INTO {} failed", dst.display()))?;
    Ok(())
}

fn count(conn: &Connection, sql: &str) -> Result<i64> {
    Ok(conn.query_row(sql, [], |r| r.get(0))?)
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!(
            "usage: f3_migration_dry_run <prod-hybrid.db> <prod-silva.db> <workdir> [model_name]"
        );
        std::process::exit(2);
    }
    let prod_hybrid = Path::new(&args[1]);
    let prod_silva = Path::new(&args[2]);
    let workdir = Path::new(&args[3]);
    let model_name = args.get(4).map(String::as_str).unwrap_or("dry-run-engine");

    let mut failures: Vec<String> = Vec::new();
    std::fs::create_dir_all(workdir)?;
    let hybrid_copy = workdir.join("hybrid_copy.db");
    let silva_copy = workdir.join("silva_copy.db");

    println!("[1/5] snapshot production -> workdir (VACUUM INTO, read-only source)");
    vacuum_into(prod_hybrid, &hybrid_copy)?;
    vacuum_into(prod_silva, &silva_copy)?;
    println!(
        "      hybrid {} -> {} ({})",
        prod_hybrid.display(),
        hybrid_copy.display(),
        hybrid_copy.metadata()?.len()
    );
    println!(
        "      silva  {} -> {} ({})",
        prod_silva.display(),
        silva_copy.display(),
        silva_copy.metadata()?.len()
    );

    println!("[2/5] open copies + baseline");
    let hybrid = HybridMemory::open(&hybrid_copy.to_string_lossy())?;
    let silva = SilvaDB::open(&silva_copy.to_string_lossy())?;
    silva.init().await.context("silva init on copy")?;
    let ro_h = open_ro(&hybrid_copy)?;
    let ro_s = open_ro(&silva_copy)?;

    let hybrid_docs = count(&ro_h, "SELECT COUNT(*) FROM documents")?;
    let silva_nodes_before = count(&ro_s, "SELECT COUNT(*) FROM nodes")?;
    let hybrid_ids_before = count(&ro_s, "SELECT COUNT(*) FROM nodes WHERE id LIKE 'hybrid:%'")?;
    println!("      hybrid documents          = {hybrid_docs}");
    println!("      silva nodes (baseline)    = {silva_nodes_before}");
    println!("      pre-existing hybrid:* ids = {hybrid_ids_before}");

    // Sample every hybrid doc (id, created_at) — content only for probe picks.
    let docs: Vec<(i64, String, Option<String>)> = {
        let mut stmt = ro_h.prepare("SELECT id, content, created_at FROM documents ORDER BY id")?;
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .filter_map(|r| r.ok())
            .collect()
    };

    println!("[3/5] migrate_hybrid_to_silva on the copy (model_name = {model_name:?})");
    let t0 = std::time::Instant::now();
    let report = migrate_hybrid_to_silva(&silva, &hybrid, model_name).await?;
    println!("      elapsed = {:.2}s", t0.elapsed().as_secs_f64());
    println!("{}", serde_json::to_string_pretty(&report)?);

    let cons = report.migrated + report.twin_skipped + report.already_present + report.failed;
    if cons != report.total {
        failures.push(format!(
            "conservation violated: {} + {} + {} + {} != {}",
            report.migrated, report.twin_skipped, report.already_present, report.failed, report.total
        ));
    }
    if report.failed > 0 {
        failures.push(format!("failed upserts: {:?}", report.failed_ids));
    }

    println!("[4/5] DB-level conservation + created_at parity");
    let migrated_in_db = count(&ro_s, "SELECT COUNT(*) FROM nodes WHERE source = 'hybrid_migration'")?;
    let dense_rows = count(&ro_s, "SELECT COUNT(*) FROM node_embeddings WHERE node_id LIKE 'hybrid:%'")?;
    let hybrid_ids_after = count(&ro_s, "SELECT COUNT(*) FROM nodes WHERE id LIKE 'hybrid:%'")?;
    println!("      nodes source='hybrid_migration'        = {migrated_in_db} (report.migrated = {})", report.migrated);
    println!("      node_embeddings hybrid:* rows           = {dense_rows} (docs with BLOB = {})", report.migrated - report.no_embedding);
    println!("      hybrid:* ids after                      = {hybrid_ids_after}");
    if migrated_in_db != report.migrated as i64 {
        failures.push(format!(
            "DB migrated count {migrated_in_db} != report.migrated {}",
            report.migrated
        ));
    }

    // Docs that actually landed in silva — sampling for parity checks must come
    // from this subset (uniform id spacing would land on twin-skipped docs).
    let migrated_ids: std::collections::HashSet<String> = {
        let mut stmt = ro_s.prepare("SELECT id FROM nodes WHERE id LIKE 'hybrid:%'")?;
        stmt.query_map([], |r| r.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect()
    };
    let migrated_docs: Vec<&(i64, String, Option<String>)> = docs
        .iter()
        .filter(|(id, _, _)| migrated_ids.contains(&format!("hybrid:{id}")))
        .collect();

    println!("[4b/5] created_at parity over migrated docs (sample {TIMESTAMP_SAMPLES})");
    let mut ts_checked = 0usize;
    let mut ts_mismatch = 0usize;
    let step = (migrated_docs.len() / TIMESTAMP_SAMPLES).max(1);
    for (id, _content, hybrid_ts) in migrated_docs.iter().step_by(step) {
        if ts_checked >= TIMESTAMP_SAMPLES {
            break;
        }
        let node_id = format!("hybrid:{id}");
        let silva_ts: Option<String> = ro_s
            .query_row("SELECT created_at FROM nodes WHERE id = ?1", [&node_id], |r| r.get(0))
            .ok();
        match (&silva_ts, hybrid_ts) {
            (None, _) => failures.push(format!("migrated doc hybrid:{id} missing in silva")),
            (Some(s), Some(h)) => {
                ts_checked += 1;
                if s != h {
                    ts_mismatch += 1;
                    println!("      MISMATCH hybrid:{id}: hybrid={h:?} silva={s:?}");
                }
            }
            (Some(s), None) => {
                ts_checked += 1;
                println!("      MISMATCH hybrid:{id}: hybrid=NULL silva={s:?}");
            }
        }
    }
    println!("      created_at checked = {ts_checked}, mismatches = {ts_mismatch}");
    if ts_mismatch > 0 {
        failures.push(format!("{ts_mismatch} created_at mismatches"));
    }

    println!("[5/5] recall parity probe (FTS leg, embedding = None, top-10)");
    // Probe candidates: migrated docs with enough content for a discriminating
    // 12-word prefix, spread evenly across the migrated id space.
    let eligible: Vec<i64> = migrated_docs
        .iter()
        .filter(|(_, content, _)| content.split_whitespace().count() >= 12)
        .map(|(id, _, _)| *id)
        .collect();
    println!(
        "      eligible probe docs (migrated, >=12 words) = {}",
        eligible.len()
    );
    let mut picks: Vec<i64> = Vec::new();
    let probe_step = (eligible.len() / PROBE_TARGET).max(1);
    for id in eligible.iter().step_by(probe_step) {
        if picks.len() >= PROBE_TARGET {
            break;
        }
        picks.push(*id);
    }
    let mut probe_hits = 0usize;
    for id in &picks {
        let content = docs
            .iter()
            .find(|(d, _, _)| d == id)
            .map(|(_, c, _)| c.clone())
            .unwrap_or_default();
        let query: String = content
            .split_whitespace()
            .take(12)
            .collect::<Vec<_>>()
            .join(" ");
        let expected = format!("hybrid:{id}");
        let (hits, _meta) = silva
            .search_hybrid_for_recall_detailed(&query, None, 10, None, false, false)
            .await?;
        let top1 = hits
            .first()
            .map(|(n, _)| n.id.clone())
            .unwrap_or_else(|| "<empty>".to_string());
        let found = hits.iter().any(|(n, _)| n.id == expected);
        if found {
            probe_hits += 1;
        }
        println!(
            "      {} doc {id}: expected {expected}, top1={top1}",
            if found { "HIT " } else { "MISS" }
        );
    }
    println!("      probe hits = {probe_hits}/{}", picks.len());
    if picks.len() < PROBE_TARGET {
        failures.push(format!("only {} probe docs available (want {PROBE_TARGET})", picks.len()));
    }
    if probe_hits != picks.len() {
        failures.push(format!("probe misses: {}/{}, want all", picks.len() - probe_hits, picks.len()));
    }

    println!();
    if failures.is_empty() {
        println!("SUMMARY: PASS — all F3 dry-run invariants held");
        Ok(())
    } else {
        println!("SUMMARY: FAIL");
        for f in &failures {
            println!("  - {f}");
        }
        std::process::exit(1);
    }
}
