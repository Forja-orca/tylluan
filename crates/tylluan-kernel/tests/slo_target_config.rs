//! TC-03 — `slo_target` es configurable (`[observability] slo_target`,
//! default 99.9) y los TRES endpoints de observabilidad lo consumen:
//! `/api/v1/dashboard/summary`, `/api/v1/health/golden-signals` y
//! `/api/v1/slo/summary`. Antes era el literal `99.9` hardcodeado en los
//! tres handlers (última instancia viva de MD-6) — estos tests fallan
//! contra ese estado y fijan el contrato del fix: valor desde config, sin
//! cambiar el esquema de respuesta.
//!
//! Nota de aislamiento: los `HttpState` del harness comparten la config
//! global cacheada (`TylluanConfig::load_cached()`), así que estos tests se
//! serializan con un lock para que cada uno controle su `slo_target` sin
//! carreras entre tests paralelos. El default 99.9 (config sin la sección)
//! queda cubierto por `observability_section_parses_and_defaults_to_99_9`,
//! que no toca estado compartido.

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use tylluan_kernel::transport::http::HttpState;
use tylluan_kernel::transport::http::build_router;

/// Serializa los tests del fichero (config global compartida, ver cabecera).
static SLO_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn get_json(app: axum::Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 4 << 20)
        .await
        .unwrap_or_default();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

async fn state_with_slo_target(tag: &str, target: f64) -> std::sync::Arc<HttpState> {
    let state = support::test_state(tag).await;
    state.config.write().await.observability.slo_target = target;
    state
}

/// `/api/v1/dashboard/summary` sirve el `slo_target` configurado.
#[tokio::test(flavor = "multi_thread")]
async fn dashboard_summary_serves_configured_slo_target() {
    let _guard = SLO_TEST_LOCK.lock().await;
    let state = state_with_slo_target("tc03_dash_custom", 98.25).await;
    let (status, json) = get_json(build_router(state), "/api/v1/dashboard/summary").await;
    assert!(status.is_success(), "dashboard/summary must answer 200, got {status}");
    // `dashboard/summary` anida el bloque de golden signals bajo `golden_signals`.
    assert_eq!(
        json["golden_signals"]["slo_target"],
        serde_json::json!(98.25),
        "slo_target must come from [observability], not a literal"
    );
}

/// `/api/v1/health/golden-signals` sirve el `slo_target` configurado.
#[tokio::test(flavor = "multi_thread")]
async fn golden_signals_serves_configured_slo_target() {
    let _guard = SLO_TEST_LOCK.lock().await;
    let state = state_with_slo_target("tc03_golden_custom", 97.5).await;
    let (status, json) = get_json(build_router(state), "/api/v1/health/golden-signals").await;
    assert!(status.is_success(), "golden-signals must answer 200, got {status}");
    assert_eq!(
        json["slo_target"],
        serde_json::json!(97.5),
        "slo_target must come from [observability], not a literal"
    );
}

/// `/api/v1/slo/summary`: el eco del objetivo, las bandas de `status` y el
/// error budget derivan TODOS del target configurado. La disponibilidad
/// medida en el harness es un dato del entorno: se lee de la respuesta y
/// se contrasta contra las bandas documentadas (healthy ≥ target;
/// degraded ≥ target − 1.0 pp; else violated) — con dos targets en lados
/// opuestos al menos uno cambia de banda respecto al literal hardcodeado.
#[tokio::test(flavor = "multi_thread")]
async fn slo_summary_thresholds_and_status_follow_configured_target() {
    let _guard = SLO_TEST_LOCK.lock().await;
    for (tag, target) in [("tc03_slo_low", 0.5_f64), ("tc03_slo_high", 100.5_f64)] {
        let state = state_with_slo_target(tag, target).await;
        let (status, json) = get_json(build_router(state), "/api/v1/slo/summary").await;
        assert_eq!(status, StatusCode::OK, "slo/summary must answer 200");
        assert_eq!(json["slo_target"], serde_json::json!(target));

        let avail = json["current_availability"]
            .as_f64()
            .expect("current_availability must be a number");
        let expected = if avail >= target {
            "healthy"
        } else if avail >= target - 1.0 {
            "degraded"
        } else {
            "violated"
        };
        assert_eq!(
            json["status"],
            serde_json::json!(expected),
            "status band must derive from the configured target ({target}), avail={avail}"
        );

        for field in ["error_budget_consumed_percent", "error_budget_remaining_percent"] {
            let v = json[field].as_f64().unwrap_or_else(|| panic!("{field} must be numeric"));
            assert!(
                (0.0..=100.0).contains(&v),
                "{field} must stay clamped to 0..=100, got {v}"
            );
        }
    }
}

/// `[observability]` parsea desde TOML y el default sigue siendo 99.9
/// (TylluanConfig completo sin la sección). No toca estado compartido:
/// no necesita el lock.
#[tokio::test]
async fn observability_section_parses_and_defaults_to_99_9() {
    let cfg: tylluan_kernel::config::TylluanConfig =
        toml::from_str("[observability]\nslo_target = 98.5").unwrap();
    assert_eq!(cfg.observability.slo_target, 98.5);

    let defaulted: tylluan_kernel::config::TylluanConfig = toml::from_str("").unwrap();
    assert_eq!(defaulted.observability.slo_target, 99.9);
    assert_eq!(tylluan_kernel::config::TylluanConfig::default().observability.slo_target, 99.9);
}
