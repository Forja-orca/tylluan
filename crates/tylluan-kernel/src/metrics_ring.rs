//! # MetricsRingBuffer
//!
//! Circular buffer of 60 system-metric snapshots, sampled every 5 seconds by a
//! background Tokio task.  Thread-safe via `Arc<RwLock<MetricsRingBuffer>>`.

use std::collections::VecDeque;
use serde::Serialize;

/// A single point-in-time snapshot of key system metrics.
#[derive(Debug, Clone, Serialize)]
pub struct MetricsSnapshot {
    /// Unix timestamp in seconds when this snapshot was taken.
    pub ts: u64,
    /// Global CPU usage, 0.0 – 100.0 %.
    pub cpu: f32,
    /// System memory usage, 0.0 – 100.0 %.
    pub mem: f32,
    /// Mean latency across all guilds that have processed at least one call,
    /// or `None` when no guild data is available yet.
    pub avg_latency_ms: Option<f32>,
}

/// Ring buffer that keeps the last `CAPACITY` metric snapshots.
#[derive(Debug, Serialize)]
pub struct MetricsRingBuffer {
    buf: VecDeque<MetricsSnapshot>,
}

const CAPACITY: usize = 60;

impl MetricsRingBuffer {
    /// Create an empty ring buffer.
    pub fn new() -> Self {
        Self {
            buf: VecDeque::with_capacity(CAPACITY),
        }
    }

    /// Append a snapshot, evicting the oldest entry when at capacity.
    pub fn push(&mut self, snapshot: MetricsSnapshot) {
        if self.buf.len() == CAPACITY {
            self.buf.pop_front();
        }
        self.buf.push_back(snapshot);
    }

    /// Return all stored snapshots in chronological order (oldest first).
    pub fn snapshots(&self) -> Vec<&MetricsSnapshot> {
        self.buf.iter().collect()
    }
}

impl Default for MetricsRingBuffer {
    fn default() -> Self { Self::new() }
}

// ── Background sampler ──────────────────────────────────────────────────────

/// Spawn the background task that fills the ring buffer every 5 seconds.
///
/// Reuses the same `sysinfo` patterns already used in `doctor/mod.rs`:
///   `sys.refresh_cpu_usage()` × 2 (with a short sleep for the delta),
///   `sys.global_cpu_usage()`, `sys.total_memory()`, `sys.used_memory()`.
///
/// Guild latency comes from `RegistryHandle::guild_call_stats()`.
pub fn spawn_metrics_sampler(
    ring: std::sync::Arc<tokio::sync::RwLock<MetricsRingBuffer>>,
    registry: crate::registry::actor::RegistryHandle,
) {
    tokio::spawn(async move {
        // Keep a persistent System object so sysinfo can compute a proper CPU delta
        // between consecutive 5-second samples (no blocking sleep needed).
        let mut sys = sysinfo::System::new();

        // Prime the first CPU reading so the very first delta is meaningful.
        sys.refresh_cpu_usage();
        sys.refresh_memory();

        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        // Skip the first tick that fires immediately.
        interval.tick().await;

        loop {
            interval.tick().await;

            // --- CPU / memory ------------------------------------------------
            sys.refresh_cpu_usage();
            sys.refresh_memory();

            let cpu = sys.global_cpu_usage();
            let mem_total = sys.total_memory();
            let mem_used  = sys.used_memory();
            let mem_pct   = if mem_total > 0 {
                (mem_used as f32 / mem_total as f32) * 100.0
            } else {
                0.0
            };

            // --- Guild average latency ---------------------------------------
            let avg_latency_ms: Option<f32> = registry
                .guild_call_stats()
                .await
                .ok()
                .and_then(|stats| {
                    let active: Vec<f64> = stats
                        .iter()
                        .filter(|s| s.total_calls > 0)
                        .map(|s| s.avg_latency_ms)
                        .collect();
                    if active.is_empty() {
                        None
                    } else {
                        let sum: f64 = active.iter().sum();
                        Some((sum / active.len() as f64) as f32)
                    }
                });

            // --- Timestamp ---------------------------------------------------
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            let snapshot = MetricsSnapshot { ts, cpu, mem: mem_pct, avg_latency_ms };

            ring.write().await.push(snapshot);
        }
    });
}
