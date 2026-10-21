//! T-DSH-17 (vía 2) — el juez colgado NO bloquea las escrituras de memoria.
//!
//! Monta un `llama_backend` de mentira que acepta la conexión y **nunca
//! responde** (el peor caso de una llamada colgada), apunta el juez a él por
//! `TYLLUAN_KERNEL_URL` y comprueba contra el router real que:
//!   - `POST /api/v1/memory/write` responde en milisegundos (capa 2 =
//!     fire-and-forget: `write_gate::spawn_write_gate_judge`), y
//!   - el juez SÍ está colgado (telemetría `in_flight >= 1`), así que la
//!     rapidez no es "porque no llamó al juez".
//!
//! La espera del timeout real (JUDGE_TIMEOUT = 90 s) y su reflejo en
//! `/health?verbose=true` viven en `asi06_judge_hang_health.rs` (lento, bajo
//! `--ignored` por coste).
//!
//! Run:
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test asi06_judge_hang_writes

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use std::time::Duration;
use tower::ServiceExt;
use tylluan_kernel::security::judge_telemetry::judge_telemetry;
use tylluan_kernel::security::write_gate::judge_spawn_count;

/// Servidor TCP que acepta conexiones y no responde jamás.
async fn spawn_hanging_judge() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((sock, _)) => {
                    tokio::spawn(async move {
                        // Mantiene la conexión abierta sin enviar respuesta.
                        let _hold = sock;
                        tokio::time::sleep(Duration::from_secs(3600)).await;
                    });
                }
                Err(_) => return,
            }
        }
    });
    format!("http://{addr}")
}

async fn post_memory_write(
    app: &axum::Router,
    content: &str,
) -> (StatusCode, String, Duration) {
    let t0 = std::time::Instant::now();
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/memory/write")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({ "content": content }).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    (
        status,
        String::from_utf8_lossy(&bytes).to_string(),
        t0.elapsed(),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn hung_judge_does_not_block_memory_writes() {
    let base = spawn_hanging_judge().await;
    // SAFETY: único #[test] de este binario; el env se fija antes de cualquier
    // llamada al juez y este binario no comparte proceso con otros tests.
    unsafe { std::env::set_var("TYLLUAN_KERNEL_URL", &base) };

    let state = support::test_state("t17a").await;
    let app = support::build_test_app(state);

    let before_spawns = judge_spawn_count();
    let before_in_flight = judge_telemetry().in_flight();

    for i in 0..2 {
        let (status, body, elapsed) =
            post_memory_write(&app, &format!("nota benigna {i} de la flota")).await;
        assert_eq!(status, StatusCode::OK, "write {i}: {body}");
        assert!(
            elapsed < Duration::from_secs(3),
            "write {i} tardó {elapsed:?}: la capa 2 NO es fire-and-forget \
             (un juez colgado no puede bloquear la escritura)"
        );
    }

    // La rapidez no es por omisión: el juez se lanzó y está colgado esperando.
    assert!(
        judge_spawn_count() >= before_spawns + 2,
        "las 2 escrituras deben lanzar el juez de capa 2 (spawns: {} -> {})",
        before_spawns,
        judge_spawn_count()
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        judge_telemetry().in_flight() > before_in_flight,
        "con el backend colgado el juez debe estar in_flight (la escritura \
         volvió igualmente: fire-and-forget confirmado)"
    );
}
