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

/// T-DSH-116 paso 3: actualizacion parcial de un punto (clase operador).
#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(default)]
pub struct PointUpdatePayload {
    pub provider: Option<String>,
    pub endpoint: Option<String>,
    pub model: Option<String>,
    pub api_key_env: Option<String>,
    pub cloud_enabled: Option<bool>,
    /// T-DSH-117: opt-in para persistir el cambio en tylluan.toml (atomico,
    /// solo la seccion [inference.points.<point>]).
    pub persist: Option<bool>,
}

/// Aplica el update sobre una copia y valida el resultado (pura, testeable).
pub fn apply_point_update(
    current: &crate::config::InferencePointConfig,
    name: &str,
    payload: &PointUpdatePayload,
) -> Result<crate::config::InferencePointConfig, Vec<String>> {
    let mut point = current.clone();
    if let Some(provider) = &payload.provider {
        point.provider = Some(provider.trim().to_ascii_lowercase());
    }
    if let Some(endpoint) = &payload.endpoint {
        point.endpoint = if endpoint.trim().is_empty() {
            None
        } else {
            Some(endpoint.trim().to_string())
        };
    }
    if let Some(model) = &payload.model {
        point.model = if model.trim().is_empty() {
            None
        } else {
            Some(model.trim().to_string())
        };
    }
    if let Some(api_key_env) = &payload.api_key_env {
        point.api_key_env = if api_key_env.trim().is_empty() {
            None
        } else {
            Some(api_key_env.trim().to_string())
        };
    }
    if let Some(cloud_enabled) = payload.cloud_enabled {
        point.cloud_enabled = cloud_enabled;
    }
    let errors = point.validate(name);
    if errors.is_empty() {
        Ok(point)
    } else {
        Err(errors)
    }
}

fn point_slot_mut<'a>(
    cfg: &'a mut crate::config::TylluanConfig,
    name: &str,
) -> Option<&'a mut crate::config::InferencePointConfig> {
    let points = &mut cfg.inference.points;
    match name {
        "decision" => Some(&mut points.decision),
        "grammar_judge" => Some(&mut points.grammar_judge),
        "sentinel" => Some(&mut points.sentinel),
        "chat" => Some(&mut points.chat),
        _ => None,
    }
}

/// `POST /api/v1/inference/points/{point}` — clase operador (REST/CLI, no MCP).
pub async fn set_inference_point_handler(
    State(state): State<Arc<HttpState>>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(payload): Json<PointUpdatePayload>,
) -> impl IntoResponse {
    let mut cfg = state.config.write().await;
    let Some(point) = point_slot_mut(&mut cfg, &name) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"ok": false, "error": format!("punto desconocido: {name}")})),
        )
            .into_response();
    };
    match apply_point_update(point, &name, &payload) {
        Ok(updated) => {
            let public = updated.public_json();
            *point = updated;
            tracing::warn!(
                "[T-DSH-116] punto de inferencia '{name}' actualizado por operador (clase operador): {}",
                public
            );
            // T-DSH-117: persistencia opt-in (--persist / ?persist=true).
            let persist_requested = payload.persist == Some(true);
            let mut persisted = serde_json::Value::Null;
            if persist_requested {
                let Some(path) = crate::config::TylluanConfig::find_config_file() else {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({
                            "ok": false,
                            "point": name,
                            "error": "no hay tylluan.toml que persistir (usa --config o crea uno)",
                            "state": cfg.inference.points.public_json(),
                        })),
                    )
                        .into_response();
                };
                let point_now = point.clone();
                if let Err(e) = crate::config::persist_inference_point(&path, &name, &point_now) {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({
                            "ok": false,
                            "point": name,
                            "error": format!("cambio aplicado en memoria pero NO persistido: {e}"),
                            "state": cfg.inference.points.public_json(),
                        })),
                    )
                        .into_response();
                }
                persisted = serde_json::json!({"path": path.display().to_string()});
            }
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "ok": true,
                    "point": name,
                    "updated": public,
                    "persisted": persisted,
                    "state": cfg.inference.points.public_json(),
                })),
            )
                .into_response()
        }
        Err(errors) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "ok": false,
                "point": name,
                "errors": errors,
                "state": cfg.inference.points.public_json(),
            })),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_update_rejects_cloud_without_opt_in() {
        let current = crate::config::InferencePointConfig::default();
        let payload = PointUpdatePayload {
            provider: Some("openai_compatible".into()),
            endpoint: Some("https://api.example.com/v1".into()),
            ..Default::default()
        };
        let errors = apply_point_update(&current, "chat", &payload).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("cloud_enabled=false")), "{errors:?}");
    }

    #[test]
    fn point_update_accepts_loopback_systemone_without_opt_in() {
        let current = crate::config::InferencePointConfig::default();
        let payload = PointUpdatePayload {
            provider: Some("systemone_http".into()),
            endpoint: Some("http://127.0.0.1:8787/v1".into()),
            ..Default::default()
        };
        let updated = apply_point_update(&current, "decision", &payload).unwrap();
        assert_eq!(updated.provider(), "systemone_http");
        assert!(!updated.is_cloud());
    }

    #[test]
    fn point_update_is_partial_and_keeps_other_fields() {
        let current = crate::config::InferencePointConfig {
            model: Some("modelo-viejo".into()),
            ..Default::default()
        };
        let payload = PointUpdatePayload {
            cloud_enabled: Some(true),
            ..Default::default()
        };
        let updated = apply_point_update(&current, "chat", &payload).unwrap();
        assert_eq!(updated.model.as_deref(), Some("modelo-viejo"));
        assert!(updated.cloud_enabled);
    }

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
