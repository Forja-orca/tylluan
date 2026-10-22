//! D-32 — regresion de las vias de escritura que antes no tenian defensa.
//!
//! `cola_d32_acceptance.rs` es la lista de aceptacion (veneno marcado en V3..V7,
//! ya sin `#[ignore]`). Este fichero cubre el otro lado del contrato, que la
//! aceptacion no mide:
//!
//! 1. contenido benigno por las cinco vias **no** se marca (cero friccion),
//!    incluida una asignacion de rol en espanol (la clase que D-31 descarto);
//! 2. la procedencia `metadata.source` sobrevive y se rellena con `http_api`
//!    cuando la guild no la declara;
//! 3. la opcion `security.mark_uploads_untrusted` (apagada por defecto) marca
//!    por procedencia al encenderse, y solo entonces;
//! 4. la regla de escritura no anade falsos positivos sobre los corpora
//!    benignos de `benchmarks/` (40 EN + 300 ES).
//!
//! Sin `cfg(unix)`/`cfg(windows)` en ninguna funcion.

mod support;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use std::sync::Arc;
use tower::ServiceExt;

type Silva = Arc<tylluan_kernel::memory::silva::SilvaDB>;

static CASE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// `test_state` usa `TylluanConfig::load_cached()`: la config es GLOBAL al
/// proceso del binario de test. Mutar `mark_uploads_untrusted` sin serializar
/// hacia que otro test viera la bandera encendida (fallo observado: V3 benigno
/// marcado como `upload_provenance`). Los tests que tocan la config toman este
/// cerrojo y restauran el valor.
static CONFIG_GUARD: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn next_case() -> u64 {
    CASE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Texto benigno en espanol: asignacion de rol, que es uso normal (8 de los 300
/// benignos de T-DSH-27 son asi). La capa 1 no puede marcarlo.
const BENIGNO_ES: &str = "Ahora eres la persona de guardia del despliegue semanal";
const BENIGNO_EN: &str = "The deploy ran on tuesday and the rollback plan is documented";

async fn node_metadata_by_content(silva: &Silva, needle: &str) -> Option<serde_json::Value> {
    let mut page = 0usize;
    loop {
        let batch = silva
            .get_nodes_paginated(200, page * 200)
            .await
            .unwrap_or_else(|e| panic!("get_nodes_paginated({page}) fallo: {e}"));
        let n = batch.len();
        for node in batch {
            if node.content.contains(needle) {
                return Some(
                    serde_json::from_str(&node.metadata).unwrap_or(serde_json::Value::Null),
                );
            }
        }
        if n < 200 {
            return None;
        }
        page += 1;
    }
}

fn flagged(meta: &serde_json::Value) -> bool {
    meta.get("untrusted_content")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

async fn post_json(
    app: &axum::Router,
    uri: &str,
    body: serde_json::Value,
) -> (StatusCode, String) {
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 4 << 20)
        .await
        .unwrap_or_default();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

async fn call_tool(
    app: &axum::Router,
    name: &str,
    arguments: serde_json::Value,
) -> (StatusCode, String) {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": name, "arguments": arguments }
    });
    let req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 4 << 20)
        .await
        .unwrap_or_default();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

#[tokio::test(flavor = "multi_thread")]
async fn vias_nuevas_no_marcan_contenido_benigno() {
    let _config_guard = CONFIG_GUARD.lock().await;
    let state = support::test_state("d32ben").await;
    let app = support::build_test_app(state.clone());
    let mcp = tylluan_kernel::transport::http::build_router(state.clone());
    let case = next_case();

    // V3 — silva/node (subida de ficheros / vision).
    let v3_marker = format!("d32-ben-v3-{case}");
    let v3_content = format!("{BENIGNO_ES} (nota {v3_marker})");
    let (status, body) = post_json(
        &app,
        "/api/v1/silva/node",
        serde_json::json!({
            "node_type": "document",
            "content": v3_content,
            "metadata": "{}",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "V3 benigno: {status} {body}");
    let meta = node_metadata_by_content(&state.silva, &v3_marker)
        .await
        .expect("V3 benigno: nodo no escrito");
    assert!(!flagged(&meta), "V3 benigno no debe marcarse: {meta}");

    // V4 — record_memory (Coloquio indexado).
    let v4_marker = format!("d32-ben-v4-{case}");
    let v4_content = format!("{BENIGNO_EN} (nota {v4_marker})");
    let mgr = tylluan_kernel::memory::agent_memory::AgentMemoryManager::new(state.silva.clone(), 20);
    mgr.record_memory("d32-benign", &v4_content, 1.0).await;
    let meta = node_metadata_by_content(&state.silva, &v4_marker)
        .await
        .expect("V4 benigno: nodo no escrito");
    assert!(!flagged(&meta), "V4 benigno no debe marcarse: {meta}");

    // V5 — add_triple (dos nodos).
    let v5_subject = format!("despliegue_benigno_{case}");
    let v5_object = format!("plan_de_rollback_{case}");
    let (status, body) = call_tool(
        &mcp,
        "tylluan_graph",
        serde_json::json!({
            "command": "add_triple",
            "subject": v5_subject,
            "predicate": "se_cumple_en",
            "object": v5_object,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "V5 benigno: {status} {body}");
    for needle in [&v5_subject, &v5_object] {
        let meta = node_metadata_by_content(&state.silva, needle)
            .await
            .expect("V5 benigno: nodo no escrito");
        assert!(!flagged(&meta), "V5 benigno no debe marcarse ({needle}): {meta}");
    }

    // V6 — @skill:save.
    let v6_marker = format!("d32-ben-v6-{case}");
    let intent = format!("@skill:save:d32ben{case}: {BENIGNO_ES} ({v6_marker})");
    let (status, body) = call_tool(&mcp, "tylluan_do", serde_json::json!({ "intent": intent })).await;
    assert_eq!(status, StatusCode::OK, "V6 benigno: {status} {body}");
    let meta = node_metadata_by_content(&state.silva, &v6_marker)
        .await
        .expect("V6 benigno: nodo no escrito");
    assert!(!flagged(&meta), "V6 benigno no debe marcarse: {meta}");

    // V7 — save-cluster-summary.
    let v7_marker = format!("d32-ben-v7-{case}");
    let (status, body) = post_json(
        &app,
        "/api/v1/silva/save-cluster-summary",
        serde_json::json!({
            "cluster_id": v7_marker,
            "summary": format!("{BENIGNO_EN} (resumen {v7_marker})"),
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "V7 benigno: {status} {body}");
    let meta = node_metadata_by_content(&state.silva, &v7_marker)
        .await
        .expect("V7 benigno: nodo no escrito");
    assert!(!flagged(&meta), "V7 benigno no debe marcarse: {meta}");
}

#[tokio::test(flavor = "multi_thread")]
async fn silva_node_deja_procedencia_y_respeta_la_de_la_guild() {
    let _config_guard = CONFIG_GUARD.lock().await;
    let state = support::test_state("d32src").await;
    let app = support::build_test_app(state.clone());
    let case = next_case();

    // Sin `source`: el kernel lo rellena con `http_api`.
    let a = format!("d32-src-default-{case}");
    let (status, body) = post_json(
        &app,
        "/api/v1/silva/node",
        serde_json::json!({ "node_type": "document", "content": a, "metadata": "{}" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{status} {body}");
    let meta = node_metadata_by_content(&state.silva, &a).await.expect("nodo A");
    assert_eq!(meta.get("source").and_then(|v| v.as_str()), Some("http_api"));

    // La guild declara su procedencia: se conserva y se usa como origen.
    let b = format!("d32-src-guild-{case}");
    let (status, body) = post_json(
        &app,
        "/api/v1/silva/node",
        serde_json::json!({
            "node_type": "vision_description",
            "content": b,
            "metadata": serde_json::json!({ "source": "guild:vision" }).to_string(),
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{status} {body}");
    let meta = node_metadata_by_content(&state.silva, &b).await.expect("nodo B");
    assert_eq!(meta.get("source").and_then(|v| v.as_str()), Some("guild:vision"));
}

#[tokio::test(flavor = "multi_thread")]
async fn mark_uploads_untrusted_es_opt_in_y_marca_solo_al_encenderse() {
    let _config_guard = CONFIG_GUARD.lock().await;
    let state = support::test_state("d32opt").await;
    let app = support::build_test_app(state.clone());
    let case = next_case();

    // Apagado por defecto: contenido benigno entra limpio.
    let off = format!("d32-opt-off-{case}");
    let (status, body) = post_json(
        &app,
        "/api/v1/silva/node",
        serde_json::json!({ "node_type": "document", "content": off, "metadata": "{}" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{status} {body}");
    let meta = node_metadata_by_content(&state.silva, &off).await.expect("nodo off");
    assert!(!flagged(&meta), "apagada: no debe marcar por procedencia: {meta}");

    // Encendido: el mismo tipo de contenido queda marcado por procedencia.
    state
        .config
        .write()
        .await
        .security
        .mark_uploads_untrusted = true;
    let on = format!("d32-opt-on-{case}");
    let (status, body) = post_json(
        &app,
        "/api/v1/silva/node",
        serde_json::json!({ "node_type": "document", "content": on, "metadata": "{}" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{status} {body}");
    let meta = node_metadata_by_content(&state.silva, &on).await.expect("nodo on");
    assert!(flagged(&meta), "encendida: debe marcar por procedencia: {meta}");
    assert!(
        meta.get("untrusted_reason")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .contains("upload_provenance"),
        "el motivo debe nombrar la procedencia: {meta}"
    );

    // Restaura la config global (ver CONFIG_GUARD) para no intoxicar al resto.
    state
        .config
        .write()
        .await
        .security
        .mark_uploads_untrusted = false;
}

#[test]
fn la_regla_de_escritura_no_marca_los_corpora_benignos() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks");
    for corpus in ["benign_corpus_v1.json", "benign_corpus_es_v1.json"] {
        let path = root.join(corpus);
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", path.display()));
        let json: serde_json::Value = serde_json::from_str(&raw).expect("json valido");
        let muestras = json
            .get("muestras")
            .and_then(|v| v.as_array())
            .unwrap_or_else(|| panic!("{corpus} sin `muestras`"));
        let mut falsos = Vec::new();
        for m in muestras {
            let texto = m.get("texto").and_then(|v| v.as_str()).unwrap_or_default();
            let mut meta = serde_json::json!({});
            if tylluan_kernel::security::trust::mark_if_layer1_flagged(&mut meta, texto, "test") {
                falsos.push(m.get("id").and_then(|v| v.as_str()).unwrap_or("?").to_string());
            }
        }
        assert!(
            falsos.is_empty(),
            "{corpus}: la regla de escritura de D-32 marco {} benignos: {:?}",
            falsos.len(),
            falsos
        );
    }
}
