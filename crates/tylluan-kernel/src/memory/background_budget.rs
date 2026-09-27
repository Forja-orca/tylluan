//! # Background Work Budget — latency budget + concurrency limit
//!
//! WHY THIS EXISTS (2026-09-08, GraphRAG incident as reference): the 2026-08-30
//! incident showed `tylluan-nexus` at ~4257% CPU (42 of 56 cores) sustained for
//! 9.5h while idle: NightConsolidation's GraphRAG phase re-wrapped cluster
//! summaries unboundedly AND the federation auto-sync fired every 5-7s, with
//! the Agnostic Reindexer and HNSW Rebuild all free to run simultaneously.
//! Each individual job had a guard; nothing limited how many heavy background
//! jobs could run AT THE SAME TIME or how long they could hold the system.
//!
//! This module gives the heavy background loops (reindexer, HNSW rebuild,
//! memory consensus) two shared constraints:
//!   1. **Concurrency limit**: a tokio Semaphore with N permits shared by all
//!      heavy loops — at most N of them run at once system-wide.
//!   2. **Latency budget**: acquiring a permit has a bounded wait; if the
//!      permits are busy for longer than `max_wait`, the cycle SKIPS this tick
//!      instead of queuing indefinitely behind another heavy job. The system
//!      stays responsive to interactive traffic even under sustained
//!      background load — the property the GraphRAG incident lacked.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

pub struct BackgroundBudget {
    sem: Arc<Semaphore>,
    max_wait: Duration,
}

/// Held while a heavy background job runs; releases its permit on drop.
pub struct BudgetGuard {
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl BackgroundBudget {
    pub fn new(permits: usize, max_wait_secs: u64) -> Self {
        Self {
            sem: Arc::new(Semaphore::new(permits.max(1))),
            max_wait: Duration::from_secs(max_wait_secs),
        }
    }

    /// Non-blocking, read-only snapshot: is at least one permit free right
    /// now? Never acquires — an observer must not consume background budget
    /// or stall dispatch behind `max_wait` just by looking. Used by the
    /// Cognitive Scheduler's observation path (router::scheduler::observe).
    pub fn available(&self) -> bool {
        self.sem.available_permits() > 0
    }

    /// Try to enter the background budget within the latency budget.
    /// Returns None (skip this tick) if the permits are still busy after
    /// `max_wait` — the caller should skip its cycle rather than pile on.
    pub async fn acquire(&self) -> Option<BudgetGuard> {
        match tokio::time::timeout(self.max_wait, self.sem.clone().acquire_owned()).await {
            Ok(Ok(permit)) => Some(BudgetGuard { _permit: permit }),
            Ok(Err(_)) => None,
            Err(_elapsed) => {
                tracing::warn!("background budget: permits busy >{:.0}s — skipping this tick", self.max_wait.as_secs_f64());
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn budget_limits_concurrent_jobs_and_skips_when_busy() {
        let budget = BackgroundBudget::new(1, 1); // 1 permit, 1s wait budget
        let g1 = budget.acquire().await;
        assert!(g1.is_some(), "first job gets the permit");
        // Second job must wait up to 1s, then skip (g1 still held).
        let started = std::time::Instant::now();
        let g2 = budget.acquire().await;
        assert!(g2.is_none(), "second job skips its tick when budget is exhausted");
        assert!(started.elapsed() >= Duration::from_millis(950), "skip must respect the wait budget");
        drop(g1);
        let g3 = budget.acquire().await;
        assert!(g3.is_some(), "after release a new job can enter");
    }

    /// Regression (2026-09-27): the periodic SQLite maintenance loop (the
    /// checkpoint spawn in main.rs) is wired to the same background budget
    /// as reindexer/HNSW/consensus. This models the checkpoint's skip-if-busy
    /// contract: when the other heavy loops hold ALL permits, the checkpoint
    /// tick SKIPS (after the wait budget) instead of queuing unboundedly;
    /// once a permit frees, the next checkpoint tick enters.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn checkpoint_skips_when_budget_exhausted_by_heavy_loops() {
        let budget = BackgroundBudget::new(2, 1); // 2 permits: reindexer + HNSW
        let _reindex = budget.acquire().await.expect("reindexer enters");
        let _hnsw = budget.acquire().await.expect("HNSW enters");
        let started = std::time::Instant::now();
        let maint = budget.acquire().await;
        assert!(maint.is_none(), "checkpoint skips its tick when heavy loops hold all permits");
        assert!(started.elapsed() >= Duration::from_millis(950), "skip must respect the wait budget");
        drop(_hnsw);
        let maint2 = budget.acquire().await;
        assert!(maint2.is_some(), "checkpoint enters once a permit frees");
    }
}