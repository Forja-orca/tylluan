//! T-DSH-41 — aceptación ejecutable de D-35 (arreglar `extract_capabilities`).
//!
//! D-35 (hallazgo de T-DSH-32): `extract_capabilities` (`router/catalog.rs`,
//! privada) solo cierra bien los dicts `CAPABILITIES = {...}` de UNA línea. En
//! las declaraciones multilínea de `guilds/core/vision.py` y
//! `guilds/core/websearch.py` la línea final `}` decrementa la profundidad a 0 y
//! el parser hace `break` **antes** de añadirla al buffer: el JSON queda truncado
//! (`{ ..., `) y `serde_json` devuelve `None`. Consecuencia: el catálogo expone
//! esas dos guilds como "sin CAPABILITIES" y su contención nunca se aplica.
//!
//! La función es privada; el test usa la vía pública equivalente,
//! `router::catalog::builtin_catalog()` (que la invoca internamente al descubrir
//! los `.py`) y comprueba `GuildDescriptor::capabilities`. No se replica la
//! lógica ni se toca `src`.
//!
//! Tests:
//!   - **vivo**: fija que HOY `builtin_catalog()` da `None` para vision y
//!     websearch (el defecto); Deep debe invertirlo al cerrar D-35;
//!   - `#[ignore = "D-35"]` (a): tras el arreglo, ambas deben parsearse (`Some`)
//!     con `process_execution=false`;
//!   - `#[ignore = "D-35"]` (c): con esas capacidades ya parseadas,
//!     `websearch.web_search(command="rust async")` NO debe bloquearse (en
//!     `websearch.py` línea 44, `command` es el tercer fallback legítimo de la
//!     query; T-DSH-32 midió que `enforce_capabilities` lo bloquearía).
//!
//! Run:
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test cola_d35_acceptance
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test cola_d35_acceptance -- --ignored
//!
//! Sin `cfg(unix)`/`cfg(windows)` en ninguna función; `src/` intacto.

use rmcp::model::CallToolRequestParam;
use serde_json::Value;
use tylluan_kernel::registry::guild_process::enforce_capabilities;
use tylluan_kernel::router::catalog::builtin_catalog;

/// Capacidades que el catálogo expone HOY para una guild (la función real,
/// privada, es la que `builtin_catalog()` invoca al descubrir los `.py`).
fn catalog_capabilities(guild: &str) -> Option<Value> {
    let catalog = builtin_catalog();
    let desc = catalog
        .iter()
        .find(|g| g.name == guild)
        .unwrap_or_else(|| panic!("guild '{guild}' ausente del catálogo"));
    desc.capabilities.clone()
}

fn params(tool: &str, args: &[(&str, &str)]) -> CallToolRequestParam {
    let mut map = serde_json::Map::new();
    for (k, v) in args {
        map.insert((*k).to_string(), Value::String((*v).to_string()));
    }
    CallToolRequestParam {
        name: tool.to_string().into(),
        arguments: if map.is_empty() { None } else { Some(map) },
    }
}

/// **Comportamiento ACTUAL (vivo).** El catálogo no expone capacidades para
/// vision ni websearch: `extract_capabilities` pierde el cierre de los dicts
/// multilínea. Deep debe invertir este test al cerrar D-35.
#[test]
fn d35_current_catalog_returns_none_for_multiline_capabilities() {
    for guild in ["vision", "websearch"] {
        assert!(
            catalog_capabilities(guild).is_none(),
            "comportamiento actual esperado: el catálogo NO debe exponer CAPABILITIES de \
             '{guild}' todavía (si ya las expone, D-35 está arreglado: invierte este test)"
        );
    }
}

/// **DESEADO (a) — D-35.** Tras arreglar el parser, `vision` y `websearch`
/// (declaraciones multilínea) deben exponerse como `Some` con
/// `process_execution = false`. Hoy falla: `None` para ambas.
#[test]
#[ignore = "D-35"]
fn d35_desired_multiline_capabilities_are_parsed() {
    for guild in ["vision", "websearch"] {
        let capabilities = catalog_capabilities(guild).unwrap_or_else(|| {
            panic!(
                "D-35 pendiente: el catálogo sigue dando None para '{guild}'; \
                 extract_capabilities pierde el '}}' final de la declaración multilínea"
            )
        });
        assert_eq!(
            capabilities.get("process_execution").and_then(|v| v.as_bool()),
            Some(false),
            "D-35: '{guild}' debe declarar process_execution=false; capacidades: {capabilities}"
        );
    }
}

/// **DESEADO (c) — D-35.** Con las capacidades ya parseadas, el uso legítimo
/// `websearch.web_search(command="rust async")` no debe bloquearse: en
/// `websearch.py` (línea 44) `command` es el tercer fallback de la query, no un
/// comando de shell. Hoy falla ya en el primer paso (el catálogo da `None`), que
/// es la razón correcta.
#[test]
#[ignore = "D-35"]
fn d35_desired_websearch_command_fallback_is_not_blocked() {
    let capabilities = catalog_capabilities("websearch").unwrap_or_else(|| {
        panic!(
            "D-35 pendiente: sin CAPABILITIES parseadas no se puede medir el falso bloqueo \
             de web_search(command=...); el catálogo sigue dando None"
        )
    });
    let blocked = enforce_capabilities(
        "websearch",
        &capabilities,
        &params("web_search", &[("command", "rust async")]),
    );
    assert!(
        blocked.is_none(),
        "D-35 pendiente (falso bloqueo de T-DSH-32): el uso legítimo \
         web_search(command=\"rust async\") se bloquea porque process_execution=false + \
         argumento 'command'; en websearch.py 'command' es fallback de query, no shell. \
         motivo: {blocked:?}"
    );
}
