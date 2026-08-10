//! # LLM Decision Examples (fase 1 del circuito CoherenceGate → dataset)
//!
//! Recolector aditivo de ejemplos estructurados de decisión LLM en modo
//! observación (ADR-011 Layer 4 hybrid). Cada fila es un par A/B evaluable:
//!
//! - `gate_label`: la decisión determinista del gate (KEEP si el nodo pasó
//!   limpio las capas 2-3, REJECT si fue penalizado).
//! - `llm_decision`: el veredicto del profesor (clasificador LLM en vivo).
//!
//! La fase 1 SOLO recolecta y exporta — no entrena nada, no bloquea el path
//! crítico (los INSERTs son best-effort fire-and-forget) y no modifica scores.
//! La etiqueta de referencia es el profesor con su confianza: mide
//! reproducción del profesor por un modelo pequeño, NO corrección del profesor.

use serde::{Deserialize, Serialize};

/// Decisión determinista del gate para un nodo (etiqueta de referencia A).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GateLabel {
    /// El nodo sobrevivió sin penalización en capas 2-3.
    Keep,
    /// El nodo fue penalizado (provenance federada o drift semántico).
    Reject,
}

impl GateLabel {
    pub fn as_str(&self) -> &'static str {
        match self {
            GateLabel::Keep => "KEEP",
            GateLabel::Reject => "REJECT",
        }
    }
}

/// Un ejemplo estructurado de decisión, tal y como se persiste en
/// `llm_decision_examples`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionExample {
    pub workflow_id: i64,
    pub query: String,
    pub node_id: String,
    /// Zonas del hybrid trigger que dispararon (p.ej. "A,C").
    pub trigger_zones: String,
    /// KEEP | KEEP_SOFT | REJECT — veredicto del clasificador LLM (profesor).
    pub llm_decision: String,
    /// Confianza del profesor si el backend la expone; `None` en fase 1.
    pub llm_confidence: Option<f32>,
    pub gate_label: String,
    /// Score pre-penalización. `None` en fase 1: `hybrid_classify` solo recibe
    /// survivors post-penalty; reconstruir el pre-penalty no es fiable
    /// (ambas penalizaciones son ×0.1 y pueden apilarse).
    pub score_before: Option<f32>,
    pub score_after: f32,
    /// Identificador del profesor (p.ej. "Qwen3-30B-A3B"). `unknown` en
    /// fase 1: el backend de razonamiento no expone el modelo cargado.
    pub model: String,
    pub latency_ms: i64,
    pub created_at: String,
}

const EXAMPLES_TABLE: &str = "llm_decision_examples";

fn ensure_examples_schema(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute(
        &format!(
            "CREATE TABLE IF NOT EXISTS {EXAMPLES_TABLE} (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                workflow_id INTEGER NOT NULL DEFAULT 0,
                query TEXT NOT NULL,
                node_id TEXT NOT NULL,
                trigger_zones TEXT NOT NULL,
                llm_decision TEXT NOT NULL,
                llm_confidence REAL,
                gate_label TEXT NOT NULL,
                score_before REAL,
                score_after REAL NOT NULL,
                model TEXT NOT NULL,
                latency_ms INTEGER NOT NULL,
                created_at TEXT NOT NULL
            )"
        ),
        [],
    )
    .map_err(|e| format!("llm_examples schema: {e}"))?;
    Ok(())
}

fn open_examples_db() -> Result<rusqlite::Connection, String> {
    let db_path = crate::security::friction_log::friction_db_path();
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("llm_examples mkdir: {e}"))?;
    }
    let conn = crate::config::open_db(&db_path).map_err(|e| format!("llm_examples open: {e}"))?;
    conn.busy_timeout(std::time::Duration::from_secs(5)).ok();
    ensure_examples_schema(&conn)?;
    Ok(conn)
}

/// Persistir un ejemplo de decisión (best-effort; el caller ignora el error).
/// Fire-and-forget por diseño: nunca bloquea el path crítico del recall.
pub fn log_decision_example(ex: &DecisionExample) -> Result<i64, String> {
    let conn = open_examples_db()?;
    conn.execute(
        &format!(
            "INSERT INTO {EXAMPLES_TABLE}
             (workflow_id, query, node_id, trigger_zones, llm_decision, llm_confidence,
              gate_label, score_before, score_after, model, latency_ms, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)"
        ),
        rusqlite::params![
            ex.workflow_id,
            ex.query,
            ex.node_id,
            ex.trigger_zones,
            ex.llm_decision,
            ex.llm_confidence,
            ex.gate_label,
            ex.score_before,
            ex.score_after,
            ex.model,
            ex.latency_ms,
            ex.created_at,
        ],
    )
    .map_err(|e| format!("llm_examples insert: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// Split determinista por node_id (80/20): el mismo nodo cae siempre en el
/// mismo split, garantizando que la evaluación held-out no tenga leak.
pub fn split_for(node_id: &str) -> &'static str {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    node_id.hash(&mut hasher);
    let bucket = hasher.finish() % 10;
    if bucket < 8 { "train" } else { "heldout" }
}

/// Estadísticas del dataset exportado.
#[derive(Debug, Default, Serialize)]
pub struct ExportStats {
    pub total: usize,
    pub train: usize,
    pub heldout: usize,
    pub gate_llm_agreement: f32,
}

/// Leer todos los ejemplos con su split, listos para serializar.
/// Devuelve las filas (JSON) y estadísticas en un solo paso.
pub fn collect_examples_json() -> Result<(Vec<serde_json::Value>, ExportStats), String> {
    let conn = open_examples_db()?;
    let mut stmt = conn
        .prepare(&format!(
            "SELECT id, workflow_id, query, node_id, trigger_zones, llm_decision,
                    llm_confidence, gate_label, score_before, score_after, model,
                    latency_ms, created_at
             FROM {EXAMPLES_TABLE} ORDER BY id"
        ))
        .map_err(|e| format!("llm_examples select: {e}"))?;

    let rows = stmt
        .query_map([], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, i64>(0)?,
                "workflow_id": r.get::<_, i64>(1)?,
                "query": r.get::<_, String>(2)?,
                "node_id": r.get::<_, String>(3)?,
                "trigger_zones": r.get::<_, String>(4)?,
                "llm_decision": r.get::<_, String>(5)?,
                "llm_confidence": r.get::<_, Option<f32>>(6)?,
                "gate_label": r.get::<_, String>(7)?,
                "score_before": r.get::<_, Option<f32>>(8)?,
                "score_after": r.get::<_, f32>(9)?,
                "model": r.get::<_, String>(10)?,
                "latency_ms": r.get::<_, i64>(11)?,
                "created_at": r.get::<_, String>(12)?,
                "split": "placeholder",
            }))
        })
        .map_err(|e| format!("llm_examples map: {e}"))?;

    let mut stats = ExportStats::default();
    let mut agreement_count: usize = 0;
    let mut out: Vec<serde_json::Value> = Vec::new();

    for row in rows {
        let mut row = row.map_err(|e| format!("llm_examples row: {e}"))?;
        let node_id = row["node_id"].as_str().unwrap_or("").to_string();
        let split = split_for(&node_id);
        row["split"] = serde_json::Value::String(split.to_string());
        if split == "train" {
            stats.train += 1;
        } else {
            stats.heldout += 1;
        }
        let llm = row["llm_decision"].as_str().unwrap_or("");
        let gate = row["gate_label"].as_str().unwrap_or("");
        if (llm == "KEEP" || llm == "KEEP_SOFT") && gate == "KEEP"
            || llm == "REJECT" && gate == "REJECT"
        {
            agreement_count += 1;
        }
        out.push(row);
        stats.total += 1;
    }

    stats.gate_llm_agreement = if stats.total > 0 {
        agreement_count as f32 / stats.total as f32
    } else {
        0.0
    };
    Ok((out, stats))
}

/// Exportar todos los ejemplos a JSONL (una fila por línea) con su split.
/// Devuelve estadísticas: total / train / heldout / acuerdo gate↔LLM.
pub fn export_examples_jsonl(out_path: &std::path::Path) -> Result<ExportStats, String> {
    let (rows, mut stats) = collect_examples_json()?;
    let mut file = std::fs::File::create(out_path)
        .map_err(|e| format!("llm_examples create {out_path:?}: {e}"))?;
    for row in &rows {
        let line = serde_json::to_string(row).map_err(|e| format!("llm_examples serde: {e}"))?;
        use std::io::Write;
        writeln!(file, "{line}").map_err(|e| format!("llm_examples write: {e}"))?;
    }
    stats.total = rows.len();
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Comparte el mutex global de la DB de test (mismo TEST_DB_PATH para todo
    /// el crate: friction_log, llm_examples y router matcher se serializan).
    static TEST_MUTEX: &std::sync::Mutex<()> = &crate::security::friction_log::TEST_DB_MUTEX;

    fn example(node_id: &str) -> DecisionExample {
        DecisionExample {
            workflow_id: 0,
            query: "test query".to_string(),
            node_id: node_id.to_string(),
            trigger_zones: "A".to_string(),
            llm_decision: "REJECT".to_string(),
            llm_confidence: None,
            gate_label: "REJECT".to_string(),
            score_before: None,
            score_after: 0.42,
            model: "test-professor".to_string(),
            latency_ms: 120,
            created_at: "2026-08-11T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_log_and_roundtrip() {
        let _guard = TEST_MUTEX.lock().unwrap();
        crate::security::friction_log::set_unique_test_db();
        let id = log_decision_example(&example("node-1")).expect("insert");
        assert!(id > 0);

        let conn = open_examples_db().expect("open");
        let row: (String, String, String) = conn
            .query_row(
                "SELECT node_id, llm_decision, gate_label FROM llm_decision_examples WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .expect("query");
        assert_eq!(row, ("node-1".to_string(), "REJECT".to_string(), "REJECT".to_string()));
    }

    #[test]
    fn test_schema_idempotent() {
        let _guard = TEST_MUTEX.lock().unwrap();
        crate::security::friction_log::set_unique_test_db();
        let conn = open_examples_db().expect("open #1");
        drop(conn);
        let conn = open_examples_db().expect("open #2");
        assert!(conn
            .execute("SELECT 1 FROM llm_decision_examples LIMIT 1", [])
            .is_ok());
    }

    #[test]
    fn test_split_is_deterministic_per_node() {
        assert_eq!(split_for("node-7"), split_for("node-7"));
        assert_eq!(split_for("xyz:123:abc"), split_for("xyz:123:abc"));
        let mut train = 0;
        for i in 0..100 {
            if split_for(&format!("node-{i}")) == "train" {
                train += 1;
            }
        }
        let ratio = train as f32 / 100.0;
        assert!((0.70..0.90).contains(&ratio), "expected ~80/20, got {ratio}");
    }

    #[test]
    fn test_export_jsonl_no_leak() {
        let _guard = TEST_MUTEX.lock().unwrap();
        crate::security::friction_log::set_unique_test_db();
        for i in 0..40 {
            let _ = log_decision_example(&example(&format!("node-{i}")));
        }
        let out = std::env::temp_dir().join(format!(
            "tylluan_examples_export_{}.jsonl",
            std::process::id()
        ));
        let stats = export_examples_jsonl(&out).expect("export");
        assert_eq!(stats.total, 40);

        let content = std::fs::read_to_string(&out).expect("read");
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 40);

        let mut train_ids = std::collections::HashSet::new();
        let mut heldout_ids = std::collections::HashSet::new();
        for line in &lines {
            let v: serde_json::Value = serde_json::from_str(line).expect("json line");
            let node = v["node_id"].as_str().unwrap().to_string();
            match v["split"].as_str() {
                Some("train") => { train_ids.insert(node); }
                Some("heldout") => { heldout_ids.insert(node); }
                _ => panic!("missing split"),
            }
        }
        assert!(train_ids.is_disjoint(&heldout_ids), "split leak: node in both");
        assert_eq!(train_ids.len() + heldout_ids.len(), 40);
    }

    #[test]
    fn test_export_agreement_metric() {
        let _guard = TEST_MUTEX.lock().unwrap();
        crate::security::friction_log::set_unique_test_db();
        let mut agree = example("n-agree");
        agree.llm_decision = "KEEP".to_string();
        agree.gate_label = "KEEP".to_string();
        let _ = log_decision_example(&agree);
        let mut disagree = example("n-disagree");
        disagree.llm_decision = "KEEP_SOFT".to_string();
        disagree.gate_label = "REJECT".to_string();
        let _ = log_decision_example(&disagree);
        let _ = log_decision_example(&disagree);

        let out = std::env::temp_dir().join(format!(
            "tylluan_examples_agree_{}.jsonl",
            std::process::id()
        ));
        let stats = export_examples_jsonl(&out).expect("export");
        assert_eq!(stats.total, 3);
        assert_eq!(stats.gate_llm_agreement, 1.0 / 3.0);
    }
}
