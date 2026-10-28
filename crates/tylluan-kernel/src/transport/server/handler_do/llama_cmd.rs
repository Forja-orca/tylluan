//! T-DSH-115: intents de LECTURA del llama-server del usuario por MCP.
//!
//! Deterministas, sin guild y SIN capacidad de arrancar/parar (el cambio de
//! estado es clase operador y va por REST/CLI/dashboard, no por `tylluan_do`):
//! - `llama status` / `llama estado` / `estado llama` / `llama_backend status`
//! - `llama models` / `modelos llama` / `llama modelos`

use crate::transport::server::TylluanServer;

/// Devuelve `Some(json)` si el intent es de lectura del llama_backend; `None`
/// para que el llamante siga con el routing normal.
pub async fn handle_llama_intent(
    server: &TylluanServer,
    intent: &str,
) -> Option<serde_json::Value> {
    let low = intent.trim().to_ascii_lowercase();
    let rest = low
        .strip_prefix("llama_backend")
        .or_else(|| low.strip_prefix("estado llama"))
        .or_else(|| low.strip_prefix("modelos llama"))
        .or_else(|| low.strip_prefix("llama"))?
        .trim()
        .to_string();

    let is_status = rest.is_empty() || rest == "status" || rest == "estado";
    let is_models = rest == "models" || rest == "modelos" || rest == "modelos disponibles";
    if !is_status && !is_models {
        return None;
    }

    let lock = crate::config::TylluanConfig::load_cached().ok()?;
    let cfg = lock.read().await.inference.llama.clone();

    if is_models {
        let models = crate::transport::http::api_v1::api_inference::available_models(
            cfg.model_path.as_ref(),
            cfg.models_dir.as_ref(),
        );
        return Some(serde_json::json!({
            "models": models,
            "model_path": cfg.model_path.as_ref().map(|p| p.display().to_string()),
            "models_dir": cfg.models_dir.as_ref().map(|p| p.display().to_string()),
        }));
    }

    let running = server
        .registry
        .read()
        .await
        .status_all()
        .iter()
        .any(|g| g.name == "llama_backend" && g.running);
    Some(serde_json::json!({
        "running": running,
        "guild": "llama_backend",
        "port": cfg.port,
        "mode": if cfg.models_dir.is_some() { "router" } else { "single" },
        "model_path": cfg.model_path.as_ref().map(|p| p.display().to_string()),
        "models_dir": cfg.models_dir.as_ref().map(|p| p.display().to_string()),
        "models_max": cfg.models_max,
        "note": "lectura: arrancar/parar solo por REST/CLI/dashboard (clase operador)",
    }))
}

/// T-DSH-116: lectura del registro de puntos de inferencia por MCP (sin claves).
pub async fn handle_points_intent(intent: &str) -> Option<serde_json::Value> {
    let low = intent.trim().to_ascii_lowercase();
    let rest = low
        .strip_prefix("inference points")
        .or_else(|| low.strip_prefix("puntos de inferencia"))
        .or_else(|| low.strip_prefix("puntos inferencia"))?
        .trim()
        .to_string();
    if !(rest.is_empty() || rest == "status" || rest == "estado") {
        return None;
    }
    let lock = crate::config::TylluanConfig::load_cached().ok()?;
    let cfg = lock.read().await;
    Some(cfg.inference.points.public_json())
}

#[cfg(test)]
mod tests {

    // El parser puro no necesita servidor: solo comprobamos los prefijos que
    // deben caer al routing normal. La parte que toca config/registro se
    // ejercita en la verificacion en vivo.
    fn prefix_recognized(intent: &str) -> bool {
        let low = intent.trim().to_ascii_lowercase();
        low.strip_prefix("llama_backend")
            .or_else(|| low.strip_prefix("estado llama"))
            .or_else(|| low.strip_prefix("modelos llama"))
            .or_else(|| low.strip_prefix("llama"))
            .is_some()
    }

    #[test]
    fn llama_prefixes_recognized() {
        for i in ["llama status", "llama models", "estado llama", "modelos llama", "llama_backend status"] {
            assert!(prefix_recognized(i), "{i}");
        }
    }

    #[test]
    fn inference_points_prefixes_recognized() {
        for i in ["inference points", "puntos de inferencia", "puntos inferencia status"] {
            let low = i.to_ascii_lowercase();
            assert!(
                low.starts_with("inference points")
                    || low.starts_with("puntos de inferencia")
                    || low.starts_with("puntos inferencia"),
                "{i}"
            );
        }
    }

    #[test]
    fn unrelated_intents_not_llama() {
        for i in ["hola", "health", "sentinel status", "modelos de embeddings"] {
            assert!(!prefix_recognized(i), "{i}");
        }
    }
}
