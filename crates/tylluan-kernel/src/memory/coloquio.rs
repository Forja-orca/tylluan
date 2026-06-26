use anyhow::Result;
use rusqlite::{Connection, params};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ColoquioChannel {
    pub channel_id: String,
    pub name: String,
    pub created_at: i64,
    pub message_count: i64,
    pub last_turn: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CollabDoc {
    pub doc_id: String,
    pub title: String,
    pub content: String,
    pub created_by: String,
    pub updated_by: String,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CollabDocSnapshot {
    pub doc_id: String,
    pub version: i64,
    pub title: String,
    pub content: String,
    pub updated_by: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ColoquioMessage {
    pub msg_id: String,
    pub channel_id: String,
    pub author_id: String,
    pub role: String,
    pub content: String,
    pub turn: i64,
    pub created_at: i64,
    pub metadata: String,
}

/// Per-reader unread state for one channel (powers `whats_new` and UI badges).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UnreadSummary {
    pub channel_id: String,
    pub name: String,
    pub last_turn: i64,
    pub last_read_turn: i64,
    pub unread_count: i64,
}

/// A channel id eligible for silent auto-creation on post: slug-safe
/// (alphanumeric, `-`, `_`), non-empty, max 64 chars. Prevents junk channels
/// created by failed NL parsing (e.g. a whole sentence as channel_id).
pub fn is_valid_channel_slug(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

fn get_flexible_created_at(row: &rusqlite::Row, index: usize) -> rusqlite::Result<i64> {
    let value: rusqlite::types::Value = row.get(index)?;
    match value {
        rusqlite::types::Value::Integer(n) => Ok(n),
        rusqlite::types::Value::Text(s) => {
            if let Ok(n) = s.parse::<i64>() {
                Ok(n)
            } else if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                Ok(dt.timestamp())
            } else if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S") {
                Ok(dt.and_utc().timestamp())
            } else {
                Ok(0)
            }
        }
        _ => Ok(0),
    }
}

/// Extract distinct `@mention` handles from message content.
/// A mention is `@` at a token boundary followed by [A-Za-z0-9_-]+.
/// Emails (`foo@bar`) are NOT mentions because the `@` is mid-token.
pub fn extract_mentions(content: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for tok in content.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-' || c == '@')) {
        if let Some(name) = tok.strip_prefix('@') {
            // reject empty and tokens with another '@' inside (mangled emails)
            if !name.is_empty() && !name.contains('@') && !out.iter().any(|n| n == name) {
                out.push(name.to_string());
            }
        }
    }
    out
}

pub struct ColoquioDb {
    conn: Arc<Mutex<Connection>>,
}

impl ColoquioDb {
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = crate::config::open_db(std::path::Path::new(db_path))?;
        conn.execute_batch("PRAGMA synchronous=NORMAL;")?;
        Self::init_schema(&conn)?;
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    /// In-memory instance for tests.
    #[cfg(test)]
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::init_schema(&conn)?;
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    fn init_schema(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS coloquio_channels (
                channel_id  TEXT PRIMARY KEY,
                name        TEXT NOT NULL,
                created_at  INTEGER DEFAULT (unixepoch())
            );
            CREATE TABLE IF NOT EXISTS coloquio_messages (
                msg_id      TEXT PRIMARY KEY,
                channel_id  TEXT NOT NULL,
                author_id   TEXT NOT NULL,
                role        TEXT NOT NULL DEFAULT 'agent',
                content     TEXT NOT NULL,
                turn        INTEGER NOT NULL,
                created_at  INTEGER DEFAULT (unixepoch()),
                metadata    TEXT DEFAULT '{}'
            );
            CREATE INDEX IF NOT EXISTS idx_coloquio_channel
                ON coloquio_messages(channel_id, turn);
            CREATE TABLE IF NOT EXISTS coloquio_read_state (
                channel_id      TEXT NOT NULL,
                reader_id       TEXT NOT NULL,
                last_read_turn  INTEGER NOT NULL DEFAULT 0,
                updated_at      INTEGER DEFAULT (unixepoch()),
                PRIMARY KEY (channel_id, reader_id)
            );
            CREATE TABLE IF NOT EXISTS collab_docs (
                doc_id      TEXT PRIMARY KEY,
                title       TEXT NOT NULL DEFAULT 'Untitled',
                content     TEXT NOT NULL DEFAULT '',
                created_by  TEXT NOT NULL DEFAULT 'system',
                updated_by  TEXT NOT NULL DEFAULT 'system',
                version     INTEGER NOT NULL DEFAULT 1,
                created_at  INTEGER DEFAULT (unixepoch()),
                updated_at  INTEGER DEFAULT (unixepoch())
            );
            CREATE TABLE IF NOT EXISTS collab_doc_snapshots (
                doc_id      TEXT NOT NULL,
                version     INTEGER NOT NULL,
                title       TEXT NOT NULL,
                content     TEXT NOT NULL,
                updated_by  TEXT NOT NULL,
                updated_at  INTEGER NOT NULL,
                PRIMARY KEY (doc_id, version)
            );",
        )?;
        Ok(())
    }

    pub async fn create_channel(&self, channel_id: &str, name: &str) -> Result<ColoquioChannel> {
        let channel_id = channel_id.to_string();
        let name = name.to_string();
        tokio::task::block_in_place(|| {
            let conn = self.conn.lock().expect("coloquio mutex poisoned");
            conn.execute(
                "INSERT OR IGNORE INTO coloquio_channels (channel_id, name) VALUES (?1, ?2)",
                params![channel_id, name],
            )?;
            let ch = conn.query_row(
                "SELECT channel_id, name, created_at FROM coloquio_channels WHERE channel_id = ?1",
                params![channel_id],
                |row| Ok(ColoquioChannel {
                    channel_id: row.get(0)?,
                    name: row.get(1)?,
                    created_at: row.get(2)?,
                    message_count: 0,
                    last_turn: 0,
                }),
            )?;
            Ok(ch)
        })
    }

    pub async fn get_last_turn(&self, channel_id: &str) -> Result<i64> {
        let channel_id = channel_id.to_string();
        tokio::task::block_in_place(|| {
            let conn = self.conn.lock().expect("coloquio mutex poisoned");
            let last_turn: i64 = conn.query_row(
                "SELECT COALESCE(MAX(turn), 0) FROM coloquio_messages WHERE channel_id = ?1",
                params![channel_id],
                |row| row.get(0),
            )?;
            Ok(last_turn)
        })
    }

    pub async fn list_channels(&self) -> Result<Vec<ColoquioChannel>> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.lock().expect("coloquio mutex poisoned");
            let mut stmt = conn.prepare(
                "SELECT c.channel_id, c.name, c.created_at,
                        COUNT(m.msg_id) AS message_count,
                        COALESCE(MAX(m.turn), 0) AS last_turn
                 FROM coloquio_channels c
                 LEFT JOIN coloquio_messages m ON c.channel_id = m.channel_id
                 GROUP BY c.channel_id
                 ORDER BY last_turn DESC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(ColoquioChannel {
                    channel_id: row.get(0)?,
                    name: row.get(1)?,
                    created_at: row.get(2)?,
                    message_count: row.get(3)?,
                    last_turn: row.get(4)?,
                })
            })?;
            Ok(rows.flatten().collect())
        })
    }

    pub async fn get_thread(&self, channel_id: &str, limit: i64, offset: i64) -> Result<Vec<ColoquioMessage>> {
        let channel_id = channel_id.to_string();
        tokio::task::block_in_place(|| {
            let conn = self.conn.lock().expect("coloquio mutex poisoned");
            let mut stmt = conn.prepare(
                "SELECT COALESCE(msg_id, CAST(turn AS TEXT)), channel_id, author_id, role, content, turn, created_at, metadata
                 FROM coloquio_messages
                 WHERE channel_id = ?1
                 ORDER BY turn ASC
                 LIMIT ?2 OFFSET ?3",
            )?;
            let rows = stmt.query_map(params![channel_id, limit, offset], |row| {
                Ok(ColoquioMessage {
                    msg_id: row.get(0)?,
                    channel_id: row.get(1)?,
                    author_id: row.get(2)?,
                    role: row.get(3)?,
                    content: row.get(4)?,
                    turn: row.get(5)?,
                    created_at: get_flexible_created_at(row, 6)?,
                    metadata: row.get(7)?,
                })
            })?;
            Ok(rows.flatten().collect())
        })
    }

    pub async fn search_messages(&self, channel_id: &str, keyword: &str, limit: i64) -> Result<Vec<ColoquioMessage>> {
        let channel_id = channel_id.to_string();
        let pattern = format!("%{}%", keyword.to_lowercase());
        tokio::task::block_in_place(|| {
            let conn = self.conn.lock().expect("coloquio mutex poisoned");
            let mut stmt = conn.prepare(
                "SELECT COALESCE(msg_id, CAST(turn AS TEXT)), channel_id, author_id, role, content, turn, created_at, metadata
                 FROM coloquio_messages
                 WHERE channel_id = ?1 AND LOWER(content) LIKE ?2
                 ORDER BY turn DESC
                 LIMIT ?3",
            )?;
            let rows = stmt.query_map(params![channel_id, pattern, limit], |row| {
                Ok(ColoquioMessage {
                    msg_id: row.get(0)?,
                    channel_id: row.get(1)?,
                    author_id: row.get(2)?,
                    role: row.get(3)?,
                    content: row.get(4)?,
                    turn: row.get(5)?,
                    created_at: get_flexible_created_at(row, 6)?,
                    metadata: row.get(7)?,
                })
            })?;
            Ok(rows.flatten().collect())
        })
    }

    pub async fn get_turn(&self, channel_id: &str, turn: i64) -> Result<Option<ColoquioMessage>> {
        let channel_id = channel_id.to_string();
        tokio::task::block_in_place(|| {
            let conn = self.conn.lock().expect("coloquio mutex poisoned");
            let result = conn.query_row(
                "SELECT COALESCE(msg_id, CAST(turn AS TEXT)), channel_id, author_id, role, content, turn, created_at, metadata
                 FROM coloquio_messages WHERE channel_id = ?1 AND turn = ?2",
                params![channel_id, turn],
                |row| Ok(ColoquioMessage {
                    msg_id: row.get(0)?,
                    channel_id: row.get(1)?,
                    author_id: row.get(2)?,
                    role: row.get(3)?,
                    content: row.get(4)?,
                    turn: row.get(5)?,
                    created_at: get_flexible_created_at(row, 6)?,
                    metadata: row.get(7)?,
                }),
            );
            match result {
                Ok(msg) => Ok(Some(msg)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
    }

    /// Returns all messages in a channel formatted as plain text (for archiving to memory).
    pub async fn get_channel_as_text(&self, channel_id: &str) -> Result<String> {
        let msgs = self.get_thread(channel_id, 10000, 0).await?;
        let ch_id = channel_id.to_string();
        let text = msgs.iter().map(|m| {
            format!("[T{}] @{}: {}", m.turn, m.author_id, m.content)
        }).collect::<Vec<_>>().join("\n\n");
        Ok(format!("# Coloquio: {}\n\n{}", ch_id, text))
    }

    /// Repair NULL or non-UUID msg_ids in all channels.
    /// After a power outage/crash, some msg_id values may be NULL in the WAL;
    /// this assigns fresh UUIDs so the frontend has stable React keys.
    pub async fn repair_msgids(&self) -> Result<usize> {
        use uuid::Uuid;
        tokio::task::block_in_place(|| {
            let conn = self.conn.lock().expect("coloquio mutex poisoned");
            let mut stmt = conn.prepare(
                "SELECT msg_id, turn, channel_id FROM coloquio_messages WHERE msg_id IS NULL OR length(msg_id) < 32"
            )?;
            let rows: Vec<(String, i64, String)> = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?))
            })?.flatten().collect();
            let count = rows.len();
            for (_, turn, ch) in &rows {
                let new_id = Uuid::new_v4().to_string();
                conn.execute(
                    "UPDATE coloquio_messages SET msg_id = ?1 WHERE turn = ?2 AND channel_id = ?3",
                    params![new_id, turn, ch],
                )?;
            }
            Ok(count)
        })
    }

    /// Delete a channel and all its messages.
    pub async fn delete_channel(&self, channel_id: &str) -> Result<usize> {
        let channel_id = channel_id.to_string();
        tokio::task::block_in_place(|| {
            let conn = self.conn.lock().expect("coloquio mutex poisoned");
            let deleted = conn.execute(
                "DELETE FROM coloquio_messages WHERE channel_id = ?1",
                params![channel_id],
            )?;
            conn.execute(
                "DELETE FROM coloquio_channels WHERE channel_id = ?1",
                params![channel_id],
            )?;
            Ok(deleted)
        })
    }

    /// Protected author IDs that cannot be impersonated via MCP tools.
    /// Only the dashboard HTTP direct or the human user can post as these.
    const PROTECTED_AUTHORS: &'static [&'static str] = &["jose", "admin", "system"];

    pub async fn post_message(
        &self,
        channel_id: &str,
        author_id: &str,
        role: &str,
        content: &str,
        metadata: &str,
    ) -> Result<ColoquioMessage> {
        let channel_id = channel_id.to_string();
        let author_id = if Self::PROTECTED_AUTHORS.contains(&author_id) && role != "human" {
            tracing::warn!("⛔ Coloquio: blocked impersonation attempt of protected author '{}'", author_id);
            format!("agent-as-{}", author_id)
        } else {
            author_id.to_string()
        };
        let role = role.to_string();
        let content = content.to_string();
        let metadata = metadata.to_string();

        let max_retries = 3;
        let mut attempt = 0usize;
        // Generate message_id ONCE — retries reuse the same UUID so INSERT OR IGNORE
        // prevents duplicates if the first attempt partially succeeded.
        let msg_id = Uuid::new_v4().to_string();
        loop {
            let result: std::result::Result<ColoquioMessage, anyhow::Error> = tokio::task::block_in_place(|| {
                let conn = self.conn.lock().expect("coloquio mutex poisoned");
                // Auto-create only slug-safe channel ids; posting to an existing
                // channel (whatever its id) keeps working for backward compat.
                let exists: bool = conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM coloquio_channels WHERE channel_id = ?1)",
                    params![channel_id],
                    |row| row.get(0),
                )?;
                if !exists {
                    if !is_valid_channel_slug(&channel_id) {
                        anyhow::bail!(
                            "channel '{}' does not exist and is not a valid slug (alphanumeric/-/_, max 64 chars). Create it first or use 'publica en coloquio <canal>: <mensaje>'",
                            channel_id
                        );
                    }
                    conn.execute(
                        "INSERT OR IGNORE INTO coloquio_channels (channel_id, name) VALUES (?1, ?1)",
                        params![channel_id],
                    )?;
                }
                // Auto-increment turn within channel
                let next_turn: i64 = conn.query_row(
                    "SELECT COALESCE(MAX(turn), 0) + 1 FROM coloquio_messages WHERE channel_id = ?1",
                    params![channel_id],
                    |row| row.get(0),
                )?;
                conn.execute(
                    "INSERT OR IGNORE INTO coloquio_messages (msg_id, channel_id, author_id, role, content, turn, metadata)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![msg_id, channel_id, author_id, role, content, next_turn, metadata],
                )?;
                let msg = conn.query_row(
                    "SELECT msg_id, channel_id, author_id, role, content, turn, created_at, metadata
                     FROM coloquio_messages WHERE msg_id = ?1",
                    params![msg_id],
                    |row| Ok(ColoquioMessage {
                        msg_id: row.get(0)?,
                        channel_id: row.get(1)?,
                        author_id: row.get(2)?,
                        role: row.get(3)?,
                        content: row.get(4)?,
                        turn: row.get(5)?,
                        created_at: get_flexible_created_at(row, 6)?,
                        metadata: row.get(7)?,
                    }),
                )?;
                Ok(msg)
            });
            match result {
                Ok(msg) => return Ok(msg),
                Err(e) => {
                    attempt += 1;
                    if attempt >= max_retries {
                        return Err(e);
                    }
                    let delay = std::time::Duration::from_millis(50 * 2u64.pow(attempt as u32));
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    /// Advance a reader's cursor in a channel. Never moves backwards.
    pub async fn mark_read(&self, channel_id: &str, reader_id: &str, turn: i64) -> Result<()> {
        let channel_id = channel_id.to_string();
        let reader_id = reader_id.to_string();
        tokio::task::block_in_place(|| {
            let conn = self.conn.lock().expect("coloquio mutex poisoned");
            conn.execute(
                "INSERT INTO coloquio_read_state (channel_id, reader_id, last_read_turn, updated_at)
                 VALUES (?1, ?2, ?3, unixepoch())
                 ON CONFLICT(channel_id, reader_id) DO UPDATE SET
                     last_read_turn = MAX(last_read_turn, excluded.last_read_turn),
                     updated_at = unixepoch()",
                params![channel_id, reader_id, turn],
            )?;
            Ok(())
        })
    }

    /// Per-channel unread counts for one reader, most-unread first.
    pub async fn unread_summary(&self, reader_id: &str) -> Result<Vec<UnreadSummary>> {
        let reader_id = reader_id.to_string();
        tokio::task::block_in_place(|| {
            let conn = self.conn.lock().expect("coloquio mutex poisoned");
            let mut stmt = conn.prepare(
                "SELECT c.channel_id, c.name,
                        COALESCE(MAX(m.turn), 0) AS last_turn,
                        COALESCE(r.last_read_turn, 0) AS last_read,
                        COUNT(CASE WHEN m.turn > COALESCE(r.last_read_turn, 0) THEN 1 END) AS unread
                 FROM coloquio_channels c
                 LEFT JOIN coloquio_messages m ON m.channel_id = c.channel_id
                 LEFT JOIN coloquio_read_state r
                        ON r.channel_id = c.channel_id AND r.reader_id = ?1
                 GROUP BY c.channel_id
                 ORDER BY unread DESC, last_turn DESC",
            )?;
            let rows = stmt.query_map(params![reader_id], |row| {
                Ok(UnreadSummary {
                    channel_id: row.get(0)?,
                    name: row.get(1)?,
                    last_turn: row.get(2)?,
                    last_read_turn: row.get(3)?,
                    unread_count: row.get(4)?,
                })
            })?;
            Ok(rows.flatten().collect())
        })
    }

    /// Messages after the reader's cursor in a channel, oldest first.
    /// Does NOT advance the cursor — callers decide via `mark_read`.
    pub async fn get_new_messages(&self, channel_id: &str, reader_id: &str, limit: i64) -> Result<Vec<ColoquioMessage>> {
        let channel_id = channel_id.to_string();
        let reader_id = reader_id.to_string();
        tokio::task::block_in_place(|| {
            let conn = self.conn.lock().expect("coloquio mutex poisoned");
            let mut stmt = conn.prepare(
                "SELECT msg_id, channel_id, author_id, role, content, turn, created_at, metadata
                 FROM coloquio_messages
                 WHERE channel_id = ?1
                   AND turn > COALESCE((SELECT last_read_turn FROM coloquio_read_state
                                        WHERE channel_id = ?1 AND reader_id = ?2), 0)
                 ORDER BY turn ASC
                 LIMIT ?3",
            )?;
            let rows = stmt.query_map(params![channel_id, reader_id, limit], |row| {
                Ok(ColoquioMessage {
                    msg_id: row.get(0)?,
                    channel_id: row.get(1)?,
                    author_id: row.get(2)?,
                    role: row.get(3)?,
                    content: row.get(4)?,
                    turn: row.get(5)?,
                    created_at: get_flexible_created_at(row, 6)?,
                    metadata: row.get(7)?,
                })
            })?;
            Ok(rows.flatten().collect())
        })
    }

    // ─── COLLABORATIVE DOCUMENTS ─────────────────────────────────────────────

    pub async fn list_documents(&self) -> Result<Vec<CollabDoc>> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.lock().expect("coloquio mutex poisoned");
            let mut stmt = conn.prepare(
                "SELECT doc_id, title, content, created_by, updated_by, version, created_at, updated_at
                 FROM collab_docs ORDER BY updated_at DESC"
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(CollabDoc {
                    doc_id: row.get(0)?,
                    title: row.get(1)?,
                    content: row.get(2)?,
                    created_by: row.get(3)?,
                    updated_by: row.get(4)?,
                    version: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            })?;
            Ok(rows.flatten().collect())
        })
    }

    pub async fn create_document(&self, title: &str, created_by: &str) -> Result<CollabDoc> {
        let doc_id = Uuid::new_v4().to_string();
        let title = title.to_string();
        let created_by = created_by.to_string();
        tokio::task::block_in_place(|| {
            let conn = self.conn.lock().expect("coloquio mutex poisoned");
            conn.execute(
                "INSERT INTO collab_docs (doc_id, title, created_by, updated_by) VALUES (?1, ?2, ?3, ?3)",
                params![doc_id, title, created_by],
            )?;
            let doc = conn.query_row(
                "SELECT doc_id, title, content, created_by, updated_by, version, created_at, updated_at
                 FROM collab_docs WHERE doc_id = ?1",
                params![doc_id],
                |row| Ok(CollabDoc {
                    doc_id: row.get(0)?,
                    title: row.get(1)?,
                    content: row.get(2)?,
                    created_by: row.get(3)?,
                    updated_by: row.get(4)?,
                    version: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                }),
            )?;
            conn.execute(
                "INSERT INTO collab_doc_snapshots (doc_id, version, title, content, updated_by, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![doc.doc_id, doc.version, doc.title, doc.content, doc.updated_by, doc.updated_at],
            )?;
            Ok(doc)
        })
    }

    pub async fn get_document(&self, doc_id: &str) -> Result<Option<CollabDoc>> {
        let doc_id = doc_id.to_string();
        tokio::task::block_in_place(|| {
            let conn = self.conn.lock().expect("coloquio mutex poisoned");
            let result = conn.query_row(
                "SELECT doc_id, title, content, created_by, updated_by, version, created_at, updated_at
                 FROM collab_docs WHERE doc_id = ?1",
                params![doc_id],
                |row| Ok(CollabDoc {
                    doc_id: row.get(0)?,
                    title: row.get(1)?,
                    content: row.get(2)?,
                    created_by: row.get(3)?,
                    updated_by: row.get(4)?,
                    version: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                }),
            );
            match result {
                Ok(doc) => Ok(Some(doc)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
    }

    pub async fn update_document(&self, doc_id: &str, title: &str, content: &str, updated_by: &str, expected_version: Option<i64>) -> Result<CollabDoc> {
        let doc_id = doc_id.to_string();
        let title = title.to_string();
        let content = content.to_string();
        let updated_by = updated_by.to_string();
        tokio::task::block_in_place(move || {
            let conn = self.conn.lock().expect("coloquio mutex poisoned");
            if let Some(exp_ver) = expected_version {
                let current_ver: i64 = conn.query_row(
                    "SELECT version FROM collab_docs WHERE doc_id = ?1",
                    params![doc_id],
                    |row| row.get(0),
                )?;
                if current_ver != exp_ver {
                    let doc = conn.query_row(
                        "SELECT doc_id, title, content, created_by, updated_by, version, created_at, updated_at
                         FROM collab_docs WHERE doc_id = ?1",
                        params![doc_id],
                        |row| Ok(CollabDoc {
                            doc_id: row.get(0)?,
                            title: row.get(1)?,
                            content: row.get(2)?,
                            created_by: row.get(3)?,
                            updated_by: row.get(4)?,
                            version: row.get(5)?,
                            created_at: row.get(6)?,
                            updated_at: row.get(7)?,
                        }),
                    )?;
                    return Err(anyhow::anyhow!("CONFLICT: version mismatch (expected {}, current {}):\n{}", exp_ver, current_ver, doc.content));
                }
            }
            conn.execute(
                "UPDATE collab_docs SET title = ?1, content = ?2, updated_by = ?3, version = version + 1, updated_at = unixepoch()
                 WHERE doc_id = ?4",
                params![title, content, updated_by, doc_id],
            )?;
            let doc = conn.query_row(
                "SELECT doc_id, title, content, created_by, updated_by, version, created_at, updated_at
                 FROM collab_docs WHERE doc_id = ?1",
                params![doc_id],
                |row| Ok(CollabDoc {
                    doc_id: row.get(0)?,
                    title: row.get(1)?,
                    content: row.get(2)?,
                    created_by: row.get(3)?,
                    updated_by: row.get(4)?,
                    version: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                }),
            )?;
            conn.execute(
                "INSERT INTO collab_doc_snapshots (doc_id, version, title, content, updated_by, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![doc.doc_id, doc.version, doc.title, doc.content, doc.updated_by, doc.updated_at],
            )?;
            Ok(doc)
        })
    }

    pub async fn append_to_document(&self, doc_id: &str, section: &str, appended_by: &str) -> Result<CollabDoc> {
        let doc_id = doc_id.to_string();
        let section = section.to_string();
        let appended_by = appended_by.to_string();
        tokio::task::block_in_place(move || {
            let conn = self.conn.lock().expect("coloquio mutex poisoned");
            // Atomic: read current content, append, write back — all inside one connection lock
            let current: String = conn.query_row(
                "SELECT content FROM collab_docs WHERE doc_id = ?1",
                rusqlite::params![doc_id],
                |row| row.get(0),
            )?;
            let new_content = if current.ends_with('\n') || current.is_empty() {
                format!("{}{}", current, section)
            } else {
                format!("{}\n{}", current, section)
            };
            conn.execute(
                "UPDATE collab_docs SET content = ?1, updated_by = ?2, version = version + 1, updated_at = unixepoch() WHERE doc_id = ?3",
                rusqlite::params![new_content, appended_by, doc_id],
            )?;
            let doc = conn.query_row(
                "SELECT doc_id, title, content, created_by, updated_by, version, created_at, updated_at FROM collab_docs WHERE doc_id = ?1",
                rusqlite::params![doc_id],
                |row| Ok(CollabDoc {
                    doc_id: row.get(0)?,
                    title: row.get(1)?,
                    content: row.get(2)?,
                    created_by: row.get(3)?,
                    updated_by: row.get(4)?,
                    version: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                }),
            )?;
            conn.execute(
                "INSERT INTO collab_doc_snapshots (doc_id, version, title, content, updated_by, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![doc.doc_id, doc.version, doc.title, doc.content, doc.updated_by, doc.updated_at],
            )?;
            Ok(doc)
        })
    }

    pub async fn list_document_snapshots(&self, doc_id: &str) -> Result<Vec<CollabDocSnapshot>> {
        let doc_id = doc_id.to_string();
        tokio::task::block_in_place(move || {
            let conn = self.conn.lock().expect("coloquio mutex poisoned");
            let mut stmt = conn.prepare(
                "SELECT doc_id, version, title, content, updated_by, updated_at
                 FROM collab_doc_snapshots WHERE doc_id = ?1 ORDER BY version DESC"
            )?;
            let rows = stmt.query_map(params![doc_id], |row| {
                Ok(CollabDocSnapshot {
                    doc_id: row.get(0)?,
                    version: row.get(1)?,
                    title: row.get(2)?,
                    content: row.get(3)?,
                    updated_by: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })?;
            Ok(rows.flatten().collect())
        })
    }

    pub async fn get_document_snapshot(&self, doc_id: &str, version: i64) -> Result<Option<CollabDocSnapshot>> {
        let doc_id = doc_id.to_string();
        tokio::task::block_in_place(move || {
            let conn = self.conn.lock().expect("coloquio mutex poisoned");
            let result = conn.query_row(
                "SELECT doc_id, version, title, content, updated_by, updated_at
                 FROM collab_doc_snapshots WHERE doc_id = ?1 AND version = ?2",
                params![doc_id, version],
                |row| Ok(CollabDocSnapshot {
                    doc_id: row.get(0)?,
                    version: row.get(1)?,
                    title: row.get(2)?,
                    content: row.get(3)?,
                    updated_by: row.get(4)?,
                    updated_at: row.get(5)?,
                }),
            );
            match result {
                Ok(snap) => Ok(Some(snap)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
    }

    pub async fn delete_document(&self, doc_id: &str) -> Result<bool> {
        let doc_id = doc_id.to_string();
        tokio::task::block_in_place(move || {
            let conn = self.conn.lock().expect("coloquio mutex poisoned");
            conn.execute("DELETE FROM collab_doc_snapshots WHERE doc_id = ?1", rusqlite::params![doc_id])?;
            let rows = conn.execute("DELETE FROM collab_docs WHERE doc_id = ?1", rusqlite::params![doc_id])?;
            Ok(rows > 0)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_unread_lifecycle() {
        let db = ColoquioDb::in_memory().unwrap();
        db.create_channel("daily", "Trabajo diario").await.unwrap();
        db.post_message("daily", "alice", "human", "hello team", "{}").await.unwrap();
        db.post_message("daily", "agent-1", "agent", "present", "{}").await.unwrap();
        db.post_message("daily", "agent-2", "agent", "ready", "{}").await.unwrap();

        // agent-3 never read anything: 3 unread
        let summary = db.unread_summary("agent-3").await.unwrap();
        assert_eq!(summary.len(), 1);
        assert_eq!(summary[0].unread_count, 3);
        assert_eq!(summary[0].last_turn, 3);

        // agent-3 catches up to turn 2: 1 unread left
        db.mark_read("daily", "agent-3", 2).await.unwrap();
        let summary = db.unread_summary("agent-3").await.unwrap();
        assert_eq!(summary[0].unread_count, 1);
        assert_eq!(summary[0].last_read_turn, 2);

        // new messages returns only turn 3
        let fresh = db.get_new_messages("daily", "agent-3", 50).await.unwrap();
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].turn, 3);
        assert_eq!(fresh[0].author_id, "agent-2");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_mark_read_never_goes_backwards() {
        let db = ColoquioDb::in_memory().unwrap();
        db.create_channel("c1", "c1").await.unwrap();
        db.post_message("c1", "a", "agent", "m1", "{}").await.unwrap();
        db.mark_read("c1", "r", 5).await.unwrap();
        db.mark_read("c1", "r", 2).await.unwrap(); // attempt regression
        let summary = db.unread_summary("r").await.unwrap();
        assert_eq!(summary[0].last_read_turn, 5);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_readers_are_independent() {
        let db = ColoquioDb::in_memory().unwrap();
        db.create_channel("c1", "c1").await.unwrap();
        db.post_message("c1", "a", "agent", "m1", "{}").await.unwrap();
        db.post_message("c1", "a", "agent", "m2", "{}").await.unwrap();
        db.mark_read("c1", "reader-1", 2).await.unwrap();
        assert_eq!(db.unread_summary("reader-1").await.unwrap()[0].unread_count, 0);
        assert_eq!(db.unread_summary("reader-2").await.unwrap()[0].unread_count, 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_post_rejects_junk_channel_autocreate() {
        let db = ColoquioDb::in_memory().unwrap();
        // Whole-sentence channel id (failed NL parsing) must NOT auto-create
        let err = db.post_message(
            "publicar en mision-activa mi informe ejecutivo con el estado real",
            "agent", "agent", "contenido", "{}",
        ).await;
        assert!(err.is_err());
        assert_eq!(db.list_channels().await.unwrap().len(), 0);

        // Valid slug auto-creates fine
        assert!(db.post_message("mision-activa", "agent", "agent", "hola", "{}").await.is_ok());

        // Pre-existing channel with non-slug id still accepts posts (backward compat)
        db.create_channel("canal con espacios legacy", "Legacy").await.unwrap();
        assert!(db.post_message("canal con espacios legacy", "agent", "agent", "ok", "{}").await.is_ok());
    }

    #[test]
    fn test_is_valid_channel_slug() {
        assert!(is_valid_channel_slug("mision-activa"));
        assert!(is_valid_channel_slug("verificacion_m21"));
        assert!(!is_valid_channel_slug("frase entera con espacios"));
        assert!(!is_valid_channel_slug(""));
        assert!(!is_valid_channel_slug(&"x".repeat(65)));
    }

    #[test]
    fn test_extract_mentions() {
        assert_eq!(extract_mentions("hola @agent-1 revisa esto"), vec!["agent-1"]);
        assert_eq!(extract_mentions("@a @b @a dup"), vec!["a", "b"]);
        // emails are not mentions
        assert!(extract_mentions("escribe a jose@example.com").is_empty());
        // punctuation-adjacent mentions still work
        assert_eq!(extract_mentions("(@agent-3: investigate)"), vec!["agent-3"]);
        assert!(extract_mentions("sin menciones aqui").is_empty());
        assert!(extract_mentions("@ solo arroba").is_empty());
    }
}
