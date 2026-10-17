//! ADR-020 Fase 2 — proveedor HTTP `/v1/systemone` de `tylluan_decide`
//! contra un SERVIDOR SIMULADO (nunca un modelo real). Cubre el diseño
//! aprobado (2026-10-16): sombra asíncrona en dos tiempos (INSERT reglas →
//! UPDATE modelo con `model_value`/`model_probs_json`/`agreed`/identidad/
//! latencia), rechazo de respuestas fuera de opciones, timeout sin tocar la
//! respuesta, abstención de reglas como estado final, consultas libres por
//! el carril síncrono, modo `off` sin llamadas, fallback a reglas con
//! endpoint no-loopback, compat de `provider = "rules"` y cortacircuitos.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use tokio::sync::RwLock;

use tylluan_kernel::config::{DecisionProviderConfig, DecisionProviderTable, TylluanConfig};
use tylluan_kernel::memory::silva::SilvaDB;
use tylluan_kernel::router::decision::DecisionService;
use tylluan_kernel::router::decision::api_types::{
    DecideRequest, DecideStatus, QuestionKind, QuestionSpec,
};

// ── Servidor simulado ───────────────────────────────────────────────────────

struct MockServer {
    url: String,
    hits: Arc<AtomicUsize>,
}

/// Mock de `POST /v1/systemone`: responde `{"answers": <responses>}` (o el
/// `status` pedido, con retardo opcional). Cuenta las peticiones recibidas.
async fn start_mock(responses: serde_json::Value, status: StatusCode, delay_ms: u64) -> MockServer {
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_for_handler = hits.clone();
    let app = Router::new().route(
        "/v1/systemone",
        post(move |_body: Json<serde_json::Value>| {
            let responses = responses.clone();
            let hits = hits_for_handler.clone();
            async move {
                hits.fetch_add(1, Ordering::SeqCst);
                if delay_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
                if status.is_success() {
                    (status, Json(serde_json::json!({ "answers": responses }))).into_response()
                } else {
                    (status, "simulated failure").into_response()
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    MockServer {
        url: format!("http://127.0.0.1:{}", addr.port()),
        hits,
    }
}

// ── Helpers de config y servicio ────────────────────────────────────────────

fn cfg_with_endpoint(endpoint: &str, shadow_timeout_ms: u64, extra: &str) -> TylluanConfig {
    let text = format!(
        r#"
[decision.provider]
kind = "systemone_http"
endpoint = "{endpoint}"
model = "test-model"
shadow_timeout_ms = {shadow_timeout_ms}
{extra}
"#
    );
    toml::from_str(&text).expect("config TOML with [decision.provider]")
}

async fn svc_for(cfg: TylluanConfig) -> (Arc<DecisionService>, Arc<SilvaDB>) {
    let db = Arc::new(SilvaDB::in_memory().await.unwrap());
    db.init().await.unwrap();
    let svc = Arc::new(DecisionService::new(Arc::new(RwLock::new(cfg)), Some(db.clone())));
    (svc, db)
}

fn route_spec() -> QuestionSpec {
    QuestionSpec {
        kind: QuestionKind::Choice,
        instructions: None,
        options: vec![
            "local_fast".into(),
            "local_deep".into(),
            "guild".into(),
            "human".into(),
        ],
        levels: vec![],
    }
}

fn contract_router(risk: f64) -> DecideRequest {
    DecideRequest {
        state: serde_json::json!({ "risk": risk }),
        questions: std::collections::BTreeMap::from([("route".to_string(), route_spec())]),
        contract: Some("scheduler_routing".to_string()),
        max_latency_ms: None,
    }
}

fn free_bool() -> DecideRequest {
    DecideRequest {
        state: serde_json::json!({ "anything": 1 }),
        questions: std::collections::BTreeMap::from([(
            "should_i".to_string(),
            QuestionSpec {
                kind: QuestionKind::Boolean,
                instructions: None,
                options: vec![],
                levels: vec![],
            },
        )]),
        contract: None,
        max_latency_ms: None,
    }
}

// ── Sombra asíncrona en dos tiempos ─────────────────────────────────────────

/// Contrato + shadow + modelo: la respuesta sigue siendo la de reglas (la
/// autoridad), y la fila se completa por UPDATE con los campos del modelo.
#[tokio::test(flavor = "multi_thread")]
async fn shadow_two_phase_updates_model_columns() {
    let mock = start_mock(
        serde_json::json!({ "route": { "probabilities": { "human": 0.8, "guild": 0.2 } } }),
        StatusCode::OK,
        0,
    )
    .await;
    let (svc, _db) = svc_for(cfg_with_endpoint(&mock.url, 5000, "")).await;

    let resp = svc.decide(contract_router(0.9), "test").await;
    assert_eq!(resp.status, DecideStatus::Ok);
    assert_eq!(
        resp.answers["route"].value.as_deref(),
        Some("human"),
        "la autoridad sigue siendo el proveedor de reglas"
    );
    assert_eq!(
        resp.model_identity.as_deref(),
        Some("systemone-http-v1:test-model"),
        "la identidad del modelo consultado viaja en la respuesta"
    );

    svc.drain_shadow_tasks().await;
    let rows = svc.read_shadow_log(5).await;
    assert_eq!(rows.len(), 1);
    let r = &rows[0];
    assert_eq!(r.rules_value.as_deref(), Some("human"));
    assert_eq!(r.model_value.as_deref(), Some("human"), "segundo tiempo completó la fila");
    assert_eq!(r.agreed, Some(1));
    assert_eq!(r.model_identity.as_deref(), Some("systemone-http-v1:test-model"));
    assert!(r.model_latency_ms.is_some(), "latencia del modelo registrada (v29)");
    assert!(
        r.model_probs_json.as_deref().unwrap_or("").contains("0.8"),
        "probabilidades del modelo registradas: {:?}",
        r.model_probs_json
    );
    assert_eq!(mock.hits.load(Ordering::SeqCst), 1);
}

/// Desacuerdo reglas↔modelo → `agreed = 0`; la respuesta no cambia.
#[tokio::test(flavor = "multi_thread")]
async fn shadow_disagreement_is_recorded_as_agreed_zero() {
    let mock = start_mock(
        serde_json::json!({ "route": { "probabilities": { "guild": 0.9, "human": 0.1 } } }),
        StatusCode::OK,
        0,
    )
    .await;
    let (svc, _db) = svc_for(cfg_with_endpoint(&mock.url, 5000, "")).await;

    let resp = svc.decide(contract_router(0.9), "test").await;
    assert_eq!(resp.answers["route"].value.as_deref(), Some("human"));
    svc.drain_shadow_tasks().await;
    let rows = svc.read_shadow_log(5).await;
    assert_eq!(rows[0].model_value.as_deref(), Some("guild"));
    assert_eq!(rows[0].agreed, Some(0));
}

/// Respuesta fuera del conjunto declarado → RECHAZADA: la fila queda NULL
/// (sin contaminar la evaluación pareada) y la respuesta sigue siendo reglas.
#[tokio::test(flavor = "multi_thread")]
async fn model_out_of_options_is_rejected_in_shadow() {
    let mock = start_mock(
        serde_json::json!({ "route": { "probabilities": { "nonsense": 1.0 } } }),
        StatusCode::OK,
        0,
    )
    .await;
    let (svc, _db) = svc_for(cfg_with_endpoint(&mock.url, 5000, "")).await;

    let resp = svc.decide(contract_router(0.9), "test").await;
    assert_eq!(resp.answers["route"].value.as_deref(), Some("human"));
    svc.drain_shadow_tasks().await;
    let rows = svc.read_shadow_log(5).await;
    assert!(rows[0].model_value.is_none(), "valor fuera de opciones → NULL");
    assert!(rows[0].agreed.is_none());
    assert_eq!(mock.hits.load(Ordering::SeqCst), 1, "el modelo SÍ fue consultado");
}

/// Timeout de la sombra → la respuesta sale inmediata y las filas quedan
/// NULL (fallo conservador del modelo, semántica de paired_eval).
#[tokio::test(flavor = "multi_thread")]
async fn shadow_timeout_leaves_rows_null() {
    let mock = start_mock(serde_json::json!({}), StatusCode::OK, 400).await;
    let (svc, _db) = svc_for(cfg_with_endpoint(&mock.url, 60, "")).await; // shadow_timeout 60ms

    let t0 = std::time::Instant::now();
    let resp = svc.decide(contract_router(0.9), "test").await;
    assert!(
        t0.elapsed() < Duration::from_millis(300),
        "la sombra no bloquea la respuesta"
    );
    assert_eq!(resp.answers["route"].value.as_deref(), Some("human"));
    svc.drain_shadow_tasks().await;
    let rows = svc.read_shadow_log(5).await;
    assert!(rows[0].model_value.is_none());
    assert!(rows[0].model_identity.is_none());
    assert!(rows[0].agreed.is_none());
    assert_eq!(mock.hits.load(Ordering::SeqCst), 1);
}

/// Abstención de reglas = estado final (diseño aprobado): el modelo NO se
/// consulta para rescatarla.
#[tokio::test(flavor = "multi_thread")]
async fn rules_abstention_is_final_and_model_is_not_called() {
    let mock = start_mock(
        serde_json::json!({ "nope": { "probabilities": { "true": 1.0 } } }),
        StatusCode::OK,
        0,
    )
    .await;
    let (svc, _db) = svc_for(cfg_with_endpoint(&mock.url, 5000, "")).await;

    let req = DecideRequest {
        state: serde_json::json!({}),
        questions: std::collections::BTreeMap::from([(
            "nope".to_string(),
            QuestionSpec {
                kind: QuestionKind::Boolean,
                instructions: None,
                options: vec![],
                levels: vec![],
            },
        )]),
        contract: Some("scheduler_routing".to_string()),
        max_latency_ms: None,
    };
    let resp = svc.decide(req, "test").await;
    assert_eq!(resp.status, DecideStatus::Degraded);
    assert!(resp.answers["nope"].abstained);
    assert_eq!(resp.model_identity, None, "sin preguntas con valor → sin consulta");

    svc.drain_shadow_tasks().await;
    assert_eq!(mock.hits.load(Ordering::SeqCst), 0, "el modelo no fue llamado");
    let rows = svc.read_shadow_log(5).await;
    assert!(rows[0].rules_value.is_none());
    assert!(rows[0].model_value.is_none());
}

// ── Carril síncrono (consultas libres) ──────────────────────────────────────

/// Pregunta libre con modelo → la responde el modelo con probabilidades; la
/// fila queda con `rules_value` NULL y `agreed` NULL.
#[tokio::test(flavor = "multi_thread")]
async fn free_question_is_answered_by_the_model_sync() {
    let mock = start_mock(
        serde_json::json!({ "should_i": { "probabilities": { "true": 0.9, "false": 0.1 } } }),
        StatusCode::OK,
        0,
    )
    .await;
    let (svc, _db) = svc_for(cfg_with_endpoint(&mock.url, 5000, "")).await;

    let resp = svc.decide(free_bool(), "test").await;
    assert_eq!(resp.status, DecideStatus::Ok);
    assert!(!resp.degraded);
    assert_eq!(resp.provider, "systemone-http-v1");
    assert_eq!(resp.model_identity.as_deref(), Some("systemone-http-v1:test-model"));
    let a = &resp.answers["should_i"];
    assert_eq!(a.value.as_deref(), Some("true"));
    assert!(!a.abstained);
    assert!((a.probabilities["true"] - 0.9).abs() < 1e-6);

    let rows = svc.read_shadow_log(5).await;
    assert_eq!(rows.len(), 1);
    let r = &rows[0];
    assert!(r.rules_value.is_none());
    assert_eq!(r.model_value.as_deref(), Some("true"));
    assert!(r.agreed.is_none(), "consulta libre: no hay regla con la que comparar");
    assert_eq!(r.provider, "systemone-http-v1");
    assert_eq!(r.model_identity.as_deref(), Some("systemone-http-v1:test-model"));
    assert!(r.model_probs_json.as_deref().unwrap_or("").contains("0.9"));
    assert_eq!(mock.hits.load(Ordering::SeqCst), 1);
}

/// Fuera de opciones en consulta libre → abstención explícita + degradado.
#[tokio::test(flavor = "multi_thread")]
async fn free_question_outside_options_abstains() {
    let mock = start_mock(
        serde_json::json!({ "should_i": { "probabilities": { "maybe": 1.0 } } }),
        StatusCode::OK,
        0,
    )
    .await;
    let (svc, _db) = svc_for(cfg_with_endpoint(&mock.url, 5000, "")).await;

    let resp = svc.decide(free_bool(), "test").await;
    assert_eq!(resp.status, DecideStatus::Degraded);
    let a = &resp.answers["should_i"];
    assert!(a.abstained);
    assert_eq!(a.abstain_reason.as_deref(), Some("answer_out_of_options"));
    let rows = svc.read_shadow_log(5).await;
    assert!(rows[0].model_value.is_none());
}

/// Timeout del carril síncrono → abstención `provider_timeout` + degradado,
/// con el deadline del wire respetado.
#[tokio::test(flavor = "multi_thread")]
async fn free_question_timeout_abstains_with_reason() {
    let mock = start_mock(serde_json::json!({}), StatusCode::OK, 400).await;
    let (svc, _db) = svc_for(cfg_with_endpoint(&mock.url, 5000, "")).await;

    let mut req = free_bool();
    req.max_latency_ms = Some(60);
    let t0 = std::time::Instant::now();
    let resp = svc.decide(req, "test").await;
    assert!(t0.elapsed() < Duration::from_millis(300));
    assert_eq!(resp.status, DecideStatus::Degraded);
    let a = &resp.answers["should_i"];
    assert!(a.abstained);
    assert_eq!(a.abstain_reason.as_deref(), Some("provider_timeout"));
    let rows = svc.read_shadow_log(5).await;
    assert!(rows[0].model_value.is_none());
    assert!(rows[0].model_latency_ms.is_some(), "la latencia del intento se registra");
}

// ── Modo off, fallback y compat ─────────────────────────────────────────────

/// Modo `off` en el contrato → CERO llamadas externas (ADR-021 §3.2),
/// aunque haya proveedor de modelo configurado.
#[tokio::test(flavor = "multi_thread")]
async fn mode_off_makes_no_model_call() {
    let mock = start_mock(
        serde_json::json!({ "route": { "probabilities": { "human": 1.0 } } }),
        StatusCode::OK,
        0,
    )
    .await;
    let extra = "[decision.contracts]\nscheduler_routing = \"off\"\n";
    let (svc, _db) = svc_for(cfg_with_endpoint(&mock.url, 5000, extra)).await;

    let resp = svc.decide(contract_router(0.9), "test").await;
    assert_eq!(resp.answers["route"].value.as_deref(), Some("human"));
    assert_eq!(resp.model_identity, None);
    svc.drain_shadow_tasks().await;
    assert_eq!(mock.hits.load(Ordering::SeqCst), 0, "off = sin llamada externa");
    let rows = svc.read_shadow_log(5).await;
    assert!(rows[0].model_value.is_none());
}

/// Endpoint no-loopback → rechazado en construcción: fallback silencioso a
/// reglas (fail-closed; el clínico nunca sale de la máquina).
#[tokio::test(flavor = "multi_thread")]
async fn non_loopback_endpoint_falls_back_to_rules() {
    let cfg = cfg_with_endpoint("http://10.0.0.5:8080", 5000, "");
    let (svc, _db) = svc_for(cfg).await;

    let resp = svc.decide(contract_router(0.9), "test").await;
    assert_eq!(resp.provider, "rules-v0");
    assert_eq!(resp.answers["route"].value.as_deref(), Some("human"));

    let free = svc.decide(free_bool(), "test").await;
    let a = &free.answers["should_i"];
    assert!(a.abstained);
    assert_eq!(a.abstain_reason.as_deref(), Some("no_model_provider"));
    assert_eq!(free.model_identity, None);
}

/// La forma histórica `provider = "rules"` sigue byte-idéntica (y cualquier
/// string desconocido también cae a reglas, sin arranque roto).
#[tokio::test(flavor = "multi_thread")]
async fn string_provider_rules_stays_byte_compatible() {
    for text in [
        "[decision]\nprovider = \"rules\"",
        "[decision]\nprovider = \"not-a-real-provider\"",
    ] {
        let cfg: TylluanConfig = toml::from_str(text).unwrap();
        let (svc, _db) = svc_for(cfg).await;
        let resp = svc.decide(contract_router(0.9), "test").await;
        assert_eq!(resp.provider, "rules-v0");
        assert_eq!(resp.answers["route"].value.as_deref(), Some("human"));
        let free = svc.decide(free_bool(), "test").await;
        assert!(free.answers["should_i"].abstained);
    }
}

// ── Cortacircuitos ──────────────────────────────────────────────────────────

/// 3 fallos consecutivos → abierto: la 4ª llamada NO llega al servidor.
#[tokio::test(flavor = "multi_thread")]
async fn breaker_opens_after_consecutive_failures() {
    let mock = start_mock(serde_json::json!({}), StatusCode::INTERNAL_SERVER_ERROR, 0).await;
    let (svc, _db) = svc_for(cfg_with_endpoint(&mock.url, 5000, "")).await;

    for _ in 0..3 {
        svc.decide(contract_router(0.2), "test").await;
        svc.drain_shadow_tasks().await;
    }
    assert_eq!(mock.hits.load(Ordering::SeqCst), 3, "3 intentos llegaron al servidor");

    svc.decide(contract_router(0.2), "test").await;
    svc.drain_shadow_tasks().await;
    assert_eq!(
        mock.hits.load(Ordering::SeqCst),
        3,
        "cortacircuitos abierto: la 4ª llamada no sale"
    );
    let rows = svc.read_shadow_log(10).await;
    assert!(rows.iter().all(|r| r.model_value.is_none()), "todos los fallos quedan NULL");
}

// ── Parseo de config ────────────────────────────────────────────────────────

#[tokio::test]
async fn config_parses_both_provider_forms_with_documented_defaults() {
    let s: TylluanConfig = toml::from_str("[decision]\nprovider = \"rules\"").unwrap();
    assert_eq!(
        s.decision.provider,
        DecisionProviderConfig::Name("rules".to_string())
    );

    let t: TylluanConfig = toml::from_str(
        "[decision.provider]\nkind = \"systemone_http\"\nendpoint = \"http://127.0.0.1:11436\"\nmodel = \"decima-small\"",
    )
    .unwrap();
    match t.decision.provider {
        DecisionProviderConfig::Table(DecisionProviderTable::SystemoneHttp(c)) => {
            assert_eq!(c.endpoint, "http://127.0.0.1:11436");
            assert_eq!(c.model, "decima-small");
            assert_eq!(c.request_timeout_ms, 5000);
            assert_eq!(c.shadow_timeout_ms, 30000);
            assert_eq!(c.max_inflight, 1);
            assert_eq!(c.breaker_failures, 3);
            assert_eq!(c.breaker_cooldown_s, 60);
            assert_eq!(c.state_max_chars, 0, "sin tope por defecto");
        }
        other => panic!("expected systemone_http table, got {other:?}"),
    }

    let defaulted: TylluanConfig = toml::from_str("").unwrap();
    assert_eq!(
        defaulted.decision.provider,
        DecisionProviderConfig::Name("rules".to_string())
    );
}

// ── T1241: 4xx de la ENTRADA no abren el cortacircuitos; 422 STATE_TRUNCATED → abstención estable ──

/// Mock crudo: responde `status` con un cuerpo arbitrario (para simular el
/// 422 STATE_TRUNCATED de Decima u otros 4xx). Cuenta las peticiones.
async fn start_mock_raw(status: StatusCode, body: &'static str) -> MockServer {
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_for_handler = hits.clone();
    let app = Router::new().route(
        "/v1/systemone",
        post(move || {
            let hits = hits_for_handler.clone();
            async move {
                hits.fetch_add(1, Ordering::SeqCst);
                (status, body).into_response()
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    MockServer {
        url: format!("http://127.0.0.1:{}", addr.port()),
        hits,
    }
}

/// T1241 (hallazgo en vivo 2026-10-17): un 4xx causado por la ENTRADA no
/// cuenta para el cortacircuitos — un llamante inválido no puede tumbar el
/// carril del modelo para todos. `breaker_failures = 1` lo hace observable
/// (con el bug, la 2ª llamada no llega al servidor).
#[tokio::test(flavor = "multi_thread")]
async fn client_4xx_does_not_count_toward_breaker() {
    let mock = start_mock_raw(StatusCode::BAD_REQUEST, "invalid entry").await;
    let (svc, _db) = svc_for(cfg_with_endpoint(&mock.url, 5000, "breaker_failures = 1")).await;

    let r1 = svc.decide(contract_router(0.9), "test").await;
    assert_eq!(
        r1.answers["route"].value.as_deref(),
        Some("human"),
        "las reglas siguen respondiendo (solo se degrada el carril modelo)"
    );
    svc.drain_shadow_tasks().await;
    let r2 = svc.decide(contract_router(0.9), "test").await;
    svc.drain_shadow_tasks().await;
    assert_eq!(r2.answers["route"].value.as_deref(), Some("human"));
    assert_eq!(
        mock.hits.load(Ordering::SeqCst),
        2,
        "el 4xx NO alimenta el cortacircuitos: las dos llamadas llegaron al servidor"
    );
}

/// T1241: 422 STATE_TRUNCATED (Decima) en consulta libre → abstención con el
/// motivo ESTABLE `state_too_long` (no un `provider_error:` con el dump del
/// HTTP), y sin abrir el cortacircuitos.
#[tokio::test(flavor = "multi_thread")]
async fn state_truncated_422_is_stable_abstention_and_breaker_safe() {
    let mock = start_mock_raw(
        StatusCode::UNPROCESSABLE_ENTITY,
        "STATE_TRUNCATED: state exceeds model limit",
    )
    .await;
    let (svc, _db) = svc_for(cfg_with_endpoint(&mock.url, 5000, "breaker_failures = 1")).await;

    let resp = svc.decide(free_bool(), "test").await;
    assert_eq!(resp.status, DecideStatus::Degraded);
    let a = &resp.answers["should_i"];
    assert!(a.abstained);
    assert_eq!(
        a.abstain_reason.as_deref(),
        Some("state_too_long"),
        "el 422 STATE_TRUNCATED se mapea a abstención con motivo estable"
    );

    let resp2 = svc.decide(free_bool(), "test").await;
    assert_eq!(resp2.status, DecideStatus::Degraded);
    assert_eq!(
        mock.hits.load(Ordering::SeqCst),
        2,
        "el 422 de la entrada NO abre el cortacircuitos: la segunda llamada salió"
    );
}

/// T1241: en el carril de contrato (sombra), el 422 STATE_TRUNCATED tampoco
/// cuenta para el cortacircuitos — las reglas responden y la fila del modelo
/// queda NULL (semántica de evaluación pareada intacta).
#[tokio::test(flavor = "multi_thread")]
async fn shadow_422_state_truncated_leaves_rows_null_and_breaker_closed() {
    let mock = start_mock_raw(
        StatusCode::UNPROCESSABLE_ENTITY,
        "STATE_TRUNCATED: state exceeds model limit",
    )
    .await;
    let (svc, _db) = svc_for(cfg_with_endpoint(&mock.url, 5000, "breaker_failures = 1")).await;

    let resp = svc.decide(contract_router(0.9), "test").await;
    assert_eq!(resp.answers["route"].value.as_deref(), Some("human"));
    svc.drain_shadow_tasks().await;
    let resp2 = svc.decide(contract_router(0.9), "test").await;
    svc.drain_shadow_tasks().await;
    assert_eq!(resp2.answers["route"].value.as_deref(), Some("human"));

    assert_eq!(
        mock.hits.load(Ordering::SeqCst),
        2,
        "el 422 de la entrada NO abre el cortacircuitos en la sombra"
    );
    let rows = svc.read_shadow_log(10).await;
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.model_value.is_none()), "las filas quedan NULL");
}

/// Guarda T1241: 429 y 5xx SÍ cuentan para el cortacircuitos (fallo del
/// carril, no de la entrada) — comportamiento previo intacto.
#[tokio::test(flavor = "multi_thread")]
async fn breaker_still_counts_429_and_5xx() {
    for (status, body) in [
        (StatusCode::TOO_MANY_REQUESTS, "slow down"),
        (StatusCode::SERVICE_UNAVAILABLE, "overloaded"),
    ] {
        let mock = start_mock_raw(status, body).await;
        let (svc, _db) = svc_for(cfg_with_endpoint(&mock.url, 5000, "breaker_failures = 1")).await;

        svc.decide(contract_router(0.9), "test").await;
        svc.drain_shadow_tasks().await;
        svc.decide(contract_router(0.9), "test").await;
        svc.drain_shadow_tasks().await;
        assert_eq!(
            mock.hits.load(Ordering::SeqCst),
            1,
            "HTTP {status}: tras el primer fallo el cortacircuitos abre y la 2ª llamada no sale"
        );
    }
}

/// T1241 (parte opcional): tope configurable `state_max_chars` — un estado
/// que se sabe que no cabe ni siquiera se envía (abstención `state_too_long`
/// sin coste de red); por debajo del tope, el carril funciona con normalidad.
#[tokio::test(flavor = "multi_thread")]
async fn state_max_chars_cap_skips_the_wire() {
    let mock = start_mock(
        serde_json::json!({ "route": { "probabilities": { "human": 1.0 } } }),
        StatusCode::OK,
        0,
    )
    .await;
    let (svc, _db) = svc_for(cfg_with_endpoint(&mock.url, 5000, "state_max_chars = 32")).await;

    // Estado pequeño (por debajo del tope) → el modelo se consulta.
    let resp = svc.decide(contract_router(0.9), "test").await;
    assert_eq!(resp.answers["route"].value.as_deref(), Some("human"));
    svc.drain_shadow_tasks().await;
    assert_eq!(mock.hits.load(Ordering::SeqCst), 1);

    // Estado por encima del tope → ni siquiera se envía; las reglas siguen.
    let big = DecideRequest {
        state: serde_json::json!({ "risk": 0.9, "pad": "x".repeat(64) }),
        questions: std::collections::BTreeMap::from([("route".to_string(), route_spec())]),
        contract: Some("scheduler_routing".to_string()),
        max_latency_ms: None,
    };
    let resp = svc.decide(big, "test").await;
    assert_eq!(
        resp.answers["route"].value.as_deref(),
        Some("human"),
        "las reglas siguen respondiendo"
    );
    svc.drain_shadow_tasks().await;
    assert_eq!(
        mock.hits.load(Ordering::SeqCst),
        1,
        "estado demasiado grande: cero llamadas nuevas al servidor"
    );
    let rows = svc.read_shadow_log(10).await;
    assert_eq!(rows.len(), 2);
    assert!(rows[0].model_value.is_none(), "la fila del estado grande queda NULL");
}
