//! D-5: telemetría de uso de `tylluan_recall`.
//!
//! Contadores **en memoria**, sin persistencia y sin I/O: la pregunta que
//! responden es operativa ("¿se usa el recall? ¿devuelve algo? ¿cuánto tarda?"),
//! no forense. Se reinician al reiniciar el kernel, y eso es deliberado: un
//! fichero aparte para esto sería otro log que rotar.
//!
//! Se exponen en `GET /health?verbose=true` dentro de `components.recall`, junto
//! al resto del estado del sistema — sin endpoint nuevo.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

/// Cota superior (ms) de cada cubo, en orden creciente. El último cubo
/// (`BUCKETS - 1`) recoge todo lo que pase del mayor.
pub const LATENCY_BUCKET_BOUNDS_MS: [u64; 9] = [5, 10, 25, 50, 100, 250, 500, 1000, 2500];

/// Un cubo por cota + el de desbordamiento.
pub const BUCKETS: usize = LATENCY_BUCKET_BOUNDS_MS.len() + 1;

/// Contadores de uso del recall. Todos los métodos toman `&self`: los campos
/// son atómicos, así que varias tool calls concurrentes no se pisan.
#[derive(Debug, Default)]
pub struct RecallTelemetry {
    calls: AtomicU64,
    empty_calls: AtomicU64,
    total_ms: AtomicU64,
    buckets: [AtomicU64; BUCKETS],
}

impl RecallTelemetry {
    /// Índice del cubo para una latencia en ms.
    pub fn bucket_index(ms: u64) -> usize {
        LATENCY_BUCKET_BOUNDS_MS
            .iter()
            .position(|bound| ms <= *bound)
            .unwrap_or(BUCKETS - 1)
    }

    /// Registra una llamada completada. `empty` = no devolvió ninguna memoria.
    pub fn observe(&self, elapsed: Duration, empty: bool) {
        let ms = elapsed.as_millis().min(u128::from(u64::MAX)) as u64;
        self.calls.fetch_add(1, Ordering::Relaxed);
        if empty {
            self.empty_calls.fetch_add(1, Ordering::Relaxed);
        }
        self.total_ms.fetch_add(ms, Ordering::Relaxed);
        self.buckets[Self::bucket_index(ms)].fetch_add(1, Ordering::Relaxed);
    }

    pub fn calls(&self) -> u64 {
        self.calls.load(Ordering::Relaxed)
    }

    pub fn empty_calls(&self) -> u64 {
        self.empty_calls.load(Ordering::Relaxed)
    }

    /// Media de latencia en ms, con un decimal. `None` si aún no hay llamadas
    /// (no inventamos un cero que parezca una medición).
    pub fn mean_latency_ms(&self) -> Option<f64> {
        let calls = self.calls();
        if calls == 0 {
            return None;
        }
        let total = self.total_ms.load(Ordering::Relaxed);
        Some((total as f64 / calls as f64 * 10.0).round() / 10.0)
    }

    /// Proporción de llamadas sin resultados, con tres decimales.
    pub fn empty_ratio(&self) -> Option<f64> {
        let calls = self.calls();
        if calls == 0 {
            return None;
        }
        let empty = self.empty_calls();
        Some((empty as f64 / calls as f64 * 1000.0).round() / 1000.0)
    }

    /// Histograma como pares (etiqueta, recuento), en orden.
    pub fn histogram(&self) -> Vec<(String, u64)> {
        let mut out = Vec::with_capacity(BUCKETS);
        for (i, bound) in LATENCY_BUCKET_BOUNDS_MS.iter().enumerate() {
            out.push((
                format!("<={bound}ms"),
                self.buckets[i].load(Ordering::Relaxed),
            ));
        }
        out.push((
            format!(">{}ms", LATENCY_BUCKET_BOUNDS_MS[BUCKETS - 2]),
            self.buckets[BUCKETS - 1].load(Ordering::Relaxed),
        ));
        out
    }

    /// Representación para `/health?verbose=true` (`components.recall`).
    pub fn to_json(&self) -> serde_json::Value {
        let histogram: serde_json::Map<String, serde_json::Value> = self
            .histogram()
            .into_iter()
            .map(|(label, count)| (label, serde_json::Value::from(count)))
            .collect();
        serde_json::json!({
            "calls": self.calls(),
            "empty_calls": self.empty_calls(),
            "empty_ratio": self.empty_ratio(),
            "mean_latency_ms": self.mean_latency_ms(),
            "latency_histogram": histogram,
        })
    }
}

/// Instancia de proceso. `OnceLock` en vez de un campo más en `TylluanServer`
/// porque el handler del tool no recibe el estado HTTP y así ambos lados leen
/// exactamente los mismos contadores.
static TELEMETRY: OnceLock<RecallTelemetry> = OnceLock::new();

pub fn recall_telemetry() -> &'static RecallTelemetry {
    TELEMETRY.get_or_init(RecallTelemetry::default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_index_maps_boundaries() {
        assert_eq!(RecallTelemetry::bucket_index(0), 0);
        assert_eq!(RecallTelemetry::bucket_index(5), 0);
        assert_eq!(RecallTelemetry::bucket_index(6), 1);
        assert_eq!(RecallTelemetry::bucket_index(2500), BUCKETS - 2);
        assert_eq!(RecallTelemetry::bucket_index(2501), BUCKETS - 1);
        assert_eq!(RecallTelemetry::bucket_index(u64::MAX), BUCKETS - 1);
    }

    #[test]
    fn records_calls_and_empty_ratio() {
        let t = RecallTelemetry::default();
        assert_eq!(t.calls(), 0);
        assert_eq!(t.empty_ratio(), None, "sin llamadas no hay proporción");
        assert_eq!(t.mean_latency_ms(), None, "sin llamadas no hay media");

        t.observe(Duration::from_millis(10), false);
        t.observe(Duration::from_millis(20), false);
        t.observe(Duration::from_millis(30), true);
        t.observe(Duration::from_millis(40), true);

        assert_eq!(t.calls(), 4);
        assert_eq!(t.empty_calls(), 2);
        assert_eq!(t.empty_ratio(), Some(0.5));
        assert_eq!(t.mean_latency_ms(), Some(25.0));
    }

    #[test]
    fn histogram_buckets_sum_to_calls() {
        let t = RecallTelemetry::default();
        for ms in [0u64, 5, 6, 100, 101, 2500, 2501, 9000] {
            t.observe(Duration::from_millis(ms), false);
        }
        let total: u64 = t.histogram().iter().map(|(_, c)| *c).sum();
        assert_eq!(total, t.calls());
        assert_eq!(t.histogram().len(), BUCKETS);
    }

    #[test]
    fn json_shape_is_stable() {
        let t = RecallTelemetry::default();
        t.observe(Duration::from_millis(12), true);
        let j = t.to_json();
        assert_eq!(j["calls"], 1);
        assert_eq!(j["empty_calls"], 1);
        assert_eq!(j["empty_ratio"], 1.0);
        assert_eq!(j["mean_latency_ms"], 12.0);
        let hist = j["latency_histogram"].as_object().expect("histogram is an object");
        assert_eq!(hist.len(), BUCKETS, "una entrada por cubo, incluido el desbordamiento");
        assert_eq!(hist["<=25ms"], 1);
        assert_eq!(hist[">2500ms"], 0);
    }

    #[test]
    fn sub_millisecond_calls_count_in_first_bucket() {
        let t = RecallTelemetry::default();
        t.observe(Duration::from_micros(300), false);
        assert_eq!(t.histogram()[0].1, 1, "0 ms cae en el primer cubo, no se pierde");
        assert_eq!(t.mean_latency_ms(), Some(0.0));
    }
}
