//! T-DSH-43 — ¿qué argumentos recibe de verdad `web_search` cuando se llama por
//! el router (`tylluan_do`)?
//!
//! Pregunta de la tarea: si el JSON de argumentos que construye el router lleva
//! SIEMPRE la clave `command` (sembrada con el intent), entonces el falso
//! bloqueo de `enforce_capabilities` (`process_execution=false` + argumento
//! `command`) es real para websearch **y para cualquier guild no-ejecutora**;
//! si la clave se poda contra el esquema de la herramienta, el falso bloqueo no
//! existe en la práctica.
//!
//! ## Hallazgo de alcanzabilidad (honesto: no es la función real)
//!
//! `resolve_and_prepare_tool_call` (`transport/server/handler_do/prepare.rs:33`)
//! es `pub(super)` y el módulo `prepare` es privado (`mod prepare;` en
//! `handler_do/mod.rs:42`), así que un test de integración **no puede llamarla**
//! sin tocar `src`. Visibilidad mínima pedida en el informe: hacer
//! `pub mod prepare;` y `pub(super) fn` → `pub fn`, o exponer un wrapper
//! `pub async fn prepare_tool_call_for_test(...)` en `handler_do/mod.rs`.
//!
//! Mientras eso no exista, este fichero fija el comportamiento por dos vías
//! verificables sin el kernel:
//!   1. **Réplica fiel** del seed de argumentos (`prepare.rs:125-131`, copiado
//!      literal) + lectura estática de que no hay poda por esquema: el
//!      `CallToolRequestParam` se construye directamente con ese mapa
//!      (`prepare.rs:335-338`) y es el mismo `params` que recibe
//!      `check_capabilities` (`guild_process.rs:649/662/680`).
//!   2. **Función real de contención**: `enforce_capabilities` (pública) con las
//!      CAPABILITIES declaradas por los `.py` (parser local) y los args
//!      replicados, para medir la consecuencia exacta bajo perfil Strict.
//!
//! Run:
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test router_websearch_args -- --nocapture
//!
//! Sin red, sin kernel, sin puertos, sin `cfg(unix)`/`cfg(windows)`.

use std::path::{Path, PathBuf};

use rmcp::model::CallToolRequestParam;
use serde_json::Value;
use tylluan_kernel::registry::guild_process::enforce_capabilities;

const INTENT: &str = "busca en la web rust async";

/// Réplica **literal** del seed de `prepare.rs:125-131` (mismas claves y
/// valores). El original añade después `agent_id` si lo hay, y `path`/`cwd`/
/// `directory` si `extract_path_from_intent` encuentra una ruta; para este
/// intent no hay ruta ni agent_id, así que el mapa es exactamente este.
fn router_seed_args(intent: &str) -> serde_json::Map<String, Value> {
    serde_json::json!({
        "command": intent, "intent": intent, "task": intent,
        "query": intent, "text": intent, "content": intent,
        "prompt": intent, "message": intent, "input": intent,
        "server_url": "", "url": "",
        "timeout_secs": 30, "language": "", "depth": 2, "max_results": 50,
    })
    .as_object()
    .cloned()
    .expect("seed es objeto")
}

fn params(tool: &str, args: &serde_json::Map<String, Value>) -> CallToolRequestParam {
    CallToolRequestParam {
        name: tool.to_string().into(),
        arguments: Some(args.clone()),
    }
}

fn lone_command(intent: &str) -> serde_json::Map<String, Value> {
    let mut m = serde_json::Map::new();
    m.insert("command".to_string(), Value::String(intent.to_string()));
    m
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn strip_python_comment(line: &str) -> &str {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (idx, ch) in line.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '#' => return &line[..idx],
            _ => {}
        }
    }
    line
}

fn push_block(code: &str, out: &mut String, depth: &mut i32) -> bool {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for ch in code.chars() {
        if let Some(q) = quote {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => {
                quote = Some(ch);
                out.push(ch);
            }
            '{' => {
                *depth += 1;
                out.push(ch);
            }
            '}' => {
                *depth -= 1;
                out.push(ch);
                if *depth <= 0 {
                    return true;
                }
            }
            _ => out.push(ch),
        }
    }
    false
}

/// Réplica local del extractor de CAPABILITIES (T-DSH-42 aún no está en
/// `origin/main`: este test debe medir con las declaraciones del `.py`
/// independientemente de que el catálogo las exponga o no).
fn declared_capabilities(rel: &str) -> Value {
    let path = repo_root().join(rel);
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", path.display()));
    let mut started = false;
    let mut depth = 0;
    let mut json = String::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(['"', '\'']) && trimmed.chars().take(3).all(|c| c == '"' || c == '\'')
        {
            continue;
        }
        if !started {
            let Some(pos) = trimmed.find("CAPABILITIES") else {
                continue;
            };
            let after = trimmed[pos + "CAPABILITIES".len()..].trim();
            let Some(rest) = after.strip_prefix('=') else {
                continue;
            };
            let Some(body) = rest.trim().strip_prefix('{') else {
                continue;
            };
            started = true;
            depth = 1;
            json.push('{');
            if push_block(body, &mut json, &mut depth) {
                break;
            }
        } else {
            let code = strip_python_comment(trimmed);
            if code.trim().is_empty() {
                continue;
            }
            if push_block(code, &mut json, &mut depth) {
                break;
            }
        }
    }
    let normalized = json
        .replace("True", "true")
        .replace("False", "false")
        .replace("None", "null")
        .replace(",}", "}")
        .replace(", }", "}");
    serde_json::from_str(&normalized)
        .unwrap_or_else(|e| panic!("CAPABILITIES ilegibles en {rel}: {e}"))
}

fn strict_caps(rel: &str) -> Value {
    let mut caps = declared_capabilities(rel);
    if let Some(obj) = caps.as_object_mut() {
        obj.insert("process_execution".to_string(), Value::Bool(false));
    }
    caps
}

// ---------------------------------------------------------------------------
// (1)(2)(3) — el seed del router lleva `command` para las cuatro herramientas
// ---------------------------------------------------------------------------

/// **Comportamiento tras D-35b (vivo; antes de D-35b esta medición mostraba el falso bloqueo).** El router siembra `"command": intent` para
/// TODAS las guilds (`prepare.rs:126`), sin poda posterior por esquema: el mapa
/// llega tal cual a `CallToolRequestParam` y de ahí a `check_capabilities`.
#[test]
fn router_seed_carries_command_for_web_search_vision_and_bash() {
    let seed = router_seed_args(INTENT);
    println!("seed del router (prepare.rs:125-131) = {seed:?}");

    let cases: [(&str, &str); 4] = [
        ("web_search", ""),
        ("vision_ocr", ""),
        ("vision_analyze", ""),
        ("bash_execute", " (bash: prepare.rs:154-175 sobrescribe `command` con extract_command_from_intent)"),
    ];
    for (tool, note) in cases {
        let command = seed.get("command").and_then(|v| v.as_str()).unwrap_or("");
        println!("herramienta {tool:<16} command presente={} valor={command:?}{note}", !command.is_empty());
        assert!(
            !command.is_empty(),
            "comportamiento actual esperado: el seed del router lleva 'command' no vacío para {tool}"
        );
        assert_eq!(command, INTENT, "{tool}: el seed pone el intent crudo en 'command'");
        assert_eq!(
            seed.get("query").and_then(|v| v.as_str()),
            Some(INTENT),
            "{tool}: el seed también pone el intent en 'query'"
        );
    }

    // Y el mapa se entrega sin filtrar: claves del seed == claves del parámetro.
    let p = params("web_search", &seed);
    let sent = p.arguments.unwrap();
    assert_eq!(
        sent.len(),
        seed.len(),
        "comportamiento actual esperado: no hay poda por esquema entre el seed y CallToolRequestParam"
    );
    assert!(sent.contains_key("command"));
}

// ---------------------------------------------------------------------------
// Consecuencia medida con la función REAL de contención
// ---------------------------------------------------------------------------

/// **Comportamiento tras D-35b (vivo; antes de D-35b esta medición mostraba el falso bloqueo).** Con las CAPABILITIES declaradas por los
/// `.py` (process_execution=false para vision y websearch) y el override de
/// Strict, `enforce_capabilities` bloquea `web_search`, `vision_ocr` y
/// `vision_analyze` **solo por el `command` sembrado por el router**. Sin ese
/// `command` (args normales del usuario) no bloquea: el falso bloqueo es del
/// seed, no del usuario.
#[test]
fn strict_enforcement_no_longer_blocks_vision_and_websearch_with_router_seed() {
    let seed = router_seed_args(INTENT);
    println!(
        "{:<16} {:<10} {:<52} process_execution=false",
        "herramienta", "caps", "args"
    );

    for (tool, rel) in [
        ("web_search", "guilds/core/websearch.py"),
        ("vision_ocr", "guilds/core/vision.py"),
        ("vision_analyze", "guilds/core/vision.py"),
    ] {
        let caps = strict_caps(rel);
        let blocked = enforce_capabilities(tool.split('_').next().unwrap_or(tool), &caps, &params(tool, &seed));
        println!(
            "{tool:<16} {:<10} {:<52} {}",
            "parsed",
            "seed con command=intent",
            if blocked.is_some() { "BLOQUEADA (regresion D-35b)" } else { "permitida" }
        );
        assert!(
            blocked.is_none(),
            "D-35b: con el seed del router ('command' siempre presente), {tool} no debe bloquearse \
             bajo Strict (herramienta no-exec); motivo: {blocked:?}"
        );

        // Sin el `command` del seed (args normales) NO se bloquea.
        let mut normal = serde_json::Map::new();
        normal.insert("query".to_string(), Value::String(INTENT.to_string()));
        normal.insert("intent".to_string(), Value::String(INTENT.to_string()));
        let blocked_without = enforce_capabilities(
            tool.split('_').next().unwrap_or(tool),
            &caps,
            &params(tool, &normal),
        );
        println!(
            "{tool:<16} {:<10} {:<52} {}",
            "parsed",
            "sin command",
            if blocked_without.is_some() { "BLOQUEADA" } else { "permitida" }
        );
        assert!(
            blocked_without.is_none(),
            "sin 'command' en los args, {tool} no debe bloquearse: el bloqueo viene del seed"
        );
    }

    // bash: la caps declara process_execution=true → permitida (el nombre
    // `bash_execute` sí es exec-like, pero su caps no prohíbe ejecución).
    let bash_caps = declared_capabilities("guilds/builders/plugins/bash.py");
    let bash_blocked =
        enforce_capabilities("bash", &bash_caps, &params("bash_execute", &lone_command(INTENT)));
    println!("bash_execute con caps declaradas (process_execution=true): {bash_blocked:?}");
    assert!(bash_blocked.is_none(), "bash declara process_execution=true: no debe bloquearse");
}
