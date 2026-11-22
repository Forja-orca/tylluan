use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::fs;
use tracing::{info, warn, error, debug};
use chrono::{Local, Datelike};

/// Manages rotating backups for TylluanNexus databases.
pub struct BackupManager {
    backup_dir: PathBuf,
}

impl Default for BackupManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BackupManager {
    pub fn new() -> Self {
        // D4986 (opcion A del TL): la carpeta de copias sigue al DATA_DIR efectivo.
        // Antes era `data/backups` relativo al cwd, asi que el backup de una instancia
        // con TYLLUAN_DATA_DIR propio caia en el directorio equivocado.
        Self::with_dir(crate::config::data_file("backups"))
    }

    pub fn with_dir(backup_dir: PathBuf) -> Self {
        fs::create_dir_all(&backup_dir).ok();
        Self { backup_dir }
    }

    /// Perform a high-integrity backup of all critical databases.
    /// Uses SQLite's VACUUM INTO for atomic hot backups with WAL integrity,
    /// or simple copy as fallback.
    pub async fn backup_all(&self) -> Result<()> {
        info!("💾 [BackupManager] Starting scheduled brain backup...");
        
        let databases = database_sources();

        let day_of_week = Local::now().weekday().number_from_monday(); // 1-7

        // D4986: decir QUE bases faltan. Antes se saltaban en silencio (`continue`) y un
        // backup vacio parecia un backup correcto — el operador se enteraba el dia que
        // lo necesitaba.
        let mut faltantes: Vec<&str> = Vec::new();

        for (label, src_path) in &databases {
            let src_path = src_path.as_path();
            if !src_path.exists() {
                faltantes.push(label);
                continue;
            }

            let target_filename = format!("{label}_{day_of_week}.db.bak");
            let target_path = self.backup_dir.join(target_filename);

            // Strategy: Zero-Lock hot backup via SQLite VACUUM INTO.
            // Fallback to simple copy if VACUUM INTO fails.
            let vacuum_success = match crate::config::open_db(src_path) {
                Ok(conn) => {
                    let tmp_path = target_path.with_extension("tmp");
                    let _ = fs::remove_file(&tmp_path);
                    let escaped = tmp_path.to_string_lossy().replace('\'', "''");
                    let sql = format!("VACUUM INTO '{escaped}'");
                    match conn.execute_batch(&sql) {
                        Ok(()) => {
                            let _ = fs::rename(&tmp_path, &target_path);
                            debug!("✅ [BackupManager] VACUUM INTO backup created for {}: {}", label, target_path.display());
                            true
                        }
                        Err(e) => {
                            warn!("⚠️ [BackupManager] VACUUM INTO failed for {}: {}, falling back to copy", label, e);
                            let _ = fs::remove_file(&tmp_path);
                            false
                        }
                    }
                }
                Err(e) => {
                    warn!("⚠️ [BackupManager] Failed to open {} for VACUUM INTO: {}, falling back to copy", label, e);
                    false
                }
            };

            if !vacuum_success {
                match fs::copy(src_path, &target_path) {
                    Ok(_) => debug!("✅ [BackupManager] Copy backup created for {}: {}", label, target_path.display()),
                    Err(e) => warn!("⚠️ [BackupManager] Failed to backup {}: {}", label, e),
                }
            }
        }

        if !faltantes.is_empty() {
            // D4986: que se sepa. Un backup que se salta bases en silencio es peor que
            // no tener backup, porque el operador cree que lo tiene.
            warn!(
                "⚠️ [BackupManager] Faltan {} base(s) en el DATA_DIR ({}): no se han copiado: {}",
                faltantes.len(),
                crate::config::data_dir().display(),
                faltantes.join(", ")
            );
        }

        info!("💾 [BackupManager] Brain backup cycle completed (Rotating 7-day slot: {}).", day_of_week);
        Ok(())
    }

    /// Verify the integrity of a database file using SQLite PRAGMA.
    pub async fn check_integrity(db_path: &str) -> Result<bool> {
        if !Path::new(db_path).exists() {
            return Ok(true); // Nothing to check
        }

        let conn = crate::config::open_db(std::path::Path::new(db_path))
            .with_context(|| format!("Failed to open {db_path} for integrity check"))?;
        
        let status: String = conn.query_row("PRAGMA integrity_check;", [], |row| row.get(0))?;
        
        if status == "ok" {
            Ok(true)
        } else {
            error!("❌ [IntegrityCheck] Database corrupted at {}: {}", db_path, status);
            Ok(false)
        }
    }
}

/// Bases que se copian y su etiqueta, resueltas contra el **DATA_DIR efectivo**
/// (D4986). Antes eran literales `"data/..."` relativos al cwd, así que una
/// instancia con `TYLLUAN_DATA_DIR` propio hacía el backup de **otra** carpeta (o de
/// ninguna) y lo daba por bueno.
pub fn database_sources() -> Vec<(&'static str, std::path::PathBuf)> {
    [("silva", "silva.db"), ("tylluan", "tylluan.db"), ("mailbox", "mailbox.db"), ("audit", "audit.db")]
        .iter()
        .map(|(label, file)| (*label, crate::config::data_file(file)))
        .collect()
}

/// Helper to run a full system integrity check at startup.
pub async fn run_startup_integrity_check() -> Result<()> {
    info!("🛡️ [Integrity] Running startup database validation...");
    
    let critical = database_sources()
        .into_iter()
        .filter(|(label, _)| *label == "silva" || *label == "tylluan")
        .map(|(_, path)| path)
        .collect::<Vec<_>>();
    for db in critical {
        let db_str = db.to_string_lossy().to_string();
        // D4986: `check_integrity` devuelve Ok(true) para un fichero que no existe
        // («nada que comprobar»), así que antes un DATA_DIR vacío se resumía como
        // «healthy». Se dice explícitamente que falta.
        if !db.exists() {
            warn!(
                "⚠️ [Integrity] no existe {} (DATA_DIR {}): no hay nada que verificar",
                db.display(),
                crate::config::data_dir().display()
            );
            continue;
        }
        match BackupManager::check_integrity(&db_str).await {
            Ok(true) => info!("✅ [Integrity] {} is healthy", db_str),
            Ok(false) => {
                warn!("⚠️ [Integrity] {} reported issues! Attempting recovery from last backup...", db_str);
                // Future: Implement auto-restore from backup_dir
            }
            Err(e) => error!("❌ [Integrity] Could not verify {}: {}", db_str, e),
        }
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_check_integrity_nonexistent_and_valid() {
        assert!(BackupManager::check_integrity("data/nonexistent_db_12345.db").await.unwrap());

        let temp_dir = tempfile::tempdir().unwrap();
        let test_db = temp_dir.path().join("test_integrity.db");
        {
            let conn = crate::config::open_db(&test_db).unwrap();
            conn.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT); INSERT INTO t VALUES (1, 'val');").unwrap();
        }
        assert!(BackupManager::check_integrity(&test_db.to_string_lossy()).await.unwrap());
    }

    #[tokio::test]
    async fn test_backup_all_with_vacuum_into() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backup_dir = temp_dir.path().join("backups");
        let mgr = BackupManager::with_dir(backup_dir.clone());
        assert!(mgr.backup_dir.exists());
        // backup_all gracefully skips missing databases without error
        assert!(mgr.backup_all().await.is_ok());
    }

    /// D4986 (opción A del TL): las bases del backup siguen al DATA_DIR efectivo.
    /// MUTACION: volver a `PathBuf::from(format!("data/{file}"))` en `data_file` deja
    /// este test en rojo.
    #[tokio::test]
    async fn las_bases_del_backup_siguen_al_data_dir() {
        let _g = crate::test_env::serial();
        let previo = std::env::var("TYLLUAN_DATA_DIR").ok();
        unsafe { std::env::set_var("TYLLUAN_DATA_DIR", "D:/tmp/instancia-backup") };
        let fuentes = super::database_sources();
        let backup_dir = BackupManager::new().backup_dir.clone();
        match previo {
            Some(p) => unsafe { std::env::set_var("TYLLUAN_DATA_DIR", p) },
            None => unsafe { std::env::remove_var("TYLLUAN_DATA_DIR") },
        }

        assert_eq!(fuentes.len(), 4, "silva, tylluan, mailbox, audit");
        for (label, path) in &fuentes {
            assert!(
                path.starts_with("D:/tmp/instancia-backup"),
                "la base {label} tiene que colgar del DATA_DIR: {}",
                path.display()
            );
        }
        assert!(
            backup_dir.starts_with("D:/tmp/instancia-backup"),
            "la carpeta de copias tambien: {}",
            backup_dir.display()
        );
    }}
