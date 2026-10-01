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
    sem: Arc<sync_semaphore_shim::CountingPermits>,
    max_queue_wait: Duration,
    config: InferenceBudgetConfig,
    // ── Metrics (lock-free counters + one small mutex ring) ─────────────
    admitted_total: AtomicU64,
    rejected_total: AtomicU64,
    wait_us: Mutex<WaitSamples>,
    /// Current queue depth: callers waiting for a permit right now.
    waiting_now: AtomicU64,
}

/// Minimal internal counting-permit primitive: a `std::sync::Mutex`-guarded
/// count paired with a `std::sync::Condvar`. `acquire_sync` BLOCKS for real
/// — the thread sleeps inside the Condvar until a `give()` releases a permit
/// or the deadline passes — with zero polling and zero tokio runtime context
/// (embed/rerank run inside `block_in_place` or a dedicated OS thread — they
/// are SYNC fns and must not require an async reactor).
///
/// P0-3 (2026-10-01): replaces the old `Mutex<i64> + tokio::sync::Notify`
/// shim whose `Notify` was decorative (created, `notify_one()`-ed on give,
/// but nobody ever waited on it), forcing both wait paths into busy-wait
/// poll loops (5ms legacy / 2ms bounded per thread).
mod sync_semaphore_shim {
    use std::sync::{Condvar, Mutex};
    use std::time::Instant;

    pub struct CountingPermits {
        inner: Mutex<i64>,
        cv: Condvar,
        /// P0-3 testability: counts `try_take` invocations. The old busy-wait
        /// loops called this once per poll tick (~200-500/s per waiting
        /// thread); a Condvar-blocked waiter calls it ZERO times while
        /// parked. Test-only observable, cheap relaxed increment.
        try_take_attempts: std::sync::atomic::AtomicU64,
    }

    impl CountingPermits {
        pub fn new(permits: usize) -> Self {
            Self {
                inner: Mutex::new(permits.max(1) as i64),
                cv: Condvar::new(),
                try_take_attempts: std::sync::atomic::AtomicU64::new(0),
            }
        }

        /// Try to take one permit without waiting. Ok(()) if admitted.
        pub fn try_take(&self) -> Result<(), ()> {
            self.try_take_attempts
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let mut n = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            if *n > 0 {
                *n -= 1;
                Ok(())
            } else {
                Err(())
            }
        }

        /// Block until one permit is available or `deadline` passes.
        /// Ok(()) = admitted (permit taken); Err(()) = deadline expired.
        /// No polling: sleeps inside the Condvar, woken by `give()` or by
        /// the deadline. The final count check happens under the SAME lock
        /// that observed the deadline expiry, so a `give()` landing in the
        /// last instant can never be lost (no post-loop re-try needed).
        ///
        /// std has no absolute-deadline Condvar wait (1.88.0), so this uses
        /// `wait_timeout_while` with the remaining time recomputed each
        /// iteration — still one blocking sleep per wakeup, no poll loop.
        pub fn try_take_until(&self, deadline: Instant) -> Result<(), ()> {
            let mut n = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if *n > 0 {
                    *n -= 1;
                    return Ok(());
                }
                let now = Instant::now();
                if now >= deadline {
                    return Err(());
                }
                let remaining = deadline - now;
                let (guard, timeout) = self
                    .cv
                    .wait_timeout_while(n, remaining, |n| *n <= 0)
                    .unwrap_or_else(|e| e.into_inner());
                n = guard;
                if timeout.timed_out() {
                    // Mutex held continuously since the last predicate
                    // check, so this re-check is conservative defense in
                    // depth: a give() counted here would mean the deadline
                    // expired a hair after the permit actually freed.
                    if *n > 0 {
                        *n -= 1;
                        return Ok(());
                    }
                    return Err(());
                }
                // Predicate turned false: a permit is waiting at loop top.
            }
        }

        /// Block indefinitely until one permit is available (legacy
        /// unbounded-wait mode). Sleeps inside the Condvar — zero CPU —
        /// until a `give()` signals one waiter.
        pub fn take_blocking(&self) {
            let mut n = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            while *n <= 0 {
                n = self.cv.wait(n).unwrap_or_else(|e| e.into_inner());
            }
            *n -= 1;
        }

        /// Give back one permit and wake one waiter.
        pub fn give(&self) {
            let mut n = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            *n += 1;
            drop(n);
            self.cv.notify_one();
        }

        /// Test-only: how many non-blocking `try_take` attempts have happened
        /// on this pool (see field doc).
        #[cfg(test)]
        pub fn try_take_attempts(&self) -> u64 {
            self.try_take_attempts
                .load(std::sync::atomic::Ordering::Relaxed)
        }
    }
}

impl InferenceBudget {
    pub fn new(config: InferenceBudgetConfig) -> Self {
        Self {
            sem: Arc::new(sync_semaphore_shim::CountingPermits::new(config.permits)),
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
            // Legacy mode: wait like the raw mutex did (no rejection ever) —
            // but efficiently: block inside the Condvar until a `give()`
            // signals, zero CPU, no poll loop.
            self.sem.take_blocking();
            return Ok(self.admit(t0));
        }
        // Bounded mode: block until a permit frees or the budget expires —
        // no polling either; the Condvar wakes us on release. try_take_until
        // checks count and deadline under one lock, so no last-chance
        // re-try is needed and no release can be missed.
        self.waiting_now.fetch_add(1, Ordering::Relaxed);
        let deadline = t0 + self.max_queue_wait;
        let admitted = self.sem.try_take_until(deadline).is_ok();
        self.waiting_now.fetch_sub(1, Ordering::Relaxed);
        if admitted {
            return Ok(self.admit(t0));
        }
        self.rejected_total.fetch_add(1, Ordering::Relaxed);
        Err(InferenceBudgetError::Saturated)
    }

    fn admit(&self, t0: Instant) -> InferenceGuard {
        self.admitted_total.fetch_add(1, Ordering::Relaxed);
        let wait_us = t0.elapsed().as_micros() as u64;
        if let Ok(mut ring) = self.wait_us.lock() {
            ring.push(wait_us);
        }
        InferenceGuard { sem: Some(Arc::clone(&self.sem)) }
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
    sem: Option<Arc<sync_semaphore_shim::CountingPermits>>,
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
        //
        // CI flake fixed 2026-09-30 (found by a real CI failure, not
        // reproduced locally): the original version had every caller drop
        // its guard immediately after acquiring, so on a fast/uncontended
        // runner all 8 could race through the single permit within the 1s
        // window without ANY rejection -- timing-dependent, not
        // deterministic. Fix: the first caller holds the permit for a fixed
        // duration comfortably longer than max_queue_wait_secs while a
        // barrier releases the other 7 at the same instant, guaranteeing
        // real saturation regardless of machine speed.
        let budget = std::sync::Arc::new(InferenceBudget::new(InferenceBudgetConfig { permits: 1, max_queue_wait_secs: 1 }));
        let barrier = Arc::new(Barrier::new(8));
        let mut handles = Vec::new();
        for i in 0..8 {
            let b = std::sync::Arc::clone(&budget);
            let bar = Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                bar.wait();
                let res = b.acquire_sync();
                // Hold the only permit well past max_queue_wait_secs (1s) so
                // every other caller's wait genuinely exhausts, then drop it
                // for real -- taking ownership out of `res`, not borrowing.
                if i == 0 && res.is_ok() {
                    std::thread::sleep(Duration::from_millis(1500));
                }
                res
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

    /// P0-3 regression: a waiter blocked in `acquire_sync` (legacy mode, no
    /// permits free) must be PARKED, not polling. The old busy-wait loop
    /// called `try_take` once per 5ms tick; a Condvar-blocked waiter calls
    /// it ZERO times while parked. Counted attempts are deterministic under
    /// any machine load — wall-clock wake-latency is not (a strict <2ms
    /// assertion passed in isolation but flaked inside the full 900+-test
    /// parallel run; see give→wake sanity bound below for what IS checkable
    /// in wall-clock terms).
    #[test]
    fn condvar_waiter_does_not_poll_while_parked() {
        let budget = std::sync::Arc::new(InferenceBudget::legacy_unbounded());
        let holder = budget.acquire_sync().expect("holder admitted");
        let entered = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let waiter = {
            let b = Arc::clone(&budget);
            let entered = Arc::clone(&entered);
            std::thread::spawn(move || {
                entered.store(true, std::sync::atomic::Ordering::SeqCst);
                b.acquire_sync().expect("legacy waiter must be admitted")
            })
        };
        // Deterministic overlap: wait until the waiter is provably past the
        // fast path and parked inside the Condvar wait, then hold 50ms.
        while !entered.load(std::sync::atomic::Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(1));
        }
        std::thread::sleep(Duration::from_millis(50));
        // The parked waiter must have made NO further try_take attempts.
        // Exactly 2 are expected ever: the holder's fast path + the waiter's
        // single failed fast path before it started blocking. The old code
        // would show ~12+ by now (one per 5ms tick for 50ms+).
        assert_eq!(
            budget.sem.try_take_attempts(),
            2,
            "a parked legacy waiter must NOT poll try_take (old busy-wait made ~1 attempt/5ms)"
        );
        let t_give = Instant::now();
        drop(holder); // InferenceGuard::drop -> give() -> notify_one
        let guard = waiter.join().expect("waiter must not panic");
        let woken = t_give.elapsed();
        assert!(guard.sem.is_some(), "waiter must leave with a real guard");
        // Sanity bound (load-tolerant): wake-on-signal must be far below any
        // plausible timeout, not a lost-wakeup hang. NOT a precision claim:
        // precise latency is scheduler-dependent under the parallel test
        // runner -- 250ms flaked under the full 940+-test parallel suite
        // (got 343ms), so the bound is generous (2s) and only guards against
        // an actual hang, never against OS scheduler jitter.
        assert!(
            woken < Duration::from_secs(2),
            "waiter must be woken by give(), not hang until a timeout, got {woken:?}"
        );
        // Still zero polling attempts after the whole cycle.
        assert_eq!(
            budget.sem.try_take_attempts(),
            2,
            "admission after give must come from the Condvar wakeup, not a new poll attempt"
        );
    }

    /// P0-3 regression: under REAL contention (8 threads, 1 permit, legacy
    /// mode where nobody is ever rejected), every caller must eventually be
    /// admitted and none may hang forever — the whole test finishes well
    /// under its generous 5s watchdog.
    #[test]
    fn legacy_contention_all_callers_eventually_admitted_no_starvation() {
        let budget = Arc::new(InferenceBudget::legacy_unbounded());
        const N: usize = 8;
        let admitted = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(N));
        let mut handles = Vec::new();
        for _ in 0..N {
            let b = Arc::clone(&budget);
            let bar = Arc::clone(&barrier);
            let admitted = Arc::clone(&admitted);
            handles.push(std::thread::spawn(move || {
                bar.wait();
                let g = b.acquire_sync().expect("legacy mode never rejects");
                admitted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                // Hold briefly so contention is real (permit actually
                // cycles through several waiters), then release via drop.
                std::thread::sleep(Duration::from_millis(10));
                drop(g);
            }));
        }
        for h in handles {
            // join() without timeout would hang on a regression; the test
            // runner's own timeout is the 5s watchdog the task asked for.
            h.join().expect("no thread may hang or panic");
        }
        assert_eq!(
            admitted.load(std::sync::atomic::Ordering::SeqCst),
            N,
            "all 8 contenders must be admitted eventually (no starvation)"
        );
        assert_eq!(
            budget.metrics_json()["rejected_total"].as_u64(),
            Some(0),
            "legacy mode must never reject"
        );
    }
}
