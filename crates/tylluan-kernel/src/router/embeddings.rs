//! # Embedding Engine
//!
//! Provides text-to-vector embeddings for semantic search using FastEmbed (ONNX).
//!
//! ## Supported Models
//!
//! | Config value       | Model                          | Dim  | Size  |
//! |--------------------|--------------------------------|------|-------|
//! | `bge-m3` (default) | BAAI/bge-m3                    | 1024 | ~1.2G |
//! | `bge-small`        | BAAI/bge-small-en-v1.5         | 384  | ~67M  |
//! | `minilm`           | all-MiniLM-L6-v2               | 384  | ~90M  |
//! | `nomic-embed-text` | nomic-ai/nomic-embed-text-v1.5 | 768  | ~274M |

use anyhow::{Result, Context, anyhow};
use fastembed::{TextEmbedding, TextInitOptions, EmbeddingModel, TextRerank, RerankInitOptions, RerankerModel, ExecutionProviderDispatch};
use std::sync::Mutex;
use tracing::{info, warn};
use crate::config::InferenceDevice;

/// Embedding engine for semantic search.
pub struct EmbeddingEngine {
    model: Mutex<TextEmbedding>,
    model_type: String,
    dimension: u32,
}

/// Resolve fastembed model enum from config string.
pub fn resolve_model(embedding_model: &str) -> EmbeddingModel {
    let lower = embedding_model.to_lowercase();
    if lower.contains("nomic") {
        EmbeddingModel::NomicEmbedTextV15
    } else if lower.contains("minilm") {
        EmbeddingModel::AllMiniLML6V2
    } else if lower.contains("bge-small") {
        EmbeddingModel::BGESmallENV15
    } else if lower.contains("bge") {
        EmbeddingModel::BGEM3
    } else {
        EmbeddingModel::BGEM3
    }
}

/// Resolve output vector dimension from config string.
pub fn resolve_dimension(embedding_model: &str) -> u32 {
    if embedding_model.is_empty() || embedding_model == "none" {
        return 0;
    }
    let lower = embedding_model.to_lowercase();
    if lower.contains("bge-m3") || lower == "bge" {
        1024
    } else if lower.contains("nomic") {
        768
    } else if lower.contains("minilm") || lower.contains("bge-small") {
        384
    } else {
        1024
    }
}

/// Human-readable model name for logs.
fn model_display_name(embedding_model: &str) -> &'static str {
    let lower = embedding_model.to_lowercase();
    if lower.contains("bge-m3") {
        "BGE-M3"
    } else if lower.contains("bge-small") {
        "BGE-Small"
    } else if lower.contains("bge") {
        "BGE"
    } else if lower.contains("minilm") {
        "MiniLM-L6-v2"
    } else if lower.contains("nomic") {
        "Nomic-Embed-v1.5"
    } else {
        "BGE-M3"
    }
}

/// Model type string for engine_id().
fn resolve_model_type(embedding_model: &str) -> String {
    let lower = embedding_model.to_lowercase();
    if lower.contains("bge-m3") {
        "bge-m3"
    } else if lower.contains("bge-small") {
        "bge-small"
    } else if lower.contains("bge") {
        "bge"
    } else if lower.contains("minilm") {
        "minilm"
    } else if lower.contains("nomic") {
        "nomic"
    } else {
        "bge-m3"
    }.to_string()
}

impl EmbeddingEngine {
    /// Initialize the embedding engine using fastembed.
    pub fn load(model_name: &str) -> Result<Self> {
        Self::load_with_device(model_name, &InferenceDevice::Cpu)
    }

    /// Initialize with an explicit execution device (cpu / directml / cuda).
    pub fn load_with_device(model_name: &str, device: &InferenceDevice) -> Result<Self> {
        let model = resolve_model(model_name);
        let dimension = resolve_dimension(model_name);
        let model_label = model_display_name(model_name);
        info!("🧠 Loading {} engine (FastEmbed v5) dim:{} device:{:?}", model_label, dimension, device);

        let eps = build_execution_providers(device);
        let options = TextInitOptions::new(model)
            .with_show_download_progress(true)
            .with_execution_providers(eps);

        let text_model = TextEmbedding::try_new(options)
            .map_err(|e| anyhow!("FastEmbed init failed: {:?}", e))?;

        let model_type = resolve_model_type(model_name);
        info!("🧠 {} engine ready (ONNX)", model_type.to_uppercase());

        Ok(Self {
            model: Mutex::new(text_model),
            model_type,
            dimension,
        })
    }

    /// Check if model weights exist (Not strictly needed for fastembed as it auto-downloads).
    pub fn ensure_provisioned(_model_dir: &str) -> Result<()> {
        Ok(())
    }

    /// Resolve model path from config string.
    /// Returns None if `embedding_model` is "none" or empty (BM25-only mode).
    pub fn model_path_from_config(embedding_model: &str) -> Option<String> {
        if embedding_model.is_empty() || embedding_model == "none" {
            return None;
        }
        Some(format!("models/{}", embedding_model))
    }

    /// Get the output vector dimension for this engine.
    pub fn dimension(&self) -> u32 {
        self.dimension
    }

    /// Embed a text string into a vector.
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // FastEmbed processes immediately
        let mut model = self.model.lock().unwrap_or_else(|e| e.into_inner());
        let embeddings = model.embed(vec![text], None)
            .map_err(|e| anyhow!("Inference failed: {:?}", e))?;
        
        // It returns a list of embeddings (one per text). We take the first.
        let mut vector = embeddings.into_iter().next()
            .context("No embedding returned")?;

        // BGE-M3 produces 1024-dim vectors natively — no truncation needed

        // L2 Normalization (FastEmbed might already do it, but we strictly enforce for Cosine Similarity)
        let norm: f32 = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 1e-6 {
            for val in &mut vector {
                *val /= norm;
            }
        }

        Ok(vector)
    }

    /// Get a unique ID for the current embedding engine
    pub fn engine_id(&self) -> String {
        format!("{}-v2-onnx", self.model_type)
    }

    /// Get a hash of the current weights
    pub fn engine_hash(&self) -> Option<String> {
        None
    }
}

/// Build execution provider list for fastembed based on configured device.
/// Falls back to CPU automatically if the requested EP is unavailable at runtime.
fn build_execution_providers(device: &InferenceDevice) -> Vec<ExecutionProviderDispatch> {
    match device {
        InferenceDevice::Cpu => {
            info!("🧠 Inference device: CPU (default)");
            vec![]
        }
        InferenceDevice::Directml => {
            #[cfg(target_os = "windows")]
            {
                use ort::execution_providers::DirectMLExecutionProvider;
                info!("🚀 Inference device: DirectML (GPU accelerated)");
                vec![DirectMLExecutionProvider::default().build()]
            }
            #[cfg(not(target_os = "windows"))]
            {
                warn!("⚠️  DirectML requested but not on Windows — falling back to CPU");
                vec![]
            }
        }
        InferenceDevice::Cuda => {
            #[cfg(feature = "cuda")]
            {
                use ort::execution_providers::CUDAExecutionProvider;
                info!("🚀 Inference device: CUDA (GPU accelerated)");
                vec![CUDAExecutionProvider::default().build()]
            }
            #[cfg(not(feature = "cuda"))]
            {
                warn!("⚠️  CUDA requested but feature not enabled — falling back to CPU");
                vec![]
            }
        }
    }
}

/// Cross-encoder reranker. Takes (query, document) pairs and scores relevance directly.
/// More accurate than bi-encoder similarity — use on top-N RRF candidates.
pub struct RerankEngine {
    model: Mutex<TextRerank>,
}

impl RerankEngine {
    pub fn load() -> Result<Self> {
        Self::load_with_device(&InferenceDevice::Cpu)
    }

    /// M25-A: the cross-encoder is the real latency bottleneck of recall
    /// (40-50 pairs/query) — it needs the GPU even more than the bi-encoder.
    pub fn load_with_device(device: &InferenceDevice) -> Result<Self> {
        // R22-1: Jina Turbo replaces BGERerankerBase (~278M→~37M params)
        info!("🔀 Loading Jina Turbo reranker (ONNX) — device: {:?}", device);
        let eps = build_execution_providers(device);
        let options = RerankInitOptions::new(RerankerModel::JINARerankerV1TurboEn)
            .with_execution_providers(eps);
        let model = TextRerank::try_new(options)
            .map_err(|e| anyhow!("Reranker init failed: {:?}", e))?;
        info!("🔀 Jina Turbo reranker ready");
        Ok(Self { model: Mutex::new(model) })
    }

    /// Rerank documents against query. Returns indices sorted by relevance descending.
    pub fn rerank(&self, query: &str, documents: &[&str]) -> Result<Vec<(usize, f32)>> {
        if documents.is_empty() { return Ok(vec![]); }
        let mut model = self.model.lock().map_err(|_| anyhow!("reranker mutex poisoned"))?;
        let results = model.rerank(query, documents, false, None)
            .map_err(|e| anyhow!("Rerank failed: {:?}", e))?;
        let mut indexed: Vec<(usize, f32)> = results.iter()
            .map(|r| (r.index, r.score))
            .collect();
        indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(indexed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_path() {
        let path = EmbeddingEngine::model_path_from_config("bge-m3");
        assert!(path.is_some());
        let none_path = EmbeddingEngine::model_path_from_config("none");
        assert!(none_path.is_none());
    }

    #[test]
    fn test_resolve_dimension() {
        assert_eq!(resolve_dimension("bge-m3"), 1024);
        assert_eq!(resolve_dimension("bge-small"), 384);
        assert_eq!(resolve_dimension("minilm"), 384);
        assert_eq!(resolve_dimension("nomic-embed-text"), 768);
        assert_eq!(resolve_dimension("none"), 0);
        assert_eq!(resolve_dimension(""), 0);
    }

    #[test]
    #[ignore]
    fn test_real_inference_bge_m3() {
        let engine = EmbeddingEngine::load("bge-m3").expect("Failed to load engine");
        let vector = engine.embed("Hello, TylluanNexus sovereignty").expect("Inference failed");
        assert_eq!(vector.len(), 1024, "BGE-M3 should produce 1024-dim vectors");
    }

    #[test]
    #[ignore]
    fn test_real_inference_minilm() {
        let engine = EmbeddingEngine::load("minilm").expect("Failed to load engine");
        let vector = engine.embed("Hello from portable mode").expect("Inference failed");
        assert_eq!(vector.len(), 384, "MiniLM should produce 384-dim vectors");
        assert_eq!(engine.dimension(), 384);
    }
}
