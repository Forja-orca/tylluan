//! # Maintenance Subsystem
//!
//! Handles export, import, and model download of the TylluanNexus sovereign state.
//! Enables the "60-second USB portability" killer feature and auto-download of AI weights.

use std::path::Path;
use std::fs::{self, File};
use anyhow::{Result, Context, anyhow};
use tar::Archive;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Serialize, Deserialize};
use reqwest::Client;
use tokio::io::AsyncWriteExt;
use tokio::fs as tokio_fs;
use tokio::sync::broadcast;
use futures_util::StreamExt;
use tracing::{info, warn, error};

/// Download progress event for SSE
#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub model: String,
    pub progress: f64,
    pub downloaded_mb: u64,
    pub total_mb: u64,
    pub status: String,
}

/// Model metadata for the auto-downloader
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMeta {
    pub name: String,
    pub path: String,
    pub url: String,
    pub size_expected: u64,
}

/// Registry of available models for sovereign auto-download
pub fn get_model_registry() -> Vec<ModelMeta> {
    vec![
        ModelMeta {
            name: "SmolLM2-135M-Instruct".to_string(),
            path: "smollm2/model.safetensors".to_string(),
            url: "https://huggingface.co/HuggingFaceTB/smollm-135m-instruct/resolve/main/model.safetensors".to_string(),
            size_expected: 270_000_000, // ~270MB
        },
        ModelMeta {
            name: "SmolLM2-135M-Config".to_string(),
            path: "smollm2/config.json".to_string(),
            url: "https://huggingface.co/HuggingFaceTB/smollm-135m-instruct/resolve/main/config.json".to_string(),
            size_expected: 2_000,
        },
        ModelMeta {
            name: "SmolLM2-135M-Tokenizer".to_string(),
            path: "smollm2/tokenizer.json".to_string(),
            url: "https://huggingface.co/HuggingFaceTB/smollm-135m-instruct/resolve/main/tokenizer.json".to_string(),
            size_expected: 2_000_000,
        },
        // --- Sovereign Tier (Qwen2.5 2024-2025) ---
        ModelMeta {
            name: "Qwen2.5-1.5B-Instruct".to_string(),
            path: "qwen2.5-1.5b/model.safetensors".to_string(),
            url: "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct/resolve/main/model.safetensors".to_string(),
            size_expected: 1_600_000_000,
        },
        ModelMeta {
            name: "Qwen2.5-0.5B-Instruct".to_string(),
            path: "qwen2.5-0.5b/model.safetensors".to_string(),
            url: "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct/resolve/main/model.safetensors".to_string(),
            size_expected: 600_000_000,
        },
        ModelMeta {
            name: "Nomic-Embed-v2".to_string(),
            path: "nomic-embed/model.safetensors".to_string(),
            url: "https://huggingface.co/nomic-ai/nomic-embed-text-v1.5/resolve/main/model.safetensors".to_string(),
            size_expected: 137_000_000, // Fixed expected size to ~137MB (V1.5 safetensors)
        },
        ModelMeta {
            name: "Nomic-Embed-v2-Config".to_string(),
            path: "nomic-embed/config.json".to_string(),
            url: "https://huggingface.co/nomic-ai/nomic-embed-text-v1.5/resolve/main/config.json".to_string(),
            size_expected: 1_000,
        },
    ]
}

#[derive(Clone)]
pub struct ModelDownloader {
    client: Client,
    progress_tx: Option<broadcast::Sender<DownloadProgress>>,
}

impl Default for ModelDownloader {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelDownloader {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .user_agent("TylluanNexus/3.0 (Sovereign Auto-Downloader)")
                .build()
                .unwrap_or_default(),
            progress_tx: None,
        }
    }

    /// Create a downloader with a progress channel for SSE events
    pub fn with_progress(tx: broadcast::Sender<DownloadProgress>) -> Self {
        Self {
            client: Client::builder()
                .user_agent("TylluanNexus/3.0 (Sovereign Auto-Downloader)")
                .build()
                .unwrap_or_default(),
            progress_tx: Some(tx),
        }
    }

    /// Check if a model's weights are present locally
    pub fn is_model_present(&self, models_dir: &Path, meta: &ModelMeta) -> bool {
        let full_path = models_dir.join(&meta.path);
        full_path.exists()
    }

    /// Download a model weight file with progress reporting via tracing and SSE
    pub async fn download_model(&self, models_dir: &Path, meta: &ModelMeta) -> Result<()> {
        let full_path = models_dir.join(&meta.path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).ok();
        }

        info!("🧠 Downloading AI weights: {} from HuggingFace...", meta.name);
        
        let response = self.client.get(&meta.url).send().await?;
        let total_size = response.content_length().unwrap_or(meta.size_expected);
        
        let mut file = tokio_fs::File::create(&full_path).await?;
        let mut downloaded: u64 = 0;
        let mut stream = response.bytes_stream();

        while let Some(item) = stream.next().await {
            let chunk = item.map_err(|e| anyhow!("Download error: {}", e))?;
            file.write_all(&chunk).await.map_err(|e| anyhow!("Write error: {}", e))?;
            downloaded += chunk.len() as u64;
            
            if downloaded.is_multiple_of(10 * 1024 * 1024) {
                let pct = (downloaded as f64 / total_size as f64) * 100.0;
                info!("   📥 {} progress: {:.1}% ({}/{} MB)", meta.name, pct, downloaded / 1024 / 1024, total_size / 1024 / 1024);
                
                // Emit SSE progress event
                if let Some(ref tx) = self.progress_tx {
                    let _ = tx.send(DownloadProgress {
                        model: meta.name.clone(),
                        progress: pct,
                        downloaded_mb: downloaded / 1024 / 1024,
                        total_mb: total_size / 1024 / 1024,
                        status: "downloading".to_string(),
                    });
                }
            }
        }

        file.flush().await?;
        
        // Emit completion event
        if let Some(ref tx) = self.progress_tx {
            let _ = tx.send(DownloadProgress {
                model: meta.name.clone(),
                progress: 100.0,
                downloaded_mb: total_size / 1024 / 1024,
                total_mb: total_size / 1024 / 1024,
                status: "complete".to_string(),
            });
        }
        
        info!("✅ Download complete: {}", meta.name);
        Ok(())
    }

    /// Download all missing models from the registry
    pub async fn download_missing(&self, models_dir: &Path) -> Result<Vec<String>> {
        let registry = get_model_registry();
        let mut downloaded = Vec::new();

        info!("🧬 ModelDownloader: Checking Sovereign Registry (v3.5 April 2026)...");

        for meta in registry {
            if !self.is_model_present(models_dir, &meta) {
                info!("📥 Model missing, downloading: {}", meta.name);
                if let Err(e) = self.download_model(models_dir, &meta).await {
                    error!("❌ Failed to download {}: {}", meta.name, e);
                } else {
                    downloaded.push(meta.name);
                }
            } else {
                info!("✅ Model present: {}", meta.name);
            }
        }

        Ok(downloaded)
    }
}

/// Export the full state of TylluanNexus to a tar.gz archive.
pub fn export_state(
    output_path: &Path, 
    data_dir: &Path, 
    config_dir: &Path, 
    models_dir: &Path,
    venv_dir: Option<&Path>
) -> Result<()> {
    info!("📦 TylluanNexus Export: Starting sovereign backup...");
    
    let file = File::create(output_path)
        .context(format!("Failed to create export file at {:?}", output_path))?;
    
    let enc = GzEncoder::new(file, Compression::default());
    let mut tar = tar::Builder::new(enc);

    // 1. Export Data (Memory, Silva, Mailbox)
    if data_dir.exists() {
        info!("   💾 Adding memory layers (data/)...");
        tar.append_dir_all("data", data_dir)?;
    } else {
        warn!("   ⚠️ Data directory not found at {:?}, skipping.", data_dir);
    }

    // 2. Export Config
    if config_dir.exists() {
        info!("   ⚙️ Adding sovereign configuration (config/)...");
        tar.append_dir_all("config", config_dir)?;
    }

    // 3. Export Models (BGE-M3 weights)
    if models_dir.exists() {
        info!("   🧠 Adding AI engines (models/)...");
        tar.append_dir_all("models", models_dir)?;
    } else {
        warn!("   ⚠️ Models directory not found at {:?}, skipping.", models_dir);
    }

    // 4. Export Executable Environment (Python venv)
    if let Some(venv) = venv_dir {
        if venv.exists() {
            info!("   🐍 Adding portable execution environment (.venv/)...");
            tar.append_dir_all(".venv", venv)?;
        } else {
            warn!("   ⚠️ Venv directory requested but not found at {:?}, skipping.", venv);
        }
    }

    tar.finish()?;
    let mut enc = tar.into_inner()?;
    enc.try_finish()?;
    let file = enc.finish()?;
    file.sync_all()?;
    
    info!("✅ Export complete: {:?}", output_path);
    Ok(())
}

/// Import a TylluanNexus state from a tar.gz archive.
pub fn import_state(input_path: &Path, target_dir: &Path) -> Result<()> {
    info!("📥 TylluanNexus Import: Restoring sovereign state (this may take time for 3GB+ backups)...");

    if !input_path.exists() {
        return Err(anyhow!("Import file not found: {:?}", input_path));
    }

    let file = File::open(input_path)?;
    let tar = GzDecoder::new(file);
    let mut archive = Archive::new(tar);

    // Ensure target directory exists
    fs::create_dir_all(target_dir)?;

    // Manually iterate entries for better error handling/reporting with large files
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_owned();
        info!("   📂 Unpacking: {:?}", path);
        entry.unpack_in(target_dir)?;
    }
    
    info!("✅ Import complete. Sovereign state restored at {:?}", target_dir);
    info!("💡 Restart the kernel to apply changes.");
    Ok(())
}
