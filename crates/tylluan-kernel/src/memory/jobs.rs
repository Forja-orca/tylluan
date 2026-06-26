use anyhow::{Result, Context};
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;
use serde_json::Value;
use uuid::Uuid;

pub struct JobQueue {
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone)]
pub struct Job {
    pub id: String,
    pub task_type: String,
    pub payload: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

impl JobQueue {
    pub fn open(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = crate::config::open_db(db_path)
            .with_context(|| format!("Failed to open jobs DB: {:?}", db_path))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS tylluan_jobs (
                id TEXT PRIMARY KEY,
                task_type TEXT NOT NULL,
                payload TEXT NOT NULL DEFAULT '{}',
                status TEXT NOT NULL DEFAULT 'pending',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_jobs_status ON tylluan_jobs(status);
            CREATE INDEX IF NOT EXISTS idx_jobs_type ON tylluan_jobs(task_type);"
        )?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn enqueue(&self, task_type: &str, payload: &Value) -> Result<String> {
        let id = format!("job:{}:{}", task_type, Uuid::new_v4().to_string().split('-').next().unwrap_or("x"));
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let payload_str = serde_json::to_string(payload)?;
        let conn = self.conn.lock().unwrap_or_else(|e| {
            tracing::warn!("JobQueue mutex poisoned, recovering");
            e.into_inner()
        });
        conn.execute(
            "INSERT INTO tylluan_jobs (id, task_type, payload, status, created_at, updated_at) VALUES (?1, ?2, ?3, 'pending', ?4, ?5)",
            rusqlite::params![id, task_type, payload_str, now, now],
        )?;
        Ok(id)
    }

    pub fn claim_next(&self, task_type: &str) -> Result<Option<Job>> {
        let conn = self.conn.lock().unwrap_or_else(|e| {
            tracing::warn!("JobQueue mutex poisoned, recovering");
            e.into_inner()
        });
        let mut stmt = conn.prepare(
            "SELECT id, task_type, payload, status, created_at, updated_at FROM tylluan_jobs WHERE status = 'pending' AND task_type = ?1 ORDER BY created_at ASC LIMIT 1"
        )?;
        let mut job = stmt.query_row(rusqlite::params![task_type], |row| {
            Ok(Job {
                id: row.get(0)?,
                task_type: row.get(1)?,
                payload: row.get(2)?,
                status: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        }).ok();

        if let Some(ref mut j) = job {
            let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
            conn.execute(
                "UPDATE tylluan_jobs SET status = 'running', updated_at = ?1 WHERE id = ?2 AND status = 'pending'",
                rusqlite::params![now, j.id],
            )?;
            j.status = "running".to_string();
        }

        Ok(job)
    }

    pub fn mark_done(&self, job_id: &str) -> Result<()> {
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let conn = self.conn.lock().unwrap_or_else(|e| {
            tracing::warn!("JobQueue mutex poisoned, recovering");
            e.into_inner()
        });
        conn.execute(
            "UPDATE tylluan_jobs SET status = 'done', updated_at = ?1 WHERE id = ?2",
            rusqlite::params![now, job_id],
        )?;
        Ok(())
    }

    pub fn mark_failed(&self, job_id: &str) -> Result<()> {
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let conn = self.conn.lock().unwrap_or_else(|e| {
            tracing::warn!("JobQueue mutex poisoned, recovering");
            e.into_inner()
        });
        conn.execute(
            "UPDATE tylluan_jobs SET status = 'failed', updated_at = ?1 WHERE id = ?2",
            rusqlite::params![now, job_id],
        )?;
        Ok(())
    }

    pub fn resume_pending(&self) -> Result<usize> {
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let conn = self.conn.lock().unwrap_or_else(|e| {
            tracing::warn!("JobQueue mutex poisoned, recovering");
            e.into_inner()
        });
        let count = conn.execute(
            "UPDATE tylluan_jobs SET status = 'pending', updated_at = ?1 WHERE status = 'running'",
            rusqlite::params![now],
        )?;
        Ok(count)
    }

    pub fn pending_count(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap_or_else(|e| {
            tracing::warn!("JobQueue mutex poisoned, recovering");
            e.into_inner()
        });
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tylluan_jobs WHERE status IN ('pending', 'running')",
            [],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    /// Count jobs stuck in 'running' status for more than 5 minutes (potential zombie jobs).
    pub fn count_stuck(&self) -> Result<u32> {
        let conn = self.conn.lock().unwrap_or_else(|e| {
            tracing::warn!("JobQueue mutex poisoned, recovering");
            e.into_inner()
        });
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tylluan_jobs WHERE status = 'running' AND datetime(updated_at) < datetime('now', '-5 minutes')",
            [],
            |row| row.get(0),
        )?;
        Ok(count as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_queue() -> JobQueue {
        JobQueue::open(Path::new(":memory:")).unwrap()
    }

    #[test]
    fn test_enqueue_and_claim() {
        let q = test_queue();
        let id = q.enqueue("test", &serde_json::json!({"key": "value"})).unwrap();
        assert!(id.starts_with("job:test:"));

        let job = q.claim_next("test").unwrap().expect("should claim a job");
        assert_eq!(job.id, id);
        assert_eq!(job.status, "running");

        let none = q.claim_next("test").unwrap();
        assert!(none.is_none(), "should not claim same job twice");
    }

    #[test]
    fn test_mark_done() {
        let q = test_queue();
        let id = q.enqueue("done_test", &serde_json::json!({})).unwrap();
        let job = q.claim_next("done_test").unwrap().unwrap();
        q.mark_done(&job.id).unwrap();

        let none = q.claim_next("done_test").unwrap();
        assert!(none.is_none());
    }

    #[test]
    fn test_resume_pending() {
        let q = test_queue();
        q.enqueue("failover", &serde_json::json!({})).unwrap();
        let job = q.claim_next("failover").unwrap().unwrap();
        assert_eq!(job.status, "running");

        let resumed = q.resume_pending().unwrap();
        assert_eq!(resumed, 1, "should resume 1 stuck running job");

        let job2 = q.claim_next("failover").unwrap().unwrap();
        assert_eq!(job2.id, job.id, "should reclaim the same job");
    }

    #[test]
    fn test_multiple_task_types() {
        let q = test_queue();
        q.enqueue("alpha", &serde_json::json!({"x": 1})).unwrap();
        q.enqueue("beta", &serde_json::json!({"y": 2})).unwrap();

        let a = q.claim_next("alpha").unwrap().unwrap();
        assert!(a.id.contains("alpha"));

        let b = q.claim_next("beta").unwrap().unwrap();
        assert!(b.id.contains("beta"));
    }
}
