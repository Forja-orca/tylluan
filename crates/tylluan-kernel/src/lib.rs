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
/// D4983: `cleanup` vive en la libreria para que su test corra en `--lib`
/// (antes era `mod cleanup;` del binario y su test no entraba en la suite).
pub mod cleanup;
pub mod workspace;
pub mod logging;
pub mod recall_telemetry;
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
pub mod runtime;
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
pub mod models;
pub mod version_check;

#[cfg(test)]
pub(crate) mod test_support;

/// D4984 (H-HER-S-01 de Hermes): **UNA** cerradura para los tests que mutan
/// variables de entorno. Antes cada modulo declaraba su propio `static
/// ENV_LOCK`, asi que las cerraduras no se excluian entre si y dos tests podian
/// pisarse `TYLLUAN_DATA_DIR` en `--lib` (que corre en paralelo). Con `dev_mode`
/// y `DATA_DIR` moviendose bajo los pies, eso es un fallo intermitente que
/// aparece en CI y no en tu maquina.
#[cfg(test)]
pub(crate) mod test_env {
    use std::sync::{Mutex, MutexGuard, OnceLock};

    fn lock() -> &'static Mutex<()> {
        static L: OnceLock<Mutex<()>> = OnceLock::new();
        L.get_or_init(|| Mutex::new(()))
    }

    /// Toma la cerradura global de entorno. Un test que la tenga NO debe
    /// esperar otra: manten el bloque corto.
    pub(crate) fn serial() -> MutexGuard<'static, ()> {
        lock().lock().unwrap_or_else(|p| p.into_inner())
    }
}
