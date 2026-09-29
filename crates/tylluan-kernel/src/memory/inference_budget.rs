//! # Inference Budget — bounded queue + explicit rejection for the INTERACTIVE
//! inference path (embeddings + reranker).
//!
//! WHY THIS EXISTS (Tarea Raíz 1, 2026-09-28 — closes the oldest unresolved
//! finding in the project): `EmbeddingEngine.model: Mutex<TextEmbedding>` and
//! `RerankEngine.model: Mutex<TextRerank>` are raw mutexes with no bounded
//! queue, no backpressure, no metrics and no explicit rejection. Under
//! concurrent load (the measured "8 agents / 50 recalls" case, STATUS.md
//! 2026-09-13) callers pile up on the mutex in acquisition order and die in
//! silence at the client's HTTP timeout — 68% of `tylluan_recall` requests
//! were lost that way.
//!
//! This module gives the interactive inference path the same shape
//! `background_budget.rs` gives heavy background loops, but with the opposite
//! contract: instead of SKIP (background may retry next tick), the caller gets
//! an EXPLICIT fast rejection (`InferenceBudgetError::Saturated`) that the HTTP
//! layer renders as 503 + Retry-After. No silent 60s timeouts, no unbounded
//! queue: either you are admitted within `max_queue_wait`, or you are told
//! immediately that the system is saturated — and the rejection is counted.
//!
//! Metrics (queue depth wait p50/p95, rejection rate) are exported via
//! [`InferenceBudget::metrics_json`] and surfaced in
//! `/api/v1/ops/golden-signals` (continuation of the MD-4 real-metrics work).
//!
//! DEFAULT PRESERVES CURRENT BEHAVIOR: with no explicit `[inference.budget]`
//! config the budget is UNBOUNDED-WAIT (equivalent to the old raw mutex wait),
//! so nothing breaks and no request is rejected until an operator opts in.
//! Opt-in is one line in `tylluan.toml` (see `InferenceBudgetConfig`).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Error returned when the interactive inference budget is exhausted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InferenceBudgetError {
    /// All permits were busy for longer than `max_queue_wait` — the caller
    /// was NOT admitted and should retry (HTTP layer maps this to
    /// 503 + Retry-After).
    Saturated,
}

impl std::fmt::Display for InferenceBudgetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InferenceBudgetError::Saturated => write!(f, "inference budget saturated: all permits busy for longer than max_queue_wait"),
        }
    }
}

impl std::error::Error for InferenceBudgetError {}

/// Declarative budget settings, wired to `[inference.budget]` in
/// `tylluan.toml`. `permits = 1` matches the physical reality that ONE local
/// ONNX model processes one inference at a time; raise it only if the engine
/// genuinely serves parallel inference (e.g. a GPU batch).
///
/// DEFAULT = LEGACY: `max_queue_wait_secs = 0` means UNBOUNDED WAIT (queue
/// like the raw mutex did, never reject). Rejection/backpressure is an
/// explicit operator opt-in — set `max_queue_wait_secs` to a positive value
/// (30 is a sane choice: strictly below the historical 60s client-timeout
/// death) to enable bounded queues with 503+Retry-After.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceBudgetConfig {
    /// Max concurrent interactive inferences admitted. Default 1.
    #[serde(default = "default_permits")]
    pub permits: usize,
    /// How long a caller may wait for a permit before being explicitly
    /// rejected. Default 0 = UNBOUNDED WAIT (legacy behavior preserved:
    /// queue like the raw mutex did, never reject).
    #[serde(default = "default_max_queue_wait_secs")]
    pub max_queue_wait_secs: u64,
}

fn default_permits() -> usize {
    1
}

fn default_max_queue_wait_secs() -> u64 {
    0
}

impl Default for InferenceBudgetConfig {
    fn default() -> Self {
        Self {
            permits: default_permits(),
            max_queue_wait_secs: default_max_queue_wait_secs(),
        }
    }
}

impl InferenceBudgetConfig {
    /// Is rejection enabled? `max_queue_wait_secs == 0` preserves the legacy
    /// unbounded-wait behavior (no caller is ever rejected).
    pub fn rejection_enabled(&self) -> bool {
        self.max_queue_wait_secs > 0
    }
}

/// Ring of recent queue-wait samples (bounded, lock-cheap). Stores wait times
/// in microseconds for admitted callers only — rejections are counted
/// separately and never wait the full budget twice.
const WAIT_SAMPLES: usize = 512;

struct WaitSamples {
    buf: Vec<u64>,
    idx: usize,
    filled: usize,
}

impl WaitSamples {
    fn new() -> Self {
        Self { buf: vec![0; WAIT_SAMPLES], idx: 0, filled: 0 }
    }

    fn push(&mut self, micros: u64) {
        self.buf[self.idx] = micros;
        self.idx = (self.idx + 1) % WAIT_SAMPLES;
        if self.filled < WAIT_SAMPLES {
            self.filled += 1;
        }
    }

    fn percentile(&self, p: f64) -> Option<u64> {
        if self.filled == 0 {
            return None;
        }
        let mut sorted = self.buf[..self.filled].to_vec();
        sorted.sort_unstable();
        let n = sorted.len();
        let k = ((p / 100.0) * n as f64).ceil().max(1.0) as usize;
        Some(sorted[(k - 1).min(n - 1)])
    }
}

/// Shared budget for the interactive inference path. Clone-friendly via Arc
/// at the call site (the engine structs embed it by value behind Arc).
pub struct InferenceBudget {
    sem: Arc<tokio_sync_semaphore_shim::CountingPermits>,
    max_queue_wait: Duration,
    config: InferenceBudgetConfig,
    // ── Metrics (lock-free counters + one small mutex ring) ─────────────
    admitted_total: AtomicU64,
    rejected_total: AtomicU64,
    wait_us: Mutex<WaitSamples>,
    /// Current queue depth: callers waiting for a permit right now.
    waiting_now: AtomicU64,
}

/// Minimal internal counting-permit primitive. Implemented with a
/// `std::sync::Mutex`-guarded count plus a tokio Notify, so `acquire_sync`
/// can poll without a tokio runtime context (embed/rerank run inside
/// `block_in_place` or a dedicated OS thread — they are SYNC fns and must
/// not require an async reactor).
mod tokio_sync_semaphore_shim {
    use std::sync::{Arc, Mutex};
    use tokio::sync::Notify;

    pub struct CountingPermits {
        inner: Mutex<i64>,
        notify: Arc<Notify>,
    }

    impl CountingPermits {
        pub fn new(permits: usize) -> Self {
            Self {
                inner: Mutex::new(permits.max(1) as i64),
                notify: Arc::new(Notify::new()),
            }
        }

        /// Try to take one permit. Ok(()) if admitted.
        pub fn try_take(&self) -> Result<(), ()> {
            let mut n = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            if *n > 0 {
                *n -= 1;
                Ok(())
            } else {
                Err(())
            }
        }

        /// Give back one permit and wake one waiter.
        pub fn give(&self) {
            {
                let mut n = self.inner.lock().unwrap_or_else(|e| e.into_inner());
                *n += 1;
            }
            self.notify.notify_one();
        }

        pub fn notify_arc(&self) -> Arc<Notify> {
            Arc::clone(&self.notify)
        }
    }
}

impl InferenceBudget {
    pub fn new(config: InferenceBudgetConfig) -> Self {
        Self {
            sem: Arc::new(tokio_sync_semaphore_shim::CountingPermits::new(config.permits)),
            max_queue_wait: Duration::from_secs(config.max_queue_wait_secs),
            config,
            admitted_total: AtomicU64::new(0),
            rejected_total: AtomicU64::new(0),
            wait_us: Mutex::new(WaitSamples::new()),
            waiting_now: AtomicU64::new(0),
        }
    }

    /// Default-config budget: UNBOUNDED WAIT (legacy raw-mutex behavior).
    /// Used when `[inference.budget]` is absent from the config file.
    pub fn legacy_unbounded() -> Self {
        Self::new(InferenceBudgetConfig {
            permits: 1,
            max_queue_wait_secs: 0,
        })
    }

    pub fn config(&self) -> &InferenceBudgetConfig {
        &self.config
    }

    /// Synchronous bounded acquisition. This is the entry the inference path
    /// uses (embed/rerank are sync fns running on the blocking pool — no
    /// tokio context is required here).
    ///
    /// Contract:
    /// - `max_queue_wait == 0` → unbounded wait, always admitted (legacy).
    /// - otherwise → wait up to `max_queue_wait`; on expiry return
    ///   `Err(Saturated)` FAST (never blocks past the budget).
    pub fn acquire_sync(&self) -> Result<InferenceGuard, InferenceBudgetError> {
        let t0 = Instant::now();
        // Fast path: permit free right now.
        if self.sem.try_take().is_ok() {
            return Ok(self.admit(t0));
        }
        if !self.config.rejection_enabled() {
            // Legacy mode: wait like the raw mutex did (no rejection ever).
            loop {
                if self.sem.try_take().is_ok() {
                    return Ok(self.admit(t0));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        // Bounded mode: poll with a short tick; on expiry reject explicitly.
        self.waiting_now.fetch_add(1, Ordering::Relaxed);
        let deadline = t0 + self.max_queue_wait;
        let mut admitted = false;
        while Instant::now() < deadline {
            // Wake on release signal OR short tick (bounded, cheap).
            if self.sem.try_take().is_ok() {
                admitted = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        if admitted || self.sem.try_take().is_ok() {
            self.waiting_now.fetch_sub(1, Ordering::Relaxed);
            return Ok(self.admit(t0));
        }
        self.waiting_now.fetch_sub(1, Ordering::Relaxed);
        self.rejected_total.fetch_add(1, Ordering::Relaxed);
        Err(InferenceBudgetError::Saturated)
    }

    fn admit(&self, t0: Instant) -> InferenceGuard {
        self.admitted_total.fetch_add(1, Ordering::Relaxed);
        let wait_us = t0.elapsed().as_micros() as u64;
        if let Ok(mut ring) = self.wait_us.lock() {
            ring.push(wait_us);
        }
        InferenceBudget::release_on_drop(self.sem.notify_arc(), self.sem.clone())
    }

    fn release_on_drop(
        _notify: Arc<tokio::sync::Notify>,
        sem: Arc<tokio_sync_semaphore_shim::CountingPermits>,
    ) -> InferenceGuard {
        InferenceGuard { sem: Some(sem) }
    }

    /// Process-wide budget shared by the dense engine and the reranker:
    /// one local machine serves ONE interactive inference at a time, so a
    /// single shared permit pool bounds the whole path. No call site acquires
    /// the budget while already holding it (embed and rerank each take it
    /// once, sequentially), so sharing cannot deadlock. Initialized from
    /// `[inference.budget]` at first use; missing/invalid config = legacy
    /// unbounded wait.
    pub fn global() -> Arc<InferenceBudget> {
        Arc::clone(&*GLOBAL_BUDGET)
    }

    /// Read-only metrics snapshot for golden-signals. Never acquires the
    /// budget (an observer must not consume inference capacity).
    pub fn metrics_json(&self) -> serde_json::Value {
        let (p50, p95) = {
            match self.wait_us.lock() {
                Ok(ring) => (
                    ring.percentile(50.0).map(|us| us / 1000),
                    ring.percentile(95.0).map(|us| us / 1000),
                ),
                Err(_) => (None, None),
            }
        };
        let admitted = self.admitted_total.load(Ordering::Relaxed);
        let rejected = self.rejected_total.load(Ordering::Relaxed);
        serde_json::json!({
            "permits": self.config.permits,
            "max_queue_wait_secs": self.config.max_queue_wait_secs,
            "rejection_enabled": self.config.rejection_enabled(),
            "waiting_now": self.waiting_now.load(Ordering::Relaxed),
            "admitted_total": admitted,
            "rejected_total": rejected,
            "rejection_rate_percent": if admitted + rejected > 0 {
                (rejected as f64 * 100.0 / (admitted + rejected) as f64 * 100.0).round() / 100.0
            } else { 0.0 },
            "wait_ms": { "p50": p50, "p95": p95 },
        })
    }
}

/// Process-wide shared budget (see [`InferenceBudget::global`]).
static GLOBAL_BUDGET: LazyLock<Arc<InferenceBudget>> = LazyLock::new(|| {
    let cfg = crate::config::TylluanConfig::load_cached().ok().and_then(|cfg| {
        cfg.try_read().ok().map(|g| g.inference.budget.clone())
    });
    Arc::new(InferenceBudget::new(cfg.unwrap_or_default()))
});

/// Held while an interactive inference runs; returns the permit on drop so a
/// panic mid-inference can never leak the budget.
pub struct InferenceGuard {
    sem: Option<Arc<tokio_sync_semaphore_shim::CountingPermits>>,
}

impl Drop for InferenceGuard {
    fn drop(&mut self) {
        if let Some(sem) = self.sem.take() {
            sem.give();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Barrier;
    use std::time::Duration;

    #[test]
    fn admission_and_release_round_trip() {
        let budget = InferenceBudget::new(InferenceBudgetConfig { permits: 1, max_queue_wait_secs: 1 });
        {
            let g = budget.acquire_sync().expect("free permit must admit");
            assert!(g.sem.is_some());
            // Second caller waits at most max_queue_wait (1s) and is then
            // EXPLICITLY rejected — bounded, observable, never a silent
            // infinite mutex wait (mirrors background_budget's contract).
            let t0 = Instant::now();
            let second = budget.acquire_sync();
            assert!(matches!(second, Err(InferenceBudgetError::Saturated)), "must reject explicitly while permit is held");
            let waited = t0.elapsed();
            assert!(waited >= Duration::from_millis(950), "rejection must respect the wait budget first, got {waited:?}");
            assert!(waited <= Duration::from_millis(1300), "rejection must be BOUNDED, not an unobservable pile-up, got {waited:?}");
        }
        // After drop the permit returns.
        let again = budget.acquire_sync();
        assert!(again.is_ok(), "permit must be available again after drop");
        assert_eq!(budget.metrics_json()["admitted_total"].as_u64(), Some(2));
        assert_eq!(budget.metrics_json()["rejected_total"].as_u64(), Some(1));
    }

    #[test]
    fn regression_rejection_is_explicit_not_silent_block() {
        // The exact property the 68% recall loss lacked: under saturation the
        // caller gets an explicit error in bounded time instead of an
        // unobservable mutex wait.
        let budget = InferenceBudget::new(InferenceBudgetConfig { permits: 1, max_queue_wait_secs: 2 });
        let _held = budget.acquire_sync().expect("holder admitted");
        let t0 = Instant::now();
        let res = budget.acquire_sync();
        let elapsed = t0.elapsed();
        assert!(matches!(res, Err(InferenceBudgetError::Saturated)), "must reject explicitly under saturation");
        // Waited at most the budget (+ small scheduling slack), never 60s.
        assert!(elapsed <= Duration::from_millis(2100), "must respect max_queue_wait, got {elapsed:?}");
        let m = budget.metrics_json();
        assert_eq!(m["rejected_total"].as_u64(), Some(1));
        assert!(m["rejection_rate_percent"].as_f64().unwrap_or(0.0) > 0.0);
    }

    #[test]
    fn legacy_unbounded_wait_never_rejects() {
        // Default-preservation contract: rejection disabled → the caller
        // waits (like the old raw mutex) and is always admitted eventually.
        let budget = InferenceBudget::legacy_unbounded();
        let holder = budget.acquire_sync().expect("first admitted");
        let b2 = std::sync::Arc::new(budget);
        let b2_metrics = std::sync::Arc::clone(&b2);
        let barrier = Arc::new(Barrier::new(2));
        let h2 = barrier.clone();
        let handle = std::thread::spawn(move || {
            h2.wait();
            b2.acquire_sync().expect("legacy mode must NOT reject")
        });
        barrier.wait();
        std::thread::sleep(Duration::from_millis(150));
        drop(holder);
        let res = handle.join().expect("waiter thread must not panic");
        assert!(res.sem.is_some(), "legacy unbounded wait must admit once released");
        assert_eq!(b2_metrics.metrics_json()["rejected_total"].as_u64(), Some(0));
    }

    #[test]
    fn concurrent_saturations_produce_explicit_rejections_not_hangs() {
        // n callers against 1 permit with a tiny budget: some admit, the rest
        // are explicitly rejected — the whole call must finish promptly.
        let budget = std::sync::Arc::new(InferenceBudget::new(InferenceBudgetConfig { permits: 1, max_queue_wait_secs: 1 }));
        let mut handles = Vec::new();
        for i in 0..8 {
            let b = std::sync::Arc::clone(&budget);
            handles.push(std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(i * 5));
                b.acquire_sync()
            }));
        }
        let mut admitted = 0usize;
        let mut rejected = 0usize;
        for h in handles {
            match h.join().expect("no panic") {
                Ok(_) => admitted += 1,
                Err(InferenceBudgetError::Saturated) => rejected += 1,
            }
        }
        assert!(admitted >= 1, "at least the first caller is admitted");
        assert!(rejected >= 1, "saturation must produce explicit rejections");
        assert_eq!(admitted + rejected, 8);
    }

    #[test]
    fn wait_percentiles_recorded_for_admitted_calls() {
        let budget = InferenceBudget::new(InferenceBudgetConfig { permits: 1, max_queue_wait_secs: 2 });
        let b = std::sync::Arc::new(budget);
        let holder = b.acquire_sync().expect("holder");
        // Deterministic overlap: the waiter SIGNALS it is about to block on
        // the permit, and only then does the holder release — a slow thread
        // spawn can never let the waiter sneak in via the fast path.
        let entered = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let waiter = {
            let bb = std::sync::Arc::clone(&b);
            let entered = std::sync::Arc::clone(&entered);
            std::thread::spawn(move || {
                entered.store(true, std::sync::atomic::Ordering::SeqCst);
                bb.acquire_sync().expect("waiter admitted after release")
            })
        };
        while !entered.load(std::sync::atomic::Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(1));
        }
        std::thread::sleep(Duration::from_millis(50));
        drop(holder);
        let _g = waiter.join().expect("join");
        let m = b.metrics_json();
        // The waiter queued behind the holder for >=~50ms; the holder's own
        // fast-path ~0ms sample is also in the ring, so p50 may legitimately
        // be the fast one — p95 must capture the queued wait.
        assert!(m["wait_ms"]["p95"].as_u64().unwrap_or(0) >= 10, "queued wait must be measured in p95, got {m}");
        assert!(m["wait_ms"]["p95"].as_u64().unwrap_or(0) >= m["wait_ms"]["p50"].as_u64().unwrap_or(0));
    }
}
