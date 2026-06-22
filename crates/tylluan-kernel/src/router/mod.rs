//! # Semantic Router
//!
//! Routes natural language queries to the correct guild using either:
//! - **Semantic matching** (with `--features semantic`): ONNX embeddings + cosine similarity
//! - **Keyword matching** (default, no ONNX): substring matching on guild descriptions
//!
//! The router maintains a static catalog of guild descriptors with pre-computed
//! description embeddings (when semantic feature is enabled).

pub mod catalog;
pub mod matcher;

pub mod embeddings;
