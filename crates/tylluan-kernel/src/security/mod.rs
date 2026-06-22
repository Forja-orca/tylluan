//! # Security Module
//!
//! Implements the three security primitives from TylluanMCP v2, rewritten in Rust:
//! 1. **ExecutionGuard** — Channel-based tool access gating
//! 2. **RateLimiter** — Sliding-window per-session rate limiting
//! 3. **CircuitBreaker** — Error cascade prevention state machine

pub mod guard;
pub mod rate_limiter;
pub mod circuit_breaker;
