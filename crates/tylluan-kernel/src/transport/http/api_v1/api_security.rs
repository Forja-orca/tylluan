use axum::{Json, extract::State};
use std::sync::Arc;

use crate::security::coherence_gate;
use crate::transport::http::HttpState;

/// ADR-011 §2.5 observability: cumulative Coherence Gate counters since kernel start.
pub async fn coherence_gate_stats() -> impl axum::response::IntoResponse {
    let stats = coherence_gate::cumulative_stats();
    Json(serde_json::json!({
        "ok": true,
        "total_seen": stats.total_seen,
        "total_eliminated": stats.total_eliminated,
        "total_penalized": stats.total_penalized,
        "note": "counters since last kernel start, not a persisted historical log",
    }))
}

/// ADR-011 §2 observability: Signal Loop progress toward the 5,000-row
/// LightReranker training threshold.
pub async fn recall_feedback_stats(State(state): State<Arc<HttpState>>) -> impl axum::response::IntoResponse {
    const THRESHOLD: i64 = 5000;
    let resolved = state.silva.resolved_feedback_count().await.unwrap_or(0);
    let pending = state.silva.pending_feedback_count().await.unwrap_or(0);
    let pct = if THRESHOLD > 0 {
        (resolved as f64 / THRESHOLD as f64 * 100.0).min(100.0)
    } else {
        0.0
    };
    Json(serde_json::json!({
        "ok": true,
        "resolved": resolved,
        "pending": pending,
        "threshold": THRESHOLD,
        "pct": pct,
    }))
}
