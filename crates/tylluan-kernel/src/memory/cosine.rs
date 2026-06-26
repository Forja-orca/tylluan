//! # Cosine Similarity
//!
//! Single source of truth for vector similarity, used by HybridMemory and SilvaDB.
//! Ported from `TylluanMCP/src/utils/cosine.ts`.

use ndarray::ArrayView1;

/// Compute cosine similarity between two equal-length f32 vectors.
///
/// Returns 0.0 if either vector has zero magnitude.
/// Returns value in [-1.0, 1.0] range.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len(), "vectors must be same length");

    let av = ArrayView1::from(a);
    let bv = ArrayView1::from(b);

    let dot = av.dot(&bv);
    let norm_a_sq = av.dot(&av);
    let norm_b_sq = bv.dot(&bv);

    if norm_a_sq == 0.0 || norm_b_sq == 0.0 {
        return 0.0;
    }

    dot / (norm_a_sq.sqrt() * norm_b_sq.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Output is always in [-1.0, 1.0]
        #[test]
        fn prop_cosine_bounded(
            a in prop::collection::vec(-1e3f32..1e3f32, 1..64),
            b in prop::collection::vec(-1e3f32..1e3f32, 1..64),
        ) {
            let len = a.len().min(b.len());
            let sim = cosine_similarity(&a[..len], &b[..len]);
            prop_assert!(sim >= -1.0 - 1e-5 && sim <= 1.0 + 1e-5,
                "cosine out of range: {}", sim);
        }

        /// sim(a, a) == 1.0 for any non-zero vector
        #[test]
        fn prop_cosine_self_is_one(
            a in prop::collection::vec(0.01f32..1e3f32, 1..64),
        ) {
            let sim = cosine_similarity(&a, &a);
            prop_assert!((sim - 1.0).abs() < 1e-5, "self-similarity != 1: {}", sim);
        }

        /// sim(a, b) == sim(b, a)
        #[test]
        fn prop_cosine_symmetric(
            a in prop::collection::vec(-1e3f32..1e3f32, 1..64),
            b in prop::collection::vec(-1e3f32..1e3f32, 1..64),
        ) {
            let len = a.len().min(b.len());
            let ab = cosine_similarity(&a[..len], &b[..len]);
            let ba = cosine_similarity(&b[..len], &a[..len]);
            prop_assert!((ab - ba).abs() < 1e-5, "not symmetric: {} vs {}", ab, ba);
        }

        /// Zero vector always returns 0.0
        #[test]
        fn prop_cosine_zero_vector(
            b in prop::collection::vec(-1e3f32..1e3f32, 1..64),
        ) {
            let zero = vec![0.0f32; b.len()];
            prop_assert_eq!(cosine_similarity(&zero, &b), 0.0);
            prop_assert_eq!(cosine_similarity(&b, &zero), 0.0);
        }
    }

    #[test]
    fn test_identical_vectors() {
        let v = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_orthogonal_vectors() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_opposite_vectors() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_zero_vector() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 2.0, 3.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_similar_vectors() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.1, 2.1, 2.9];
        let sim = cosine_similarity(&a, &b);
        assert!(sim > 0.99); // Very similar
    }
}
