//! T-DSH-171 — reparacion explicita del indice FTS de SilvaDB.
//!
//! `tylluan-cli repair-fts --db <silva.db> --yes` hace, en este orden:
//!   1. Inspecciona `nodes` / `nodes_fts` y cuenta huerfanos y sin indexar.
//!   2. Copia de seguridad de la base (VACUUM INTO + manifiesto, igual que
//!      `tylluan-cli backup`). Si el backup falla, ABORTA sin tocar la base.
//!   3. Reconstruye `nodes_fts` (`INSERT ... VALUES('rebuild')`) dentro de una
//!      transaccion y verifica que huerfanos=0 y sin indexar=0. Si la
//!      verificacion falla, hace ROLLBACK: la base queda como estaba.
//!
//! Nunca se ejecuta solo: es un subcomando explicito del operador, sin arranque
//! automatico ni endpoint. La razon esta en T-DSH-170: `DELETE FROM nodes_fts
//! WHERE rowid` es un no-op silencioso en FTS5 de contenido externo cuando el
//! nodo ya no existe; la reparacion fiable es `rebuild`.

use anyhow::{bail, Context, Result};
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};

/// Recuento de coherencia entre `nodes` y `nodes_fts_docsize`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FtsCounts {
    pub nodes: i64,
    pub indexed: i64,
    pub orphan: i64,
    pub missing: i64,
}

impl FtsCounts {
    pub fn is_clean(&self) -> bool {
        self.orphan == 0 && self.missing == 0
    }
}

fn open_ro(db: &Path) -> Result<Connection> {
    Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("abriendo {} en solo lectura", db.display()))
}

fn table_exists(conn: &Connection, name: &str, kind: &str) -> Result<bool> {
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE name = ?1 AND type = ?2",
        rusqlite::params![name, kind],
        |r| r.get(0),
    )?;
    Ok(exists > 0)
}

/// Comprueba que la ruta es una SilvaDB con `nodes` y `nodes_fts`.
pub fn validate_silva_db(db: &Path) -> Result<()> {
    if !db.exists() {
        bail!("no existe la base indicada: {}", db.display());
    }
    let conn = open_ro(db)?;
    if !table_exists(&conn, "nodes", "table")? {
        bail!(
            "{} no parece una silva.db: falta la tabla `nodes`",
            db.display()
        );
    }
    if !table_exists(&conn, "nodes_fts", "table")? {
        bail!(
            "{} no parece una silva.db: falta la tabla `nodes_fts`",
            db.display()
        );
    }
    Ok(())
}

/// Cuenta nodos, docids indexados, huerfanos y nodos sin indexar.
pub fn inspect(db: &Path) -> Result<FtsCounts> {
    validate_silva_db(db)?;
    let conn = open_ro(db)?;
    let nodes: i64 = conn.query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))?;
    let indexed: i64 =
        conn.query_row("SELECT COUNT(*) FROM nodes_fts_docsize", [], |r| r.get(0))?;
    let orphan: i64 = conn.query_row(
        "SELECT COUNT(*) FROM nodes_fts_docsize d \
         WHERE NOT EXISTS (SELECT 1 FROM nodes n WHERE n.rowid = d.id)",
        [],
        |r| r.get(0),
    )?;
    let missing: i64 = conn.query_row(
        "SELECT COUNT(*) FROM nodes n \
         WHERE NOT EXISTS (SELECT 1 FROM nodes_fts_docsize d WHERE d.id = n.rowid)",
        [],
        |r| r.get(0),
    )?;
    Ok(FtsCounts { nodes, indexed, orphan, missing })
}

fn default_backup_dir(db: &Path) -> PathBuf {
    let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let base = db
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join(format!("fts_rebuild_backup_{stamp}"))
}

/// Ejecuta el comando. `yes=false` es un dry-run: informa y no toca nada.
pub fn run_repair_fts(db: &Path, backup_dir: Option<&Path>, yes: bool) -> Result<()> {
    let before = inspect(db)?;
    println!("SilvaDB: {}", db.display());
    println!(
        "Antes:  nodes={} indexados={} huerfanos={} sin_indexar={}",
        before.nodes, before.indexed, before.orphan, before.missing
    );
    if !yes {
        println!(
            "DRY-RUN: no se toca nada. Para reparar de verdad: \
             tylluan-cli repair-fts --db {} --yes",
            db.display()
        );
        return Ok(());
    }
    if before.is_clean() {
        println!("El indice ya esta coherente (huerfanos=0, sin_indexar=0); no hay nada que reparar.");
        return Ok(());
    }

    // 1) Backup ANTES de tocar la base. Si falla, no se ha modificado nada.
    let backup_dir = backup_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_backup_dir(db));
    crate::backup::backup_files(&backup_dir, &[db.to_path_buf()]).with_context(|| {
        format!(
            "backup previo fallido en {}; se ABORTA sin tocar la base",
            backup_dir.display()
        )
    })?;
    println!("Backup previo: {}", backup_dir.display());

    // 2) Rebuild + verificacion en la MISMA transaccion: si no cuadra, rollback.
    let conn = Connection::open(db).with_context(|| format!("abriendo {}", db.display()))?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    {
        let tx = conn
            .unchecked_transaction()
            .context("no se pudo abrir transaccion (otro proceso escribiendo?)")?;
        tx.execute("INSERT INTO nodes_fts(nodes_fts) VALUES('rebuild')", [])
            .context("rebuild de nodes_fts fallido")?;
        let after = {
            let nodes: i64 = tx.query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))?;
            let indexed: i64 =
                tx.query_row("SELECT COUNT(*) FROM nodes_fts_docsize", [], |r| r.get(0))?;
            let orphan: i64 = tx.query_row(
                "SELECT COUNT(*) FROM nodes_fts_docsize d \
                 WHERE NOT EXISTS (SELECT 1 FROM nodes n WHERE n.rowid = d.id)",
                [],
                |r| r.get(0),
            )?;
            let missing: i64 = tx.query_row(
                "SELECT COUNT(*) FROM nodes n \
                 WHERE NOT EXISTS (SELECT 1 FROM nodes_fts_docsize d WHERE d.id = n.rowid)",
                [],
                |r| r.get(0),
            )?;
            FtsCounts { nodes, indexed, orphan, missing }
        };
        if !after.is_clean() {
            drop(tx); // rollback: la base queda intacta (el backup ya esta hecho)
            bail!(
                "verificacion FALLIDA tras rebuild (huerfanos={}, sin_indexar={}); \
                 rollback aplicado, la base NO se ha modificado",
                after.orphan,
                after.missing
            );
        }
        tx.commit().context("commit del rebuild fallido")?;
        println!(
            "Despues: nodes={} indexados={} huerfanos={} sin_indexar={}",
            after.nodes, after.indexed, after.orphan, after.missing
        );
    }
    println!("OK: nodes_fts reconstruido y verificado (huerfanos=0, sin_indexar=0).");
    Ok(())
}
