//! # Maintenance Subsystem
//!
//! Handles export, import, and model download of the TylluanNexus sovereign state.
//! Enables the "60-second USB portability" killer feature and auto-download of AI weights.

use std::path::{Path, PathBuf};
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

/// Roles que el catálogo de descarga admite: **lo mínimo** que el kernel carga
/// de verdad (el embedder de la búsqueda vectorial y el reranker).
///
/// Un `chat`, un `vision` o un `decision` no se descargan solos nunca, ni
/// siquiera con `allow_hf_download = true`: si un manifiesto los declara, se
/// descarta la entrada y se dice por qué.
pub const DOWNLOADABLE_ROLES: [&str; 2] = ["embedding", "reranker"];

/// Catálogo de descarga, **leído del manifiesto** (`<models_dir>/*/model.json`).
///
/// DIR-CATALOGO-DESCARGA (orden de Jose 2026-11-10, TL T3913). Antes esto era un
/// `vec![...]` de 14 entradas con URL de HuggingFace, y tres de ellas eran pesos
/// de CHAT (SmolLM2-135M-Instruct, Qwen2.5-1.5B-Instruct, Qwen2.5-0.5B-Instruct)
/// que se autodescargaban en cuanto `allow_hf_download` estaba en `true`.
/// Tylluan no sirve chat: el catálogo es exactamente lo que el manifiesto
/// declara, y sólo en los roles de [`DOWNLOADABLE_ROLES`].
///
/// Un manifiesto sin bloque `download` no aporta entradas: aquí no se inventan
/// URLs ni tamaños.
pub fn get_model_registry(models_dir: &Path) -> Vec<ModelMeta> {
    let mut registry = Vec::new();
    let Ok(entries) = fs::read_dir(models_dir) else {
        // Sin carpeta de modelos no hay catálogo (y no es un error: es el
        // estado normal de una instalación que no los ha traído todavía).
        return registry;
    };

    let mut dirs: Vec<std::path::PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();

    for dir in dirs {
        let Some(id) = dir.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if id.starts_with('.') {
            continue;
        }
        let manifest_path_for_log = dir.join("model.json").display().to_string();
        let manifest = match crate::models::read_manifest(&dir) {
            Ok(Some(manifest)) => manifest,
            // Sin manifiesto no hay declaración de descarga: no se inventa nada.
            Ok(None) => continue,
            Err(err) => {
                warn!(
                    "ModelDownloader: {} no se pudo leer como manifiesto; fuera del catálogo ({err})",
                    manifest_path_for_log
                );
                continue;
            }
        };
        let Some(spec) = manifest.download.as_ref() else {
            continue;
        };
        if spec.files.is_empty() {
            continue;
        }

        let allowed = DOWNLOADABLE_ROLES
            .iter()
            .any(|role| manifest.has_role(role));
        if !allowed {
            warn!(
                "ModelDownloader: '{}' declara descarga pero su rol no está entre los mínimos \
                 ({:?}); NO se descarga. Si de verdad hace falta, se declara el rol o se copia a mano.",
                id, DOWNLOADABLE_ROLES
            );
            continue;
        }

        let display = manifest
            .name
            .clone()
            .unwrap_or_else(|| format!("Local {id}"));
        for file in &spec.files {
            if file.url.trim().is_empty() {
                warn!(
                    "ModelDownloader: '{}' declara el fichero '{}' sin URL; se ignora",
                    id, file.path
                );
                continue;
            }
            registry.push(ModelMeta {
                name: format!("{display} ({})", file.path),
                path: format!("{id}/{}", file.path),
                url: file.url.clone(),
                size_expected: file.size_bytes,
            });
        }
    }

    registry
}

/// Un `path` del manifiesto solo puede ser **relativo y limpio**: sin `..`,
/// sin ruta absoluta, sin prefijo de unidad (`C:`), sin componentes vacíos.
///
/// T4106 (TL): antes `path` y `url` eran constantes de confianza en el código;
/// ahora salen de `<models_dir>/*/model.json`, que puede escribir cualquier
/// guild o agente con acceso a esa carpeta. Devuelve el path normalizado o
/// `None` si no es aceptable.
pub fn safe_relative_model_path(raw: &str) -> Option<PathBuf> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    // `C:algo`, `C:\algo`, `c:/algo`: prefijo de unidad.
    let bytes = raw.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return None;
    }
    if raw.starts_with('\\') || raw.starts_with('/') || raw.starts_with('~') {
        return None;
    }
    let parsed = Path::new(raw);
    if parsed.is_absolute() {
        return None;
    }
    let mut out = PathBuf::new();
    for comp in parsed.components() {
        match comp {
            std::path::Component::Normal(seg) => out.push(seg),
            // ParentDir (`..`), CurDir (`.`), RootDir y Prefix quedan fuera.
            _ => return None,
        }
    }
    if out.as_os_str().is_empty() { None } else { Some(out) }
}

/// `true` solo si la URL es **https** y su host está en la allow-list.
///
/// T4106 (TL): sin esto, un `model.json` podía apuntar a cualquier host (o a
/// http) y el kernel lo descargaba con `allow_hf_download = true`.
pub fn is_allowed_model_url(url: &str, allowed_hosts: &[String]) -> bool {
    let Some((scheme, rest)) = url.trim().split_once("://") else {
        return false;
    };
    if !scheme.eq_ignore_ascii_case("https") {
        return false;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    // Sin credenciales embebidas (`user@host`) ni puerto: solo host.
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = host.split(':').next().unwrap_or(host).to_ascii_lowercase();
    if host.is_empty() {
        return false;
    }
    allowed_hosts
        .iter()
        .any(|allowed| allowed.trim().eq_ignore_ascii_case(&host))
}

/// Resuelve `meta.path` dentro de `models_dir` y comprueba la contención.
///
/// Devuelve `Err` con el motivo para que `download_model` lo registre y se
/// descarte la entrada, en vez de escribir fuera de `models/`.
pub fn resolve_model_download_path(models_dir: &Path, raw: &str) -> Result<PathBuf, String> {
    let rel = safe_relative_model_path(raw)
        .ok_or_else(|| format!("path no valido en el manifiesto: '{raw}'"))?;
    let full = models_dir.join(&rel);
    if !full.starts_with(models_dir) {
        return Err(format!(
            "path fuera de models_dir tras normalizar: '{}'",
            full.display()
        ));
    }
    Ok(full)
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
    pub async fn download_model(
        &self,
        models_dir: &Path,
        meta: &ModelMeta,
        allowed_hosts: &[String],
    ) -> Result<()> {
        // T4106: path y url vienen del manifiesto (escribible por guilds y
        // agentes). Se validan ANTES de crear directorios o descargar nada.
        let full_path = resolve_model_download_path(models_dir, &meta.path)
            .map_err(|why| anyhow!("{} (modelo '{}')", why, meta.name))?;
        if !is_allowed_model_url(&meta.url, allowed_hosts) {
            return Err(anyhow!(
                "url no permitida en el manifiesto del modelo '{}': '{}' (solo https y hosts {:?})",
                meta.name,
                meta.url,
                allowed_hosts
            ));
        }
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
            let chunk = item.map_err(|e| anyhow!("Download error: {e}"))?;
            file.write_all(&chunk).await.map_err(|e| anyhow!("Write error: {e}"))?;
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
    ///
    /// HF-CACHE-ACOPLADA (2026-11-08): con `[nexus] allow_hf_download = false` (default) esto NO
    /// descarga nada y **dice que falta y donde**. Los modelos se resuelven de la carpeta
    /// compartida (`models/`). El registro de URLs no se borra: queda inerte, y eliminarlo es
    /// decision de Jose.
    pub async fn download_missing(&self, models_dir: &Path) -> Result<Vec<String>> {
        let (permite_descarga, allowed_hosts) = crate::config::TylluanConfig::load_cached()
            .map(|c| {
                c.try_read()
                    .map(|g| (g.nexus.allow_hf_download, g.nexus.allowed_model_hosts.clone()))
                    .unwrap_or((false, crate::config::default_allowed_model_hosts()))
            })
            .unwrap_or((false, crate::config::default_allowed_model_hosts()));
        let registry = get_model_registry(models_dir);
        let mut downloaded = Vec::new();

        if !permite_descarga {
            if registry.is_empty() {
                // DIR-CATALOGO-DESCARGA: sin manifiesto de descargas no hay
                // nada que echar de menos. Decirlo así evita el mensaje falso
                // de "registro completo" cuando en realidad no se declara nada.
                info!(
                    "ModelDownloader: el manifiesto de {} no declara descargas; nada que comprobar \
                     (allow_hf_download=false)",
                    models_dir.display()
                );
                return Ok(downloaded);
            }
            let mut faltan = Vec::new();
            for meta in &registry {
                if !self.is_model_present(models_dir, meta) {
                    faltan.push(format!(
                        "{} ({})",
                        meta.name,
                        models_dir.join(&meta.path).display()
                    ));
                }
            }
            if faltan.is_empty() {
                info!("ModelDownloader: registro completo en disco; nada que descargar (allow_hf_download=false)");
            } else {
                warn!(
                    "ModelDownloader: allow_hf_download=false -> NO se descarga nada. Faltan {} \
                     modelo(s) en {}: {}. Para resolverlo deja los ficheros en models/ o activa \
                     [nexus] allow_hf_download = true a proposito.",
                    faltan.len(),
                    models_dir.display(),
                    faltan.join(", ")
                );
            }
            return Ok(downloaded);
        }

        info!("🧬 ModelDownloader: Checking Sovereign Registry (v3.5 April 2026)...");

        for meta in registry {
            if !self.is_model_present(models_dir, &meta) {
                info!("📥 Model missing, downloading: {}", meta.name);
                if let Err(e) = self.download_model(models_dir, &meta, &allowed_hosts).await {
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
        .context(format!("Failed to create export file at {output_path:?}"))?;
    
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
        return Err(anyhow!("Import file not found: {input_path:?}"));
    }

    let file = File::open(input_path)?;
    let tar = GzDecoder::new(file);
    let mut archive = Archive::new(tar);

    // Ensure target directory exists
    fs::create_dir_all(target_dir)?;

    // Manually iterate entries for better error handling/reporting with large files
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        info!("   📂 Unpacking: {:?}", path);
        entry.unpack_in(target_dir)?;
    }
    
    info!("✅ Import complete. Sovereign state restored at {:?}", target_dir);
    info!("💡 Restart the kernel to apply changes.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Escribe `<dir>/<id>/model.json` con el contenido dado.
    fn write_manifest(dir: &Path, id: &str, json: &str) {
        let model_dir = dir.join(id);
        std::fs::create_dir_all(&model_dir).expect("crear carpeta de modelo");
        std::fs::write(model_dir.join("model.json"), json).expect("escribir model.json");
    }

    /// DIR-CATALOGO-DESCARGA: el catálogo sale del MANIFIESTO, no de una lista
    /// en Rust. Un `embedding` con bloque `download` produce exactamente sus
    /// ficheros, con su URL y su tamaño declarados.
    #[test]
    fn catalogo_sale_del_manifiesto() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(
            dir.path(),
            "embedder",
            r#"{
                "name": "Embedder de prueba",
                "role": "embedding",
                "download": { "required": true, "files": [
                    { "path": "onnx/model.onnx", "url": "https://example.invalid/m.onnx", "size_bytes": 1234 }
                ] }
            }"#,
        );

        let registry = get_model_registry(dir.path());
        assert_eq!(registry.len(), 1, "una sola entrada declarada: {registry:?}");
        assert_eq!(registry[0].path, "embedder/onnx/model.onnx");
        assert_eq!(registry[0].url, "https://example.invalid/m.onnx");
        assert_eq!(registry[0].size_expected, 1234);
    }

    /// Y sin bloque `download` no hay entrada: aquí no se inventan URLs.
    #[test]
    fn manifiesto_sin_bloque_download_no_aporta_entradas() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(dir.path(), "decima", r#"{ "role": "decision", "name": "Decima" }"#);
        assert!(
            get_model_registry(dir.path()).is_empty(),
            "sin download.files no hay nada que descargar"
        );
    }

    /// El test de la orden de Jose (TL T3913): un modelo de CHAT declarado en
    /// el manifiesto **no** entra en el catálogo, ni con URL ni sin ella.
    /// Mutacion: si se quita la lista blanca de roles, este test cae.
    #[test]
    fn catalogo_descarta_modelos_de_chat() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(
            dir.path(),
            "smollm2",
            r#"{
                "name": "SmolLM2-135M-Instruct",
                "role": "chat",
                "family": "smollm2",
                "download": { "files": [
                    { "path": "model.safetensors", "url": "https://example.invalid/chat.safetensors", "size_bytes": 270000000 }
                ] }
            }"#,
        );
        // Y un rol fuera de la lista mínima que tampoco es chat, por si acaso.
        write_manifest(
            dir.path(),
            "visioncito",
            r#"{ "role": "vision", "download": { "files": [
                { "path": "v.onnx", "url": "https://example.invalid/v.onnx", "size_bytes": 10 }
            ] } }"#,
        );

        let registry = get_model_registry(dir.path());
        assert!(
            registry.is_empty(),
            "ni chat ni vision pueden entrar en el catálogo de descarga: {registry:?}"
        );
    }

    /// La lista mínima es `embedding` + `reranker`: el reranker sí es descargable
    /// (lo carga el kernel para la búsqueda híbrida).
    #[test]
    fn catalogo_admite_embedding_y_reranker() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(
            dir.path(),
            "embedder",
            r#"{ "role": "embedding", "download": { "files": [
                { "path": "e.onnx", "url": "https://example.invalid/e.onnx", "size_bytes": 1 }
            ] } }"#,
        );
        write_manifest(
            dir.path(),
            "reranker",
            r#"{ "role": "reranker", "roles": ["embedding"], "download": { "files": [
                { "path": "r.onnx", "url": "https://example.invalid/r.onnx", "size_bytes": 2 }
            ] } }"#,
        );
        let paths: Vec<String> = get_model_registry(dir.path()).into_iter().map(|m| m.path).collect();
        assert!(paths.contains(&"embedder/e.onnx".to_string()), "{paths:?}");
        assert!(paths.contains(&"reranker/r.onnx".to_string()), "{paths:?}");
    }

    /// Y ninguna entrada del catálogo declara metadatos inválidos: esto antes
    /// se comprobaba sobre la lista hardcodeada, ahora sobre lo que la función
    /// devuelve de verdad.
    #[test]
    fn entradas_del_catalogo_tienen_metadatos_utiles() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(
            dir.path(),
            "embedder",
            r#"{ "role": "embedding", "download": { "files": [
                { "path": "e.onnx", "url": "https://example.invalid/e.onnx", "size_bytes": 7 },
                { "path": "sin-url.onnx", "url": "", "size_bytes": 7 }
            ] } }"#,
        );
        let registry = get_model_registry(dir.path());
        assert_eq!(
            registry.len(),
            1,
            "la entrada sin URL se ignora en vez de viajar rota: {registry:?}"
        );
        for meta in &registry {
            assert!(!meta.name.is_empty(), "name vacio");
            assert!(!meta.path.is_empty(), "path vacio");
            assert!(meta.url.starts_with("https://"), "url no https: {}", meta.url);
            assert!(meta.size_expected > 0, "size_expected debe ser > 0");
        }
    }

    /// HF-CACHE-ACOPLADA (2026-11-08): por defecto `allow_hf_download` es **false**, y con esa
    /// configuracion `download_missing` **no descarga nada** aunque falten modelos: devuelve lista
    /// vacia y deja el directorio como estaba. Es el comportamiento que pide Jose (nada de red).
    #[tokio::test(flavor = "multi_thread")]
    async fn download_missing_does_not_download_by_default() {
        let config = crate::config::TylluanConfig::default();
        assert!(
            !config.nexus.allow_hf_download,
            "allow_hf_download debe nacer en false"
        );

        let tmp = std::env::temp_dir().join(format!(
            "tylluan-hf-off-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("tmp dir");

        let downloader = ModelDownloader::new();
        let descargados = downloader
            .download_missing(&tmp)
            .await
            .expect("download_missing no debe fallar con la descarga apagada");

        assert!(
            descargados.is_empty(),
            "con allow_hf_download=false no puede descargar nada, devolvio {descargados:?}"
        );
        let ficheros: Vec<_> = std::fs::read_dir(&tmp)
            .expect("leer tmp")
            .filter_map(|e| e.ok())
            .collect();
        assert!(
            ficheros.is_empty(),
            "no debe escribir nada en disco: {:?}",
            ficheros.iter().map(|e| e.file_name()).collect::<Vec<_>>()
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
}

#[cfg(test)]
mod t3_download_guards_tests {
    use super::*;

    fn hosts() -> Vec<String> {
        vec!["huggingface.co".to_string(), "cdn-lfs.huggingface.co".to_string()]
    }

    /// T4106: `..` no puede salir de `models/`. Antes se hacía
    /// `models_dir.join(&meta.path)` a secas y un manifiesto con `../../algo`
    /// escribía fuera.
    #[test]
    fn path_con_parent_dir_se_rechaza() {
        assert!(safe_relative_model_path("../../.ssh/authorized_keys").is_none());
        assert!(safe_relative_model_path("modelo/../../../fuera.bin").is_none());
        assert!(resolve_model_download_path(Path::new("/models"), "../../evil.bin").is_err());
    }

    /// Ni rutas absolutas ni prefijo de unidad (`C:`) ni `~`.
    #[test]
    fn path_absoluto_o_con_unidad_se_rechaza() {
        for raw in ["/etc/passwd", "\\servidor\\share\\x.bin", "C:\\windows\\x.dll", "C:x.dll", "~/x.bin"] {
            assert!(
                safe_relative_model_path(raw).is_none(),
                "debe rechazar '{raw}'"
            );
        }
    }

    /// Una ruta relativa limpia se acepta y se resuelve dentro de models_dir.
    #[test]
    fn path_relativo_limpio_se_acepta_y_queda_dentro() {
        let rel = safe_relative_model_path("mxbai-embed-large/onnx/model.onnx").unwrap();
        assert_eq!(rel, PathBuf::from("mxbai-embed-large/onnx/model.onnx"));
        let full = resolve_model_download_path(Path::new("/models"), "m/w.bin").unwrap();
        assert!(full.starts_with(Path::new("/models")));
    }

    /// T4106: solo https. `http://` (o sin esquema) se descarta.
    #[test]
    fn url_http_se_rechaza() {
        assert!(!is_allowed_model_url("http://huggingface.co/x.bin", &hosts()));
        assert!(!is_allowed_model_url("huggingface.co/x.bin", &hosts()));
        assert!(!is_allowed_model_url("ftp://huggingface.co/x.bin", &hosts()));
    }

    /// T4106: host ajeno se descarta aunque sea https.
    #[test]
    fn url_de_host_ajeno_se_rechaza() {
        assert!(!is_allowed_model_url("https://evil.example/x.bin", &hosts()));
        // Con credenciales o puerto, el host tampoco cuela.
        assert!(!is_allowed_model_url("https://huggingface.co@evil.example/x.bin", &hosts()));
        assert!(!is_allowed_model_url("https://evil.example:443/x.bin", &hosts()));
    }

    /// Y la URL buena pasa: https + host de la allow-list.
    #[test]
    fn url_https_de_host_permitido_se_acepta() {
        assert!(is_allowed_model_url(
            "https://huggingface.co/mxbai/resolve/main/model.onnx",
            &hosts()
        ));
        assert!(is_allowed_model_url(
            "https://cdn-lfs.huggingface.co/blob/abc",
            &hosts()
        ));
        // La comparación de host no distingue mayúsculas.
        assert!(is_allowed_model_url("https://HuggingFace.co/x", &hosts()));
    }
}
