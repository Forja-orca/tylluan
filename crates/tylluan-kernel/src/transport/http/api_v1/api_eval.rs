use axum::{Json, extract::State};
use serde::Deserialize;
use std::sync::Arc;
use crate::eval;
use crate::eval::EvalResult;
use crate::transport::http::HttpState;

const RESULTS_FILE: &str = "./data/eval_results.json";

fn results_path() -> std::path::PathBuf {
    std::path::PathBuf::from(RESULTS_FILE)
}

fn save_result(r: &EvalResult) {
    let path = results_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut existing: Vec<EvalResult> = if path.exists() {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    existing.push(r.clone());
    if let Ok(json) = serde_json::to_string_pretty(&existing) {
        let _ = std::fs::write(&path, json);
    }
}

fn load_results() -> Vec<EvalResult> {
    let path = results_path();
    if !path.exists() {
        return Vec::new();
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[derive(Deserialize)]
pub struct EvalRunPayload {
    pub benchmark: Option<String>,
    pub num_queries: Option<usize>,
    pub seed: Option<u64>,
}

pub async fn eval_run_handler(
    State(state): State<Arc<HttpState>>,
    Json(payload): Json<EvalRunPayload>,
) -> Json<serde_json::Value> {
    let benchmark = payload.benchmark.as_deref().unwrap_or("longmemeval-s");

    match benchmark {
        "longmemeval-s" => {
            let memory = match &state.server {
                Some(server) => {
                    let guard = server.read().await;
                    guard.memory.clone()
                }
                None => {
                    return Json(serde_json::json!({
                        "ok": false, "error": "Kernel server not initialized"
                    }));
                }
            };

            let result = eval::run_longmemeval_s(memory, payload.num_queries, payload.seed).await;
            save_result(&result);

            Json(serde_json::json!({
                "ok": true,
                "result": result
            }))
        }
        other => Json(serde_json::json!({
            "ok": false, "error": format!("Unknown benchmark: {}", other)
        })),
    }
}

pub async fn eval_list_handler() -> Json<serde_json::Value> {
    let results = load_results();
    let results_rev: Vec<EvalResult> = results.into_iter().rev().take(20).collect();
    Json(serde_json::json!({
        "ok": true,
        "results": results_rev,
        "total": results_rev.len(),
    }))
}
