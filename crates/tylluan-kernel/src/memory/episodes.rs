//! T-DSH-165 (F1): registro fiel de episodios, append-only (primer escalon).
//!
//! Cada llamada a las herramientas soberanas `tylluan_do`, `tylluan_remember`,
//! `tylluan_recall` y `tylluan_think` deja DOS filas en `episodes.db` (peticion
//! y respuesta), escritas ANTES de devolver la respuesta al cliente:
//!
//! - `episodes.db` vive en el DATA_DIR del kernel, en su propia base SQLite.
//! - El contenido pasa por la redaccion de secretos existente
//!   (`crate::logging::scrub`) ANTES de guardarse; nunca se inventa otro scrub.
//! - Las respuestas de herramienta > 64 KB se guardan como hash + primeros 4 KB.
//! - Cada fila encadena `prev_hash` con el `content_hash` de la fila anterior de
//!   la misma sesion (cadena verificable con [`EpisodeLog::verify_chain`]).
//! - Triggers SQLite rechazan cualquier `UPDATE` y `DELETE` (sin retencion: no
//!   se borra nada).
//!
//! Sin herramientas MCP nuevas ni cambios de contrato: el registro es interno.

use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Mutex;

/// Nombre del fichero dentro del DATA_DIR (debe estar en `DATA_DBS` del CLI).
pub const EPISODES_DB_NAME: &str = "episodes.db";

/// A partir de este tamano (bytes) una respuesta se guarda truncada.
pub const MAX_RESPONSE_BYTES: usize = 64 * 1024;
/// Bytes que se conservan de una respuesta truncada.
pub const TRUNCATED_KEEP_BYTES: usize = 4 * 1024;
/// `prev_hash` de la primera fila de una sesion.
pub const GENESIS_HASH: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

const SCHEMA_SQL: &str = "
CREATE TABLE IF NOT EXISTS episodes (
    id               TEXT PRIMARY KEY,
    session_id       TEXT NOT NULL,
    seq              INTEGER NOT NULL,
    agent_id         TEXT NOT NULL,
    tool             TEXT NOT NULL,
    role             TEXT NOT NULL CHECK (role IN ('request', 'response')),
    content_redacted TEXT NOT NULL,
    content_hash     TEXT NOT NULL,
    prev_hash        TEXT NOT NULL,
    created_at       TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (session_id, seq)
);
CREATE INDEX IF NOT EXISTS idx_episodes_session_seq ON episodes (session_id, seq);
CREATE TRIGGER IF NOT EXISTS episodes_no_update
BEFORE UPDATE ON episodes
BEGIN
    SELECT RAISE(ABORT, 'episodes es append-only: UPDATE prohibido');
END;
CREATE TRIGGER IF NOT EXISTS episodes_no_delete
BEFORE DELETE ON episodes
BEGIN
    SELECT RAISE(ABORT, 'episodes es append-only: DELETE prohibido');
END;
";

fn sha256_hex(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    let mut out = String::with_capacity(64);
    for b in &digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Redaccion existente del kernel (`logging::scrub`): enmascara secretos.
fn redact(content: &str) -> String {
    crate::logging::scrub(content)
}

/// Aplica el recorte de respuestas largas: >64 KB -> hash del original + 4 KB.
/// Devuelve (contenido a guardar, hash logico del contenido original).
fn prepare_content(role: &str, redacted: &str) -> (String, String) {
    let full_hash = sha256_hex(redacted);
    if role == "response" && redacted.len() > MAX_RESPONSE_BYTES {
        let cut: String = redacted.chars().take(TRUNCATED_KEEP_BYTES).collect();
        let stored = format!(
            "{cut}\n[truncated sha256={full_hash} bytes={}]",
            redacted.len()
        );
        let mut hasher = Sha256::new();
        hasher.update(stored.as_bytes());
        let stored_hash: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        (stored, stored_hash)
    } else {
        (redacted.to_string(), full_hash)
    }
}

/// Registro append-only de episodios.
pub struct EpisodeLog {
    conn: Mutex<Connection>,
}

impl EpisodeLog {
    /// Abre (o crea) `episodes.db` en `path`.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("abriendo {}", path.display()))?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        let _ = conn.pragma_update(None, "synchronous", "NORMAL");
        Self::init_schema(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Abre un registro en memoria (tests).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::init_schema(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    fn init_schema(conn: &Connection) -> Result<()> {
        conn.execute_batch(SCHEMA_SQL).context("creando esquema episodes")?;
        Ok(())
    }

    /// Anade una fila al registro. `role` debe ser `"request"` o `"response"`.
    ///
    /// El `seq` es por sesion (1, 2, 3...) y `prev_hash` enlaza con el
    /// `content_hash` de la fila anterior de la misma sesion.
    pub fn append(
        &self,
        session_id: &str,
        agent_id: &str,
        tool: &str,
        role: &str,
        content: &str,
    ) -> Result<String> {
        if role != "request" && role != "response" {
            bail!("role invalido para episodes: '{role}' (request|response)");
        }
        let redacted = redact(content);
        let (stored, content_hash) = prepare_content(role, &redacted);
        let mut conn = self.conn.lock().expect("episodes mutex poisoned");
        let tx = conn.transaction()?;
        let seq: i64 = tx.query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM episodes WHERE session_id = ?1",
            params![session_id],
            |r| r.get(0),
        )?;
        let prev_hash: String = tx
            .query_row(
                "SELECT content_hash FROM episodes WHERE session_id = ?1 ORDER BY seq DESC LIMIT 1",
                params![session_id],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or_else(|| GENESIS_HASH.to_string());
        let id = format!("episode:{}", uuid::Uuid::new_v4());
        tx.execute(
            "INSERT INTO episodes (id, session_id, seq, agent_id, tool, role, content_redacted, content_hash, prev_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![id, session_id, seq, agent_id, tool, role, stored, content_hash, prev_hash],
        )?;
        tx.commit()?;
        Ok(id)
    }

    /// Numero total de filas.
    pub fn count(&self) -> Result<i64> {
        let conn = self.conn.lock().expect("episodes mutex poisoned");
        Ok(conn.query_row("SELECT COUNT(*) FROM episodes", [], |r| r.get(0))?)
    }

    /// Filas de una sesion (para tests/verificacion).
    pub fn rows_for_session(&self, session_id: &str) -> Result<Vec<(i64, String, String)>> {
        let conn = self.conn.lock().expect("episodes mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT seq, role, content_redacted FROM episodes WHERE session_id = ?1 ORDER BY seq ASC",
        )?;
        let rows = stmt
            .query_map(params![session_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Verifica la cadena de hashes: cada `content_hash` debe coincidir con el
    /// contenido guardado y cada `prev_hash` con el `content_hash` anterior de
    /// su sesion. Devuelve error con la fila concreta si algo no cuadra.
    pub fn verify_chain(&self) -> Result<()> {
        let conn = self.conn.lock().expect("episodes mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT session_id, seq, content_redacted, content_hash, prev_hash
             FROM episodes ORDER BY session_id ASC, seq ASC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut last_session: Option<String> = None;
        let mut expected_prev = GENESIS_HASH.to_string();
        for (session, seq, content, hash, prev) in rows {
            if last_session.as_deref() != Some(session.as_str()) {
                expected_prev = GENESIS_HASH.to_string();
                last_session = Some(session.clone());
            }
            if prev != expected_prev {
                bail!(
                    "cadena rota en sesion '{session}' seq {seq}: prev_hash={prev} esperado={expected_prev}"
                );
            }
            let recomputed = sha256_hex(&content);
            if recomputed != hash {
                bail!(
                    "fila alterada en sesion '{session}' seq {seq}: content_hash no coincide con el contenido"
                );
            }
            expected_prev = hash;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log() -> EpisodeLog {
        EpisodeLog::open_in_memory().unwrap()
    }

    #[test]
    fn ten_calls_produce_ten_rows() {
        // (1) 10 llamadas = 10 filas (5 peticiones + 5 respuestas).
        let ep = log();
        for i in 0..5 {
            ep.append("s1", "agent", "tylluan_do", "request", &format!("peticion {i}"))
                .unwrap();
            ep.append("s1", "agent", "tylluan_do", "response", &format!("respuesta {i}"))
                .unwrap();
        }
        assert_eq!(ep.count().unwrap(), 10);
        let rows = ep.rows_for_session("s1").unwrap();
        assert_eq!(rows.len(), 10);
        assert_eq!(rows.first().unwrap().0, 1, "seq empieza en 1");
        assert_eq!(rows.last().unwrap().0, 10);
    }

    #[test]
    fn update_and_delete_are_rejected_by_triggers() {
        // (2) UPDATE y DELETE rechazados por triggers SQLite.
        let ep = log();
        ep.append("s1", "agent", "tylluan_think", "request", "hola").unwrap();
        // SQL crudo por la misma conexion del registro.
        let conn = Connection::open_in_memory().unwrap();
        drop(conn);
        {
            let conn = ep.conn.lock().unwrap();
            let upd = conn.execute(
                "UPDATE episodes SET content_redacted = 'alterado' WHERE session_id = 's1'",
                [],
            );
            assert!(upd.is_err(), "UPDATE debe fallar: {upd:?}");
            let del = conn.execute("DELETE FROM episodes WHERE session_id = 's1'", []);
            assert!(del.is_err(), "DELETE debe fallar: {del:?}");
        }
        assert_eq!(ep.count().unwrap(), 1, "la fila sigue ahi");
    }

    #[test]
    fn secrets_are_redacted_before_storage() {
        // (3) un secreto de prueba no aparece en claro.
        let ep = log();
        let secret = "sk-DSH165superSecretToken1234567890";
        ep.append(
            "s1",
            "agent",
            "tylluan_remember",
            "request",
            &format!("guarda esto: {secret} y tambien Bearer abcdef123456"),
        )
        .unwrap();
        let rows = ep.rows_for_session("s1").unwrap();
        let stored = &rows[0].2;
        assert!(!stored.contains(secret), "el sk- no debe quedar en claro: {stored}");
        assert!(
            !stored.contains("abcdef123456"),
            "el Bearer no debe quedar en claro: {stored}"
        );
        assert!(stored.contains("scrubbed"), "debe llevar la marca de redaccion: {stored}");
    }

    #[test]
    fn hash_chain_verifies_and_detects_tampering() {
        // (4) la cadena verifica y detecta una fila alterada.
        let ep = log();
        for i in 0..3 {
            ep.append("s1", "agent", "tylluan_recall", "request", &format!("q{i}")).unwrap();
            ep.append("s1", "agent", "tylluan_recall", "response", &format!("r{i}")).unwrap();
        }
        ep.verify_chain().expect("cadena intacta");

        // Saltarse el trigger SOLO en el test para simular una base manipulada:
        {
            let conn = ep.conn.lock().unwrap();
            conn.execute_batch("DROP TRIGGER episodes_no_update;").unwrap();
            conn.execute(
                "UPDATE episodes SET content_redacted = 'manipulado' WHERE session_id = 's1' AND seq = 2",
                [],
            )
            .unwrap();
        }
        let err = ep.verify_chain().expect_err("la manipulacion debe detectarse");
        let msg = err.to_string();
        assert!(
            msg.contains("content_hash no coincide") || msg.contains("prev_hash"),
            "mensaje de deteccion inesperado: {msg}"
        );
    }

    #[test]
    fn reopening_does_not_duplicate_or_break_the_chain() {
        // (5) reiniciar no duplica: cerrar y reabrir conserva filas y sigue la cadena.
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(EPISODES_DB_NAME);
        {
            let ep = EpisodeLog::open(&path).unwrap();
            ep.append("s1", "agent", "tylluan_do", "request", "uno").unwrap();
            assert_eq!(ep.count().unwrap(), 1);
        }
        {
            let ep = EpisodeLog::open(&path).unwrap();
            assert_eq!(ep.count().unwrap(), 1, "reabrir no debe duplicar");
            ep.append("s1", "agent", "tylluan_do", "response", "dos").unwrap();
            assert_eq!(ep.count().unwrap(), 2);
            ep.verify_chain().expect("cadena sigue valida tras reinicio");
        }
    }

    #[test]
    fn long_responses_are_stored_as_hash_plus_first_4kb() {
        let ep = log();
        // Texto normal (sin runs base64/hex que el scrub enmascararia): asi el
        // contenido redactado sigue superando el umbral de 64 KB.
        let big = "respuesta normal sin secretos. ".repeat(3000);
        ep.append("s1", "agent", "tylluan_think", "response", &big).unwrap();
        let rows = ep.rows_for_session("s1").unwrap();
        let stored = &rows[0].2;
        assert!(
            stored.len() < MAX_RESPONSE_BYTES,
            "la respuesta larga debe truncarse, len={}",
            stored.len()
        );
        assert!(stored.contains("[truncated sha256="), "falta el hash del original");
        assert!(
            stored.len() >= TRUNCATED_KEEP_BYTES,
            "debe conservar los primeros 4 KB"
        );
        ep.verify_chain().expect("la cadena no depende del original truncado");
    }
}
