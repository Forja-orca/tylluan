//! E2E del circuito ADR-021 §4: filas escritas por el **servicio real**
//! (`DecisionService` del kernel, la misma puerta que usan MCP y REST) y
//! leídas por el **harness pareado** (`paired_eval::load_rows`, rusqlite
//! solo-lectura, conexión INDEPENDIENTE a la del servicio).
//!
//! Esto cierra el hueco que dejó el informe del turno 1220 ("no verifiqué el
//! comportamiento del servicio real"): aquí no se fabrica ninguna fila — las
//! escribe el servicio con su `log_shadow` de verdad sobre una SilvaDB de
//! fichero (schema v28), y el arnés las une con una verdad externa por
//! `request_hash` calculado con el `decision_row_hash` público del kernel.
//!
//! Escenarios:
//! - 4 peticiones reales del contrato `scheduler_routing` (una por nivel de
//!   riesgo de la matriz documentada) + 1 consulta libre que abstuvo → el
//!   arnés ve exactamente las mismas filas que `read_shadow_log` del servicio
//!   (mismo número, mismos hashes, mismo valor), se puntúan 4/4 para reglas,
//!   0/4 para el modelo (abstención = fallo, decisión documentada) y el
//!   veredicto es `model_worse` — nunca una subida de modo con Fase 1.
//! - Interruptor apagado → cero filas: no hay nada que el arnés pueda leer.
//!
//! Nota WAL: la conexión escritora (servicio) se mantiene viva durante la
//! lectura del arnés; el `-shm` existe y la apertura solo-lectura es válida.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;

use tylluan_kernel::config::TylluanConfig;
use tylluan_kernel::memory::silva::SilvaDB;
use tylluan_kernel::router::decision::api_types::{
    DecideRequest, QuestionKind, QuestionSpec,
};
use tylluan_kernel::router::decision::service::{DecisionService, decision_row_hash};

use crate::paired_eval::{self, DEFAULT_ECE_MAX};

/// Ruta canónica de la pregunta `route` (espejo de la política documentada:
/// riesgo ≥0.8 → human, ≥0.5 → guild, ≥0.2 → local_deep, resto → local_fast).
const ROUTE_OPTIONS: [&str; 4] = ["local_fast", "local_deep", "guild", "human"];

/// (riesgo, valor esperado según la matriz documentada) — la verdad de campo
/// de este e2e, escrita A MANO aquí para que no dependa del código que se mide.
const GROUND_TRUTH: [(f64, &str); 4] = [
    (0.05, "local_fast"),
    (0.25, "local_deep"),
    (0.55, "guild"),
    (0.9, "human"),
];

struct TempDb {
    dir: PathBuf,
    db_path: PathBuf,
}

impl TempDb {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "tylluan_paired_e2e_{tag}_{}_{}",
            std::process::id(),
            nanos
        ));
        std::fs::create_dir_all(&dir).expect("crear dir temporal");
        let db_path = dir.join("silva.db");
        Self { dir, db_path }
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        // Mejor esfuerzo: si el SO retiene el fichero, no es un fallo del test.
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// SilvaDB de fichero inicializada (schema v28 con `decision_shadow_log`).
async fn file_silva(db_path: &std::path::Path) -> Arc<SilvaDB> {
    let silva = Arc::new(SilvaDB::open(db_path.to_str().expect("path utf8")).expect("open silva"));
    silva.init().await.expect("init silva (schema v28)");
    silva
}

fn route_spec() -> QuestionSpec {
    QuestionSpec {
        kind: QuestionKind::Choice,
        instructions: None,
        options: ROUTE_OPTIONS.iter().map(|s| s.to_string()).collect(),
        levels: vec![],
    }
}

fn route_request(risk: f64) -> DecideRequest {
    DecideRequest {
        state: serde_json::json!({ "risk": risk }),
        questions: BTreeMap::from([("route".to_string(), route_spec())]),
        contract: Some("scheduler_routing".to_string()),
        max_latency_ms: None,
    }
}

/// E2E principal: servicio real escribe → harness pareado lee y puntúa.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rows_written_by_the_real_service_are_read_by_the_paired_harness() {
    let tmp = TempDb::new("roundtrip");
    let silva = file_silva(&tmp.db_path).await;
    let cfg = Arc::new(RwLock::new(TylluanConfig::default()));
    let service = DecisionService::new(cfg, Some(silva.clone()));

    // ── 1. Cuatro peticiones reales del contrato (una por nivel de riesgo) ──
    for (risk, _) in GROUND_TRUTH {
        let resp = service.decide(route_request(risk), "mcp").await;
        assert_eq!(
            resp.status,
            tylluan_kernel::router::decision::api_types::DecideStatus::Ok,
            "las reglas responden limpias para risk={risk}"
        );
        assert!(!resp.degraded);
    }

    // ── 2. Una consulta libre: abstención real del servicio (Fase 1) ────────
    let free = DecideRequest {
        state: serde_json::json!({ "anything": 1 }),
        questions: BTreeMap::from([(
            "should_i".to_string(),
            QuestionSpec {
                kind: QuestionKind::Boolean,
                instructions: None,
                options: vec![],
                levels: vec![],
            },
        )]),
        contract: None,
        max_latency_ms: None,
    };
    let free_resp = service.decide(free, "mcp").await;
    assert!(free_resp.answers["should_i"].abstained);
    assert_eq!(
        free_resp.answers["should_i"].abstain_reason.as_deref(),
        Some("no_model_provider")
    );

    // ── 3. Lo que ve el SERVICIO (su propia lectura) ────────────────────────
    let service_rows = service.read_shadow_log(50).await;
    assert_eq!(service_rows.len(), 5, "4 del contrato + 1 de la consulta libre");

    // ── 4. Lo que ve el HARNESS (conexión rusqlite independiente, RO) ──────
    let contract_rows = paired_eval::load_rows(&tmp.db_path, Some("scheduler_routing"))
        .expect("el arnés debe leer las filas escritas por el servicio real");
    assert_eq!(
        contract_rows.len(),
        4,
        "el arnés ve exactamente las 4 filas del contrato"
    );

    // Misma verdad en ambas vistas: hash, valor y metadatos de la fila.
    let service_view: HashSet<(String, String)> = service_rows
        .iter()
        .filter(|r| r.contract == "scheduler_routing")
        .map(|r| (r.request_hash.clone(), r.rules_value.clone().unwrap_or_default()))
        .collect();
    let harness_view: HashSet<(String, String)> = contract_rows
        .iter()
        .map(|r| {
            (
                r.request_hash.clone(),
                r.rules_value.clone(),
            )
        })
        .collect();
    assert_eq!(
        service_view, harness_view,
        "ambas vistas (servicio y arnés) ven las mismas filas"
    );
    for r in &contract_rows {
        assert_eq!(r.provider.as_deref(), Some("rules-v0"));
        assert_eq!(r.mode.as_deref(), Some("shadow"));
        assert_eq!(r.request_hash.len(), 64, "sha256 hex, jamás el state");
        assert!(r.model_value.is_none(), "NULL hasta Fase 2");
        assert!(r.model_probs.is_none());
    }

    // ── 5. Verdad externa por request_hash (join del arnés, §1217) ─────────
    let mut truth: paired_eval::TruthMap = BTreeMap::new();
    for (risk, expected) in GROUND_TRUTH {
        let state = serde_json::json!({ "risk": risk });
        truth.insert(decision_row_hash(&state, "route"), expected.to_string());
    }
    let truth_path = tmp.dir.join("truth.json");
    std::fs::write(&truth_path, serde_json::to_string(&truth).unwrap()).expect("truth json");

    let report = paired_eval::run_paired(
        &tmp.db_path,
        Some("scheduler_routing"),
        &truth_path,
        DEFAULT_ECE_MAX,
    )
    .expect("evaluación pareada sobre filas reales");

    assert_eq!(report.global.rows_total, 4);
    assert_eq!(report.contracts.len(), 1);
    let c = &report.contracts[0];
    assert_eq!(c.contract, "scheduler_routing");
    assert_eq!(c.n_rows, 4);
    assert_eq!(
        c.n_labeled, 4,
        "las 4 filas reales se unen con la verdad por request_hash"
    );
    assert_eq!(c.rules_correct, 4, "reglas 4/4 contra la matriz a mano");
    assert_eq!(c.model_correct, 0, "Fase 1: el modelo no respondió");
    assert_eq!(c.n_abstained, 4);
    assert!((c.abstention_rate.unwrap() - 1.0).abs() < 1e-12);
    assert!((c.rules_accuracy.unwrap() - 1.0).abs() < 1e-12);
    assert!((c.model_accuracy.unwrap()).abs() < 1e-12);
    let mc = c.mcnemar.as_ref().expect("hay pares etiquetados");
    assert_eq!((mc.b, mc.c), (4, 0), "reglas bien y modelo mal en las 4");
    // p = 2 · P(X ≤ 0) = 2/16 = 0.125 — el modelo pierde, no sube de modo.
    assert!((mc.p_value - 0.125).abs() < 1e-12, "p={}", mc.p_value);
    assert!(c.brier.is_none() && c.ece.value.is_none(), "sin probabilidades");
    assert!(!c.verdict.upgrade_allowed);
    assert_eq!(c.verdict.reason, "model_worse");

    // ── 6. La consulta libre SÍ está en la tabla, pero fuera del join ──────
    let all = paired_eval::load_rows(&tmp.db_path, None).expect("tabla completa");
    assert_eq!(all.len(), 5, "la abstención también deja fila auditada");
    let free_rows: Vec<_> = all.iter().filter(|r| r.contract.is_empty()).collect();
    assert_eq!(free_rows.len(), 1);
    assert!(free_rows[0].rules_value.is_empty(), "abstención sin valor de reglas (NULL aplaneado a \"\" por el arnés: nunca coincide con la verdad)");

    let free_report = paired_eval::run_paired(
        &tmp.db_path,
        None,
        &truth_path,
        DEFAULT_ECE_MAX,
    )
    .expect("run sin filtro");
    let free_eval = free_report
        .contracts
        .iter()
        .find(|c| c.contract.is_empty())
        .expect("grupo de la consulta libre");
    assert_eq!(free_eval.n_rows, 1);
    assert_eq!(
        free_eval.n_labeled, 0,
        "su hash no está en la verdad → fuera del score (insufficient_data)"
    );
    assert_eq!(free_eval.verdict.reason, "insufficient_data");

    // El drop de TempDb limpia ficheros (y de paso la conexión del servicio
    // se libera al salir de este ámbito).
    drop(service);
    drop(silva);
}

/// Interruptor global apagado → el servicio no escribe NADA → el arnés no
/// tiene filas que leer (el hueco "cero efectos" visto desde la otra puerta).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disabled_service_leaves_the_paired_harness_with_no_rows() {
    let tmp = TempDb::new("disabled");
    let silva = file_silva(&tmp.db_path).await;
    let mut cfg = TylluanConfig::default();
    cfg.decision.enabled = false;
    let service = DecisionService::new(Arc::new(RwLock::new(cfg)), Some(silva.clone()));

    let resp = service.decide(route_request(0.9), "rest").await;
    assert_eq!(
        resp.status,
        tylluan_kernel::router::decision::api_types::DecideStatus::Disabled
    );
    assert!(service.read_shadow_log(10).await.is_empty());

    // Incluso con verdad disponible, no hay filas: el arnés reporta 0 filas
    // y run_paired no inventa pares.
    let truth = serde_json::json!({
        decision_row_hash(&serde_json::json!({ "risk": 0.9 }), "route"): "human"
    });
    let truth_path = tmp.dir.join("truth.json");
    std::fs::write(&truth_path, serde_json::to_string(&truth).unwrap()).unwrap();

    let rows = paired_eval::load_rows(&tmp.db_path, None).expect("tabla existe (v28), vacía");
    assert!(rows.is_empty(), "apagado = cero filas para el arnés");

    let report = paired_eval::run_paired(&tmp.db_path, None, &truth_path, DEFAULT_ECE_MAX)
        .expect("run sobre BD vacía");
    assert_eq!(report.global.rows_total, 0);
    assert_eq!(report.global.contracts_evaluated, 0);
    assert_eq!(report.global.contracts_upgrade_recommended, 0);

    drop(service);
    drop(silva);
}
