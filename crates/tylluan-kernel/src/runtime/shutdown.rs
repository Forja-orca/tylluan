//! T-DSH-130 — apagado ordenado con deadline de join (ADR-025 R4/R6).
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

    /// Espera a las tareas registradas hasta el deadline. Devuelve los NOMBRES
    /// de las que no respondieron (sin abortarlas, R6). Las ya terminadas se
    /// consumen sin coste.
    pub async fn wait_for(&self, tasks: Vec<(String, JoinHandle<()>)>) -> Vec<String> {
        let mut pending: Vec<String> = Vec::new();
        let mut iter = tasks.into_iter();
        loop {
            let Some((name, handle)) = iter.next() else {
                break;
            };
            match tokio::time::timeout_at(self.deadline, handle).await {
                Ok(_) => {}
                Err(_) => {
                    pending.push(name);
                    pending.extend(iter.map(|(n, _)| n));
                    break;
                }
            }
        }
        pending
    }
}

/// Corte duro opcional tras la secuencia ordenada.
///
/// - `force_exit == false`: no hace nada (se vuelve al retorno normal de main).
/// - `force_exit == true`: loguea `shutdown_force_exit` (con las tareas
///   pendientes, si se conocen) y sale con `exit_code`.
///
/// Nota: hoy `main` no tiene handles de tareas de fondo (los `tokio::spawn`
/// no se guardan), asi que `pending` puede ir vacio; aun asi el retardo del
/// desmontaje del runtime existe y es lo que este corte elimina. La lista de
/// pendientes se rellenara cuando exista el `TaskRegistry` (ADR-025 R2).
pub fn force_exit_after_shutdown(pending: &[String], exit_code: i32, force_exit: bool) {
    if !force_exit {
        if !pending.is_empty() {
            tracing::warn!(
                pending = ?pending,
                "shutdown: tareas de fondo sin terminar antes del deadline (sin corte duro)"
            );
        }
        return;
    }
    tracing::warn!(
        pending = ?pending,
        exit_code,
        "shutdown_force_exit: la secuencia ordenada termino; se corta el proceso para no esperar al desmontaje del runtime"
    );
    // Flush best-effort de tracing antes de salir (process::exit no corre Drop).
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
        let pending = deadline.wait_for(vec![("done".to_string(), handle)]).await;
        assert!(pending.is_empty(), "una tarea terminada no debe quedar pendiente");
    }

    #[tokio::test]
    async fn slow_task_is_reported_pending_without_abort() {
        let deadline = ShutdownDeadline::start(50);
        let handle = tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(30)).await;
        });
        let pending = deadline.wait_for(vec![("slow".to_string(), handle)]).await;
        assert_eq!(pending, vec!["slow".to_string()]);
    }

    #[tokio::test]
    async fn timeout_zero_does_not_expire() {
        let deadline = ShutdownDeadline::start(0);
        let handle = tokio::spawn(async {});
        let pending = deadline.wait_for(vec![("done".to_string(), handle)]).await;
        assert!(pending.is_empty());
    }

    #[test]
    fn force_exit_false_returns_without_exit() {
        // Solo comprueba que la rama no sale del proceso; el corte real no es
        // testeable dentro del mismo proceso (se mide con test-run).
        force_exit_after_shutdown(&["tarea".to_string()], 0, false);
    }
}
