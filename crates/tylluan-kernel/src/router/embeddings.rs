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
use fastembed::{TextEmbedding, TextInitOptions, EmbeddingModel, TextRerank, RerankInitOptions, RerankerModel, ExecutionProviderDispatch, SparseTextEmbedding, SparseInitOptions, SparseModel};
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use tracing::{info, warn};
use crate::config::InferenceDevice;

/// Embedding engine for semantic search.
pub struct EmbeddingEngine {
    model: Mutex<TextEmbedding>,
    model_type: String,
    dimension: u32,
    cache: Mutex<LruCache<String, Vec<f32>>>,
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
    } else {
        // Covers "bge" (full-size BGE-M3) and any unrecognized model name, which
        // defaults to BGE-M3 as the project's baseline embedding model.
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
            .map_err(|e| anyhow!("FastEmbed init failed: {e:?}"))?;

        let model_type = resolve_model_type(model_name);
        info!("🧠 {} engine ready (ONNX)", model_type.to_uppercase());

        Ok(Self {
            model: Mutex::new(text_model),
            model_type,
            dimension,
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(512).unwrap())),
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
        Some(format!("models/{embedding_model}"))
    }

    /// Get the output vector dimension for this engine.
    pub fn dimension(&self) -> u32 {
        self.dimension
    }

    /// Embed a text string into a vector.
    /// Uses an LRU cache (512 slots) to avoid repeated ONNX inference on identical inputs.
    /// Cache hit: <5ms. Cache miss: 2-8s (CPU) / 200-500ms (GPU).
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let cache_key = text.trim().to_lowercase();
        {
            let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(cached) = cache.get(&cache_key) {
                return Ok(cached.clone());
            }
        }
        let mut batch = self.embed_batch(&[text])?;
        let embedding = batch.pop().context("No embedding returned")?;
        {
            let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            cache.put(cache_key, embedding.clone());
        }
        Ok(embedding)
    }

    /// Embed multiple texts in one ONNX batch call.
    /// FastEmbed natively batches — this avoids N sequential inference calls.
    /// Each returned vector is L2-normalized for cosine similarity.
    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let mut model = self.model.lock().unwrap_or_else(|e| e.into_inner());
        let mut embeddings = model.embed(texts, None)
            .map_err(|e| anyhow!("Batch inference failed: {e:?}"))?;

        for vector in &mut embeddings {
            let norm: f32 = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
            if norm > 1e-6 {
                for val in vector.iter_mut() {
                    *val /= norm;
                }
            }
        }

        Ok(embeddings)
    }

    /// Async-safe wrapper: runs the synchronous ONNX inference on the tokio
    /// blocking pool instead of the async worker.
    ///
    /// WHY THIS EXISTS (2026-09-01, live incident): `embed_batch` is a
    /// synchronous, CPU-bound ONNX call (2-8s per batch) that also holds the
    /// engine's std Mutex for the whole inference. Called directly from an
    /// async task, it blocks that tokio worker AND makes every other embed
    /// caller (recall, routing, cascade) queue on the mutex inside async
    /// context, burning one worker each — the Agnostic Reindexer could starve
    /// the runtime until new HTTP requests (even DB-free /health) never got a
    /// worker: TCP established, no response. The blocking pool has its own
    /// threads, so neither the ONNX latency nor the mutex wait consumes async
    /// workers. Every background/inference call site must use this (or
    /// spawn_blocking) — never call `embed_batch` directly from async.
    pub async fn embed_batch_async(
        self: &Arc<Self>,
        texts: Vec<String>,
    ) -> Result<Vec<Vec<f32>>> {
        let this = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            let refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
            this.embed_batch(&refs)
        })
        .await
        .map_err(|e| anyhow!("embed_batch_async join failed: {e}"))?
    }

    /// Single-text async wrapper — same rationale as `embed_batch_async`.
    pub async fn embed_async(self: &Arc<Self>, text: String) -> Result<Vec<f32>> {
        let mut out = self.embed_batch_async(vec![text]).await?;
        out.pop().context("No embedding returned")
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

/// Learned-sparse vector: vocabulary dimension indices + learned lexical weights
/// (BGE-M3 sparse head). Stored and compared as-is; scoring is a dot product over
/// shared indices.
#[derive(Debug, Clone, PartialEq)]
pub struct SparseVec {
    pub indices: Vec<u32>,
    pub values: Vec<f32>,
}

impl SparseVec {
    /// Serialize as [u32 LE indices][f32 LE values] — BLOB-friendly pair.
    pub fn to_bytes(&self) -> (Vec<u8>, Vec<u8>) {
        let idx: Vec<u8> = self.indices.iter().flat_map(|i| i.to_le_bytes()).collect();
        let val: Vec<u8> = self.values.iter().flat_map(|v| v.to_le_bytes()).collect();
        (idx, val)
    }

    pub fn from_bytes(indices_blob: &[u8], values_blob: &[u8]) -> Result<Self> {
        if !indices_blob.len().is_multiple_of(4) || !values_blob.len().is_multiple_of(4) {
            return Err(anyhow!("sparse blob length not multiple of 4"));
        }
        let indices: Vec<u32> = indices_blob
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().expect("4 bytes")))
            .collect();
        let values: Vec<f32> = values_blob
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes(c.try_into().expect("4 bytes")))
            .collect();
        if indices.len() != values.len() {
            return Err(anyhow!(
                "sparse indices/values length mismatch: {} vs {}",
                indices.len(),
                values.len()
            ));
        }
        Ok(Self { indices, values })
    }

    /// Dot product over shared dimension indices (SPLADE-style lexical matching).
    pub fn dot(&self, other: &SparseVec) -> f32 {
        sparse_dot(&self.indices, &self.values, &other.indices, &other.values)
    }
}

/// Pure dot product over parallel index/value vectors. O(n·m); nnz per vector is
/// small (hundreds), fine for linear candidate scans at Tylluan's scale.
pub fn sparse_dot(a_idx: &[u32], a_val: &[f32], b_idx: &[u32], b_val: &[f32]) -> f32 {
    if a_idx.len() > b_idx.len() {
        return sparse_dot(b_idx, b_val, a_idx, a_val);
    }
    use std::collections::HashMap;
    let b_map: HashMap<u32, f32> = b_idx.iter().copied().zip(b_val.iter().copied()).collect();
    a_idx.iter()
        .zip(a_val.iter())
        .filter_map(|(i, av)| b_map.get(i).map(|bv| av * bv))
        .sum()
}

/// Engine for fastembed `SparseTextEmbedding::BGEM3` (BGE-M3 sparse/lexical head).
///
/// Separate ONNX model from the dense engine (~1GB RAM when loaded). Validated by
/// the T289 spike (tests/sparse_signature_spike.rs, commit dbbf910): overlap
/// near-dup=0.58 vs unrelated=0.16 → GO as a retrieval fusion signal.
pub struct SparseEngine {
    model: Mutex<SparseTextEmbedding>,
    cache: Mutex<LruCache<String, SparseVec>>,
}

impl SparseEngine {
    pub const MODEL_ID: &'static str = "bge-m3-sparse";

    pub fn try_new(device: &InferenceDevice) -> Result<Self> {
        let eps = build_execution_providers(device);
        let options = SparseInitOptions::new(SparseModel::BGEM3).with_execution_providers(eps);
        let model = SparseTextEmbedding::try_new(options)
            .map_err(|e| anyhow!("SparseEngine init failed: {e:?}"))?;
        info!("🧠 BGE-M3 sparse engine ready (ONNX)");
        Ok(Self {
            model: Mutex::new(model),
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(512).unwrap())),
        })
    }

    /// Embed text into a learned-sparse vector (LRU-cached like the dense path).
    pub fn embed(&self, text: &str) -> Result<SparseVec> {
        let key = text.trim().to_lowercase();
        {
            let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(hit) = cache.get(&key) {
                return Ok(hit.clone());
            }
        }
        let mut model = self.model.lock().unwrap_or_else(|e| e.into_inner());
        let mut out = model
            .embed(vec![text], None)
            .map_err(|e| anyhow!("Sparse inference failed: {e:?}"))?;
        let emb = out.pop().context("No sparse embedding returned")?;
        // usize → u32: vocabulary dims fit comfortably; clamp defensively.
        let sv = SparseVec {
            indices: emb.indices.into_iter().map(|i| u32::try_from(i).unwrap_or(u32::MAX)).collect(),
            values: emb.values,
        };
        drop(model);
        {
            let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            cache.put(key, sv.clone());
        }
        Ok(sv)
    }

    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<SparseVec>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let mut model = self.model.lock().unwrap_or_else(|e| e.into_inner());
        let out = model
            .embed(texts, None)
            .map_err(|e| anyhow!("Sparse batch inference failed: {e:?}"))?;
        Ok(out.into_iter().map(|emb| SparseVec {
            indices: emb.indices.into_iter().map(|i| u32::try_from(i).unwrap_or(u32::MAX)).collect(),
            values: emb.values,
        }).collect())
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
        InferenceDevice::Coreml => {
            #[cfg(target_os = "macos")]
            {
                use ort::execution_providers::CoreMLExecutionProvider;
                info!("🍎 Inference device: CoreML (Apple GPU/Neural Engine)");
                vec![CoreMLExecutionProvider::default().build()]
            }
            #[cfg(not(target_os = "macos"))]
            {
                warn!("⚠️  CoreML requested but not on macOS — falling back to CPU");
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
            .map_err(|e| anyhow!("Reranker init failed: {e:?}"))?;
        info!("🔀 Jina Turbo reranker ready");
        Ok(Self { model: Mutex::new(model) })
    }

    /// Rerank documents against query. Returns indices sorted by relevance descending.
    pub fn rerank(&self, query: &str, documents: &[&str]) -> Result<Vec<(usize, f32)>> {
        if documents.is_empty() { return Ok(vec![]); }
        let mut model = self.model.lock().map_err(|_| anyhow!("reranker mutex poisoned"))?;
        let results = model.rerank(query, documents, false, None)
            .map_err(|e| anyhow!("Rerank failed: {e:?}"))?;
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

    // CONTRACT-01 invariant: BGE-M3 is ALWAYS 1024 dimensions
    #[test]
    fn contract_01_bge_m3_1024_dimensions() {
        assert_eq!(resolve_dimension("bge-m3"), 1024);
        assert_eq!(resolve_dimension("BGE-M3"), 1024);
        assert_eq!(resolve_dimension("bge"), 1024);
        assert_eq!(resolve_dimension("BGE"), 1024);
        assert_eq!(resolve_dimension(""), 0);
        assert_eq!(resolve_dimension("none"), 0);
    }

    // ---- SparseVec serialization + scoring (no model needed) ----

    #[test]
    fn sparse_vec_roundtrip() {
        let sv = SparseVec { indices: vec![1, 42, 100_000], values: vec![0.5, 2.25, -1.0] };
        let (ib, vb) = sv.to_bytes();
        assert_eq!(ib.len(), 12);
        assert_eq!(vb.len(), 12);
        let back = SparseVec::from_bytes(&ib, &vb).unwrap();
        assert_eq!(sv, back);
    }

    #[test]
    fn sparse_vec_roundtrip_rejects_corrupt() {
        assert!(SparseVec::from_bytes(&[1, 2, 3], &[0; 8]).is_err(), "idx not %4");
        assert!(SparseVec::from_bytes(&[0; 8], &[1, 2, 3]).is_err(), "val not %4");
        assert!(SparseVec::from_bytes(&[0; 8], &[0; 4]).is_err(), "length mismatch");
    }

    #[test]
    fn sparse_dot_shared_indices_only() {
        let a = SparseVec { indices: vec![1, 2, 3], values: vec![1.0, 2.0, 4.0] };
        let b = SparseVec { indices: vec![2, 3, 9], values: vec![0.5, 0.5, 10.0] };
        assert!((a.dot(&b) - 3.0).abs() < 1e-6);
        assert!((a.dot(&b) - b.dot(&a)).abs() < 1e-6, "commutative");
        let disjoint = SparseVec { indices: vec![7, 8], values: vec![1.0, 1.0] };
        assert_eq!(a.dot(&disjoint), 0.0);
        let empty = SparseVec { indices: vec![], values: vec![] };
        assert_eq!(a.dot(&empty), 0.0);
    }
}
