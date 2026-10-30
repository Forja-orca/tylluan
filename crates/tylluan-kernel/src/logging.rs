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
use std::sync::{Mutex, OnceLock};

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
// H-54: capa de auditoría LOCAL con contenido FIEL (sin scrub).
//
// Arquitectura (espec T2112 + H-47C):
//   - APAGADA por defecto (`audit_layer=false`). No es un default de "seguro
//     por olvido": es un default de "no escribir contenido fiel a disco
//     sin que el operador lo pida conscientemente".
//   - El contenido fiel SOLO existe en los helpers: `redacted()`,
//     `safe_with()`, `mcp_payload_with()` reciben el texto CRUDO antes de
//     scrub/recorte. Ahí instalamos el hook de auditoría: si la capa está
//     encendida, escribimos el texto crudo a `audit.log` (paralelo, sin
//     scrub). El vivo (tracing) recibe SIEMPRE la versión ya scrubada/len+sha
//     exactamente como hoy (H-47C). No hay un solo writer "dual".
//   - El fichero de auditoría es `audit.log` en `audit_dir` (default
//     `./data/audit`), rotado por tamaño igual que el vivo, con limpieza por
//     edad (`audit_retention_days`) al abrir y al rotar.
//   - `data/` está en `.gitignore` (línea 12): el fichero de auditoría nunca
//     llega al repo público.
//   - Test obligatorio: "la capa de producción nunca contiene texto" → el
//     vivo siempre usa `redacted`/`safe`/`mcp_payload` con su scrub; la
//     auditoría es un ESPEJO paralelo y opcional del texto crudo.
// ─────────────────────────────────────────────────────────────────────────────

use crate::config::LoggingConfig;

/// Limpieza por edad de los `audit.log.N` más antiguos.
fn prune_audit_by_age(dir: &Path, max_files: usize, retention_days: u64) {
    if retention_days == 0 {
        return;
    }
    let now = std::time::SystemTime::now();
    let cutoff = now - std::time::Duration::from_secs(retention_days * 86_400);
    for i in 1..max_files {
        let p = dir.join(format!("audit.log.{i}"));
        if let Ok(meta) = fs::metadata(&p)
            && meta.created().or(meta.modified()).map(|t| t < cutoff).unwrap_or(false)
        {
            let _ = fs::remove_file(&p);
        }
    }
}

/// Crea (o reabre) el writer de auditoría, aplicando la rotación inicial y la
/// limpieza por edad. Devuelve `None` si `audit_layer` está apagada o si la
/// creación falla (no es fatal: el kernel sigue sin ella).
pub fn audit_writer(cfg: &LoggingConfig) -> Option<RotatingFileWriter> {
    if !cfg.audit_layer {
        return None;
    }
    let dir = PathBuf::from(&cfg.audit_dir);
    if let Err(e) = fs::create_dir_all(&dir) {
        tracing::warn!("audit_layer=true pero no se puede crear {}: {e}", cfg.audit_dir);
        return None;
    }
    prune_audit_by_age(&dir, cfg.audit_max_files, cfg.audit_retention_days);
    RotatingFileWriter::open(&dir, "audit.log", cfg.audit_max_bytes(), cfg.audit_max_files)
        .map_err(|e| {
            tracing::warn!("audit_layer=true pero no se puede abrir audit.log: {e}");
        })
        .ok()
}

/// Sink global de auditoría: se inicializa en el primer uso del helper
/// (`redacted`/`safe_with`/`mcp_payload_with`) leyendo `[logging]` de la
/// config. Si `audit_layer=false` el sink es `None` (cero overhead).
fn audit_sink() -> &'static Option<Mutex<RotatingFileWriter>> {
    static SINK: OnceLock<Option<Mutex<RotatingFileWriter>>> = OnceLock::new();
    SINK.get_or_init(|| audit_writer(&redaction_cfg()).map(Mutex::new))
}

/// Escribe una línea de auditoría (texto crudo, JSON compacto). No falla:
/// si el sink está apagado o el lock/contención falla, se ignora silenciosamente.
fn audit_write(canal: &str, text: &str) {
    if let Some(w) = audit_sink().as_ref() {
        let _ = w.lock().ok().map(|mut w| {
            let rec = serde_json::json!({
                "ts": chrono::Utc::now().to_rfc3339(),
                "canal": canal,
                "len": text.chars().count(),
                "text": text
            });
            writeln!(w, "{rec}").ok();
        });
    }
}

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
    audit_write(canal, text);
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
        audit_write(canal, text);
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
    audit_write("mcp_payload", &raw);
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

/// H-54: true si `mtime` es más antigua de `days` días antes de `now`.
/// Clock-skew (mtime > now) → false (conservador: no borra).
pub fn is_expired(mtime: std::time::SystemTime, now: std::time::SystemTime, days: u64) -> bool {
    if days == 0 {
        return false;
    }
    match now.duration_since(mtime) {
        Ok(age) => age > std::time::Duration::from_secs(days * 86_400),
        Err(_) => false,
    }
}

/// H-54: valida que el nombre solicitado sea `audit.log` o `audit.log.N`
/// con N ≤ 9999. Previene traversal.
pub fn valid_audit_name(name: &str) -> bool {
    let re = regex::Regex::new(r"^audit\.log(\.[0-9]{1,4})?$").unwrap();
    re.is_match(name)
}

/// H-54: lista de ficheros de auditoría (name, size, mtime_rfc3339).
pub fn audit_listing(cfg: &LoggingConfig) -> io::Result<Vec<(String, u64, String)>> {
    if !cfg.audit_layer {
        return Ok(Vec::new());
    }
    let dir = PathBuf::from(&cfg.audit_dir);
    let mut out = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let e = entry?;
        let name = e.file_name().to_string_lossy().to_string();
        if !valid_audit_name(&name) {
            continue;
        }
        let meta = e.metadata()?;
        let mtime = meta
            .modified()
            .or_else(|_| meta.created())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let mtime_str = chrono::DateTime::<chrono::Utc>::from(mtime).to_rfc3339();
        out.push((name, meta.len(), mtime_str));
    }
    out.sort_by(|a, b| b.2.cmp(&a.2)); // más recientes primero
    Ok(out)
}

/// H-54: lee el contenido de un fichero de auditoría validado.
/// Devuelve `None` si no existe; error si el nombre no es válido.
pub fn read_audit_file(cfg: &LoggingConfig, name: &str) -> io::Result<Option<String>> {
    if !valid_audit_name(name) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid audit file name"));
    }
    let path = PathBuf::from(&cfg.audit_dir).join(name);
    if !path.exists() {
        return Ok(None);
    }
    fs::read_to_string(&path).map(Some)
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

    // ───────────────────────────────────────────────────────────────────────────
    // H-54 tests: capa de auditoría local
    // ───────────────────────────────────────────────────────────────────────────

    #[test]
    fn audit_defaults_off_no_dir_created() {
        let d = crate::config::LoggingConfig::default();
        assert!(!d.audit_layer);
        assert_eq!(d.audit_dir, "./data/audit");
        assert_eq!(d.audit_max_mb, 20);
        assert_eq!(d.audit_max_files, 10);
        assert_eq!(d.audit_retention_days, 14);
        let p: crate::config::LoggingConfig = serde_json::from_str("{}").unwrap();
        assert!(!p.audit_layer && p.audit_retention_days == 14);
        // Writer devuelve None, no crea directorio
        let w = crate::logging::audit_writer(&p);
        assert!(w.is_none());
    }

    #[test]
    fn is_expired_pure_logic() {
        let now = std::time::SystemTime::now();
        // 31 días → expirado con retención 30
        let old = now - std::time::Duration::from_secs(31 * 86_400);
        assert!(crate::logging::is_expired(old, now, 30));
        // 29 días → NO expirado
        let recent = now - std::time::Duration::from_secs(29 * 86_400);
        assert!(!crate::logging::is_expired(recent, now, 30));
        // days=0 → nunca expirado
        assert!(!crate::logging::is_expired(old, now, 0));
        // mtime en el futuro (clock-skew) → false (conservador)
        let future = now + std::time::Duration::from_secs(86_400);
        assert!(!crate::logging::is_expired(future, now, 30));
    }

    #[test]
    fn valid_audit_name_validation() {
        assert!(crate::logging::valid_audit_name("audit.log"));
        assert!(crate::logging::valid_audit_name("audit.log.1"));
        assert!(crate::logging::valid_audit_name("audit.log.9999"));
        assert!(!crate::logging::valid_audit_name("audit.log.10000")); // 5 dígitos
        assert!(!crate::logging::valid_audit_name("audit.log.1/../secret"));
        assert!(!crate::logging::valid_audit_name("../../etc/passwd"));
        assert!(!crate::logging::valid_audit_name("audit.logx"));
        assert!(!crate::logging::valid_audit_name("other.log"));
    }

    #[test]
    fn audit_layer_writes_raw_audit_prod_stays_redacted() {
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        let cfg = crate::config::LoggingConfig {
            audit_layer: true,
            audit_dir: dir.path().to_string_lossy().to_string(),
            audit_max_mb: 10,
            audit_max_files: 5,
            audit_retention_days: 14,
            ..Default::default()
        };
        // Escritura directa por el sink (sin pasar por helpers: prueba del writer)
        let mut w = crate::logging::audit_writer(&cfg).expect("writer creado");
        let secret = "sk-real-secret-1234567890";
        let rec = serde_json::json!({
            "ts": chrono::Utc::now().to_rfc3339(),
            "canal": "test",
            "len": secret.chars().count(),
            "text": secret
        });
        writeln!(w, "{rec}").unwrap();
        w.flush().unwrap();
        drop(w);
        // El fichero de auditoría contiene el secreto EN CLARO
        let content = std::fs::read_to_string(dir.path().join("audit.log")).unwrap();
        assert!(content.contains("sk-real-secret-1234567890"), "audit contiene secreto crudo: {content}");
        assert!(content.contains("test"));
        // Producción: redacted() sigue devolviendo len+sha (sin contenido)
        let prod = crate::logging::redacted("test", secret);
        assert!(!prod.contains("sk-rea...7890"), "producción no puede contener el crudo: {prod}");
        assert!(re_shape().is_match(&prod));
        // safe_with (default cfg sin log_content) → producción redacted
        let safe_prod = crate::logging::safe_with(&crate::config::LoggingConfig::default(), "test", secret);
        assert!(!safe_prod.contains("sk-rea...7890"), "safe_with default no puede contener el crudo: {safe_prod}");
        assert!(re_shape().is_match(&safe_prod));
        // mcp_payload default → sin content=
        let payload = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"arguments":{"intent": secret}}});
        let mcp_prod = crate::logging::mcp_payload_with(&crate::config::LoggingConfig::default(), &payload);
        assert!(!mcp_prod.contains("content="));
        assert!(mcp_prod.contains("method=tools/call"));
    }

    #[test]
    fn audit_listing_and_read_end_to_end() {
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        let cfg = crate::config::LoggingConfig {
            audit_layer: true,
            audit_dir: dir.path().to_string_lossy().to_string(),
            audit_max_mb: 10,
            audit_max_files: 5,
            audit_retention_days: 14,
            ..Default::default()
        };
        let mut w = crate::logging::audit_writer(&cfg).unwrap();
        let rec1 = serde_json::json!({"ts":"2026-10-30T00:00:00Z","canal":"a","len":5,"text":"hello"});
        let rec2 = serde_json::json!({"ts":"2026-10-30T00:00:01Z","canal":"b","len":5,"text":"world"});
        writeln!(w, "{rec1}").unwrap();
        writeln!(w, "{rec2}").unwrap();
        w.flush().unwrap();
        drop(w);
        // listing
        let list = crate::logging::audit_listing(&cfg).unwrap();
        // Solo existe el fichero vivo: la rotacion es por tamano (10 MB) y dos
        // registros no la disparan. Lo que se prueba es que el listing ve Y
        // nombra el fichero real, no un numero arbitrario de entradas.
        assert_eq!(list.len(), 1, "listing: {list:?}");
        assert_eq!(list[0].0, "audit.log"); // live file
        assert!(list[0].1 > 0, "tamano registrado en listing");
        // read
        let content = crate::logging::read_audit_file(&cfg, "audit.log").unwrap().unwrap();
        assert!(content.contains("hello"));
        assert!(content.contains("world"));
        // nombre inválido → error
        assert!(crate::logging::read_audit_file(&cfg, "audit.log.1/../etc").is_err());
        // inexistente → None
        let none = crate::logging::read_audit_file(&cfg, "audit.log.999").unwrap();
        assert!(none.is_none());
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
