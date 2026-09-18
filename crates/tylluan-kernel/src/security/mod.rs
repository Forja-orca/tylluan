//! # Security Module
//!
//! Implements the three security primitives from TylluanMCP v2, rewritten in Rust:
//! 1. **ExecutionGuard** â€” Channel-based tool access gating
//! 2. **RateLimiter** â€” Sliding-window per-session rate limiting
//! 3. **CircuitBreaker** â€” Error cascade prevention state machine

pub mod guard;
pub mod rate_limiter;
pub mod circuit_breaker;
pub mod grants;
pub mod hooks;
pub mod coherence_gate;
pub mod poison_patterns;
pub mod agents_contract;
pub mod dispatch_subscriber;
pub mod friction_log;
pub mod llm_examples;
pub mod write_gate;

#[cfg(test)]
mod integration_tests;
