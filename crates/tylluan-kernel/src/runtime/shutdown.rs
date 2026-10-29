//! T-DSH-130/132 — apagado ordenado con deadline de join (ADR-025 R4/R6).
//!
//! `ShutdownDeadline` acota la espera de las tareas de fondo registradas y,
//! si el operador lo activa (`[runtime] shutdown_force_exit = true`), corta el
//! proceso con `std::process::exit` cuando la secuencia ordenada ya termino y
//! el runtime seguiria esperando a hilos que no puede cancelar (ONNX/SQLite).
//!
//! R6: NUNCA se aborta una tarea que pueda estar escribiendo; el deadline solo
//! deja de esperar y el corte duro ocurre despues del drenaje best-effort.

use std::time::Duration;

use tokio::task::JoinHandle;

/// Politica de espera de una tarea de fondo al apagar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitPolicy {
    /// Tarea corta o cooperativa: se espera hasta el deadline.
    Wait,
    /// Tarea larga pero acotada: se espera hasta el deadline; si no termina,
    /// se registra por nombre (sin abortar).
    Drain,
    /// Bucle sin cancelacion cooperativa (ONNX/SQLite de riesgo alto): NO se
    /// espera; se deja morir con el proceso. No se hace `abort` (R6).
    Skip,
}

/// Tarea de fondo registrada en el arranque para el apagado ordenado.
pub struct BackgroundTask {
    /// Nombre estable para trazabilidad (sin rutas ni contenido).
    pub name: &'static str,
    /// Subruntime ADR-025 al que pertenece.
    pub subruntime: &'static str,
    pub handle: JoinHandle<()>,
    pub policy: WaitPolicy,
}

impl BackgroundTask {
    pub fn new(
        name: &'static str,
        subruntime: &'static str,
        policy: WaitPolicy,
        handle: JoinHandle<()>,
    ) -> Self {
        Self {
            name,
            subruntime,
            handle,
            policy,
        }
    }

    pub fn wait(name: &'static str, subruntime: &'static str, handle: JoinHandle<()>) -> Self {
        Self::new(name, subruntime, WaitPolicy::Wait, handle)
    }

    pub fn drain(name: &'static str, subruntime: &'static str, handle: JoinHandle<()>) -> Self {
        Self::new(name, subruntime, WaitPolicy::Drain, handle)
    }

    pub fn skip(name: &'static str, subruntime: &'static str, handle: JoinHandle<()>) -> Self {
        Self::new(name, subruntime, WaitPolicy::Skip, handle)
    }
}

/// Deadline de apagado. Se crea al entrar en la secuencia de apagado.
pub struct ShutdownDeadline {
    deadline: tokio::time::Instant,
}

impl ShutdownDeadline {
    /// `timeout_ms == 0` => sin deadline (espera indefinida, comportamiento
    /// historico); se usa para el control A/B.
    pub fn start(timeout_ms: u64) -> Self {
        let deadline = if timeout_ms == 0 {
            tokio::time::Instant::now() + Duration::from_secs(60 * 60 * 24 * 365)
        } else {
            tokio::time::Instant::now() + Duration::from_millis(timeout_ms)
        };
        Self { deadline }
    }

    /// Espera a las tareas registradas hasta el deadline.
    ///
    /// Devuelve `(pending, skipped)`:
    /// - `pending`: nombres de `Wait`/`Drain` que no respondieron (sin abortar).
    /// - `skipped`: nombres de `Skip` que no se esperaron (mueren con el proceso).
    pub async fn wait_for(&self, tasks: Vec<BackgroundTask>) -> (Vec<String>, Vec<String>) {
        let mut pending: Vec<String> = Vec::new();
        let mut skipped: Vec<String> = Vec::new();
        for task in tasks {
            if task.policy == WaitPolicy::Skip {
                // R6: no se aborta; se dropea el handle y el proceso termina.
                skipped.push(task.name.to_string());
                continue;
            }
            match tokio::time::timeout_at(self.deadline, task.handle).await {
                Ok(_) => {}
                Err(_) => pending.push(task.name.to_string()),
            }
        }
        (pending, skipped)
    }
}

/// Corte duro opcional tras la secuencia ordenada.
///
/// - `force_exit == false`: no hace nada (se vuelve al retorno normal de main).
/// - `force_exit == true`: loguea `shutdown_force_exit` (con pendientes y
///   saltadas) y sale con `exit_code`.
pub fn force_exit_after_shutdown(
    pending: &[String],
    skipped: &[String],
    exit_code: i32,
    force_exit: bool,
) {
    if !force_exit {
        if !pending.is_empty() {
            tracing::warn!(
                pending = ?pending,
                skipped = ?skipped,
                "shutdown: tareas de fondo sin terminar antes del deadline (sin corte duro)"
            );
        }
        return;
    }
    tracing::warn!(
        pending = ?pending,
        skipped = ?skipped,
        exit_code,
        "shutdown_force_exit: la secuencia ordenada termino; se corta el proceso para no esperar al desmontaje del runtime"
    );
    tracing::info!("shutdown_force_exit: exiting({exit_code})");
    std::process::exit(exit_code);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn finished_task_is_not_pending() {
        let deadline = ShutdownDeadline::start(1_000);
        let handle = tokio::spawn(async {});
        let (pending, skipped) = deadline
            .wait_for(vec![BackgroundTask::wait("done", "Red", handle)])
            .await;
        assert!(pending.is_empty());
        assert!(skipped.is_empty());
    }

    #[tokio::test]
    async fn slow_wait_task_is_reported_pending_without_abort() {
        let deadline = ShutdownDeadline::start(50);
        let handle = tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(30)).await;
        });
        let (pending, skipped) = deadline
            .wait_for(vec![BackgroundTask::wait("slow", "Memoria", handle)])
            .await;
        assert_eq!(pending, vec!["slow".to_string()]);
        assert!(skipped.is_empty());
    }

    #[tokio::test]
    async fn skip_task_is_not_awaited() {
        let deadline = ShutdownDeadline::start(5_000);
        let started = std::time::Instant::now();
        let handle = tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(30)).await;
        });
        let (pending, skipped) = deadline
            .wait_for(vec![BackgroundTask::skip("loop", "Memoria", handle)])
            .await;
        assert!(pending.is_empty(), "Skip no debe quedar en pending");
        assert_eq!(skipped, vec!["loop".to_string()]);
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "Skip no debe consumir el deadline"
        );
    }

    #[tokio::test]
    async fn mixed_policies_are_classified() {
        let deadline = ShutdownDeadline::start(50);
        let fast = tokio::spawn(async {});
        let slow = tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(30)).await;
        });
        let skip = tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(30)).await;
        });
        let (pending, skipped_names) = deadline
            .wait_for(vec![
                BackgroundTask::wait("fast", "Red", fast),
                BackgroundTask::drain("slow", "Memoria", slow),
                BackgroundTask::skip("skip", "Memoria", skip),
            ])
            .await;
        assert_eq!(pending, vec!["slow".to_string()]);
        assert_eq!(skipped_names, vec!["skip".to_string()]);
    }

    #[tokio::test]
    async fn timeout_zero_does_not_expire() {
        let deadline = ShutdownDeadline::start(0);
        let handle = tokio::spawn(async {});
        let (pending, skipped) = deadline
            .wait_for(vec![BackgroundTask::wait("done", "Red", handle)])
            .await;
        assert!(pending.is_empty());
        assert!(skipped.is_empty());
    }

    #[test]
    fn force_exit_false_returns_without_exit() {
        // Solo comprueba que la rama no sale del proceso; el corte real se
        // mide con el montaje de test-run (T-DSH-130).
        force_exit_after_shutdown(&["tarea".to_string()], &["skip".to_string()], 0, false);
    }
}
