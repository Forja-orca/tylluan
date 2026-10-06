//! F3 — One-way content migration HybridMemory (tylluan.db) → SilvaDB.
//!
//! Design announced in Coloquio T963 (2026-10-06). Key contract points:
//!
//! * **Idempotent**: node id is `hybrid:{doc_id}` — already-present ids are
//!   skipped, so a run aborted by `GuardedTask` resumes cleanly on the next
//!   tick. Completion is recorded in `silva_kv` under `hybrid_migration_v1`.
//! * **Reversible**: migrated rows carry `nodes.source = 'hybrid_migration'`
//!   (M40-P4 column); undo is `DELETE FROM nodes WHERE source='hybrid_migration'`.
//! * **Conservation invariant** reported per class:
//!   `migrated + twin_skipped + already_present + failed == total`.
//! * **Twin skip**: content jaccard > 0.85 against ANY existing silva node
//!   (same function and threshold the recall read path uses) — a near-copy
//!   already in silva must not be imported twice. Size prefilter keeps the
//!   2.8k × 7.2k comparison cheap: jaccard ≤ min/max, so word counts outside
//!   the `[0.85s, s/0.85]` band can never exceed the threshold.
//! * **Timestamps preserved** via a post-upsert `UPDATE` (upsert stamps
//!   `CURRENT_TIMESTAMP`; migrated history must not look fresh to decay).
//! * **Embeddings**: dense BLOB copied byte-identically (no re-embedding),
//!   `model_name = engine.engine_id()` passed by the caller (hardcoded model
//!   names in embedding writes are banned by anti-drift test), `model_hash =
//!   MODEL_HASH_UNKNOWN` (pre-traceability sentinel, TL 2026-10-05). The
//!   sparse signature is NOT computed here: the Agnostic Reindexer backfills
//!   missing `node_sparse_embeddings` rows on its own cadence via
//!   `save_embedding` (budget-gated, coalesced batches) — the migration job
//!   must stay ONNX-free so it cannot compete with first-recall traffic.
//! * **Eval isolation**: `run_longmemeval_s` writes to its own temp
//!   HybridMemory per run (see `api_eval.rs`), so this store has exactly one
//!   writer path ever: the hybrid guild pipeline that F4 will retire.

use anyhow::{Context, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tracing::{error, info, warn};

use super::hybrid::HybridMemory;
use super::silva::nodes::{NodeWriteOptions, MODEL_HASH_UNKNOWN};
use super::silva::{jaccard_similarity, SilvaDB};

/// Column value marking a node as imported from hybrid. Undo: `DELETE FROM
/// nodes WHERE source = 'hybrid_migration'`.
pub const MIGRATION_SOURCE: &str = "hybrid_migration";

/// `silva_kv` key holding the completion flag (JSON `MigrationReport`).
const FLAG_KEY: &str = "hybrid_migration_v1";

/// `silva_kv` key counting consecutive passes that ended with `failed > 0`.
const ATTEMPTS_KEY: &str = "hybrid_migration_attempts";

/// After this many passes with failures, stop retrying and write the flag
/// anyway (with `failed_ids` recorded) so F4 has an explicit gate.
const MAX_ATTEMPTS: u32 = 5;

/// Same threshold as the read-path twin skip (`handler_recall.rs`).
const TWIN_JACCARD: f64 = 0.85;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MigrationReport {
    pub total: usize,
    pub migrated: usize,
    pub twin_skipped: usize,
    pub already_present: usize,
    /// Migrated rows where no embedding row could be written.
    pub no_embedding: usize,
    pub failed: usize,
    #[serde(default)]
    pub already_done: bool,
    /// Migrated rows per node type (conservation by class).
    #[serde(default)]
    pub by_type: HashMap<String, usize>,
    /// Ids that failed the nodes upsert (gate input for F4).
    #[serde(default)]
    pub failed_ids: Vec<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
}

/// True once the completion flag exists in `silva_kv`.
pub async fn migration_complete(silva: &SilvaDB) -> bool {
    kv_get(silva, FLAG_KEY)
        .await
        .ok()
        .flatten()
        .is_some()
}

/// Run (or resume) the hybrid → silva content migration.
///
/// `model_name` must be `engine.engine_id()` from the caller — never a
/// hardcoded literal (anti-drift test `no_hardcoded_model_names_in_embedding_writes`).
pub async fn migrate_hybrid_to_silva(
    silva: &SilvaDB,
    hybrid: &HybridMemory,
    model_name: &str,
) -> Result<MigrationReport> {
    if let Some(raw) = kv_get(silva, FLAG_KEY).await? {
        // Completion flag: short-circuit with a FRESH report — this run did
        // nothing, so reporting the stored counters would double-count them
        // for any caller that aggregates across runs. Only `completed_at` is
        // carried over (when the original run finished).
        let stored_at = serde_json::from_str::<MigrationReport>(&raw)
            .ok()
            .and_then(|s| s.completed_at);
        return Ok(MigrationReport {
            already_done: true,
            completed_at: stored_at,
            ..Default::default()
        });
    }

    let docs = hybrid.scan_for_migration().await?;
    let total = docs.len();
    let (existing_ids, silva_pairs) = load_silva_state(silva).await?;
    // (word count, content) per silva node — the size prefilter runs before
    // the exact jaccard call.
    let silva_index: Vec<(usize, &str)> = silva_pairs
        .iter()
        .map(|(_, c)| (c.split_whitespace().count(), c.as_str()))
        .collect();

    let mut report = MigrationReport {
        total,
        ..Default::default()
    };

    for row in &docs {
        let id = format!("hybrid:{}", row.id);

        if existing_ids.contains(&id) {
            // Resume path: node already imported. The dense row may be missing
            // if a previous pass died between the two writes — heal it here.
            if let Some(blob) = valid_blob(&row.embedding) {
                if let Err(e) = write_dense(silva, &id, blob, model_name).await {
                    warn!("F3 migration: dense heal failed for {id}: {e:?}");
                }
            }
            report.already_present += 1;
            continue;
        }

        let doc_words = row.content.split_whitespace().count();
        if doc_words > 0 && is_twin(doc_words, &row.content, &silva_index) {
            report.twin_skipped += 1;
            continue;
        }

        let node_type = classify_node_type(&row.metadata);
        let opts = NodeWriteOptions::new("unverified")
            .source(Some(MIGRATION_SOURCE))
            .drift_allowed(true);
        if let Err(e) = silva
            .upsert_node_with_validity(&id, &node_type, &row.content, &row.metadata, opts)
            .await
        {
            warn!("F3 migration: upsert failed for {id}: {e:?}");
            report.failed += 1;
            report.failed_ids.push(id);
            continue;
        }

        if let Some(ts) = row.created_at.as_deref().filter(|t| !t.is_empty()) {
            preserve_timestamps(silva, &id, ts).await;
        }

        match valid_blob(&row.embedding) {
            Some(blob) => {
                if let Err(e) = write_dense(silva, &id, blob, model_name).await {
                    // Node row is already in (content conserved); F4 must gate
                    // on failed_ids only when `failed` counted this.
                    warn!("F3 migration: dense write failed for {id}: {e:?}");
                    report.no_embedding += 1;
                }
            }
            None => report.no_embedding += 1,
        }

        report.migrated += 1;
        *report.by_type.entry(node_type).or_insert(0) += 1;
    }

    if report.failed == 0 {
        finalize_flag(silva, &report).await?;
        if report.migrated > 0 {
            if let Err(e) = silva.consolidate_ivf_index().await {
                warn!("F3 migration: IVF consolidate failed (non-fatal): {e:?}");
            }
        }
        info!(
            "F3 hybrid→silva migration complete: total={} migrated={} twin_skipped={} already_present={} no_embedding={} by_type={:?}",
            report.total, report.migrated, report.twin_skipped,
            report.already_present, report.no_embedding, report.by_type
        );
        return Ok(report);
    }

    // Partial pass — retry on the next tick, bounded by MAX_ATTEMPTS.
    let attempts: u32 = kv_get(silva, ATTEMPTS_KEY)
        .await?
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
        + 1;
    if attempts >= MAX_ATTEMPTS {
        error!(
            "F3 hybrid→silva migration: {} rows still failing after {attempts} passes — writing completion flag anyway; failed_ids={:?} (F4 must gate on this)",
            report.failed, report.failed_ids
        );
        finalize_flag(silva, &report).await?;
    } else {
        kv_set(silva, ATTEMPTS_KEY, &attempts.to_string()).await?;
        info!(
            "F3 hybrid→silva migration partial: {}/{} failed (attempt {attempts}/{MAX_ATTEMPTS}) — retrying next tick",
            report.failed, report.total
        );
    }
    Ok(report)
}

/// Rule-based type mapping (announced T963):
/// `source=tylluan_do` → `episode`; explicit `metadata.type` next;
/// any other `source` → `document`; source-less → `coloquio_memory`.
fn classify_node_type(metadata: &str) -> String {
    let parsed: serde_json::Value = serde_json::from_str(metadata).unwrap_or(serde_json::Value::Null);
    let source = parsed.get("source").and_then(|s| s.as_str());
    if source == Some("tylluan_do") {
        return "episode".to_string();
    }
    if let Some(t) = parsed.get("type").and_then(|t| t.as_str()).filter(|t| !t.is_empty()) {
        return t.to_string();
    }
    if source.is_some() {
        return "document".to_string();
    }
    "coloquio_memory".to_string()
}

/// True when an existing silva node's word set has jaccard > 0.85 with the
/// doc — i.e. the read path would already surface it as the same memory.
fn is_twin(doc_words: usize, content: &str, silva_index: &[(usize, &str)]) -> bool {
    silva_index.iter().any(|(cand_words, cand_content)| {
        // jaccard ≤ min/max — candidates outside the band can't reach 0.85.
        if doc_words * 100 <= *cand_words * 85 || *cand_words * 100 <= doc_words * 85 {
            return false;
        }
        jaccard_similarity(content, cand_content) > TWIN_JACCARD
    })
}

fn valid_blob(blob: &Option<Vec<u8>>) -> Option<&[u8]> {
    blob.as_deref().filter(|b| !b.is_empty() && b.len().is_multiple_of(4))
}

/// Mirror of `save_embedding`'s dense SQL without the ONNX sparse sidecar
/// (F3 job stays ONNX-free; reindexer backfills sparse).
async fn write_dense(silva: &SilvaDB, id: &str, blob: &[u8], model_name: &str) -> Result<()> {
    let id = id.to_string();
    let blob = blob.to_vec();
    let model_name = model_name.to_string();
    let dimensions = (blob.len() / 4) as i32;
    tokio::task::block_in_place(|| -> Result<()> {
        let conn = silva.conn.blocking_lock();
        conn.execute(
            "INSERT INTO node_embeddings (node_id, embedding, model_name, model_hash, dimensions)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(node_id) DO UPDATE SET
                embedding = excluded.embedding,
                model_name = excluded.model_name,
                model_hash = excluded.model_hash,
                dimensions = excluded.dimensions",
            params![id, blob, model_name, MODEL_HASH_UNKNOWN, dimensions],
        )?;
        Ok(())
    })
    .context("dense embedding insert")
}

async fn preserve_timestamps(silva: &SilvaDB, id: &str, ts: &str) {
    let id = id.to_string();
    let ts = ts.to_string();
    let res: Result<usize> = tokio::task::block_in_place(|| {
        let conn = silva.conn.blocking_lock();
        Ok(conn.execute(
            "UPDATE nodes SET created_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![ts, id],
        )?)
    });
    if let Err(e) = res {
        warn!("F3 migration: timestamp preserve failed for {id}: {e:?}");
    }
}

/// `hybrid:` ids already in silva + `(id, content)` of every silva node.
type SilvaScanState = (HashSet<String>, Vec<(String, String)>);

async fn load_silva_state(silva: &SilvaDB) -> Result<SilvaScanState> {
    tokio::task::block_in_place(|| -> Result<SilvaScanState> {
        let conn = silva.conn.blocking_lock();
        let mut existing = HashSet::new();
        {
            let mut stmt = conn.prepare("SELECT id FROM nodes WHERE id LIKE 'hybrid:%'")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            for id in rows.filter_map(|r| r.ok()) {
                existing.insert(id);
            }
        }
        let mut pairs = Vec::new();
        let mut stmt = conn.prepare("SELECT id, content FROM nodes")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for pair in rows.filter_map(|r| r.ok()) {
            pairs.push(pair);
        }
        Ok((existing, pairs))
    })
}

async fn kv_get(silva: &SilvaDB, key: &str) -> Result<Option<String>> {
    let key = key.to_string();
    tokio::task::block_in_place(|| -> Result<Option<String>> {
        let conn = silva.conn.blocking_lock();
        let mut stmt = conn.prepare("SELECT value FROM silva_kv WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    })
}

async fn kv_set(silva: &SilvaDB, key: &str, value: &str) -> Result<()> {
    let key = key.to_string();
    let value = value.to_string();
    tokio::task::block_in_place(|| -> Result<()> {
        let conn = silva.conn.blocking_lock();
        conn.execute(
            "INSERT OR REPLACE INTO silva_kv (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    })
}

async fn finalize_flag(silva: &SilvaDB, report: &MigrationReport) -> Result<()> {
    let mut stamped = report.clone();
    stamped.completed_at = Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());
    let json = serde_json::to_string(&stamped)?;
    kv_set(silva, FLAG_KEY, &json).await?;
    kv_set(silva, ATTEMPTS_KEY, "0").await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::hybrid::HybridMemory;
    use crate::memory::silva::SilvaDB;

    async fn temp_hybrid() -> (tempfile::TempDir, HybridMemory) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("hybrid.db");
        let mem = HybridMemory::open(path.to_str().expect("utf8 path")).expect("open hybrid");
        mem.init().await.expect("init hybrid");
        (dir, mem)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn migration_classifies_dedups_copies_and_resumes() {
        let (_dir, mem) = temp_hybrid().await;

        // doc 1 — tylluan_do action trace → episode, WITH embedding (byte parity).
        mem.add_document(
            "zanzibar spice route unique marker content",
            r#"{"source":"tylluan_do","agent_id":"claude"}"#,
            Some(&[0.25f32; 4]),
        )
        .await
        .expect("add doc1");
        // doc 2 — twin of a pre-existing silva node → skipped.
        let twin_content = "twin silhouette duplicate content text reused";
        mem.add_document(twin_content, r#"{"source":"tylluan_do"}"#, None)
            .await
            .expect("add doc2");
        // doc 3 — explicit metadata.type.
        mem.add_document("project falcon milestone delta", r#"{"type":"project"}"#, None)
            .await
            .expect("add doc3");
        // doc 4 — no source, no type → coloquio_memory.
        mem.add_document("coloquio morning standup message body", "{}", None)
            .await
            .expect("add doc4");
        // doc 5 — other source → document.
        mem.add_document("manual smoke test entry", r#"{"source":"smoke-test"}"#, None)
            .await
            .expect("add doc5");

        let silva = SilvaDB::in_memory().await.expect("silva in_memory");
        silva
            .upsert_node("preexisting-twin", "note", twin_content, "{}")
            .await
            .expect("seed twin");

        let report = migrate_hybrid_to_silva(&silva, &mem, "test-engine")
            .await
            .expect("migrate");
        assert_eq!(report.total, 5);
        assert_eq!(report.migrated, 4);
        assert_eq!(report.twin_skipped, 1);
        assert_eq!(report.already_present, 0);
        assert_eq!(report.failed, 0);
        assert!(!report.already_done);
        assert_eq!(report.no_embedding, 3); // doc3/doc4/doc5 have no BLOB
        assert_eq!(report.by_type.get("episode"), Some(&1));
        assert_eq!(report.by_type.get("project"), Some(&1));
        assert_eq!(report.by_type.get("coloquio_memory"), Some(&1));
        assert_eq!(report.by_type.get("document"), Some(&1));
        assert_eq!(
            report.migrated + report.twin_skipped + report.already_present + report.failed,
            report.total
        );

        // Node row: type/provenance/source + preserved created_at.
        let (typ, prov, src, created): (String, String, String, String) = tokio::task::block_in_place(|| {
            let conn = silva.conn.blocking_lock();
            conn.query_row(
                "SELECT type, provenance, source, created_at FROM nodes WHERE id = 'hybrid:1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .expect("hybrid:1 row")
        });
        assert_eq!(typ, "episode");
        assert_eq!(prov, "unverified");
        assert_eq!(src, MIGRATION_SOURCE);
        let hybrid_ts: String = mem
            .scan_for_migration()
            .await
            .expect("hybrid scan")
            .iter()
            .find(|r| r.id == 1)
            .and_then(|r| r.created_at.clone())
            .expect("hybrid doc1 ts");
        assert_eq!(created, hybrid_ts, "created_at must be preserved verbatim");

        // Dense row: byte-identical copy, engine id + pre-hash sentinel.
        let (blob, mname, mhash, dims): (Vec<u8>, String, String, i64) =
            tokio::task::block_in_place(|| {
                let conn = silva.conn.blocking_lock();
                conn.query_row(
                    "SELECT embedding, model_name, model_hash, dimensions FROM node_embeddings WHERE node_id = 'hybrid:1'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .expect("hybrid:1 embedding row")
            });
        let expected: Vec<u8> = [0.25f32; 4].iter().flat_map(|f| f.to_le_bytes().to_vec()).collect();
        assert_eq!(blob, expected, "dense BLOB must be byte-identical");
        assert_eq!(mname, "test-engine");
        assert_eq!(mhash, MODEL_HASH_UNKNOWN);
        assert_eq!(dims, 4);

        // FTS synced (content=nodes external table).
        let fts_hits: i64 = tokio::task::block_in_place(|| {
            let conn = silva.conn.blocking_lock();
            conn.query_row(
                "SELECT COUNT(*) FROM nodes_fts WHERE nodes_fts MATCH 'zanzibar'",
                [],
                |r| r.get(0),
            )
            .expect("fts match")
        });
        assert!(fts_hits >= 1, "FTS must contain migrated content");

        // Completion flag written.
        let flag = kv_get(&silva, FLAG_KEY).await.expect("kv read").expect("flag present");
        let stored: MigrationReport = serde_json::from_str(&flag).expect("flag json");
        assert_eq!(stored.migrated, 4);

        // Second run — flag short-circuits.
        let again = migrate_hybrid_to_silva(&silva, &mem, "test-engine").await.expect("rerun");
        assert!(again.already_done);
        assert_eq!(again.migrated, 0);

        // Simulate a lost flag → per-doc idempotency (already_present).
        tokio::task::block_in_place(|| {
            let conn = silva.conn.blocking_lock();
            conn.execute("DELETE FROM silva_kv WHERE key = ?1", params![FLAG_KEY])
                .expect("clear flag");
        });
        let resumed = migrate_hybrid_to_silva(&silva, &mem, "test-engine").await.expect("resume");
        assert_eq!(resumed.already_present, 4);
        assert_eq!(resumed.twin_skipped, 1);
        assert_eq!(resumed.migrated, 0);
        assert_eq!(resumed.failed, 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn twin_threshold_matches_read_path() {
        let (_dir, mem) = temp_hybrid().await;
        let silva = SilvaDB::in_memory().await.expect("silva in_memory");

        let existing = "alpha beta gamma delta epsilon zeta eta theta";
        silva.upsert_node("seed", "note", existing, "{}").await.expect("seed");

        // 7/9 ≈ 0.777 → below threshold → migrated.
        mem.add_document("alpha beta gamma delta epsilon zeta eta omega", "{}", None)
            .await
            .expect("near doc");
        // 8/9 ≈ 0.888 → above threshold → twin skip.
        mem.add_document("alpha beta gamma delta epsilon zeta eta theta kappa", "{}", None)
            .await
            .expect("twin doc");

        let report = migrate_hybrid_to_silva(&silva, &mem, "test-engine")
            .await
            .expect("migrate");
        assert_eq!(report.migrated, 1, "0.777 jaccard must migrate");
        assert_eq!(report.twin_skipped, 1, "0.888 jaccard must be skipped");
        assert!(silva.get_node("hybrid:1").await.expect("get").is_some());
        assert!(silva.get_node("hybrid:2").await.expect("get").is_none());
    }

    #[test]
    fn classify_node_type_rules() {
        assert_eq!(
            classify_node_type(r#"{"source":"tylluan_do","type":"ignored"}"#),
            "episode"
        );
        assert_eq!(classify_node_type(r#"{"type":"project"}"#), "project");
        assert_eq!(classify_node_type(r#"{"type":""}"#), "coloquio_memory");
        assert_eq!(classify_node_type(r#"{"source":"smoke"}"#), "document");
        assert_eq!(classify_node_type("{}"), "coloquio_memory");
        assert_eq!(classify_node_type("not json"), "coloquio_memory");
    }
}
