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
                    stigmergy_heat REAL DEFAULT 0.0,
                    fsrs_stability REAL DEFAULT 14.0,
                    fsrs_difficulty REAL DEFAULT 0.3,
                    fsrs_last_review INTEGER DEFAULT 0,
                    content_hash TEXT DEFAULT '',
                    lifecycle_state TEXT NOT NULL DEFAULT 'active',
                    last_agent_access INTEGER NOT NULL DEFAULT 0,
                    reactivation_count INTEGER NOT NULL DEFAULT 0
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
            const SCHEMA_VERSION: i32 = 23;

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
            if schema_version < 13 {
                let _ = conn.execute("ALTER TABLE nodes ADD COLUMN fsrs_stability REAL NOT NULL DEFAULT 14.0", []);
                let _ = conn.execute("ALTER TABLE nodes ADD COLUMN fsrs_difficulty REAL NOT NULL DEFAULT 0.3", []);
                let _ = conn.execute("ALTER TABLE nodes ADD COLUMN fsrs_last_review INTEGER NOT NULL DEFAULT 0", []);
                tracing::info!("🌲 SilvaDB: added FSRS columns (v13)");
            }
            if schema_version < 14 {
                let _ = conn.execute("ALTER TABLE nodes ADD COLUMN content_hash TEXT DEFAULT ''", []);
                // Backfill content_hash for existing nodes using Rust-side SHA-256
                {
                    let nodes: Vec<(String, String)> = {
                        let mut stmt = conn.prepare(
                            "SELECT id, content FROM nodes WHERE content != '' AND content_hash = ''"
                        )?;
                        stmt.query_map([], |row| {
                            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                        })?
                        .filter_map(|r| r.ok())
                        .collect()
                    };
                    let mut update = conn.prepare("UPDATE nodes SET content_hash = ?1 WHERE id = ?2")?;
                    for (id, content) in &nodes {
                        use sha2::Digest;
                        let hash = format!("{:x}", sha2::Sha256::digest(content.as_bytes()));
                        let _ = update.execute(rusqlite::params![hash, id]);
                    }
                    if !nodes.is_empty() {
                        tracing::info!("🌲 SilvaDB: backfilled content_hash for {} nodes", nodes.len());
                    }
                }
                tracing::info!("🌲 SilvaDB: added content_hash column (v14)");
            }
            if schema_version < 15 {
                let _ = conn.execute("ALTER TABLE nodes ADD COLUMN provenance TEXT NOT NULL DEFAULT 'unverified'", []);
                tracing::info!("🌲 SilvaDB: added provenance column (v15)");
            }
            if schema_version < 16 {
                // J-8: hierarchical scope tag, format "user:<id>/session:<id>/agent:<id>"
                // (any level may be omitted). NULL = unscoped (visible to all, current behavior).
                let _ = conn.execute("ALTER TABLE nodes ADD COLUMN owner_scope TEXT", []);
                conn.execute_batch(
                    "CREATE INDEX IF NOT EXISTS idx_nodes_owner_scope ON nodes(owner_scope);"
                ).ok();
                tracing::info!("🌲 SilvaDB: added owner_scope column + index (v16)");
            }
            if schema_version < 17 {
                conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS a2a_tasks (
                        id TEXT PRIMARY KEY,
                        state TEXT NOT NULL DEFAULT 'submitted',
                        client_agent_id TEXT NOT NULL,
                        method TEXT NOT NULL,
                        params_json TEXT NOT NULL,
                        result_json TEXT,
                        grant_id TEXT,
                        created_at INTEGER NOT NULL,
                        updated_at INTEGER NOT NULL
                    );"
                ).ok();
                tracing::info!("🌲 SilvaDB: added a2a_tasks table (v17)");
            }
            if schema_version < 18 {
                // ADR-011 §Signal Loop: implicit-usefulness feedback for recall results.
                // useful: 0=unknown (still in resolution window), 1=useful, -1=not_useful.
                conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS recall_feedback (
                        id            INTEGER PRIMARY KEY AUTOINCREMENT,
                        memory_id     TEXT NOT NULL,
                        agent_id      TEXT NOT NULL,
                        task_hash     TEXT NOT NULL,
                        query_text    TEXT NOT NULL,
                        rank_position INTEGER NOT NULL,
                        useful        INTEGER NOT NULL DEFAULT 0,
                        accessed_at   TEXT NOT NULL DEFAULT (datetime('now')),
                        resolved_at   TEXT,
                        UNIQUE(memory_id, task_hash)
                    );
                     CREATE INDEX IF NOT EXISTS idx_recall_feedback_agent ON recall_feedback(agent_id);
                     CREATE INDEX IF NOT EXISTS idx_recall_feedback_useful ON recall_feedback(useful);"
                ).ok();
                tracing::info!("🌲 SilvaDB: added recall_feedback table (v18, ADR-011)");
            }
            if schema_version < 19 {
                // M40-P4: explicit confidence for evidence-based memory. `conflicted`
                // (v1) and `valid_until` (v?) already exist and already carry most of
                // the "is this still true" signal -- confidence is the one real gap:
                // no node has ever recorded how sure the system is about its own
                // content. Defaults to 1.0 (fully confident) so existing nodes don't
                // silently become "provisional" the moment this column appears.
                let _ = conn.execute("ALTER TABLE nodes ADD COLUMN confidence REAL NOT NULL DEFAULT 1.0", []);
                tracing::info!("🌲 SilvaDB: added confidence column (v19, M40-P4)");
            }
            if schema_version < 20 {
                // M40-P4 (second cut): explicit source/author/evidence beyond
                // generic `provenance`. Nullable — NULL means "not specified",
                // backward compatible with all existing nodes.
                let _ = conn.execute("ALTER TABLE nodes ADD COLUMN source TEXT", []);
                let _ = conn.execute("ALTER TABLE nodes ADD COLUMN author TEXT", []);
                let _ = conn.execute("ALTER TABLE nodes ADD COLUMN evidence_url TEXT", []);
                tracing::info!("🌲 SilvaDB: added source/author/evidence_url columns (v20, M40-P4)");
            }
            if schema_version < 21 {
                // ASI06: ingestion-time coherence gate for tylluan_remember. Deliberately
                // orthogonal to `status`/`memory_status()` (confidence-over-time) --
                // quarantine is a security/coherence-at-write-time signal, a node can be
                // `confirmed` in memory_status and `quarantined=1` at the same time, they
                // answer different questions. Defaults to 0 (not quarantined) so existing
                // nodes are unaffected. `quarantine_reason` is nullable: NULL means never
                // quarantined or reason not recorded.
                let _ = conn.execute("ALTER TABLE nodes ADD COLUMN quarantined INTEGER NOT NULL DEFAULT 0", []);
                let _ = conn.execute("ALTER TABLE nodes ADD COLUMN quarantine_reason TEXT", []);
                tracing::info!("🌲 SilvaDB: added quarantined/quarantine_reason columns (v21, ASI06)");
            }
            if schema_version < 22 {
                // A2A external agents (runtime config via REST, persisted in SilvaDB):
                // which arbitrary external A2A agents the kernel may delegate to. The
                // auth_token column is a bearer secret for the REMOTE agent -- stored
                // locally, never logged. enabled=0 keeps the agent registered but
                // disallows delegation (fail-closed).
                conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS a2a_agents (
                        id          TEXT PRIMARY KEY,
                        name        TEXT NOT NULL,
                        url         TEXT NOT NULL,
                        auth_token  TEXT NOT NULL DEFAULT '',
                        enabled     INTEGER NOT NULL DEFAULT 1,
                        created_at  INTEGER NOT NULL,
                        updated_at  INTEGER NOT NULL
                    );"
                ).ok();
                tracing::info!("🌲 SilvaDB: added a2a_agents table (v22)");
            }
            if schema_version < 23 {
                // ADR-012 Fase 1: Memory Lifecycle State Machine
                // lifecycle_state: active | quiet | consolidated | archived (4 estados, ADR-012 §2 Decision 1)
                // last_agent_access: unix timestamp del último acceso REAL de agente (recall/ingest), NO touch_node interno
                // reactivation_count: contador de reactivaciones archived→active (RAE barata §2.5)
                let _ = conn.execute(
                    "ALTER TABLE nodes ADD COLUMN lifecycle_state TEXT NOT NULL DEFAULT 'active'",
                    [],
                );
                let _ = conn.execute(
                    "ALTER TABLE nodes ADD COLUMN last_agent_access INTEGER NOT NULL DEFAULT 0",
                    [],
                );
                let _ = conn.execute(
                    "ALTER TABLE nodes ADD COLUMN reactivation_count INTEGER NOT NULL DEFAULT 0",
                    [],
                );
                // Backfill DERIVADO: no DEFAULT 'active' ciego.
                // Nodos con updated_at > 30 días → quiet, resto active.
                // Nodos protected/identity NUNCA se degradan a quiet — son
                // inmunes al pruning y su lifecycle es estructural (fix review).
                // last_agent_access = 0 (nunca accedido por agente registrado aún).
                // reactivation_count = 0.
                conn.execute_batch(
                    "UPDATE nodes SET lifecycle_state = CASE
                        WHEN updated_at < datetime('now', '-30 days') THEN 'quiet'
                        ELSE 'active'
                     END
                     WHERE protected = 0 AND type != 'identity';
                     UPDATE nodes SET last_agent_access = 0;
                     UPDATE nodes SET reactivation_count = 0;",
                )?;
                tracing::info!("🌲 SilvaDB: added lifecycle_state, last_agent_access, reactivation_count (v23, ADR-012 Fase 1)");
            }
            if schema_version < SCHEMA_VERSION {
                conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION}"))?;
                tracing::info!("🌲 SilvaDB schema migrado a v{}", SCHEMA_VERSION);
            }

            conn.execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_nodes_type ON nodes(type);
                 CREATE INDEX IF NOT EXISTS idx_nodes_weight ON nodes(weight DESC);
                 CREATE INDEX IF NOT EXISTS idx_nodes_updated ON nodes(updated_at);
                 CREATE INDEX IF NOT EXISTS idx_nodes_topic ON nodes(topic_key);
                 CREATE INDEX IF NOT EXISTS idx_nodes_conflicted ON nodes(conflicted);
                 CREATE INDEX IF NOT EXISTS idx_nodes_quarantined ON nodes(quarantined);
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
                 );

                 CREATE TABLE IF NOT EXISTS hnsw_index (
                     id INTEGER PRIMARY KEY CHECK (id = 1),
                     index_blob BLOB NOT NULL,
                     node_count INTEGER NOT NULL,
                     built_at TEXT NOT NULL
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
