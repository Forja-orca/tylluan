//! # DecisionService — núcleo de `tylluan_decide` (ADR-021, Fase 1 del reparto)
//!
//! Semántica fijada por el TL (Coloquio 1217/1218):
//! - **Interruptor global**: apagado → `status=disabled`, `answers` vacío,
//!   CERO llamadas (ni auditoría: no hay decisión que registrar).
//! - **Contratos internos nombrados** (`scheduler_routing`, `human_review`…):
//!   los responde el proveedor de reglas existente (`rules.rs`, intacto).
//! - **Preguntas libres** (sin contrato) sin proveedor de modelo:
//!   `abstained=true`, `abstain_reason="no_model_provider"` (§1217).
//! - **Regla aditiva**: una respuesta fuera de las opciones declaradas se
//!   RECHAZA (abstención `answer_out_of_options`); el modelo nunca revierte
//!   un rechazo determinista; abstención explícita siempre.
//! - **Auditoría** en `decision_shadow_log` con SOLO el hash del `state`
//!   (nunca en claro): columnas exactas del §1217.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::RwLock;

use crate::config::TylluanConfig;
use crate::memory::silva::SilvaDB;

use super::api_types::{
    ABSTAIN_ANSWER_OUT_OF_OPTIONS, ABSTAIN_NO_MODEL_PROVIDER, Answer, DecideRequest,
    DecideResponse, DecideStatus, DecisionMode, abstain, answer_from_core, to_core_question,
    validate_request,
};
use super::{DecisionProvider, DecisionRequest, rules::RulesDecisionProvider};

/// Clave de unión de una fila de `decision_shadow_log` con una verdad
/// externa: SHA-256 hex de `serde_json::to_string(state)`, un separador 0x1F
/// y el nombre de la pregunta. Es la única forma en que el `state` aparece en
/// la BD.
pub fn decision_row_hash(state: &serde_json::Value, question: &str) -> String {
    use sha2::Digest;
    let canonical = serde_json::to_string(state).unwrap_or_else(|_| "null".to_string());
    let mut h = sha2::Sha256::new();
    h.update(canonical.as_bytes());
    h.update([0x1f]);
    h.update(question.as_bytes());
    format!("{:x}", h.finalize())
}

/// Núcleo del servicio. Construirlo NO abre puertas.
pub struct DecisionService {
    config: Arc<RwLock<TylluanConfig>>,
    provider: Arc<dyn DecisionProvider>,
    /// SilvaDB para `decision_shadow_log`. `None` desactiva solo el registro.
    silva: Option<Arc<SilvaDB>>,
}

/// Una fila de `decision_shadow_log` (columnas EXACTAS del §1217).
#[derive(Debug, Clone)]
pub struct ShadowLogRow {
    pub id: i64,
    pub ts: String,
    pub contract: String,
    pub request_hash: String,
    pub rules_value: Option<String>,
    pub model_value: Option<String>,
    pub model_probs_json: Option<String>,
    pub provider: String,
    pub latency_ms: i64,
    pub agreed: Option<i64>,
    pub mode: String,
}

impl DecisionService {
    /// Fase 1: `[decision] provider` solo admite `"rules"`; cualquier otro
    /// valor avisa y cae al baseline determinista (nunca un arranque roto).
    pub fn new(config: Arc<RwLock<TylluanConfig>>, silva: Option<Arc<SilvaDB>>) -> Self {
        let provider: Arc<dyn DecisionProvider> = Arc::new(RulesDecisionProvider::new());
        Self { config, provider, silva }
    }

    pub fn provider_id(&self) -> &'static str {
        self.provider.provider_id()
    }

    /// Decide `req`. `caller` identifica la puerta ("mcp", "rest",
    /// "module:<nombre>") para trazabilidad; el esquema de la tabla no
    /// incluye columna de caller (esquema único del TL).
    pub async fn decide(&self, req: DecideRequest, caller: &str) -> DecideResponse {
        let started = Instant::now();

        let (enabled, configured_provider, contract_mode, shadow_log) = {
            let cfg = self.config.read().await;
            let d = &cfg.decision;
            let contract_mode = req
                .contract
                .as_ref()
                .and_then(|c| d.contracts.get(c).copied())
                .unwrap_or(d.default_mode);
            (d.enabled, d.provider.clone(), contract_mode, d.shadow_log)
        };

        if !enabled {
            // Cero efectos: ni proveedor ni auditoría (no hay decisión).
            return DecideResponse::disabled();
        }

        if configured_provider != "rules" {
            tracing::warn!(
                "decision: [decision] provider='{configured_provider}' is not implemented yet — using rules baseline"
            );
        }

        if let Err(clear) = validate_request(&req) {
            tracing::warn!("decision: invalid request from caller '{caller}': {clear}");
            return DecideResponse::error();
        }

        let mode = if req.contract.is_some() {
            contract_mode
        } else {
            DecisionMode::default()
        };
        tracing::debug!(
            "decision: caller='{caller}' contract={:?} mode={}",
            req.contract,
            mode.as_str()
        );

        // Consulta LIBRE sin proveedor de modelo (§1217): abstención explícita.
        if req.contract.is_none() {
            let answers: BTreeMap<String, Answer> = req
                .questions
                .keys()
                .map(|name| (name.clone(), abstain(ABSTAIN_NO_MODEL_PROVIDER)))
                .collect();
            let response = DecideResponse {
                request_id: uuid::Uuid::new_v4().to_string(),
                status: DecideStatus::Degraded,
                mode,
                provider: self.provider_id().to_string(),
                model_identity: None,
                degraded: true,
                answers,
            };
            if shadow_log {
                self.log_shadow(&req, &response, started).await;
            }
            return response;
        }

        // Contrato nombrado → proveedor de reglas existente.
        let core_req = DecisionRequest {
            state: req.state.clone(),
            questions: req
                .questions
                .iter()
                .map(|(n, s)| (n.clone(), to_core_question(s)))
                .collect(),
        };

        let (answers, degraded) = match self.provider.decide(&core_req).await {
            Ok(core_answers) => {
                let mut degraded = false;
                let mut wire: BTreeMap<String, Answer> = BTreeMap::new();
                for (name, spec) in &req.questions {
                    match core_answers.get(name) {
                        Some(core) => {
                            let a = answer_from_core(core);
                            // Respuesta fuera de las opciones declaradas:
                            // RECHAZADA (§1217) — nunca se reenvía.
                            let out_of_set = a
                                .value
                                .as_deref()
                                .map(|v| !spec.allowed_values().contains(&v))
                                .unwrap_or(false);
                            if out_of_set {
                                degraded = true;
                                wire.insert(
                                    name.clone(),
                                    abstain(ABSTAIN_ANSWER_OUT_OF_OPTIONS),
                                );
                            } else {
                                wire.insert(name.clone(), a);
                            }
                        }
                        None => {
                            degraded = true;
                            wire.insert(
                                name.clone(),
                                abstain("provider returned no answer for this question"),
                            );
                        }
                    }
                }
                (wire, degraded)
            }
            Err(e) => {
                // El proveedor falla → abstención explícita por pregunta; el
                // llamante aplica su regla de reserva (§3.4).
                let reason = format!("provider error: {e}");
                let wire = req
                    .questions
                    .keys()
                    .map(|name| (name.clone(), abstain(reason.clone())))
                    .collect();
                (wire, true)
            }
        };

        let response = DecideResponse {
            request_id: uuid::Uuid::new_v4().to_string(),
            status: if degraded { DecideStatus::Degraded } else { DecideStatus::Ok },
            mode,
            provider: self.provider_id().to_string(),
            model_identity: None,
            degraded,
            answers,
        };

        if shadow_log {
            self.log_shadow(&req, &response, started).await;
        }
        response
    }

    /// Fila de auditoría (§1217), UNA POR PREGUNTA: así cada fila es una
    /// decisión comparable con una verdad externa (evaluación pareada).
    /// `request_hash` = [`decision_row_hash`] (SHA-256 de `state` + nombre de
    /// la pregunta); el `state` nunca se guarda en claro. `rules_value` es el
    /// valor simple de la respuesta (NULL si hubo abstención);
    /// `model_value`/`model_probs_json`/`agreed` quedan NULL hasta que exista
    /// proveedor de modelo. Un fallo de escritura se avisa y no rompe la decisión.
    async fn log_shadow(&self, req: &DecideRequest, resp: &DecideResponse, started: Instant) {
        let Some(silva) = &self.silva else {
            return;
        };
        let ts = chrono::Utc::now().to_rfc3339();
        let latency_ms = started.elapsed().as_millis() as i64;
        let contract = req.contract.clone().unwrap_or_default();
        let g = silva.conn_lock();
        let conn = g.lock().await;
        for (question, answer) in &resp.answers {
            let result = conn.execute(
                "INSERT INTO decision_shadow_log
                 (ts, contract, request_hash, rules_value, model_value, model_probs_json,
                  provider, latency_ms, agreed, mode)
                 VALUES (?1, ?2, ?3, ?4, NULL, NULL, ?5, ?6, NULL, ?7)",
                rusqlite::params![
                    ts,
                    contract,
                    decision_row_hash(&req.state, question),
                    answer.value,
                    resp.provider,
                    latency_ms,
                    resp.mode.as_str(),
                ],
            );
            if let Err(e) = result {
                tracing::warn!("decision_shadow_log write failed: {e}");
            }
        }
    }

    /// Lectura de auditoría (tests y Fase 2). Más recientes primero.
    pub async fn read_shadow_log(&self, limit: usize) -> Vec<ShadowLogRow> {
        let Some(silva) = &self.silva else {
            return Vec::new();
        };
        let g = silva.conn_lock();
        let conn = g.lock().await;
        let mut stmt = match conn.prepare(
            "SELECT id, ts, contract, request_hash, rules_value, model_value, model_probs_json,
                    provider, latency_ms, agreed, mode
             FROM decision_shadow_log ORDER BY id DESC LIMIT ?1",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        stmt.query_map(rusqlite::params![limit as i64], |r| {
            Ok(ShadowLogRow {
                id: r.get(0)?,
                ts: r.get(1)?,
                contract: r.get(2)?,
                request_hash: r.get(3)?,
                rules_value: r.get(4)?,
                model_value: r.get(5)?,
                model_probs_json: r.get(6)?,
                provider: r.get(7)?,
                latency_ms: r.get(8)?,
                agreed: r.get(9)?,
                mode: r.get(10)?,
            })
        })
        .map(|it| it.filter_map(Result::ok).collect())
        .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::decision::api_types::QuestionKind;

    async fn silva_mem() -> Arc<SilvaDB> {
        let db = Arc::new(SilvaDB::in_memory().await.expect("in-memory silva"));
        db.init().await.expect("init");
        db
    }

    fn cfg_default() -> Arc<RwLock<TylluanConfig>> {
        Arc::new(RwLock::new(TylluanConfig::default()))
    }

    fn route_spec() -> super::super::api_types::QuestionSpec {
        super::super::api_types::QuestionSpec {
            kind: QuestionKind::Choice,
            instructions: None,
            options: vec!["local_fast".into(), "local_deep".into(), "guild".into(), "human".into()],
            levels: vec![],
        }
    }

    fn contract_req(risk: f64) -> DecideRequest {
        DecideRequest {
            state: serde_json::json!({ "risk": risk }),
            questions: BTreeMap::from([("route".to_string(), route_spec())]),
            contract: Some("scheduler_routing".to_string()),
            max_latency_ms: None,
        }
    }

    /// Defaults de `[decision]` (§1217/1218): ON, contratos decididos en
    /// Shadow, resto off, límites del esquema.
    #[test]
    fn decision_config_defaults_match_the_canonical_schema() {
        let d = crate::config::DecisionConfig::default();
        assert!(d.enabled, "tool ON por defecto (José)");
        assert_eq!(d.contracts.get("scheduler_routing"), Some(&DecisionMode::Shadow));
        assert_eq!(d.contracts.get("human_review"), Some(&DecisionMode::Shadow));
        assert_eq!(d.default_mode, DecisionMode::Off);
        assert_eq!(d.provider, "rules");
        assert_eq!(d.max_questions, 16);
        assert_eq!(d.max_state_bytes, 16 * 1024);
        assert!(d.shadow_log);
    }

    /// §1217: apagado → status=disabled, answers vacío, cero filas.
    #[tokio::test(flavor = "multi_thread")]
    async fn disabled_service_disables_and_has_zero_side_effects() {
        let db = silva_mem().await;
        let mut cfg = TylluanConfig::default();
        cfg.decision.enabled = false;
        let svc = DecisionService::new(Arc::new(RwLock::new(cfg)), Some(db.clone()));
        let resp = svc.decide(contract_req(0.1), "mcp").await;
        assert_eq!(resp.status, DecideStatus::Disabled);
        assert!(resp.answers.is_empty());
        assert_eq!(resp.provider, "none");
        assert!(svc.read_shadow_log(10).await.is_empty());
    }

    /// §1217: pregunta libre sin proveedor de modelo → abstain
    /// `no_model_provider` + degradado.
    #[tokio::test(flavor = "multi_thread")]
    async fn free_questions_abstain_with_no_model_provider() {
        let db = silva_mem().await;
        let svc = DecisionService::new(cfg_default(), Some(db.clone()));
        let req = DecideRequest {
            state: serde_json::json!({"anything": 1}),
            questions: BTreeMap::from([(
                "should_i".to_string(),
                super::super::api_types::QuestionSpec {
                    kind: QuestionKind::Boolean,
                    instructions: None,
                    options: vec![],
                    levels: vec![],
                },
            )]),
            contract: None,
            max_latency_ms: None,
        };
        let resp = svc.decide(req, "mcp").await;
        assert_eq!(resp.status, DecideStatus::Degraded);
        assert!(resp.degraded);
        let a = resp.answers.get("should_i").unwrap();
        assert!(a.abstained);
        assert_eq!(a.abstain_reason.as_deref(), Some("no_model_provider"));
        // También queda auditada (una fila por pregunta); la abstención no
        // tiene valor de reglas.
        let rows = svc.read_shadow_log(5).await;
        assert_eq!(rows.len(), 1);
        assert!(rows[0].rules_value.is_none());
    }

    /// Contrato nombrado → reglas responden; fila canónica con hash y NULLs.
    #[tokio::test(flavor = "multi_thread")]
    async fn named_contract_is_answered_by_rules_and_logged_canonically() {
        let db = silva_mem().await;
        let svc = DecisionService::new(cfg_default(), Some(db.clone()));
        let resp = svc.decide(contract_req(0.9), "rest").await;
        assert_eq!(resp.status, DecideStatus::Ok, "clean rules answer");
        assert!(!resp.degraded);
        assert_eq!(resp.provider, "rules-v0");
        assert_eq!(resp.mode, DecisionMode::Shadow, "scheduler_routing default");
        let a = resp.answers.get("route").unwrap();
        assert_eq!(a.value.as_deref(), Some("human"));
        assert!(!a.abstained);

        let rows = svc.read_shadow_log(5).await;
        assert_eq!(rows.len(), 1);
        let r = &rows[0];
        assert_eq!(r.contract, "scheduler_routing");
        assert_eq!(r.provider, "rules-v0");
        assert_eq!(r.mode, "shadow");
        assert_eq!(r.request_hash.len(), 64, "sha256 hex of state");
        assert_eq!(r.rules_value.as_deref(), Some("human"), "valor simple, no JSON");
        assert_eq!(
            r.request_hash,
            decision_row_hash(&serde_json::json!({ "risk": 0.9 }), "route")
        );
        assert!(r.model_value.is_none(), "NULL hasta Fase 2");
        assert!(r.model_probs_json.is_none());
        assert!(r.agreed.is_none());
        assert!(!r.ts.is_empty());
    }

    /// Pregunta desconocida dentro de un contrato → abstención con el error
    /// del proveedor + degradado (nunca respuesta inventada).
    #[tokio::test(flavor = "multi_thread")]
    async fn unknown_question_inside_contract_degrades_to_abstain() {
        let db = silva_mem().await;
        let svc = DecisionService::new(cfg_default(), Some(db.clone()));
        let req = DecideRequest {
            state: serde_json::json!({}),
            questions: BTreeMap::from([(
                "nope".to_string(),
                super::super::api_types::QuestionSpec {
                    kind: QuestionKind::Boolean,
                    instructions: None,
                    options: vec![],
                    levels: vec![],
                },
            )]),
            contract: Some("scheduler_routing".to_string()),
            max_latency_ms: None,
        };
        let resp = svc.decide(req, "mcp").await;
        assert_eq!(resp.status, DecideStatus::Degraded);
        let a = resp.answers.get("nope").unwrap();
        assert!(a.abstained);
        assert!(a.abstain_reason.as_deref().unwrap_or("").contains("provider error"));
    }

    /// Validación fail-closed: 17 preguntas → status error, sin filas.
    #[tokio::test(flavor = "multi_thread")]
    async fn invalid_request_returns_error_status_without_audit_row() {
        let db = silva_mem().await;
        let svc = DecisionService::new(cfg_default(), Some(db.clone()));
        let mut questions = BTreeMap::new();
        for i in 0..17 {
            questions.insert(format!("q{i}"), route_spec());
        }
        let req = DecideRequest {
            state: serde_json::json!({}),
            questions,
            contract: Some("scheduler_routing".to_string()),
            max_latency_ms: None,
        };
        let resp = svc.decide(req, "mcp").await;
        assert_eq!(resp.status, DecideStatus::Error);
        assert!(resp.answers.is_empty());
        assert!(svc.read_shadow_log(5).await.is_empty(), "no decision → no row");
    }

    /// Sin SilvaDB se decide igual (el registro es opcional, la decisión no).
    #[tokio::test(flavor = "multi_thread")]
    async fn service_decides_without_silva() {
        let svc = DecisionService::new(cfg_default(), None);
        let resp = svc.decide(contract_req(0.1), "rest").await;
        assert_eq!(resp.answers["route"].value.as_deref(), Some("local_fast"));
        assert!(svc.read_shadow_log(5).await.is_empty());
    }
}
