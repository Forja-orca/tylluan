//! # Esquema canónico de `tylluan_decide` (ADR-021, Fase 1 del reparto) — DUEÑO DEL ESQUEMA
//!
//! ESTE fichero implementa el **esquema ÚNICO obligatorio** fijado por el TL
//! (Coloquio 1217/1218, ADR-021 aprobado por José 2026-10-16). No se cambia
//! sin pasar por el TL. La superficie MCP (`transport/server/tools.rs`) y el
//! endpoint REST (`POST /api/v1/decide`) comparten EXACTAMENTE estos tipos:
//! Antigravity copia este fichero byte a byte.
//!
//! Contrato (§1217):
//! - `DecideRequest{state, questions, contract, max_latency_ms}` —
//!   `questions: BTreeMap` 1..=16; `state` máx 16 KiB serializado;
//!   `max_latency_ms` por defecto 2000, máx 10000.
//! - `QuestionSpec{kind: QuestionKind{Choice,Score,Boolean} (snake_case),
//!   instructions (máx 1 KiB), options (Choice: 2..=32), levels (Score: 2..=10)}`;
//!   `Boolean` mapea al `Noul` interno.
//! - `DecideResponse{request_id, status{Ok,Disabled,Degraded,Error}, mode,
//!   provider, model_identity, degraded, answers: BTreeMap}`.
//! - `Answer{value, probabilities, confidence, abstained, abstain_reason}`.
//! - Una respuesta FUERA de las opciones declaradas se RECHAZA (nunca se
//!   reenvía); con el servicio apagado: `status=disabled`, `answers` vacío,
//!   CERO llamadas externas; preguntas libres sin proveedor de modelo →
//!   `abstained=true`, `abstain_reason="no_model_provider"`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{CoreQuestion, DecisionAnswer, DecisionQuestion};

/// Límites del contrato (§1217). Constantes del esquema, no configurables.
pub const MAX_QUESTIONS: usize = 16;
pub const MAX_STATE_BYTES: usize = 16 * 1024; // 16 KiB serializado
pub const MAX_INSTRUCTIONS_BYTES: usize = 1024; // 1 KiB por pregunta
pub const MIN_CHOICE_OPTIONS: usize = 2;
pub const MAX_CHOICE_OPTIONS: usize = 32;
pub const MIN_SCORE_LEVELS: usize = 2;
pub const MAX_SCORE_LEVELS: usize = 10;
pub const DEFAULT_MAX_LATENCY_MS: u32 = 2000;
pub const MAX_MAX_LATENCY_MS: u32 = 10_000;

/// Motivo de abstención cuando no hay proveedor de modelo (§1217).
pub const ABSTAIN_NO_MODEL_PROVIDER: &str = "no_model_provider";
/// Motivo de abstención cuando el proveedor devolvió un valor fuera de las
/// opciones declaradas (respuesta RECHAZADA, §1217).
pub const ABSTAIN_ANSWER_OUT_OF_OPTIONS: &str = "answer_out_of_options";
/// Motivo de abstención cuando la ENTRADA no cabe en el proveedor (422
/// STATE_TRUNCATED de Decima, o tope `state_max_chars`): motivo estable,
/// no es un fallo del proveedor ni abre el cortacircuitos (T1241).
pub const ABSTAIN_STATE_TOO_LONG: &str = "state_too_long";

/// Modo de un contrato interno (ADR-021 §3.2). `off` por defecto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DecisionMode {
    #[default]
    Off,
    Shadow,
    Advisory,
    Gate,
}

impl DecisionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            DecisionMode::Off => "off",
            DecisionMode::Shadow => "shadow",
            DecisionMode::Advisory => "advisory",
            DecisionMode::Gate => "gate",
        }
    }
}

/// Tipo de pregunta tipada (§1217), serde snake_case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionKind {
    Choice,
    Score,
    Boolean,
}

/// Pregunta: conjunto cerrado de salidas construido por el SISTEMA.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuestionSpec {
    pub kind: QuestionKind,
    /// Instrucciones opcionales de redacción (máx 1 KiB).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    /// Opciones para `Choice` (2..=32). Vacío en otros tipos.
    #[serde(default)]
    pub options: Vec<String>,
    /// Niveles ordenados para `Score` (2..=10). Vacío en otros tipos.
    #[serde(default)]
    pub levels: Vec<String>,
}

impl QuestionSpec {
    /// El conjunto cerrado de salidas válidas de esta pregunta.
    pub fn allowed_values(&self) -> Vec<&str> {
        match self.kind {
            QuestionKind::Choice => self.options.iter().map(String::as_str).collect(),
            QuestionKind::Score => self.levels.iter().map(String::as_str).collect(),
            QuestionKind::Boolean => vec!["true", "false"],
        }
    }
}

/// Petición (§1217): `state` (texto u objeto, máx 16 KiB serializado),
/// `questions` (1..=16), contrato interno opcional y presupuesto de latencia.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecideRequest {
    pub state: serde_json::Value,
    pub questions: BTreeMap<String, QuestionSpec>,
    /// Invoca un contrato interno nombrado (p. ej. `scheduler_routing`);
    /// ausente = consulta libre (la responde un proveedor de modelo, no las
    /// reglas — en Fase 1 sin modelo: abstención `no_model_provider`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract: Option<String>,
    /// Límite de latencia en ms (defecto 2000, máx 10000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_latency_ms: Option<u32>,
}

/// Estado de la respuesta (§1217).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DecideStatus {
    Ok,
    Disabled,
    Degraded,
    Error,
}

/// Respuesta de UNA pregunta.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Answer {
    /// Opción, nivel, o "true"/"false"; `null` si hay abstención.
    /// Los campos de respuesta se serializan SIEMPRE (con `null` si no hay
    /// valor): forma estable para los clientes.
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub probabilities: BTreeMap<String, f32>,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub abstained: bool,
    #[serde(default)]
    pub abstain_reason: Option<String>,
}

/// Respuesta completa (MCP y REST comparten esta forma EXACTA).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecideResponse {
    pub request_id: String,
    pub status: DecideStatus,
    /// Modo del contrato invocado (`off` si consulta libre).
    pub mode: DecisionMode,
    pub provider: String,
    #[serde(default)]
    pub model_identity: Option<String>,
    pub degraded: bool,
    #[serde(default)]
    pub answers: BTreeMap<String, Answer>,
}

impl DecideResponse {
    /// Servicio apagado: `status=disabled`, `answers` vacío, cero llamadas.
    pub fn disabled() -> Self {
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            status: DecideStatus::Disabled,
            mode: DecisionMode::default(),
            provider: "none".to_string(),
            model_identity: None,
            degraded: false,
            answers: BTreeMap::new(),
        }
    }

    /// Petición inválida según las validaciones del esquema (§1218: "errores
    /// claros" — el detalle va al log del kernel; el estado es `error`).
    pub fn error() -> Self {
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            status: DecideStatus::Error,
            mode: DecisionMode::default(),
            provider: "none".to_string(),
            model_identity: None,
            degraded: false,
            answers: BTreeMap::new(),
        }
    }
}

/// Valida la petición contra el contrato (§1217). `Err` = mensaje claro.
pub fn validate_request(req: &DecideRequest) -> Result<(), String> {
    if req.questions.is_empty() {
        return Err("questions must contain at least 1 question (0 given)".to_string());
    }
    if req.questions.len() > MAX_QUESTIONS {
        return Err(format!(
            "too many questions: {} > {MAX_QUESTIONS}",
            req.questions.len()
        ));
    }
    let state_bytes = serde_json::to_string(&req.state).map(|s| s.len()).unwrap_or(0);
    if state_bytes > MAX_STATE_BYTES {
        return Err(format!(
            "state too large: {state_bytes} bytes > {MAX_STATE_BYTES} (16 KiB)"
        ));
    }
    if let Some(ms) = req.max_latency_ms
        && ms > MAX_MAX_LATENCY_MS
    {
        return Err(format!(
            "max_latency_ms too large: {ms} > {MAX_MAX_LATENCY_MS}"
        ));
    }
    if let Some(c) = &req.contract
        && (c.trim().is_empty() || c.len() > 64)
    {
        return Err("contract name must be 1..=64 chars".to_string());
    }
    for (name, spec) in &req.questions {
        if name.trim().is_empty() {
            return Err("question names must not be empty".to_string());
        }
        if name.len() > 64 {
            return Err(format!("question name too long: '{name}'"));
        }
        if let Some(instr) = &spec.instructions
            && instr.len() > MAX_INSTRUCTIONS_BYTES
        {
            return Err(format!(
                "question '{name}': instructions too large ({} > {MAX_INSTRUCTIONS_BYTES} bytes)",
                instr.len()
            ));
        }
        match spec.kind {
            QuestionKind::Choice => {
                if !(MIN_CHOICE_OPTIONS..=MAX_CHOICE_OPTIONS).contains(&spec.options.len()) {
                    return Err(format!(
                        "question '{name}': choice needs {MIN_CHOICE_OPTIONS}..={MAX_CHOICE_OPTIONS} options ({} given)",
                        spec.options.len()
                    ));
                }
            }
            QuestionKind::Score => {
                if !(MIN_SCORE_LEVELS..=MAX_SCORE_LEVELS).contains(&spec.levels.len()) {
                    return Err(format!(
                        "question '{name}': score needs {MIN_SCORE_LEVELS}..={MAX_SCORE_LEVELS} levels ({} given)",
                        spec.levels.len()
                    ));
                }
            }
            QuestionKind::Boolean => {}
        }
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for v in spec.allowed_values() {
            if v.trim().is_empty() {
                return Err(format!("question '{name}': empty option/level"));
            }
            if !seen.insert(v) {
                return Err(format!("question '{name}': duplicated option '{v}'"));
            }
        }
    }
    Ok(())
}

/// Traduce una pregunta al tipo interno del Decision Fabric (`Boolean`→`Noul`).
/// T1252: el canal `instructions` del wire viaja SIN interpretar hasta el
/// proveedor — aquí no se valida ni se sustituye (la desconfianza vive en
/// la frontera del proveedor y el tope ya lo puso `validate_request`).
pub fn to_core_question(spec: &QuestionSpec) -> CoreQuestion {
    let kind = match spec.kind {
        QuestionKind::Choice => DecisionQuestion::Choice {
            options: spec.options.clone(),
        },
        QuestionKind::Score => DecisionQuestion::Score {
            levels: spec.levels.clone(),
        },
        QuestionKind::Boolean => DecisionQuestion::Noul,
    };
    CoreQuestion::new(kind, spec.instructions.clone())
}

/// Respuesta del proveedor → esquema wire (argmax como `value`).
pub fn answer_from_core(core: &DecisionAnswer) -> Answer {
    let value = core
        .distribution
        .iter()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(k, _)| k.clone());
    let confidence = core.distribution.values().copied().reduce(f32::max);
    Answer {
        value,
        probabilities: core.distribution.iter().map(|(k, v)| (k.clone(), *v)).collect(),
        confidence,
        abstained: false,
        abstain_reason: None,
    }
}

/// Abstención explícita con motivo (§1217).
pub fn abstain(reason: impl Into<String>) -> Answer {
    Answer {
        value: None,
        probabilities: BTreeMap::new(),
        confidence: None,
        abstained: true,
        abstain_reason: Some(reason.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice_spec() -> QuestionSpec {
        QuestionSpec {
            kind: QuestionKind::Choice,
            instructions: None,
            options: vec!["local_fast".into(), "guild".into(), "human".into()],
            levels: vec![],
        }
    }

    fn req(q: BTreeMap<String, QuestionSpec>) -> DecideRequest {
        DecideRequest {
            state: serde_json::json!({"risk": 0.9}),
            questions: q,
            contract: None,
            max_latency_ms: None,
        }
    }

    /// El esquema wire queda congelado con estos nombres de campo: MCP, REST
    /// y dashboard dependen de ellos (§1217: mismo contrato exacto).
    #[test]
    fn wire_schema_field_names_are_frozen() {
        let r = DecideRequest {
            state: serde_json::json!({"risk": 0.9}),
            questions: BTreeMap::from([("route".to_string(), choice_spec())]),
            contract: Some("scheduler_routing".into()),
            max_latency_ms: Some(1500),
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["questions"]["route"]["kind"], "choice");
        assert_eq!(v["questions"]["route"]["options"][2], "human");
        assert_eq!(v["contract"], "scheduler_routing");
        assert_eq!(v["max_latency_ms"], 1500);

        let back: DecideRequest = serde_json::from_value(v).unwrap();
        assert_eq!(back.contract.as_deref(), Some("scheduler_routing"));

        // Response: request_id + 4 estados + mode + answers BTreeMap
        let d = DecideResponse::disabled();
        let dv = serde_json::to_value(&d).unwrap();
        assert!(dv["request_id"].as_str().unwrap_or("").len() >= 8);
        assert_eq!(dv["status"], "disabled");
        assert_eq!(dv["provider"], "none");
        assert!(dv["answers"].as_object().unwrap().is_empty());
        for (status, name) in [
            (DecideStatus::Ok, "ok"),
            (DecideStatus::Disabled, "disabled"),
            (DecideStatus::Degraded, "degraded"),
            (DecideStatus::Error, "error"),
        ] {
            assert_eq!(serde_json::to_value(status).unwrap(), serde_json::json!(name));
        }
        for (mode, name) in [
            (DecisionMode::Off, "off"),
            (DecisionMode::Shadow, "shadow"),
            (DecisionMode::Advisory, "advisory"),
            (DecisionMode::Gate, "gate"),
        ] {
            assert_eq!(serde_json::to_value(mode).unwrap(), serde_json::json!(name));
        }
    }

    /// Límites exactos de §1218: 0 preguntas, 17 preguntas, Choice con 1
    /// opción, Score con 11 niveles, state > 16 KiB, max_latency_ms > 10000.
    #[test]
    fn schema_limits_reject_the_specified_cases() {
        // 0 preguntas
        assert!(validate_request(&req(BTreeMap::new())).unwrap_err().contains("at least 1"));

        // 17 preguntas
        let mut q17 = BTreeMap::new();
        for i in 0..17 {
            q17.insert(format!("q{i}"), choice_spec());
        }
        assert!(validate_request(&req(q17)).unwrap_err().contains("too many"));

        // Choice con 1 opción
        let one = QuestionSpec { kind: QuestionKind::Choice, instructions: None, options: vec!["x".into()], levels: vec![] };
        assert!(validate_request(&req(BTreeMap::from([("a".into(), one)]))).unwrap_err().contains("2..=32 options"));

        // Score con 11 niveles
        let mut qs = BTreeMap::new();
        qs.insert("a".to_string(), QuestionSpec {
            kind: QuestionKind::Score,
            instructions: None,
            options: vec![],
            levels: (0..11).map(|i| format!("l{i}")).collect(),
        });
        assert!(validate_request(&req(qs)).unwrap_err().contains("2..=10 levels"));

        // state > 16 KiB
        let big = DecideRequest {
            state: serde_json::json!({"blob": "x".repeat(17 * 1024)}),
            questions: BTreeMap::from([("a".to_string(), choice_spec())]),
            contract: None,
            max_latency_ms: None,
        };
        assert!(validate_request(&big).unwrap_err().contains("too large"));

        // max_latency_ms > 10000
        let slow = DecideRequest { max_latency_ms: Some(10001), ..req(BTreeMap::from([("a".to_string(), choice_spec())])) };
        assert!(validate_request(&slow).unwrap_err().contains("max_latency_ms"));

        // 16 preguntas y latency 10000 exactos: pasan
        let mut ok16 = BTreeMap::new();
        for i in 0..16 {
            ok16.insert(format!("q{i}"), choice_spec());
        }
        let boundary = DecideRequest { max_latency_ms: Some(10_000), ..req(ok16) };
        assert!(validate_request(&boundary).is_ok());
    }

    #[test]
    fn core_conversions_argmax_and_abstain() {
        let core = DecisionAnswer {
            distribution: std::collections::HashMap::from([
                ("local_fast".to_string(), 0.1f32),
                ("guild".to_string(), 0.7f32),
                ("human".to_string(), 0.2f32),
            ]),
            calibration: None,
        };
        let a = answer_from_core(&core);
        assert_eq!(a.value.as_deref(), Some("guild"));
        assert!((a.confidence.unwrap() - 0.7).abs() < 1e-6);
        assert_eq!(a.probabilities.len(), 3);

        let ab = abstain(ABSTAIN_NO_MODEL_PROVIDER);
        assert!(ab.abstained);
        assert_eq!(ab.abstain_reason.as_deref(), Some("no_model_provider"));

        // Boolean ↔ Noul
        let q = to_core_question(&QuestionSpec { kind: QuestionKind::Boolean, instructions: None, options: vec![], levels: vec![] });
        assert!(matches!(q.kind, DecisionQuestion::Noul));
        assert!(q.instructions.is_none(), "sin instructions = canal cerrado");
    }

    /// T1252: el canal reabierto — las instructions del wire llegan crudas
    /// al núcleo (el proveedor decide confianza/límite), y su ausencia
    /// mantiene el canal cerrado tal cual estaba.
    #[test]
    fn to_core_question_carries_wire_instructions_verbatim() {
        let text = "Pick billing unless the cache layer is implicated.";
        let q = to_core_question(&QuestionSpec {
            kind: QuestionKind::Choice,
            instructions: Some(text.to_string()),
            options: vec!["billing".into(), "technical".into()],
            levels: vec![],
        });
        assert_eq!(q.instructions.as_deref(), Some(text));

        let none = to_core_question(&QuestionSpec {
            kind: QuestionKind::Choice,
            instructions: None,
            options: vec!["billing".into(), "technical".into()],
            levels: vec![],
        });
        assert!(none.instructions.is_none());
    }
}
