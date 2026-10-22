//! D-14/C4: telemetria del juez (CoherenceGate Layer 4 + puerta de escritura ASI06).
//!
//! Por que existe: B-4 (2026-10-20) no pudo atribuir un solo fallo del juez porque
//! los errores se registraban con `tracing::debug!` y no habia contadores. Un juez
//! que falla en silencio es indistinguible de un juez que dice "limpio".
//!
//! Contadores **en memoria**, sin persistencia, sin texto de contenido: nunca se
//! guarda el prompt ni la respuesta, solo cifras. Se reinician con el kernel.
//! Se exponen en `GET /health?verbose=true` dentro de `components.judge`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

/// Cubos de latencia (ms) del juez. Mas anchos que los del recall porque la
/// generacion medida en B-4 iba de 36,8 s a 59,8 s.
pub const LATENCY_BUCKET_BOUNDS_MS: [u64; 10] =
    [100, 500, 1_000, 2_500, 5_000, 10_000, 20_000, 30_000, 60_000, 90_000];

pub const BUCKETS: usize = LATENCY_BUCKET_BOUNDS_MS.len() + 1;

#[derive(Debug, Default)]
pub struct JudgeTelemetry {
    calls: AtomicU64,
    ok: AtomicU64,
    timeouts: AtomicU64,
    http_errors: AtomicU64,
    /// D-33: el backend **respondio rechazando** la peticion (400 por falta de
    /// `model`, 404 de ruta, 5xx con "Bad Request" dentro del cuerpo). Es el
    /// sintoma de "juez muerto por configuracion", distinto de un fallo de
    /// transporte o de un 5xx opaco (`http_errors`).
    backend_errors: AtomicU64,
    transport_errors: AtomicU64,
    parse_errors: AtomicU64,
    busy_skips: AtomicU64,
    in_flight: AtomicU64,
    total_ms: AtomicU64,
    max_ms: AtomicU64,
    buckets: [AtomicU64; BUCKETS],
}

pub fn bucket_index(ms: u64) -> usize {
    LATENCY_BUCKET_BOUNDS_MS
        .iter()
        .position(|bound| ms <= *bound)
        .unwrap_or(BUCKETS - 1)
}

impl JudgeTelemetry {
    /// Una llamada que llego a enviarse y termino (bien o mal).
    pub fn observe(&self, elapsed: Duration, outcome: JudgeOutcome) {
        let ms = elapsed.as_millis().min(u128::from(u64::MAX)) as u64;
        self.calls.fetch_add(1, Ordering::Relaxed);
        match outcome {
            JudgeOutcome::Ok => {
                self.ok.fetch_add(1, Ordering::Relaxed);
            }
            JudgeOutcome::Timeout => {
                self.timeouts.fetch_add(1, Ordering::Relaxed);
            }
            JudgeOutcome::HttpError => {
                self.http_errors.fetch_add(1, Ordering::Relaxed);
            }
            JudgeOutcome::BackendError => {
                self.backend_errors.fetch_add(1, Ordering::Relaxed);
            }
            JudgeOutcome::TransportError => {
                self.transport_errors.fetch_add(1, Ordering::Relaxed);
            }
            JudgeOutcome::ParseError => {
                self.parse_errors.fetch_add(1, Ordering::Relaxed);
            }
        }
        self.total_ms.fetch_add(ms, Ordering::Relaxed);
        self.max_ms.fetch_max(ms, Ordering::Relaxed);
        self.buckets[bucket_index(ms)].fetch_add(1, Ordering::Relaxed);
    }

    /// Una llamada que NO se envio porque el juez estaba ocupado (C1+C2: el
    /// semaforo no consiguio permiso a tiempo). No es un fallo del juez: es
    /// nuestra decision de no encolar, y por eso se cuenta aparte.
    pub fn note_busy_skip(&self) {
        self.busy_skips.fetch_add(1, Ordering::Relaxed);
    }

    pub fn enter(&self) -> InFlight<'_> {
        self.in_flight.fetch_add(1, Ordering::Relaxed);
        InFlight(self)
    }

    pub fn calls(&self) -> u64 {
        self.calls.load(Ordering::Relaxed)
    }
    pub fn ok(&self) -> u64 {
        self.ok.load(Ordering::Relaxed)
    }
    pub fn failures(&self) -> u64 {
        self.timeouts.load(Ordering::Relaxed)
            + self.http_errors.load(Ordering::Relaxed)
            + self.backend_errors.load(Ordering::Relaxed)
            + self.transport_errors.load(Ordering::Relaxed)
            + self.parse_errors.load(Ordering::Relaxed)
    }
    pub fn busy_skips(&self) -> u64 {
        self.busy_skips.load(Ordering::Relaxed)
    }
    pub fn in_flight(&self) -> u64 {
        self.in_flight.load(Ordering::Relaxed)
    }

    /// Media en ms, un decimal. `None` sin llamadas.
    pub fn avg_ms(&self) -> Option<f64> {
        let calls = self.calls();
        (calls > 0).then(|| self.total_ms.load(Ordering::Relaxed) as f64 / calls as f64)
    }

    pub fn max_ms(&self) -> u64 {
        self.max_ms.load(Ordering::Relaxed)
    }

    pub fn to_json(&self) -> serde_json::Value {
        let buckets: Vec<serde_json::Value> = self
            .buckets
            .iter()
            .enumerate()
            .map(|(i, c)| {
                serde_json::json!({
                    "le_ms": LATENCY_BUCKET_BOUNDS_MS.get(i).copied(),
                    "count": c.load(Ordering::Relaxed),
                })
            })
            .collect();
        serde_json::json!({
            "calls": self.calls(),
            "ok": self.ok(),
            "failures": self.failures(),
            "by_kind": {
                "timeout": self.timeouts.load(Ordering::Relaxed),
                "http": self.http_errors.load(Ordering::Relaxed),
                "backend_error": self.backend_errors.load(Ordering::Relaxed),
                "transport": self.transport_errors.load(Ordering::Relaxed),
                "parse": self.parse_errors.load(Ordering::Relaxed),
                "busy_skip": self.busy_skips(),
            },
            "in_flight": self.in_flight(),
            "latency_ms_avg": self.avg_ms(),
            "latency_ms_max": self.max_ms(),
            "latency_buckets": buckets,
            "timeout_ms": crate::security::coherence_gate::judge_timeout().as_millis() as u64,
            "queue_wait_ms": crate::security::coherence_gate::JUDGE_QUEUE_WAIT.as_millis() as u64,
            "max_concurrency": crate::security::coherence_gate::judge_max_concurrency(),
        })
    }
}

/// Guard de `in_flight`: se decrementa aunque la llamada haga panic o se cancele.
pub struct InFlight<'a>(&'a JudgeTelemetry);

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        self.0.in_flight.fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JudgeOutcome {
    Ok,
    Timeout,
    HttpError,
    /// D-33: el backend rechazo nuestra peticion (configuracion: falta `model`,
    /// ruta inexistente...). Se cuenta aparte para que `/health` pueda decir
    /// "el juez no puede emitir veredictos" en vez de "hay errores http".
    BackendError,
    TransportError,
    ParseError,
}

static TELEMETRY: OnceLock<JudgeTelemetry> = OnceLock::new();

pub fn judge_telemetry() -> &'static JudgeTelemetry {
    TELEMETRY.get_or_init(JudgeTelemetry::default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_index_covers_overflow_and_bounds() {
        assert_eq!(bucket_index(0), 0);
        assert_eq!(bucket_index(100), 0, "bound is inclusive");
        assert_eq!(bucket_index(101), 1);
        assert_eq!(bucket_index(90_000), LATENCY_BUCKET_BOUNDS_MS.len() - 1);
        assert_eq!(bucket_index(usize::MAX as u64), BUCKETS - 1, "overflow bucket");
    }

    #[test]
    fn in_flight_is_balanced_even_on_early_return() {
        let t = JudgeTelemetry::default();
        {
            let _g = t.enter();
            assert_eq!(t.in_flight(), 1);
        }
        assert_eq!(t.in_flight(), 0);
        let g = t.enter();
        drop(g);
        assert_eq!(t.in_flight(), 0);
    }

    #[test]
    fn observe_classifies_and_aggregates() {
        let t = JudgeTelemetry::default();
        t.observe(Duration::from_millis(1200), JudgeOutcome::Ok);
        t.observe(Duration::from_millis(300), JudgeOutcome::Timeout);
        t.observe(Duration::from_millis(80), JudgeOutcome::ParseError);
        assert_eq!(t.calls(), 3);
        assert_eq!(t.ok(), 1);
        assert_eq!(t.failures(), 2);
        assert_eq!(t.max_ms(), 1200);
        assert_eq!(t.avg_ms(), Some((1200.0 + 300.0 + 80.0) / 3.0));

        let json = t.to_json();
        assert_eq!(json["by_kind"]["timeout"], 1);
        assert_eq!(json["by_kind"]["parse"], 1);
        assert_eq!(json["by_kind"]["http"], 0);
        // Sin contenido: el JSON del juez no debe tener ninguna clave de texto.
        let text = json.to_string();
        assert!(!text.contains("prompt") && !text.contains("verdict"));
    }

    #[test]
    fn backend_error_is_its_own_kind_and_counts_as_failure() {
        let t = JudgeTelemetry::default();
        t.observe(Duration::from_millis(50), JudgeOutcome::BackendError);
        assert_eq!(t.failures(), 1, "un rechazo del backend es un fallo del juez");
        assert_eq!(t.ok(), 0);
        let json = t.to_json();
        assert_eq!(json["by_kind"]["backend_error"], 1);
        assert_eq!(json["by_kind"]["http"], 0, "no se mezcla con http");
    }

    #[test]
    fn busy_skips_are_not_counted_as_failures() {
        let t = JudgeTelemetry::default();
        t.note_busy_skip();
        t.note_busy_skip();
        assert_eq!(t.busy_skips(), 2);
        assert_eq!(t.calls(), 0, "a skip never reached the model");
        assert_eq!(t.failures(), 0, "a skip is our decision, not a judge failure");
        assert_eq!(t.to_json()["by_kind"]["busy_skip"], 2);
    }
}
