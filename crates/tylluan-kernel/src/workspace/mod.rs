//! F9-0 (ADR-026, aceptado parcialmente): intermediario de espacio de trabajo.
//!
//! Base propia `workspace.db` (siempre vía [`crate::config::open_db`], que es el
//! unico punto de cifrado en reposo, y bajo el DATA_DIR efectivo vía
//! [`crate::config::data_file`]) con:
//!
//! - `events` **solo añadir** (triggers que abortan UPDATE/DELETE), la fuente de
//!   verdad de lo que pasa con cada tarea.
//! - `tasks` materializada desde `events` (se puede reconstruir con
//!   [`WorkspaceStore::rebuild_tasks_from_events`]).
//! - `claims` con arrendamiento de 120 s y latido de 15 s; un reclamo de
//!   escritura se hace en **una transaccion** que rechaza rutas solapadas.
//! - `agents` con su ultimo latido.
//!
//! Esta fase NO toca git, no asigna tareas automaticamente, no tiene tool MCP ni
//! UI: solo el esquema y las operaciones atomicas que usara F9-1.
//!
//! Limitacion documentada: SQLite no puede expresar «dos globs se solapan» como
//! constraint; el indice unico protege el MISMO glob (y una sola reclamacion
//! activa por tarea) y la comprobacion de solape se hace dentro de la
//! transaccion con [`globs_overlap`] (regla conservadora).

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Arrendamiento de un reclamo: 120 s.
pub const LEASE_MS: i64 = 120_000;
/// Latido esperado de un agente con reclamo vivo: 15 s.
pub const HEARTBEAT_MS: i64 = 15_000;

/// Tipos de evento permitidos (los que fija el contrato de F9-0).
pub const EVENT_KINDS: &[&str] = &[
    "created",
    "claimed",
    "heartbeat",
    "released",
    "expired",
    "review",
    "merged",
    "failed",
    "requeued",
];

/// Estado de una tarea.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Queued,
    Claimed,
    Review,
    Merged,
    Failed,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Claimed => "claimed",
            Self::Review => "review",
            Self::Merged => "merged",
            Self::Failed => "failed",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "claimed" => Self::Claimed,
            "review" => Self::Review,
            "merged" => Self::Merged,
            "failed" => Self::Failed,
            _ => Self::Queued,
        }
    }
}

/// Especificacion de una tarea nueva.
#[derive(Debug, Clone)]
pub struct TaskSpec {
    pub id: String,
    pub title: String,
    pub paths_glob: String,
    pub priority: i64,
    pub base_commit: String,
    pub acceptance_cmd: String,
}

impl TaskSpec {
    pub fn new(id: impl Into<String>, paths_glob: impl Into<String>) -> Self {
        let id = id.into();
        Self {
            title: id.clone(),
            paths_glob: paths_glob.into(),
            priority: 0,
            base_commit: String::new(),
            acceptance_cmd: String::new(),
            id,
        }
    }
}

/// Reclamo concedido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    pub task_id: String,
    pub agent_id: String,
    pub epoch: i64,
    pub token_hash: String,
    pub expires_at: i64,
}

/// Por que se rechaza un reclamo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimRejection {
    UnknownTask,
    NotQueued(TaskStatus),
    Conflict {
        task_id: String,
        agent_id: String,
        path_glob: String,
    },
}

/// Resultado atomico de un reclamo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimOutcome {
    Claimed(Claim),
    Rejected(ClaimRejection),
}

/// Una reclamacion activa (para comprobaciones e inspeccion).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveClaim {
    pub task_id: String,
    pub agent_id: String,
    pub path_glob: String,
    pub expires_at: i64,
}

/// Intermediario del espacio de trabajo. Barato de clonar: solo guarda la ruta.
#[derive(Debug, Clone)]
pub struct WorkspaceStore {
    path: PathBuf,
}

impl WorkspaceStore {
    /// Abre `<DATA_DIR>/workspace.db` (respeta `TYLLUAN_DATA_DIR`).
    pub fn open_default() -> Result<Self> {
        Self::open(crate::config::data_file("workspace.db"))
    }

    /// Abre (creando si hace falta) una base de workspace y aplica el esquema.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let store = Self { path: path.into() };
        store.init_schema()?;
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn conn(&self) -> Result<Connection> {
        let conn = crate::config::open_db(&self.path)
            .with_context(|| format!("abriendo workspace db {}", self.path.display()))?;
        conn.busy_timeout(Duration::from_secs(5))?;
        Ok(conn)
    }

    /// Crea el esquema si no existe. Idempotente.
    pub fn init_schema(&self) -> Result<()> {
        let conn = self.conn()?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS events (
                seq      INTEGER PRIMARY KEY AUTOINCREMENT,
                ts       INTEGER NOT NULL,
                agent_id TEXT NOT NULL,
                task_id  TEXT NOT NULL DEFAULT '',
                kind     TEXT NOT NULL,
                payload  TEXT NOT NULL DEFAULT '{}'
            );
            CREATE TRIGGER IF NOT EXISTS events_append_only_update
            BEFORE UPDATE ON events BEGIN
                SELECT RAISE(ABORT, 'events is append-only: UPDATE rejected');
            END;
            CREATE TRIGGER IF NOT EXISTS events_append_only_delete
            BEFORE DELETE ON events BEGIN
                SELECT RAISE(ABORT, 'events is append-only: DELETE rejected');
            END;

            CREATE TABLE IF NOT EXISTS tasks (
                id             TEXT PRIMARY KEY,
                title          TEXT NOT NULL DEFAULT '',
                paths_glob     TEXT NOT NULL DEFAULT '',
                priority       INTEGER NOT NULL DEFAULT 0,
                base_commit    TEXT NOT NULL DEFAULT '',
                acceptance_cmd TEXT NOT NULL DEFAULT '',
                status         TEXT NOT NULL DEFAULT 'queued',
                dirty_paths    TEXT NOT NULL DEFAULT '',
                created_seq    INTEGER,
                updated_seq    INTEGER
            );

            CREATE TABLE IF NOT EXISTS claims (
                task_id     TEXT NOT NULL,
                agent_id    TEXT NOT NULL,
                path_glob   TEXT NOT NULL,
                token_hash  TEXT NOT NULL,
                epoch       INTEGER NOT NULL,
                expires_at  INTEGER NOT NULL,
                claimed_at  INTEGER NOT NULL,
                released_at INTEGER,
                PRIMARY KEY (task_id, epoch)
            );
            -- Una sola reclamacion activa por tarea.
            CREATE UNIQUE INDEX IF NOT EXISTS idx_claims_one_active_per_task
                ON claims(task_id) WHERE released_at IS NULL;
            -- El MISMO glob no puede tener dos reclamaciones activas.
            CREATE UNIQUE INDEX IF NOT EXISTS idx_claims_one_active_glob
                ON claims(path_glob) WHERE released_at IS NULL;
            CREATE INDEX IF NOT EXISTS idx_claims_active_expiry
                ON claims(expires_at) WHERE released_at IS NULL;

            CREATE TABLE IF NOT EXISTS agents (
                id             TEXT PRIMARY KEY,
                kind           TEXT NOT NULL DEFAULT 'agent',
                capabilities   TEXT NOT NULL DEFAULT '{}',
                state          TEXT NOT NULL DEFAULT 'idle',
                last_heartbeat INTEGER NOT NULL DEFAULT 0
            );
            "#,
        )?;
        Ok(())
    }

    /// Añade un evento (solo añadir; `kind` debe estar en [`EVENT_KINDS`]).
    pub fn append_event(
        &self,
        ts: i64,
        agent_id: &str,
        task_id: &str,
        kind: &str,
        payload: &Value,
    ) -> Result<i64> {
        if !EVENT_KINDS.contains(&kind) {
            anyhow::bail!("kind de evento desconocido: {kind}");
        }
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO events (ts, agent_id, task_id, kind, payload) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![ts, agent_id, task_id, kind, payload.to_string()],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Crea una tarea `queued` y su evento `created`.
    pub fn create_task(&self, ts: i64, spec: &TaskSpec) -> Result<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT INTO tasks (id, title, paths_glob, priority, base_commit, acceptance_cmd, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'queued')",
            params![
                spec.id,
                spec.title,
                spec.paths_glob,
                spec.priority,
                spec.base_commit,
                spec.acceptance_cmd
            ],
        )?;
        let payload = json!({
            "title": spec.title,
            "paths_glob": spec.paths_glob,
            "priority": spec.priority,
            "base_commit": spec.base_commit,
            "acceptance_cmd": spec.acceptance_cmd,
        });
        tx.execute(
            "INSERT INTO events (ts, agent_id, task_id, kind, payload) VALUES (?1, 'broker', ?2, 'created', ?3)",
            params![ts, spec.id, payload.to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn task_status(&self, task_id: &str) -> Result<Option<TaskStatus>> {
        let conn = self.conn()?;
        let status: Option<String> = conn
            .query_row("SELECT status FROM tasks WHERE id = ?1", params![task_id], |r| r.get(0))
            .optional()?;
        Ok(status.map(|s| TaskStatus::from_str(&s)))
    }

    pub fn task_glob(&self, task_id: &str) -> Result<Option<String>> {
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                "SELECT paths_glob FROM tasks WHERE id = ?1",
                params![task_id],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn task_dirty_paths(&self, task_id: &str) -> Result<String> {
        let conn = self.conn()?;
        Ok(conn
            .query_row("SELECT dirty_paths FROM tasks WHERE id = ?1", params![task_id], |r| r.get(0))
            .optional()?
            .unwrap_or_default())
    }

    /// Marca cambios sucios de una tarea (F9-1 los rellenara; en F9-0 solo se
    /// guarda el campo para que la caducidad sepa si va a `review` o `queued`).
    pub fn set_dirty_paths(&self, task_id: &str, dirty_paths: &str) -> Result<bool> {
        let conn = self.conn()?;
        let changed = conn.execute(
            "UPDATE tasks SET dirty_paths = ?2 WHERE id = ?1",
            params![task_id, dirty_paths],
        )?;
        Ok(changed > 0)
    }

    /// Reclamo atomico. `now_ms` lo pasa el llamante (tests deterministas).
    pub fn claim(
        &self,
        agent_id: &str,
        task_id: &str,
        path_glob: &str,
        now_ms: i64,
    ) -> Result<ClaimOutcome> {
        if agent_id.trim().is_empty() || task_id.trim().is_empty() || path_glob.trim().is_empty() {
            anyhow::bail!("agent_id, task_id y path_glob son obligatorios");
        }
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

        tx.execute(
            "INSERT INTO agents (id, state, last_heartbeat) VALUES (?1, 'active', ?2)
             ON CONFLICT(id) DO UPDATE SET state = 'active', last_heartbeat = excluded.last_heartbeat",
            params![agent_id, now_ms],
        )?;

        let task: Option<(String, String)> = tx
            .query_row(
                "SELECT status, paths_glob FROM tasks WHERE id = ?1",
                params![task_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((status, task_glob)) = task else {
            return Ok(ClaimOutcome::Rejected(ClaimRejection::UnknownTask));
        };
        let status = TaskStatus::from_str(&status);
        if status != TaskStatus::Queued {
            return Ok(ClaimOutcome::Rejected(ClaimRejection::NotQueued(status)));
        }
        // El glob reclamado debe ser coherente con el de la tarea (evita que un
        // agente reclame una tarea con un glob que no le corresponde).
        if path_glob != task_glob {
            tx.rollback()?;
            anyhow::bail!(
                "path_glob {path_glob:?} no coincide con el de la tarea {task_id} ({task_glob:?})"
            );
        }

        // Solape con reclamaciones activas de OTRAS tareas.
        let mut stmt = tx.prepare(
            "SELECT task_id, agent_id, path_glob FROM claims
             WHERE released_at IS NULL AND expires_at > ?1 AND task_id != ?2",
        )?;
        let actives: Vec<(String, String, String)> = stmt
            .query_map(params![now_ms, task_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        for (other_task, other_agent, other_glob) in actives {
            if globs_overlap(path_glob, &other_glob) {
                return Ok(ClaimOutcome::Rejected(ClaimRejection::Conflict {
                    task_id: other_task,
                    agent_id: other_agent,
                    path_glob: other_glob,
                }));
            }
        }

        let epoch: i64 = tx.query_row(
            "SELECT COALESCE(MAX(epoch), 0) + 1 FROM claims WHERE task_id = ?1",
            params![task_id],
            |r| r.get(0),
        )?;
        let expires_at = now_ms + LEASE_MS;
        let token_hash = token_hash(task_id, agent_id, epoch, expires_at);
        tx.execute(
            "INSERT INTO claims (task_id, agent_id, path_glob, token_hash, epoch, expires_at, claimed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![task_id, agent_id, path_glob, token_hash, epoch, expires_at, now_ms],
        )?;
        let payload = json!({"agent_id": agent_id, "epoch": epoch, "expires_at": expires_at});
        tx.execute(
            "INSERT INTO events (ts, agent_id, task_id, kind, payload) VALUES (?1, ?2, ?3, 'claimed', ?4)",
            params![now_ms, agent_id, task_id, payload.to_string()],
        )?;
        tx.execute(
            "UPDATE tasks SET status = 'claimed' WHERE id = ?1",
            params![task_id],
        )?;
        tx.commit()?;
        Ok(ClaimOutcome::Claimed(Claim {
            task_id: task_id.to_string(),
            agent_id: agent_id.to_string(),
            epoch,
            token_hash,
            expires_at,
        }))
    }

    /// Latido: extiende el arrendamiento del reclamo vivo de `(agente, tarea)`.
    pub fn heartbeat(&self, agent_id: &str, task_id: &str, now_ms: i64) -> Result<bool> {
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let expires_at = now_ms + LEASE_MS;
        let changed = tx.execute(
            "UPDATE claims SET expires_at = ?3
             WHERE agent_id = ?1 AND task_id = ?2 AND released_at IS NULL",
            params![agent_id, task_id, expires_at],
        )?;
        if changed == 0 {
            return Ok(false);
        }
        tx.execute(
            "INSERT INTO agents (id, state, last_heartbeat) VALUES (?1, 'active', ?2)
             ON CONFLICT(id) DO UPDATE SET state = 'active', last_heartbeat = excluded.last_heartbeat",
            params![agent_id, now_ms],
        )?;
        tx.execute(
            "INSERT INTO events (ts, agent_id, task_id, kind, payload) VALUES (?1, ?2, ?3, 'heartbeat', '{}')",
            params![now_ms, agent_id, task_id],
        )?;
        tx.commit()?;
        Ok(true)
    }

    /// Libera el reclamo vivo de `(agente, tarea)`: la tarea vuelve a `queued`
    /// (o `review` si tiene cambios sucios) en la misma transaccion.
    pub fn release(&self, agent_id: &str, task_id: &str, now_ms: i64) -> Result<bool> {
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let dirty: Option<String> = tx
            .query_row(
                "SELECT dirty_paths FROM tasks WHERE id = ?1",
                params![task_id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(dirty) = dirty else {
            return Ok(false);
        };
        let changed = tx.execute(
            "UPDATE claims SET released_at = ?3
             WHERE agent_id = ?1 AND task_id = ?2 AND released_at IS NULL",
            params![agent_id, task_id, now_ms],
        )?;
        if changed == 0 {
            return Ok(false);
        }
        let next = if dirty.is_empty() { "queued" } else { "review" };
        tx.execute(
            "UPDATE tasks SET status = ?2 WHERE id = ?1",
            params![task_id, next],
        )?;
        tx.execute(
            "INSERT INTO events (ts, agent_id, task_id, kind, payload) VALUES (?1, ?2, ?3, 'released', ?4)",
            params![now_ms, agent_id, task_id, json!({"dirty": !dirty.is_empty()}).to_string()],
        )?;
        tx.commit()?;
        Ok(true)
    }

    /// Caduca todas las reclamaciones vencidas: `review` si hay cambios sucios,
    /// `queued` si no. Devuelve cuantas caducaron.
    pub fn expire_leases(&self, now_ms: i64) -> Result<usize> {
        self.expire_leases_where(now_ms, None)
    }

    /// Caduca solo las reclamaciones de un agente (deteccion de agente muerto).
    pub fn expire_agent_leases(&self, agent_id: &str, now_ms: i64) -> Result<usize> {
        self.expire_leases_where(now_ms, Some(agent_id))
    }

    fn expire_leases_where(&self, now_ms: i64, agent_id: Option<&str>) -> Result<usize> {
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let rows: Vec<(String, String)> = {
            let sql = if agent_id.is_some() {
                "SELECT task_id, agent_id FROM claims
                 WHERE released_at IS NULL AND expires_at <= ?1 AND agent_id = ?2"
            } else {
                "SELECT task_id, agent_id FROM claims
                 WHERE released_at IS NULL AND expires_at <= ?1"
            };
            let mut stmt = tx.prepare(sql)?;
            let mut rows = if let Some(agent) = agent_id {
                stmt.query_map(params![now_ms, agent], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<rusqlite::Result<Vec<_>>>()?
            } else {
                stmt.query_map(params![now_ms], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<rusqlite::Result<Vec<_>>>()?
            };
            rows.sort();
            rows
        };
        for (task_id, owner) in &rows {
            let dirty: String = tx
                .query_row(
                    "SELECT dirty_paths FROM tasks WHERE id = ?1",
                    params![task_id],
                    |r| r.get(0),
                )
                .optional()?
                .unwrap_or_default();
            tx.execute(
                "UPDATE claims SET released_at = ?3
                 WHERE task_id = ?1 AND agent_id = ?2 AND released_at IS NULL",
                params![task_id, owner, now_ms],
            )?;
            let next = if dirty.is_empty() { "queued" } else { "review" };
            tx.execute(
                "UPDATE tasks SET status = ?2 WHERE id = ?1",
                params![task_id, next],
            )?;
            tx.execute(
                "INSERT INTO events (ts, agent_id, task_id, kind, payload) VALUES (?1, ?2, ?3, 'expired', ?4)",
                params![now_ms, owner, task_id, json!({"dirty": !dirty.is_empty()}).to_string()],
            )?;
            let kind = if dirty.is_empty() { "requeued" } else { "review" };
            tx.execute(
                "INSERT INTO events (ts, agent_id, task_id, kind, payload) VALUES (?1, 'broker', ?2, ?3, '{}')",
                params![now_ms, task_id, kind],
            )?;
        }
        tx.commit()?;
        Ok(rows.len())
    }

    /// Reconstruye `tasks` desde `events` (fuente de verdad). Devuelve filas.
    pub fn rebuild_tasks_from_events(&self) -> Result<usize> {
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let events: Vec<(String, String, String, String)> = {
            let mut stmt =
                tx.prepare("SELECT task_id, kind, payload, agent_id FROM events ORDER BY seq ASC")?;
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        tx.execute("DELETE FROM tasks", [])?;
        for (task_id, kind, payload, _agent) in &events {
            let payload: Value = serde_json::from_str(payload).unwrap_or(Value::Null);
            match kind.as_str() {
                "created" => {
                    tx.execute(
                        "INSERT INTO tasks (id, title, paths_glob, priority, base_commit, acceptance_cmd, status)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'queued')
                         ON CONFLICT(id) DO NOTHING",
                        params![
                            task_id,
                            payload.get("title").and_then(Value::as_str).unwrap_or(""),
                            payload.get("paths_glob").and_then(Value::as_str).unwrap_or(""),
                            payload.get("priority").and_then(Value::as_i64).unwrap_or(0),
                            payload.get("base_commit").and_then(Value::as_str).unwrap_or(""),
                            payload.get("acceptance_cmd").and_then(Value::as_str).unwrap_or(""),
                        ],
                    )?;
                }
                "claimed" => {
                    set_status(&tx, task_id, "claimed")?;
                }
                "released" | "expired" => {
                    let dirty = payload
                        .get("dirty")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    set_status(&tx, task_id, if dirty { "review" } else { "queued" })?;
                }
                "review" => set_status(&tx, task_id, "review")?,
                "merged" => set_status(&tx, task_id, "merged")?,
                "failed" => set_status(&tx, task_id, "failed")?,
                "requeued" => set_status(&tx, task_id, "queued")?,
                _ => {}
            }
        }
        let count: i64 = tx.query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))?;
        tx.commit()?;
        Ok(count as usize)
    }

    /// Reclamaciones activas (inspeccion/tests).
    pub fn active_claims(&self) -> Result<Vec<ActiveClaim>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT task_id, agent_id, path_glob, expires_at FROM claims WHERE released_at IS NULL",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(ActiveClaim {
                    task_id: r.get(0)?,
                    agent_id: r.get(1)?,
                    path_glob: r.get(2)?,
                    expires_at: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn event_count(&self) -> Result<i64> {
        let conn = self.conn()?;
        Ok(conn.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))?)
    }

    pub fn events_of_kind(&self, kind: &str) -> Result<Vec<(String, String)>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT task_id, payload FROM events WHERE kind = ?1 ORDER BY seq ASC",
        )?;
        let rows = stmt
            .query_map(params![kind], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}

fn set_status(conn: &Connection, task_id: &str, status: &str) -> Result<()> {
    conn.execute(
        "UPDATE tasks SET status = ?2 WHERE id = ?1",
        params![task_id, status],
    )?;
    Ok(())
}

/// Hash opaco del testigo de un reclamo (nunca se guarda el testigo en claro).
pub fn token_hash(task_id: &str, agent_id: &str, epoch: i64, expires_at: i64) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(task_id.as_bytes());
    hasher.update(b"\x1f");
    hasher.update(agent_id.as_bytes());
    hasher.update(b"\x1f");
    hasher.update(epoch.to_string().as_bytes());
    hasher.update(b"\x1f");
    hasher.update(expires_at.to_string().as_bytes());
    format!("{:x}", hasher.finalize())
}

/// ¿Se solapan dos globs de ruta? Regla **conservadora** (puede decir que si
/// cuando no hay solape real, nunca al reves): compara segmento a segmento;
/// `**` y `*` casan con cualquier sufijo/segmento. Asi el reclamo atomico nunca
/// permite dos escrituras potencialmente solapadas.
pub fn globs_overlap(a: &str, b: &str) -> bool {
    let a: Vec<&str> = a
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    let b: Vec<&str> = b
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    overlap_segments(&a, &b)
}

fn overlap_segments(a: &[&str], b: &[&str]) -> bool {
    if a.contains(&"**") || b.contains(&"**") {
        // `**` casa con cualquier sufijo: si los prefijos literales son
        // compatibles hasta el `**`, se solapan.
        return prefix_compatible(a, b) && prefix_compatible(b, a);
    }
    match (a.first(), b.first()) {
        (None, None) => true,
        (Some(x), Some(y)) => {
            if segments_intersect(x, y) {
                overlap_segments(&a[1..], &b[1..])
            } else {
                false
            }
        }
        // Un patron termino antes que el otro: sin solape (conservador).
        (None, Some(_)) | (Some(_), None) => false,
    }
}

/// Hasta el primer `**`, los segmentos deben casar (o ser `*`).
fn prefix_compatible(a: &[&str], b: &[&str]) -> bool {
    let mut i = 0;
    while i < a.len() && i < b.len() {
        if a[i] == "**" || b[i] == "**" {
            return true;
        }
        if !segments_intersect(a[i], b[i]) {
            return false;
        }
        i += 1;
    }
    true
}

fn segments_intersect(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    if a.contains('*') {
        return wildcard_match(a, b) || wildcard_match(b, a);
    }
    if b.contains('*') {
        return wildcard_match(b, a);
    }
    false
}

/// Match de `*` dentro de un nombre (sin `**`).
fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    fn go(p: &[char], t: &[char]) -> bool {
        match p.first() {
            None => t.is_empty(),
            Some('*') => {
                go(&p[1..], t)
                    || (!t.is_empty() && go(p, &t[1..]))
            }
            Some(c) => !t.is_empty() && *c == t[0] && go(&p[1..], &t[1..]),
        }
    }
    go(&p, &t)
}

#[cfg(test)]
mod tests;
