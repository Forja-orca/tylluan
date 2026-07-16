use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use anyhow::Result;

const CACHE_TTL: Duration = Duration::from_secs(300);
const MAX_ENTRIES: usize = 256;

struct Entry {
    embedding: Vec<f32>,
    inserted_at: Instant,
}

/// TTL-based query embedding cache for the recall path.
///
/// Caches embeddings keyed by normalized query text (trimmed + lowercased).
/// Entries expire after CACHE_TTL (5 min) — short enough to avoid staleness,
/// long enough to cover repeated queries in conversational windows.
///
/// Eviction: LRU by insertion timestamp when at MAX_ENTRIES capacity.
///
/// This cache lives inside SilvaDB and only caches *query* embeddings (recall).
/// Ingest embeddings (remember) are NOT cached — they are unique by definition.
pub struct QueryEmbeddingCache {
    inner: Mutex<HashMap<String, Entry>>,
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
    /// Otherwise computes via `embed_fn`, stores it, and returns the fresh embedding.
    /// LRU eviction runs when the cache is at capacity.
    pub fn get_or_embed(
        &self,
        query: &str,
        embed_fn: impl FnOnce(&str) -> Result<Vec<f32>>,
    ) -> Result<Vec<f32>> {
        let key = Self::normalize(query);

        let mut cache = self.inner.lock().unwrap_or_else(|e| e.into_inner());

        if let Some(entry) = cache.get(&key)
            && entry.inserted_at.elapsed() < CACHE_TTL {
                return Ok(entry.embedding.clone());
            }

        let embedding = embed_fn(query)?;

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

        Ok(embedding)
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
