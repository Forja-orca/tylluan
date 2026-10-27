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
//! ESTADO verificado 2026-10-27 (M-BUFFY-13): D-35 está CERRADO en `4fdfb2e`;
//! los 3 tests corren en la suite normal (0 ignorados) y pasan:
//!   - **pin de regresión** (`d35_catalog_now_parses_multiline_capabilities`):
//!     antes del arreglo fijaba el defecto (`None` para vision y websearch);
//!     hoy fija lo contrario y detecta cualquier regresión del parser;
//!   - (a): ambas guilds se parsean (`Some`) con `process_execution=false`;
//!   - (c): `websearch.web_search(command="rust async")` no se bloquea
//!     (`websearch.py:44`: `command` es el tercer fallback legítimo de la query).
//!
//! Run:
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test cola_d35_acceptance
//! (sin tests ignorados: `-- --ignored` no ejecuta nada)
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

/// **Comportamiento TRAS D-35(a) (pin de regresion).** El catalogo ya expone
/// las CAPABILITIES declaradas en multilinea de vision y websearch: si vuelve a
/// `None`, el parser ha regresado.
#[test]
fn d35_catalog_now_parses_multiline_capabilities() {
    for guild in ["vision", "websearch"] {
        let capabilities = catalog_capabilities(guild).unwrap_or_else(|| {
            panic!("regresion de D-35: el catalogo vuelve a dar None para '{guild}'")
        });
        assert_eq!(
            capabilities.get("process_execution").and_then(|v| v.as_bool()),
            Some(false),
            "'{guild}' debe declarar process_execution=false; capacidades: {capabilities}"
        );
    }
}

/// **Aceptación (a) — D-35.** `vision` y `websearch` (declaraciones
/// multilínea) se exponen como `Some` con `process_execution = false`;
/// cumplido desde `4fdfb2e`, donde el test pasó de rojo a verde.
#[test]
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

/// **Aceptación (c) — D-35.** Con las capacidades ya parseadas, el uso
/// legítimo `websearch.web_search(command="rust async")` no se bloquea: en
/// `websearch.py` (línea 44) `command` es el tercer fallback de la query, no
/// un comando de shell. Antes del arreglo fallaba en el primer paso (catálogo
/// `None`); desde `4fdfb2e` pasa.
#[test]
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
