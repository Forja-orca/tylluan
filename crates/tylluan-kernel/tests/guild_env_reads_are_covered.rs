//! T-DSH-49 — ¿alguna guild lee una variable de entorno que D-34 no hereda?
//!
//! D-34 (`crates/tylluan-kernel/src/registry/guild_process.rs`) hace
//! `command.env_clear()` sobre el hijo stdio y solo reinyecta:
//! - `guild_inherited_env(key)` → `const EXACT` + `const PREFIXES`;
//! - `guild_may_inherit(guild, key)` → credenciales de servicio por guild;
//! - `TYLLUAN_GUILD_TOKEN` (credencial efímera) y el entorno del launcher.
//!
//! `TYLLUAN_TOKEN` se excluye a propósito (el maestro no se hereda).
//!
//! Este test **lee el contrato del propio `guild_process.rs`** (extrae las listas
//! con regex, igual que `exec_like_tool_names.rs` lee la heurística) y barre
//! `guilds/` COMPLETO (`builders/`, `scholars/`, `wardens/`, `watchers/`,
//! `core/`, `plugins/` y subcarpetas) buscando lecturas de entorno:
//!
//! - `os.environ["X"]` / `os.environ.get("X")` / `os.getenv("X")`;
//! - `"X" in os.environ` (pertenencia: también es lectura);
//! - claves **dinámicas** (`os.environ.get(api_key_env)`): se intentan resolver
//!   contra constantes literales del mismo fichero (`NAME = "X"`); si no se
//!   resuelven, se marcan como no verificables (solo se aceptan en guilds con
//!   comodín explícito en `guild_may_inherit`, hoy `llama_backend` con
//!   `*_API_KEY`/`*_API_TOKEN`);
//! - claves en f-string (`os.getenv(f"...")`): se listan igual; si no se pueden
//!   resolver, cuentan como dinámicas.
//!
//! No son lecturas (y por eso no fallan): `os.environ.setdefault("X", …)`
//! (escritura local del guild), `os.environ.copy()` / `{**os.environ}` (reenvío
//! del entorno YA filtrado por D-34) y las iteraciones bare sobre `os.environ`
//! (solo ven lo que D-34 dejó pasar).
//!
//! Excepciones documentadas en `EXCEPTIONS`: `kernel_auth` puede leer
//! `TYLLUAN_TOKEN` porque el diseño D-34 lo mantiene como **fallback de arranque
//! manual** (con `TYLLUAN_GUILD_TOKEN` presente no se usa; el maestro nunca se
//! inyecta). Si mañana se quiere «ni siquiera mencionarlo», es una decisión de
//! D-34 y esta lista debe cambiar con ella.
//!
//! Si D-34 aún no está en el árbol (función `guild_inherited_env` ausente), el
//! test lo declara y pasa sin evaluar: se activa solo cuando el push de D-34
//! llegue. Criterio del TL: en verde sobre `origin/main` una vez empujado D-34.
//!
//! Run:
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test guild_env_reads_are_covered -- --nocapture
//!
//! Sin red, sin kernel, sin `cfg(unix)`/`cfg(windows)` en ninguna función.

use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Excepciones documentadas (guild, variable). Justificación por entrada.
const EXCEPTIONS: [(&str, &str); 1] = [(
    "kernel_auth",
    // D-34 diseño (`DESIGN_D-34_credencial_guilds_deep_2026-10.md`): el kernel
    // NO inyecta `TYLLUAN_TOKEN`; `kernel_auth.py` lo mantiene como fallback
    // SOLO para arranque manual del operador (y cae al fichero si tampoco
    // existe). Con `TYLLUAN_GUILD_TOKEN` presente no hay fallback.
    "TYLLUAN_TOKEN",
)];

/// Credencial inyectada por D-34 (no viene del padre; se permite leerla).
const INJECTED_GUILD_TOKEN: &str = "TYLLUAN_GUILD_TOKEN";

#[derive(Debug, Clone)]
struct Contract {
    exact: BTreeSet<String>,
    prefixes: Vec<String>,
    /// guild -> (sufijos permitidos, nombres exactos permitidos)
    grants: BTreeMap<String, (Vec<String>, Vec<String>)>,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_source(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", path.display()))
}

/// Extrae el bloque `[...]` tras `const NAME`, respetando comillas anidadas.
fn bracket_block(src: &str, after: &str) -> Option<String> {
    let start = src.find(after)?;
    let rest = &src[start + after.len()..];
    // Saltar el tipo (`&[&str]`) y buscar el `[` del VALOR, tras el `=`.
    let eq = rest.find('=')?;
    let open = eq + rest[eq..].find('[')?;
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (idx, ch) in rest[open..].char_indices() {
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
            '"' | '\'' => quote = Some(ch),
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(rest[open..=open + idx].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

fn quoted_literals(block: &str) -> Vec<String> {
    let re = Regex::new(r#""([^"]+)""#).expect("regex de literales");
    re.captures_iter(block)
        .map(|c| c[1].to_string())
        .collect()
}

/// Parsea `guild_inherited_env` / `guild_may_inherit` del código real.
/// `None` si D-34 todavía no está en el árbol.
fn contract_from_source() -> Option<Contract> {
    let src = read_source("crates/tylluan-kernel/src/registry/guild_process.rs");
    if !src.contains("fn guild_inherited_env") || !src.contains("fn guild_may_inherit") {
        return None;
    }
    let exact_block = bracket_block(&src, "const EXACT")?;
    let prefixes_block = bracket_block(&src, "const PREFIXES")?;
    let exact = quoted_literals(&exact_block).into_iter().collect();
    let prefixes = quoted_literals(&prefixes_block);

    // Armas de guild_may_inherit: `"guild" => condición,`
    let mut grants: BTreeMap<String, (Vec<String>, Vec<String>)> = BTreeMap::new();
    let start = src.find("fn guild_may_inherit")?;
    let rest = &src[start..];
    let end = rest.find("\n}").map(|e| e + 2).unwrap_or(rest.len());
    let body = &rest[..end];
    let arm_re = Regex::new(r#""([a-z0-9_]+)"\s*=>\s*([^\n]+)"#).expect("regex de armas");
    let endswith_re = Regex::new(r#"ends_with\(\s*"([^"]+)"\s*\)"#).expect("ends_with");
    let eq_re = Regex::new(r#"(?:==|eq_ignore_ascii_case\()\s*"([^"]+)"\s*\)?"#).expect("eq");
    for cap in arm_re.captures_iter(body) {
        let guild = cap[1].to_string();
        let cond = &cap[2];
        if cond.contains("false") && !cond.contains("ends_with") && !cond.contains("==") {
            continue;
        }
        let mut suffixes = Vec::new();
        let mut exacts = Vec::new();
        for c in endswith_re.captures_iter(cond) {
            suffixes.push(c[1].to_string());
        }
        for c in eq_re.captures_iter(cond) {
            exacts.push(c[1].to_string());
        }
        grants.insert(guild, (suffixes, exacts));
    }
    Some(Contract {
        exact,
        prefixes,
        grants,
    })
}

impl Contract {
    fn inherited_env(&self, key: &str) -> bool {
        if self
            .exact
            .iter()
            .any(|e| e.eq_ignore_ascii_case(key))
        {
            return true;
        }
        let upper = key.to_ascii_uppercase();
        self.prefixes
            .iter()
            .any(|p| upper.starts_with(&p.to_ascii_uppercase()))
    }

    fn may_inherit(&self, guild: &str, key: &str) -> bool {
        let Some((suffixes, exacts)) = self.grants.get(guild) else {
            return false;
        };
        let upper = key.to_ascii_uppercase();
        suffixes
            .iter()
            .any(|s| upper.ends_with(&s.to_ascii_uppercase()))
            || exacts.iter().any(|e| e.eq_ignore_ascii_case(key))
    }

    /// ¿Puede la guild leer esta variable con el contrato D-34?
    /// ¿La guild tiene un comodín por SUFIJO en `guild_may_inherit`? Es el
    /// único caso en que una clave dinámica puede considerarse cubierta: el
    /// contrato concede cualquier nombre que acabe en ese sufijo (hoy
    /// `llama_backend` con `*_API_KEY` / `*_API_TOKEN`). No es verificable
    /// estáticamente, pero sí es el contrato explícito.
    fn has_suffix_wildcard(&self, guild: &str) -> bool {
        self.grants
            .get(guild)
            .is_some_and(|(suffixes, _)| !suffixes.is_empty())
    }

    fn covered(&self, guild: &str, key: &str) -> bool {
        if key == INJECTED_GUILD_TOKEN {
            return true;
        }
        if EXCEPTIONS
            .iter()
            .any(|(g, v)| *g == guild && v.eq_ignore_ascii_case(key))
        {
            return true;
        }
        self.inherited_env(key) || self.may_inherit(guild, key)
    }
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

/// Texto del primer argumento de una llamada/corchete que empieza en
/// `open_pos` (justo después de `(` o `[`), hasta el `,` o `)`/`]` de nivel 0.
fn first_arg_text(lines: &[String], start_line: usize, open_pos: usize) -> String {
    let mut text = String::new();
    for (k, line) in lines.iter().enumerate().skip(start_line).take(8) {
        let seg = if k == start_line {
            line.get(open_pos.min(line.len())..).unwrap_or("")
        } else {
            line.as_str()
        };
        text.push_str(seg);
        if let Some(cut) = arg_boundary(&text) {
            return text[..cut].trim().to_string();
        }
        text.push('\n');
    }
    text.trim().to_string()
}

/// Posición del primer `,` o cierre de nivel 0 en `text`.
fn arg_boundary(text: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (idx, ch) in text.char_indices() {
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
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                if depth == 0 {
                    return Some(idx);
                }
                depth -= 1;
            }
            ',' if depth == 0 => return Some(idx),
            _ => {}
        }
    }
    None
}

/// Resto de argumentos tras el primero (para el default del `.get`).
fn rest_args(lines: &[String], start_line: usize, open_pos: usize) -> String {
    let mut text = String::new();
    for (k, line) in lines.iter().enumerate().skip(start_line).take(8) {
        let seg = if k == start_line {
            line.get(open_pos.min(line.len())..).unwrap_or("")
        } else {
            line.as_str()
        };
        text.push_str(seg);
        if let Some(cut) = arg_boundary(&text) {
            return text[cut + 1..]
                .trim_start()
                .trim_end_matches([')', ']'])
                .trim()
                .to_string();
        }
        text.push('\n');
    }
    String::new()
}

/// Constantes literales del mismo fichero: `NAME = "valor"` / `NAME: str = "valor"`.
fn file_constants(lines: &[String]) -> BTreeMap<String, String> {
    let re =
        Regex::new(r#"^\s*([A-Za-z_][A-Za-z0-9_]*)\s*(?::\s*[A-Za-z_\[\]., ]+)?=\s*"([^"]*)"\s*$"#)
            .expect("regex de constantes");
    let mut map = BTreeMap::new();
    for line in lines {
        let code = strip_comment(line);
        if let Some(c) = re.captures(code.trim_end()) {
            map.insert(c[1].to_string(), c[2].to_string());
        }
    }
    map
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Literal,
    Dynamic,
    ResolvedConst,
    Membership,
    /// setdefault/copy/forward: no es lectura de una variable del kernel.
    NotARead,
}

#[derive(Debug, Clone)]
struct Read {
    file: String,
    line: usize,
    guild: String,
    var: String,
    default: String,
    kind: Kind,
    resolved: bool,
}

impl Read {
    fn is_secret(&self) -> bool {
        let up = self.var.to_ascii_uppercase();
        ["KEY", "TOKEN", "SECRET", "PASSWORD", "CREDENTIAL", "PASSWD"]
            .iter()
            .any(|s| up.contains(s))
    }

    fn covered(&self, c: &Contract) -> bool {
        match self.kind {
            Kind::NotARead => true,
            Kind::Dynamic => self.resolved || c.has_suffix_wildcard(&self.guild),
            Kind::Literal | Kind::ResolvedConst | Kind::Membership => c.covered(&self.guild, &self.var),
        }
    }
}

fn classify_key(raw: &str, constants: &BTreeMap<String, String>) -> (String, Kind, bool) {
    let raw = raw.trim();
    if raw.len() >= 2 {
        let first = raw.chars().next().unwrap_or(' ');
        if (first == '"' || first == '\'') && raw.ends_with(first) && raw.len() > 2 {
            return (raw[1..raw.len() - 1].to_string(), Kind::Literal, true);
        }
    }
    if let Some(rest) = raw.strip_prefix("f\"").or_else(|| raw.strip_prefix("f'"))
        && !rest.contains('{')
    {
        let quote = raw.chars().nth(1).unwrap_or('"');
        if let Some(end) = rest.rfind(quote) {
            return (rest[..end].to_string(), Kind::Literal, true);
        }
    }
    // identificador simple -> constante literal del fichero
    if let Some(name) = constants.get(raw) {
        return (name.clone(), Kind::ResolvedConst, true);
    }
    (format!("<dinamico: {raw}>"), Kind::Dynamic, false)
}

fn scan_file(path: &Path, rel: &str, guild: &str, out: &mut Vec<Read>) {
    let content = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", path.display()));
    let lines: Vec<String> = content.lines().map(strip_comment).collect();
    let constants = file_constants(&lines);
    for i in 0..lines.len() {
        let line = &lines[i];
        if !line.contains("environ") && !line.contains("getenv") {
            continue;
        }
        let mut handled = false;
        // setdefault / copy / forward: no son lecturas del kernel
        if line.contains(".setdefault(")
            || line.contains(".copy()")
            || line.contains("**os.environ")
            || line.contains("**_os.environ")
            || line.contains("dict(os.environ")
        {
            out.push(Read {
                file: rel.to_string(),
                line: i + 1,
                guild: guild.to_string(),
                var: "<no-lectura>".to_string(),
                default: String::new(),
                kind: Kind::NotARead,
                resolved: true,
            });
            handled = true;
        }
        // .get / getenv
        for marker in [".environ.get(", ".getenv("] {
            let mut offset = 0usize;
            while let Some(pos) = line[offset..].find(marker) {
                let at = offset + pos;
                let open = at + marker.len();
                let raw = first_arg_text(&lines, i, open);
                let default = rest_args(&lines, i, open);
                let (var, kind, resolved) = classify_key(&raw, &constants);
                out.push(Read {
                    file: rel.to_string(),
                    line: i + 1,
                    guild: guild.to_string(),
                    var,
                    default,
                    kind,
                    resolved,
                });
                handled = true;
                offset = at + marker.len();
            }
        }
        // indexado
        if let Some(pos) = line.find(".environ[") {
            let open = pos + ".environ[".len();
            let raw = first_arg_text(&lines, i, open);
            let (var, kind, resolved) = classify_key(&raw, &constants);
            out.push(Read {
                file: rel.to_string(),
                line: i + 1,
                guild: guild.to_string(),
                var,
                default: String::new(),
                kind: if kind == Kind::Literal {
                    Kind::Literal
                } else {
                    kind
                },
                resolved,
            });
            handled = true;
        }
        // pertenencia "X" in os.environ
        if line.contains(" in ")
            && (line.contains("os.environ") || line.contains("_os.environ"))
            && let Some(t) = line.split(" in ").next()
        {
            let t = t.trim_end();
            if let Some(quote) = t.chars().next_back()
                && (quote == '"' || quote == '\'')
            {
                let inner = &t[..t.len() - 1];
                if let Some(start) = inner.rfind(quote) {
                    let var = inner[start + 1..].to_string();
                    out.push(Read {
                        file: rel.to_string(),
                        line: i + 1,
                        guild: guild.to_string(),
                        var,
                        default: String::new(),
                        kind: Kind::Membership,
                        resolved: true,
                    });
                    handled = true;
                }
            }
        }
        if !handled {
            // bare os.environ (iteración / items / dict): solo ve lo filtrado
            out.push(Read {
                file: rel.to_string(),
                line: i + 1,
                guild: guild.to_string(),
                var: "<no-lectura>".to_string(),
                default: String::new(),
                kind: Kind::NotARead,
                resolved: true,
            });
        }
    }
}

fn py_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n == "__pycache__")
            {
                continue;
            }
            py_files(&path, out);
        } else if path.extension().is_some_and(|x| x == "py") {
            out.push(path);
        }
    }
}

fn scan_guilds() -> Vec<Read> {
    let guilds = repo_root().join("guilds");
    let mut files = Vec::new();
    py_files(&guilds, &mut files);
    files.sort();
    let mut reads = Vec::new();
    for path in files {
        let rel = path
            .strip_prefix(&guilds)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let guild = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string();
        scan_file(&path, &format!("guilds/{rel}"), &guild, &mut reads);
    }
    reads.sort_by(|a, b| {
        (&a.file, a.line, &a.var).cmp(&(&b.file, b.line, &b.var))
    });
    reads
}

#[test]
fn guild_env_reads_are_covered_by_d34() {
    let reads = scan_guilds();
    assert!(
        reads.len() >= 30,
        "solo {} lecturas catalogadas: el barrido de guilds/ parece roto",
        reads.len()
    );
    let Some(contract) = contract_from_source() else {
        eprintln!(
            "D-34 no integrado todavía en guild_process.rs (falta `guild_inherited_env`); \
             la guardia se activa cuando llegue el push de D-34. Lecturas catalogadas: {}.",
            reads.len()
        );
        return;
    };

    let mut uncovered: Vec<&Read> = Vec::new();
    println!(
        "{:<40} {:<44} {:<18} {:<8} cubierta",
        "variable", "fichero:línea", "default", "secreto"
    );
    for r in &reads {
        if r.kind == Kind::NotARead {
            continue;
        }
        let cov = r.covered(&contract);
        println!(
            "{:<40} {:<44} {:<18} {:<8} {}",
            r.var,
            format!("{}:{}", r.file, r.line),
            if r.default.is_empty() {
                "-".to_string()
            } else {
                r.default.replace('\n', " ")
            },
            if r.is_secret() { "SÍ" } else { "no" },
            if cov { "sí" } else { "NO" }
        );
        if !cov {
            uncovered.push(r);
        }
    }

    println!("\ncontrato D-34: {} exactas, {} prefijos, {} guilds con grant",
        contract.exact.len(),
        contract.prefixes.len(),
        contract.grants.len()
    );

    assert!(
        uncovered.is_empty(),
        "{} lecturas de entorno NO cubiertas por D-34 (ni por la lista de excepciones del test):\n{:#?}\n\
         Arregla la guild o amplía explícitamente el contrato D-34 (`guild_inherited_env`/`guild_may_inherit`). \
         No añadas la variable a la lista blanca sin revisarla.",
        uncovered.len(),
        uncovered
    );
}
