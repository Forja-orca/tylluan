//! Cognitive Scheduler.
//!
//! See `docs/architecture/DESIGN_cognitive_scheduler.md` for the full design.
//! - `types` (§4): canonical data types. Closed — do not extend casually.
//! - `decision` (§5): the pure decision matrix. Closed — tested truth table.
//! - `observe` (Phase 3): observation-mode wiring into `tylluan_do` dispatch.
//!   Logs the Scheduler's verdict per dispatch and never influences routing
//!   — exactly the CoherenceGate Layer 4 pattern. Phase 4 (acting on the
//!   verdict) is deliberately NOT here yet.
//! - `confusion` (WS3, observation-only half): pairs each completed
//!   dispatch's Scheduler verdict with what the cascade actually did,
//!   classifies agree/differ, and accumulates tallies in SQLite for the
//!   eventual cutover decision (which requires Tech Lead sign-off and is
//!   NOT implemented here — nothing reads the store back into dispatch).

pub mod types;
pub mod decision;
pub mod observe;
pub mod confusion;
