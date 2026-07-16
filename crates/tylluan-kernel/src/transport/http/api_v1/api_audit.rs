use axum::Json;
use crate::transport::server::handler_do;

pub async fn audit_verify() -> impl axum::response::IntoResponse {
    match handler_do::verify_audit_chain() {
        Ok((ok, bad)) => {
            let status = if bad == 0 { "clean" } else { "tampered" };
            let message = if ok == 0 && bad == 0 {
                "No audit entries yet.".to_string()
            } else if bad == 0 {
                format!("✅ Chain integrity verified — {ok} entries intact.")
            } else {
                format!("🚨 Chain broken — {ok} valid, {bad} tampered entries.")
            };
            (axum::http::StatusCode::OK, Json(serde_json::json!({
                "ok": true,
                "status": status,
                "valid_count": ok,
                "tampered_count": bad,
                "message": message,
            })))
        }
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "ok": false, "error": e.to_string()
        }))),
    }
}
