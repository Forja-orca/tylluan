//! D-17 — los cuatro caminos de escritura pasan por la capa 1 (ASI06) y por el
//! juez. B-9 midió (`docs/internal/IMPL_b9_h5_ingest_memory_write_buffy_2026-10.md`)
//! que dos rutas REST escribían sin ninguna de las dos capas:
//!
//!   - `POST /api/v1/memory/write` (escritura de AGENTE, misma severidad que
//!     `tylluan_remember`): aceptaba 8/8 muestras de inyección sin marca y sin juez.
//!   - `POST /api/v1/ingest` (guardar DOCUMENTOS): aceptaba 8/8 sin marca y sin juez.
//!
//! Política tras D-17: `memory/write` RECHAZA (403 `ACCESS_DENIED`) porque su
//! contrato es el de un agente recordando algo; `ingest` GUARDA pero MARCA
//! (`untrusted_content: true` en metadata) porque un documento legítimo puede
//! hablar de inyección (paper, cheat-sheet, este mismo informe) y rechazarlo
//! sería un falso positivo. Ambas lanzan el juez (capa 2) al escribir.
//!
//! Un único `#[tokio::test]` a propósito: `judge_spawn_count()` es un contador
//! global del proceso y `TYLLUAN_KERNEL_URL` es una variable de entorno; con
//! varios tests en paralelo las dos medidas dejarían de significar nada.
//!
//! El juez se apunta a un puerto muerto (`127.0.0.1:9`) para que nunca hable con
//! el kernel vivo de la máquina: aquí se verifica QUE SE LANZA (contador), no
//! el veredicto del LLM (eso lo cubre la telemetría del juez en vivo).

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use serde_json::Value;
use tower::ServiceExt;

use tylluan_kernel::security::poison_patterns::matches_injection_pattern;
use tylluan_kernel::security::write_gate::judge_spawn_count;
use tylluan_kernel::transport::http::{build_router, HttpState};

const MASTER: &str = "master-test-token";

const INJECTION_CORPUS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../benchmarks/injection_corpus_v1.json"
);
const BENIGN_CORPUS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../benchmarks/benign_corpus_v1.json"
);

/// Ids que B-9 midió aceptados sin marca por ambas rutas. Si el corpus cambia,
/// este test avisa: la lista de capa 1 detectada es parte del contrato medido.
const B9_FLAGGED: [&str; 8] = [
    "inj_01", "inj_02", "inj_07", "inj_12", "inj_19", "inj_22", "inj_35", "inj_36",
];

fn samples(path: &str) -> Vec<(String, String)> {
    let raw = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let doc: Value = serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse {path}: {e}"));
    doc.get("muestras")
        .and_then(|m| m.as_array())
        .unwrap_or_else(|| panic!("{path}: missing 'muestras'"))
        .iter()
        .map(|m| {
            (
                m.get("id").and_then(|v| v.as_str()).expect("id").to_string(),
                m.get("texto")
                    .and_then(|v| v.as_str())
                    .expect("texto")
                    .to_string(),
            )
        })
        .collect()
}

async fn post_json(
    app: &Router,
    uri: &str,
    body: Value,
) -> (StatusCode, Value) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header(header::AUTHORIZATION, format!("Bearer {MASTER}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .expect("oneshot");
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("body");
    let parsed = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, parsed)
}

async fn ingest(app: &Router, text: &str) -> String {
    let (status, body) = post_json(app, "/api/v1/ingest?tags=d17", serde_json::json!({ "text": text })).await;
    assert_eq!(status, StatusCode::OK, "ingest should accept and mark: {body}");
    body.get("node_id")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("ingest response without node_id: {body}"))
        .to_string()
}

/// Cuenta nodos que contienen exactamente `text` y NO vienen del pipeline de
/// ingesta: si es 0, `memory/write` de verdad no escribió nada.
async fn non_ingest_nodes_with(state: &Arc<HttpState>, text: &str) -> i64 {
    let guard = state.silva.conn_lock();
    let conn = guard.lock().await;
    conn.query_row(
        "SELECT COUNT(*) FROM nodes
         WHERE content = ?1
           AND COALESCE(json_extract(metadata, '$.source'), '') <> 'ingest_pipeline'",
        rusqlite::params![text],
        |row| row.get(0),
    )
    .expect("count nodes")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn d17_layer1_and_judge_cover_memory_write_and_ingest() {
    // SAFETY (set_var): edition-2024 unsafe op; este binario de test contiene un
    // solo test, así que no hay otro hilo leyendo el entorno mientras se fija
    // antes de la primera escritura.
    unsafe { std::env::set_var("TYLLUAN_KERNEL_URL", "http://127.0.0.1:9") };

    let state = support::test_state_custom("d17l1", Some(MASTER.to_string()), false).await;
    let app = build_router(state.clone());

    let injection = samples(INJECTION_CORPUS);
    let benign = samples(BENIGN_CORPUS);

    // (1) La capa 1 detecta exactamente el conjunto que midió B-9.
    let mut flagged: Vec<String> = injection
        .iter()
        .filter(|(_, text)| matches_injection_pattern(text))
        .map(|(id, _)| id.clone())
        .collect();
    flagged.sort();
    let mut expected: Vec<String> = B9_FLAGGED.iter().map(|s| s.to_string()).collect();
    expected.sort();
    assert_eq!(flagged, expected, "Layer 1 detection drifted from the B-9 measurement");

    let spawns_before = judge_spawn_count();

    // (2) memory/write rechaza las 8 ANTES de escribir y no lanza juez.
    for (id, text) in injection.iter().filter(|(id, _)| B9_FLAGGED.contains(&id.as_str())) {
        let (status, body) =
            post_json(&app, "/api/v1/memory/write", serde_json::json!({ "content": text, "agent_id": "d17" })).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{id}: memory/write must reject, got {body}");
        assert_eq!(
            body.get("error").and_then(|v| v.as_str()),
            Some("ACCESS_DENIED"),
            "{id}: {body}"
        );
        assert_eq!(
            non_ingest_nodes_with(&state, text).await,
            0,
            "{id}: rejected content must not land in SilvaDB"
        );
    }
    assert_eq!(
        judge_spawn_count(),
        spawns_before,
        "a rejected write must not spend a judge call"
    );

    // (3) ingest guarda las 8 MARCADAS y lanza un juicio por escritura.
    for (id, text) in injection.iter().filter(|(id, _)| B9_FLAGGED.contains(&id.as_str())) {
        let node_id = ingest(&app, text).await;
        let node = state
            .silva
            .get_node(&node_id)
            .await
            .expect("get_node")
            .unwrap_or_else(|| panic!("{id}: ingest node {node_id} not stored"));
        let meta: Value = serde_json::from_str(&node.metadata).expect("metadata JSON");
        assert_eq!(
            meta.get("untrusted_content").and_then(|v| v.as_bool()),
            Some(true),
            "{id}: ingest must mark untrusted_content (B-9 stored it clean)"
        );
        assert!(
            meta.get("untrusted_reason")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .contains("Layer 1"),
            "{id}: reason should cite Layer 1, got {:?}",
            meta.get("untrusted_reason")
        );
    }
    assert_eq!(
        judge_spawn_count(),
        spawns_before + 8,
        "ingest must launch the judge once per stored document"
    );

    // (4) Benigno: pasa las dos rutas sin rechazo ni marca (0 falsos positivos)
    // y cada escritura lanza su juicio.
    for (id, text) in &benign {
        assert!(
            !matches_injection_pattern(text),
            "{id}: benign corpus sample must not trip Layer 1"
        );
        let (status, body) =
            post_json(&app, "/api/v1/memory/write", serde_json::json!({ "content": text, "agent_id": "d17" })).await;
        assert_eq!(status, StatusCode::OK, "{id}: benign memory/write, got {body}");

        let node_id = ingest(&app, text).await;
        let node = state.silva.get_node(&node_id).await.expect("get_node").unwrap();
        let meta: Value = serde_json::from_str(&node.metadata).expect("metadata JSON");
        assert!(
            meta.get("untrusted_content").is_none(),
            "{id}: benign document must not be marked untrusted"
        );
    }
    assert_eq!(
        judge_spawn_count(),
        spawns_before + 8 + 2 * benign.len() as u64,
        "every accepted write on both routes must reach the judge"
    );
}
