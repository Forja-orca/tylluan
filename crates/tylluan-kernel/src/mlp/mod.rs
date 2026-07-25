pub mod replay;

use ndarray::Array2;
use ort::session::Session;
use ort::value::TensorRef;
use std::path::Path;
#[cfg(not(test))]
use std::path::PathBuf;
use tracing::info;

pub struct MlpScorer {
    session: Option<std::sync::Mutex<Session>>,
}

impl MlpScorer {
    pub fn new(models_dir: &Path) -> Self {
        let path = models_dir.join("complexity_mlp.onnx");
        if path.exists() {
            return Self::load_from(&path);
        }
        #[cfg(not(test))]
        if let Some(workspace_root) = cargo_workspace_root() {
            let ws_path = workspace_root.join("models").join("complexity_mlp.onnx");
            if ws_path.exists() {
                return Self::load_from(&ws_path);
            }
        }
        info!("MLP model not found at {:?} — MLP scoring disabled", path);
        Self { session: None }
    }

    fn load_from(path: &Path) -> Self {
        match Session::builder()
            .and_then(|b| b.commit_from_file(path))
        {
            Ok(session) => {
                info!("MLP scorer loaded from {:?}", path);
                Self { session: Some(std::sync::Mutex::new(session)) }
            }
            Err(e) => {
                info!("MLP scorer failed to load from {:?}: {e} — MLP scoring disabled", path);
                Self { session: None }
            }
        }
    }

    pub fn score(&self, features: &[f32]) -> Option<f64> {
        let session = self.session.as_ref()?;
        let mut session_guard = session.lock().ok()?;
        let input = Array2::from_shape_vec((1, features.len()), features.to_vec()).ok()?;
        let tensor = TensorRef::from_array_view(input.view()).ok()?;
        let outputs = session_guard.run(ort::inputs![tensor]).ok()?;
        let (_shape, data) = outputs[0].try_extract_tensor::<f32>().ok()?;
        data.first().copied().map(|v| v.clamp(0.0, 1.0) as f64)
    }

    pub fn is_active(&self) -> bool {
        self.session.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mlp_no_model_graceful_degradation() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let scorer = MlpScorer::new(tmp.path());
        assert!(!scorer.is_active(), "scorer should be inactive when model absent");
        assert!(scorer.score(&[0.1; 6]).is_none(), "score should be None when inactive");
    }

    #[test]
    fn test_mlp_is_active_false_by_default() {
        let scorer = MlpScorer { session: None };
        assert!(!scorer.is_active());
    }

    #[test]
    fn test_mlp_fallback_disabled_in_test_mode() {
        // When not (test), the fallback to workspace models/ would fire.
        // But we cfg-gated it away, so no model should be found.
        let tmp = tempfile::tempdir().expect("tempdir");
        let scorer = MlpScorer::new(tmp.path());
        assert!(!scorer.is_active(), "fallback should be disabled in test builds");
    }
}

/// Resolve the workspace root by navigating up from CARGO_MANIFEST_DIR.
#[cfg(not(test))]
fn cargo_workspace_root() -> Option<PathBuf> {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    // Walk up until we find a directory containing a top-level Cargo.toml with [workspace]
    let mut current = Some(manifest);
    while let Some(dir) = current {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
                if content.contains("[workspace]") {
                    return Some(dir.to_path_buf());
                }
            }
        }
        current = dir.parent();
    }
    None
}