//! T-DSH-36 — caza de falsos positivos de la capa 1 de D-31 sobre el repositorio.
//!
//! D-31 añadió cláusulas en español muy comunes ("sin restricciones", "sin
//! filtros", "sin censura", "autoridad maxima", "ignora las reglas"…). El corpus
//! benigno de 300 no las agota, así que este test recorre **todos los ficheros
//! rastreados** `.md`, `.py` y `.rs` del repo, parte cada uno en párrafos de
//! hasta 500 caracteres y llama a `matches_injection_pattern` sobre cada uno.
//!
//! Imprime: nº de ficheros, nº de párrafos, nº de aciertos, y por cada acierto
//! fichero:línea, el patrón que casa (atribuido probando cada literal de
//! `INJECTION_PATTERNS`) y un veredicto heurístico. El test **pasa hoy**:
//! fija el recuento en `MAX_HITS` (subirlo solo con motivo) y no falla por
//! aciertos que sean documentación de seguridad que habla de inyecciones.
//!
//! Run:
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test layer1_fp_hunt_repo -- --nocapture
//!
//! Sin `cfg(unix)`/`cfg(windows)` en ninguna función y sin tocar `src/`.

use std::path::{Path, PathBuf};
use std::process::Command;

use tylluan_kernel::security::poison_patterns::{matches_injection_pattern, normalize};

/// Tope de aciertos medido hoy (2026-10-22, con D-31 `51c6e41`): 122 aciertos
/// sobre 794 ficheros / 37.301 párrafos. De ellos 68 son FALSOS POSITIVOS de
/// `squash:imstart` / `squash:imend` (casan con `trim_start`/`trim_end` y con
/// «régimen de»); el informe T-DSH-36 propone quitarlos de SQUASHED_PATTERNS,
/// lo que bajaría el tope a 54. Subirlo solo con motivo escrito; bajarlo al
/// recortar un patrón.
///
/// T-DSH-151 (2026-10-31, base `b9f1331`): 92 aciertos sobre 1.098 ficheros /
/// 46.410 párrafos. Motivo del nuevo tope: el repo creció con documentación e
/// informes de seguridad (D-31, T-DSH-36/37, ASI06/redteam, D-80) y tests que
/// contienen los literales de inyección a propósito. Revisada la lista impresa:
/// los 92 son literales de patrón en código/tests/docs (veredictos
/// "literal del propio patrón", "datos/literal de test" o "código/docs"), no
/// prosa benigna nueva. El test sigue cazando un FP real de prosa si aparece.
///
/// T-DSH-151 (2026-10-31, tras push218 `1e021f5`): 98 aciertos sobre 1.107
/// ficheros / 46.673 párrafos. Mismo motivo: push218 integró más informes de
/// seguridad (T-DSH-153/154, D-80, AG-20) y tests que citan los patrones. La
/// lista sigue sin prosa benigna nueva; se fija 98 para que un salto vuelva a
/// pedir revisión.
const MAX_HITS: usize = 98;

/// Mínimo de párrafos analizados para detectar que el escaneo realmente recorrió
/// el repo (si esto baja, el test avisa en vez de pasar en vacío).
const MIN_PARAGRAPHS: usize = 500;

const MAX_PARAGRAPH_CHARS: usize = 500;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn is_scanned_ext(path: &str) -> bool {
    path.ends_with(".md") || path.ends_with(".py") || path.ends_with(".rs")
}

/// Ficheros rastreados por git (NUL-separado). Si git no está (CI mínima),
/// devuelve vacío y se usa el fallback de directorios.
fn tracked_files(root: &Path) -> Vec<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z"])
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .split('\0')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect(),
        _ => Vec::new(),
    }
}

/// Fallback sin git: recorre `docs/`, `guilds/` y `README.md`.
fn walk_files(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.join("docs"), root.join("guilds")];
    if root.join("README.md").is_file() {
        out.push("README.md".to_string());
    }
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with('.') || name == "node_modules" || name == "target" {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(rel) = path.strip_prefix(root) {
                let rel = rel.to_string_lossy().replace('\\', "/");
                if is_scanned_ext(&rel) {
                    out.push(rel);
                }
            }
        }
    }
    out
}

/// Párrafos de hasta `MAX_PARAGRAPH_CHARS`, con la línea donde empiezan.
fn paragraphs(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut line = 1usize;
    let mut current = String::new();
    let mut current_line = 1usize;
    for raw in text.lines() {
        let is_blank = raw.trim().is_empty();
        if is_blank {
            if !current.trim().is_empty() {
                flush_paragraph(&mut current, current_line, &mut out);
                current = String::new();
            }
            line += 1;
            current_line = line;
            continue;
        }
        if current.is_empty() {
            current_line = line;
        } else {
            current.push('\n');
        }
        current.push_str(raw);
        line += 1;
    }
    if !current.trim().is_empty() {
        flush_paragraph(&mut current, current_line, &mut out);
    }
    out
}

fn flush_paragraph(current: &mut String, start_line: usize, out: &mut Vec<(usize, String)>) {
    let chars: Vec<char> = current.chars().collect();
    if chars.is_empty() {
        current.clear();
        return;
    }
    let mut start = 0usize;
    while start < chars.len() {
        let end = (start + MAX_PARAGRAPH_CHARS).min(chars.len());
        let piece: String = chars[start..end].iter().collect();
        out.push((start_line, piece.trim().to_string()));
        start = end;
    }
    current.clear();
}

/// Literales de una lista `const <NAME>: &[&str] = &[ ... ]` del fuente.
/// Se queda con líneas que empiezan por `"` (ignora comentarios y el resto),
/// así una comilla suelta en un comentario no desincroniza el recuento.
fn literals_from_source(name: &str) -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/security/poison_patterns.rs");
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("no se pudo leer {} para atribuir patrones: {e}", path.display())
    });
    let start = src
        .find(&format!("const {name}"))
        .unwrap_or_else(|| panic!("falta {name} en poison_patterns.rs"));
    let block = &src[start..];
    let end = block.find("];").unwrap_or(block.len());
    let mut patterns = Vec::new();
    for line in block[..end].lines() {
        let t = line.trim();
        if !t.starts_with('"') {
            continue;
        }
        // Primera y última comilla de la línea: "literal",
        if let (Some(a), Some(b)) = (t.find('"'), t.rfind('"'))
            && b > a
        {
            patterns.push(t[a + 1..b].to_string());
        }
    }
    patterns
}

/// Vista aplastada de D-31 (solo alfanuméricos) para atribuir frases espaciadas.
fn squash(text: &str) -> String {
    normalize(text)
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Qué literal de la lista casa con este párrafo, replicando la implementación
/// de D-31: primero patrones crudos contra `normalize(texto)`; si no, los
/// patrones de la vista aplastada contra `squash(normalize(texto))`. Devuelve
/// `crudo:<patrón>` o `squash:<patrón>`; `desconocido` solo si la lista del
/// fuente no coincide con lo que hace la función (señal de desincronización).
fn attribute(paragraph: &str, raw: &[String], squashed_patterns: &[String]) -> String {
    let norm = normalize(paragraph);
    for p in raw {
        if norm.contains(&normalize(p)) {
            return format!("crudo:{p}");
        }
    }
    let squashed = squash(paragraph);
    for p in squashed_patterns {
        if squashed.contains(p.as_str()) {
            return format!("squash:{p}");
        }
    }
    "desconocido".to_string()
}

fn verdict(path: &str) -> &'static str {
    if path.contains("poison_patterns.rs") {
        return "literal del propio patrón (código)";
    }
    if path.contains("redteam") || path.contains("corpus") || path.contains("injection") {
        return "datos/literal de test de inyección (legítimo)";
    }
    if path.starts_with("docs/") {
        return "documentación (revisar si es FP real)";
    }
    if path.ends_with(".py") || path.ends_with(".rs") {
        return "código (revisar si es FP real)";
    }
    "revisar"
}

#[test]
fn layer1_fp_hunt_over_tracked_repo() {
    let root = repo_root();
    assert!(root.is_dir(), "repo root no encontrado: {}", root.display());

    let mut files = tracked_files(&root);
    if files.is_empty() {
        files = walk_files(&root);
    }
    files.retain(|f| is_scanned_ext(f));
    files.sort();
    assert!(files.len() > 50, "¿git ls-files vacío? solo {} ficheros", files.len());

    let raw_patterns = literals_from_source("INJECTION_PATTERNS");
    let squashed_patterns = literals_from_source("SQUASHED_PATTERNS");
    assert!(
        !raw_patterns.is_empty() && !squashed_patterns.is_empty(),
        "no se pudieron extraer las listas de patrones del fuente"
    );

    let mut total_paragraphs = 0usize;
    let mut hits: Vec<(String, usize, String, String, &'static str)> = Vec::new();

    for rel in &files {
        let path = root.join(rel);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue; // binario o no UTF-8
        };
        for (line, para) in paragraphs(&text) {
            total_paragraphs += 1;
            if matches_injection_pattern(&para) {
                let snippet: String = para.chars().take(120).collect();
                let pat = attribute(&para, &raw_patterns, &squashed_patterns);
                hits.push((rel.clone(), line, pat, snippet, verdict(rel)));
            }
        }
    }

    println!("ficheros analizados: {}", files.len());
    println!("parrafos analizados: {total_paragraphs}");
    println!("aciertos de capa 1: {}", hits.len());
    if !hits.is_empty() {
        println!();
        println!("ACERTOS (fichero:linea | patron | veredicto | fragmento):");
        for (file, line, pat, snippet, ver) in &hits {
            println!("{file}:{line} | {pat} | {ver} | {}", snippet.replace('\n', " "));
        }
    }

    assert!(
        total_paragraphs >= MIN_PARAGRAPHS,
        "solo {total_paragraphs} parrafos analizados (< {MIN_PARAGRAPHS}); el escaneo no recorrio el repo"
    );
    assert!(
        hits.len() <= MAX_HITS,
        "la capa 1 da {} aciertos sobre el repo y el tope fijado es {MAX_HITS}; \
         revisa la lista impresa y, si son falsos positivos reales, recorta el patron \
         (o sube el tope con motivo en el informe T-DSH-36)",
        hits.len()
    );
}
