//! B-12 — especificación ejecutable de D-17 (capa 1 de ASI06 en `ingest` y `memory/write`).
//!
//!(spec-first, sin tocar `src/`). Contrato de D-17:
//!  - `/api/v1/memory/write` **rechaza** las 8 muestras de `benchmarks/injection_corpus_v1.json`
//!    que la capa 1 detecta (mismas 8 que `tylluan_remember` ya rechaza).
//!  - `/api/v1/ingest` **no rechaza** ninguna, pero las guarda **marcadas**
//!    (`untrusted_content` en metadata) para que el recall las trate como no confiables.
//!  - **Ningún** documento benigno de `benchmarks/benign_corpus_v1.json` se rechaza
//!    ni se marca por ninguna de las dos rutas.
//!
//! Se ejercitan los handlers reales contra el router de producción en proceso
//! (`support::test_state` + `support::build_test_app`, mismo harness que
//! `adversarial_http_battery.rs`). No se arranca ningún kernel ni se toca producción.
//!
//! Estado tras la verificación T-DSH-15 (D-17 ya integrado en origin/main):
//!  - `d17_memory_write_rejects_the_8_layer1_injections` corre en la suite
//!    normal (rápido: solo valida el rechazo de la capa 1).
//!  - `d17_ingest_stores_injections_marked_and_never_rejects` queda
//!    `#[ignore]` porque el camino de marcado hace spawn del guild `knowledge`
//!    (~60 s y dependiente del entorno Python); con `-- --ignored` pasa.
//!
//! Informe de verificación: `docs/internal/IMPL_T-DSH-15_verificacion_specs_asi06_d13_2026-10-21.md`.

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use tylluan_kernel::security::poison_patterns::matches_injection_pattern;

/// Las 8 muestras que la capa 1 detecta hoy (B-9 midió exactamente estas sobre 40).
const EXPECTED_DETECTED: &[&str] = &["inj_01", "inj_02", "inj_07", "inj_12", "inj_19", "inj_22", "inj_35", "inj_36"];

/// Las 40 muestras benignas.
const BENIGN_COUNT: usize = 40;

fn corpus_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks")
}

fn read_json(path: &std::path::Path) -> serde_json::Value {
    let raw = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("JSON inválido en {}: {e}", path.display()))
}

/// Devuelve `(id, texto)` de cada corpus, validando que las 8 esperadas siguen
/// siendo las que la capa 1 detecta — así el test no se rompe en silencio si
/// `poison_patterns.rs` cambia.
fn load_corpora() -> Corpora {
    let inj = read_json(&corpus_root().join("injection_corpus_v1.json"));
    let ben = read_json(&corpus_root().join("benign_corpus_v1.json"));

    let mut samples = Vec::new();
    for m in inj["muestras"].as_array().expect("muestras debe ser array") {
        samples.push(Sample {
            id: m["id"].as_str().expect("id").to_string(),
            texto: m["texto"].as_str().expect("texto").to_string(),
        });
    }

    let detected: Vec<&str> = samples
        .iter()
        .filter(|s| matches_injection_pattern(&s.texto))
        .map(|s| s.id.as_str())
        .collect();

    assert_eq!(
        detected.len(),
        EXPECTED_DETECTED.len(),
        "la capa 1 ya no detecta exactamente las {EXPECTED_DETECTED:?} muestras esperadas: \
         detecta {detected:?}. Si cambió `poison_patterns.rs`, actualiza EXPECTED_DETECTED."
    );
    for want in EXPECTED_DETECTED {
        assert!(detected.contains(want), "falta {want} en las detectadas: {detected:?}");
    }

    let mut benign = Vec::new();
    for m in ben["muestras"].as_array().expect("muestras debe ser array") {
        benign.push(Sample {
            id: m["id"].as_str().expect("id").to_string(),
            texto: m["texto"].as_str().expect("texto").to_string(),
        });
    }
    assert_eq!(benign.len(), BENIGN_COUNT, "el corpus benigno cambió de tamaño");

    // El benigno no debe activar la capa 1 — si lo hace, el test mediría otra cosa.
    let benign_hits: Vec<&str> = benign
        .iter()
        .filter(|s| matches_injection_pattern(&s.texto))
        .map(|s| s.id.as_str())
        .collect();
    assert!(benign_hits.is_empty(), "la capa 1 marca benignos: {benign_hits:?}");

    Corpora { injection: samples, benign }
}

/// Identificador único por invocación (el handler genera el suyo con
/// `timestamp_millis()`, que colisiona entre llamadas del mismo milisegundo).
static CASE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

type Silva = std::sync::Arc<tylluan_kernel::memory::silva::SilvaDB>;

/// Una muestra del corpus: el identificador y su texto.
struct Sample {
    id: String,
    texto: String,
}

/// Las dos mitades del corpus, ya separadas.
struct Corpora {
    injection: Vec<Sample>,
    benign: Vec<Sample>,
}

/// `POST /api/v1/memory/write` con el cuerpo JSON dado.
/// Devuelve `(status, body, marker)`, donde `marker` es el `agent_id` único que
/// se adjuntó y con el que se recupera el nodo después.
async fn post_memory_write(app: &axum::Router, content: &str) -> (StatusCode, String, String) {
    let marker = format!("b12w-{}", CASE.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/memory/write")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({ "content": content, "agent_id": marker }).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 2 * 1024 * 1024).await.unwrap();
    (status, String::from_utf8_lossy(&bytes).to_string(), marker)
}

/// `POST /api/v1/ingest` con cuerpo JSON `{"text": ...}`.
/// Devuelve `(status, body, tag)`, donde `tag` es el tag único adjuntado.
async fn post_ingest_text(app: &axum::Router, text: &str) -> (StatusCode, String, String) {
    let tag = format!("b12i-{}", CASE.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/ingest")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({ "text": text, "tags": [tag.clone()] }).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 2 * 1024 * 1024).await.unwrap();
    (status, String::from_utf8_lossy(&bytes).to_string(), tag)
}

/// ¿El nodo existe y su metadata lo marca como no confiable?
///
/// `ingest` no devuelve el id del nodo creado, así que además se busca por
/// `agent_id` (`b12w-N`) o `tags` (`b12i-N`) — los marcadores únicos que
/// adjunta este test para poder localizarlo de nuevo.
async fn is_marked_untrusted(silva: &Silva, marker_field: &str, marker: &str) -> Option<bool> {
    let mut found: Option<bool> = None;
    let mut scanned = 0usize;
    let mut page = 0usize;
    loop {
        let batch = silva
            .get_nodes_paginated(200, page * 200)
            .await
            .unwrap_or_else(|e| panic!("get_nodes_paginated({page}) falló: {e}"));
        let n = batch.len();
        for node in batch {
            scanned += 1;
            let meta: serde_json::Value =
                serde_json::from_str(&node.metadata).unwrap_or(serde_json::Value::Null);
            let hit = if marker_field == "tags" {
                meta.get("tags")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().any(|t| t.as_str() == Some(marker)))
                    .unwrap_or(false)
            } else {
                meta.get(marker_field).and_then(|v| v.as_str()) == Some(marker)
            };
            if hit {
                found = Some(meta.get("untrusted_content").and_then(|v| v.as_bool()).unwrap_or(false));
            }
        }
        if n < 200 || found.is_some() {
            break;
        }
        page += 1;
    }
    match found {
        Some(v) => Some(v),
        None => panic!("no se encontró ningún nodo con {marker_field}={marker} (escaneados: {scanned})"),
    }
}

/// Extrae el `node_id` de la respuesta de `/api/v1/memory/write`.
fn node_id_from(body: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("node_id").and_then(|n| n.as_str()).map(|s| s.to_string()))
}
#[tokio::test(flavor = "multi_thread")]
async fn d17_memory_write_rejects_the_8_layer1_injections() {
    let corpora = load_corpora();
    let state = support::test_state("b12w").await;
    let app = support::build_test_app(state.clone());

    let mut wrongly_accepted = Vec::new();
    for Sample { id, texto } in &corpora.injection {
        if !matches_injection_pattern(texto) {
            continue; // fuera del alcance de D-17 (las otras 32 no las detecta la capa 1)
        }
        let (status, body, _) = post_memory_write(&app, texto).await;
        if status == StatusCode::OK {
            wrongly_accepted.push(format!("{id}: {}", node_id_from(&body).unwrap_or_default()));
        }
    }
    assert!(
        wrongly_accepted.is_empty(),
        "D-17: `/api/v1/memory/write` aceptó muestras que la capa 1 detecta \
         (debería responder ACCESS_DENIED): {wrongly_accepted:?}"
    );

    for Sample { id, texto } in &corpora.benign {
        let (status, body, marker) = post_memory_write(&app, texto).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "D-17: `/api/v1/memory/write` rechazó el benigno {id}: {body}"
        );
        assert!(
            is_marked_untrusted(&state.silva, "agent_id", &marker).await == Some(false),
            "D-17: el benigno {id} quedó marcado como untrusted en /memory/write"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "verificado en T-DSH-15 (pasa con --ignored); se mantiene fuera de la suite normal porque el marcado hace spawn del guild knowledge (~60 s, dependiente del entorno Python)"]
async fn d17_ingest_stores_injections_marked_and_never_rejects() {
    let corpora = load_corpora();
    let state = support::test_state("b12i").await;
    let app = support::build_test_app(state.clone());

    let mut not_marked = Vec::new();
    for Sample { id, texto } in &corpora.injection {
        if !matches_injection_pattern(texto) {
            continue;
        }
        let (status, body, tag) = post_ingest_text(&app, texto).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "D-17: `/api/v1/ingest` rechazó {id} (debería guardarla marcada, no rechazar): {body}"
        );

        // `ingest` no devuelve el id del nodo: lo localizamos por el tag único
        // `b12i-N` que adjunta `post_ingest_text`.
        if is_marked_untrusted(&state.silva, "tags", &tag).await != Some(true) {
            not_marked.push(id.clone());
        }
    }
    assert!(
        not_marked.is_empty(),
        "D-17: `/api/v1/ingest` guardó estas inyecciones sin marcarlas `untrusted_content`: {not_marked:?}"
    );

    // Ningún benigno se rechaza ni se marca.
    for Sample { id, texto } in &corpora.benign {
        let (status, body, tag) = post_ingest_text(&app, texto).await;
        assert_eq!(status, StatusCode::OK, "D-17: `/api/v1/ingest` rechazó el benigno {id}: {body}");
        assert!(
            is_marked_untrusted(&state.silva, "tags", &tag).await == Some(false),
            "D-17: el benigno {id} quedó marcado como untrusted en /api/v1/ingest"
        );
    }
}
