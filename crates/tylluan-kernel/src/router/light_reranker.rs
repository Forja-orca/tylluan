//! ADR-011 §Learned LightReranker (P1). A tiny FFN (4 -> 16 -> 1, ~100
//! trainable params, <10KB ONNX) that reorders `search_hybrid` candidates
//! using a feature vector RRF alone can't see: per-agent affinity learned
//! from the Signal Loop (`recall_feedback`).
//!
//! Same load/degrade shape as `mlp::MlpScorer` from the complexity-cascade
//! experiment: if no trained model exists yet (which is the honest default
//! state until `recall_feedback` accumulates the >=5,000 resolved rows
//! ADR-011 §3.3 requires), `score()` returns `None` and callers fall back
//! to RRF untouched. Not wired into `search_hybrid`'s signature — with no
//! real trained model yet, changing a 17-call-site function signature for
//! an always-None reranker is exactly the premature-complexity this project
//! avoids. `rerank()` is an additive, opt-in wrapper any call site can adopt
//! once ADR-011's Fase 3-4 data threshold is actually met.

use ndarray::Array2;
use ort::session::Session;
use ort::value::TensorRef;
use std::path::Path;

/// Feature vector for one recall candidate, in the exact order the ONNX
/// model expects. `agent_affinity` defaults to 0.0 until the trainer (not
/// built yet — depends on real recall_feedback data) computes it.
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
}

impl LightReranker {
    pub fn new(models_dir: &Path) -> Self {
        let path = models_dir.join("light_reranker.onnx");
        if !path.exists() {
            return Self { session: None };
        }
        match Session::builder().and_then(|b| b.commit_from_file(&path)) {
            Ok(session) => {
                tracing::info!("LightReranker loaded from {:?}", path);
                Self { session: Some(std::sync::Mutex::new(session)) }
            }
            Err(e) => {
                tracing::info!("LightReranker failed to load from {:?}: {e} — reranking disabled", path);
                Self { session: None }
            }
        }
    }

    pub fn is_active(&self) -> bool {
        self.session.is_some()
    }

    pub fn score(&self, features: &RerankFeatures) -> Option<f32> {
        let session = self.session.as_ref()?;
        let mut guard = session.lock().ok()?;
        let arr = features.to_array();
        let input = Array2::from_shape_vec((1, 4), arr.to_vec()).ok()?;
        let tensor = TensorRef::from_array_view(input.view()).ok()?;
        let outputs = guard.run(ort::inputs![tensor]).ok()?;
        let (_shape, data) = outputs[0].try_extract_tensor::<f32>().ok()?;
        data.first().copied()
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
}
