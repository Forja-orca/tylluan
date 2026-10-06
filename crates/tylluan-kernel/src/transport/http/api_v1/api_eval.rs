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
    State(_state): State<Arc<HttpState>>,
    Json(payload): Json<EvalRunPayload>,
) -> Json<serde_json::Value> {
    let benchmark = payload.benchmark.as_deref().unwrap_or("longmemeval-s");

    match benchmark {
        "longmemeval-s" => {
            // F3: the benchmark WRITES (add_document) into an isolated
            // HybridMemory per run. Production hybrid must stay writer-free
            // ahead of F4, and repeated runs must not pollute each other.
            let eval_dir = std::env::temp_dir().join(format!(
                "tylluan_eval_{}_{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0)
            ));
            let eval_db = eval_dir.join("eval.db");
            let eval_db_str = eval_db.to_string_lossy().into_owned();
            let memory = match crate::memory::hybrid::HybridMemory::open(&eval_db_str) {
                Ok(m) => m,
                Err(e) => {
                    return Json(serde_json::json!({
                        "ok": false, "error": format!("eval db open failed: {e}")
                    }));
                }
            };
            if let Err(e) = memory.init().await {
                return Json(serde_json::json!({
                    "ok": false, "error": format!("eval db init failed: {e}")
                }));
            }

            let result =
                eval::run_longmemeval_s(Arc::new(memory), payload.num_queries, payload.seed).await;
            save_result(&result);
            // The Arc was consumed by the run and dropped with it, so DB
            // handles are closed here — best-effort cleanup of the temp dir.
            let _ = std::fs::remove_dir_all(&eval_dir);

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
