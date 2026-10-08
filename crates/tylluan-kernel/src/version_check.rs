//! Startup version check for out-of-code dependencies (T999).
//!
//! Checks ONE dependency today: the pinned llama.cpp `llama-server` binary.
//! Compares the version marker written at install time by
//! `guilds/core/llama_backend.py::_install_llama_server()` against the
//! latest GitHub release tag of llama.cpp, and posts a Coloquio notification
//! when they differ. NEVER auto-downloads or auto-updates (pin updates are
//! a human decision — same philosophy as `setup_hint_handler` confirm=false).
//!
//! Design approved by the TL (Fase 1, Coloquio 2026-10-08):
//! - Fire-and-forget `tokio::spawn` after the HTTP listener is bound; a
//!   network check must never delay boot.
//! - Timeout 10s, silent on any failure (no network / rate-limit / parse
//!   errors are debug-level logs, never warnings that pollute the boot log).
//! - Missing marker = honest unknown ("pre-marker install"), no notification.
//! - Notification goes through the in-process `ColoquioDb` directly (no
//!   loopback HTTP round-trip; the kernel owns the DB).
//!
//! Explicitly OUT of scope (Fase 1 decision): crates (cargo audit owns CI),
//! pip packages (pip owns its ecosystem; importability is doctor's job),
//! ONNX/GGUF model revisions (comparing against HF "latest" risks re-embed
//! storms — the engine fingerprint v1 in router/embeddings.rs deliberately
//! pins content, not freshness).

use tracing::{debug, info, warn};

/// GitHub API endpoint for the latest llama.cpp release.
pub const LLAMA_RELEASES_URL: &str =
    "https://api.github.com/repos/ggml-org/llama.cpp/releases/latest";

/// Marker file written next to the installed llama-server binary.
pub const LLAMA_MARKER_FILE: &str = ".llama-server-version";

/// Cache dir where llama_backend.py installs the binary.
pub fn llama_cache_dir() -> Option<std::path::PathBuf> {
    std::env::var("TYLLUAN_LLAMA_CACHE_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".cache").join("tylluan").join("llama-cpp")))
}

/// Read the installed version marker. `None` = pre-marker install or no
/// install at all (honest unknown — same spirit as fingerprint `None` in
/// router/embeddings.rs).
pub fn read_installed_version() -> Option<String> {
    let path = llama_cache_dir()?.join(LLAMA_MARKER_FILE);
    let raw = std::fs::read_to_string(&path).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Extract `tag_name` from the GitHub releases/latest JSON.
pub fn parse_latest_tag(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    v.get("tag_name")?.as_str().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Pure decision logic, unit-testable without network or filesystem.
/// Returns Some(notification_text) when the check has something to say.
pub fn decide_notification(
    installed: Option<&str>,
    latest: Option<&str>,
) -> Option<String> {
    let (Some(installed), Some(latest)) = (installed, latest) else {
        // Unknown installed (pre-marker) or unknown latest (no network):
        // nothing to say, stay silent.
        return None;
    };
    if installed == latest {
        return None;
    }
    Some(format!(
        "[kernel] Check de versiones (T999): llama.cpp tiene un release más nuevo que el pin \
         de llama_backend.py — instalado={installed}, latest={latest}. \
         El binario no se toca automáticamente: actualizar el pin es decisión humana \
         (editar guilds/core/llama_backend.py y borrar ~/.cache/tylluan/llama-cpp para forzar re-descarga)."
    ))
}

/// Post the notification into #general as the kernel itself.
/// The DB call is in-process and must not fail the task even if it errors.
pub async fn notify_coloquio(coloquio: &crate::memory::coloquio::ColoquioDb, content: &str) {
    match coloquio
        .post_message("general", "tylluan-kernel", "agent", content, "{}")
        .await
    {
        Ok(_) => info!("✅ [version_check] Notificación de llama.cpp publicada en Coloquio #general"),
        Err(e) => warn!("⚠️ [version_check] No se pudo publicar en Coloquio: {e}"),
    }
}

/// Run the check once. Fire-and-forget: callers `tokio::spawn` this.
/// Any error path is silent (debug log) by design — a version check must
/// never turn into a boot problem.
pub async fn run_version_check(coloquio: std::sync::Arc<crate::memory::coloquio::ColoquioDb>) {
    let installed = read_installed_version();
    if installed.is_none() {
        debug!("[version_check] llama-server sin marker de versión (pre-marker o no instalado) — no notifico");
    }

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            debug!("[version_check] reqwest build error (silencioso): {e}");
            return;
        }
    };

    let latest = match tokio::time::timeout(
        std::time::Duration::from_secs(10),
        client
            .get(LLAMA_RELEASES_URL)
            .header("User-Agent", "tylluan-kernel")
            .header("Accept", "application/vnd.github+json")
            .send(),
    )
    .await
    {
        Ok(Ok(resp)) if resp.status().is_success() => match resp.text().await {
            Ok(body) => parse_latest_tag(&body),
            Err(_) => None,
        },
        Ok(Ok(resp)) => {
            debug!("[version_check] GitHub API status {} (silencioso)", resp.status());
            None
        },
        Ok(Err(e)) => {
            debug!("[version_check] Sin acceso a GitHub API (silencioso): {e}");
            None
        }
        Err(_) => {
            debug!("[version_check] Timeout consultando GitHub API (silencioso)");
            None
        }
    };

    if let Some(text) = decide_notification(installed.as_deref(), latest.as_deref()) {
        notify_coloquio(&coloquio, &text).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_JSON: &str = r#"{"url":"https://api.github.com/repos/ggml-org/llama.cpp/releases/1","tag_name":"b10234","name":"b10234"}"#;

    #[test]
    fn parse_latest_tag_extracts_tag() {
        assert_eq!(parse_latest_tag(SAMPLE_JSON).as_deref(), Some("b10234"));
    }

    #[test]
    fn parse_latest_tag_rejects_garbage_and_empty() {
        assert_eq!(parse_latest_tag("not json"), None);
        assert_eq!(parse_latest_tag(r#"{"tag_name":""}"#), None);
        assert_eq!(parse_latest_tag(r#"{"other":1}"#), None);
    }

    #[test]
    fn decide_same_version_is_silent() {
        assert_eq!(decide_notification(Some("b10158"), Some("b10158")), None);
    }

    #[test]
    fn decide_unknown_inputs_are_silent() {
        // Pre-marker install, latest known: silent (honest unknown).
        assert_eq!(decide_notification(None, Some("b10234")), None);
        // No network (latest unknown), installed known: silent.
        assert_eq!(decide_notification(Some("b10158"), None), None);
        assert_eq!(decide_notification(None, None), None);
    }

    #[test]
    fn decide_differs_notifies_with_both_tags() {
        let msg = decide_notification(Some("b10158"), Some("b10234")).expect("should notify");
        assert!(msg.contains("b10158"), "must include installed tag");
        assert!(msg.contains("b10234"), "must include latest tag");
        assert!(msg.contains("llama_backend.py"), "must point at the pin site");
    }

    #[test]
    fn marker_roundtrip_then_missing_is_honest_unknown() {
        // TYLLUAN_LLAMA_CACHE_DIR override keeps the test hermetic (no $HOME writes).
        // Single test (not two) because env vars are process-global and the Rust
        // test harness runs tests in parallel, so two tests mutating the same
        // var would race. set_var/remove_var are unsafe in edition 2024.
        let tmp = std::env::temp_dir().join(format!("tylluan_vc_test_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join(LLAMA_MARKER_FILE);
        unsafe { std::env::set_var("TYLLUAN_LLAMA_CACHE_DIR", &tmp) };
        std::fs::write(&path, "b10158
").unwrap();
        assert_eq!(read_installed_version().as_deref(), Some("b10158"));
        std::fs::remove_file(&path).ok();
        assert_eq!(read_installed_version(), None);
        unsafe { std::env::remove_var("TYLLUAN_LLAMA_CACHE_DIR") };
        std::fs::remove_dir_all(&tmp).ok();
    }
}
