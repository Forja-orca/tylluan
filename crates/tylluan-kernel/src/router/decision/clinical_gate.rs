//! T-DSH-121: ClinicalHtlGate — versión **fail-closed** del gate clínico.
//!
//! Decorador de `DecisionProvider` que se aplica SOLO a proveedores cloud
//! (los locales/loopback no se envuelven o se envuelven con `cloud = false`):
//! - Sensibilidad `Clinical` o **desconocida** ⇒ `Abstained("clinical_requires_hitl")`
//!   **sin abrir ninguna conexión** (fail-closed, ADR-020 E-4).
//! - Sensibilidad `Public`/`Private` conocida ⇒ delega en el proveedor.
//!
//! La cola HITL (aprobación por petición) es el paso siguiente; aquí solo se
//! bloquea, no se aprueba.

use std::collections::HashMap;
use std::sync::Arc;

use super::{DecisionAnswer, DecisionError, DecisionProvider, DecisionRequest};

/// Sensibilidad declarada en el estado de la petición.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sensitivity {
    Public,
    Private,
    Clinical,
}

/// Deriva la sensibilidad del `state` de la petición. Sin campo explícito ⇒
/// `None` (desconocida ⇒ fail-closed en cloud).
pub fn parse_sensitivity(raw: &str) -> Option<Sensitivity> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "public" | "publico" | "público" => Some(Sensitivity::Public),
        "private" | "privado" | "privada" => Some(Sensitivity::Private),
        "clinical" | "clinico" | "clínico" | "clinica" | "clínica" => Some(Sensitivity::Clinical),
        _ => None,
    }
}

/// T-DSH-122: sensibilidad de la petición con prioridad del campo del wire
/// (`req.sensitivity`) sobre la heurística de `state`.
pub fn sensitivity_from_request(req: &DecisionRequest) -> Option<Sensitivity> {
    if let Some(raw) = req.sensitivity.as_deref()
        && let Some(parsed) = parse_sensitivity(raw)
    {
        return Some(parsed);
    }
    sensitivity_from_state(&req.state)
}

pub fn sensitivity_from_state(state: &serde_json::Value) -> Option<Sensitivity> {
    let parse = parse_sensitivity;
    if let Some(s) = state.get("sensitivity").and_then(|v| v.as_str())
        && let Some(parsed) = parse(s)
    {
        return Some(parsed);
    }
    if let Some(s) = state
        .get("metadata")
        .and_then(|m| m.get("sensitivity"))
        .and_then(|v| v.as_str())
        && let Some(parsed) = parse(s)
    {
        return Some(parsed);
    }
    // Espacio clínico heredado de recall: metadata.space con "clinical".
    if let Some(space) = state
        .get("metadata")
        .and_then(|m| m.get("space"))
        .and_then(|v| v.as_str())
        && space.to_ascii_lowercase().contains("clinical")
    {
        return Some(Sensitivity::Clinical);
    }
    None
}

/// Decorador fail-closed para proveedores cloud.
pub struct ClinicalHtlGate {
    inner: Arc<dyn DecisionProvider>,
    cloud: bool,
}

impl ClinicalHtlGate {
    pub fn new(inner: Arc<dyn DecisionProvider>, cloud: bool) -> Self {
        Self { inner, cloud }
    }
}

#[async_trait::async_trait]
impl DecisionProvider for ClinicalHtlGate {
    async fn decide(
        &self,
        req: &DecisionRequest,
    ) -> Result<HashMap<String, DecisionAnswer>, DecisionError> {
        if self.cloud {
            match sensitivity_from_request(req) {
                Some(Sensitivity::Clinical) | None => {
                    tracing::warn!(
                        "decision: petición clínica/desconocida hacia proveedor cloud bloqueada (fail-closed, clinical_requires_hitl)"
                    );
                    return Err(DecisionError::Abstained(
                        "clinical_requires_hitl".to_string(),
                    ));
                }
                Some(Sensitivity::Public) | Some(Sensitivity::Private) => {}
            }
        }
        self.inner.decide(req).await
    }

    fn provider_id(&self) -> &'static str {
        self.inner.provider_id()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::decision::{CoreQuestion, DecisionQuestion};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingProvider {
        hits: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl DecisionProvider for CountingProvider {
        async fn decide(
            &self,
            _req: &DecisionRequest,
        ) -> Result<HashMap<String, DecisionAnswer>, DecisionError> {
            self.hits.fetch_add(1, Ordering::SeqCst);
            Ok(HashMap::new())
        }

        fn provider_id(&self) -> &'static str {
            "counting"
        }
    }

    fn request_with_sensitivity(sensitivity: Option<&str>) -> DecisionRequest {
        let mut questions = HashMap::new();
        questions.insert(
            "q".to_string(),
            CoreQuestion::without_instructions(DecisionQuestion::Noul),
        );
        let state = match sensitivity {
            Some(s) => serde_json::json!({"sensitivity": s}),
            None => serde_json::json!({"task": "sin sensibilidad"}),
        };
        DecisionRequest { state, questions, sensitivity: None }
    }

    fn gate(cloud: bool) -> (ClinicalHtlGate, Arc<AtomicUsize>) {
        let hits = Arc::new(AtomicUsize::new(0));
        let inner = Arc::new(CountingProvider { hits: hits.clone() });
        (ClinicalHtlGate::new(inner, cloud), hits)
    }

    #[tokio::test]
    async fn cloud_clinical_is_blocked_without_any_call() {
        let (g, hits) = gate(true);
        let err = g
            .decide(&request_with_sensitivity(Some("clinical")))
            .await
            .unwrap_err();
        assert!(matches!(err, DecisionError::Abstained(_)), "{err}");
        assert_eq!(hits.load(Ordering::SeqCst), 0, "no debe llamar al proveedor");
    }

    #[tokio::test]
    async fn cloud_public_and_private_delegate() {
        let (g, hits) = gate(true);
        g.decide(&request_with_sensitivity(Some("public"))).await.unwrap();
        g.decide(&request_with_sensitivity(Some("private"))).await.unwrap();
        assert_eq!(hits.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn local_clinical_delegates() {
        let (g, hits) = gate(false);
        g.decide(&request_with_sensitivity(Some("clinical"))).await.unwrap();
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn cloud_unknown_sensitivity_is_blocked_fail_closed() {
        let (g, hits) = gate(true);
        let err = g.decide(&request_with_sensitivity(None)).await.unwrap_err();
        assert!(matches!(err, DecisionError::Abstained(_)), "{err}");
        assert_eq!(hits.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn wire_sensitivity_overrides_state_heuristic() {
        // state dice public, pero el wire declara clinical => bloquea.
        let (g, hits) = gate(true);
        let mut req = request_with_sensitivity(Some("public"));
        req.sensitivity = Some("clinical".to_string());
        let err = g.decide(&req).await.unwrap_err();
        assert!(matches!(err, DecisionError::Abstained(_)), "{err}");
        assert_eq!(hits.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn wire_public_overrides_clinical_state() {
        // state dice clinical, pero el wire declara public => delega.
        let (g, hits) = gate(true);
        let mut req = request_with_sensitivity(Some("clinical"));
        req.sensitivity = Some("public".to_string());
        g.decide(&req).await.unwrap();
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn sensitivity_parsing() {
        assert_eq!(
            sensitivity_from_state(&serde_json::json!({"sensitivity": "Clinical"})),
            Some(Sensitivity::Clinical)
        );
        assert_eq!(
            sensitivity_from_state(&serde_json::json!({"metadata": {"sensitivity": "private"}})),
            Some(Sensitivity::Private)
        );
        assert_eq!(
            sensitivity_from_state(&serde_json::json!({"metadata": {"space": "clinical_notes"}})),
            Some(Sensitivity::Clinical)
        );
        assert_eq!(sensitivity_from_state(&serde_json::json!({"x": 1})), None);
    }
}
