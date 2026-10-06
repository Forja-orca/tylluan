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

/// One queued embedding request: the text to embed and the channel the
/// collector resolves with the result.
struct BatchItem {
    texts: Vec<String>,
    resp: std::sync::mpsc::Sender<Result<Vec<Vec<f32>>, String>>,
}

/// Capacity of the coalescing batcher's request queue — ~16 full coalescing
/// batches at the default max_batch=16. The queue is a bounded `sync_channel`:
/// if the collector stalls (e.g. inference queued on the model mutex or the
/// budget), senders block (backpressure) instead of growing the queue without
/// bound. Explicit overflow REJECTION deliberately lives one layer down in
/// `InferenceBudget::acquire_sync` (`Err(Saturated)`), not here — see the
/// `EmbedBatcher` docs.
const EMBED_QUEUE_CAPACITY: usize = 256;

/// Coalescing batcher (embed-batching contract, T582): a collector thread
/// merges concurrent single-text requests arriving within a short window
/// into ONE `embed_batch` call — N concurrent callers pay one ONNX inference
/// (one mutex acquisition) instead of N serialized ones.
///
/// Queue discipline (corrected 2026-10-01, P0-4): the request channel is a
/// BOUNDED `sync_channel` (`EMBED_QUEUE_CAPACITY`). When it is full the
/// sender BLOCKS until the collector drains one batch — backpressure, never
/// unbounded queue growth. This layer does NOT reject: explicit overflow
/// rejection is `InferenceBudget::acquire_sync`'s job (see `embed_batch`),
/// which returns the `Err(Saturated)` the HTTP layer renders as
/// 503 + Retry-After once an operator opts into `[inference.budget]`.
/// (The old comment claimed "bounded queue with explicit rejection" here,
/// but the channel was `mpsc::channel` — unbounded — and only the budget
/// ever rejected. Both halves of that claim are now true: bounded here,
/// rejection at the budget.)
pub struct EmbedBatcher {
    tx: std::sync::mpsc::SyncSender<BatchItem>,
}

impl EmbedBatcher {
    pub fn spawn(
        engine: Arc<EmbeddingEngine>,
        max_batch: usize,
        window_ms: u64,
    ) -> Result<(Self, std::thread::JoinHandle<()>)> {
        let (tx, rx) = std::sync::mpsc::sync_channel::<BatchItem>(EMBED_QUEUE_CAPACITY);
        let handle = std::thread::Builder::new()
            .name("embed-batcher".to_string())
            .spawn(move || {
                while let Ok(first) = rx.recv() {
                    let mut pending = vec![first];
                    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(window_ms);
                    while pending.len() < max_batch && std::time::Instant::now() < deadline {
                        match rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now())) {
                            Ok(item) => pending.push(item),
                            Err(_) => break,
                        }
                    }
                    let texts: Vec<String> = pending.iter().flat_map(|it| it.texts.clone()).collect();
                    let refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
                    let sizes: Vec<usize> = pending.iter().map(|it| it.texts.len()).collect();
                    let result: Result<Vec<Vec<f32>>, String> = engine.embed_batch(&refs).map_err(|e| e.to_string());
                    // Split failures (model returned the wrong number of
                    // vectors) take the SAME path as inference failures:
                    // every pending request gets the error and the batcher
                    // thread keeps serving — never a panic, because this
                    // thread runs under `panic = "abort"` in release, where
                    // a panic here kills the whole kernel process.
                    let split: Result<Vec<Vec<Vec<f32>>>, String> = result
                        .and_then(|all| split_batch_results(&sizes, all).map_err(|e| e.to_string()));
                    match split {
                        Ok(split) => {
                            // Each pending request must receive ITS OWN slice:
                            // the merged batch returns embeddings in the same
                            // order as the merged texts. (Before this split,
                            // every caller got the FULL merged batch and
                            // `embed_one` popped the last vector — the wrong
                            // embedding for every caller except the final one
                            // whenever the window merged >1 request.)
                            for (item, slice) in pending.into_iter().zip(split) {
                                let _ = item.resp.send(Ok(slice));
                            }
                        }
                        Err(e) => {
                            for item in pending {
                                let _ = item.resp.send(Err(e.clone()));
                            }
                        }
                    }
                }
            })?;
        Ok((Self { tx }, handle))
    }

    pub fn embed_one(&self, text: String) -> Result<Vec<f32>> {
        let (resp_tx, resp_rx) = std::sync::mpsc::channel();
        self.tx
            .send(BatchItem { texts: vec![text], resp: resp_tx })
            .map_err(|e| anyhow!("embed batcher channel closed: {e}"))?;
        let mut out = resp_rx
            .recv()
            .map_err(|e| anyhow!("embed batcher response channel closed: {e}"))?
            .map_err(|e| anyhow!("embed batcher inference failed: {e}"))?;
        out.pop().context("No embedding returned from batcher")
    }

    /// Send a multi-text request through the same coalescing window: the
    /// collector merges it with concurrent single-text requests into ONE
    /// ONNX batch and returns exactly this request's own embeddings, in
    /// order. Used by the reindexer so its chunks share inference with
    /// recall instead of serializing against it on the model Mutex.
    pub fn embed_many(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        let (resp_tx, resp_rx) = std::sync::mpsc::channel();
        self.tx
            .send(BatchItem { texts, resp: resp_tx })
            .map_err(|e| anyhow!("embed batcher channel closed: {e}"))?;
        resp_rx
            .recv()
            .map_err(|e| anyhow!("embed batcher response channel closed: {e}"))?
            .map_err(|e| anyhow!("embed batcher inference failed: {e}"))
    }
}

/// Split a merged batch result back into per-request slices, in the same
/// order the requests were merged. Pure helper so the offset arithmetic is
/// unit-testable without an ONNX model (the wrong-slice bug lived here).
///
/// Returns `Err` when the model produced a different number of embeddings
/// than the batch requested — the unchecked slice index below used to panic,
/// and the collector runs in the `embed-batcher` thread under
/// `panic = "abort"` (Cargo.toml:37), where a panic kills the entire kernel
/// process, not just the affected requests (ROADMAP_O3:60; verified
/// 2026-10-05: the 1:1 input/output invariant is enforced nowhere in
/// fastembed 5.8.0 — the output length comes from the ONNX tensor rows,
/// `text_embedding/output.rs`, not from the input count).
fn split_batch_results(sizes: &[usize], merged: Vec<Vec<f32>>) -> Result<Vec<Vec<Vec<f32>>>> {
    let expected: usize = sizes.iter().sum();
    if merged.len() != expected {
        return Err(anyhow!(
            "batch split: model returned {} embeddings for {} requested ({} pending request(s))",
            merged.len(),
            expected,
            sizes.len()
        ));
    }
    // Length invariant holds: every intermediate `offset + n` <= expected.
    let mut offset = 0usize;
    Ok(sizes
        .iter()
        .map(|&n| {
            let slice = merged[offset..offset + n].to_vec();
            offset += n;
            slice
        })
        .collect())
}

/// Embedding engine for semantic search.
pub struct EmbeddingEngine {
    model: Mutex<TextEmbedding>,
    model_type: String,
    dimension: u32,
    cache: Mutex<LruCache<String, Vec<f32>>>,
    /// Coalescing batcher (lazy-spawned when `embed_batching_enabled` is on).
    batcher: Mutex<Option<Arc<EmbedBatcher>>>,
    /// ADR-017 F1: batcher de la clase ROUTING (fallback semántico de do,
    /// anchors, DCR) — ventana y lote propios, separado del batcher global
    /// de recall para eliminar el head-of-line blocking cruzado medido en la
    /// RUN2 del harness (do p50 ×10 con el batcher único). Lazy, solo si
    /// `[silva] embed_batching_routing_enabled` está on.
    routing_batcher: Mutex<Option<Arc<EmbedBatcher>>>,
    /// SHA-256 fingerprint (modelo+revisión+dims+normalización+ficheros) del
    /// engine exacto que produjo un vector — computado UNA vez en el load.
    /// `None` = identidad de pesos no verificable en disco (desconocido
    /// honesto: el engine funciona; las filas quedan marcadas
    /// `unknown-pre-hash` por la migración de boot, nunca falsa procedencia).
    fingerprint: Option<String>,
}

/// Resolve fastembed model enum from config string.
///
/// Unknown names are an `Err`, NOT a silent fallback to the BGE-M3 baseline:
/// a typo in `embedding_model` used to download and load ~1.2GB of a
/// different model without any warning (ROADMAP_O3:61, verified 2026-10-05 —
/// the old `else` branch defaulted to `BGEM3` with an explicit comment).
/// Matching stays substring-based over the same known families as before
/// (including the `models/<name>` path form the loader passes), so every
/// legitimate config value keeps resolving exactly as it did; only names
/// that used to fall through to the baseline are rejected.
pub fn resolve_model(embedding_model: &str) -> Result<EmbeddingModel> {
    let lower = embedding_model.to_lowercase();
    if lower.contains("mxbai-q") || lower.contains("mxbai-quantized") {
        Ok(EmbeddingModel::MxbaiEmbedLargeV1Q)
    } else if lower.contains("mxbai") {
        Ok(EmbeddingModel::MxbaiEmbedLargeV1)
    } else if lower.contains("nomic") {
        Ok(EmbeddingModel::NomicEmbedTextV15)
    } else if lower.contains("minilm") {
        Ok(EmbeddingModel::AllMiniLML6V2)
    } else if lower.contains("bge-small") {
        Ok(EmbeddingModel::BGESmallENV15)
    } else if lower.contains("arctic") {
        Ok(EmbeddingModel::SnowflakeArcticEmbedL)
    } else if lower.contains("e5") {
        Ok(EmbeddingModel::MultilingualE5Large)
    } else if lower.contains("bge") {
        // "bge" / "bge-m3" and friends: the project's baseline family.
        Ok(EmbeddingModel::BGEM3)
    } else {
        Err(anyhow!(
            "embedding_model '{embedding_model}' is not a recognized model — refusing to silently substitute BGE-M3. \
             Valid values: mxbai-embed-large, mxbai-q/quantized, nomic-embed-text, minilm, bge-m3, bge-small, \
             snowflake-arctic-embed-l (arctic), multilingual-e5-large (e5) \
             (or 'none' to disable embeddings and run BM25-only)"
        ))
    }
}

/// Resolve output vector dimension from config string.
pub fn resolve_dimension(embedding_model: &str) -> u32 {
    if embedding_model.is_empty() || embedding_model == "none" {
        return 0;
    }
    let lower = embedding_model.to_lowercase();
    if lower.contains("bge-m3") || lower == "bge" || lower.contains("mxbai") {
        1024
    } else if lower.contains("nomic") {
        768
    } else if lower.contains("minilm") || lower.contains("bge-small") {
        384
    } else {
        // arctic-embed-l, e5-large and any future 1024-dim model.
        1024
    }
}

/// Human-readable model name for logs.
fn model_display_name(embedding_model: &str) -> &'static str {
    let lower = embedding_model.to_lowercase();
    if lower.contains("mxbai-q") || lower.contains("mxbai-quantized") {
        "Mxbai-Embed-Large-v1-Quantized"
    } else if lower.contains("mxbai") {
        "Mxbai-Embed-Large-v1"
    } else if lower.contains("bge-m3") {
        "BGE-M3"
    } else if lower.contains("bge-small") {
        "BGE-Small"
    } else if lower.contains("bge") {
        "BGE"
    } else if lower.contains("minilm") {
        "MiniLM-L6-v2"
    } else if lower.contains("nomic") {
        "Nomic-Embed-v1.5"
    } else if lower.contains("arctic") {
        "Snowflake-Arctic-Embed-L"
    } else if lower.contains("e5") {
        "Multilingual-E5-Large"
    } else {
        "BGE-M3"
    }
}

/// Model type string for engine_id().
fn resolve_model_type(embedding_model: &str) -> String {
    let lower = embedding_model.to_lowercase();
    if lower.contains("mxbai-q") || lower.contains("mxbai-quantized") {
        "mxbai-embed-large-q"
    } else if lower.contains("mxbai") {
        "mxbai-embed-large"
    } else if lower.contains("bge-m3") {
        "bge-m3"
    } else if lower.contains("bge-small") {
        "bge-small"
    } else if lower.contains("bge") {
        "bge"
    } else if lower.contains("minilm") {
        "minilm"
    } else if lower.contains("nomic") {
        "nomic"
    } else if lower.contains("arctic") {
        "snowflake-arctic-embed-l"
    } else if lower.contains("e5") {
        "multilingual-e5-large"
    } else {
        "bge-m3"
    }.to_string()
}

// fastembed 5.8's REAL HF-hub cache resolution (verified in its common.rs):
// default cache dir is ".fastembed_cache" RELATIVE TO THE PROCESS CWD, overridable
// via FASTEMBED_CACHE_DIR; pull_from_hf redirects to HF_HOME when that env var is
// set. The home-dir and XDG variants cover older fastembed layouts and Linux
// packaging. Order matters for find_fastembed_snapshot: cwd-relative first, then
// explicit env overrides, then the historical per-user locations.
fn fastembed_cache_dirs() -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    // fastembed default: cwd-relative ".fastembed_cache" (e.g. cargo test runs
    // with cwd = crate dir -> crates/<crate>/.fastembed_cache).
    if let Ok(cwd) = std::env::current_dir() {
        out.push(cwd.join(".fastembed_cache"));
    }
    if let Ok(env_dir) = std::env::var("FASTEMBED_CACHE_DIR")
        && !env_dir.is_empty()
    {
        out.push(std::path::PathBuf::from(env_dir));
    }
    if let Ok(hf_home) = std::env::var("HF_HOME")
        && !hf_home.is_empty()
    {
        out.push(std::path::PathBuf::from(hf_home));
    }
    if let Some(home) = dirs::home_dir() {
        out.push(home.join(".fastembed_cache"));
    }
    if let Some(cache) = dirs::cache_dir() {
        out.push(cache.join("fastembed"));
    }
    out
}

/// HF repo id for every model type `resolve_model_type` can produce — must
/// match the `models--<org>--<name>` layout fastembed actually downloads.
fn hf_repo_for(model_type: &str) -> Option<&'static str> {
    match model_type {
        "bge-m3" => Some("BAAI/bge-m3"),
        "bge-small" => Some("BAAI/bge-small-en-v1.5"),
        "mxbai-embed-large" | "mxbai-embed-large-q" => Some("mixedbread-ai/mxbai-embed-large-v1"),
        "nomic" => Some("nomic-ai/nomic-embed-text-v1.5"),
        "minilm" => Some("sentence-transformers/all-MiniLM-L6-v2"),
        "snowflake-arctic-embed-l" => Some("snowflake/snowflake-arctic-embed-l"),
        // fastembed 5.8 downloads the ONNX export from Qdrant for e5-large
        // (ModelInfo.model_code), NOT the intfloat original.
        "multilingual-e5-large" => Some("Qdrant/multilingual-e5-large-onnx"),
        _ => None,
    }
}

/// Latest (lexicographically max — deterministic across boots) HF revision
/// snapshot fastembed has for this model.
fn find_fastembed_snapshot(model_type: &str) -> Option<std::path::PathBuf> {
    let repo = hf_repo_for(model_type)?;
    let hub_dir = format!("models--{}", repo.replace('/', "--"));
    for cache in fastembed_cache_dirs() {
        let snapshots = cache.join(&hub_dir).join("snapshots");
        let Ok(entries) = std::fs::read_dir(&snapshots) else { continue };
        let mut revs: Vec<std::path::PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        if revs.is_empty() {
            continue;
        }
        revs.sort();
        return revs.pop();
    }
    None
}

/// Streaming SHA-256 of a file (64 KiB chunks — tokenizer 17 MB, ONNX graph
/// 725 KB; worst case a single-file model ~550 MB, ~1 s once per boot).
fn sha256_file(path: &std::path::Path) -> Option<String> {
    use sha2::Digest;
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = sha2::Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(format!("{:x}", hasher.finalize()))
}

/// Canonical fingerprint v1 (format `sha256:<64 hex>`): deterministic parts,
/// NO volatile inputs (no mtimes, no absolute paths) — an identical
/// re-download keeps the SAME fingerprint and does NOT trigger a re-embed
/// storm. It changes only when model, HF revision, dims, normalization or
/// file content change — exactly when re-embedding is semantically correct
/// (get_stale_embeddings consumes it via engine_hash).
pub(crate) fn fingerprint_from_parts(parts: &[(&str, String)]) -> String {
    use sha2::Digest;
    let mut canonical = String::new();
    for (key, value) in parts {
        canonical.push_str(key);
        canonical.push('=');
        canonical.push_str(value);
        canonical.push('\n');
    }
    format!("sha256:{:x}", sha2::Sha256::digest(canonical.as_bytes()))
}

/// ONNX graph inside an HF snapshot. fastembed models use TWO layouts:
/// `onnx/model.onnx` (bge-m3, arctic-embed-l, mxbai...) and `model.onnx` at
/// the snapshot root (Qdrant/multilingual-e5-large-onnx). Returns the first
/// that exists — `None` if the snapshot has neither (honest unknown).
fn locate_onnx_graph(snapshot: &std::path::Path) -> Option<std::path::PathBuf> {
    for rel in ["onnx/model.onnx", "model.onnx"] {
        let candidate = snapshot.join(rel);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Fingerprint of the engine fastembed just loaded (TL-approved design
/// 2026-10-05): model type + HF revision + output dims + normalization +
/// content hashes of the ONNX graph and the tokenizer + byte size of the
/// external weights blob. Never reads the multi-GB weights blob itself
/// (boot stays ~100 ms); revision sha + graph/tokenizer hashes + size pin
/// the identity. `None` = cache not found (honest unknown, never guessed).
pub(crate) fn compute_engine_fingerprint(model_type: &str, dimension: u32) -> Option<String> {
    let snapshot = find_fastembed_snapshot(model_type)?;
    let graph = locate_onnx_graph(&snapshot)?;
    let graph_hash = sha256_file(&graph)?;
    let tokenizer_hash = sha256_file(&snapshot.join("tokenizer.json"))
        .unwrap_or_else(|| "absent".to_string());
    let data_size = graph
        .parent()
        .map(|dir| dir.join("model.onnx_data"))
        .as_deref()
        .map(std::fs::metadata)
        .map(|m| m.map(|meta| meta.len().to_string()).unwrap_or_else(|_| "none".to_string()))
        .unwrap_or_else(|| "none".to_string());
    let revision = snapshot.file_name()?.to_str()?.to_string();
    let parts: Vec<(&str, String)> = vec![
        ("scheme", "v1".to_string()),
        ("model_type", model_type.to_string()),
        ("revision", revision),
        ("dims", dimension.to_string()),
        ("norm", "l2".to_string()),
        ("graph_sha256", graph_hash),
        ("tokenizer_sha256", tokenizer_hash),
        ("weights_data_size", data_size),
    ];
    Some(fingerprint_from_parts(&parts))
}

impl EmbeddingEngine {
    /// Initialize the embedding engine using fastembed.
    pub fn load(model_name: &str) -> Result<Self> {
        Self::load_with_device(model_name, &InferenceDevice::Cpu)
    }

    /// Initialize with an explicit execution device (cpu / directml / cuda).
    pub fn load_with_device(model_name: &str, device: &InferenceDevice) -> Result<Self> {
        // Unknown model names fail HERE, before TextInitOptions/try_new —
        // fastembed auto-downloads on try_new (see ensure_provisioned), so
        // this is the last point where a typo can be rejected without
        // fetching 1.2GB of the wrong model first.
        let model = resolve_model(model_name)?;
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

        // Fingerprint UNA vez (TL 2026-10-05): modelo+revisión+dims+norm+ficheros
        // de identidad. Best-effort: si el cache de fastembed no se localiza,
        // fingerprint=None (desconocido honesto) y el engine sigue funcionando.
        let fingerprint = compute_engine_fingerprint(&model_type, dimension);
        match &fingerprint {
            Some(fp) => info!("🧠 {} fingerprint {}", model_type.to_uppercase(), fp),
            None => warn!(
                "🧠 {} fingerprint unavailable (fastembed cache not found) — embeddings saved without hash, marked unknown-pre-hash at next boot",
                model_type.to_uppercase()
            ),
        }

        Ok(Self {
            model: Mutex::new(text_model),
            model_type,
            dimension,
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(512).unwrap())),
            batcher: Mutex::new(None),
            routing_batcher: Mutex::new(None),
            fingerprint,
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

    /// Engine-level coalescing entry point (embed-batching contract, T582):
    /// ALL dense-embed call sites use this instead of `embed()` so concurrent
    /// callers share ONNX batches. Flag-gated (`[silva] embed_batching_enabled`,
    /// default off): when disabled this is a thin wrapper over `embed()`.
    /// Queue: bounded (`sync_channel`, see `EmbedBatcher`) — a full queue
    /// blocks the sender (backpressure) until the collector drains; explicit
    /// overflow rejection stays in `InferenceBudget` (`Err(Saturated)` when
    /// `[inference.budget] max_queue_wait_secs > 0`).
    pub fn embed_batch_coalesced(self: &Arc<Self>, text: &str) -> Result<Vec<f32>> {
        if !Self::batching_enabled() {
            return self.embed(text);
        }
        self.shared_batcher()?.embed_one(text.to_string())
    }

    /// Async-safe coalesced batch entry (reindexer contract, 2026-09-27):
    /// when `embed_batching_enabled` is on, routes ALL texts through the
    /// shared EmbedBatcher window so background re-indexing coalesces with
    /// concurrent recall traffic (one ONNX batch instead of serialized
    /// mutex contention — the contamination measured in the A/B baseline).
    /// When the flag is off it delegates to `embed_batch_async` (blocking
    /// pool), preserving exact current behavior. Never runs ONNX on an
    /// async worker.
    pub async fn embed_batch_coalesced_async(
        self: &Arc<Self>,
        texts: Vec<String>,
    ) -> Result<Vec<Vec<f32>>> {
        if !Self::batching_enabled() {
            return self.embed_batch_async(texts).await;
        }
        let batcher = self.shared_batcher()?;
        tokio::task::block_in_place(move || batcher.embed_many(texts))
    }

    /// Is the coalescing flag on? Reads the cached runtime config; defaults
    /// to off on any load failure (fail-safe: no behavioral change).
    fn batching_enabled() -> bool {
        crate::config::TylluanConfig::load_cached()
            .ok()
            .map(|cfg| cfg.try_read().ok().map(|g| g.silva.embed_batching_enabled).unwrap_or(false))
            .unwrap_or(false)
    }

    /// ADR-017 F1: entry point de la clase ROUTING. Gated por
    /// `[silva] embed_batching_routing_enabled` (independiente del master de
    /// recall): cuando está off degrada a `embed()` — comportamiento exacto
    /// de producción actual. Cuando está on, coalesce los embeds cortos de
    /// routing en su PROPIA ventana (75ms, lote 32; valor inicial a calibrar
    /// empíricamente según ADR-017 §3/F1). Invariante del ADR: ningún rechazo
    /// deja datos a medias — si la cola falla, cae a embed directo con warn,
    /// nunca devuelve error hacia arriba.
    pub fn embed_batch_coalesced_routing(self: &Arc<Self>, text: &str) -> Result<Vec<f32>> {
        if !Self::routing_batching_enabled() {
            return self.embed(text);
        }
        match self.routing_shared_batcher().and_then(|b| b.embed_one(text.to_string())) {
            Ok(v) => Ok(v),
            Err(e) => {
                warn!("routing batcher fallback: {e} (embed directo, sin pérdida)");
                self.embed(text)
            }
        }
    }

    /// Flag de la clase ROUTING (ADR-017 F1). Fail-safe off.
    fn routing_batching_enabled() -> bool {
        crate::config::TylluanConfig::load_cached()
            .ok()
            .map(|cfg| cfg.try_read().ok().map(|g| g.silva.embed_batching_routing_enabled).unwrap_or(false))
            .unwrap_or(false)
    }

    /// Get-or-spawn del batcher ROUTING (ventana 75ms, lote 32 — punto de
    /// calibración de F1; el segundo valor se medirá en el gate).
    fn routing_shared_batcher(self: &Arc<Self>) -> Result<Arc<EmbedBatcher>> {
        {
            let guard = self.routing_batcher.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(b) = guard.as_ref() {
                return Ok(Arc::clone(b));
            }
        }
        let engine = Arc::clone(self);
        let (batcher, handle) = EmbedBatcher::spawn(engine, 32, 75)?;
        let _ = handle;
        let arc = Arc::new(batcher);
        if let Ok(mut guard) = self.routing_batcher.lock()
            && guard.is_none()
        {
            *guard = Some(Arc::clone(&arc));
        }
        Ok(arc)
    }

    /// Get-or-spawn the shared coalescing batcher (lazy; only called when
    /// the flag is on). Losing the spawn race just means the caller uses its
    /// own local batcher for this one call; the winner is stored for the
    /// next callers.
    fn shared_batcher(self: &Arc<Self>) -> Result<Arc<EmbedBatcher>> {
        {
            let guard = self.batcher.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(b) = guard.as_ref() {
                return Ok(Arc::clone(b));
            }
        }
        let engine = Arc::clone(self);
        let (batcher, handle) = EmbedBatcher::spawn(engine, 16, 5)?;
        let _ = handle;
        let arc = Arc::new(batcher);
        if let Ok(mut guard) = self.batcher.lock()
            && guard.is_none()
        {
            *guard = Some(Arc::clone(&arc));
        }
        Ok(arc)
    }

    /// Embed multiple texts in one ONNX batch call.
    /// FastEmbed natively batches — this avoids N sequential inference calls.
    /// Each returned vector is L2-normalized for cosine similarity.
    ///
    /// Tarea Raíz 1 (2026-09-28): this is THE chokepoint every dense path
    /// converges on (`embed()` L1-miss, `embed_batch_async`, the coalescing
    /// batcher thread), so the interactive inference budget is acquired here,
    /// AFTER the L1 cache (a cache hit must not consume budget) and BEFORE
    /// the raw model mutex. With the default config this is an unbounded wait
    /// (legacy behavior preserved, zero rejections); with
    /// `[inference.budget] max_queue_wait_secs > 0` saturation returns an
    /// explicit fast error instead of an unobservable mutex pile-up.
    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let _budget = crate::memory::inference_budget::InferenceBudget::global().acquire_sync()?;
        let mut model = self.model.lock().unwrap_or_else(|e| e.into_inner());
        let mut embeddings = model.embed(texts, None)
            .map_err(|e| anyhow!("Batch inference failed: {e:?}"))?;
        // Enforce the 1:1 input/output invariant fastembed does not check
        // (its output length comes from the ONNX tensor rows): a short or
        // long batch returned Ok would silently misalign every caller that
        // pairs texts with vectors — including the batcher's split below.
        if embeddings.len() != texts.len() {
            return Err(anyhow!(
                "Batch inference returned {} embeddings for {} texts — refusing to misalign callers",
                embeddings.len(),
                texts.len()
            ));
        }

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

    /// SHA-256 fingerprint del engine exacto que produjo un vector: modelo +
    /// revisión HF + dims + política de normalización + hashes del grafo ONNX
    /// y del tokenizer + tamaño de los pesos externos (esquema v1, TL
    /// 2026-10-05). `None` = identidad no verificable en disco — esas filas
    /// se marcan `unknown-pre-hash` en el boot, nunca se inventa procedencia.
    pub fn engine_hash(&self) -> Option<String> {
        self.fingerprint.clone()
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
    ///
    /// T925: on a cache MISS this acquires the same shared interactive
    /// inference budget as the dense/rerank paths, BEFORE the raw model
    /// mutex (budget → model order everywhere, no deadlock). A cache HIT
    /// returns before the acquire — it must not consume budget (same rule
    /// as `EmbeddingEngine::embed_batch`, Tarea Raíz 1).
    pub fn embed(&self, text: &str) -> Result<SparseVec> {
        let key = text.trim().to_lowercase();
        {
            let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(hit) = cache.get(&key) {
                return Ok(hit.clone());
            }
        }
        let _budget = crate::memory::inference_budget::InferenceBudget::global().acquire_sync()?;
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

    /// Embed multiple texts in one sparse ONNX batch call.
    ///
    /// T925: acquires the same shared interactive inference budget as
    /// `EmbeddingEngine::embed_batch` and `RerankEngine::rerank` — after the
    /// empty-input short-circuit (nothing to infer) and before the raw model
    /// mutex. Bounded/rejection behavior identical to the dense path
    /// (`Err(Saturated)` when `[inference.budget]` opts in).
    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<SparseVec>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let _budget = crate::memory::inference_budget::InferenceBudget::global().acquire_sync()?;
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
    ///
    /// Tarea Raíz 1 (2026-09-28): acquires the same shared interactive
    /// inference budget as the dense path before the raw model mutex (one
    /// local machine serves one interactive inference at a time; each caller
    /// acquires once, sequentially — no deadlock). Bounded/rejection behavior
    /// identical to `embed_batch`.
    pub fn rerank(&self, query: &str, documents: &[&str]) -> Result<Vec<(usize, f32)>> {
        if documents.is_empty() { return Ok(vec![]); }
        let _budget = crate::memory::inference_budget::InferenceBudget::global().acquire_sync()?;
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
    fn test_resolve_model() {
        assert_eq!(resolve_model("mxbai-embed-large").unwrap(), EmbeddingModel::MxbaiEmbedLargeV1);
        assert_eq!(resolve_model("mxbai-embed-large-v1").unwrap(), EmbeddingModel::MxbaiEmbedLargeV1);
        assert_eq!(resolve_model("mxbai-q").unwrap(), EmbeddingModel::MxbaiEmbedLargeV1Q);
        assert_eq!(resolve_model("mxbai-quantized").unwrap(), EmbeddingModel::MxbaiEmbedLargeV1Q);
        assert_eq!(resolve_model("bge-m3").unwrap(), EmbeddingModel::BGEM3);
        assert_eq!(resolve_model("bge").unwrap(), EmbeddingModel::BGEM3);
        assert_eq!(resolve_model("nomic").unwrap(), EmbeddingModel::NomicEmbedTextV15);
        assert_eq!(resolve_model("minilm").unwrap(), EmbeddingModel::AllMiniLML6V2);
        assert_eq!(resolve_model("bge-small").unwrap(), EmbeddingModel::BGESmallENV15);
        assert_eq!(resolve_model("snowflake-arctic-embed-l").unwrap(), EmbeddingModel::SnowflakeArcticEmbedL);
        assert_eq!(resolve_model("arctic").unwrap(), EmbeddingModel::SnowflakeArcticEmbedL);
        assert_eq!(resolve_model("multilingual-e5-large").unwrap(), EmbeddingModel::MultilingualE5Large);
        assert_eq!(resolve_model("e5-large").unwrap(), EmbeddingModel::MultilingualE5Large);
        // The path form the loader actually passes (matcher: "models/<cfg>").
        assert_eq!(resolve_model("models/bge-m3").unwrap(), EmbeddingModel::BGEM3);
        assert_eq!(resolve_model("models/mxbai-embed-large").unwrap(), EmbeddingModel::MxbaiEmbedLargeV1);
        // ROADMAP_O3:61: unknown names are an explicit error, never a
        // silent substitution of the BGE-M3 baseline (a typo used to
        // trigger a ~1.2GB download of the wrong model without warning).
        for typo in ["unknown-custom", "bert-base-uncased", "glove", ""] {
            let err = resolve_model(typo).unwrap_err();
            let msg = err.to_string();
            assert!(msg.contains("BGE-M3"), "error must name the substituted baseline: {msg}");
            assert!(msg.contains("Valid values"), "error must list valid values: {msg}");
        }
        let msg = resolve_model("unknown-custom").unwrap_err().to_string();
        assert!(msg.contains("unknown-custom"), "error must echo the offending value: {msg}");
    }

    #[test]
    fn locate_onnx_graph_supports_both_fastembed_layouts() {
        use std::fs;
        // Unique temp dir per run — no global cwd mutation (parallel-test safe).
        let base = std::env::temp_dir().join(format!(
            "tylluan-graph-layout-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        // Layout A: onnx/ subdir (bge-m3, arctic-embed-l).
        let a = base.join("a").join("snap");
        fs::create_dir_all(a.join("onnx")).unwrap();
        fs::write(a.join("onnx").join("model.onnx"), b"graph-a").unwrap();
        assert_eq!(
            locate_onnx_graph(&a),
            Some(a.join("onnx").join("model.onnx"))
        );
        // Layout B: snapshot root (Qdrant e5-large-onnx).
        let b = base.join("b").join("snap");
        fs::create_dir_all(&b).unwrap();
        fs::write(b.join("model.onnx"), b"graph-b").unwrap();
        assert_eq!(locate_onnx_graph(&b), Some(b.join("model.onnx")));
        // No graph at all -> None (honest unknown).
        let c = base.join("c").join("snap");
        fs::create_dir_all(&c).unwrap();
        assert_eq!(locate_onnx_graph(&c), None);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_resolve_dimension() {
        assert_eq!(resolve_dimension("mxbai-embed-large"), 1024);
        assert_eq!(resolve_dimension("mxbai-embed-large-v1"), 1024);
        assert_eq!(resolve_dimension("mxbai-q"), 1024);
        assert_eq!(resolve_dimension("bge-m3"), 1024);
        assert_eq!(resolve_dimension("bge-small"), 384);
        assert_eq!(resolve_dimension("minilm"), 384);
        assert_eq!(resolve_dimension("nomic-embed-text"), 768);
        assert_eq!(resolve_dimension("none"), 0);
        assert_eq!(resolve_dimension(""), 0);
    }

    #[test]
    fn test_model_display_name_and_type() {
        assert_eq!(model_display_name("mxbai-embed-large"), "Mxbai-Embed-Large-v1");
        assert_eq!(model_display_name("mxbai-q"), "Mxbai-Embed-Large-v1-Quantized");
        assert_eq!(model_display_name("bge-m3"), "BGE-M3");
        assert_eq!(resolve_model_type("mxbai-embed-large"), "mxbai-embed-large");
        assert_eq!(resolve_model_type("mxbai-q"), "mxbai-embed-large-q");
        assert_eq!(resolve_model_type("bge-m3"), "bge-m3");
        assert_eq!(resolve_model_type("arctic"), "snowflake-arctic-embed-l");
        assert_eq!(resolve_model_type("e5-large"), "multilingual-e5-large");
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

    // Regression (2026-09-27): the EmbedBatcher collector used to send every
    // caller the FULL merged batch and `embed_one` popped the last vector —
    // the wrong embedding for every caller except the final one whenever the
    // 5ms window merged >1 request (multi-caller misalignment).
    #[test]
    fn split_batch_results_returns_each_caller_its_own_slice() {
        let merged = vec![
            vec![1.0, 0.0],
            vec![2.0, 0.0],
            vec![3.0, 0.0],
            vec![4.0, 0.0],
        ];
        let sizes = [1usize, 2, 1];
        let split = split_batch_results(&sizes, merged.clone()).unwrap();
        assert_eq!(split.len(), 3);
        assert_eq!(split[0], vec![vec![1.0, 0.0]]);
        assert_eq!(split[1], vec![vec![2.0, 0.0], vec![3.0, 0.0]]);
        assert_eq!(split[2], vec![vec![4.0, 0.0]]);
        let remerged: Vec<Vec<f32>> = split.into_iter().flatten().collect();
        assert_eq!(remerged, merged, "slices must re-assemble the merged batch in order");
    }

    #[test]
    fn split_batch_results_single_caller_gets_everything() {
        let merged = vec![vec![1.0], vec![2.0]];
        let split = split_batch_results(&[2], merged).unwrap();
        assert_eq!(split, vec![vec![vec![1.0], vec![2.0]]]);
    }

    // ROADMAP_O3:60 regression: the unchecked slice index used to panic when
    // the model returned fewer/more vectors than requested — under
    // `panic = "abort"` that killed the whole kernel, not just the request.
    #[test]
    fn split_batch_results_rejects_wrong_model_output_size() {
        let short = vec![vec![1.0], vec![2.0]];
        let err = split_batch_results(&[1, 1, 1], short).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("2 embeddings for 3 requested"), "must report both counts: {msg}");

        let long = vec![vec![1.0], vec![2.0], vec![3.0]];
        let err = split_batch_results(&[1, 1], long).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("3 embeddings for 2 requested"), "must report both counts: {msg}");

        // Empty sizes + empty merged stays a valid no-op.
        assert!(split_batch_results(&[], Vec::new()).is_ok());
    }

    // CONTRACT-01 invariant: BGE-M3 is ALWAYS 1024 dimensions
    #[test]
    fn contract_01_bge_m3_1024_dimensions() {
        assert_eq!(resolve_dimension("bge-m3"), 1024);
        assert_eq!(resolve_dimension("BGE-M3"), 1024);
        assert_eq!(resolve_dimension("bge"), 1024);
        assert_eq!(resolve_dimension("BGE"), 1024);
        assert_eq!(resolve_dimension("mxbai-embed-large"), 1024);
        assert_eq!(resolve_dimension("MXBAI-EMBED-LARGE"), 1024);
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

    /// Anti-drift (2026-09-28): embedding writes must never hardcode a model
    /// name. `save_embedding`/`INSERT INTO node_embeddings` must record the
    /// id of the engine that produced the vector — hardcoded "bge-m3"/"nomic"
    /// recreated permanently-stale rows under every other engine, feeding
    /// the Agnostic Reindexer's never-converging stale loop (98/196 nodes).
    #[test]
    fn no_hardcoded_model_names_in_embedding_writes() {
        let banned: &[&str] = &["\"bge-m3\"", "'bge-m3'", "\"nomic\"", "'nomic'"];
        let mut violations: Vec<String> = Vec::new();
        let mut stack: Vec<std::path::PathBuf> = vec![std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().and_then(|e| e.to_str()) == Some("rs")
                    && let Ok(src) = std::fs::read_to_string(&path) {
                        for (ln, line) in src.lines().enumerate() {
                            if (line.contains("save_embedding(") || line.contains("INSERT INTO node_embeddings"))
                                && banned.iter().any(|b| line.contains(b)) {
                                violations.push(format!("{}:{}", path.display(), ln + 1));
                            }
                        }
                }
            }
        }
        assert!(
            violations.is_empty(),
            "hardcoded model names in embedding writes (use engine.engine_id()):\n{}",
            violations.join("\n")
        );
    }

    #[test]
    fn fingerprint_is_deterministic_and_sensitive() {
        let a = fingerprint_from_parts(&[("model_type", "bge-m3".into()), ("dims", "1024".into())]);
        let b = fingerprint_from_parts(&[("model_type", "bge-m3".into()), ("dims", "1024".into())]);
        assert_eq!(a, b, "same parts -> same fingerprint (stable across boots)");
        assert!(a.starts_with("sha256:"));
        let c = fingerprint_from_parts(&[("model_type", "bge-m3".into()), ("dims", "768".into())]);
        let d = fingerprint_from_parts(&[("model_type", "mxbai-embed-large".into()), ("dims", "1024".into())]);
        assert_ne!(a, c, "dims are part of the identity");
        assert_ne!(a, d, "model is part of the identity");
    }

    #[test]
    fn hf_repo_covers_every_resolvable_model_type() {
        for model_type in [
            "bge-m3",
            "bge-small",
            "mxbai-embed-large",
            "mxbai-embed-large-q",
            "nomic",
            "minilm",
        ] {
            assert!(hf_repo_for(model_type).is_some(), "{model_type} must map to its HF repo");
        }
        assert!(hf_repo_for("totally-unknown").is_none(), "unknown types never guess a repo");
    }

    /// Exercises the REAL fastembed cache discovery on machines that have it
    /// (dev box: yes). On CI runners without the model this silently skips:
    /// an honest None is a valid result there, nothing to verify.
    #[test]
    fn engine_fingerprint_matches_local_cache_when_present() {
        if let Some(fp) = compute_engine_fingerprint("bge-m3", 1024) {
            assert!(fp.starts_with("sha256:"), "v1 format: {fp}");
            assert_eq!(fp.len(), "sha256:".len() + 64, "full hex digest: {fp}");
            let again = compute_engine_fingerprint("bge-m3", 1024);
            assert_eq!(Some(fp), again, "deterministic across calls");
        }
    }
}
