//! # Baseline A del spike (ADR-018 §3): proveedor determinista de reglas.
//!
//! Envuelve en el contrato `DecisionProvider` un subconjunto documentado
//! de las reglas vigentes de producción, con umbrales copiados de la
//! matriz real del Scheduler (`router/scheduler/decision.rs` y tipos) —
//! NO reemplaza esa matriz ni se cablea a nada: es la referencia contra
//! la que el spike mide Laya/LLM. Distribuciones one-hot (una regla es
//! determinista); `calibration: None` por diseño (una regla no tiene
//! calibración que declarar).
//!
//! Decisiones soportadas (el harness del spike alimenta `state` con los
//! campos documentados):
//! - `risk_tier` (Score: low/medium/high/critical) — umbral de
//!   `state.risk` (0-1) espejo de la documentación de RiskTier.
//! - `should_execute` (Noul) — true si `state.risk < 0.7` (política:
//!   riesgo crítico nunca auto-ejecuta; HITL decide).
//! - `latency_class` (Score: instant/interactive/standard/deferred) —
//!   umbrales espejo de `latency_budget_for` en
//!   `router/scheduler/decision.rs` (50/500/5000 ms).
//! - `provider_selection` (Choice) — por `state.risk` y
//!   `state.recent_failures` (0 = local_fast; fallos recientes > 0 =
//!   local_deep).

use super::*;

pub struct RulesDecisionProvider;

impl RulesDecisionProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RulesDecisionProvider {
    fn default() -> Self {
        Self::new()
    }
}

fn one_hot(key: &str) -> HashMap<String, f32> {
    HashMap::from([(key.to_string(), 1.0f32)])
}

#[async_trait::async_trait]
impl DecisionProvider for RulesDecisionProvider {
    async fn decide(
        &self,
        req: &DecisionRequest,
    ) -> Result<HashMap<String, DecisionAnswer>, DecisionError> {
        let mut out = HashMap::new();
        for (name, question) in &req.questions {
            let distribution = match question {
                DecisionQuestion::Choice { options } => {
                    let dist = match name.as_str() {
                        "provider_selection" => {
                            let failures = req
                                .state
                                .get("recent_failures")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            if failures > 0 {
                                one_hot("local_deep")
                            } else {
                                one_hot("local_fast")
                            }
                        }
                        "route" => {
                            // Espejo de la política de escalado: riesgo
                            // creciente sube el coste de la vía; crítico
                            // siempre va a revisión humana (HITL).
                            let risk = req
                                .state
                                .get("risk")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.0) as f32;
                            if risk >= 0.8 {
                                one_hot("human")
                            } else if risk >= 0.5 {
                                one_hot("guild")
                            } else if risk >= 0.2 {
                                one_hot("local_deep")
                            } else {
                                one_hot("local_fast")
                            }
                        }
                        _ => {
                            return Err(DecisionError::UnknownDecision(
                                name.clone(),
                                "provider_selection, route".into(),
                            ))
                        }
                    };
                    // Sanity: la regla solo puede elegir una opción válida.
                    if !options.iter().any(|o| dist.contains_key(o)) {
                        return Err(DecisionError::EmptyQuestion(name.clone()));
                    }
                    dist
                }
                DecisionQuestion::Score { levels } => {
                    let risk = req
                        .state
                        .get("risk")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0) as f32;
                    let level = match name.as_str() {
                        "risk_tier" => {
                            if risk >= 0.8 {
                                "critical"
                            } else if risk >= 0.5 {
                                "high"
                            } else if risk >= 0.2 {
                                "medium"
                            } else {
                                "low"
                            }
                        }
                        "latency_class" => {
                            let budget_ms = req
                                .state
                                .get("latency_budget_ms")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(5000);
                            if budget_ms <= 50 {
                                "instant"
                            } else if budget_ms <= 500 {
                                "interactive"
                            } else if budget_ms <= 5000 {
                                "standard"
                            } else {
                                "deferred"
                            }
                        }
                        _ => {
                            return Err(DecisionError::UnknownDecision(
                                name.clone(),
                                "risk_tier, latency_class".into(),
                            ))
                        }
                    };
                    if !levels.iter().any(|l| l == level) {
                        return Err(DecisionError::EmptyQuestion(name.clone()));
                    }
                    one_hot(level)
                }
                DecisionQuestion::Noul => {
                    if name != "should_execute" {
                        return Err(DecisionError::UnknownDecision(
                            name.clone(),
                            "should_execute".into(),
                        ));
                    }
                    let risk = req
                        .state
                        .get("risk")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0) as f32;
                    if risk < 0.7 {
                        one_hot("true")
                    } else {
                        one_hot("false")
                    }
                }
            };
            out.insert(
                name.clone(),
                DecisionAnswer {
                    distribution,
                    calibration: None,
                },
            );
        }
        Ok(out)
    }

    fn provider_id(&self) -> &'static str {
        "rules-v0"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::decision::rules::RulesDecisionProvider;

    fn req_with(state: serde_json::Value, questions: HashMap<String, DecisionQuestion>) -> DecisionRequest {
        DecisionRequest { state, questions }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn risk_tier_thresholds_mirror_documented_matrix() {
        let p = RulesDecisionProvider::new();
        for (risk, expected) in [
            (0.05, "low"),
            (0.25, "medium"),
            (0.55, "high"),
            (0.9, "critical"),
        ] {
            let mut q = HashMap::new();
            q.insert(
                "risk_tier".into(),
                DecisionQuestion::Score {
                    levels: vec!["low".into(), "medium".into(), "high".into(), "critical".into()],
                },
            );
            let res = p
                .decide(&req_with(serde_json::json!({ "risk": risk }), q))
                .await
                .unwrap();
            let dist = &res["risk_tier"].distribution;
            assert_eq!(dist[expected], 1.0, "risk {risk} -> {expected}, got {dist:?}");
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn should_execute_respects_hitl_floor() {
        let p = RulesDecisionProvider::new();
        let mut q = HashMap::new();
        q.insert("should_execute".into(), DecisionQuestion::Noul);
        let low = p
            .decide(&req_with(serde_json::json!({ "risk": 0.3 }), q.clone()))
            .await
            .unwrap();
        assert_eq!(low["should_execute"].distribution["true"], 1.0);
        let crit = p
            .decide(&req_with(serde_json::json!({ "risk": 0.9 }), q))
            .await
            .unwrap();
        assert_eq!(crit["should_execute"].distribution["false"], 1.0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn latency_class_thresholds_match_scheduler_budget_table() {
        let p = RulesDecisionProvider::new();
        for (ms, expected) in [(40u64, "instant"), (300, "interactive"), (2000, "standard"), (9000, "deferred")] {
            let mut q = HashMap::new();
            q.insert(
                "latency_class".into(),
                DecisionQuestion::Score {
                    levels: vec![
                        "instant".into(),
                        "interactive".into(),
                        "standard".into(),
                        "deferred".into(),
                    ],
                },
            );
            let res = p
                .decide(&req_with(serde_json::json!({ "latency_budget_ms": ms }), q))
                .await
                .unwrap();
            assert_eq!(
                res["latency_class"].distribution[expected], 1.0,
                "{ms}ms -> {expected}, got {:?}",
                res["latency_class"].distribution
            );
        }
    }
}
