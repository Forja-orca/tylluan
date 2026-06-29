//! # Embedding Engine
//!
//! Provides text-to-vector embeddings for semantic search using FastEmbed (ONNX).
//!
//! ## Current Status
//!
//! **Sovereign Mode**: Uses fastembed with BGE-M3 (multilingual, 100+ languages).

use anyhow::{Result, Context, anyhow};
use fastembed::{TextEmbedding, TextInitOptions, EmbeddingModel, TextRerank, RerankInitOptions, RerankerModel, ExecutionProviderDispatch};
use std::sync::Mutex;
use tracing::{info, warn};
use crate::config::InferenceDevice;

/// Embedding engine for semantic search.
pub struct EmbeddingEngine {
    model: Mutex<TextEmbedding>,
    model_type: String,
}

impl EmbeddingEngine {
    /// Initialize the embedding engine using fastembed.
    pub fn load(model_dir: &str) -> Result<Self> {
        Self::load_with_device(model_dir, &InferenceDevice::Cpu)
    }

    /// Initialize with an explicit execution device (cpu / directml / cuda).
    pub fn load_with_device(model_dir: &str, device: &InferenceDevice) -> Result<Self> {
        info!("🧠 Loading Sovereign AI engine (FastEmbed v5) hint: {} device: {:?}", model_dir, device);

        let model_name = if model_dir.contains("bge-m3") || model_dir.contains("m3") {
            EmbeddingModel::BGEM3
        } else if model_dir.contains("bge") {
            EmbeddingModel::BGEBaseENV15
        } else if model_dir.contains("nomic") {
            EmbeddingModel::NomicEmbedTextV15
        } else {
            EmbeddingModel::BGEM3
        };

        let eps = build_execution_providers(device);
        let options = TextInitOptions::new(model_name)
            .with_show_download_progress(true)
            .with_execution_providers(eps);

        let model = TextEmbedding::try_new(options)
            .map_err(|e| anyhow!("FastEmbed init failed: {:?}", e))?;

        let model_type = if model_dir.contains("bge-m3") || model_dir.contains("m3") || (!model_dir.contains("bge") && !model_dir.contains("nomic")) {
            "bge-m3".to_string()
        } else if model_dir.contains("bge") {
            "bge".to_string()
        } else {
            "nomic".to_string()
        };
        info!("🧠 Sovereign {} engine ready (ONNX)", model_type.to_uppercase());

        Ok(Self {
            model: Mutex::new(model),
            model_type,
        })
    }

    /// Check if model weights exist (Not strictly needed for fastembed as it auto-downloads).
    pub fn ensure_provisioned(_model_dir: &str) -> Result<()> {
        Ok(())
    }

    /// Find the default model directory.
    pub fn default_model_path() -> Option<String> {
        Some("models/bge-m3".to_string())
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
        let path = EmbeddingEngine::default_model_path();
        println!("Effective model path: {:?}", path);
    }

    #[test]
    #[ignore]
    fn test_real_inference() {
        let path = "models/bge-m3".to_string();
        let engine = EmbeddingEngine::load(&path).expect("Failed to load engine");
        let vector = engine.embed("Hello, TylluanNexus sovereignty").expect("Inference failed");
        
        println!("Vector dimension: {}", vector.len());
        assert_eq!(vector.len(), 1024, "BGE-M3 should produce 1024-dim vectors");
    }
}
