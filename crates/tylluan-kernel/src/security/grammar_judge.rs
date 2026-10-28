//! T-DSH-115: juez de gramatica de write_gate en servidor EFIMERO aislado.
//!
//! Patron del centinela (T-DSH-109/113): `llama-server` propio, CPU, hilos <= 4,
//! prioridad baja, Job Object kill-on-close, descarga por inactividad y
//! fail-open. NO depende del llama-server del usuario (9000): el servidor del
//! usuario es opcional y solo lo arranca el usuario.
//!
//! Config: `[security.write_gate_judge]` (mismas claves que `[security.sentinel]`:
//! `enabled`, `mode`, `model_path`, `llama_server_path`, `threads`, `ctx_size`,
//! `timeout_ms`, `idle_unload_secs`, `max_cpu_pct`). Apagado por defecto: sin
//! `model_path` la capa 2 se omite (la capa 1 determinista ya corrio).

use std::sync::OnceLock;

use crate::config::SentinelConfig;
use super::sentinel::SentinelJudge;
use super::write_gate::{WRITE_GATE_GRAMMAR, WRITE_GATE_PROMPT_PREFIX};

static GRAMMAR_JUDGE: OnceLock<SentinelJudge> = OnceLock::new();

fn config_from_disk() -> SentinelConfig {
    crate::config::TylluanConfig::load()
        .map(|c| c.security.write_gate_judge)
        .unwrap_or_default()
}

fn judge() -> &'static SentinelJudge {
    GRAMMAR_JUDGE.get_or_init(|| {
        let mut cfg = config_from_disk();
        // Un `model_path` explicito basta para habilitarlo, igual que el
        // centinela: `enabled = true` equivale a `mode = "enforce"`.
        if cfg.model_path.is_some() {
            cfg.enabled = true;
            if cfg.mode.is_none() {
                cfg.mode = Some("enforce".to_string());
            }
        }
        SentinelJudge::new(cfg)
    })
}

/// ¿Hay un juez de gramatica configurado (modelo existente y modo activo)?
pub fn is_configured() -> bool {
    let cfg = config_from_disk();
    let model_ok = cfg.model_path.as_ref().is_some_and(|p| p.is_file());
    model_ok && (cfg.enabled || cfg.mode.is_some())
}

/// Dictamina el contenido con el juez efimero. Devuelve el veredicto en
/// mayusculas (`SAFE`/`SUSPICIOUS`) o un error (fail-open en el llamante).
pub async fn judge_grammar(content: &str) -> Result<String, String> {
    let judge = judge();
    let prompt = format!(
        "{WRITE_GATE_PROMPT_PREFIX}{}",
        content.chars().take(2000).collect::<String>()
    );
    let raw = judge.raw_completion(&prompt, Some(WRITE_GATE_GRAMMAR)).await?;
    Ok(raw.trim().to_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_not_configured() {
        let cfg = SentinelConfig::default();
        assert!(cfg.model_path.is_none());
        assert!(!cfg.enabled);
        assert!(cfg.mode.is_none());
    }

    #[tokio::test]
    async fn judge_returns_error_when_not_configured() {
        // Sin model_path el modo resuelve a Off: raw_completion devuelve Err
        // antes de intentar arrancar nada (fail-open, sin procesos).
        let err = judge_grammar("contenido").await.unwrap_err();
        assert!(err.contains("off") || err.contains("model_path"), "{err}");
    }
}
