//! ADR-011 §Learned LightReranker (P1). A tiny FFN (4 -> 16 -> 1, ~100
//! trainable params, <10KB ONNX) that reorders `search_hybrid` candidates
//! using a feature vector RRF alone can't see: per-agent affinity learned
//! from the Signal Loop (`recall_feedback`).
//!
//! Two backends tried in order at construction:
//! 1. ONNX model at `models_dir/light_reranker.onnx`
//! 2. Native JSON weights at `models_dir/light_reranker.weights`
//!
//! If neither exists the reranker is inactive and all calls are no-ops
//! (RRF order preserved).

use ndarray::{Array1, Array2};
use ort::session::Session;
use ort::value::TensorRef;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Serializable weights produced by the NightConsolidation trainer.
/// FFN: 4 inputs → hidden_size → 1 output.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LightRerankerWeights {
    pub w1: Vec<f32>,
    pub b1: Vec<f32>,
    pub w2: Vec<f32>,
    pub b2: f32,
    pub hidden_size: usize,
}

/// Feature vector for one recall candidate, in the exact order the ONNX
/// model expects. `agent_affinity` defaults to 0.0 until the trainer
/// computes it from resolved recall_feedback data.
pub struct RerankFeatures {
    pub score_rrf: f32,
    pub score_graph: f32,
    pub recency_score: f32,
    pub agent_affinity: f32,
}

impl RerankFeatures {
    pub fn to_array(&self) -> [f32; 4] {
        [self.score_rrf, self.score_graph, self.recency_score, self.agent_affinity]
    }
}

pub struct LightReranker {
    session: Option<std::sync::Mutex<Session>>,
    weights: Option<LightRerankerWeights>,
}

impl LightReranker {
    /// Check if ANY reranker model file exists (ONNX or weights).
    pub fn exists(models_dir: &Path) -> bool {
        models_dir.join("light_reranker.onnx").exists()
            || models_dir.join("light_reranker.weights").exists()
    }

    /// Dirección #3: restore last-good weights over the current ones. Returns
    /// true if a backup existed and was promoted. Call when production recall
    /// quality degrades after a night-training write.
    pub fn restore_last_good(models_dir: &Path) -> bool {
        let weights_path = models_dir.join("light_reranker.weights");
        let backup = models_dir.join("light_reranker.weights.bak");
        if backup.exists() {
            std::fs::copy(&backup, &weights_path).is_ok()
        } else {
            false
        }
    }

    pub fn new(models_dir: &Path) -> Self {
        let path = models_dir.join("light_reranker.onnx");
        if path.exists() {
            match Session::builder().and_then(|b| b.commit_from_file(&path)) {
                Ok(session) => {
                    tracing::info!("LightReranker loaded from {:?}", path);
                    return Self { session: Some(std::sync::Mutex::new(session)), weights: None };
                }
                Err(e) => {
                    tracing::info!("LightReranker failed to load from {:?}: {e} — trying native weights", path);
                }
            }
        }

        let weights_path = models_dir.join("light_reranker.weights");
        if weights_path.exists() {
            match std::fs::read_to_string(&weights_path) {
                Ok(json) => match serde_json::from_str::<LightRerankerWeights>(&json) {
                    Ok(weights) => {
                        tracing::info!("LightReranker loaded native weights from {:?}", weights_path);
                        return Self { session: None, weights: Some(weights) };
                    }
                    Err(e) => {
                        tracing::info!("LightReranker failed to parse weights from {:?}: {e}", weights_path);
                    }
                },
                Err(e) => {
                    tracing::info!("LightReranker failed to read weights from {:?}: {e}", weights_path);
                }
            }
        }

        Self { session: None, weights: None }
    }

    pub fn is_active(&self) -> bool {
        self.session.is_some() || self.weights.is_some()
    }

    pub fn score(&self, features: &RerankFeatures) -> Option<f32> {
        if let Some(ref session) = self.session {
            let mut guard = session.lock().ok()?;
            let arr = features.to_array();
            let input = Array2::from_shape_vec((1, 4), arr.to_vec()).ok()?;
            let tensor = TensorRef::from_array_view(input.view()).ok()?;
            let outputs = guard.run(ort::inputs![tensor]).ok()?;
            let (_shape, data) = outputs[0].try_extract_tensor::<f32>().ok()?;
            return data.first().copied();
        }

        if let Some(ref w) = self.weights {
            return Some(score_native(features, w));
        }

        None
    }

    /// Reorders `candidates` by `score()` when active, else returns them
    /// unchanged (RRF order preserved). Never fails, never blocks callers
    /// that don't care about this feature.
    pub fn rerank(&self, candidates: Vec<(RerankFeatures, usize)>) -> Vec<usize> {
        if !self.is_active() {
            return candidates.into_iter().map(|(_, idx)| idx).collect();
        }
        let mut scored: Vec<(f32, usize)> = candidates
            .into_iter()
            .map(|(f, idx)| (self.score(&f).unwrap_or(f.score_rrf), idx))
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.into_iter().map(|(_, idx)| idx).collect()
    }
}

/// Native forward pass: 4 → hidden_size (ReLU) → 1 (sigmoid).
fn score_native(features: &RerankFeatures, w: &LightRerankerWeights) -> f32 {
    let x = Array1::from_vec(features.to_array().to_vec());
    let w1 = match Array2::from_shape_vec((4, w.hidden_size), w.w1.clone()) {
        Ok(m) => m,
        _ => return features.score_rrf,
    };
    let w2 = match Array2::from_shape_vec((w.hidden_size, 1), w.w2.clone()) {
        Ok(m) => m,
        _ => return features.score_rrf,
    };
    let b1 = Array1::from_vec(w.b1.clone());

    let hidden = x.dot(&w1) + &b1;
    let hidden = hidden.mapv(|v| v.max(0.0));
    let logit_arr = hidden.dot(&w2);
    let logit = logit_arr[0] + w.b2;
    1.0 / (1.0 + (-logit).exp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_model_is_inactive_and_preserves_order() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let reranker = LightReranker::new(tmp.path());
        assert!(!reranker.is_active());
        assert!(reranker.score(&RerankFeatures { score_rrf: 0.5, score_graph: 0.1, recency_score: 0.9, agent_affinity: 0.0 }).is_none());

        let candidates = vec![
            (RerankFeatures { score_rrf: 0.9, score_graph: 0.0, recency_score: 0.0, agent_affinity: 0.0 }, 0),
            (RerankFeatures { score_rrf: 0.1, score_graph: 0.0, recency_score: 0.0, agent_affinity: 0.0 }, 1),
        ];
        assert_eq!(reranker.rerank(candidates), vec![0, 1], "inactive reranker must preserve input (RRF) order");
    }

    #[test]
    fn rerank_features_to_array_preserves_order() {
        let f = RerankFeatures { score_rrf: 1.0, score_graph: 2.0, recency_score: 3.0, agent_affinity: 4.0 };
        assert_eq!(f.to_array(), [1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn native_weights_score_produces_valid_output() {
        let weights = LightRerankerWeights {
            w1: vec![0.1; 64],
            b1: vec![0.0; 16],
            w2: vec![0.2; 16],
            b2: 0.0,
            hidden_size: 16,
        };
        let tmp = tempfile::tempdir().expect("tempdir");
        let weights_path = tmp.path().join("light_reranker.weights");
        std::fs::write(&weights_path, serde_json::to_vec(&weights).unwrap()).unwrap();

        let reranker = LightReranker::new(tmp.path());
        assert!(reranker.is_active(), "should be active with native weights file");

        let score = reranker.score(&RerankFeatures { score_rrf: 0.5, score_graph: 0.3, recency_score: 0.9, agent_affinity: 0.2 });
        assert!(score.is_some(), "native weights must produce a score");
        let s = score.unwrap();
        assert!((0.0..=1.0).contains(&s), "sigmoid output must be in [0,1], got {s}");
    }

    #[test]
    fn native_weights_gives_higher_score_for_better_features() {
        let weights = LightRerankerWeights {
            w1: vec![1.0; 64],
            b1: vec![0.0; 16],
            w2: vec![1.0; 16],
            b2: 0.0,
            hidden_size: 16,
        };
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(tmp.path().join("light_reranker.weights"), serde_json::to_vec(&weights).unwrap()).unwrap();
        let reranker = LightReranker::new(tmp.path());

        let low = reranker.score(&RerankFeatures { score_rrf: 0.1, score_graph: 0.1, recency_score: 0.1, agent_affinity: 0.1 }).unwrap();
        let high = reranker.score(&RerankFeatures { score_rrf: 0.9, score_graph: 0.9, recency_score: 0.9, agent_affinity: 0.9 }).unwrap();
        assert!(high > low, "better features must produce higher score: low={low} high={high}");
    }

    #[test]
    fn corrupted_weights_file_falls_back_gracefully() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(tmp.path().join("light_reranker.weights"), b"not valid json").unwrap();
        let reranker = LightReranker::new(tmp.path());
        assert!(!reranker.is_active(), "corrupted weights must not activate the reranker");
    }

    #[test]
    fn rerank_active_with_weights_reorders_by_score() {
        let weights = LightRerankerWeights {
            w1: vec![1.0; 64],
            b1: vec![0.0; 16],
            w2: vec![1.0; 16],
            b2: 0.0,
            hidden_size: 16,
        };
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(tmp.path().join("light_reranker.weights"), serde_json::to_vec(&weights).unwrap()).unwrap();
        let reranker = LightReranker::new(tmp.path());
        assert!(reranker.is_active());

        let candidates = vec![
            (RerankFeatures { score_rrf: 0.1, score_graph: 0.1, recency_score: 0.1, agent_affinity: 0.1 }, 0),
            (RerankFeatures { score_rrf: 0.9, score_graph: 0.9, recency_score: 0.9, agent_affinity: 0.9 }, 1),
        ];
        let reordered = reranker.rerank(candidates);
        assert_eq!(reordered, vec![1, 0], "higher-score candidate should be ranked first");
    }
}
