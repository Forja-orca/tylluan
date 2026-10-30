//! T-DSH-17 (vía 2) — el juez colgado NO bloquea las escrituras de memoria.
//!
//! T-DSH-148: la premisa original (`TYLLUAN_KERNEL_URL` apuntando al juez HTTP
//! legacy) quedó obsoleta tras T-DSH-115/116: `write_gate` ya no llama al juez
//! HTTP del kernel, sino a `grammar_judge::judge_grammar()`, que resuelve el
//! punto `[inference.points.grammar_judge]`. Este test conserva la garantía real
//! (un juez colgado no bloquea la escritura) apuntando ese punto a un servidor
//! OpenAI-compatible falso que **acepta la conexión y nunca responde**:
//!   - `POST /api/v1/memory/write` responde en milisegundos (capa 2 =
//!     fire-and-forget: `write_gate::spawn_write_gate_judge`), y
//!   - el juez SÍ llegó a llamar al punto: el servidor falso contabiliza al
//!     menos una conexión aceptada que queda colgada (la rapidez no es "porque
//!     no llamó al juez").
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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tower::ServiceExt;
use tylluan_kernel::config::TylluanConfig;
use tylluan_kernel::security::write_gate::judge_spawn_count;

/// Servidor OpenAI-compatible falso: acepta conexiones y no responde jamás.
/// Devuelve la base HTTP y un contador de conexiones aceptadas.
async fn spawn_hanging_inference_point() -> (String, Arc<AtomicUsize>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let accepted_for_task = accepted.clone();
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((sock, _)) => {
                    accepted_for_task.fetch_add(1, Ordering::SeqCst);
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
    (format!("http://{addr}"), accepted)
}

/// Escribe un `tylluan.toml` mínimo que apunta el punto `grammar_judge` al
/// servidor falso colgado y fija ese fichero como config del proceso.
fn point_grammar_judge_to(base: &str) {
    let dir = std::env::temp_dir().join(format!(
        "tylluan-t148-grammar-judge-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("tylluan.toml");
    let body = format!(
        "[inference.points.grammar_judge]\n\
         provider = \"openai_compatible\"\n\
         endpoint = \"{base}\"\n\
         model = \"fake-hung-judge\"\n"
    );
    std::fs::write(&cfg, body).unwrap();
    // Único #[test] de este binario: fijar el override antes de que ningún
    // camino llame a TylluanConfig::load_cached() es seguro.
    TylluanConfig::set_config_path_override(cfg);
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
    let (base, accepted) = spawn_hanging_inference_point().await;
    point_grammar_judge_to(&base);

    let state = support::test_state("t17a").await;
    let app = support::build_test_app(state);

    let before_spawns = judge_spawn_count();

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

    // La rapidez no es por omisión: el juez se lanzó (spawns) y llegó a llamar
    // al punto configurado, que acepta la conexión y no responde.
    assert!(
        judge_spawn_count() >= before_spawns + 2,
        "las 2 escrituras deben lanzar el juez de capa 2 (spawns: {} -> {})",
        before_spawns,
        judge_spawn_count()
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    while accepted.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(
        accepted.load(Ordering::SeqCst) >= 1,
        "el juez debe haber abierto al menos una conexión al punto colgado \
         (conexiones aceptadas: {})",
        accepted.load(Ordering::SeqCst)
    );
}
