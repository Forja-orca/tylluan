use axum::{Json, extract::State};
use std::sync::Arc;

use crate::security::coherence_gate;
use crate::security::friction_log;
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
    let (resolved, pending) = feedback_counts(
        state.silva.resolved_feedback_count().await,
        state.silva.pending_feedback_count().await,
    );
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

/// T-DSH-79: los fallos de lectura de los contadores dejan de ser silenciosos;
/// el contrato de la respuesta (0) no cambia.
fn feedback_counts(
    resolved: anyhow::Result<i64>,
    pending: anyhow::Result<i64>,
) -> (i64, i64) {
    let resolved = resolved.unwrap_or_else(|error| {
        tracing::warn!(
            error = %error,
            "recall_feedback_stats: no se pudo contar 'resolved'; mostrando 0"
        );
        0
    });
    let pending = pending.unwrap_or_else(|error| {
        tracing::warn!(
            error = %error,
            "recall_feedback_stats: no se pudo contar 'pending'; mostrando 0"
        );
        0
    });
    (resolved, pending)
}

/// Friction log observability: aggregate friction stats across all sessions.
pub async fn friction_stats_handler() -> impl axum::response::IntoResponse {
    let stats = friction_log::get_global_friction_stats();
    Json(serde_json::json!({
        "ok": true,
        "total_sessions": stats.total_sessions,
        "total_workflows": stats.total_workflows,
        "total_events": stats.total_events,
        "manual_interventions": stats.manual_interventions,
        "routing_errors": stats.routing_errors,
        "routing_ambiguous": stats.routing_ambiguous,
        "coloquio_roundtrips": stats.coloquio_roundtrips,
        "timeouts": stats.timeouts,
        "retries": stats.retries,
        "guild_errors": stats.guild_errors,
        "avg_round_trips_per_workflow": stats.avg_round_trips_per_workflow,
        "total_friction_score": stats.total_friction_score,
    }))
}

#[cfg(test)]
mod tests {
    use super::feedback_counts;

    /// T-DSH-79: los fallos de los contadores avisan y el contrato (0) no cambia.
    #[test]
    fn feedback_counts_warns_and_keeps_zero_contract() {
        let captured = crate::test_support::capture_warns(|| {
            let counts = feedback_counts(
                Err(anyhow::anyhow!("boom-resolved")),
                Err(anyhow::anyhow!("boom-pending")),
            );
            assert_eq!(counts, (0, 0));
        });
        assert!(
            captured.contains("mostrando 0"),
            "el fallo debe avisar, got: {captured}"
        );
    }

    #[test]
    fn feedback_counts_does_not_warn_on_success() {
        let captured = crate::test_support::capture_warns(|| {
            assert_eq!(feedback_counts(Ok(7), Ok(3)), (7, 3));
        });
        assert!(
            !captured.contains("mostrando 0"),
            "un éxito no debe avisar, got: {captured}"
        );
    }
}
