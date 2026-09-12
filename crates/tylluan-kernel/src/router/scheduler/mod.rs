//! Cognitive Scheduler.
//!
//! See `docs/architecture/DESIGN_cognitive_scheduler.md` for the full design.
//! - `types` (§4): canonical data types. Closed — do not extend casually.
//! - `decision` (§5): the pure decision matrix. Closed — tested truth table.
//! - `observe` (Phase 3): observation-mode wiring into `tylluan_do` dispatch.
//!   Logs the Scheduler's verdict per dispatch and never influences routing
//!   — exactly the CoherenceGate Layer 4 pattern. Phase 4 (acting on the
//!   verdict) is deliberately NOT here yet.

pub mod types;
pub mod decision;
pub mod observe;
