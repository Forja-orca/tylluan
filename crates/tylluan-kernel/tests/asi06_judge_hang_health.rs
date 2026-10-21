//! T-DSH-17 (vías 2b y 3) — el timeout real del juez (90 s) libera el semáforo
//! y queda contado como FALLO en `/health?verbose=true components.judge`.
//!
//! Escenario idéntico a `asi06_judge_hang_writes.rs` (backend que nunca
//! responde), pero aquí se espera el `JUDGE_TIMEOUT` real (90 s) para observar:
//!   1. el juicio termina solo (no se queda colgado para siempre),
//!   2. el permiso del semáforo (1 ranura) y el contador `in_flight` se liberan
//!      — por eso una escritura posterior vuelve a entrar en el juez,
//!   3. `/health?verbose=true components.judge` lo cuenta como fallo
//!      (`failures` / `by_kind.timeout`), con `timeout_ms = 90000`.
//!
//! LENTO por diseño (~95 s reales): se queda `#[ignore]` para no meterlo en la
//! suite normal; es la demostración que se corre a mano con `--ignored`.
//!
//! Run:
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test asi06_judge_hang_health -- --ignored

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use std::time::Duration;
use tower::ServiceExt;
use tylluan_kernel::security::judge_telemetry::judge_telemetry;
use tylluan_kernel::security::write_gate::judge_spawn_count;

async fn spawn_hanging_judge() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((sock, _)) => {
                    tokio::spawn(async move {
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

async fn post_memory_write(app: &axum::Router, content: &str) -> StatusCode {
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
    let _ = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await;
    status
}

async fn get_health_verbose(app: &axum::Router) -> serde_json::Value {
    let req = Request::builder()
        .method("GET")
        .uri("/health?verbose=true")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "health debe responder 200");
    let bytes = axum::body::to_bytes(resp.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).expect("health?verbose=true debe ser JSON")
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "lento por diseno (~95 s): espera el JUDGE_TIMEOUT real de 90 s; correr con --ignored"]
async fn hung_judge_times_out_frees_the_slot_and_is_counted_in_health() {
    let base = spawn_hanging_judge().await;
    // SAFETY: único #[test] de este binario.
    unsafe { std::env::set_var("TYLLUAN_KERNEL_URL", &base) };

    let state = support::test_state("t17b").await;
    // Router de producción (no `build_test_app`): este test necesita `/health`,
    // que el harness de tests no monta.
    let app = tylluan_kernel::transport::http::build_router(state);

    let failures_before = judge_telemetry().failures();

    assert_eq!(
        post_memory_write(&app, "nota 1 que se queda colgada en el juez").await,
        StatusCode::OK
    );
    // El juez está colgado y aun así la escritura ya volvió.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        judge_telemetry().in_flight() >= 1,
        "el juez debe estar in_flight mientras el backend no responde"
    );

    // Espera al timeout real (90 s) + margen para que se registre la telemetría.
    tokio::time::sleep(Duration::from_secs(92)).await;

    // (2) El permiso/contador se liberó: in_flight vuelve a 0.
    assert_eq!(
        judge_telemetry().in_flight(),
        0,
        "tras JUDGE_TIMEOUT el contador in_flight debe volver a 0 (permiso liberado)"
    );

    // (3) /health?verbose=true lo cuenta como fallo del juez, con su tipo.
    let health = get_health_verbose(&app).await;
    let judge = &health["components"]["judge"];
    assert!(
        judge.is_object(),
        "components.judge debe existir en /health?verbose=true: {health}"
    );
    let failures = judge["failures"].as_u64().unwrap_or(0);
    assert!(
        failures > failures_before,
        "el timeout debe sumar a failures (antes {failures_before}, ahora {failures}): {judge}"
    );
    assert!(
        judge["by_kind"]["timeout"].as_u64().unwrap_or(0) >= 1,
        "el timeout debe clasificarse como timeout, no como otro error: {judge}"
    );
    assert_eq!(
        judge["timeout_ms"].as_u64(),
        Some(90_000),
        "el health debe publicar el JUDGE_TIMEOUT real: {judge}"
    );
    assert_eq!(
        judge["max_concurrency"].as_u64(),
        Some(1),
        "una sola ranura de juguete (llama-server sin --parallel): {judge}"
    );

    // (2b) El slot está libre de verdad: otra escritura vuelve a entrar al juez.
    let spawns_before = judge_spawn_count();
    assert_eq!(
        post_memory_write(&app, "nota 2 tras el timeout").await,
        StatusCode::OK
    );
    assert!(
        judge_spawn_count() > spawns_before,
        "la escritura posterior debe lanzar un juez nuevo"
    );
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        judge_telemetry().in_flight() >= 1,
        "si el permiso se hubiera quedado tomado, el segundo juez no entraría \
         (in_flight seguiría 0 y/o subiría busy_skip)"
    );
}
