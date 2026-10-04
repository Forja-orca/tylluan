//! # Decision Fabric — contrato de decisión tipada (ADR-018)
//!
//! Interfaz unificada para decisiones probabilísticas pequeñas y baratas
//! (`Choice` / `Score` / `Noul`) que los subsistemas (Scheduler,
//! CoherenceGate, router) pueden consumir como DATO, nunca como
//! conversación. Fase 0 (spike de medición): este módulo define el
//! contrato `DecisionProvider` y el baseline determinista de reglas; NO
//! se cablea a ningún camino de producción en esta fase.
//!
//! Invariantes del ADR-018:
//! 1. El modelo nunca tiene autoridad directa — `DecisionAnswer` es un
//!    insumo de la política existente, nunca ejecuta una acción.
//! 2. Shadow-mode obligatorio antes de cualquier cutover.
//! 3. Cero cambio al camino caliente en Fase 0.

use std::collections::HashMap;

/// Una pregunta tipada sobre el estado del sistema.
/// Esquema de Laya/System One: las salidas posibles están definidas de
/// antemano — el modelo no genera texto ni puede inventar opciones.
#[derive(Debug, Clone, PartialEq)]
pub enum DecisionQuestion {
    /// Elegir UNA opción entre las dadas (distribución sobre `options`).
    Choice { options: Vec<String> },
    /// Puntuar en niveles ordenados (distribución sobre `levels`).
    Score { levels: Vec<String> },
    /// Binario con incertidumbre explícita (distribución sobre
    /// "true"/"false").
    Noul,
}

/// Estado observable arbitrario (task, agent, memory, risk, budget...)
/// más el conjunto de preguntas a responder sobre él.
#[derive(Debug, Clone)]
pub struct DecisionRequest {
    pub state: serde_json::Value,
    pub questions: HashMap<String, DecisionQuestion>,
}

/// Distribución de probabilidad sobre las opciones/niveles de UNA
/// pregunta, junto con la calibración del proveedor que respondió
/// (si la conoce).
#[derive(Debug, Clone)]
pub struct DecisionAnswer {
    /// opción/nivel -> probabilidad. Para `Noul`: claves "true"/"false".
    pub distribution: HashMap<String, f32>,
    /// ECE/Brier del proveedor, si lo publica (el baseline de reglas no).
    pub calibration: Option<CalibrationMeta>,
}

/// Calibración auto-declarada por el proveedor (medida en vivo en
/// shadow-mode, no offline) — Fase 0 permite None.
#[derive(Debug, Clone, Copy)]
pub struct CalibrationMeta {
    pub ece: f32,
    pub brier: f32,
}

#[derive(Debug, thiserror::Error)]
pub enum DecisionError {
    #[error("unknown decision '{0}' (available: {1})")]
    UnknownDecision(String, String),
    #[error("question '{0}' has no valid options/levels")]
    EmptyQuestion(String),
    #[error("provider '{0}' failed: {1}")]
    Provider(String, String),
}

/// Contrato del Decision Fabric (ADR-018 §2.1): nunca `LayaProvider` —
/// el kernel solo ve esta interfaz, y cada implementación se sustituye
/// sin tocar ningún call-site.
#[async_trait::async_trait]
pub trait DecisionProvider: Send + Sync {
    async fn decide(
        &self,
        req: &DecisionRequest,
    ) -> Result<HashMap<String, DecisionAnswer>, DecisionError>;

    fn provider_id(&self) -> &'static str;
}

/// Utilidad compartida: valida que una distribución sobre las claves
/// esperadas sume ~1 (tolerancia 1e-3 para errores de coma flotante).
/// Solo la usan los tests del contrato en Fase 0 — el validador de
/// producción vendrá con el shadow-mode (ADR-018 §2.2.2).
#[cfg(test)]
pub(crate) fn distribution_sums_to_one(dist: &HashMap<String, f32>) -> bool {
    let sum: f32 = dist.values().sum();
    (sum - 1.0).abs() < 1e-3
}

pub mod rules;

#[cfg(test)]
mod tests {
    use super::*;

    fn choice_req() -> DecisionRequest {
        let mut questions = HashMap::new();
        questions.insert(
            "route".to_string(),
            DecisionQuestion::Choice {
                options: vec![
                    "local_fast".into(),
                    "local_deep".into(),
                    "guild".into(),
                    "human".into(),
                ],
            },
        );
        DecisionRequest {
            state: serde_json::json!({ "risk": 0.2 }),
            questions,
        }
    }

    /// El contrato exige que todo proveedor devuelva UNA respuesta por
    /// pregunta, con distribución sobre exactamente sus opciones.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn rules_provider_answers_only_known_questions() {
        let provider = rules::RulesDecisionProvider::new();
        let res = provider.decide(&choice_req()).await.unwrap();
        assert_eq!(res.len(), 1);
        let ans = res.get("route").expect("route answered");
        assert!(
            distribution_sums_to_one(&ans.distribution),
            "distribution must sum to 1: {:?}",
            ans.distribution
        );
        assert!(ans.calibration.is_none(), "rules baseline declares no calibration");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn unknown_question_is_an_explicit_error() {
        let provider = rules::RulesDecisionProvider::new();
        let mut req = choice_req();
        req.questions
            .insert("nope".to_string(), DecisionQuestion::Noul);
        let err = provider.decide(&req).await.unwrap_err();
        assert!(matches!(err, DecisionError::UnknownDecision(..)));
    }
}
