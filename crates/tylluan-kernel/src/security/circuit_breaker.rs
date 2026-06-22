//! CircuitBreaker — Error cascade prevention state machine.
//!
//! States: CLOSED → OPEN → HALF_OPEN → CLOSED
//!
//! - CLOSED: Normal operation. Errors are counted.
//! - OPEN: Too many consecutive errors (threshold=3). All calls are rejected.
//! - HALF_OPEN: After cooldown (30s), one probe call is allowed.
//!   - If it succeeds → back to CLOSED.
//!   - If it fails → back to OPEN with fresh cooldown.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const ERROR_THRESHOLD: u32 = 3;
const COOLDOWN: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq)]
enum State {
    Closed,
    Open { since: Instant },
    HalfOpen,
}

struct SessionBreaker {
    state: State,
    consecutive_errors: u32,
}

pub struct CircuitBreaker {
    breakers: Mutex<HashMap<String, SessionBreaker>>,
}

#[derive(Debug)]
pub struct BreakerResult {
    pub open: bool,
    pub reason: Option<String>,
}

impl CircuitBreaker {
    pub fn new() -> Self {
        Self {
            breakers: Mutex::new(HashMap::new()),
        }
    }

    /// Check if a session's circuit is open.
    /// Returns Ok if the call should proceed, Err if blocked.
    pub fn check(&self, session_key: &str) -> BreakerResult {
        let mut breakers = self.breakers.lock().unwrap_or_else(|e| e.into_inner());
        let breaker = breakers
            .entry(session_key.to_string())
            .or_insert(SessionBreaker {
                state: State::Closed,
                consecutive_errors: 0,
            });

        match &breaker.state {
            State::Closed => BreakerResult { open: false, reason: None },

            State::Open { since } => {
                let elapsed = since.elapsed();
                if elapsed >= COOLDOWN {
                    breaker.state = State::HalfOpen;
                    BreakerResult { open: false, reason: None }
                } else {
                    let remaining = COOLDOWN - elapsed;
                    BreakerResult {
                        open: true,
                        reason: Some(format!(
                            "Circuit breaker OPEN. {} consecutive errors. Retry in {:.0}s.",
                            breaker.consecutive_errors,
                            remaining.as_secs_f32()
                        )),
                    }
                }
            }

            State::HalfOpen => {
                // Already in probe mode — allow the call
                BreakerResult { open: false, reason: None }
            }
        }
    }

    /// Record a successful tool call. Resets the breaker.
    pub fn record_success(&self, session_key: &str) {
        let mut breakers = self.breakers.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(breaker) = breakers.get_mut(session_key) {
            breaker.state = State::Closed;
            breaker.consecutive_errors = 0;
        }
    }

    /// Record a failed tool call. May trip the breaker.
    pub fn record_error(&self, session_key: &str) {
        let mut breakers = self.breakers.lock().unwrap_or_else(|e| e.into_inner());
        let breaker = breakers
            .entry(session_key.to_string())
            .or_insert(SessionBreaker {
                state: State::Closed,
                consecutive_errors: 0,
            });

        breaker.consecutive_errors += 1;

        if breaker.consecutive_errors >= ERROR_THRESHOLD {
            breaker.state = State::Open { since: Instant::now() };
        }
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_closed_by_default() {
        let cb = CircuitBreaker::new();
        let result = cb.check("s1");
        assert!(!result.open);
    }

    #[test]
    fn test_opens_after_threshold() {
        let cb = CircuitBreaker::new();
        cb.record_error("s1");
        cb.record_error("s1");
        assert!(!cb.check("s1").open); // 2 errors, still closed

        cb.record_error("s1"); // 3rd error → trips
        assert!(cb.check("s1").open);
    }

    #[test]
    fn test_success_resets() {
        let cb = CircuitBreaker::new();
        cb.record_error("s1");
        cb.record_error("s1");
        cb.record_success("s1"); // Reset before threshold
        cb.record_error("s1");
        assert!(!cb.check("s1").open); // Only 1 error since reset
    }

    #[test]
    fn test_separate_sessions_isolated() {
        let cb = CircuitBreaker::new();
        for _ in 0..3 {
            cb.record_error("bad-session");
        }
        assert!(cb.check("bad-session").open);
        assert!(!cb.check("good-session").open);
    }
}
