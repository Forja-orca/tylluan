//! T-DSH-115: control del llama-server DEL USUARIO.
//!
//! El servidor es opcional y del usuario: NO se autoarranca desde ningun codigo
//! interno. Estas rutas son la orden explicita (clase operador para start/stop).
//! `status`/`models` son lecturas (no arrancan la guild).

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use std::path::PathBuf;
use std::sync::Arc;

use crate::transport::http::HttpState;
use rmcp::model::CallToolRequestParam;

const GUILD: &str = "llama_backend";

fn gguf_in(dir: &std::path::Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.is_file() && p.extension().and_then(|x| x.to_str()) == Some("gguf") {
                Some(p.display().to_string())
            } else {
                None
            }
        })
        .collect();
    out.sort();
    out
}

/// Modelos disponibles segun `[inference.llama]` (model_path o models_dir).
pub fn available_models(
    model_path: Option<&PathBuf>,
    models_dir: Option<&PathBuf>,
) -> Vec<String> {
    if let Some(dir) = models_dir {
        return gguf_in(dir);
    }
    model_path
        .filter(|p| p.is_file())
        .map(|p| vec![p.display().to_string()])
        .unwrap_or_default()
}

async fn guild_running(state: &Arc<HttpState>) -> bool {
    state
        .registry
        .status_all()
        .await
        .ok()
        .into_iter()
        .flatten()
        .any(|g| g.name == GUILD && g.running)
}

async fn llama_cfg() -> Option<crate::config::InferenceLlamaConfig> {
    let lock = crate::config::TylluanConfig::load_cached().ok()?;
    let cfg = lock.read().await;
    Some(cfg.inference.llama.clone())
}

fn tool_error_text(value: &serde_json::Value) -> Option<String> {
    let is_err = value
        .get("isError")
        .or_else(|| value.get("is_error"))
        .and_then(|v| v.as_bool())
        == Some(true);
    if is_err {
        return value
            .get("content")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("text"))
            .and_then(|t| t.as_str())
            .map(str::to_string);
    }
    None
}

async fn call_guild_tool(state: &Arc<HttpState>, tool: &str) -> Result<serde_json::Value, String> {
    state
        .registry
        .ensure_running(GUILD)
        .await
        .map_err(|e| e.to_string())?;
    let req = CallToolRequestParam {
        name: tool.to_string().into(),
        arguments: None,
    };
    let res = state
        .registry
        .call_tool_as(GUILD, req, Some("kernel".to_string()))
        .await
        .map_err(|e| e.to_string())?;
    let value = serde_json::to_value(res).map_err(|e| e.to_string())?;
    if let Some(err) = tool_error_text(&value) {
        return Err(err);
    }
    Ok(value)
}

/// Estado REAL del llama-server del usuario (via tool `server_status` de la
/// guild), no solo "la guild esta viva". Si la guild no corre: running=false.
async fn guild_server_status(state: &Arc<HttpState>) -> Option<serde_json::Value> {
    if !guild_running(state).await {
        return None;
    }
    let req = CallToolRequestParam {
        name: "server_status".to_string().into(),
        arguments: None,
    };
    let res = state
        .registry
        .call_tool_as(GUILD, req, Some("kernel".to_string()))
        .await
        .ok()?;
    let value = serde_json::to_value(res).ok()?;
    let text = value
        .get("content")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())?;
    serde_json::from_str(text).ok()
}

async fn status_json(state: &Arc<HttpState>) -> serde_json::Value {
    let guild_up = guild_running(state).await;
    let server = guild_server_status(state).await;
    let running = server
        .as_ref()
        .and_then(|s| s.get("running"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let cfg = llama_cfg().await;
    let (model_path, models_dir, models_max, port) = match cfg {
        Some(c) => (c.model_path, c.models_dir, c.models_max, c.port),
        None => (None, None, 4, 9000),
    };
    serde_json::json!({
        "running": running,
        "guild_running": guild_up,
        "guild": GUILD,
        "port": server.as_ref().and_then(|s| s.get("port")).cloned().unwrap_or(serde_json::json!(port)),
        "mode": if models_dir.is_some() { "router" } else { "single" },
        "model_path": model_path.as_ref().map(|p| p.display().to_string()),
        "models_dir": models_dir.as_ref().map(|p| p.display().to_string()),
        "models_max": models_max,
        "models": available_models(model_path.as_ref(), models_dir.as_ref()),
    })
}

/// `GET /api/v1/inference/llama/status` — lectura; nunca arranca nada.
pub async fn llama_status_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    Json(status_json(&state).await)
}

/// `GET /api/v1/inference/llama/models` — lectura de la config/carpeta.
pub async fn llama_models_handler() -> impl IntoResponse {
    let cfg = llama_cfg().await;
    let (model_path, models_dir) = match cfg {
        Some(c) => (c.model_path, c.models_dir),
        None => (None, None),
    };
    Json(serde_json::json!({
        "models": available_models(model_path.as_ref(), models_dir.as_ref()),
        "model_path": model_path.as_ref().map(|p| p.display().to_string()),
        "models_dir": models_dir.as_ref().map(|p| p.display().to_string()),
    }))
}

/// `POST /api/v1/inference/llama/start` — orden explicita (clase operador).
pub async fn llama_start_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    match call_guild_tool(&state, "start_server").await {
        Ok(tool_value) => {
            // Verificacion real: no damos 200 si el servidor no quedo escuchando.
            for _ in 0..20 {
                let st = status_json(&state).await;
                if st.get("running").and_then(|v| v.as_bool()) == Some(true) {
                    return (StatusCode::OK, Json(st)).into_response();
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "ok": false,
                    "error": "el llama-server no quedo escuchando tras start (revisa model_path/llama_server_path)",
                    "tool": tool_value,
                    "state": status_json(&state).await,
                })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"ok": false, "error": e})),
        )
            .into_response(),
    }
}

/// `POST /api/v1/inference/llama/stop` — para el servidor del usuario.
pub async fn llama_stop_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    if !guild_running(&state).await {
        return (StatusCode::OK, Json(status_json(&state).await)).into_response();
    }
    match call_guild_tool(&state, "stop_server").await {
        Ok(_) => (StatusCode::OK, Json(status_json(&state).await)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"ok": false, "error": e})),
        )
            .into_response(),
    }
}

/// `GET /api/v1/inference/points` — T-DSH-116: registro de puntos de inferencia
/// (lectura; NUNCA devuelve valores de claves, solo el nombre de la env var).
pub async fn inference_points_handler(
    State(_state): State<Arc<HttpState>>,
) -> impl IntoResponse {
    let json = match crate::config::TylluanConfig::load_cached() {
        Ok(lock) => lock.read().await.inference.points.public_json(),
        Err(e) => serde_json::json!({"valid": false, "errors": [format!("no pude cargar config: {e}")]}),
    };
    Json(json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn available_models_uses_models_dir_first() {
        let dir = std::env::temp_dir().join(format!("t115_models_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let a = dir.join("b.gguf");
        let b = dir.join("a.gguf");
        std::fs::write(&a, b"x").unwrap();
        std::fs::write(&b, b"x").unwrap();
        let models = available_models(None, Some(&dir));
        assert_eq!(models.len(), 2);
        assert!(models[0].ends_with("a.gguf"), "{models:?}");
        assert!(models[1].ends_with("b.gguf"), "{models:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn available_models_single_path_must_exist() {
        assert!(available_models(Some(&PathBuf::from("E:/no/existe.gguf")), None).is_empty());
    }
}
