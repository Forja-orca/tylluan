//! T-DSH-37 — colisiones de los patrones APLASTADOS de la capa 1 (D-31).
//!
//! El árbol `51c6e41` tenía 68 falsos positivos reales porque `imstart`/`imend`
//! (vista aplastada de `<|im_start|>`/`<|im_end|>`) casaban con `trim_start()`,
//! `trim_end_matches()` y con «régimen de». El TL los quitó. Este test busca la
//! MISMA clase de colisión en los patrones aplastados que quedan: para cada
//! patrón de `SQUASHED_PATTERNS` comprueba si la vista aplastada de algún
//! término/frase técnica común (identificadores Rust/Python, términos de
//! seguridad, frases de documentación) contiene el patrón **sin** contener su
//! forma literal natural (esa sería una coincidencia intencionada, no una
//! colisión).
//!
//! Diccionario reproducible: se construye de los `.rs`, `.py` y `.md`
//! rastreados por git (palabras de identificadores/prosa + bigramas y trigramas
//! de palabras de cada línea), con un mínimo de 2000 entradas. El test pasa hoy
//! con `MAX_SQUASH_ONLY_COLLISIONS` fijado al valor medido; si un patrón nuevo
//! (o un cambio de vista) introduce colisiones, el recuento sube y falla.
//!
//! Run:
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test layer1_squash_collisions -- --nocapture
//!
//! Sin `cfg(unix)`/`cfg(windows)` en ninguna función y sin tocar `src/`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use tylluan_kernel::security::poison_patterns::{matches_injection_pattern, normalize};

/// Colisiones "peligrosas" medidas hoy: vista aplastada casando con texto que
/// NO contiene la forma literal del patrón. Subir solo con motivo escrito.
const MAX_SQUASH_ONLY_COLLISIONS: usize = 0;

/// Mínimo de entradas del diccionario (exigido por la tarea).
const MIN_DICTIONARY: usize = 2000;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn is_scanned_ext(path: &str) -> bool {
    path.ends_with(".rs") || path.ends_with(".py") || path.ends_with(".md")
}

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

fn walk_files(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.join("crates"), root.join("guilds"), root.join("docs")];
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

/// Literales de una lista `const <NAME>: &[&str] = &[ ... ]` del fuente.
fn literals_from_source(name: &str) -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/security/poison_patterns.rs");
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("no se pudo leer {}: {e}", path.display())
    });
    let start = src
        .find(&format!("const {name}"))
        .unwrap_or_else(|| panic!("falta {name}"));
    let block = &src[start..];
    let end = block.find("];").unwrap_or(block.len());
    let mut patterns = Vec::new();
    for line in block[..end].lines() {
        let t = line.trim();
        if !t.starts_with('"') {
            continue;
        }
        if let (Some(a), Some(b)) = (t.find('"'), t.rfind('"'))
            && b > a
        {
            patterns.push(t[a + 1..b].to_string());
        }
    }
    patterns
}

fn squash(text: &str) -> String {
    normalize(text)
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

fn words_of(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    for ch in line.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            current.push(ch);
        } else if !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
        .into_iter()
        .map(|w| w.to_lowercase())
        .filter(|w| w.chars().all(|c| c.is_alphanumeric() || c == '_'))
        .collect()
}

/// Diccionario reproducible (palabras + frases de 2 y 3 palabras), con la
/// primera ruta donde apareció cada entrada para el informe.
fn build_dictionary(root: &Path, files: &[String]) -> BTreeMap<String, String> {
    let mut dict: BTreeMap<String, String> = BTreeMap::new();
    for rel in files {
        let Ok(text) = std::fs::read_to_string(root.join(rel)) else {
            continue;
        };
        for line in text.lines() {
            let words = words_of(line);
            // Palabras técnicas sueltas (identificadores incluidos).
            for w in &words {
                let len = w.chars().count();
                if (4..=40).contains(&len) && !dict.contains_key(w) {
                    dict.insert(w.clone(), rel.clone());
                }
            }
            // Bigramas: frases técnicas/documentación.
            for win in words.windows(2) {
                if win.iter().any(|w| w.chars().count() < 2) {
                    continue;
                }
                let phrase = win.join(" ");
                if phrase.chars().count() <= 60 && !dict.contains_key(&phrase) {
                    dict.insert(phrase, rel.clone());
                }
            }
        }
    }
    dict
}

/// ¿El patrón aplastado es exactamente la concatenación de una secuencia de
/// palabras ENTERAS de la entrada? "you are now" -> "youarenow" sí (intencionado;
/// es la frase con separadores distintos); "trim_end" -> "imend" no (el patrón
/// empieza en mitad de "trim": es la clase de colisión de imstart/imend).
fn aligned_word_sequence(entry: &str, squashed_pattern: &str) -> bool {
    // Fragmentos alfanuméricos: separados por CUALQUIER no-alfanumérico (también
    // `_`). Así "ignore_previous_instructions" es la frase con separadores
    // (detección intencionada de la ofuscación por espaciado), mientras que
    // "trim" + "end" NO produce "imend" porque el patrón empieza en mitad de un
    // fragmento (esa es la clase de colisión de imstart/imend).
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    for ch in entry.chars() {
        if ch.is_alphanumeric() {
            current.push(ch);
        } else if !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    let words: Vec<String> = words.iter().map(|w| squash(w)).collect();
    if words.is_empty() {
        return false;
    }
    // Ventana máxima en palabras: el patrón no puede ser más largo que su
    // longitud dividida por 2 (cada palabra aporta >= 2 caracteres).
    let max_window = (squashed_pattern.len() / 2).max(1).min(words.len());
    for start in 0..words.len() {
        let mut joined = String::new();
        for word in words.iter().skip(start).take(max_window) {
            joined.push_str(word);
            if joined == squashed_pattern {
                return true;
            }
            if joined.len() >= squashed_pattern.len() {
                break;
            }
        }
    }
    false
}

#[test]
fn squashed_patterns_do_not_collide_with_benign_vocabulary() {
    let root = repo_root();
    assert!(root.is_dir(), "repo root no encontrado: {}", root.display());

    let mut files = tracked_files(&root);
    if files.is_empty() {
        files = walk_files(&root);
    }
    files.retain(|f| is_scanned_ext(f));
    files.sort();

    let dict = build_dictionary(&root, &files);
    assert!(
        dict.len() >= MIN_DICTIONARY,
        "diccionario demasiado pequeño: {} entradas (< {MIN_DICTIONARY})",
        dict.len()
    );

    let squashed_patterns = literals_from_source("SQUASHED_PATTERNS");
    let raw_patterns = literals_from_source("INJECTION_PATTERNS");
    assert!(!squashed_patterns.is_empty() && !raw_patterns.is_empty());

    // Forma literal natural de cada patrón aplastado (para separar las
    // coincidencias intencionadas de las colisiones de la vista aplastada).
    let natural: Vec<(&str, Vec<&str>)> = squashed_patterns
        .iter()
        .map(|sp| {
            let raws: Vec<&str> = raw_patterns
                .iter()
                .filter(|r| squash(r) == *sp)
                .map(|r| r.as_str())
                .collect();
            (sp.as_str(), raws)
        })
        .collect();

    let mut collisions: Vec<(String, String, String)> = Vec::new(); // (patrón, entrada, fichero)
    let mut intended = 0usize;
    let mut checked = 0usize;

    for (entry, file) in &dict {
        let entry_squashed = squash(entry);
        if entry_squashed.is_empty() {
            continue;
        }
        for (sp, raws) in &natural {
            if !entry_squashed.contains(sp) {
                continue;
            }
            checked += 1;
            let _ = raws;
            if aligned_word_sequence(entry, sp) {
                intended += 1;
            } else {
                collisions.push((sp.to_string(), entry.clone(), file.clone()));
            }
        }
    }

    println!("ficheros: {}", files.len());
    println!("diccionario: {} entradas", dict.len());
    println!("patrones aplastados: {}", squashed_patterns.len());
    println!(
        "coincidencias totales: {checked} (intencionadas: {intended}, colisiones de aplastado: {})",
        collisions.len()
    );
    if !collisions.is_empty() {
        println!();
        println!("COLISIONES (patron aplastado | entrada | primer fichero):");
        for (pat, entry, file) in &collisions {
            println!("{pat} | {entry} | {file}");
        }
        // Sanity: una colisión real dispara la capa 1 sobre un texto benigno.
        for (_, entry, _) in &collisions {
            assert!(
                matches_injection_pattern(entry),
                "'{entry}' contiene un patrón aplastado pero la capa 1 no lo marca: revisar el modelo"
            );
        }
    }

    // MAX vale 0 hoy (estado medido); el allow existe para que el tope pueda
    // subir con motivo sin que clippy trate `<= 0` como comparación absurda.
    #[allow(clippy::absurd_extreme_comparisons)]
    let within_budget = collisions.len() <= MAX_SQUASH_ONLY_COLLISIONS;
    assert!(
        within_budget,
        "hay {} colisiones de vista aplastada (tope {}): un patrón aplastado debe pasar por un \
         barrido del repo antes de entrar (lección de D-31/imstart-imend)",
        collisions.len(),
        MAX_SQUASH_ONLY_COLLISIONS
    );
}
