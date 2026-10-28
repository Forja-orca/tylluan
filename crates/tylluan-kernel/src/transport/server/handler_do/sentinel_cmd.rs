//! T-DSH-114: interruptor del centinela por MCP (`tylluan_do`).
//!
//! Deterministic intents, no guild routing and no new MCP tool (CONTRACT-01
//! keeps exactly 6 sovereign tools):
//! - `sentinel status` / `sentinel estado` / `estado centinela` / `estado del centinela`
//! - `sentinel mode <off|observe|enforce>` / `modo centinela <...>` / `sentinel <...>`
//!
//! Solo LECTURA por MCP. Cambiar el modo exige clase operador (REST/CLI): un
//! agente no puede apagar el centinela con un intent.

use crate::security::sentinel::{self, SentinelMode};

fn parse_mode(raw: &str) -> Option<SentinelMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "off" => Some(SentinelMode::Off),
        "observe" => Some(SentinelMode::Observe),
        "enforce" => Some(SentinelMode::Enforce),
        _ => None,
    }
}

/// Devuelve `Some(json)` si el intent es del centinela; `None` para que el
/// llamante siga con el routing normal.
pub fn handle_sentinel_intent(intent: &str) -> Option<serde_json::Value> {
    let low = intent.trim().to_ascii_lowercase();
    // Solo si el intent EMPIEZA por un prefijo del centinela; el resto cae al
    // routing normal (si no, "health" devolvería el estado del centinela).
    let rest = low
        .strip_prefix("sentinel")
        .or_else(|| low.strip_prefix("modo centinela"))
        .or_else(|| low.strip_prefix("estado del centinela"))
        .or_else(|| low.strip_prefix("estado centinela"))
        .or_else(|| low.strip_prefix("centinela"))?;
    let rest = rest.trim();

    // Status: "sentinel status|estado", "estado centinela", o "sentinel" a secas.
    if rest.is_empty() || rest == "status" || rest == "estado" {
        return Some(sentinel::health_json());
    }

    // Cambiar el modo NO se hace por MCP/`tylluan_do`: este camino lo alcanza
    // cualquier agente (y cualquier texto inyectado que un agente lea), y apagar el
    // centinela es una decision de clase operador. El cambio en caliente va por
    // REST (`POST /api/v1/security/sentinel/mode`, operator_class_required) o por
    // la CLI `tylluan sentinel mode`, que usan el token de operador.
    let mode_raw = rest
        .strip_prefix("mode")
        .or_else(|| rest.strip_prefix("modo"))
        .map(str::trim)
        .unwrap_or(rest);
    parse_mode(mode_raw)?;
    Some(serde_json::json!({
        "ok": false,
        "error": "cambiar el modo del centinela requiere clase operador: usa POST /api/v1/security/sentinel/mode o `tylluan sentinel mode <off|observe|enforce>`",
        "state": sentinel::health_json(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_intents_are_recognized() {
        for intent in [
            "sentinel status",
            "sentinel estado",
            "estado centinela",
            "estado del centinela",
            "sentinel",
            "centinela",
        ] {
            let v = handle_sentinel_intent(intent).expect(intent);
            assert!(v.get("mode").is_some(), "{intent}: {v}");
        }
    }

    #[test]
    fn mode_intents_are_refused_and_do_not_change_the_mode() {
        let before = sentinel::global().mode();
        for intent in ["sentinel mode observe", "modo centinela off", "sentinel enforce"] {
            let v = handle_sentinel_intent(intent).expect(intent);
            assert_eq!(v["ok"], serde_json::json!(false), "{intent}: {v}");
            assert!(v["error"].as_str().unwrap().contains("operador"), "{intent}: {v}");
        }
        assert_eq!(sentinel::global().mode(), before, "un intent MCP no puede cambiar el modo");
    }

    #[test]
    fn invalid_mode_falls_through_to_normal_routing() {
        assert!(handle_sentinel_intent("sentinel mode basura").is_none());
    }

    #[test]
    fn unrelated_intents_fall_through() {
        assert!(handle_sentinel_intent("lista los ficheros del repo").is_none());
        assert!(handle_sentinel_intent("hola").is_none());
    }

    #[test]
    fn parse_mode_accepts_case_and_spaces() {
        assert_eq!(parse_mode(" ENFORCE "), Some(SentinelMode::Enforce));
        assert_eq!(parse_mode("Observe"), Some(SentinelMode::Observe));
        assert_eq!(parse_mode("nope"), None);
    }
}
