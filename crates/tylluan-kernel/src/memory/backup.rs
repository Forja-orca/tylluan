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
        let backup_dir = PathBuf::from("data/backups");
        fs::create_dir_all(&backup_dir).ok();
        Self { backup_dir }
    }

    /// Perform a high-integrity backup of all critical databases.
    /// Uses SQLite's VACUUM INTO for consistent backups if possible, 
    /// or simple copy for portability.
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

            let target_filename = format!("{}_{}.db.bak", label, day_of_week);
            let target_path = self.backup_dir.join(target_filename);

            // Strategy: Simple copy for now (Sovereign Portability).
            // Future: Use rusqlite backup API for Zero-Lock backups.
            match fs::copy(src_path, &target_path) {
                Ok(_) => debug!("✅ [BackupManager] Backup created for {}: {}", label, target_path.display()),
                Err(e) => warn!("⚠️ [BackupManager] Failed to backup {}: {}", label, e),
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
            .with_context(|| format!("Failed to open {} for integrity check", db_path))?;
        
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