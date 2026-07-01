use anyhow::Result;
use tracing::info;

impl super::SilvaDB {
    pub(super) async fn init_schema(&self) -> Result<()> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            conn.execute_batch("PRAGMA journal_mode = WAL;")?;

            conn.execute_batch(
                "PRAGMA synchronous = NORMAL;
                 PRAGMA cache_size = -64000;
                 PRAGMA temp_store = MEMORY;
                 PRAGMA mmap_size = 268435456;
                 PRAGMA page_size = 4096;"
            )?;

            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS nodes (
                    id TEXT PRIMARY KEY,
                    type TEXT NOT NULL,
                    content TEXT NOT NULL,
                    metadata TEXT DEFAULT '{}',
                    weight REAL DEFAULT 1.0,
                    protected INTEGER DEFAULT 0,
                    topic_key TEXT,
                    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                    stigmergy_heat REAL DEFAULT 0.0
                );

                CREATE TABLE IF NOT EXISTS edges (
                    source TEXT NOT NULL,
                    target TEXT NOT NULL,
                    type TEXT NOT NULL,
                    metadata TEXT DEFAULT '{}',
                    weight REAL DEFAULT 1.0,
                    valid_from INTEGER,
                    valid_until INTEGER,
                    PRIMARY KEY (source, target, type)
                );")?;

            let schema_version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap_or(0);
            const SCHEMA_VERSION: i32 = 11;

            if schema_version < 1 {
                let _ = conn.execute("ALTER TABLE nodes ADD COLUMN conflicted INTEGER NOT NULL DEFAULT 0", []);
                let _ = conn.execute("ALTER TABLE nodes ADD COLUMN topic_key TEXT", []);
                let _ = conn.execute("ALTER TABLE nodes ADD COLUMN protected INTEGER NOT NULL DEFAULT 0", []);
                let _ = conn.execute("ALTER TABLE nodes ADD COLUMN last_touched INTEGER", []);
            }
            if schema_version < 2 {
                let _ = conn.execute("ALTER TABLE nodes ADD COLUMN last_accessed INTEGER", []);
                let _ = conn.execute("ALTER TABLE nodes ADD COLUMN access_count INTEGER NOT NULL DEFAULT 0", []);
                let _ = conn.execute("ALTER TABLE nodes ADD COLUMN last_touched INTEGER", []);
            }
            if schema_version < 3 {
                conn.execute_batch("
                    CREATE TABLE IF NOT EXISTS guild_call_stats (
                        guild_name TEXT PRIMARY KEY,
                        total_calls INTEGER NOT NULL DEFAULT 0,
                        successful_calls INTEGER NOT NULL DEFAULT 0,
                        total_latency_ms INTEGER NOT NULL DEFAULT 0,
                        last_call_unix INTEGER NOT NULL DEFAULT 0
                     );
                ").ok();
            }
            if schema_version < 4 {
                conn.execute_batch("
                    CREATE INDEX IF NOT EXISTS idx_node_traces_agent ON node_traces(agent_id, touched_at DESC);
                    CREATE INDEX IF NOT EXISTS idx_nodes_weight ON nodes(weight DESC);
                ").ok();
            }
            if schema_version < 5 {
                let _ = conn.execute("ALTER TABLE nodes ADD COLUMN valid_from INTEGER", []);
                let _ = conn.execute("ALTER TABLE nodes ADD COLUMN valid_until INTEGER", []);
            }
            if schema_version < 6 {
                let _ = conn.execute("ALTER TABLE nodes ADD COLUMN shareable INTEGER NOT NULL DEFAULT 0", []);
            }
            if schema_version < 7 {
                let _ = conn.execute("ALTER TABLE edges ADD COLUMN valid_from INTEGER", []);
                let _ = conn.execute("ALTER TABLE edges ADD COLUMN valid_until INTEGER", []);
            }
            if schema_version < 8 {
                let _ = conn.execute("ALTER TABLE nodes ADD COLUMN salience_score REAL NOT NULL DEFAULT 1.0", []);
                tracing::info!("🌲 SilvaDB: added salience_score column");
            }
            if schema_version < 9 {
                conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS silva_kv (
                        key TEXT PRIMARY KEY,
                        value TEXT NOT NULL,
                        updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
                     );"
                ).ok();
                tracing::info!("🌲 SilvaDB: added silva_kv table (v9)");
            }
            if schema_version < 10 {
                let _ = conn.execute("ALTER TABLE nodes ADD COLUMN federation_source TEXT", []);
                // Backfill from metadata JSON for nodes already tagged via the old approach
                conn.execute_batch(
                    "UPDATE nodes
                     SET federation_source = json_extract(metadata, '$.federation_source')
                     WHERE federation_source IS NULL
                       AND json_extract(metadata, '$.federation_source') IS NOT NULL;"
                ).ok();
                tracing::info!("🌲 SilvaDB: added federation_source column + backfill (v10)");
            }
            if schema_version < 11 {
                conn.execute_batch(
                    "CREATE VIRTUAL TABLE IF NOT EXISTS nodes_fts USING fts5(
                        id UNINDEXED,
                        content,
                        metadata,
                        content=nodes,
                        content_rowid=rowid,
                        tokenize='porter unicode61'
                    );"
                )?;
                conn.execute("INSERT INTO nodes_fts(nodes_fts) VALUES('rebuild')", [])?;
                tracing::info!("🌲 SilvaDB: created nodes_fts FTS5 table + backfill (v11)");
            }
            if schema_version < SCHEMA_VERSION {
                conn.execute_batch(&format!("PRAGMA user_version = {}", SCHEMA_VERSION))?;
                tracing::info!("🌲 SilvaDB schema migrado a v{}", SCHEMA_VERSION);
            }

            conn.execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_nodes_type ON nodes(type);
                 CREATE INDEX IF NOT EXISTS idx_nodes_weight ON nodes(weight DESC);
                 CREATE INDEX IF NOT EXISTS idx_nodes_updated ON nodes(updated_at);
                 CREATE INDEX IF NOT EXISTS idx_nodes_topic ON nodes(topic_key);
                 CREATE INDEX IF NOT EXISTS idx_nodes_conflicted ON nodes(conflicted);
                 CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source);
                 CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target);

                 CREATE TABLE IF NOT EXISTS node_embeddings (
                     node_id TEXT PRIMARY KEY,
                     embedding BLOB NOT NULL,
                     model_id TEXT DEFAULT 'bge-m3',
                     model_name TEXT DEFAULT 'bge-m3',
                     model_hash TEXT,
                     dimensions INTEGER DEFAULT 1024,
                     FOREIGN KEY(node_id) REFERENCES nodes(id) ON DELETE CASCADE
                 );

                 CREATE TABLE IF NOT EXISTS node_communities (
                     node_id TEXT PRIMARY KEY,
                     cluster_id INTEGER NOT NULL,
                     updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                     FOREIGN KEY(node_id) REFERENCES nodes(id) ON DELETE CASCADE
                 );

                 CREATE TABLE IF NOT EXISTS cluster_centroids (
                     cluster_id TEXT PRIMARY KEY,
                     centroid_vector BLOB NOT NULL,
                     model_id TEXT DEFAULT 'bge-m3',
                     updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
                 );

                 CREATE TABLE IF NOT EXISTS node_traces (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     node_id TEXT NOT NULL,
                     agent_id TEXT NOT NULL,
                     touched_at INTEGER NOT NULL,
                     trace_type TEXT NOT NULL
                 );

                 CREATE INDEX IF NOT EXISTS idx_node_traces_node ON node_traces(node_id);
                 CREATE INDEX IF NOT EXISTS idx_node_traces_time ON node_traces(touched_at DESC);

                 CREATE TABLE IF NOT EXISTS mcp_sessions (
                     agent_id TEXT PRIMARY KEY,
                     client_name TEXT NOT NULL,
                     last_active_unix INTEGER NOT NULL,
                     tool_count INTEGER NOT NULL DEFAULT 0,
                     last_intent TEXT,
                     last_guild TEXT,
                     created_unix INTEGER NOT NULL DEFAULT 0,
                     id TEXT NOT NULL
                 );"
            )?;

            let _ = conn.execute("ALTER TABLE nodes ADD COLUMN cluster_id INTEGER", []);
            let _ = conn.execute("ALTER TABLE node_embeddings ADD COLUMN model_name TEXT DEFAULT 'bge-m3'", []);
            let _ = conn.execute("ALTER TABLE node_embeddings ADD COLUMN model_hash TEXT", []);
            let _ = conn.execute("ALTER TABLE node_embeddings ADD COLUMN dimensions INTEGER DEFAULT 1024", []);

            Ok::<(), anyhow::Error>(())
        })?;

        info!("🌲 SilvaDB schema initialized (nodes + edges + agnostic embeddings).");
        Ok(())
    }
}
