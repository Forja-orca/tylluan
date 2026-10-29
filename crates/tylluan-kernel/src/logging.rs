//! Rotación de logs por tamaño (D-4).
//!
//! `./logs/kernel.log` crecía sin límite (1,3 GB observados en un kernel real).
//! Este writer rota por tamaño y conserva un número acotado de ficheros:
//!
//! ```text
//! kernel.log        ← vivo (se escribe aquí)
//! kernel.log.1      ← el anterior
//! kernel.log.2      ← el de antes
//! ...
//! kernel.log.{max_files-1}
//! ```
//!
//! Reglas:
//! - **Nunca parte una línea**: si una entrada no cabe en el hueco que queda, se
//!   escribe entera en el fichero actual y la rotación ocurre en la siguiente
//!   escritura. Un registro de más de `max_bytes` vive solo en su fichero.
//! - Un fichero que ya nace por encima del límite (el caso real de 1,3 GB) se
//!   rota al abrirlo, antes de escribir nada nuevo.
//! - Si la rotación falla (permisos, fichero bloqueado en Windows), el writer
//!   **sigue escribiendo** en el fichero actual y cuenta el fallo: preferimos un
//!   log demasiado grande a perder líneas.
//! - `max_files = 1` desactiva el historial: se vacía el fichero vivo al rotar.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Writer con rotación por tamaño, pensado para `tracing_subscriber`.
pub struct RotatingFileWriter {
    dir: PathBuf,
    base: String,
    max_bytes: u64,
    max_files: usize,
    file: File,
    written: u64,
    rotation_failures: u64,
}

impl RotatingFileWriter {
    /// Abre (o crea) `dir/base` en modo append con rotación por tamaño.
    ///
    /// `max_bytes = 0` desactiva la rotación (comportamiento histórico). El
    /// límite es aproximado: se comprueba antes de cada escritura, así que un
    /// fichero puede superar `max_bytes` solo por el tamaño de un registro
    /// completo.
    pub fn open(dir: &Path, base: &str, max_bytes: u64, max_files: usize) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let live = dir.join(base);
        let mut writer = Self {
            dir: dir.to_path_buf(),
            base: base.to_string(),
            max_bytes,
            max_files: max_files.max(1),
            file: OpenOptions::new().create(true).append(true).open(&live)?,
            written: fs::metadata(&live).map(|m| m.len()).unwrap_or(0),
            rotation_failures: 0,
        };
        if writer.max_bytes > 0 && writer.written > writer.max_bytes {
            writer.rotate()?;
        }
        Ok(writer)
    }

    /// Tamaño actual del fichero vivo, en bytes.
    pub fn current_len(&self) -> u64 {
        self.written
    }

    /// Rotaciones fallidas desde el arranque. No es fatal: el writer sigue
    /// escribiendo en el fichero actual.
    pub fn rotation_failures(&self) -> u64 {
        self.rotation_failures
    }

    fn path_for(&self, n: usize) -> PathBuf {
        self.dir.join(format!("{}.{n}", self.base))
    }

    fn rotate(&mut self) -> io::Result<()> {
        if self.max_files <= 1 {
            // Truncar sobre el handle abierto no es fiable en Windows; se
            // recrea el fichero y se reabre en modo append.
            let live = self.dir.join(&self.base);
            self.file = File::create(&live)?;
            self.file = OpenOptions::new().append(true).open(&live)?;
            self.written = 0;
            return Ok(());
        }
        let last = self.max_files - 1;
        // Se borra el más antiguo y se desplaza el resto de mayor a menor para
        // no pisar ficheros que aún hay que mover (rename no sobrescribe en
        // Windows).
        let _ = fs::remove_file(self.path_for(last));
        for i in (1..last).rev() {
            let from = self.path_for(i);
            if from.exists() {
                let to = self.path_for(i + 1);
                let _ = fs::remove_file(&to);
                fs::rename(&from, &to)?;
            }
        }
        let live = self.dir.join(&self.base);
        let first = self.path_for(1);
        let _ = fs::remove_file(&first);
        fs::rename(&live, &first)?;
        self.file = OpenOptions::new().create(true).append(true).open(&live)?;
        self.written = 0;
        Ok(())
    }
}

impl Write for RotatingFileWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.max_bytes > 0
            && self.written > 0
            && self.written.saturating_add(buf.len() as u64) > self.max_bytes
            && self.rotate().is_err()
        {
            self.rotation_failures += 1;
        }
        let n = self.file.write(buf)?;
        self.written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// H-47C (diseño §5 B1-B4 en docs/internal/IMPL_H-47A_hermes_2026-10.md):
// helper único de redacción de logs.
//
// Medido en prod (H-47A): 7 fuentes logueaban el texto entero de
// intent/query/payload. Un secreto escrito por un agente en content se
// quedaba raw en disco ~25 días (rotación 50 MiB × 5 a 0,42 MB/h).
//
// Reglas aprobadas por el TL (T2060, «tal cual»):
// - Default `log_content=false`: SOLO `canal=… len=<chars> sha=<sha256[:12]>`.
// - Las 6 fuentes internas usan `redacted()`: jamas contenido, ni con opt-in.
// - Unica fuente con contenido posible: la de entrada (`mcp_payload()`), y
//   solo con opt-in, con field-walk que scrubuta SOLO strings (id/num/bool
//   crudos para correlacionar).
// - Orden obligatorio: scrub ANTES del recorte. Cortar dentro de un secreto
//   filtra su prefijo (el corte ~100 car. del matcher hacia exactamente eso).
// - Mascara sk-/Bearer/token=/JWT/base64-hex≥32 incluso con opt-in.
// - El sha correlaciona las 7 líneas de una misma petición sin contenido:
//   mismo texto → mismo sha.
// ─────────────────────────────────────────────────────────────────────────────

/// Config `[logging]` leida una vez en el primer uso del helper. Editar el
/// fichero en disco NO se aplica con `config::reload()`: el helper fija el
/// valor en el primer uso y exige reinicio (riesgo B6 aprobado en el diseno).
fn redaction_cfg() -> crate::config::LoggingConfig {
    static CFG: OnceLock<crate::config::LoggingConfig> = OnceLock::new();
    CFG.get_or_init(|| {
        crate::config::TylluanConfig::load()
            .map(|c| c.logging)
            .unwrap_or_default()
    })
    .clone()
}

fn scrub_patterns() -> &'static [regex::Regex] {
    static RE: OnceLock<Vec<regex::Regex>> = OnceLock::new();
    RE.get_or_init(|| {
        [
            // JWT primero: si no, la corrida base64 se comería sus segmentos.
            r"[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{4,}",
            r"sk-[A-Za-z0-9_-]{8,}",
            r"(?i)bearer\s+[A-Za-z0-9._+/=-]{6,}",
            r#"(?i)\btoken\b\s*[=:]\s*[^\s"]{4,}"#,
            r"[A-Fa-f0-9]{32,}",
            r"[A-Za-z0-9+/]{32,}={0,2}",
        ]
        .iter()
        .map(|p| regex::Regex::new(p).expect("regex de scrub invalido"))
        .collect()
    })
}

/// Enmascara secretos: cada coincidencia se sustituye entera por
/// `«scrubbed»`, de modo que no queda ningún fragmento interior que una
/// truncación posterior pudiera devolver.
pub fn scrub(text: &str) -> String {
    let mut out = text.to_string();
    for re in scrub_patterns() {
        out = re.replace_all(&out, "\u{ab}scrubbed\u{bb}").into_owned();
    }
    out
}

fn sha12(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(text.as_bytes());
    let mut s = String::with_capacity(12);
    for b in &digest[..6] {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn cap_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect()
}

/// Lo que las 6 fuentes internas deben loguear: NUNCA contenido (B2).
pub fn redacted(canal: &str, text: &str) -> String {
    format!("canal={canal} len={} sha={}", text.chars().count(), sha12(text))
}

/// Fuente de entrada con opt-in (B1): default → len+sha; con
/// `log_content=true` → contenido ya scrubado y recortado (recorte detras
/// del scrub, jamas antes).
pub fn safe(canal: &str, text: &str) -> String {
    safe_with(&redaction_cfg(), canal, text)
}

pub fn safe_with(cfg: &crate::config::LoggingConfig, canal: &str, text: &str) -> String {
    if cfg.log_content {
        format!(
            "canal={canal} content={}",
            cap_chars(&scrub(text), cfg.log_content_max_chars.max(1))
        )
    } else {
        redacted(canal, text)
    }
}

/// Línea para el payload JSON-RPC de MCP: unica fuente con contenido posible
/// (B2/B3). Default: method+id+len+sha. Opt-in: field-walk que scrubuta solo
/// strings; el recorte va detras del scrub completo.
pub fn mcp_payload(payload: &serde_json::Value) -> String {
    mcp_payload_with(&redaction_cfg(), payload)
}

pub fn mcp_payload_with(cfg: &crate::config::LoggingConfig, payload: &serde_json::Value) -> String {
    let method = payload.get("method").and_then(|v| v.as_str()).unwrap_or("?");
    let id = payload
        .get("id")
        .map(|v| v.to_string())
        .unwrap_or_else(|| "-".to_string());
    let raw = payload.to_string();
    let base = format!(
        "[MCP] method={method} id={id} len={} sha={}",
        raw.chars().count(),
        sha12(&raw)
    );
    if !cfg.log_content {
        return base;
    }
    let walked = cap_chars(&scrub_json(payload), cfg.log_content_max_chars.max(1));
    format!("{base} content={walked}")
}

fn scrub_json(value: &serde_json::Value) -> String {
    use serde_json::{Map, Value};
    fn walk(v: &Value) -> Value {
        match v {
            Value::String(s) => Value::String(scrub(s)),
            Value::Array(items) => Value::Array(items.iter().map(walk).collect()),
            Value::Object(map) => {
                let mut out = Map::with_capacity(map.len());
                for (k, val) in map {
                    out.insert(k.clone(), walk(val));
                }
                Value::Object(out)
            }
            other => other.clone(),
        }
    }
    walk(value).to_string()
}

#[cfg(test)]
mod redaction_tests {
    use super::*;

    const SK: &str = "sk-ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    const JWT: &str = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dBjT0kS6mEYxjW7qL3nZ9cQrT1uV8pX4";

    /// Ningún fragmento contiguo de 8 chars del secreto sobrevive.
    fn assert_no_leak(scrubbed: &str, secret: &str) {
        let chars: Vec<char> = secret.chars().collect();
        for w in chars.windows(8) {
            let frag: String = w.iter().collect();
            assert!(!scrubbed.contains(&frag), "fragmento sobrevive: {frag}");
        }
    }

    fn cfg_on() -> crate::config::LoggingConfig {
        crate::config::LoggingConfig {
            log_content: true,
            log_content_max_chars: 200,
            ..Default::default()
        }
    }

    #[test]
    fn scrub_mask_every_pattern_class() {
        let hex = "0123456789abcdef0123456789abcdef01234567";
        let text = format!("clave {SK} y Bearer eyJhbGciOiJIUzI1NiJ9x y token=abcdef12 y {JWT} y hash {hex} fin");
        let out = scrub(&text);
        for secret in [SK, "eyJhbGciOiJIUzI1NiJ9x", "abcdef12", JWT, hex] {
            assert_no_leak(&out, secret);
        }
        assert!(out.contains("\u{ab}scrubbed\u{bb}"));
        assert!(out.contains("clave") && out.contains("fin"));
    }

    #[test]
    fn scrub_runs_before_cap_so_no_prefix_survives() {
        // Secreto cruzando el corte: 90 chars + SK(43) + cola; cap=100.
        let text = format!("{}{SK} fin", "a".repeat(90));
        let out = cap_chars(&scrub(&text), 100);
        assert_no_leak(&out, SK);
    }

    #[test]
    fn internal_sites_never_leak_content_shape_is_len_sha() {
        let line = redacted("do_routing", "contenido secreto aquí");
        assert!(re_shape().is_match(&line), "forma inesperada: {line}");
        assert!(!line.contains("secreto") && !line.contains("aquí"));
    }

    fn re_shape() -> regex::Regex {
        regex::Regex::new(r"^canal=[a-z_]+ len=\d+ sha=[0-9a-f]{12}$").unwrap()
    }

    #[test]
    fn sha_correlates_same_text_across_sites_and_chars_not_bytes() {
        assert_eq!(sha12("mismo intent"), sha12("mismo intent"));
        assert_ne!(sha12("mismo intent"), sha12("mismo intent."));
        let (a, b) = (redacted("matcher", "ñandú"), redacted("recall", "ñandú"));
        let sha = |s: &str| s.split("sha=").nth(1).unwrap().to_string();
        assert_eq!(sha(&a), sha(&b), "mismo texto debe dar mismo sha en otra fuente");
        let line = redacted("matcher", "áéí");
        assert!(line.contains("len=3"), "len debe contar chars: {line}");
    }

    #[test]
    fn mcp_default_hides_content_keeps_correlation_fields() {
        let payload = serde_json::json!({
            "jsonrpc": "2.0", "id": 7, "method": "tools/call",
            "params": {"name": "tylluan_do", "arguments": {"intent": SK}}
        });
        let line = mcp_payload_with(&crate::config::LoggingConfig::default(), &payload);
        assert_no_leak(&line, SK);
        assert!(line.contains("method=tools/call") && line.contains("id=7"));
        assert!(!line.contains("content="));
    }

    #[test]
    fn mcp_opt_in_scrubs_nested_strings_keeps_scalars_and_caps() {
        let payload = serde_json::json!({
            "jsonrpc": "2.0", "id": 7, "method": "tools/call",
            "params": {"arguments": {"content": SK, "limit": 5, "plan": true}}
        });
        let line = mcp_payload_with(&cfg_on(), &payload);
        assert_no_leak(&line, SK);
        assert!(line.contains("tools/call") && line.contains("\"id\":7") && line.contains("\"limit\":5"));
    }

    #[test]
    fn logging_config_defaults_are_redacting() {
        let d = crate::config::LoggingConfig::default();
        assert!(!d.log_content);
        assert_eq!(d.log_content_max_chars, 200);
        let p: crate::config::LoggingConfig = serde_json::from_str("{}").unwrap();
        assert!(!p.log_content && p.log_content_max_chars == 200);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lee todas las líneas del historial, del más antiguo al vivo.
    fn all_lines(dir: &Path, max_files: usize) -> Vec<String> {
        let mut files: Vec<PathBuf> = (1..max_files).rev().map(|i| dir.join(format!("kernel.log.{i}"))).collect();
        files.push(dir.join("kernel.log"));
        let mut out = Vec::new();
        for f in files {
            if let Ok(content) = fs::read_to_string(&f) {
                out.extend(content.lines().map(|l| l.to_string()));
            }
        }
        out
    }

    #[test]
    fn rotates_at_size_and_preserves_every_line() {
        let dir = tempfile::tempdir().unwrap();
        // 100 líneas de ~41 B ≈ 4,1 KB; con max_files=30 la retención (~12 KB)
        // cubre todas, así que la prueba de preservación es exacta.
        let mut w = RotatingFileWriter::open(dir.path(), "kernel.log", 400, 30).unwrap();
        let expected: Vec<String> = (0..100).map(|i| format!("line-{i:03}-{}", "x".repeat(20))).collect();
        for line in &expected {
            writeln!(w, "{line}").unwrap();
        }
        w.flush().unwrap();

        // Rotó: hay más de un fichero y ninguno supera el límite (cada registro
        // es menor que él, así que la comprobación previa basta).
        let mut count = 0;
        for i in 1..30 {
            if dir.path().join(format!("kernel.log.{i}")).exists() {
                count += 1;
            }
        }
        assert!(count >= 1, "expected rotation, saw {count} rotated files");
        for f in std::fs::read_dir(dir.path()).unwrap() {
            let meta = f.unwrap().metadata().unwrap();
            assert!(meta.len() <= 400, "file exceeds max_bytes: {}", meta.len());
        }

        // Sin pérdida y sin líneas partidas, en orden.
        let got = all_lines(dir.path(), 30);
        assert_eq!(got, expected, "every line must survive, intact and in order");
    }

    #[test]
    fn keeps_at_most_max_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = RotatingFileWriter::open(dir.path(), "kernel.log", 200, 3).unwrap();
        for i in 0..200 {
            writeln!(w, "n{i:03}-{}", "y".repeat(18)).unwrap();
        }
        w.flush().unwrap();

        let files: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(files.len(), 3, "live + 2 rotated, saw {files:?}");
        // Las últimas líneas están; las primeras ya no (retención acotada).
        let got = all_lines(dir.path(), 3);
        assert_eq!(got.last().unwrap(), "n199-yyyyyyyyyyyyyyyyyy");
        assert!(!got.contains(&"n000-yyyyyyyyyyyyyyyyyy".to_string()));
    }

    #[test]
    fn oversized_record_is_never_split() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = RotatingFileWriter::open(dir.path(), "kernel.log", 256, 3).unwrap();
        w.write_all(b"small\n").unwrap();
        let big = format!("BIG-{}-END\n", "z".repeat(1000));
        w.write_all(big.as_bytes()).unwrap();
        w.write_all(b"after\n").unwrap();
        w.flush().unwrap();

        let got = all_lines(dir.path(), 3);
        assert!(got.contains(&big.trim_end().to_string()), "the big record must survive intact");
        assert!(got.iter().any(|l| l == "after"));
    }

    #[test]
    fn existing_oversized_file_is_rotated_on_open() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("kernel.log"), "x".repeat(2000)).unwrap();
        let w = RotatingFileWriter::open(dir.path(), "kernel.log", 512, 5).unwrap();
        assert_eq!(w.current_len(), 0, "live file starts empty after startup rotation");
        assert_eq!(fs::metadata(dir.path().join("kernel.log.1")).unwrap().len(), 2000);
        drop(w);
    }

    #[test]
    fn single_file_mode_truncates_instead_of_keeping_history() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = RotatingFileWriter::open(dir.path(), "kernel.log", 64, 1).unwrap();
        w.write_all("a".repeat(60).as_bytes()).unwrap();
        w.write_all("b".repeat(60).as_bytes()).unwrap();
        w.write_all(b"\n").unwrap();
        w.flush().unwrap();
        assert!(!dir.path().join("kernel.log.1").exists());
        let live = fs::read_to_string(dir.path().join("kernel.log")).unwrap();
        assert!(live.len() <= 64, "live file must not grow past the limit: {}", live.len());
    }
}
