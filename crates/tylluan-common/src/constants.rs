//! # Sovereign Constants
//!
//! Centralized repository of the "Laws" that power TylluanNexus o3.
//! Shared across kernel, common, and tests.

/// The universal key for the Master Token Tunnel.
/// Access to dangerous tools (bash, filesystem) requires this token.
// Security: MASTER_TOKEN removed from source code. 
// Use TYLLUAN_TOKEN environment variable for authenticated tool access.

/// Default directory for sovereign AI models (BGE-M3, Moondream).
pub const DEFAULT_MODEL_DIR: &str = "models";

/// The specific path to the BGE-M3 brain weights.
pub const BGE_M3_DIR: &str = "models/bge-m3";
