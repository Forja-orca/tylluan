//! # TylluanNexus Kernel Library
//!
//! Shared logic, types, and AI engines for the TylluanNexus ecosystem.

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
