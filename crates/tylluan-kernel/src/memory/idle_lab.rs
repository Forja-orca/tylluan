use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use tracing::{info, warn};

use crate::memory::idle_lab_oracle::{self, OraclePair};
use crate::memory::silva::SilvaDB;
use crate::router::embeddings::{EmbeddingEngine, RerankEngine};

// ─── Global retrieval params (set at startup, mutated by IdleLab) ────────────

/// candidate_pool_mult: candidate_pool = limit * CANDIDATE_POOL_MULT (min 100)
/// Default 20. IdleLab explores 10-40.
pub static CANDIDATE_POOL_MULT: AtomicUsize = AtomicUsize::new(20);

/// rerank_window: how many candidates pass to Jina cross-encoder.
/// Default 50. IdleLab explores 20-80.
pub static RERANK_WINDOW: AtomicUsize = AtomicUsize::new(50);

// ─── Grupo B — routing & consolidation params (tuned by IdleLab) ────────────

/// semantic_weight: balance between vector (semantic) and BM25 (keyword) scores in RRF fusion.
/// Stored as fixed-point u32: 70 = 0.70. Range 30-90.
pub static SEMANTIC_WEIGHT: AtomicU32 = AtomicU32::new(70);

/// builder_bonus: routing bonus added when intent matches builder keywords (architecture, deploy, etc).
/// Stored as fixed-point u32: 12 = 0.12. Range 0-30.
pub static BUILDER_BONUS: AtomicU32 = AtomicU32::new(12);

/// scholar_bonus: routing bonus for scholar/research intents.
pub static SCHOLAR_BONUS: AtomicU32 = AtomicU32::new(12);

/// warden_bonus: routing bonus for security/audit intents.
pub static WARDEN_BONUS: AtomicU32 = AtomicU32::new(20);

/// dedup_cosine: cosine similarity threshold for duplicate node merging in DreamCycle.
/// Stored as fixed-point u32: 92 = 0.92. Range 80-98.
pub static DEDUP_COSINE: AtomicU32 = AtomicU32::new(92);

/// Global experiment step counter — persists across run_experiments calls so that
/// even single-experiment daemon cycles rotate through all 8 mutation steps.
pub static EXPERIMENT_STEP: AtomicUsize = AtomicUsize::new(0);

// ─── Structs ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RetrievalParams {
    pub candidate_pool_mult: usize,
    pub rerank_window: usize,
    pub semantic_weight: u32,
    pub dedup_cosine: u32,
}

impl Default for RetrievalParams {
    fn default() -> Self {
        Self { candidate_pool_mult: 20, rerank_window: 50, semantic_weight: 70, dedup_cosine: 92 }
    }
}

#[derive(Debug)]
pub struct ExperimentResult {
    pub params: RetrievalParams,
    pub recall_at_1: f64,
    pub recall_at_5: f64,
    pub kept: bool,
    pub timestamp: i64,
    pub note: String,
}

pub struct IdleLab {
    silva: Arc<SilvaDB>,
    results_path: PathBuf,
    best_path: PathBuf,
    oracle_path: PathBuf,
}

impl IdleLab {
    pub fn new(silva: Arc<SilvaDB>, data_dir: &std::path::Path) -> Self {
        Self {
            silva,
            results_path: data_dir.join("idle_lab_results.tsv"),
            best_path: data_dir.join("idle_lab_best.json"),
            oracle_path: data_dir.join("idle_lab_oracle.json"),
        }
    }

    /// Load best params from disk if available and apply to globals.
    pub fn load_best_params(&self) {
        if let Ok(raw) = std::fs::read_to_string(&self.best_path)
            && let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(mult) = v["candidate_pool_mult"].as_u64() {
                    CANDIDATE_POOL_MULT.store(mult as usize, Ordering::Relaxed);
                }
                if let Some(win) = v["rerank_window"].as_u64() {
                    RERANK_WINDOW.store(win as usize, Ordering::Relaxed);
                }
                if let Some(sw) = v["semantic_weight"].as_u64() {
                    SEMANTIC_WEIGHT.store(sw as u32, Ordering::Relaxed);
                }
                if let Some(dc) = v["dedup_cosine"].as_u64() {
                    DEDUP_COSINE.store(dc as u32, Ordering::Relaxed);
                }
                info!("🧪 IdleLab: loaded best params (pool_mult={}, rerank_win={}, semantic_weight={}, dedup_cosine={})",
                    CANDIDATE_POOL_MULT.load(Ordering::Relaxed),
                    RERANK_WINDOW.load(Ordering::Relaxed),
                    SEMANTIC_WEIGHT.load(Ordering::Relaxed),
                    DEDUP_COSINE.load(Ordering::Relaxed));
            }
    }

    /// Run up to `max_experiments` keep/discard cycles.
    /// Loads oracle from `oracle_path` (JSON) on each call, fallback to hardcoded default.
    /// Hill climbing uses composite score: 0.6 * R@1 + 0.4 * R@5.
    pub async fn run_experiments(
        &self,
        engine: Option<&EmbeddingEngine>,
        reranker: Option<&RerankEngine>,
        max_experiments: usize,
    ) {
        let oracle = idle_lab_oracle::load_oracle(&self.oracle_path);
        info!("🧪 IdleLab: starting {} experiment(s) with oracle {} pairs (from {})",
            max_experiments, oracle.len(), self.oracle_path.display());

        let current = RetrievalParams {
            candidate_pool_mult: CANDIDATE_POOL_MULT.load(Ordering::Relaxed),
            rerank_window: RERANK_WINDOW.load(Ordering::Relaxed),
            semantic_weight: SEMANTIC_WEIGHT.load(Ordering::Relaxed),
            dedup_cosine: DEDUP_COSINE.load(Ordering::Relaxed),
        };
        let (baseline_r1, baseline_r5) = self.measure_recall(&current, &oracle, engine, reranker).await;
        let mut best_score = 0.6 * baseline_r1 + 0.4 * baseline_r5;
        let mut best_params = current;

        for _ in 0..max_experiments {
            let step = EXPERIMENT_STEP.fetch_add(1, Ordering::Relaxed);
            let candidate = self.suggest_mutation(&best_params, step);
            let (r1, r5) = self.measure_recall(&candidate, &oracle, engine, reranker).await;
            let score = 0.6 * r1 + 0.4 * r5;
            let improved = score > best_score + 0.01;
            let note = if improved {
                format!("KEPT score {:.1}%→{:.1}% (r@1 {:.0}% r@5 {:.0}%) pool={} win={}",
                    best_score*100.0, score*100.0, r1*100.0, r5*100.0,
                    candidate.candidate_pool_mult, candidate.rerank_window)
            } else {
                format!("DISCARD score {:.1}% (best {:.1}%) (r@1 {:.0}% r@5 {:.0}%) pool={} win={}",
                    score*100.0, best_score*100.0, r1*100.0, r5*100.0,
                    candidate.candidate_pool_mult, candidate.rerank_window)
            };

            info!("🧪 IdleLab exp step={}: {}", step % 8, note);

            if improved {
                best_score = score;
                best_params = candidate.clone();
                CANDIDATE_POOL_MULT.store(candidate.candidate_pool_mult, Ordering::Relaxed);
                RERANK_WINDOW.store(candidate.rerank_window, Ordering::Relaxed);
                SEMANTIC_WEIGHT.store(candidate.semantic_weight, Ordering::Relaxed);
                DEDUP_COSINE.store(candidate.dedup_cosine, Ordering::Relaxed);
                self.save_best_params(&candidate);
            }

            let result = ExperimentResult {
                params: candidate,
                recall_at_1: r1,
                recall_at_5: r5,
                kept: improved,
                timestamp: chrono::Utc::now().timestamp(),
                note,
            };
            self.log_result(&result);
        }

        info!("🧪 IdleLab: done. Best score={:.1}% (params: pool={} win={} s_w={} dedup={})",
            best_score * 100.0,
            CANDIDATE_POOL_MULT.load(Ordering::Relaxed),
            RERANK_WINDOW.load(Ordering::Relaxed),
            SEMANTIC_WEIGHT.load(Ordering::Relaxed),
            DEDUP_COSINE.load(Ordering::Relaxed));
    }

    /// Propose one param mutation via simple hill climbing.
    /// Cycles through Grupo A (pool, window) and Grupo B (semantic_weight, dedup_cosine).
    fn suggest_mutation(&self, current: &RetrievalParams, step: usize) -> RetrievalParams {
        let mut candidate = current.clone();
        match step % 8 {
            0 => candidate.candidate_pool_mult = (current.candidate_pool_mult + 5).min(40),
            1 => candidate.candidate_pool_mult = current.candidate_pool_mult.saturating_sub(5).max(10),
            2 => candidate.rerank_window = (current.rerank_window + 10).min(80),
            3 => candidate.rerank_window = current.rerank_window.saturating_sub(10).max(20),
            4 => candidate.semantic_weight = (current.semantic_weight + 5).min(90),
            5 => candidate.semantic_weight = current.semantic_weight.saturating_sub(5).max(30),
            6 => candidate.dedup_cosine = (current.dedup_cosine + 2).min(98),
            _ => candidate.dedup_cosine = current.dedup_cosine.saturating_sub(2).max(80),
        }
        candidate
    }

    /// Measure Recall@1 and Recall@5 against the given oracle.
    /// candidate_pool = EVAL_LIMIT * params.candidate_pool_mult fed into search_hybrid.
    /// After RRF, the top rerank_window candidates are scored.
    async fn measure_recall(
        &self,
        params: &RetrievalParams,
        oracle: &[OraclePair],
        engine: Option<&EmbeddingEngine>,
        reranker: Option<&RerankEngine>,
    ) -> (f64, f64) {
        const EVAL_LIMIT: usize = 5;
        let mut hit1 = 0usize;
        let mut hit5 = 0usize;
        let n = oracle.len();

        for pair in oracle {
            let pool_size = (EVAL_LIMIT * params.candidate_pool_mult).max(50);
            let embedding = engine.and_then(|e| e.embed(&pair.query).ok());

            let results = match self.silva.search_hybrid(
                &pair.query,
                embedding.as_deref(),
                pool_size,
                None,
                false,
            ).await {
                Ok(r) => r,
                Err(e) => { warn!("IdleLab search error: {}", e); continue; }
            };

            let reranked: Vec<(crate::memory::silva::GraphNode, f32)> = if let Some(r) = reranker {
                let rerank_pool: Vec<_> = results.iter().take(params.rerank_window).collect();
                let texts: Vec<&str> = rerank_pool.iter().map(|(n, _)| n.content.as_str()).collect();
                if let Ok(ranked) = r.rerank(&pair.query, &texts) {
                    let mut reordered: Vec<_> = ranked.into_iter()
                        .filter_map(|(idx, logit)| {
                            let norm = 1.0f32 / (1.0 + (-logit).exp());
                            rerank_pool.get(idx).map(|&(n, _)| (n.clone(), norm))
                        })
                        .collect();
                    reordered.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                    reordered
                } else {
                    results.iter().map(|(n, s)| (n.clone(), *s)).collect()
                }
            } else {
                results.iter().map(|(n, s)| (n.clone(), *s)).collect()
            };

            let top5: Vec<_> = reranked.iter().take(EVAL_LIMIT).collect();

            let expected_id_lower = pair.expected_id.to_lowercase();
            let hit_fn = |(node, _): &&(crate::memory::silva::GraphNode, f32)| {
                node.id.to_lowercase() == expected_id_lower
            };

            if top5.first().map(hit_fn).unwrap_or(false) {
                hit1 += 1;
            }
            if top5.iter().any(hit_fn) {
                hit5 += 1;
            }
        }

        (hit1 as f64 / n as f64, hit5 as f64 / n as f64)
    }

    fn save_best_params(&self, params: &RetrievalParams) {
        let json = serde_json::json!({
            "candidate_pool_mult": params.candidate_pool_mult,
            "rerank_window": params.rerank_window,
            "semantic_weight": params.semantic_weight,
            "dedup_cosine": params.dedup_cosine,
            "updated_at": chrono::Utc::now().to_rfc3339(),
        });
        if let Err(e) = std::fs::write(&self.best_path, json.to_string()) {
            warn!("IdleLab: failed to save best params: {}", e);
        }
    }

    fn log_result(&self, result: &ExperimentResult) {
        let line = format!("{}\t{}\t{}\t{}\t{}\t{:.4}\t{:.4}\t{}\t{}\n",
            result.timestamp,
            result.params.candidate_pool_mult,
            result.params.rerank_window,
            result.params.semantic_weight,
            result.params.dedup_cosine,
            result.recall_at_1,
            result.recall_at_5,
            if result.kept { "KEPT" } else { "DISCARD" },
            result.note,
        );

        // Append header if file is new
        if !self.results_path.exists() {
            let header = "timestamp\tpool_mult\trerank_win\tsemantic_weight\tdedup_cosine\tr_at_1\tr_at_5\tdecision\tnote\n";
            let _ = std::fs::write(&self.results_path, header);
        }
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(&self.results_path) {
            let _ = f.write_all(line.as_bytes());
        }
    }

    /// Return last N experiment results as JSON for the status endpoint.
    pub fn get_status(&self) -> serde_json::Value {
        let current = serde_json::json!({
            "candidate_pool_mult": CANDIDATE_POOL_MULT.load(Ordering::Relaxed),
            "rerank_window": RERANK_WINDOW.load(Ordering::Relaxed),
            "semantic_weight": SEMANTIC_WEIGHT.load(Ordering::Relaxed),
            "dedup_cosine": DEDUP_COSINE.load(Ordering::Relaxed),
        });

        let oracle = idle_lab_oracle::load_oracle(&self.oracle_path);

        let history: Vec<serde_json::Value> = self.read_last_results(10)
            .into_iter()
            .map(|line| serde_json::json!({"raw": line}))
            .collect();

        serde_json::json!({
            "current_params": current,
            "oracle_queries": oracle.len(),
            "oracle_source": self.oracle_path.to_string_lossy(),
            "results_file": self.results_path.to_string_lossy(),
            "history": history,
            "group_a": {"pool_mult": "10-40", "rerank_window": "20-80"},
            "group_b": {"semantic_weight": "30-90", "dedup_cosine": "80-98", "routing_bonuses": "exposed as BUILDER_BONUS/SCHOLAR_BONUS/WARDEN_BONUS atomics"},
            "note": "IdleLab optimizes retrieval params during NightConsolidation idle cycles. M8-B: dynamic oracle from JSON, composite score 0.6*R@1+0.4*R@5."
        })
    }

    fn read_last_results(&self, n: usize) -> Vec<String> {
        std::fs::read_to_string(&self.results_path)
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.starts_with("timestamp"))
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .take(n)
            .collect()
    }
}
