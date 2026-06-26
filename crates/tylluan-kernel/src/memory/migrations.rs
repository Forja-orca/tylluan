use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use tracing::info;

/// Migration structure defining a SQL patch from version N to N+1.
struct Migration {
    version: i32,
    name: &'static str,
    sql: &'static str,
}

pub struct MigrationRunner;

impl MigrationRunner {
    /// Run all pending migrations on a given connection.
    pub fn run(conn: &mut Connection, schema_name: &str) -> Result<()> {
        info!("🔧 [Migrations] Checking schema '{}'...", schema_name);

        // Ensure we have a migrations table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS _migrations (
                schema_name TEXT NOT NULL,
                version INTEGER NOT NULL,
                applied_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY(schema_name, version)
            )",
            [],
        )?;

        // Check if this is a fresh database (no migrations recorded)
        let has_migrations: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM _migrations WHERE schema_name = ?1)",
            params![schema_name],
            |row| row.get(0),
        ).unwrap_or(false);

        // If no migrations exist, init_schema already created the latest schema.
        // Seed the _migrations table up to the current max version so future
        // runs never attempt duplicate ALTER TABLE / CREATE TABLE operations.
        if !has_migrations {
            let latest_version = match schema_name {
                "silva" => 4,
                "hybrid" => 1,
                _ => 0,
            };
            if latest_version > 0 {
                for v in 1..=latest_version {
                    conn.execute(
                        "INSERT INTO _migrations (schema_name, version) VALUES (?1, ?2)
                         ON CONFLICT(schema_name, version) DO NOTHING",
                        params![schema_name, v],
                    )?;
                }
                info!("🔧 [Migrations] Fresh database '{}' — seeded migration records 1..={} (init_schema has latest schema)", schema_name, latest_version);
            } else {
                info!("🔧 [Migrations] Fresh database '{}' - skipping migrations (init_schema has latest schema)", schema_name);
            }
            return Ok(());
        }

        let current_version: i32 = conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM _migrations WHERE schema_name = ?1",
            params![schema_name],
            |row| row.get(0),
        ).unwrap_or(0);

        let migrations = match schema_name {
            "silva" => Self::silva_migrations(),
            "hybrid" => Self::hybrid_migrations(),
            _ => vec![],
        };

        for migration in migrations {
            if migration.version > current_version {
                info!("🚀 [Migrations] Applying {} v{}: {}", schema_name, migration.version, migration.name);
                
                let tx = conn.transaction()?;
                match tx.execute_batch(migration.sql) {
                    Ok(_) => {}
                    Err(e) => {
                        let err_msg = e.to_string();
                        if err_msg.contains("duplicate column name") {
                            info!("⚠️ [Migrations] Column already exists in {}, skipping SQL step.", schema_name);
                        } else {
                            return Err(e).with_context(|| format!("Migration v{} failed for {}", migration.version, schema_name));
                        }
                    }
                }
                
                tx.execute(
                    "INSERT INTO _migrations (schema_name, version) VALUES (?1, ?2)
                     ON CONFLICT(schema_name, version) DO NOTHING",
                    params![schema_name, migration.version],
                )?;
                
                tx.commit()?;
            }
        }

        Ok(())
    }

    fn silva_migrations() -> Vec<Migration> {
        vec![
            Migration {
                version: 1,
                name: "Initial Context Columns",
                sql: "ALTER TABLE nodes ADD COLUMN topic_key TEXT;
                      ALTER TABLE nodes ADD COLUMN conflicted INTEGER DEFAULT 0;",
            },
            Migration {
                version: 2,
                name: "Clustering Table",
                sql: "CREATE TABLE IF NOT EXISTS cluster_summaries (
                        cluster_id INTEGER PRIMARY KEY,
                        summary TEXT NOT NULL,
                        members TEXT NOT NULL,
                        created_at DATETIME DEFAULT CURRENT_TIMESTAMP
                      );",
            },
            Migration {
                version: 3,
                name: "IVF Index Support",
                sql: "ALTER TABLE nodes ADD COLUMN cluster_id INTEGER;
                      CREATE TABLE IF NOT EXISTS cluster_centroids (
                        cluster_id INTEGER PRIMARY KEY,
                        centroid_vector BLOB NOT NULL,
                        model_id TEXT NOT NULL,
                        created_at DATETIME DEFAULT CURRENT_TIMESTAMP
                      );
                      CREATE INDEX IF NOT EXISTS idx_nodes_cluster ON nodes(cluster_id);",
            },
            Migration {
                version: 4,
                name: "Normalize Timestamps to TEXT",
                sql: "UPDATE nodes SET created_at = datetime(CAST(created_at AS INTEGER), 'unixepoch')
                       WHERE typeof(created_at) = 'integer';
                      UPDATE nodes SET updated_at = datetime(CAST(updated_at AS INTEGER), 'unixepoch')
                       WHERE typeof(updated_at) = 'integer';",
            },
        ]
    }

    fn hybrid_migrations() -> Vec<Migration> {
        vec![
            Migration {
                version: 1,
                name: "Timestamps for Tiered Search",
                sql: "ALTER TABLE documents ADD COLUMN created_at DATETIME DEFAULT CURRENT_TIMESTAMP;",
            },
        ]
    }
}
