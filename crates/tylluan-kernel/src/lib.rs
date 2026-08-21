//! # TylluanNexus Kernel Library
//!
//! Shared logic, types, and AI engines for the TylluanNexus ecosystem.

// CI's toolchain (rustc 1.98) ships a newer clippy than local dev environments,
// introducing `chunks_exact_to_as_chunks` as a style suggestion (slice::as_chunks
// is a recent stabilization). It fires on ~15 pre-existing f32-from-bytes decode
// sites across memory/silva/*.rs and memory/auto_link.rs, none of which are
// correctness issues — chunks_exact(4) is still valid, just no longer clippy's
// preferred idiom. Allowed at the crate level rather than rewritten piecemeal to
// unblock CI without touching working decode logic; revisit if/when the local
// toolchain catches up and the idiom migration is done deliberately.
#![allow(unknown_lints, clippy::chunks_exact_to_as_chunks)]

pub mod config;
pub mod consensus;
pub mod security;
pub mod transport;
pub mod registry;
pub mod memory;
pub mod router;
pub mod doctor;
pub mod maintenance;
pub mod curriculum;
pub mod guard;
pub mod hormones;
pub mod tunnel;
pub mod metrics_ring;
#[cfg(feature = "observability")]
pub mod metrics_exporter;
pub mod federation;
pub mod repo_map;
pub mod process_isolation;
pub mod eval;
pub mod mlp;
