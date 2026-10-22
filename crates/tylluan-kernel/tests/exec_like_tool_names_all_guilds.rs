//! T-DSH-45 — extiende T-DSH-44 a TODOS los guilds fuera de `guilds/core` y
//! audita el bypass de allowlist de `mcp_bridge`.
//!
//! Parte 1: mismo escáner de T-DSH-44 (`is_exec_like_tool` replica de
//! `guild_process.rs:175-180`) sobre `guilds/builders`, `guilds/scholars`,
//! `guilds/wardens`, `guilds/watchers` (recursivo; excluye `guilds/core`, que ya
//! cubre `tests/exec_like_tool_names.rs`). Mide falsos positivos (exec-like que
//! NO ejecutan procesos) y falsos negativos (no exec-like que SÍ ejecutan, vía
//! `subprocess`/`asyncio.create_subprocess_*`/`utils.run_command` o un argumento
//! ejecutado), con transitividad por helpers del mismo fichero.
//!
//! Parte 2: lee `guilds/core/mcp_bridge.py` y comprueba si el **llamador** puede
//! saltarse la allowlist de `stdio://` pasando `enforce_allowlist=False` a la
//! herramienta `mcp_call` (`mcp_bridge.py:136-177`), qué allowlist hay por
//! defecto (`mcp_bridge.py:46-48`) y qué implica el bypass (comando arbitrario +
//! entorno completo, `mcp_bridge.py:76-99`).
//!
//! Run: rustup run 1.99.1 cargo test -p tylluan-kernel --test exec_like_tool_names_all_guilds -- --nocapture
//!
//! Sin red, sin kernel, sin puertos, sin `cfg(unix)`/`cfg(windows)` en ninguna
//! función (solo se leen ficheros `.py` del repo).

use std::path::{Path, PathBuf};

/// Réplica literal de la heurística (`guild_process.rs:175-180`).
const EXEC_SUBSTRINGS: [&str; 6] = ["execute", "run", "exec", "spawn", "shell", "command"];

/// Patrones de ejecución de procesos. Se añaden a los de T-DSH-44 los que usan
/// el resto de guilds: `asyncio.create_subprocess_*` (git, docker, sandbox) y
/// `utils.run_command(` (helper compartido de `guilds/builders/plugins/utils.py`,
/// que ejecuta `subprocess.run` en `utils.py:88-89`; lo usan clipboard, ffmpeg,
/// screenshot, biome, search, cron…).
const PROC_PATTERNS: [&str; 12] = [
    "subprocess.",
    "os.system",
    "os.popen",
    "Popen(",
    "check_output",
    "check_call",
    "shell=True",
    "stdio_client(",
    "create_subprocess_exec",
    "create_subprocess_shell",
    "asyncio.subprocess",
    "run_command(",
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

/// Vacía el contenido de literales de cadena: `code_security_scan` contiene
/// patrones regex como `r"subprocess\s*\.\s*shell"` que NO ejecutan nada y el
/// escáner no debe contar como ejecución.
fn strip_strings(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for ch in line.chars() {
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
            '\'' | '"' => {
                quote = Some(ch);
                out.push(ch);
            }
            _ => out.push(ch),
        }
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
        .map(|l| strip_strings(&strip_comment(l)))
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

fn proc_reach(defs: &[Def], lines: &[String], target: usize) -> (Vec<usize>, Option<String>) {
    let mut direct = Vec::new();
    let text = body_text(lines, &defs[target]);
    let code_lines = strip_docstrings(lines);
    for (offset, line) in code_lines[defs[target].body_start..defs[target].body_end]
        .iter()
        .enumerate()
    {
        let code = strip_strings(&strip_comment(line));
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

fn arg_exec_reachable(defs: &[Def], lines: &[String], tool: &Def) -> bool {
    let mut reachable: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
    let mut pending: Vec<usize> = Vec::new();
    let tool_idx = defs.iter().position(|x| x.line == tool.line).unwrap_or(usize::MAX);
    for (idx, d) in defs.iter().enumerate() {
        if idx != tool_idx && contains_call(&body_text(lines, tool), &d.name) {
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

struct ToolRow {
    key: String,
    line: usize,
    tool: String,
    exec_like: bool,
    proc_lines: Vec<usize>,
    via: Option<String>,
    arg_exec: bool,
}

impl ToolRow {
    fn executes(&self) -> bool {
        !self.proc_lines.is_empty() || self.arg_exec
    }
}

fn python_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n == "__pycache__" || n == "core")
            {
                continue;
            }
            python_files(&path, out);
        } else if path.extension().is_some_and(|x| x == "py") {
            out.push(path);
        }
    }
}

fn scan_non_core() -> Vec<ToolRow> {
    let guilds = repo_root().join("guilds");
    let mut files = Vec::new();
    python_files(&guilds, &mut files);
    files.sort();
    let mut rows = Vec::new();
    for path in files {
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", path.display()));
        let lines: Vec<String> = content.lines().map(str::to_string).collect();
        let defs = parse_defs(&lines);
        let rel = path
            .strip_prefix(&guilds)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let rel = rel.trim_end_matches(".py").to_string();
        for (idx, d) in defs.iter().enumerate() {
            if !d.is_tool {
                continue;
            }
            let (proc_lines, via) = proc_reach(&defs, &lines, idx);
            let arg_exec = arg_exec_reachable(&defs, &lines, d);
            rows.push(ToolRow {
                key: format!("{rel}.{}", d.name),
                line: d.line,
                tool: d.name.clone(),
                exec_like: is_exec_like(&d.name),
                proc_lines,
                via,
                arg_exec,
            });
        }
    }
    rows.sort_by(|a, b| a.key.cmp(&b.key));
    rows
}

#[test]
fn exec_like_heuristic_measured_over_guilds_outside_core() {
    let rows = scan_non_core();
    assert!(!rows.is_empty(), "no se encontraron herramientas @mcp.tool fuera de guilds/core");

    println!(
        "{:<44} {:>5} {:<30} {:<5} {:<16} {:<22} arg_exec",
        "guild/fichero", "linea", "herramienta", "exec", "ejecuta(lineas)", "via"
    );
    for r in &rows {
        println!(
            "{:<44} {:>5} {:<30} {:<5} {:<16} {:<22} {}",
            r.key.rsplit_once('.').map(|(f, _)| f).unwrap_or(""),
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

    let fps: Vec<String> = rows
        .iter()
        .filter(|r| r.exec_like && !r.executes())
        .map(|r| r.key.clone())
        .collect();
    let fns: Vec<String> = rows
        .iter()
        .filter(|r| !r.exec_like && r.executes())
        .map(|r| r.key.clone())
        .collect();
    println!("\nHerramientas fuera de core: {}", rows.len());
    println!("FALSOS POSITIVOS (exec-like sin ejecución real): {fps:?}");
    println!("FALSOS NEGATIVOS (no exec-like pero ejecutan): {fns:?}");

    assert_eq!(
        fps,
        vec!["builders/plugins/database.db_execute_write".to_string()],
        "falsos positivos medidos hoy fuera de guilds/core"
    );
    assert_eq!(
        fns,
        vec![
            "builders/plugins/clipboard_tools.clipboard_read".to_string(),
            "builders/plugins/clipboard_tools.clipboard_write".to_string(),
            "builders/plugins/docker.docker_images".to_string(),
            "builders/plugins/docker.docker_logs".to_string(),
            "builders/plugins/docker.docker_ps".to_string(),
            "builders/plugins/docker.docker_status".to_string(),
            "builders/plugins/docker.docker_stop".to_string(),
            "builders/plugins/ffmpeg_tools.audio_extract".to_string(),
            "builders/plugins/ffmpeg_tools.media_probe".to_string(),
            "builders/plugins/ffmpeg_tools.video_add_text".to_string(),
            "builders/plugins/ffmpeg_tools.video_concat".to_string(),
            "builders/plugins/ffmpeg_tools.video_resize".to_string(),
            "builders/plugins/ffmpeg_tools.video_trim".to_string(),
            "builders/plugins/git.git_add".to_string(),
            "builders/plugins/git.git_branch".to_string(),
            "builders/plugins/git.git_commit".to_string(),
            "builders/plugins/git.git_diff".to_string(),
            "builders/plugins/git.git_log".to_string(),
            "builders/plugins/git.git_status".to_string(),
            "builders/plugins/git.git_status_quick".to_string(),
            "builders/plugins/sandbox.sandbox_ingest".to_string(),
            "builders/plugins/sandbox.sandbox_python".to_string(),
            "builders/plugins/sandbox.sandbox_status".to_string(),
            "builders/plugins/screenshot_tools.screenshot_capture".to_string(),
            "scholars/plugins/search.search_code_structure".to_string(),
            "scholars/plugins/search.search_content".to_string(),
            "wardens/plugins/biome_warden.biome_format_file".to_string(),
            "wardens/plugins/biome_warden.biome_lint_file".to_string(),
            "watchers/plugins/cron_scheduler.cron_cancel".to_string(),
            "watchers/plugins/cron_scheduler.cron_schedule".to_string(),
        ],
        "falsos negativos medidos hoy fuera de guilds/core"
    );
}

// ---------------------------------------------------------------------------
// Parte 2: allowlist de mcp_bridge y bypass por el llamador
// ---------------------------------------------------------------------------

fn read_lines(rel: &str) -> Vec<String> {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", path.display()))
        .lines()
        .map(str::to_string)
        .collect()
}

fn line_of(lines: &[String], needle: &str) -> Option<usize> {
    lines.iter().position(|l| l.contains(needle)).map(|i| i + 1)
}

#[test]
fn mcp_bridge_allowlist_is_bypassable_by_the_caller() {
    let lines = read_lines("guilds/core/mcp_bridge.py");
    let text = lines.join("\n");

    // Allowlist por defecto (mcp_bridge.py:46-48).
    let allow_start =
        line_of(&lines, "_STDIO_ALLOWLIST: frozenset[str]").expect("allowlist declarada");
    let allowlist = lines[allow_start - 1..]
        .iter()
        .take_while(|l| !l.contains("})"))
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    println!("allowlist por defecto en mcp_bridge.py:{allow_start} => {allowlist}");
    for allowed in ["node", "npx", "uv", "python", "python3", "gk"] {
        assert!(
            allowlist.contains(&format!("\"{allowed}\"")),
            "la allowlist debe incluir '{allowed}'; línea {allow_start}"
        );
    }

    // El bypass existe y está expuesto como argumento de la herramienta.
    assert!(
        text.contains("enforce_allowlist: bool = True"),
        "mcp_bridge debe exponer enforce_allowlist como argumento de herramienta"
    );
    assert!(
        text.contains("Pass enforce_allowlist=False to bypass"),
        "el propio mensaje de bloqueo documenta el bypass"
    );
    assert!(
        text.contains("Set False for ad-hoc dev usage"),
        "la docstring de mcp_call documenta el bypass"
    );
    let warn_line = line_of(&lines, "logging.warning(\"mcp_call: enforce_allowlist=False")
        .expect("mcp_call avisa cuando se salta la allowlist");
    let pass_line = line_of(&lines, "_do_call(server_url, tool_name, args, enforce_allowlist)")
        .expect("mcp_call propaga enforce_allowlist a _do_call");
    println!(
        "bypass del llamador: mcp_call acepta enforce_allowlist=False (aviso en mcp_bridge.py:{warn_line}) \
         y lo propaga a _do_call en mcp_bridge.py:{pass_line}"
    );

    // Con enforce_allowlist=False el comando es libre y el entorno es el COMPLETO
    // (no el mínimo): dos consecuencias del bypass, citadas.
    assert!(
        text.contains("env={**os.environ} if not enforce_allowlist else {**_MINIMAL_ENV}"),
        "sin allowlist se pasa el entorno completo del proceso al hijo stdio"
    );
    assert!(
        text.contains("command=parts[0]"),
        "el comando del hijo stdio sale del server_url del llamador"
    );

    // T-DSH-46: mcp_bridge ya declara CAPABILITIES con exec_tools; el bypass
    // queda bloqueado por allowlist_arg cuando la contención está activa.
    assert!(
        text.contains("CAPABILITIES") && text.contains("\"exec_tools\""),
        "mcp_bridge.py debe declarar CAPABILITIES con exec_tools (T-DSH-46)"
    );
    assert!(
        text.contains("\"mcp_call\": {\"command_arg\": \"server_url\""),
        "mcp_call debe declarar command_arg=server_url en exec_tools"
    );
    assert!(
        text.contains("\"allowlist_arg\": \"enforce_allowlist\""),
        "mcp_call debe declarar allowlist_arg=enforce_allowlist (bypass bloqueado con la contención activa)"
    );

    // Líneas exactas para el informe (se imprimen y se fijan).
    let mcp_call_line = line_of(&lines, "async def mcp_call(").expect("mcp_call");
    let stdio_params_line = line_of(&lines, "StdioServerParameters(").expect("StdioServerParameters");
    let stdio_client_line = line_of(&lines, "stdio_client(params)").expect("stdio_client");
    println!(
        "mcp_call en mcp_bridge.py:{mcp_call_line}; StdioServerParameters en :{stdio_params_line}; \
         stdio_client en :{stdio_client_line}; allowlist en :{allow_start}"
    );
    assert_eq!(allow_start, 62, "allowlist medido en mcp_bridge.py:62");
    assert_eq!(warn_line, 176, "aviso de bypass medido en mcp_bridge.py:176");
    assert_eq!(pass_line, 181, "propagación medido en mcp_bridge.py:181");
    assert_eq!(mcp_call_line, 150, "mcp_call medido en mcp_bridge.py:150");
    assert_eq!(stdio_params_line, 101, "StdioServerParameters medido en mcp_bridge.py:101");
    assert_eq!(stdio_client_line, 106, "stdio_client medido en mcp_bridge.py:106");
}
