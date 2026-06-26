//! # Agent Mailbox
//!
//! Asynchronous messaging between agents via SQLite.
//! Ported from `TylluanMCP/src/brain/SilvaDB.ts` (agent_mail table).
//!
//! Provides:
//! - **send_mail**: Queue a message from one agent to another
//! - **check_mail**: Retrieve unread messages, optionally marking as read
//! - **message_count**: Count unread messages for an agent

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::time::Duration;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::info;

/// A message in the agent mailbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailMessage {
    pub message_id: String,
    pub sender_id: String,
    pub receiver_id: String,
    pub status: String,
    pub payload: String,
    pub ttl_secs: i64,
    pub created_at: i64,
}

/// A canonical message format for the Blackboard protocol.
/// As per arxiv 2507.01701 for LLM multi-agent coordination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlackboardMessage {
    /// Semantic type of the message: "task" | "result" | "request" | "broadcast" | "welcome"
    pub msg_type: String,
    /// Main human/agent readable content
    pub body: String,
    /// Target agent ID (or "broadcast")
    pub to: String,
    /// Issuing agent ID
    pub from: String,
    /// Optional thread identifier for conversation grouping
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    /// Priority 0-10 (10 = critical)
    #[serde(default = "default_priority")]
    pub priority: u8,
}

fn default_priority() -> u8 { 5 }

impl BlackboardMessage {
    pub fn task(from: &str, to: &str, body: &str) -> Self {
        Self {
            msg_type: "task".into(),
            body: body.into(),
            to: to.into(),
            from: from.into(),
            thread_id: None,
            priority: 5,
        }
    }

    pub fn result(from: &str, to: &str, body: &str, thread_id: Option<String>) -> Self {
        Self {
            msg_type: "result".into(),
            body: body.into(),
            to: to.into(),
            from: from.into(),
            thread_id,
            priority: 5,
        }
    }

    pub fn broadcast(from: &str, body: &str) -> Self {
        Self {
            msg_type: "broadcast".into(),
            body: body.into(),
            to: "broadcast".into(),
            from: from.into(),
            thread_id: None,
            priority: 3,
        }
    }

    pub fn direct(from: &str, to: &str, body: &str) -> Self {
        Self {
            msg_type: "direct".into(),
            body: body.into(),
            to: to.into(),
            from: from.into(),
            thread_id: None,
            priority: 5,
        }
    }

    /// Serialize to JSON string for Mailbox.payload
    pub fn to_payload(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Try to parse a payload as BlackboardMessage
    pub fn from_payload(payload: &str) -> Option<Self> {
        serde_json::from_str(payload).ok()
    }
}

/// Retry wrapper for SQLite writes under WAL contention
fn execute_with_retry(conn: &Connection, query: &str, params: &[&dyn rusqlite::ToSql], max_retries: u32) -> rusqlite::Result<usize> {
    let mut attempt = 0;
    loop {
        match conn.execute(query, params) {
            Ok(res) => return Ok(res),
            Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == rusqlite::ErrorCode::DatabaseLocked && attempt < max_retries => {
                attempt += 1;
                std::thread::sleep(Duration::from_millis(10 * 2_u64.pow(attempt)));
                continue;
            }
            Err(e) => return Err(e),
        }
    }
}

/// The agent mailbox engine.
pub struct Mailbox {
    conn: tokio::sync::Mutex<rusqlite::Connection>,
    pub notifier: std::sync::Arc<tokio::sync::Notify>,
}

impl Mailbox {
    /// Open or create a Mailbox database at the given path.
    pub fn open(db_path: &str) -> Result<Self> {
        if let Some(parent) = std::path::Path::new(db_path).parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let conn = crate::config::open_db(std::path::Path::new(db_path))
            .with_context(|| format!("Failed to open Mailbox DB: {}", db_path))?;

        let mailbox = Self { 
            conn: tokio::sync::Mutex::new(conn),
            notifier: std::sync::Arc::new(tokio::sync::Notify::new()),
        };
        Ok(mailbox)
    }

    /// Complete initialization.
    pub async fn init(&self) -> Result<()> {
        self.init_schema().await
    }

    /// Perform a WAL checkpoint to merge transaction logs into the main database file.
    /// This prevents the -wal file from growing indefinitely.
    pub async fn checkpoint(&self) -> Result<()> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
                .with_context(|| "Failed to checkpoint Mailbox")
        })?;
        Ok(())
    }

    pub async fn new(path: &str) -> Result<Self> {
        let conn = crate::config::open_db(std::path::Path::new(path))?;
        let mailbox = Self { 
            conn: Mutex::new(conn),
            notifier: std::sync::Arc::new(tokio::sync::Notify::new()),
        };
        mailbox.init_schema().await?;
        Ok(mailbox)
    }

    /// Create an in-memory instance (for testing).
    #[allow(dead_code)]
    pub async fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let mailbox = Self { 
            conn: Mutex::new(conn),
            notifier: std::sync::Arc::new(tokio::sync::Notify::new()),
        };
        mailbox.init_schema().await?;
        Ok(mailbox)
    }

    /// Health check: verify database integrity via quick query.
    pub async fn health_check(&self) -> Result<()> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            // Use query_row instead of execute for SELECT to avoid "Execute returned results" error
            let _: i32 = conn.query_row("SELECT 1 FROM agent_mail LIMIT 1", [], |row| row.get(0))?;
            Ok::<(), anyhow::Error>(())
        })?;
        Ok(())
    }

    async fn init_schema(&self) -> Result<()> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();

            // 1. Ensure table exists
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS agent_mail (
                    message_id TEXT PRIMARY KEY,
                    sender_id TEXT NOT NULL,
                    receiver_id TEXT NOT NULL,
                    status TEXT DEFAULT 'UNREAD',
                    payload TEXT NOT NULL,
                    created_at INTEGER DEFAULT (unixepoch())
                );"
            )?;

            // 2. Migration: Add new columns if they don't exist
            let _ = conn.execute("ALTER TABLE agent_mail ADD COLUMN ttl_secs INTEGER DEFAULT 3600", []);
            
            // 3. Ensure indexes exist (now we are sure columns exist)
            conn.execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_mail_receiver
                    ON agent_mail (receiver_id, status);
                
                CREATE INDEX IF NOT EXISTS idx_mail_expiry
                    ON agent_mail (created_at, ttl_secs);"
            )?;
            
            Ok::<(), anyhow::Error>(())
        })?;

        info!("📬 Mailbox schema initialized (v2 with TTL).");
        Ok(())
    }

    /// Purge expired messages from the mailbox.
    pub async fn purge_expired(&self) -> Result<usize> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let count = conn.execute(
                "DELETE FROM agent_mail WHERE created_at < (unixepoch() - ttl_secs)",
                [],
            )?;
            if count > 0 {
                info!("📬 Mailbox: purged {} expired messages.", count);
            }
            Ok(count)
        })
    }

    /// Send a message from one agent to another.
    /// Returns the generated message ID.
    pub async fn send_mail(
        &self,
        sender_id: &str,
        receiver_id: &str,
        payload: &str,
    ) -> Result<String> {
        self.send_mail_with_ttl(sender_id, receiver_id, payload, 3600).await
    }

    pub async fn send_mail_with_ttl(
        &self,
        sender_id: &str,
        receiver_id: &str,
        payload: &str,
        ttl_secs: i64,
    ) -> Result<String> {
        let message_id = format!(
            "msg_{}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("0000")
        );

        let ttl = ttl_secs.to_string();
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let msg_id = message_id.clone();
            let sender = sender_id.to_string();
            let receiver = receiver_id.to_string();
            let payload = payload.to_string();
            let params: Vec<&dyn rusqlite::ToSql> = vec![&msg_id, &sender, &receiver, &payload, &ttl];
            let _ = execute_with_retry(
                &conn,
                "INSERT INTO agent_mail (message_id, sender_id, receiver_id, payload, ttl_secs, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, unixepoch())",
                &params,
                5,
            )?;
            Ok::<(), anyhow::Error>(())
        })?;
        
        // Notify any listeners that a new message has arrived
        self.notifier.notify_one();
        
        Ok(message_id)
    }

    /// [BLACKBOARD] Sends a task to an agent with 'PENDING' status.
    pub async fn send_task(
        &self,
        sender_id: &str,
        receiver_id: &str,
        payload: &str,
        ttl_secs: i64,
    ) -> Result<String> {
        let message_id = format!(
            "task_{}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("0000")
        );

        let ttl = ttl_secs.to_string();
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let msg_id = message_id.clone();
            let sender = sender_id.to_string();
            let receiver = receiver_id.to_string();
            let payload = payload.to_string();
            let params: Vec<&dyn rusqlite::ToSql> = vec![&msg_id, &sender, &receiver, &payload, &ttl];
            let _ = execute_with_retry(
                &conn,
                "INSERT INTO agent_mail (message_id, sender_id, receiver_id, payload, ttl_secs, status, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'PENDING', unixepoch())",
                &params,
                5,
            )?;
            Ok::<(), anyhow::Error>(())
        })?;
        
        self.notifier.notify_one();
        Ok(message_id)
    }

    /// Check mail for a specific agent. Returns unread messages.
    /// If `mark_as_read` is true, marks them as READ atomically.
    /// Uses a single transaction to prevent TOCTOU race conditions.
    /// `max_messages` limits the number of messages processed per call (default: 50).
    pub async fn check_mail(
        &self,
        receiver_id: &str,
        mark_as_read: bool,
        max_messages: usize,
    ) -> Result<Vec<MailMessage>> {
        let receiver = receiver_id.to_string();
        let max = max_messages as i64;

        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();

            let messages: Vec<MailMessage> = if mark_as_read {
                let _ = execute_with_retry(&conn, "BEGIN IMMEDIATE", &[], 5)?;

                let mut attempt = 0;
                let rows: Vec<MailMessage> = loop {
                    match conn.prepare(
                        "SELECT message_id, sender_id, receiver_id, status, payload, ttl_secs, created_at
                         FROM agent_mail WHERE receiver_id = ?1 AND status = 'UNREAD' ORDER BY created_at ASC LIMIT ?2"
                    ).and_then(|mut stmt| {
                        let mut rows = Vec::new();
                        let mut rows_iter = stmt.query(params![&receiver, max])?;
                        while let Some(row) = rows_iter.next()? {
                            rows.push(MailMessage {
                                message_id: row.get(0)?,
                                sender_id: row.get(1)?,
                                receiver_id: row.get(2)?,
                                status: row.get(3)?,
                                payload: row.get(4)?,
                                ttl_secs: row.get(5)?,
                                created_at: row.get(6)?,
                            });
                        }
                        Ok(rows)
                    }) {
                        Ok(r) => break Ok(r),
                        Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == rusqlite::ErrorCode::DatabaseLocked && attempt < 5 => {
                            attempt += 1;
                            std::thread::sleep(Duration::from_millis(10 * 2_u64.pow(attempt)));
                            continue;
                        }
                        Err(e) => break Err(e),
                    }
                }.unwrap_or_default();

                if !rows.is_empty() {
                    let placeholders: Vec<String> = std::iter::repeat_n("?".to_string(), rows.len()).collect();
                    let ids: Vec<String> = rows.iter().map(|m| m.message_id.clone()).collect();
                    let params: Vec<&dyn rusqlite::ToSql> = ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
                    let _ = execute_with_retry(
                        &conn,
                        &format!("UPDATE agent_mail SET status = 'READ' WHERE message_id IN ({})", placeholders.join(",")),
                        &params,
                        5,
                    )?;
                }
                let _ = execute_with_retry(&conn, "COMMIT", &[], 5)?;
                rows
            } else {
                let mut attempt = 0;
                loop {
                    match conn.prepare(
                        "SELECT message_id, sender_id, receiver_id, status, payload, ttl_secs, created_at
                         FROM agent_mail WHERE receiver_id = ?1 AND status = 'UNREAD' ORDER BY created_at ASC LIMIT ?2"
                    ).and_then(|mut stmt| {
                        let mut rows = Vec::new();
                        let mut rows_iter = stmt.query(params![&receiver, max])?;
                        while let Some(row) = rows_iter.next()? {
                            rows.push(MailMessage {
                                message_id: row.get(0)?,
                                sender_id: row.get(1)?,
                                receiver_id: row.get(2)?,
                                status: row.get(3)?,
                                payload: row.get(4)?,
                                ttl_secs: row.get(5)?,
                                created_at: row.get(6)?,
                            });
                        }
                        Ok(rows)
                    }) {
                        Ok(r) => break r,
                        Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == rusqlite::ErrorCode::DatabaseLocked && attempt < 5 => {
                            attempt += 1;
                            std::thread::sleep(Duration::from_millis(10 * 2_u64.pow(attempt)));
                            continue;
                        }
                        Err(_) => break vec![],
                    }
                }
            };

            Ok(messages)
        })
    }

    /// Count unread messages for a receiver.
    #[allow(dead_code)]
    pub async fn unread_count(&self, receiver_id: &str) -> Result<i64> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM agent_mail WHERE receiver_id = ?1 AND status = 'UNREAD'",
                params![receiver_id],
                |row| row.get(0),
            )?;
            Ok(count)
        })
    }

    /// Retrieve messages for an agent (unread first).
    pub async fn get_messages_for(&self, agent_id: &str, limit: usize) -> Result<Vec<MailMessage>> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT message_id, sender_id, receiver_id, status, payload, ttl_secs, created_at
                 FROM agent_mail
                 WHERE receiver_id = ?1
                 ORDER BY (status = 'UNREAD') DESC, created_at DESC
                 LIMIT ?2"
            )?;
            let rows = stmt.query_map(params![agent_id, limit as i64], |row| {
                Ok(MailMessage {
                    message_id: row.get(0)?,
                    sender_id: row.get(1)?,
                    receiver_id: row.get(2)?,
                    status: row.get(3)?,
                    payload: row.get(4)?,
                    ttl_secs: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>().map_err(anyhow::Error::from)
        })
    }

    /// Retrieve a specific thread (messages matching thread_id in payload JSON).
    pub async fn get_thread(&self, agent_id: &str, thread_id: &str) -> Result<Vec<MailMessage>> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT message_id, sender_id, receiver_id, status, payload, ttl_secs, created_at
                 FROM agent_mail
                 WHERE (receiver_id = ?1 OR sender_id = ?1)
                   AND json_valid(payload)
                   AND json_extract(payload, '$.thread_id') = ?2
                 ORDER BY created_at ASC"
            )?;
            let rows = stmt.query_map(params![agent_id, thread_id], |row| {
                Ok(MailMessage {
                    message_id: row.get(0)?,
                    sender_id: row.get(1)?,
                    receiver_id: row.get(2)?,
                    status: row.get(3)?,
                    payload: row.get(4)?,
                    ttl_secs: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>().map_err(anyhow::Error::from)
        })
    }

    /// Broadcast a message to all agents except the sender.
    /// `known_agents` should be the list of registered agent IDs from agent_profiles
    /// (prevents blind broadcasts that only work if agents already have mail history).
    /// Returns the number of messages delivered.
    pub async fn broadcast(
        &self,
        sender_id: &str,
        msg_type: &str,
        content: &str,
        known_agents: &[String],
    ) -> Result<usize> {
        let payload = serde_json::json!({
            "type": msg_type,
            "content": content,
            "from": sender_id,
        }).to_string();

        // Build receiver list: known_agents + anyone seen in mail history
        let mut receivers: Vec<String> = known_agents.iter()
            .filter(|a| a.as_str() != sender_id)
            .cloned()
            .collect();

        // Also include agents seen in mail history not already in known_agents
        let history: Vec<String> = tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT DISTINCT receiver_id FROM agent_mail WHERE receiver_id != ?1
                 UNION
                 SELECT DISTINCT sender_id FROM agent_mail WHERE sender_id != ?1"
            )?;
            let rows = stmt.query_map(params![sender_id], |row| row.get(0))?;
            rows.collect::<rusqlite::Result<Vec<String>>>().map_err(anyhow::Error::from)
        })?;

        for id in history {
            if !receivers.contains(&id) {
                receivers.push(id);
            }
        }

        let mut sent = 0usize;
        for receiver in &receivers {
            self.send_mail(sender_id, receiver, &payload).await?;
            sent += 1;
        }
        Ok(sent)
    }

    /// Get recent broadcasts of a given type for an agent (last N hours).
    pub async fn get_recent_broadcasts(
        &self,
        receiver_id: &str,
        msg_type: &str,
        hours: u64,
    ) -> Result<Vec<MailMessage>> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let cutoff_secs = (hours * 3600) as i64;
            let mut stmt = conn.prepare(
                "SELECT message_id, sender_id, receiver_id, status, payload, ttl_secs, created_at
                 FROM agent_mail
                 WHERE receiver_id = ?1
                   AND json_valid(payload)
                   AND json_extract(payload, '$.type') = ?2
                   AND created_at >= (unixepoch() - ?3)
                 ORDER BY created_at DESC
                 LIMIT 20"
            )?;
            let rows = stmt.query_map(params![receiver_id, msg_type, cutoff_secs], |row| {
                Ok(MailMessage {
                    message_id: row.get(0)?,
                    sender_id: row.get(1)?,
                    receiver_id: row.get(2)?,
                    status: row.get(3)?,
                    payload: row.get(4)?,
                    ttl_secs: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>().map_err(anyhow::Error::from)
        })
    }

    /// Count broadcasts sent in the last N hours.
    pub async fn broadcast_count_last_hours(&self, hours: u64) -> Result<i64> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let cutoff_secs = (hours * 3600) as i64;
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM agent_mail
                 WHERE json_valid(payload)
                   AND json_extract(payload, '$.type') = 'knowledge_share'
                   AND created_at >= (unixepoch() - ?1)",
                params![cutoff_secs],
                |row| row.get(0),
            )?;
            Ok(count)
        })
    }

    /// [BLACKBOARD] Retrieves pending tasks for a specific agent.
    pub async fn get_tasks_for_agent(&self, agent_name: &str) -> Result<Vec<MailMessage>> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT message_id, sender_id, receiver_id, status, payload, ttl_secs, created_at
                 FROM agent_mail
                 WHERE receiver_id = ?1 AND status = 'PENDING'
                 ORDER BY created_at ASC"
            )?;
            let rows = stmt.query_map(params![agent_name], |row| {
                Ok(MailMessage {
                    message_id: row.get(0)?,
                    sender_id: row.get(1)?,
                    receiver_id: row.get(2)?,
                    status: row.get(3)?,
                    payload: row.get(4)?,
                    ttl_secs: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>().map_err(anyhow::Error::from)
        })
    }

    /// [BLACKBOARD] Marks a task as done and appends the result to the payload.
    pub async fn mark_task_done(&self, msg_id: &str, result_summary: &str) -> Result<()> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            // Get current payload
            let payload: String = conn.query_row(
                "SELECT payload FROM agent_mail WHERE message_id = ?1",
                params![msg_id],
                |row| row.get(0),
            )?;

            // Append result summary to the payload (assuming it's either JSON or text)
            let updated_payload = format!("{}\n\nRESULT: {}", payload, result_summary);

            let _ = conn.execute(
                "UPDATE agent_mail SET status = 'COMPLETED', payload = ?1 WHERE message_id = ?2",
                params![updated_payload, msg_id],
            )?;
            
            info!("[BLACKBOARD] Task {} marked as COMPLETED", msg_id);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_mailbox() -> Mailbox {
        Mailbox::in_memory().await.unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_send_and_receive() {
        let mb = test_mailbox().await;
        let msg_id = mb.send_mail("agent-a", "agent-b", r#"{"task":"analyze"}"#).await.unwrap();
        assert!(msg_id.starts_with("msg_"));

        let messages = mb.check_mail("agent-b", false, 50).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].sender_id, "agent-a");
        assert_eq!(messages[0].status, "UNREAD");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_mark_as_read() {
        let mb = test_mailbox().await;
        mb.send_mail("a", "b", "hello").await.unwrap();

        // Read and mark
        let messages = mb.check_mail("b", true, 50).await.unwrap();
        assert_eq!(messages.len(), 1);

        // Second read should return empty (already READ)
        let messages = mb.check_mail("b", false, 50).await.unwrap();
        assert!(messages.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_unread_count() {
        let mb = test_mailbox().await;
        mb.send_mail("a", "b", "msg1").await.unwrap();
        mb.send_mail("a", "b", "msg2").await.unwrap();
        mb.send_mail("a", "c", "msg3").await.unwrap(); // Different receiver
 
        assert_eq!(mb.unread_count("b").await.unwrap(), 2);
        assert_eq!(mb.unread_count("c").await.unwrap(), 1);
        assert_eq!(mb.unread_count("nobody").await.unwrap(), 0);
    }
 
    #[tokio::test(flavor = "multi_thread")]
    async fn test_multiple_senders() {
        let mb = test_mailbox().await;
        mb.send_mail("agent-1", "hub", "task1").await.unwrap();
        mb.send_mail("agent-2", "hub", "task2").await.unwrap();
 
        let messages = mb.check_mail("hub", false, 50).await.unwrap();
        assert_eq!(messages.len(), 2);
 
        // Order should be by creation time
        assert_eq!(messages[0].sender_id, "agent-1");
        assert_eq!(messages[1].sender_id, "agent-2");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_empty_mailbox() {
        let mb = test_mailbox().await;
        let messages = mb.check_mail("nobody", false, 50).await.unwrap();
        assert!(messages.is_empty());
    }
}
