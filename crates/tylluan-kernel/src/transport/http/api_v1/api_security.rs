use axum::{Json, extract::State};
use std::sync::Arc;

use crate::security::coherence_gate;
use crate::security::friction_log;
use crate::transport::http::HttpState;
use crate::security::sentinel::{self, SentinelMode};

/// ADR-011 §2.5 observability: cumulative Coherence Gate counters since kernel start.
///
/// H-53 (C-3): el endpoint ya existía y solo exponía 3 contadores. Para poder
/// **validar** `coherence_semantic_scope` en vivo hacen falta dos cosas más, y
/// entran aquí sin ruta nueva:
/// - `semantic_scope`: el valor de `[security] coherence_semantic_scope` leído
///   de la config (el mismo que `main.rs` fija al arrancar; si el fichero cambia
///   tras el arranque, lo que está EN EJECUCIÓN sigue siendo el del arranque —
///   el valor mostrado es el de la config en este instante, no un snapshot del
///   proceso).
/// - Los contadores de capa semántica (`total_semantic_penalized` y compañía),
///   que son la evidencia medible del efecto del scope.
pub async fn coherence_gate_stats() -> impl axum::response::IntoResponse {
    let stats = coherence_gate::cumulative_stats();
    let cfg_scope = crate::config::TylluanConfig::load()
        .map(|c| c.security.coherence_semantic_scope)
        .unwrap_or_else(|_| coherence_gate::SEMANTIC_SCOPE_ALL.to_string());
    let known = matches!(
        cfg_scope.as_str(),
        coherence_gate::SEMANTIC_SCOPE_ALL | coherence_gate::SEMANTIC_SCOPE_CONFLICTING_TOP
    );
    Json(serde_json::json!({
        "ok": true,
        "total_seen": stats.total_seen,
        "total_eliminated": stats.total_eliminated,
        "total_penalized": stats.total_penalized,
        "total_pattern_eliminated": stats.total_pattern_eliminated,
        "total_provenance_penalized": stats.total_provenance_penalized,
        "total_semantic_penalized": stats.total_semantic_penalized,
        "semantic_scope": cfg_scope,
        "semantic_scope_known": known,
        "semantic_scope_note": "valor de [security] coherence_semantic_scope en la config actual; el proceso fija el suyo al arrancar (main.rs), un cambio posterior en el fichero no se aplica sin reinicio. Un valor desconocido se comporta como 'all' (semantic_scope_penalizes: match _ => true).",
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


// ── T-DSH-114: interruptor del centinela (MCP + REST) ────────────────────────

/// `GET /api/v1/security/sentinel` — estado del centinela (sin tokens ni rutas).
pub async fn sentinel_status_handler() -> impl axum::response::IntoResponse {
    axum::Json(sentinel::health_json())
}

#[derive(serde::Deserialize)]
pub struct SentinelModePayload {
    pub mode: String,
}

/// `POST /api/v1/security/sentinel/mode` — cambia el modo en caliente.
/// Misma clase de auth que las rutas de config (ver `operator_class_required`).
pub async fn set_sentinel_mode_handler(
    axum::Json(p): axum::Json<SentinelModePayload>,
) -> impl axum::response::IntoResponse {
    let raw = p.mode.trim().to_ascii_lowercase();
    let mode = match raw.as_str() {
        "off" => SentinelMode::Off,
        "observe" => SentinelMode::Observe,
        "enforce" => SentinelMode::Enforce,
        _ => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({
                    "ok": false,
                    "error": "mode inválido: usa off|observe|enforce",
                })),
            );
        }
    };
    let previous = sentinel::set_mode(mode);
    (
        axum::http::StatusCode::OK,
        axum::Json(serde_json::json!({
            "ok": true,
            "previous": previous.as_str(),
            "mode": mode.as_str(),
            "state": sentinel::health_json(),
        })),
    )
}
