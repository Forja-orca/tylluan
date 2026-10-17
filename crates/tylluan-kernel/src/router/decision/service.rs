//! # DecisionService — núcleo de `tylluan_decide` (ADR-021 Fase 1; ADR-020 Fase 2)
//!
//! Semántica fijada por el TL (Coloquio 1217/1218) y por el diseño de Fase 2
//! aprobado por José (2026-10-16):
//! - **Interruptor global**: apagado → `status=disabled`, `answers` vacío,
//!   CERO llamadas (ni auditoría: no hay decisión que registrar).
//! - **Contratos internos nombrados** (`scheduler_routing`, `human_review`…):
//!   los responde el proveedor de reglas existente (`rules.rs`, intacto);
//!   con un proveedor de modelo configurado y modo ≠ `off`, además se lanza
//!   una consulta EN SOMBRA ASÍNCRONA (dos tiempos) que compara y registra.
//! - **Preguntas libres** (sin contrato): sin proveedor de modelo → abstención
//!   `no_model_provider` (§1217); con proveedor → las responde el modelo en
//!   el carril síncrono (deadline = min(`max_latency_ms`, `request_timeout_ms`)).
//! - **Regla aditiva**: una respuesta fuera de las opciones declaradas se
//!   RECHAZA (abstención `answer_out_of_options`); el modelo nunca revierte un
//!   rechazo determinista, y una ABSTENCIÓN DE REGLAS ES ESTADO FINAL (no se
//!   consulta al modelo para rescatarla — diseño aprobado).
//! - **Auditoría** en `decision_shadow_log` con SOLO el hash del `state`
//!   (nunca en claro): columnas del §1217 + `model_identity`/`model_latency_ms`
//!   (v29). Sombra en dos tiempos: INSERT con las reglas → UPDATE con el
//!   modelo (`model_value`, `model_probs_json`, `agreed` 1/0/NULL).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;
use tokio::task::JoinHandle;

use crate::config::{DecisionProviderConfig, DecisionProviderTable, TylluanConfig};
use crate::memory::silva::SilvaDB;

use super::api_types::{
    ABSTAIN_ANSWER_OUT_OF_OPTIONS, ABSTAIN_NO_MODEL_PROVIDER, Answer, DEFAULT_MAX_LATENCY_MS,
    DecideRequest, DecideResponse, DecideStatus, DecisionMode, QuestionSpec, abstain,
    answer_from_core, to_core_question, validate_request,
};
use super::systemone_http::SystemOneHttpProvider;
use super::{DecisionAnswer, DecisionProvider, DecisionQuestion, DecisionRequest, rules::RulesDecisionProvider};

/// Motivo de abstención del carril síncrono cuando el modelo excede su deadline.
const ABSTAIN_PROVIDER_TIMEOUT: &str = "provider_timeout";

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

/// Proveedor de modelo de Fase 2 + parámetros de sus dos carriles.
#[derive(Clone)]
struct ModelSlot {
    provider: Arc<dyn DecisionProvider>,
    /// `"systemone-http-v1:<model>"` — viaja en la respuesta y en la fila.
    identity: String,
    /// Techo del carril síncrono (consultas libres).
    request_timeout: Duration,
    /// Deadline del carril asíncrono (contratos en sombra).
    shadow_timeout: Duration,
}

/// Objetivo de la sombra asíncrona: pregunta + spec + valor de reglas (que
/// es estado final) + fila a actualizar en el segundo tiempo.
#[derive(Clone)]
struct ShadowTarget {
    name: String,
    spec: QuestionSpec,
    rules_value: String,
    row_id: i64,
}

/// Núcleo del servicio. Construirlo NO abre puertas.
pub struct DecisionService {
    config: Arc<RwLock<TylluanConfig>>,
    provider: Arc<dyn DecisionProvider>,
    /// Slot del proveedor de modelo (Fase 2). `None` = solo reglas.
    model: Option<ModelSlot>,
    /// SilvaDB para `decision_shadow_log`. `None` desactiva solo el registro.
    silva: Option<Arc<SilvaDB>>,
    /// Tareas de sombra en vuelo (dos tiempos) — drenables por tests y
    /// apagado ordenado; los handles terminados se podan al añadir otro.
    shadow_tasks: std::sync::Mutex<Vec<JoinHandle<()>>>,
}

/// Una fila de `decision_shadow_log` (columnas §1217 + v29).
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
    /// v29: identidad del modelo que respondió (NULL si no respondió).
    pub model_identity: Option<String>,
    /// v29: latencia de la llamada al modelo en ms (NULL si no respondió).
    pub model_latency_ms: Option<i64>,
}

/// Resuelve el proveedor de modelo Fase 2 desde `[decision.provider]`.
/// Fuera de la impl: se llama en construcción (sin lock retenido más allá del
/// `try_read`). Cualquier config inválida o endpoint no-loopback avisa y cae
/// al baseline de reglas: nunca un arranque roto, nunca una salida de red no
/// aprobada (fail-closed).
fn resolve_model_slot(config: &Arc<RwLock<TylluanConfig>>) -> Option<ModelSlot> {
    let cfg = match config.try_read() {
        Ok(c) => c.clone(),
        Err(_) => {
            tracing::warn!(
                "decision: config lock contended at construction — model provider not resolved (rules only)"
            );
            return None;
        }
    };
    match &cfg.decision.provider {
        DecisionProviderConfig::Name(name) => {
            if name != "rules" {
                tracing::warn!(
                    "decision: [decision] provider='{name}' is not implemented yet — using rules baseline"
                );
            }
            None
        }
        DecisionProviderConfig::Table(DecisionProviderTable::SystemoneHttp(sc)) => {
            match SystemOneHttpProvider::from_config(sc) {
                Ok(p) => {
                    let identity = format!("{}:{}", p.provider_id(), sc.model);
                    tracing::info!(
                        "decision: systemone_http provider ready ({identity}); loopback-only phase 2"
                    );
                    Some(ModelSlot {
                        provider: Arc::new(p),
                        identity,
                        request_timeout: Duration::from_millis(sc.request_timeout_ms),
                        shadow_timeout: Duration::from_millis(sc.shadow_timeout_ms),
                    })
                }
                Err(e) => {
                    tracing::warn!(
                        "decision: [decision.provider] systemone_http rejected ({e}) — using rules baseline"
                    );
                    None
                }
            }
        }
    }
}

/// Argmax + JSON compacto SOLO si está dentro del conjunto cerrado de la
/// pregunta. Fuera de opciones → `None` (la respuesta se RECHAZA, §1217; el
/// valor violador no contamina la evaluación pareada). Traza `warn`.
fn validated_argmax(core: &DecisionAnswer, spec: &QuestionSpec) -> Option<(String, String)> {
    let (value, _) = core.distribution.iter().max_by(|a, b| a.1.total_cmp(b.1))?;
    if !spec.allowed_values().contains(&value.as_str()) {
        tracing::warn!(
            "decision shadow: model answered '{value}' outside the declared set — rejected"
        );
        return None;
    }
    let probs_json = serde_json::to_string(&core.distribution).unwrap_or_default();
    Some((value.clone(), probs_json))
}

impl DecisionService {
    /// Resuelve el proveedor desde `[decision] provider`. La sección se lee
    /// al construir (igual que el arranque del kernel; sin recarga en
    /// caliente, ADR-021 §13).
    pub fn new(config: Arc<RwLock<TylluanConfig>>, silva: Option<Arc<SilvaDB>>) -> Self {
        let provider: Arc<dyn DecisionProvider> = Arc::new(RulesDecisionProvider::new());
        let model = resolve_model_slot(&config);
        Self {
            config,
            provider,
            model,
            silva,
            shadow_tasks: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn provider_id(&self) -> &'static str {
        self.provider.provider_id()
    }

    /// Espera a que terminen las tareas de sombra en vuelo (tests y apagado
    /// ordenado). No afecta a `decide()`.
    pub async fn drain_shadow_tasks(&self) {
        let handles: Vec<JoinHandle<()>> = match self.shadow_tasks.lock() {
            Ok(mut h) => std::mem::take(&mut *h),
            Err(_) => Vec::new(),
        };
        for h in handles {
            let _ = h.await;
        }
    }

    /// Decide `req`. `caller` identifica la puerta ("mcp", "rest",
    /// "module:<nombre>") para trazabilidad; el esquema de la tabla no
    /// incluye columna de caller (esquema único del TL).
    pub async fn decide(&self, req: DecideRequest, caller: &str) -> DecideResponse {
        let started = Instant::now();

        let (enabled, contract_mode, shadow_log) = {
            let cfg = self.config.read().await;
            let d = &cfg.decision;
            let contract_mode = req
                .contract
                .as_ref()
                .and_then(|c| d.contracts.get(c).copied())
                .unwrap_or(d.default_mode);
            (d.enabled, contract_mode, d.shadow_log)
        };

        if !enabled {
            // Cero efectos: ni proveedor ni auditoría (no hay decisión).
            return DecideResponse::disabled();
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

        // Consulta LIBRE: sin proveedor de modelo → abstención explícita
        // (§1217); con proveedor → carril síncrono del modelo.
        if req.contract.is_none() {
            return self.decide_free(req, mode, shadow_log, started).await;
        }

        // Contrato nombrado → proveedor de reglas existente (autoridad).
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

        // Registro de la decisión de reglas (INSERT, primer tiempo).
        let row_ids = if shadow_log {
            self.insert_rule_rows(&req, &answers, mode, started).await
        } else {
            Vec::new()
        };

        // Sombra asíncrona (segundo tiempo): solo con proveedor de modelo,
        // modo != off y AL MENOS una pregunta con valor de reglas. La
        // abstención de reglas es estado final (diseño aprobado): no se
        // consulta al modelo para rescatarla.
        let shadow_slot = match (&self.model, mode != DecisionMode::Off, shadow_log) {
            (Some(slot), true, true) => Some(slot.clone()),
            _ => None,
        };
        let mut shadow_targets: Vec<ShadowTarget> = Vec::new();
        if shadow_slot.is_some() {
            for (name, row_id) in &row_ids {
                if let (Some(value), Some(spec)) = (
                    answers.get(name).and_then(|a| a.value.clone()),
                    req.questions.get(name),
                ) {
                    shadow_targets.push(ShadowTarget {
                        name: name.clone(),
                        spec: spec.clone(),
                        rules_value: value,
                        row_id: *row_id,
                    });
                }
            }
        }
        let consulted_identity = if shadow_targets.is_empty() {
            None
        } else {
            shadow_slot.as_ref().map(|m| m.identity.clone())
        };
        if let Some(slot) = shadow_slot {
            if !shadow_targets.is_empty() {
                let state = req.state.clone();
                self.spawn_shadow(slot, state, shadow_targets);
            }
        }

        DecideResponse {
            request_id: uuid::Uuid::new_v4().to_string(),
            status: if degraded { DecideStatus::Degraded } else { DecideStatus::Ok },
            mode,
            provider: self.provider_id().to_string(),
            model_identity: consulted_identity,
            degraded,
            answers,
        }
    }

    /// Carril síncrono de consultas libres con proveedor de modelo: deadline
    /// = min(`max_latency_ms` del wire ‖ 2000, `request_timeout_ms`). Fallo o
    /// timeout → abstención explícita con motivo; jamás una respuesta
    /// inventada.
    async fn decide_free(
        &self,
        req: DecideRequest,
        mode: DecisionMode,
        shadow_log: bool,
        started: Instant,
    ) -> DecideResponse {
        let Some(slot) = self.model.clone() else {
            // Fase 1 intacta: sin proveedor de modelo → abstención explícita.
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
                self.insert_rule_rows(&req, &response.answers, mode, started).await;
            }
            return response;
        };

        let wire_deadline = req.max_latency_ms.unwrap_or(DEFAULT_MAX_LATENCY_MS);
        let config_cap = slot.request_timeout.as_millis().min(u128::from(u32::MAX)) as u32;
        let deadline_ms = wire_deadline.min(config_cap);
        let core_req = DecisionRequest {
            state: req.state.clone(),
            questions: req
                .questions
                .iter()
                .map(|(n, s)| (n.clone(), to_core_question(s)))
                .collect(),
        };
        let call_started = Instant::now();
        let outcome = tokio::time::timeout(
            Duration::from_millis(u64::from(deadline_ms)),
            slot.provider.decide(&core_req),
        )
        .await;
        let model_latency_ms = call_started.elapsed().as_millis() as i64;

        let (answers, degraded) = match outcome {
            Ok(Ok(core_answers)) => {
                let mut degraded = false;
                let mut wire: BTreeMap<String, Answer> = BTreeMap::new();
                for (name, spec) in &req.questions {
                    match core_answers.get(name) {
                        Some(core) => {
                            let a = answer_from_core(core);
                            let out_of_set = a
                                .value
                                .as_deref()
                                .map(|v| !spec.allowed_values().contains(&v))
                                .unwrap_or(false);
                            if out_of_set {
                                degraded = true;
                                wire.insert(name.clone(), abstain(ABSTAIN_ANSWER_OUT_OF_OPTIONS));
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
            Ok(Err(e)) => {
                let reason = format!("provider_error: {e}");
                let wire = req
                    .questions
                    .keys()
                    .map(|name| (name.clone(), abstain(reason.clone())))
                    .collect();
                (wire, true)
            }
            Err(_) => {
                let wire = req
                    .questions
                    .keys()
                    .map(|name| (name.clone(), abstain(ABSTAIN_PROVIDER_TIMEOUT)))
                    .collect();
                (wire, true)
            }
        };

        let response = DecideResponse {
            request_id: uuid::Uuid::new_v4().to_string(),
            status: if degraded { DecideStatus::Degraded } else { DecideStatus::Ok },
            mode,
            provider: slot.provider.provider_id().to_string(),
            model_identity: Some(slot.identity.clone()),
            degraded,
            answers,
        };
        if shadow_log {
            self.insert_free_rows(
                &req,
                &response.answers,
                mode,
                started,
                slot.provider.provider_id(),
                &slot.identity,
                model_latency_ms,
            )
            .await;
        }
        response
    }

    /// INSERT (primer tiempo) de la decisión de REGLAS, una fila por pregunta.
    /// Devuelve `(pregunta, rowid)` para que la sombra asíncrona haga el
    /// UPDATE (segundo tiempo) de las columnas del modelo. Un fallo de
    /// escritura se avisa y no rompe la decisión.
    async fn insert_rule_rows(
        &self,
        req: &DecideRequest,
        answers: &BTreeMap<String, Answer>,
        mode: DecisionMode,
        started: Instant,
    ) -> Vec<(String, i64)> {
        let Some(silva) = &self.silva else {
            return Vec::new();
        };
        let ts = chrono::Utc::now().to_rfc3339();
        let latency_ms = started.elapsed().as_millis() as i64;
        let contract = req.contract.clone().unwrap_or_default();
        let g = silva.conn_lock();
        let conn = g.lock().await;
        let mut ids = Vec::with_capacity(answers.len());
        for (question, answer) in answers {
            let result = conn.execute(
                "INSERT INTO decision_shadow_log
                 (ts, contract, request_hash, rules_value, provider, latency_ms, mode)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    ts,
                    contract,
                    decision_row_hash(&req.state, question),
                    answer.value,
                    self.provider_id(),
                    latency_ms,
                    mode.as_str(),
                ],
            );
            match result {
                Ok(_) => ids.push((question.clone(), conn.last_insert_rowid())),
                Err(e) => tracing::warn!("decision_shadow_log write failed: {e}"),
            }
        }
        ids
    }

    /// INSERT (único tiempo) de una respuesta SÍNCRONA del MODELO a una
    /// consulta libre: `rules_value` NULL, columnas del modelo pobladas,
    /// `agreed` NULL (no hay regla con la que comparar).
    #[allow(clippy::too_many_arguments)]
    async fn insert_free_rows(
        &self,
        req: &DecideRequest,
        answers: &BTreeMap<String, Answer>,
        mode: DecisionMode,
        started: Instant,
        provider_id: &str,
        identity: &str,
        model_latency_ms: i64,
    ) {
        let Some(silva) = &self.silva else {
            return;
        };
        let ts = chrono::Utc::now().to_rfc3339();
        let latency_ms = started.elapsed().as_millis() as i64;
        let contract = req.contract.clone().unwrap_or_default();
        let g = silva.conn_lock();
        let conn = g.lock().await;
        for (question, answer) in answers {
            let probs_json = if answer.abstained {
                None
            } else {
                Some(serde_json::to_string(&answer.probabilities).unwrap_or_default())
            };
            let result = conn.execute(
                "INSERT INTO decision_shadow_log
                 (ts, contract, request_hash, rules_value, model_value, model_probs_json,
                  provider, latency_ms, agreed, mode, model_identity, model_latency_ms)
                 VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6, ?7, NULL, ?8, ?9, ?10)",
                rusqlite::params![
                    ts,
                    contract,
                    decision_row_hash(&req.state, question),
                    answer.value,
                    probs_json,
                    provider_id,
                    latency_ms,
                    mode.as_str(),
                    identity,
                    model_latency_ms,
                ],
            );
            if let Err(e) = result {
                tracing::warn!("decision_shadow_log write failed: {e}");
            }
        }
    }

    /// Sombra asíncrona (segundo tiempo): consulta al modelo con deadline
    /// `shadow_timeout` y UPDATE de las columnas del modelo por fila, juntos
    /// o ninguno. Timeout/error → las filas quedan NULL (que `paired_eval`
    /// interpreta como fallo conservador del modelo) y la causa va a tracing.
    fn spawn_shadow(&self, slot: ModelSlot, state: serde_json::Value, targets: Vec<ShadowTarget>) {
        let Some(silva) = self.silva.clone() else {
            return;
        };
        let handle = tokio::spawn(async move {
            let questions: std::collections::HashMap<String, DecisionQuestion> = targets
                .iter()
                .map(|t| (t.name.clone(), to_core_question(&t.spec)))
                .collect();
            let core_req = DecisionRequest { state, questions };
            let call_started = Instant::now();
            let outcome =
                tokio::time::timeout(slot.shadow_timeout, slot.provider.decide(&core_req)).await;
            let model_latency_ms = call_started.elapsed().as_millis() as i64;
            let answers = match outcome {
                Ok(Ok(a)) => a,
                Ok(Err(e)) => {
                    tracing::warn!("decision shadow: model provider error: {e}");
                    return;
                }
                Err(_) => {
                    tracing::warn!(
                        "decision shadow: model call timed out ({} ms) — rows stay NULL",
                        slot.shadow_timeout.as_millis()
                    );
                    return;
                }
            };
            let g = silva.conn_lock();
            let conn = g.lock().await;
            for t in &targets {
                let Some(core) = answers.get(&t.name) else {
                    continue;
                };
                let Some((value, probs_json)) = validated_argmax(core, &t.spec) else {
                    continue;
                };
                let agreed = (value.as_str() == t.rules_value.as_str()) as i64;
                let result = conn.execute(
                    "UPDATE decision_shadow_log
                     SET model_value = ?1, model_probs_json = ?2, agreed = ?3,
                         model_identity = ?4, model_latency_ms = ?5
                     WHERE id = ?6",
                    rusqlite::params![
                        value,
                        probs_json,
                        agreed,
                        slot.identity,
                        model_latency_ms,
                        t.row_id
                    ],
                );
                if let Err(e) = result {
                    tracing::warn!("decision shadow UPDATE failed: {e}");
                }
            }
        });
        if let Ok(mut hs) = self.shadow_tasks.lock() {
            hs.retain(|h| !h.is_finished());
            hs.push(handle);
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
                    provider, latency_ms, agreed, mode, model_identity, model_latency_ms
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
                model_identity: r.get(11)?,
                model_latency_ms: r.get(12)?,
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
        assert_eq!(
            d.provider,
            crate::config::DecisionProviderConfig::Name("rules".to_string())
        );
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
        assert!(r.model_value.is_none(), "NULL sin proveedor de modelo (reglas solas)");
        assert!(r.model_probs_json.is_none());
        assert!(r.model_identity.is_none());
        assert!(r.model_latency_ms.is_none());
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
