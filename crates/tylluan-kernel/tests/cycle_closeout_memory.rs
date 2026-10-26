//! T-DSH-97 — prototipo (SOLO TESTS) de "memoria de cierre de ciclo".
//!
//! Contexto: en la evaluación de T-DSH-95, 10 de 40 preguntas con respuesta
//! conocida fallaron porque el hecho **no estaba guardado como memoria** (vivía
//! en código, commits o README). Este fichero no toca `src/` ni escribe en la
//! memoria de producción: define el esquema de un registro de cierre y lo
//! ejercita contra `SilvaDB::in_memory()`.
//!
//! Qué debe guardarse automáticamente al cerrar un ciclo (diseño de T-DSH-97):
//!   1. Entregas aceptadas: tarea, veredicto, commit, informe, criterios+resultado.
//!   2. Decisiones explícitas: flags/umbrales/defaults/reglas (DCR 0.87, freeze...).
//!   3. Mediciones y artefactos: números con puntero al JSON/asset que los respalda.
//!   4. Estado volátil: SOLO como snapshot etiquetado con su fuente autoritativa
//!      (nunca como hecho durable: un contador de arranque o el HEAD de hoy caducan).
//!   5. Implementación: rutas/símbolos clave que responden "¿dónde está esto?".
//!
//! Lo que NO se guarda: secretos/tokens, logs completos, copias de código,
//! mensajes sueltos de chat (solo eventos de cierre), ni volátiles sin fecha/fuente.

use tylluan_kernel::memory::silva::SilvaDB;

/// Registro mínimo de cierre de ciclo (prototipo a nivel de test).
#[derive(Clone)]
struct CloseoutRecord {
    task_id: &'static str,
    kind: &'static str,
    verdict: &'static str,
    commit: &'static str,
    impl_report: &'static str,
    criteria: Vec<String>,
    decisions: Vec<String>,
    measurements: Vec<String>,
    implementation: Vec<String>,
    runtime_snapshot: Option<String>,
}

impl CloseoutRecord {
    fn memory_id(&self) -> String {
        match &self.runtime_snapshot {
            // Un snapshot es inmutable y caduca: id propio, nunca reutiliza el
            // id del cierre estable (si no, dos snapshots se pisarían).
            Some(text) => format!(
                "cycle_closeout:{}:snapshot:{:016x}",
                self.task_id,
                simple_hash(text)
            ),
            None => format!("cycle_closeout:{}", self.task_id),
        }
    }

    /// Contenido pensado para BM25/recall: términos distintivos (id de tarea,
    /// commit, símbolo) en la primera línea y bloques `criterio/decisión/
    /// medición/implementación/estado` para que una consulta natural recupere
    /// el cierre por cualquiera de ellos.
    fn to_memory_content(&self) -> String {
        let mut out = format!(
            "[cierre de ciclo] tarea={} tipo={} veredicto={} commit={}\ninforme={}\n",
            self.task_id, self.kind, self.verdict, self.commit, self.impl_report
        );
        for c in &self.criteria {
            out.push_str(&format!("criterio: {c}\n"));
        }
        for d in &self.decisions {
            out.push_str(&format!("decisión: {d}\n"));
        }
        for m in &self.measurements {
            out.push_str(&format!("medición: {m}\n"));
        }
        for i in &self.implementation {
            out.push_str(&format!("implementación: {i}\n"));
        }
        if let Some(snapshot) = &self.runtime_snapshot {
            out.push_str(&format!("estado en el cierre (snapshot): {snapshot}\n"));
        }
        out
    }
}

fn simple_hash(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

fn base_record(task_id: &'static str) -> CloseoutRecord {
    CloseoutRecord {
        task_id,
        kind: "entrega_aceptada",
        verdict: "aceptada",
        commit: "79f88c6",
        impl_report: "docs/internal/IMPL_T-DSH-97_cobertura_memoria_2026-10-26.md",
        criteria: vec!["cargo clippy -D warnings: exit 0".into(), "cargo test --lib: 1293 passed".into()],
        decisions: vec![],
        measurements: vec![],
        implementation: vec![],
        runtime_snapshot: None,
    }
}

/// Las 10 categorías de hecho que la memoria viva NO recuperaba en T-DSH-95.
/// Cada registro de cierre debe cubrir una con un token verificable.
fn ten_unguarded_fact_records() -> Vec<(CloseoutRecord, &'static str)> {
    vec![
        // 1) Investigación/entrega sin números: T-DSH-88 (tests frágiles por timing).
        (
            CloseoutRecord {
                implementation: vec!["crates/tylluan-kernel: tests frágiles por timing".into()],
                ..base_record("T-DSH-88")
            },
            "timing",
        ),
        // 2) Constante de código: umbral DCR.
        (
            CloseoutRecord {
                decisions: vec!["DCR_THRESHOLD = 0.87 en handler_remember".into()],
                ..base_record("T-DSH-95")
            },
            "0.87",
        ),
        // 3) Default de configuración: perfil de instalación.
        (
            CloseoutRecord {
                decisions: vec!["perfil de instalación por defecto = portable".into()],
                ..base_record("T-DSH-92")
            },
            "portable",
        ),
        // 4) Medición con artefacto: tamaño del binario Windows.
        (
            CloseoutRecord {
                measurements: vec![
                    "archivo Windows v0.17.1: 15.364.195 bytes (15364195; sha256 6713235a…); \
                     JSON de medición de la rama readme-pro".into(),
                ],
                ..base_record("T-DSH-92")
            },
            "15364195",
        ),
        // 5) Medición con rango: arranque en Windows.
        (
            CloseoutRecord {
                measurements: vec!["arranque en Windows: 12,8–16,8 s en frío (4 ejecuciones)".into()],
                ..base_record("T-DSH-92")
            },
            "12,8",
        ),
        // 6) Estado volátil: contador del gate (snapshot, no hecho durable).
        (
            CloseoutRecord {
                runtime_snapshot: Some(
                    "CoherenceGate total_seen=570, total_eliminated=4; valor volátil desde el \
                     arranque, autoridad: GET /api/v1/security/coherence-gate/stats"
                        .into(),
                ),
                ..base_record("T-DSH-96")
            },
            "570",
        ),
        // 7) Estado volátil: HEAD de origin/main (autoridad git, no memoria).
        (
            CloseoutRecord {
                runtime_snapshot: Some(
                    "origin/main en el cierre = 4d9b416; autoridad: git log -1 origin/main".into(),
                ),
                ..base_record("T-DSH-95")
            },
            "4d9b416",
        ),
        // 8) Decisión de proceso: regla de congelación de seguridad.
        (
            CloseoutRecord {
                decisions: vec![
                    "congelación de seguridad: no se abren items de seguridad nuevos salvo \
                     hallazgo CRÍTICO verificado; el resto va a backlog etiquetado".into(),
                ],
                ..base_record("T-DSH-93")
            },
            "congelación",
        ),
        // 9) Símbolo/ruta de implementación: lista de rutas públicas.
        (
            CloseoutRecord {
                implementation: vec![
                    "is_public_bypass_path en crates/tylluan-kernel/src/transport/http/auth.rs".into(),
                ],
                ..base_record("T-DSH-86")
            },
            "is_public_bypass_path",
        ),
        // 10) Afirmación documental: auditoría externa.
        (
            CloseoutRecord {
                decisions: vec![
                    "README: sin auditoría de seguridad independiente de terceros \
                     (third-party); enlace a DISCLAIMER.md".into(),
                ],
                ..base_record("T-DSH-92")
            },
            "third-party",
        ),
    ]
}

#[test]
fn closeout_records_cover_the_ten_unguarded_fact_kinds() {
    let records = ten_unguarded_fact_records();
    assert_eq!(records.len(), 10);
    for (record, token) in records {
        let content = record.to_memory_content();
        assert!(
            content.contains(token),
            "el cierre de {} debe contener el token {token:?}:\n{content}",
            record.task_id
        );
        // El id de tarea y el commit van en la primera línea: recall por nombre.
        assert!(content.starts_with("[cierre de ciclo] tarea="));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn closeout_record_roundtrip_is_idempotent_in_memory() {
    let db = SilvaDB::in_memory().await.unwrap();
    let record = CloseoutRecord {
        decisions: vec!["DCR_THRESHOLD = 0.87".into()],
        ..base_record("T-DSH-95")
    };
    let content = record.to_memory_content();

    db.upsert_node(&record.memory_id(), "cycle_closeout", &content, "{}")
        .await
        .unwrap();
    let first = db.get_node(&record.memory_id()).await.unwrap().expect("nodo de cierre");
    assert_eq!(first.node_type, "cycle_closeout");
    assert!(first.content.contains("T-DSH-95"));
    assert!(first.content.contains("0.87"));

    // Reintento del mismo cierre (mismo id, mismo payload): no crea un nodo
    // nuevo ni cambia el contenido; el hash se mantiene estable.
    db.upsert_node(&record.memory_id(), "cycle_closeout", &content, "{}")
        .await
        .unwrap();
    let again = db.get_node(&record.memory_id()).await.unwrap().expect("nodo de cierre");
    assert_eq!(again.content_hash, first.content_hash, "cierre idempotente");
    assert_eq!(again.content, first.content);
}

#[tokio::test(flavor = "multi_thread")]
async fn volatile_snapshots_get_their_own_id_and_are_labelled() {
    let snapshot_a = CloseoutRecord {
        runtime_snapshot: Some("total_seen=570 (2026-10-26T04:45Z)".into()),
        ..base_record("T-DSH-96")
    };
    let snapshot_b = CloseoutRecord {
        runtime_snapshot: Some("total_seen=602 (2026-10-26T06:10Z)".into()),
        ..base_record("T-DSH-96")
    };
    // Un snapshot y otro del mismo task son registros distintos (no deben
    // fusionarse como si fueran el mismo hecho), y ambos se etiquetan.
    assert_ne!(snapshot_a.memory_id(), snapshot_b.memory_id());
    let content = snapshot_a.to_memory_content();
    assert!(
        content.contains("estado en el cierre (snapshot):"),
        "el estado volátil debe ir etiquetado como snapshot: {content}"
    );
    assert!(content.contains("570"));
}
