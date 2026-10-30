//! T-DSH-140 (ADR-025 R2): `TaskRegistry` mínimo y vista REST de solo lectura.
//!
//! Registro en proceso de las tareas de fondo del kernel. No sustituye al
//! supervisor de guilds (`registry/supervisor.rs`): solo lleva nombre,
//! subruntime, estado, última ejecución y contadores saneados. La primera
//! tarea migrada es el reindexer (`main.rs`, bucle "Agnostic RE-INDEXING").
//!
//! No hay herramienta MCP nueva (CONTRACT-01: seis tools): la superficie es
//! solo `GET /api/v1/runtime/tasks`, clase operador, solo lectura.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Instantánea saneada de una tarea (nunca rutas, agentes ni contenido).
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct TaskSnapshot {
    pub name: &'static str,
    pub subruntime: &'static str,
    pub state: &'static str,
    pub last_run_unix: Option<u64>,
    pub runs: u64,
    pub errors: u64,
}

struct TaskEntry {
    subruntime: &'static str,
    state: &'static str,
    last_run_unix: Option<u64>,
    runs: u64,
    errors: u64,
}

/// Registro de tareas de fondo del kernel.
pub struct TaskRegistry {
    tasks: RwLock<HashMap<&'static str, TaskEntry>>,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl TaskRegistry {
    pub fn new() -> Self {
        Self {
            tasks: RwLock::new(HashMap::new()),
        }
    }

    /// Registra una tarea en estado `idle`. Idempotente (no duplica).
    pub fn register(&self, name: &'static str, subruntime: &'static str) {
        let mut guard = match self.tasks.write() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.entry(name).or_insert(TaskEntry {
            subruntime,
            state: "idle",
            last_run_unix: None,
            runs: 0,
            errors: 0,
        });
    }

    /// Marca el inicio de una ejecución: `running`, incrementa `runs` y fija
    /// `last_run_unix`.
    pub fn mark_running(&self, name: &'static str) {
        let mut guard = match self.tasks.write() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if let Some(t) = guard.get_mut(name) {
            t.state = "running";
            t.runs += 1;
            t.last_run_unix = Some(now_unix());
        }
    }

    /// Marca el final correcto de una ejecución: `idle`.
    pub fn mark_idle(&self, name: &'static str) {
        let mut guard = match self.tasks.write() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if let Some(t) = guard.get_mut(name) {
            t.state = "idle";
        }
    }

    /// Marca un fallo: incrementa `errors` y vuelve a `idle`.
    pub fn mark_error(&self, name: &'static str) {
        let mut guard = match self.tasks.write() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if let Some(t) = guard.get_mut(name) {
            t.errors += 1;
            t.state = "idle";
        }
    }

    /// Instantánea ordenada por nombre para la vista REST.
    pub fn snapshot(&self) -> Vec<TaskSnapshot> {
        let guard = match self.tasks.read() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let mut out: Vec<TaskSnapshot> = guard
            .iter()
            .map(|(name, t)| TaskSnapshot {
                name,
                subruntime: t.subruntime,
                state: t.state,
                last_run_unix: t.last_run_unix,
                runs: t.runs,
                errors: t.errors,
            })
            .collect();
        out.sort_by_key(|t| t.name);
        out
    }
}

impl Default for TaskRegistry {
    fn default() -> Self {
        Self::new()
    }
}

static REGISTRY: OnceLock<TaskRegistry> = OnceLock::new();

/// Registro global del proceso (inicializado en el primer uso).
pub fn global() -> &'static TaskRegistry {
    REGISTRY.get_or_init(TaskRegistry::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_is_idempotent_and_snapshot_is_sorted() {
        let reg = TaskRegistry::new();
        reg.register("reindexer", "Memoria");
        reg.register("reindexer", "Memoria");
        reg.register("alpha", "Red");
        let snap = reg.snapshot();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].name, "alpha");
        assert_eq!(snap[1].name, "reindexer");
        assert_eq!(snap[1].subruntime, "Memoria");
        assert_eq!(snap[1].state, "idle");
        assert_eq!(snap[1].runs, 0);
    }

    #[test]
    fn running_idle_and_error_counters() {
        let reg = TaskRegistry::new();
        reg.register("reindexer", "Memoria");
        reg.mark_running("reindexer");
        let s = reg.snapshot();
        assert_eq!(s[0].state, "running");
        assert_eq!(s[0].runs, 1);
        assert!(s[0].last_run_unix.is_some());
        reg.mark_idle("reindexer");
        assert_eq!(reg.snapshot()[0].state, "idle");
        reg.mark_error("reindexer");
        let s = reg.snapshot();
        assert_eq!(s[0].errors, 1);
        assert_eq!(s[0].state, "idle");
    }

    #[test]
    fn snapshot_has_no_paths_or_content_fields() {
        let reg = TaskRegistry::new();
        reg.register("reindexer", "Memoria");
        let json = serde_json::to_string(&reg.snapshot()).expect("json");
        for forbidden in ["path", "content", "agent_id", "\\"] {
            assert!(!json.contains(forbidden), "campo prohibido en snapshot: {forbidden}");
        }
    }
}
