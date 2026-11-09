//! # D-156 — inventario real de modelos en disco
//!
//! Antes de este módulo, `GET /api/v1/models` clasificaba cada carpeta de
//! `models/` con una heurística sobre el **nombre** (`contains("bge")`,
//! `contains("vision")`) y sumaba sólo los ficheros del primer nivel. El
//! resultado medido en la instancia viva (José, 2026-11-09) fue:
//!
//! - `.hf_home` y `.hub_cache` rotulados como modelos,
//! - `decima`, `faster-whisper-base` y `MiniCPM-V-4.6` rotulados
//!   «LLM Generativo»,
//! - `smolvlm2-256m` con 4 MB cuando en disco ocupa 985,8 MB (sus pesos viven
//!   en subcarpetas),
//! - botones «Establecer como primario» activos en modelos que no chatean.
//!
//! Regla nueva, **sin adivinar**: el rol lo declara la propia carpeta en
//! `model.json`. Sin manifiesto no hay rol (`None`) y sin rol no hay
//! `chat_capable`; el panel rotula eso como «sin rol declarado» y deshabilita
//! el botón. Un manifiesto ilegible tampoco adivina: queda sin rol y el motivo
//! viaja en `manifest_error` para que el operador vea qué arreglar.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Nombre del manifiesto dentro de cada carpeta de modelo.
pub const MANIFEST_FILE: &str = "model.json";

/// Carpeta raíz por defecto, relativa al cwd del kernel (igual que el resto
/// del código: `doctor`, `mlp`, `maintenance`).
pub const MODELS_ROOT: &str = "models";

/// Profundidad máxima al sumar tamaños. Evita que un junction/symlink de
/// Windows (que `is_symlink()` no siempre detecta) nos haga recorrer el disco
/// entero; los pesos de un modelo nunca están a más de unas pocas carpetas.
const MAX_DEPTH: usize = 8;

/// Nombres de carpeta que son caché, no modelos.
const CACHE_DIR_NAMES: [&str; 6] = ["__pycache__", "node_modules", "cache", "caches", "tmp", "temp"];

/// Contenido de `models/<id>/model.json`.
///
/// Todos los campos son opcionales para que un manifiesto parcial no rompa el
/// listado: lo que falte simplemente no se declara.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelManifest {
    /// Nombre legible para el panel. Si falta se usa `Local <id>`.
    #[serde(default)]
    pub name: Option<String>,
    /// Rol principal: `chat`, `decision`, `sentinel`, `grammar_judge`,
    /// `embedding`, `reranker`, `router`, `vision`, `stt`.
    #[serde(default)]
    pub role: Option<String>,
    /// Roles secundarios. El principal manda para `chat_capable`.
    #[serde(default)]
    pub roles: Vec<String>,
    /// Familia (p. ej. `decima`, `qwen2.5`).
    #[serde(default)]
    pub family: Option<String>,
    /// `tylluan` (lo sirve el kernel) o `user` (lo aporta el operador).
    #[serde(default)]
    pub origin: Option<String>,
    /// Nota libre de auditoría: de dónde sale el rol declarado.
    #[serde(default)]
    pub notes: Option<String>,
}

/// Una entrada del inventario tal como la consume `/api/v1/models`.
#[derive(Debug, Clone, Serialize)]
pub struct LocalModel {
    /// Nombre de la carpeta: identifica el modelo en el resto de la API.
    pub id: String,
    pub name: String,
    pub path: String,
    /// Tamaño en disco de **todo** el árbol, no sólo del primer nivel.
    pub size_bytes: u64,
    /// `size_bytes` redondeado a MiB, para paneles que aún no leen bytes.
    pub size_mb: u64,
    pub installed: bool,
    /// Rol declarado en el manifiesto. `None` = «sin rol declarado».
    pub role: Option<String>,
    pub roles: Vec<String>,
    /// `true` sólo si el rol principal es `chat`.
    pub chat_capable: bool,
    /// `true` si la carpeta tiene `model.json` (aunque sea ilegible).
    pub has_manifest: bool,
    /// Motivo por el que el manifiesto no se pudo usar. `None` si todo bien.
    pub manifest_error: Option<String>,
}

/// Raíz por defecto del inventario.
pub fn models_root() -> PathBuf {
    PathBuf::from(MODELS_ROOT)
}

/// ¿Esta carpeta es caché o está oculta y por tanto no es un modelo?
///
/// Criterio de José (T3695): «0 entradas con punto» en la respuesta.
pub fn is_hidden_or_cache(name: &str) -> bool {
    if name.starts_with('.') {
        return true;
    }
    let lower = name.to_ascii_lowercase();
    CACHE_DIR_NAMES.contains(&lower.as_str())
}

/// Suma recursiva del tamaño de un árbol, sin seguir symlinks.
///
/// Un fichero que no se puede leer o un directorio que desaparece a mitad de
/// recorrido cuentan como 0 en vez de abortar el inventario entero.
pub fn dir_size_bytes(path: &Path) -> u64 {
    dir_size_bytes_at(path, 0)
}

fn dir_size_bytes_at(path: &Path, depth: usize) -> u64 {
    if depth >= MAX_DEPTH {
        return 0;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    let mut total = 0u64;
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            total = total.saturating_add(dir_size_bytes_at(&entry.path(), depth + 1));
        } else if file_type.is_file()
            && let Ok(meta) = entry.metadata()
        {
            total = total.saturating_add(meta.len());
        }
    }
    total
}

/// Lee el manifiesto de una carpeta de modelo.
///
/// - `Ok(None)`: no hay `model.json` (modelo sin rol declarado).
/// - `Ok(Some(_))`: manifiesto válido.
/// - `Err(_)`: el fichero existe pero no se pudo leer o parsear.
pub fn read_manifest(dir: &Path) -> Result<Option<ModelManifest>, String> {
    let path = dir.join(MANIFEST_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path)
        .map_err(|err| format!("no se pudo leer {}: {err}", path.display()))?;
    serde_json::from_str::<ModelManifest>(&raw)
        .map(Some)
        .map_err(|err| format!("{} no es un manifiesto válido: {err}", path.display()))
}

fn normalize_role(role: &str) -> Option<String> {
    let role = role.trim().to_ascii_lowercase();
    if role.is_empty() {
        None
    } else {
        Some(role)
    }
}

fn primary_role(manifest: &ModelManifest) -> Option<String> {
    manifest
        .role
        .as_deref()
        .and_then(normalize_role)
        .or_else(|| manifest.roles.iter().find_map(|r| normalize_role(r)))
}

/// Inventario de `<root>/<carpeta>`, ordenado por `id`.
///
/// Oculta carpetas con punto y cachés conocidas; un error de lectura del
/// manifiesto no descarta el modelo, lo deja sin rol y con el motivo.
pub fn scan(root: &Path) -> Vec<LocalModel> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut models = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if is_hidden_or_cache(name) {
            continue;
        }
        let (manifest, manifest_error) = match read_manifest(&path) {
            Ok(found) => (found, None),
            Err(err) => {
                tracing::warn!(model = %name, error = %err, "model.json ilegible; el modelo queda sin rol declarado");
                (None, Some(err))
            }
        };
        let has_manifest = manifest.is_some() || manifest_error.is_some();
        let role = manifest.as_ref().and_then(primary_role);
        let roles = manifest
            .as_ref()
            .map(|m| {
                m.roles
                    .iter()
                    .filter_map(|r| normalize_role(r))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let display_name = manifest
            .as_ref()
            .and_then(|m| m.name.clone())
            .unwrap_or_else(|| format!("Local {name}"));
        let size_bytes = dir_size_bytes(&path);
        let chat_capable = role.as_deref() == Some("chat");
        models.push(LocalModel {
            id: name.to_string(),
            name: display_name,
            path: path.to_string_lossy().to_string(),
            size_bytes,
            size_mb: (size_bytes + 512 * 1024) / (1024 * 1024),
            installed: true,
            role,
            roles,
            chat_capable,
            has_manifest,
            manifest_error,
        });
    }
    models.sort_by(|a, b| a.id.cmp(&b.id));
    models
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, rel: &str, content: &[u8]) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    fn model_dir(root: &Path, id: &str) -> PathBuf {
        let dir = root.join(id);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn scan_skips_dot_dirs_and_known_caches() {
        let tmp = tempfile::tempdir().unwrap();
        for id in [".hf_home", ".hub_cache", "__pycache__", "fastembed"] {
            model_dir(tmp.path(), id);
        }
        let ids: Vec<String> = scan(tmp.path()).into_iter().map(|m| m.id).collect();
        assert_eq!(ids, vec!["fastembed".to_string()]);
        assert!(ids.iter().all(|id| !id.starts_with('.')));
    }

    #[test]
    fn scan_size_is_recursive_not_first_level() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = model_dir(tmp.path(), "smolvlm2-256m");
        write(&dir, "model.onnx", &[0u8; 10 * 1024]);
        write(&dir, "onnx/decoder/model.onnx_data", &[0u8; 20 * 1024]);
        write(&dir, "tokenizer/tokenizer.json", &[0u8; 5 * 1024]);
        let model = scan(tmp.path()).pop().expect("un modelo");
        assert_eq!(model.size_bytes, 35 * 1024);
    }

    #[test]
    fn scan_declares_role_from_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = model_dir(tmp.path(), "decima");
        write(
            &dir,
            MANIFEST_FILE,
            br#"{"name":"Decima","role":"decision","roles":["decision"],"family":"decima"}"#,
        );
        let model = scan(tmp.path()).pop().expect("un modelo");
        assert_eq!(model.role.as_deref(), Some("decision"));
        assert_eq!(model.roles, vec!["decision".to_string()]);
        assert_eq!(model.name, "Decima");
        assert!(model.has_manifest);
        assert!(model.manifest_error.is_none());
        assert!(!model.chat_capable, "decision no chatea");
    }

    #[test]
    fn chat_capable_is_true_only_for_chat_role() {
        let tmp = tempfile::tempdir().unwrap();
        for (id, role) in [
            ("qwen2.5-1.5b", "chat"),
            ("faster-whisper-base", "stt"),
            ("all-minilm-l6-v2", "embedding"),
            ("MiniCPM-V-4.6", "vision"),
        ] {
            let dir = model_dir(tmp.path(), id);
            write(&dir, MANIFEST_FILE, format!(r#"{{"role":"{role}"}}"#).as_bytes());
        }
        let models = scan(tmp.path());
        let chat: Vec<&str> = models
            .iter()
            .filter(|m| m.chat_capable)
            .map(|m| m.id.as_str())
            .collect();
        assert_eq!(chat, vec!["qwen2.5-1.5b"]);
        let whisper = models.iter().find(|m| m.id == "faster-whisper-base").unwrap();
        assert_eq!(whisper.role.as_deref(), Some("stt"));
        assert!(!whisper.chat_capable);
    }

    #[test]
    fn no_manifest_means_no_role_and_not_chat_capable() {
        let tmp = tempfile::tempdir().unwrap();
        model_dir(tmp.path(), "moondream2");
        let model = scan(tmp.path()).pop().expect("un modelo");
        assert!(model.role.is_none(), "sin manifiesto no se adivina el rol");
        assert!(model.roles.is_empty());
        assert!(!model.has_manifest);
        assert!(model.manifest_error.is_none());
        assert!(!model.chat_capable);
        assert_eq!(model.name, "Local moondream2");
    }

    #[test]
    fn malformed_manifest_is_reported_and_not_guessed() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = model_dir(tmp.path(), "roto");
        write(&dir, MANIFEST_FILE, b"{ role: chat ");
        let model = scan(tmp.path()).pop().expect("un modelo");
        assert!(model.role.is_none());
        assert!(!model.chat_capable);
        assert!(model.has_manifest, "el fichero existe, aunque no sirva");
        let err = model.manifest_error.expect("motivo del fallo");
        assert!(err.contains("no es un manifiesto válido"), "error: {err}");
    }

    #[test]
    fn roles_plural_is_used_when_role_is_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = model_dir(tmp.path(), "mixto");
        write(&dir, MANIFEST_FILE, br#"{"roles":["vision","chat"]}"#);
        let model = scan(tmp.path()).pop().expect("un modelo");
        assert_eq!(model.role.as_deref(), Some("vision"));
        assert!(!model.chat_capable, "el rol principal es vision");
    }

    #[test]
    fn role_is_trimmed_and_lowercased() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = model_dir(tmp.path(), "grita");
        write(&dir, MANIFEST_FILE, br#"{"role":"  CHAT "}"#);
        let model = scan(tmp.path()).pop().expect("un modelo");
        assert_eq!(model.role.as_deref(), Some("chat"));
        assert!(model.chat_capable);
    }

    #[test]
    fn missing_models_root_is_an_empty_inventory() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(scan(&tmp.path().join("no-existe")).is_empty());
    }

    #[test]
    fn inventory_is_sorted_by_id() {
        let tmp = tempfile::tempdir().unwrap();
        for id in ["zeta", "alfa", "Media"] {
            model_dir(tmp.path(), id);
        }
        let ids: Vec<String> = scan(tmp.path()).into_iter().map(|m| m.id).collect();
        assert_eq!(ids, vec!["Media", "alfa", "zeta"]);
    }
}
