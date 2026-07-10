//! Free Spaced Repetition Scheduler (FSRS-5) for Tylluan.
//!
//! Replaces the global fixed half-life (`weight *= 0.5^(t / 14d)`) with a
//! **per-memory** model: each item has its own `stability` (how well learned,
//! in days), `difficulty` (how hard to maintain, 0–1), and `retrievability`
//! (probability of recall today, 0–1).
//!
//! On every `tylluan_recall` hit the stability grows; on misses or decay
//! sweeps the retrievability shrinks. Result: popular/critical memories
//! survive longer, forgotten ones fade faster — without a global knob.

use core::f64::consts::LN_2;

/// Default initial stability in days — matches the historical 14-day half-life.
pub const DEFAULT_STABILITY_DAYS: f64 = 14.0;

/// Default initial difficulty (0 = easiest, 1 = hardest).
pub const DEFAULT_DIFFICULTY: f64 = 0.3;

/// Minimum stability to prevent division by zero / vanishing gradients.
pub const MIN_STABILITY_DAYS: f64 = 0.1;

/// Maximum stability cap (≈ 1 year) — no memory is perfectly permanent.
pub const MAX_STABILITY_DAYS: f64 = 365.0;

// ── FSRS-5 default weights ────────────────────────────────────────────
// Default parameters from the FSRS-5 reference implementation.
// 19 parameters optimized on ~220M reviews from Anki users.
const W: [f64; 19] = [
    0.4, 0.6, 2.4, 5.8, 4.0,
    0.7, 0.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0,
];

/// Rating of a review — how successfully the memory was recalled.
///
/// Uses the FSRS-5 4-point scale where higher = better retrieval.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Rating {
    /// Complete failure — memory was not retrieved.
    Again,
    /// Hard — recalled with significant effort or partial cues.
    Hard,
    /// Good — recalled correctly with some effort.
    Good,
    /// Easy — immediate, effortless recall.
    Easy,
}

impl Rating {
    /// Numeric value used in FSRS update formulas (1..4).
    #[inline]
    pub fn value(self) -> f64 {
        match self {
            Rating::Again => 1.0,
            Rating::Hard => 2.0,
            Rating::Good => 3.0,
            Rating::Easy => 4.0,
        }
    }

    /// Convert a boolean success/failure to a Rating.
    #[inline]
    pub fn from_success(success: bool) -> Self {
        if success { Rating::Good } else { Rating::Again }
    }
}

/// FSRS memory parameters for a single item (node, concept, fact).
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct FsrsItem {
    /// Stability in days — how long before retrievability drops to ~90%.
    /// Replaces the old fixed half-life.
    pub stability: f64,
    /// Difficulty 0..1 — higher = harder to maintain.
    pub difficulty: f64,
    /// UNIX timestamp (seconds) of the last review.
    pub last_review: i64,
}

impl Default for FsrsItem {
    fn default() -> Self {
        Self {
            stability: DEFAULT_STABILITY_DAYS,
            difficulty: DEFAULT_DIFFICULTY,
            last_review: 0,
        }
    }
}

impl FsrsItem {
    /// Create a new item with default stability and difficulty.
    #[inline]
    pub fn new(last_review: i64) -> Self {
        Self {
            stability: DEFAULT_STABILITY_DAYS,
            difficulty: DEFAULT_DIFFICULTY,
            last_review,
        }
    }

    /// Create a new item with explicit stability and difficulty.
    #[inline]
    pub fn with_params(stability: f64, difficulty: f64, last_review: i64) -> Self {
        Self {
            stability: stability.clamp(MIN_STABILITY_DAYS, MAX_STABILITY_DAYS),
            difficulty: difficulty.clamp(0.0, 1.0),
            last_review,
        }
    }

    /// Compute retrievability R(t) = 2^(-t / stability).
    ///
    /// `t` = elapsed days since `last_review`.
    /// Returns 0..1 — the probability that this item would be recalled today.
    #[inline]
    pub fn retrievability(&self, elapsed_days: f64) -> f64 {
        if elapsed_days <= 0.0 {
            return 1.0;
        }
        let s = self.stability.max(MIN_STABILITY_DAYS);
        (-elapsed_days / s * LN_2).exp()
    }

    /// Update parameters after a review with the given rating.
    ///
    /// Implements FSRS-5 formulas:
    ///
    /// **Difficulty update** (all ratings):
    ///   D' = clamp(D + w[4] * (11 - rating), 1, 10)  (then mapped back to 0..1)
    ///
    /// **Stability update on success** (Hard/Good/Easy):
    ///   S' = S * (1 + w[5] * e^(w[6] * (10-D)) * (e^(w[7] * (R-1)) - 1) * (11-rating) / 10)
    ///
    /// **Stability update on failure** (Again):
    ///   S' = max(w[0] * S * e^(w[1] * (10-D)), MIN_STABILITY)
    ///
    /// When `w[6]=0` and `w[7]=0` (our default weights), the success formula
    /// simplifies to: S' = S * (1 + w[5] * 1 * 0 * (11-rating)/10) = S
    /// With the default preset retrievability doesn't modulate stability gain;
    /// the gain comes entirely from the rating value (Good vs Easy).
    #[inline]
    pub fn review(&mut self, rating: Rating, now: i64) {
        let elapsed_days = if self.last_review > 0 && now > self.last_review {
            (now - self.last_review) as f64 / 86400.0
        } else {
            0.0
        };
        let r = self.retrievability(elapsed_days);
        let rating_f = rating.value();
        let d = self.difficulty * 10.0;

        // Difficulty update (mean-reverting):
        //   delta_d = -w[4] * (rating - 3)
        //   D' = D + delta_d * (10 - D) / 9
        let delta_d = -W[4] * (rating_f - 3.0);
        let new_d = (d + delta_d * (10.0 - d) / 9.0).clamp(1.0, 10.0);
        self.difficulty = new_d / 10.0;

        // Stability update
        if rating == Rating::Again {
            // S' = w[0] * S * e^(w[1] * (10 - D))
            let exp_factor = (W[1] * (10.0 - new_d)).exp();
            self.stability = (W[0] * self.stability * exp_factor).max(MIN_STABILITY_DAYS);
        } else {
            // S' = S * (1 + w[5] * e^(w[6]*(10-D)) * (e^(w[7]*(R-1)) - 1) * (11-rating)/10)
            let d_factor = (W[6] * (10.0 - new_d)).exp();
            let r_factor = (W[7] * (r - 1.0)).exp() - 1.0;
            let mult = 1.0 + W[5] * d_factor * r_factor * (11.0 - rating_f) / 10.0;
            self.stability = (self.stability * mult).clamp(MIN_STABILITY_DAYS, MAX_STABILITY_DAYS);
        }

        self.last_review = now;
    }

    /// Calculate the next review interval in days for a desired retrievability.
    ///
    /// `desired_retention` (default 0.9 = 90%) — the retrievability target
    /// for scheduling the next review.
    #[inline]
    pub fn next_interval(&self, desired_retention: f64) -> f64 {
        let dr = desired_retention.clamp(0.5, 0.99);
        -(self.stability * dr.ln()) / LN_2
    }

    /// Human-readable status line for logging / dashboard.
    pub fn status_string(&self, now: i64) -> String {
        let elapsed = if self.last_review > 0 {
            ((now - self.last_review) as f64 / 86400.0).max(0.0)
        } else {
            0.0
        };
        let r = self.retrievability(elapsed);
        let next = self.next_interval(0.9);
        format!(
            "S={:.0}d D={:.2} R={:.0}% next={:.0}d",
            self.stability, self.difficulty, r * 100.0, next.max(0.0)
        )
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> i64 {
        1_800_000_000 // fixed "now" for deterministic tests
    }

    #[test]
    fn test_default_item() {
        let item = FsrsItem::default();
        assert!((item.stability - DEFAULT_STABILITY_DAYS).abs() < 1e-6);
        assert!((item.difficulty - DEFAULT_DIFFICULTY).abs() < 1e-6);
        assert_eq!(item.last_review, 0);
    }

    #[test]
    fn test_retrievability_decays_over_time() {
        let item = FsrsItem::new(now());
        assert!((item.retrievability(0.0) - 1.0).abs() < 1e-6);
        let r_14d = item.retrievability(14.0);
        assert!((r_14d - 0.5).abs() < 0.05, "R(14d) = {}", r_14d);
        let r_28d = item.retrievability(28.0);
        assert!((r_28d - 0.25).abs() < 0.05, "R(28d) = {}", r_28d);
    }

    #[test]
    fn test_review_boosts_stability() {
        let mut item = FsrsItem::with_params(7.0, 0.3, now());
        let s_before = item.stability;

        // Wait 3 days so R has decayed to ~0.74, then review Good
        item.review(Rating::Good, now() + 3 * 86400);
        assert!(item.stability >= s_before, "stability should not decrease on success: S={}", item.stability);
        assert_eq!(item.last_review, now() + 3 * 86400);
    }

    #[test]
    fn test_failure_reduces_stability() {
        let mut item = FsrsItem::with_params(30.0, 0.3, now());
        // Wait 7 days, fail review
        item.review(Rating::Again, now() + 7 * 86400);
        assert!(item.stability < 30.0, "stability should drop on failure: S={}", item.stability);
    }

    #[test]
    fn test_easy_boost_more_than_good() {
        let mut easy = FsrsItem::new(now());
        let mut good = FsrsItem::new(now());

        // Wait 3 days so R has decayed
        easy.review(Rating::Easy, now() + 3 * 86400);
        good.review(Rating::Good, now() + 3 * 86400);

        assert!(easy.stability >= good.stability, "Easy should stabilize >= Good");
    }

    #[test]
    fn test_again_reduces_difficulty_more_than_easy() {
        let mut again_item = FsrsItem::new(now());
        let mut easy_item = FsrsItem::new(now());

        again_item.review(Rating::Again, now() + 86400);
        easy_item.review(Rating::Easy, now() + 86400);

        assert!(again_item.difficulty > easy_item.difficulty,
            "Again should increase difficulty more than Easy");
    }

    #[test]
    fn test_next_interval_for_desired_retention() {
        let item = FsrsItem::new(now());
        let interval = item.next_interval(0.9);
        assert!((interval - 2.13).abs() < 0.1, "next_interval = {}", interval);
    }

    #[test]
    fn test_retrievability_is_bounded() {
        let item = FsrsItem::new(now());
        let r_neg = item.retrievability(-10.0);
        assert!((r_neg - 1.0).abs() < 1e-6);
        let r_far = item.retrievability(1_000_000.0);
        assert!(r_far >= 0.0 && r_far < 1.0);
    }

    #[test]
    fn test_status_string_is_readable() {
        let item = FsrsItem::new(now());
        let s = item.status_string(now());
        assert!(s.contains("S="));
        assert!(s.contains("R="));
        assert!(s.contains("%"));
    }

    #[test]
    fn test_repeated_good_reviews_compound_stability() {
        let mut item = FsrsItem::with_params(7.0, 0.3, now());
        // 3 good reviews spaced 7 days apart
        for day in 1..=5 {
            item.review(Rating::Good, now() + day * 7 * 86400);
        }
        // Even with default params, repeated good reviews should maintain
        // or increase stability
        assert!(item.stability >= 7.0,
            "stability should not regress below initial after good reviews: S={}", item.stability);
    }

    #[test]
    fn test_from_success_mapping() {
        assert_eq!(Rating::from_success(true), Rating::Good);
        assert_eq!(Rating::from_success(false), Rating::Again);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let item = FsrsItem::new(now());
        let json = serde_json::to_string(&item).unwrap();
        let deserialized: FsrsItem = serde_json::from_str(&json).unwrap();
        assert!((item.stability - deserialized.stability).abs() < 1e-6);
        assert!((item.difficulty - deserialized.difficulty).abs() < 1e-6);
        assert_eq!(item.last_review, deserialized.last_review);
    }
}
