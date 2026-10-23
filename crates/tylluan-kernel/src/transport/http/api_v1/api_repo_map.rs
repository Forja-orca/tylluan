use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;
use crate::repo_map::RepoMapHandle;
use crate::transport::http::HttpState;

/// D-39: respuesta pura (testeable sin montar el router) según el estado del
/// mapa. El arranque ya no espera al mapa: mientras se construye en segundo
/// plano, el endpoint responde 503 con un mensaje claro en vez de bloquear.
pub fn repo_map_response(handle: &RepoMapHandle) -> (StatusCode, serde_json::Value) {
    match handle.get() {
        Some(map) => match serde_json::to_value(&*map) {
            Ok(json) => (StatusCode::OK, json),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({
                    "error": "serialization failed",
                    "detail": e.to_string(),
                }),
            ),
        },
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            serde_json::json!({
                "error": "repo map en construccion",
                "status": handle.status(),
                "failure": handle.failure(),
                "retry_after_ms": 1000,
            }),
        ),
    }
}

/// M31-P4: GET /api/v1/repo-map
/// Returns the lightweight repo map built once at kernel startup.
pub async fn repo_map_handler(
    State(state): State<Arc<HttpState>>,
) -> impl IntoResponse {
    let (status, json) = repo_map_response(&state.repo_map);
    (status, Json(json))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo_map::RepoMap;

    #[test]
    fn test_repo_map_response_503_while_building() {
        let handle = RepoMapHandle::building();
        let (status, json) = repo_map_response(&handle);
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(json["status"], "building");
        assert!(json["error"].as_str().unwrap().contains("construccion"));
    }

    #[test]
    fn test_repo_map_response_200_when_ready() {
        let dir = tempfile::tempdir().unwrap();
        let handle = RepoMapHandle::ready(RepoMap::build(dir.path()));
        let (status, json) = repo_map_response(&handle);
        assert_eq!(status, StatusCode::OK);
        assert!(json["total_files"].as_u64().is_some());
    }

    #[test]
    fn test_repo_map_response_503_reports_failure() {
        let handle = RepoMapHandle::building();
        handle.set_failed("boom");
        let (status, json) = repo_map_response(&handle);
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(json["status"], "failed");
        assert_eq!(json["failure"], "boom");
    }
}
