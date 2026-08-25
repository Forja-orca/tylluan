//! # Recall Contract — dirección #4 (memoria explicable)
//!
//! Single source of truth for the policies that were previously scattered
//! across RecallCache, QueryEmbeddingCache, search_hybrid_for_recall, the
//! lexical cascade and the handler-side reranking. Divergences between these
//! paths were a real risk (one route including archived nodes, another not).
//!
//! Any new recall path MUST reference these constants instead of hardcoding
//! its own policy values. Divergence points audited 2026-08-25:
//! - search_hybrid_for_recall: archived excluded unless include_archived=true
//! - cascade stage-1/stage-2: same policy via apply_recall_filters
//! - RecallCache: keyed by query text, TTL below
//! - QueryEmbeddingCache: LRU 256, TTL below

/// Default policy for archived nodes in agent-facing recall: EXCLUDED.
/// Opt-in via explicit include_archived=true.
pub const INCLUDE_ARCHIVED_DEFAULT: bool = false;

/// RecallCache entry freshness (Jaccard LRU, handler_recall).
pub const RECALL_CACHE_TTL_SECS: u64 = 300;

/// QueryEmbeddingCache entry freshness (SilvaDB, dense query vectors).
pub const QUERY_EMBED_TTL_SECS: u64 = 300;

/// Candidate pool multiplier for broad first-stage gathering.
pub const CANDIDATE_POOL_MULT_BASE: usize = 3;

#[cfg(test)]
mod tests {
    use super::*;

    // Compile-time invariants (clippy::assertions_on_constants-clean): pool
    // multiplier below 1 would make first-stage gathering smaller than limit;
    // archived exclusion must be opt-in by construction.
    const _: () = assert!(CANDIDATE_POOL_MULT_BASE >= 1);
    const _: () = assert!(!INCLUDE_ARCHIVED_DEFAULT);

    #[test]
    fn contract_constants_are_coherent() {
        assert_eq!(
            RECALL_CACHE_TTL_SECS, QUERY_EMBED_TTL_SECS,
            "a cache hit must never pair fresh candidates with stale embeddings"
        );
    }
}
