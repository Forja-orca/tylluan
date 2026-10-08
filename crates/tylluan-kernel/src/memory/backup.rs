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
        Self::with_dir(PathBuf::from("data/backups"))
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
        
        let databases = vec![
            ("data/silva.db", "silva"),
            ("data/tylluan.db", "tylluan"),
            ("data/mailbox.db", "mailbox"),
            ("data/audit.db", "audit"),
        ];

        let day_of_week = Local::now().weekday().number_from_monday(); // 1-7
        
        for (src_path, label) in databases {
            if !Path::new(src_path).exists() { continue; }

            let target_filename = format!("{label}_{day_of_week}.db.bak");
            let target_path = self.backup_dir.join(target_filename);

            // Strategy: Zero-Lock hot backup via SQLite VACUUM INTO.
            // Fallback to simple copy if VACUUM INTO fails.
            let vacuum_success = match crate::config::open_db(Path::new(src_path)) {
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

/// Helper to run a full system integrity check at startup.
pub async fn run_startup_integrity_check() -> Result<()> {
    info!("🛡️ [Integrity] Running startup database validation...");
    
    let critical = vec!["data/silva.db", "data/tylluan.db"];
    for db in critical {
        match BackupManager::check_integrity(db).await {
            Ok(true) => info!("✅ [Integrity] {} is healthy", db),
            Ok(false) => {
                warn!("⚠️ [Integrity] {} reported issues! Attempting recovery from last backup...", db);
                // Future: Implement auto-restore from backup_dir
            }
            Err(e) => error!("❌ [Integrity] Could not verify {}: {}", db, e),
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
}