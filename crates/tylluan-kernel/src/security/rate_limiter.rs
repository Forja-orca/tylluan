//! RateLimiter — Per-session sliding window rate limiting.
//!
//! Implements a 60-second sliding window that tracks tool call timestamps.
//! Default limit: 60 calls per minute per session (configurable via env).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const DEFAULT_WINDOW: Duration = Duration::from_secs(60);
const DEFAULT_MAX_CALLS: u32 = 60;

pub struct RateLimiter {
    windows: Mutex<HashMap<String, Vec<Instant>>>,
    max_calls: u32,
    window_duration: Duration,
}

impl RateLimiter {
    pub fn new(max_calls: Option<u32>) -> Self {
        Self {
            windows: Mutex::new(HashMap::new()),
            max_calls: max_calls.unwrap_or(DEFAULT_MAX_CALLS),
            window_duration: DEFAULT_WINDOW,
        }
    }

    /// Check if a session is within rate limits. If allowed, records the call.
    /// Returns Ok(()) if allowed, Err(reason) if blocked.
    pub fn check_and_record(&self, session_key: &str) -> Result<(), String> {
        let mut windows = self.windows.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();

        let timestamps = windows.entry(session_key.to_string()).or_default();

        // Slide window: remove timestamps older than the window
        timestamps.retain(|t| now.duration_since(*t) < self.window_duration);

        if timestamps.len() as u32 >= self.max_calls {
            return Err(format!(
                "Rate limit exceeded: max {} calls per minute. Wait and retry.",
                self.max_calls
            ));
        }

        timestamps.push(now);

        // Prune empty session keys to prevent memory leak on long uptime
        if windows.len() > 100 {
            windows.retain(|_, v| !v.is_empty());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allows_within_limit() {
        let limiter = RateLimiter::new(Some(5));
        for _ in 0..5 {
            assert!(limiter.check_and_record("test-session").is_ok());
        }
    }

    #[test]
    fn test_blocks_over_limit() {
        let limiter = RateLimiter::new(Some(3));
        for _ in 0..3 {
            limiter.check_and_record("test-session").unwrap();
        }
        let result = limiter.check_and_record("test-session");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Rate limit exceeded"));
    }

    #[test]
    fn test_separate_sessions() {
        let limiter = RateLimiter::new(Some(2));
        limiter.check_and_record("session-a").unwrap();
        limiter.check_and_record("session-a").unwrap();
        // session-a is now at limit
        assert!(limiter.check_and_record("session-a").is_err());
        // session-b should still work
        assert!(limiter.check_and_record("session-b").is_ok());
    }
}
