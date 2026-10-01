use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use anyhow::{anyhow, Result};

const CACHE_TTL: Duration = Duration::from_secs(300);
const MAX_ENTRIES: usize = 256;

struct Entry {
    embedding: Vec<f32>,
    inserted_at: Instant,
}

/// Published outcome of an in-flight embedding, shared with every caller that
/// joined the same normalized query while it was being computed. The error is
/// stored as a formatted string because `anyhow::Error` is not `Clone`: the
/// leader returns the original error to its own caller, joiners receive a
/// re-wrapped copy carrying the same message.
type InflightOutcome = Result<Vec<f32>, String>;

/// One single-flight computation slot. Joiners block on `OnceLock::wait` until
/// the leader publishes the outcome (or the leader's panic guard fills the
/// slot with an error, so a panicking leader can never leave joiners hanging).
type InflightSlot = Arc<OnceLock<InflightOutcome>>;

/// TTL-based query embedding cache for the recall path.
///
/// Caches embeddings keyed by normalized query text (trimmed + lowercased).
/// Entries expire after CACHE_TTL (5 min) — short enough to avoid staleness,
/// long enough to cover repeated queries in conversational windows.
///
/// Eviction: LRU by insertion timestamp when at MAX_ENTRIES capacity.
///
/// Concurrency contract (P0-1, 2026-10-01):
/// - the result-cache lock is NEVER held across `embed_fn` (the 2-8s ONNX
///   call): N queries with DIFFERENT text never serialize behind one another;
/// - single-flight: concurrent callers of the SAME normalized query join one
///   in-flight computation instead of each launching its own ONNX inference.
///   Invariant: never two simultaneous inferences for the same text.
///
/// This cache lives inside SilvaDB and only caches *query* embeddings (recall).
/// Ingest embeddings (remember) are NOT cached — they are unique by definition.
pub struct QueryEmbeddingCache {
    inner: Mutex<HashMap<String, Entry>>,
    /// Single-flight registry: normalized query -> slot of the computation
    /// currently running for it. Kept SEPARATE from `inner` so the result
    /// cache stays unlocked while inference runs. Entries live exactly as
    /// long as the computation: the leader's guard removes them on drop
    /// (normal return, error return, or panic).
    inflight: Arc<Mutex<HashMap<String, InflightSlot>>>,
}

impl Default for QueryEmbeddingCache {
    fn default() -> Self {
        Self::new()
    }
}

impl QueryEmbeddingCache {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::with_capacity(MAX_ENTRIES)),
            inflight: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn normalize(query: &str) -> String {
        query
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }

    /// Returns a cached embedding if present and within TTL.
    /// Otherwise computes via `embed_fn` (outside every lock) and returns the
    /// fresh embedding, sharing ONE computation with concurrent callers of the
    /// same normalized query. LRU eviction runs when the cache is at capacity.
    pub fn get_or_embed(
        &self,
        query: &str,
        embed_fn: impl FnOnce(&str) -> Result<Vec<f32>>,
    ) -> Result<Vec<f32>> {
        let key = Self::normalize(query);

        // 1) Fast path: cache hit — short lock, released before any computation.
        let hit = {
            let cache = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            cache
                .get(&key)
                .filter(|e| e.inserted_at.elapsed() < CACHE_TTL)
                .map(|e| e.embedding.clone())
        };
        if let Some(embedding) = hit {
            return Ok(embedding);
        }

        // 2) Single-flight: join the computation already running for this
        //    exact normalized query, or become its leader. Exactly one thread
        //    wins the leader role — the inflight lock serializes the decision.
        let (slot, is_leader) = {
            let mut inflight = self.inflight.lock().unwrap_or_else(|e| e.into_inner());
            match inflight.get(&key) {
                Some(running) => (Arc::clone(running), false),
                None => {
                    let slot: InflightSlot = Arc::new(OnceLock::new());
                    inflight.insert(key.clone(), Arc::clone(&slot));
                    (slot, true)
                }
            }
        };

        if !is_leader {
            // Joiner: block on the leader's slot. No lock is held while
            // waiting, so this never blocks queries with different text, and
            // it replaces what used to be a duplicate ONNX inference.
            return match slot.wait() {
                Ok(embedding) => Ok(embedding.clone()),
                Err(err) => Err(anyhow!("concurrent query embedding failed: {err}")),
            };
        }

        // 3) Leader: compute OUTSIDE every lock (this is the 2-8s ONNX call).
        //    The guard guarantees joiners are released and the inflight entry
        //    is freed even if embed_fn panics mid-inference.
        let guard = InflightGuard {
            inflight: Arc::clone(&self.inflight),
            key: key.clone(),
            slot: Arc::clone(&slot),
        };
        let result = embed_fn(query);

        // Publish to joiners BEFORE touching the result cache (joiners read
        // the slot, not the cache). `set` is a no-op if already filled.
        let _ = slot.set(match &result {
            Ok(embedding) => Ok(embedding.clone()),
            Err(err) => Err(format!("{err:#}")),
        });

        // Only successes are cached — the old code cached nothing on error
        // either, so transient failures stay retryable, not sticky.
        if let Ok(embedding) = &result {
            let mut cache = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            if cache.len() >= MAX_ENTRIES {
                Self::evict_lru(&mut cache);
            }
            cache.insert(
                key,
                Entry {
                    embedding: embedding.clone(),
                    inserted_at: Instant::now(),
                },
            );
        }

        drop(guard);
        result
    }

    /// Remove the single oldest entry (by insertion timestamp).
    fn evict_lru(cache: &mut HashMap<String, Entry>) {
        if let Some(oldest_key) = cache
            .iter()
            .min_by_key(|(_, e)| e.inserted_at)
            .map(|(k, _)| k.clone())
        {
            cache.remove(&oldest_key);
        }
    }

    /// Clear all cached embeddings.
    /// Called after `tylluan_remember` to ensure fresh embeddings on subsequent recalls.
    /// Read a fresh cached embedding for `query`, if present (no computation).
    /// Lets search_hybrid share one cache across all callers (api_memory,
    /// think, autolink, dual_retrieval...) instead of re-embedding identical
    /// queries on every path.
    pub fn get(&self, query: &str) -> Option<Vec<f32>> {
        let key = Self::normalize(query);
        let cache = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = cache.get(&key)
            && entry.inserted_at.elapsed() < CACHE_TTL {
                return Some(entry.embedding.clone());
            }
        None
    }

    /// Store an already-computed embedding for `query` (LRU eviction at cap).
    pub fn put(&self, query: &str, embedding: Vec<f32>) {
        let key = Self::normalize(query);
        let mut cache = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if cache.len() >= MAX_ENTRIES {
            Self::evict_lru(&mut cache);
        }
        cache.insert(key, Entry { embedding, inserted_at: Instant::now() });
    }

    pub fn invalidate(&self) {
        if let Ok(mut cache) = self.inner.lock() {
            cache.clear();
        }
    }

    /// Current number of cached entries (for diagnostics).
    pub fn len(&self) -> usize {
        self.inner.lock().map(|c| c.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// RAII cleanup for the leader of a single-flight computation. On drop —
/// normal return, error return, or unwind — it releases every joiner and
/// frees the inflight slot so the next caller can retry. Without this, a
/// leader panicking mid-`embed_fn` would leave the slot unset forever and
/// every future caller of that query blocked on it.
struct InflightGuard {
    inflight: Arc<Mutex<HashMap<String, InflightSlot>>>,
    key: String,
    slot: InflightSlot,
}

impl Drop for InflightGuard {
    fn drop(&mut self) {
        // If the leader never published (panic), fill the slot with an error
        // so waiters wake up; `set` is a no-op when the outcome was set.
        let _ = self.slot.set(Err(
            "query embedding computation did not complete (leader panicked)".to_string(),
        ));
        let mut inflight = self.inflight.lock().unwrap_or_else(|e| e.into_inner());
        // Only remove OUR entry — never one a subsequent leader re-created.
        if inflight
            .get(&self.key)
            .is_some_and(|s| Arc::ptr_eq(s, &self.slot))
        {
            inflight.remove(&self.key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    #[test]
    fn test_cache_hit_returns_same_vector() {
        let cache = QueryEmbeddingCache::new();
        let v = cache.get_or_embed("hello world", |_| Ok(vec![1.0, 2.0, 3.0])).unwrap();
        assert_eq!(v, vec![1.0, 2.0, 3.0]);
        // Second call with same query should hit cache (embed_fn not called)
        let v2 = cache.get_or_embed("hello world", |_| Ok(vec![9.9, 9.9, 9.9])).unwrap();
        assert_eq!(v2, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_cache_normalization_dedup() {
        let cache = QueryEmbeddingCache::new();
        let _ = cache.get_or_embed("  Hello   World  ", |_| Ok(vec![0.1, 0.2])).unwrap();
        let v = cache.get_or_embed("hello world", |_| Ok(vec![0.9, 0.9])).unwrap();
        assert_eq!(v, vec![0.1, 0.2]);
    }

    #[test]
    fn test_different_queries_miss() {
        let cache = QueryEmbeddingCache::new();
        let v1 = cache.get_or_embed("query one", |_| Ok(vec![1.0])).unwrap();
        let v2 = cache.get_or_embed("query two", |_| Ok(vec![2.0])).unwrap();
        assert_ne!(v1, v2);
    }

    #[test]
    fn test_invalidate_clears_all() {
        let cache = QueryEmbeddingCache::new();
        let _ = cache.get_or_embed("foo", |_| Ok(vec![1.0])).unwrap();
        assert_eq!(cache.len(), 1);
        cache.invalidate();
        assert_eq!(cache.len(), 0);
        let v = cache.get_or_embed("foo", |_| Ok(vec![2.0])).unwrap();
        assert_eq!(v, vec![2.0]);
    }

    #[test]
    fn test_cache_hit_latency_under_2ms() {
        let cache = QueryEmbeddingCache::new();
        // Embed a realistic-length query (40+ words like a real conversation topic)
        let query = "what was the architecture decision about the mesh protocol and how does it relate to consensus in the federation layer for the memory system";
        let mut _total_fresh_ns: u128 = 0;
        let mut total_hit_ns: u128 = 0;
        let trials = 100;
        for _ in 0..trials {
            // Fresh embed (cache miss) — time the embedding itself
            let start = std::time::Instant::now();
            let _ = cache.get_or_embed(query, |_q| {
                // Simulate real embedding latency (~50ms for BGE-M3 on CPU)
                std::thread::sleep(std::time::Duration::from_millis(50));
                Ok(vec![0.1; 768])
            }).unwrap();
            _total_fresh_ns += start.elapsed().as_nanos();

            // Cache hit — should be near-instant
            let start = std::time::Instant::now();
            let _ = cache.get_or_embed(query, |_q| {
                panic!("should not be called on cache hit");
            }).unwrap();
            total_hit_ns += start.elapsed().as_nanos();
        }
        // Allow overhead: 2ms per cache hit
        let avg_hit_ms = total_hit_ns as f64 / trials as f64 / 1_000_000.0;
        assert!(avg_hit_ms < 2.0, "average cache hit latency {avg_hit_ms:.3}ms >= 2ms");
    }

    #[test]
    fn test_eviction_at_capacity() {
        let cache = QueryEmbeddingCache::new();
        // Fill exactly to MAX_ENTRIES
        for i in 0..MAX_ENTRIES {
            let q = format!("query_{i}");
            let _ = cache.get_or_embed(&q, |_| Ok(vec![i as f32])).unwrap();
        }
        assert_eq!(cache.len(), MAX_ENTRIES);
        // One more triggers eviction
        let _ = cache.get_or_embed("query_new", |_| Ok(vec![999.0])).unwrap();
        assert_eq!(cache.len(), MAX_ENTRIES);
        // First entry should be gone
        let v = cache.get_or_embed("query_0", |_| Ok(vec![0.0])).unwrap();
        // query_0 was evicted, so it re-embeds with 0.0 (not from cache)
        assert_eq!(v, vec![0.0]);
    }

    // ---- P0-1 (2026-10-01): lock released during embed_fn + single-flight ----

    /// N threads requesting the SAME normalized query concurrently must share
    /// exactly ONE `embed_fn` invocation (one ONNX inference), and every one
    /// of them must receive the leader's result. The leader is inside
    /// `embed_fn` (sleeping) while the others pile up — with the old
    /// lock-held-during-embed_fn code this test would instead serialize
    /// without deduping (calls == N).
    #[test]
    fn test_single_flight_identical_queries_compute_once() {
        let cache = Arc::new(QueryEmbeddingCache::new());
        let calls = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(std::sync::Barrier::new(8));
        let mut handles = Vec::new();
        for _ in 0..8 {
            let (cache, calls, barrier) =
                (Arc::clone(&cache), Arc::clone(&calls), Arc::clone(&barrier));
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                cache
                    .get_or_embed("shared recall query", |_q| {
                        calls.fetch_add(1, Ordering::SeqCst);
                        // Simulated ONNX latency: hold the computation long
                        // enough for every joiner to reach the inflight map.
                        std::thread::sleep(Duration::from_millis(200));
                        Ok(vec![7.0; 4])
                    })
                    .unwrap()
            }));
        }
        for h in handles {
            assert_eq!(
                h.join().unwrap(),
                vec![7.0; 4],
                "every caller receives the shared result"
            );
        }
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "identical concurrent queries must share ONE embed_fn invocation"
        );
    }

    /// Two DIFFERENT queries must not serialize: while query A's embed_fn is
    /// deliberately still blocked, query B (instant embed_fn) must complete.
    /// Under the old code the Mutex was held across embed_fn, so B could only
    /// finish after A's release — this test hangs/fails there.
    #[test]
    fn test_concurrent_distinct_queries_do_not_serialize() {
        let cache = Arc::new(QueryEmbeddingCache::new());
        let a_entered = Arc::new(AtomicBool::new(false));
        let release_a = Arc::new(AtomicBool::new(false));

        let handle_a = {
            let (cache, a_entered, release_a) =
                (Arc::clone(&cache), Arc::clone(&a_entered), Arc::clone(&release_a));
            std::thread::spawn(move || {
                cache
                    .get_or_embed("slow query a", |_q| {
                        a_entered.store(true, Ordering::SeqCst);
                        while !release_a.load(Ordering::SeqCst) {
                            std::thread::sleep(Duration::from_millis(2));
                        }
                        Ok(vec![1.0; 2])
                    })
                    .unwrap()
            })
        };
        while !a_entered.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(1));
        }

        let handle_b = {
            let cache = Arc::clone(&cache);
            std::thread::spawn(move || {
                cache
                    .get_or_embed("fast query b", |_q| Ok(vec![2.0; 2]))
                    .unwrap()
            })
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        while !handle_b.is_finished() {
            assert!(
                Instant::now() < deadline,
                "a distinct concurrent query must not wait for the in-flight one"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(handle_b.join().unwrap(), vec![2.0; 2]);

        // Release A and make sure everything drains cleanly.
        release_a.store(true, Ordering::SeqCst);
        assert_eq!(handle_a.join().unwrap(), vec![1.0; 2]);
    }

    /// Leader failure must not stick: joiners wake with an error (never hang
    /// on an unpublished slot), nothing is cached, the inflight entry is
    /// freed, and the next caller retries as a fresh leader — exactly two
    /// embed_fn invocations total.
    #[test]
    fn test_single_flight_error_wakes_joiners_and_retries() {
        let cache = Arc::new(QueryEmbeddingCache::new());
        let attempts = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(std::sync::Barrier::new(4));
        let mut handles = Vec::new();
        for _ in 0..4 {
            let (cache, attempts, barrier) =
                (Arc::clone(&cache), Arc::clone(&attempts), Arc::clone(&barrier));
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                cache.get_or_embed("transient failure query", |_q| {
                    let n = attempts.fetch_add(1, Ordering::SeqCst);
                    if n == 0 {
                        // Hold the first (failing) computation long enough
                        // for the other threads to join it as waiters.
                        std::thread::sleep(Duration::from_millis(300));
                        Err(anyhow!("transient embed failure"))
                    } else {
                        Ok(vec![3.5; 2])
                    }
                })
            }));
        }
        let results: Vec<Result<Vec<f32>>> = handles
            .into_iter()
            .map(|h| h.join().expect("joiner thread must not panic"))
            .collect();
        assert!(
            results.iter().all(|r| r.is_err()),
            "all four callers observe the failure, none hang: {results:?}"
        );
        assert_eq!(cache.len(), 0, "errors must not be cached");

        // The inflight slot was freed: the next caller retries and succeeds.
        let retry = cache
            .get_or_embed("transient failure query", |_q| {
                attempts.fetch_add(1, Ordering::SeqCst);
                Ok(vec![3.5; 2])
            })
            .unwrap();
        assert_eq!(retry, vec![3.5; 2]);
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            2,
            "exactly one retry after the transient failure"
        );
    }
}
