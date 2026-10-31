//! T-DSH-44 — ¿acierta `is_exec_like_tool` con los nombres reales de
//! `guilds/core`?
//!
//! La heurística de contención (`crates/tylluan-kernel/src/registry/guild_process.rs:175-180`)
//! decide si una herramienta "ejecuta comandos" buscando subcadenas en su NOMBRE:
//! `execute`, `run`, `exec`, `spawn`, `shell`, `command`. Con D-35b ese nombre es
//! el ÚNICO criterio para `process_execution=false`, así que un falso positivo
//! bloquea una herramienta legítima y un falso negativo deja pasar una que sí
//! ejecuta procesos.
//!
//! Este test escanea TODAS las herramientas `@mcp.tool` de `guilds/core/*.py`
//! (parseando `def`/`async def` y su cuerpo) y mide:
//!   1. **falsos positivos**: exec-like por subcadena pero que NO ejecutan
//!      procesos (ni directa ni transitivamente por helpers del mismo fichero);
//!   2. **falsos negativos**: NO exec-like pero que ejecutan procesos
//!      (directa o transitivamente) o aceptan un argumento que acaba ejecutándose
//!      (p. ej. `server_url='stdio://...'`).
//!
//! Además lista las coincidencias de subcadena en defs NO-herramienta
//! (`_truncate` contiene `run`), que hoy no son invocables por MCP pero muestran
//! la fragilidad del mecanismo.
//!
//! Run: rustup run 1.99.1 cargo test -p tylluan-kernel --test exec_like_tool_names -- --nocapture
//!
//! Sin red, sin kernel, sin puertos, sin `cfg(unix)`/`cfg(windows)` en ninguna
//! función (solo se leen ficheros `.py` del repo).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Réplica literal de la heurística (`guild_process.rs:175-180`).
const EXEC_SUBSTRINGS: [&str; 6] = ["execute", "run", "exec", "spawn", "shell", "command"];

/// Patrones que indican ejecución de procesos en un cuerpo Python.
const PROC_PATTERNS: [&str; 8] = [
    "subprocess.",
    "os.system",
    "os.popen",
    "Popen(",
    "check_output",
    "check_call",
    "shell=True",
    "stdio_client(",
];

/// Patrones de "argumento que se ejecuta" (proceso elegido por el llamador).
const ARG_EXEC_PATTERNS: [&str; 4] = [
    "StdioServerParameters(",
    "stdio_client(",
    "subprocess.",
    "Popen(",
];

fn is_exec_like(name: &str) -> bool {
    EXEC_SUBSTRINGS.iter().any(|s| name.contains(s))
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

struct Def {
    line: usize,
    name: String,
    params: Vec<String>,
    body_start: usize,
    body_end: usize,
    is_tool: bool,
}

fn line_starts_at_col0(line: &str) -> bool {
    !line.starts_with(' ') && !line.starts_with('\t')
}

/// Vacía el contenido de docstrings triples (comillas triples), que mencionan
/// `subprocess` sin ejecutarlo (p. ej. `night_reasoner.py:104-117`).
fn strip_docstrings(lines: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(lines.len());
    let mut in_doc = false;
    for line in lines {
        let mut rest: &str = line;
        let mut kept = String::new();
        loop {
            if in_doc {
                if let Some(pos) = find_triple(rest) {
                    rest = &rest[pos + 3..];
                    in_doc = false;
                    continue;
                }
                break;
            }
            match find_triple(rest) {
                Some(pos) => {
                    kept.push_str(&rest[..pos]);
                    rest = &rest[pos + 3..];
                    in_doc = true;
                }
                None => {
                    kept.push_str(rest);
                    break;
                }
            }
        }
        out.push(kept);
    }
    out
}

fn find_triple(s: &str) -> Option<usize> {
    let d = s.find("\"\"\"");
    let q = s.find("'''");
    match (d, q) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

/// Quita un comentario `# ...` de una línea de Python (solo fuera de cadenas),
/// para no confundir comentarios ("... manages llama-server subprocess.") con
/// ejecución real.
fn strip_comment(line: &str) -> String {
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
            '#' => return line[..idx].to_string(),
            _ => {}
        }
    }
    line.to_string()
}

fn parse_signature_params(sig: &str) -> Vec<String> {
    let mut params = Vec::new();
    for part in sig.split(',') {
        let head = part.split([':', '=']).next().unwrap_or("").trim();
        let head = head.trim_start_matches('*').trim();
        if head.is_empty() || head == "/" || head == "self" || head.contains(' ') {
            continue;
        }
        params.push(head.to_string());
    }
    params
}

fn parse_defs(lines: &[String]) -> Vec<Def> {
    let mut defs: Vec<Def> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if !line_starts_at_col0(line) {
            continue;
        }
        let trimmed = line.trim_end();
        if !(trimmed.starts_with("def ") || trimmed.starts_with("async def ")) {
            continue;
        }
        let Some(def_pos) = trimmed.find("def ") else {
            continue;
        };
        let after = &trimmed[def_pos + 4..];
        let Some(paren) = after.find('(') else {
            continue;
        };
        let name = after[..paren].trim().to_string();
        if name.is_empty() {
            continue;
        }
        // Firma puede ser multilínea: acumular hasta el paréntesis de cierre.
        let mut sig = String::new();
        let mut collected = String::new();
        for (k, line) in lines.iter().enumerate().skip(i) {
            let fragment = if k == i {
                &after[paren + 1..]
            } else {
                line.as_str()
            };
            collected.push_str(fragment);
            collected.push(' ');
            if fragment.contains(')') {
                let close = collected.find(')').unwrap_or(collected.len());
                sig = collected[..close].to_string();
                break;
            }
        }
        let mut body_end = lines.len();
        for (j, line) in lines.iter().enumerate().skip(i + 1) {
            if !line_starts_at_col0(line) {
                continue;
            }
            let t = line.trim_start();
            if t.starts_with("def ")
                || t.starts_with("async def ")
                || t.starts_with('@')
                || t.starts_with("class ")
            {
                body_end = j;
                break;
            }
        }
        let mut is_tool = false;
        let mut k = i;
        while k > 0 {
            k -= 1;
            let t = lines[k].trim_start();
            if t.starts_with('@') {
                if t.contains("mcp.tool") || t.contains("app.tool") || t.starts_with("@tool") {
                    is_tool = true;
                }
                continue;
            }
            break;
        }
        defs.push(Def {
            line: i + 1,
            name,
            params: parse_signature_params(&sig),
            body_start: i,
            body_end,
            is_tool,
        });
    }
    defs
}

fn body_text(lines: &[String], d: &Def) -> String {
    let code = strip_docstrings(lines);
    code[d.body_start..d.body_end]
        .iter()
        .map(|l| strip_comment(l))
        .collect::<Vec<_>>()
        .join("\n")
}

fn contains_call(text: &str, name: &str) -> bool {
    let mut rest: &str = text;
    while let Some(pos) = rest.find(name) {
        let before_ok = pos == 0
            || !rest[..pos]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_');
        let after = &rest[pos + name.len()..];
        let after_ok = after.trim_start().starts_with('(');
        if before_ok && after_ok {
            return true;
        }
        rest = &after[0..];
        if rest.is_empty() {
            break;
        }
        rest = &rest[1..];
    }
    false
}

/// ¿Ejecuta un proceso directa o transitivamente por helpers del mismo fichero?
fn proc_reach(defs: &[Def], lines: &[String], target: usize) -> (Vec<usize>, Option<String>) {
    let mut direct = Vec::new();
    let text = body_text(lines, &defs[target]);
    let code_lines = strip_docstrings(lines);
    for (offset, line) in code_lines[defs[target].body_start..defs[target].body_end]
        .iter()
        .enumerate()
    {
        let code = strip_comment(line);
        if PROC_PATTERNS.iter().any(|p| code.contains(p)) {
            direct.push(defs[target].body_start + offset + 1);
        }
    }
    if !direct.is_empty() {
        return (direct, None);
    }
    for (idx, other) in defs.iter().enumerate() {
        if idx == target || !contains_call(&text, &other.name) {
            continue;
        }
        let (lines_proc, _) = proc_reach(defs, lines, idx);
        if !lines_proc.is_empty() {
            return (lines_proc, Some(other.name.clone()));
        }
    }
    (Vec::new(), None)
}

/// ¿Algún argumento de la herramienta acaba en una llamada que ejecuta un
/// proceso (a través de helpers del mismo fichero)?
fn arg_exec_reachable(defs: &[Def], lines: &[String], tool: &Def) -> bool {
    let mut reachable: BTreeSet<usize> = BTreeSet::new();
    let mut pending: Vec<usize> = Vec::new();
    for (idx, d) in defs.iter().enumerate() {
        if idx != defs.iter().position(|x| x.line == tool.line).unwrap_or(usize::MAX)
            && contains_call(&body_text(lines, tool), &d.name)
        {
            pending.push(idx);
        }
    }
    while let Some(idx) = pending.pop() {
        if !reachable.insert(idx) {
            continue;
        }
        let text = body_text(lines, &defs[idx]);
        for (j, d) in defs.iter().enumerate() {
            if j != idx && contains_call(&text, &d.name) && !reachable.contains(&j) {
                pending.push(j);
            }
        }
    }
    let mut corpus = body_text(lines, tool);
    let mut exec_called = false;
    for idx in &reachable {
        let text = body_text(lines, &defs[*idx]);
        if ARG_EXEC_PATTERNS.iter().any(|p| text.contains(p)) {
            exec_called = true;
        }
        corpus.push_str(&text);
    }
    exec_called && tool.params.iter().any(|p| corpus.contains(p.as_str()))
}

#[derive(Debug)]
struct ToolRow {
    file: String,
    line: usize,
    tool: String,
    exec_like: bool,
    proc_lines: Vec<usize>,
    via: Option<String>,
    arg_exec: bool,
}

impl ToolRow {
    fn key(&self) -> String {
        format!("{}.{}", self.file, self.tool)
    }

    fn executes(&self) -> bool {
        !self.proc_lines.is_empty() || self.arg_exec
    }
}

fn scan_core() -> (Vec<ToolRow>, Vec<(String, usize, String)>) {
    let core = repo_root().join("guilds/core");
    let mut rows = Vec::new();
    let mut fragile = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(&core)
        .unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", core.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "py"))
        .collect();
    entries.sort();
    for path in entries {
        let file = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string();
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", path.display()));
        let lines: Vec<String> = content.lines().map(str::to_string).collect();
        let defs = parse_defs(&lines);
        for (idx, d) in defs.iter().enumerate() {
            if d.is_tool {
                let (proc_lines, via) = proc_reach(&defs, &lines, idx);
                let arg_exec = arg_exec_reachable(&defs, &lines, d);
                rows.push(ToolRow {
                    file: file.clone(),
                    line: d.line,
                    tool: d.name.clone(),
                    exec_like: is_exec_like(&d.name),
                    proc_lines,
                    via,
                    arg_exec,
                });
            } else if is_exec_like(&d.name) {
                fragile.push((file.clone(), d.line, d.name.clone()));
            }
        }
    }
    rows.sort_by_key(ToolRow::key);
    (rows, fragile)
}

#[test]
fn exec_like_heuristic_measured_over_real_core_tools() {
    let (rows, fragile) = scan_core();
    assert!(!rows.is_empty(), "no se encontraron herramientas @mcp.tool en guilds/core");

    println!(
        "{:<20} {:>5} {:<30} {:<4} {:<16} {:<24} arg_exec",
        "fichero", "linea", "herramienta", "exec", "ejecuta(lineas)", "via"
    );
    for r in &rows {
        println!(
            "{:<20} {:>5} {:<30} {:<4} {:<16} {:<24} {}",
            r.file,
            r.line,
            r.tool,
            r.exec_like,
            if r.proc_lines.is_empty() {
                "-".to_string()
            } else {
                format!("{:?}", r.proc_lines)
            },
            r.via.clone().unwrap_or_else(|| "-".to_string()),
            r.arg_exec
        );
    }

    // (1) Falsos positivos: exec-like por subcadena pero que NO ejecutan nada.
    let fps: Vec<String> = rows
        .iter()
        .filter(|r| r.exec_like && !r.executes())
        .map(ToolRow::key)
        .collect();
    println!("\nFALSOS POSITIVOS (exec-like sin ejecución real): {fps:?}");

    // (2) Falsos negativos: no exec-like pero ejecutan o aceptan arg ejecutado.
    let fns: Vec<String> = rows
        .iter()
        .filter(|r| !r.exec_like && r.executes())
        .map(ToolRow::key)
        .collect();
    println!("FALSOS NEGATIVOS (no exec-like pero ejecutan): {fns:?}");

    println!("\nSubcadenas en defs NO-herramienta (no invocables por MCP hoy):");
    for (f, line, name) in &fragile {
        println!("  {f}:{line} def {name}");
    }

    // Comportamiento de HOY, fijado (cambiar solo con revisión explícita).
    assert_eq!(
        fps,
        vec![
            "n8n_bridge.execute_workflow".to_string(),
            "n8n_bridge.get_execution_status".to_string(),
        ],
        "falsos positivos exec-like medidos hoy sobre guilds/core"
    );
    assert_eq!(
        fns,
        vec![
            "browser.browser_navigate".to_string(),
            "browser.search_web".to_string(),
            "comfy_ui.generate_documentary_video".to_string(),
            "comfy_ui.generate_short".to_string(),
            // T-DSH-151: medido en b9f1331 — llama_backend expone ahora
            // start_server/stop_server (exec-like) y query_model ya no lo es.
            "llama_backend.start_server".to_string(),
            "llama_backend.stop_server".to_string(),
            "mcp_bridge.mcp_call".to_string(),
            "mcp_bridge.mcp_list_tools".to_string(),
            "mcp_bridge.mcp_ping".to_string(),
        ],
        "falsos negativos medidos hoy sobre guilds/core"
    );
    let fragile_expected = vec![
        "browser:81 ensure_browser_running",
        "browser:112 send_cdp_command",
        "coloquio:336 _truncate",
        "coloquio_digest:279 _run_reasoning_background",
        "scheduler:171 _run_deepeval_once",
        "utils:64 run_command",
        "utils:165 find_executable",
        "utils:177 truncate_output",
        "utils:272 safe_mcp_run",
        "vision:338 _run_vision",
    ];
    let fragile_keys: Vec<String> = fragile
        .iter()
        .map(|(f, line, name)| format!("{f}:{line} {name}"))
        .collect();
    assert_eq!(
        fragile_keys, fragile_expected,
        "nombres NO herramienta con subcadenas exec-like (fragilidad del mecanismo)"
    );
}
