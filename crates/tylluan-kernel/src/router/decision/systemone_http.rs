//! ADR-020 Fase 2 (diseño aprobado 2026-10-16): cliente HTTP aislado del
//! dialecto System One (`POST /v1/systemone`) — shim sustituible por el
//! Inference Gateway cuando aterrice su Fase 1; el mapeo y la semántica del
//! registro no cambian al hacer el traslado.
//!
//! Decisiones del diseño aprobado materializadas aquí:
//! - **Solo loopback**: [`SystemOneHttpProvider::from_config`] rechaza
//!   endpoints no-loopback (dato clínico nunca sale de la máquina; la puerta
//!   LAN llegará con `[[inference.allowed_lan_endpoints]]` de ADR-020).
//! - **Agnóstico del runtime**: un solo cliente cubre `llama-server`
//!   (llama.cpp PR #29818), `decima serve` (:11436), Ollama >= 0.35 y LiteLLM.
//! - **Mapeo portado del spike de Fase 0** (`examples/decision_fabric_spike.rs`,
//!   probado contra servidor real con clef-flash) con sus lecciones T1004:
//!   `instructions` obligatorio (HTTP 400 sin él), una pregunta inválida no
//!   tumba el lote, respuesta parcial no invalida las demás preguntas.
//! - **Sin dependencias nuevas**: `reqwest` ya está en el árbol.
//! - **Presupuesto**: semáforo `max_inflight` (realidad single-slot) y
//!   cortacircuitos por fallos consecutivos. El deadline por carril lo pone
//!   el servicio (síncrono: deadline del wire; sombra: `shadow_timeout`).
//!   Nota honesta: los timeouts bajo el deadline del LLAMANTE no llegan a
//!   contar para el cortacircuitos (la future se descarta antes); solo
//!   cuentan los fallos que el proveedor observa dentro de su propio bound.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

use crate::config::SystemoneHttpConfig;

use super::api_types::ABSTAIN_STATE_TOO_LONG;
use super::{DecisionAnswer, DecisionError, DecisionProvider, DecisionQuestion, DecisionRequest};

pub const PROVIDER_ID: &str = "systemone-http-v1";

/// ¿El endpoint es loopback? Solo esos se aceptan en esta fase.
pub fn is_loopback_endpoint(endpoint: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(endpoint) else {
        return false;
    };
    match url.host_str() {
        Some(h) => {
            let host = h.trim_start_matches('[').trim_end_matches(']');
            host.eq_ignore_ascii_case("localhost")
                || host
                    .parse::<std::net::IpAddr>()
                    .map(|ip| ip.is_loopback())
                    .unwrap_or(false)
        }
        None => false,
    }
}

fn now_epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub struct SystemOneHttpProvider {
    endpoint: String,
    model: String,
    http: reqwest::Client,
    semaphore: tokio::sync::Semaphore,
    breaker_failures: u32,
    breaker_cooldown_s: u64,
    /// Tope de la representación JSON de `state` (caracteres); 0 = sin
    /// límite (T1241).
    state_max_chars: usize,
    consecutive_failures: AtomicU32,
    /// Epoch secs hasta el que el cortacircuitos está abierto (0 = cerrado).
    breaker_open_until: AtomicU64,
}

impl SystemOneHttpProvider {
    /// Construye el cliente desde `[decision.provider]` (`kind = "systemone_http"`).
    /// `Err` con motivo claro para que el servicio caiga a reglas (fail-closed).
    pub fn from_config(cfg: &SystemoneHttpConfig) -> Result<Self, String> {
        if !is_loopback_endpoint(&cfg.endpoint) {
            return Err(format!(
                "endpoint '{}' is not loopback — phase 2 only allows 127.0.0.1/localhost/::1 \
                 (clinical data never leaves the machine)",
                cfg.endpoint
            ));
        }
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(1))
            .timeout(Duration::from_millis(cfg.shadow_timeout_ms.max(1000)))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| format!("reqwest client: {e}"))?;
        Ok(Self {
            endpoint: cfg.endpoint.trim_end_matches('/').to_string(),
            model: cfg.model.clone(),
            http,
            semaphore: tokio::sync::Semaphore::new(cfg.max_inflight.max(1)),
            breaker_failures: cfg.breaker_failures.max(1),
            breaker_cooldown_s: cfg.breaker_cooldown_s,
            state_max_chars: cfg.state_max_chars,
            consecutive_failures: AtomicU32::new(0),
            breaker_open_until: AtomicU64::new(0),
        })
    }

    fn breaker_open(&self) -> bool {
        let until = self.breaker_open_until.load(Ordering::Relaxed);
        until > 0 && now_epoch_secs() < until
    }

    fn note_failure(&self) {
        if self.breaker_open() {
            return;
        }
        let fails = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
        if fails >= self.breaker_failures {
            let until = now_epoch_secs() + self.breaker_cooldown_s;
            self.breaker_open_until.store(until, Ordering::Relaxed);
            self.consecutive_failures.store(0, Ordering::Relaxed);
            tracing::warn!(
                "decision: systemone breaker OPEN for {}s after {fails} consecutive failures",
                self.breaker_cooldown_s
            );
        }
    }

    fn note_success(&self) {
        if self.breaker_open_until.swap(0, Ordering::Relaxed) != 0 {
            tracing::info!("decision: systemone breaker CLOSED (success)");
        }
        self.consecutive_failures.store(0, Ordering::Relaxed);
    }
}

#[derive(Debug, serde::Deserialize)]
struct SystemOneAnswer {
    #[serde(default)]
    probabilities: HashMap<String, f64>,
    #[serde(default)]
    noul: Option<f64>,
}

#[derive(Debug, serde::Deserialize)]
struct SystemOneResponse {
    answers: HashMap<String, SystemOneAnswer>,
}

#[async_trait::async_trait]
impl DecisionProvider for SystemOneHttpProvider {
    async fn decide(
        &self,
        req: &DecisionRequest,
    ) -> Result<HashMap<String, DecisionAnswer>, DecisionError> {
        if self.breaker_open() {
            return Err(DecisionError::Provider(
                PROVIDER_ID.into(),
                "circuit breaker open".into(),
            ));
        }
        // Tope del estado (T1241): si se sabe que no cabe en el modelo, ni
        // siquiera se envía — abstención estable sin coste de red.
        if self.state_max_chars > 0
            && req.state.to_string().chars().count() > self.state_max_chars
        {
            return Err(DecisionError::Abstained(ABSTAIN_STATE_TOO_LONG.to_string()));
        }
        let _permit = self.semaphore.acquire().await.map_err(|e| {
            DecisionError::Provider(PROVIDER_ID.into(), format!("semaphore closed: {e}"))
        })?;

        // Mapeo del lote (T1004): una pregunta degenerada se omite, no tumba
        // a las demás del mismo lote.
        let mut questions = serde_json::Map::new();
        for (name, q) in &req.questions {
            match question_spec(name, q) {
                Ok(spec) => {
                    questions.insert(name.clone(), spec);
                }
                Err(_) => tracing::debug!(
                    "decision: systemone question '{name}' has no valid options/levels — skipped from the batch"
                ),
            }
        }
        if questions.is_empty() {
            return Err(DecisionError::EmptyQuestion("all questions in item".to_string()));
        }

        let body = serde_json::json!({
            "model": self.model,
            "state": req.state,
            "questions": questions,
        });
        let url = format!("{}/v1/systemone", self.endpoint);
        let resp = match self.http.post(&url).json(&body).send().await {
            Ok(r) => r,
            Err(e) => {
                self.note_failure();
                return Err(DecisionError::Provider(
                    PROVIDER_ID.into(),
                    format!("{url}: {e}"),
                ));
            }
        };
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            // 422 STATE_TRUNCATED (Decima, hallazgo en vivo 2026-10-17): la
            // ENTRADA no cabe — abstención con motivo estable, sin penalizar
            // al proveedor ni al resto de llamantes.
            if status == reqwest::StatusCode::UNPROCESSABLE_ENTITY
                && text.to_ascii_lowercase().contains("state_truncated")
            {
                return Err(DecisionError::Abstained(ABSTAIN_STATE_TOO_LONG.to_string()));
            }
            // 4xx salvo 429: causa en la ENTRADA, no en el proveedor — no
            // cuenta para el cortacircuitos (un llamante inválido no puede
            // tumbar el carril del modelo para todos).
            if status.is_client_error() && status != reqwest::StatusCode::TOO_MANY_REQUESTS {
                return Err(DecisionError::Provider(
                    PROVIDER_ID.into(),
                    format!("{url}: HTTP {status} — {}", clip_str(&text, 300)),
                ));
            }
            self.note_failure();
            return Err(DecisionError::Provider(
                PROVIDER_ID.into(),
                format!("{url}: HTTP {status} — {}", clip_str(&text, 300)),
            ));
        }
        let parsed: SystemOneResponse = match resp.json().await {
            Ok(p) => p,
            Err(e) => {
                self.note_failure();
                return Err(DecisionError::Provider(
                    PROVIDER_ID.into(),
                    format!("decode: {e}"),
                ));
            }
        };

        // Respuesta parcial (T1004): el servidor puede omitir preguntas sin
        // invalidar las que sí respondió; solo falla el lote si no hay
        // ninguna utilizable.
        let mut out = HashMap::new();
        for (name, q) in &req.questions {
            let Some(ans) = parsed.answers.get(name) else {
                tracing::debug!(
                    "decision: systemone answered {} of {} questions; '{name}' missing — skipped",
                    parsed.answers.len(),
                    req.questions.len()
                );
                continue;
            };
            let distribution = distribution_for(q, ans);
            if distribution.is_empty() {
                tracing::debug!("decision: systemone '{name}' answered with no probabilities — skipped");
                continue;
            }
            out.insert(
                name.clone(),
                DecisionAnswer {
                    distribution,
                    calibration: None,
                },
            );
        }
        if out.is_empty() {
            self.note_failure();
            return Err(DecisionError::Provider(
                PROVIDER_ID.into(),
                "server answered none of the requested questions usably".to_string(),
            ));
        }
        self.note_success();
        Ok(out)
    }

    fn provider_id(&self) -> &'static str {
        PROVIDER_ID
    }
}

/// Mapeo puro `DecisionQuestion` → spec System One (idéntico al del spike
/// probado contra servidor real). `Noul` viaja como choice de dos opciones
/// para que la distribución por opción sea siempre la única fuente de
/// verdad (el campo `noul` tipado de la respuesta se prefiere si viene).
fn question_spec(name: &str, q: &DecisionQuestion) -> Result<serde_json::Value, DecisionError> {
    let instructions = format!("Answer the {name} question about this state.");
    Ok(match q {
        DecisionQuestion::Choice { options } => {
            if options.len() < 2 {
                return Err(DecisionError::EmptyQuestion(name.to_string()));
            }
            serde_json::json!({
                "type": "choice",
                "instructions": instructions,
                "criteria": options.iter()
                    .map(|o| (o.clone(), serde_json::Value::Null))
                    .collect::<serde_json::Map<String, serde_json::Value>>(),
            })
        }
        DecisionQuestion::Score { levels } => {
            if levels.len() < 2 {
                return Err(DecisionError::EmptyQuestion(name.to_string()));
            }
            serde_json::json!({ "type": "score", "instructions": instructions, "criteria": levels })
        }
        DecisionQuestion::Noul => serde_json::json!({
            "type": "choice",
            "instructions": instructions,
            "criteria": {
                "true": "the statement holds",
                "false": "the statement does not hold",
            },
        }),
    })
}

/// Mapeo puro respuesta System One → distribución (clamp `[0,1]`; para `Noul`
/// el campo tipado `noul` gana cuando está presente).
fn distribution_for(q: &DecisionQuestion, ans: &SystemOneAnswer) -> HashMap<String, f32> {
    if matches!(q, DecisionQuestion::Noul)
        && let Some(p) = ans.noul
    {
        let p = p.clamp(0.0, 1.0) as f32;
        return [("true".to_string(), p), ("false".to_string(), 1.0 - p)]
            .into_iter()
            .collect();
    }
    ans.probabilities
        .iter()
        .map(|(k, v)| (k.clone(), v.clamp(0.0, 1.0) as f32))
        .collect()
}

fn clip_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// El mapeo debe producir un spec System One válido para cada variante —
    /// es lo que hace al proveedor agnóstico del runtime (llama.cpp #29818 y
    /// Ollama >= 0.35 aceptan lo mismo).
    #[test]
    fn question_spec_covers_all_variants() {
        let choice = question_spec(
            "route",
            &DecisionQuestion::Choice {
                options: vec!["billing".into(), "technical".into()],
            },
        )
        .unwrap();
        assert_eq!(choice["type"], "choice");
        assert_eq!(choice["instructions"], "Answer the route question about this state.");
        assert_eq!(choice["criteria"]["billing"], serde_json::Value::Null);

        let score = question_spec(
            "urgency",
            &DecisionQuestion::Score {
                levels: vec!["low".into(), "high".into()],
            },
        )
        .unwrap();
        assert_eq!(score["type"], "score");
        assert_eq!(score["criteria"].as_array().unwrap().len(), 2);

        let noul = question_spec("should_i", &DecisionQuestion::Noul).unwrap();
        assert_eq!(noul["type"], "choice");
        assert!(noul["criteria"].get("true").is_some());
        assert!(noul["criteria"].get("false").is_some());
    }

    #[test]
    fn degenerate_questions_are_rejected_before_the_wire() {
        let one_opt = DecisionQuestion::Choice {
            options: vec!["only".into()],
        };
        assert!(matches!(
            question_spec("q", &one_opt),
            Err(DecisionError::EmptyQuestion(..))
        ));
        let one_level = DecisionQuestion::Score {
            levels: vec!["only".into()],
        };
        assert!(matches!(
            question_spec("q", &one_level),
            Err(DecisionError::EmptyQuestion(..))
        ));
    }

    #[test]
    fn noul_answer_prefers_the_typed_field() {
        let ans = SystemOneAnswer {
            probabilities: HashMap::new(),
            noul: Some(0.996),
        };
        let dist = distribution_for(&DecisionQuestion::Noul, &ans);
        assert!((dist["true"] - 0.996).abs() < 1e-6);
        assert!((dist["false"] - 0.004).abs() < 1e-6);
    }

    #[test]
    fn probabilities_pass_through_clamped() {
        let ans = SystemOneAnswer {
            probabilities: HashMap::from([
                ("true".to_string(), 1.5f64),
                ("false".to_string(), -0.2f64),
            ]),
            noul: None,
        };
        let dist = distribution_for(&DecisionQuestion::Noul, &ans);
        assert_eq!(dist["true"], 1.0);
        assert_eq!(dist["false"], 0.0);

        let ans = SystemOneAnswer {
            probabilities: HashMap::from([
                ("billing".to_string(), 0.7f64),
                ("technical".to_string(), 0.3f64),
            ]),
            noul: None,
        };
        let dist = distribution_for(
            &DecisionQuestion::Choice {
                options: vec!["billing".into(), "technical".into()],
            },
            &ans,
        );
        assert!((dist["billing"] - 0.7f32).abs() < 1e-6);
        assert!((dist["technical"] - 0.3f32).abs() < 1e-6);
    }

    /// Solo loopback (diseño aprobado): LAN/Internet → rechazados; loopback
    /// en todas sus formas → aceptados.
    #[test]
    fn loopback_validation_rejects_everything_else() {
        assert!(is_loopback_endpoint("http://127.0.0.1:8080"));
        assert!(is_loopback_endpoint("http://127.0.0.1:11436"));
        assert!(is_loopback_endpoint("http://localhost:9000"));
        assert!(is_loopback_endpoint("http://[::1]:8080"));
        assert!(is_loopback_endpoint("http://127.8.8.8:8080"));

        assert!(!is_loopback_endpoint("http://192.168.1.10:8080"));
        assert!(!is_loopback_endpoint("http://10.0.0.5:11434"));
        assert!(!is_loopback_endpoint("http://172.16.0.9:8080"));
        assert!(!is_loopback_endpoint("https://api.openai.com"));
        assert!(!is_loopback_endpoint("http://169.254.169.254"));
        assert!(!is_loopback_endpoint("not a url"));
        assert!(!is_loopback_endpoint(""));
    }
}
