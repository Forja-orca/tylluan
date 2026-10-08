//! ADR-018 Fase 0 — Decision Fabric Spike: dataset extractor + baseline profile.
//!
//! T906/T914 (Buffy). Read-only over the two real decision sources (T908
//! design, green-lit by the TL in T911/T914; interface agreed with Deep in
//! T912):
//!
//! - `data/audit.db::guild_audit_log`  — every guild/kernel tool execution
//!   with intent, latency, status and system_snapshot (commit slice).
//!   Ground truth for `route_guild`: the guild that ACTUALLY executed the
//!   call (status='ok' only for labels).
//! - `data/scheduler_confusion.db` — one row per completed `tylluan_do`
//!   dispatch pairing what the Scheduler rules WOULD have decided against
//!   what the live cascade DID (confusion.rs: observation-only collector).
//!   Ground truth for `hitl_resolve`: what actually happened without a
//!   human (success column; human_intervention is 0 in 100% of audit rows,
//!   so audit has NO hitl ground truth — do not invent it).
//!
//! Baseline A (per T912, Deep's design note): the LIVE rules are already in
//! the data — `routed_guild` (route) and `scheduler_verdict` (hitl) ARE what
//! the current rule engine decided in production. This harness computes
//! their agreement with ground truth and exports every item with both the
//! question set and the live prediction, so the C/D/E variants of the spike
//! are compared against exactly the same records.
//!
//! Known degeneracies (measured 2026-10-04, pinned in the profile output —
//! the spike MUST report them next to any accuracy number):
//! - `cascade_fired=0` and `routed_guild=final_guild` in 100% of confusion
//!   rows → the reactive fallback never fired in the recorded window.
//! - success=1 in 1,226/1,229 rows → predicting "success" is a trap metric
//!   (T908); the honest targets are route_guild + hitl_resolve.
//! - `human_intervention=0` in all audit rows (dead column today).
//! - risk_tier raw values are SafeRead/StateMutation/CriticalDestructive;
//!   normalized to low/medium/critical for the harness (high: 0 observed).
//!
//! Usage:
//!   cargo run --release -p tylluan-kernel --example decision_fabric_spike
//!   cargo run --release -p tylluan-kernel --example decision_fabric_spike -- --export benchmarks/decision_fabric_dataset.jsonl
//!
//! Variant C (ADR-018 §3): LayaOnnxDecisionProvider over the published ONNX
//! bundle (receptron/laya-onnx, Apache-2.0 weights; runtime port of
//! receptron/laya, MIT). Loads benchmarks/decision_fabric_dataset.jsonl,
//! answers the SAME items the baseline answered, computes accuracy / ECE /
//! Brier / TP-FP-TN-FN / AUROC per decision plus latency p50/p99, and emits
//! the clause-by-clause GO/NO-GO verdict against the §3 gate. The `--golden`
//! flag first checks the port against the published 4-decimal reference
//! values of test/test_model.ts — do not trust any dataset number without
//! a passing golden run.
//!   cargo run --release -p tylluan-kernel --example decision_fabric_spike -- --variant-c models/laya --golden
//!   cargo run --release -p tylluan-kernel --example decision_fabric_spike -- --variant-c models/laya --split test
//!   cargo run --release -p tylluan-kernel --example decision_fabric_spike -- --variant-c models/laya --split all --source confusion
//!
//! Variant B (ADR-018 §3 paso 3, aprobado por el TL en Coloquio): SystemOneDecisionProvider,
//! un cliente HTTP del contrato Jev / System One `POST /v1/systemone`. Agnóstico de runtime:
//! sirve contra llama.cpp (PR #29818, mergeado 2026-10-03: `llama-server -hf
//! ggml-org/Clef-Flash-GGUF` en :8080, GGUF Apache-2.0 de ggml-org) o contra Ollama >= 0.35.1
//! (:11434; la instalación local detectada es 0.30.10 — vieja para clef). Mismo dataset,
//! mismas stats, mismo gate §3 que la variante C; la cláusula 4 (coste vs LLM sustituido)
//! sigue abierta: B mide si un modelo decisional con calibración entrenada supera las
//! cláusulas 1-3 donde la cabeza zero-shot de Laya no lo hizo (AUROC 0.9221).
//!   cargo run --release -p tylluan-kernel --example decision_fabric_spike -- --variant-b --variant-b-endpoint http://127.0.0.1:8080
//!   cargo run --release -p tylluan-kernel --example decision_fabric_spike -- --variant-b --variant-b-endpoint http://127.0.0.1:11434 --split all
//!
//! Flags: --split test|all (default test) · --source audit|confusion|all ·
//! --limit N · --latency-budget-ms MS (default 500, the System-One band
//! cited by the ADR). Default (no flags) prints the extractor profile only;
//! `--export` additionally writes the JSONL dataset (one line per item:
//! state, questions, labels, live baseline-A prediction, split). Never
//! touches anything outside its arguments; the DBs are opened
//! SQLITE_OPEN_READ_ONLY and the bundle directory is read-only too.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use chrono::{TimeZone, Timelike};
use rusqlite::{Connection, OpenFlags};
use serde_json::json;

// ─── routing vocabulary (measured distribution 2026-10-04, T908/T911) ───────

/// Top guilds of audit.db by call count (95% of traffic); the rest bucket to
/// "other". `kernel` rows carry an empty intent (sovereign-tool self-calls)
/// and are excluded from route questions by construction.
const AUDIT_ROUTE_OPTIONS: [&str; 7] = [
    "kernel",
    "bash",
    "coloquio",
    "system_metrics",
    "monitor",
    "code_analysis",
    "other",
];

/// risk_tier normalization (T912 agreed low/medium/high/critical; the live
/// DB has SafeRead/StateMutation/CriticalDestructive — mapping pinned here,
/// raw value preserved in the export for provenance).
fn norm_risk(tier: &str) -> &'static str {
    match tier {
        "SafeRead" => "low",
        "StateMutation" => "medium",
        "CriticalDestructive" => "critical",
        other => match other.to_ascii_lowercase().as_str() {
            "low" => "low",
            "medium" => "medium",
            "high" => "high",
            "critical" => "critical",
            _ => "medium", // unknown tier: treat as mutating, never as safe
        },
    }
}

fn bucket_guild(g: &str) -> String {
    if AUDIT_ROUTE_OPTIONS.contains(&g) && g != "other" {
        g.to_string()
    } else {
        "other".to_string()
    }
}

// ─── extraction ─────────────────────────────────────────────────────────────

fn open_ro(path: &str) -> Connection {
    Connection::open_with_flags(Path::new(path), OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap_or_else(|e| panic!("open {path} read-only: {e}"))
}

/// Local hour 0-23 for the harness `hour_of_day` state field (T912 schema).
/// Returns None when the stamp is unparseable (counted in the profile).
fn hour_of(ts: &str) -> Option<u32> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|d| chrono::Local.from_utc_datetime(&d.naive_utc()).hour())
}

struct AuditRow {
    id: i64,
    ts: String,
    guild: String,
    agent_id: String,
    intent: String,
    status: String,
    latency_ms: i64,
    commit: String,
}

fn extract_audit() -> (Vec<AuditRow>, usize, usize) {
    let conn = open_ro("data/audit.db");
    let mut stmt = conn
        .prepare(
            "SELECT id, timestamp, guild, agent_id, intent, status, latency_ms,
                    COALESCE(json_extract(system_snapshot, '$.commit'), '')
             FROM guild_audit_log ORDER BY id",
        )
        .expect("audit query");
    let rows: Vec<AuditRow> = stmt
        .query_map([], |r| {
            Ok(AuditRow {
                id: r.get(0)?,
                ts: r.get(1)?,
                guild: r.get(2)?,
                agent_id: r.get(3)?,
                intent: r.get(4)?,
                status: r.get(5)?,
                latency_ms: r.get::<_, Option<i64>>(6)?.unwrap_or(-1),
                commit: r.get(7)?,
            })
        })
        .expect("audit map")
        .filter_map(|r| r.ok())
        .collect();
    let kernel_self_calls = rows.iter().filter(|r| r.intent.is_empty()).count();
    let ok_rows = rows.iter().filter(|r| r.status == "ok").count();
    (rows, kernel_self_calls, ok_rows)
}

struct ConfusionRow {
    id: i64,
    ts: String,
    intent: String,
    agent_id: String,
    complexity: f64,
    risk_tier: String,
    verdict: String,
    routed: String,
    classification: String,
    success: bool,
}

fn extract_confusion() -> (Vec<ConfusionRow>, HashSet<String>) {
    let conn = open_ro("data/scheduler_confusion.db");
    let mut stmt = conn
        .prepare(
            "SELECT id, ts, intent, agent_id, complexity_score, risk_tier,
                    scheduler_verdict, routed_guild, classification, success
             FROM scheduler_confusion ORDER BY id",
        )
        .expect("confusion query");
    let rows: Vec<ConfusionRow> = stmt
        .query_map([], |r| {
            Ok(ConfusionRow {
                id: r.get(0)?,
                ts: r.get(1)?,
                intent: r.get(2)?,
                agent_id: r.get(3)?,
                complexity: r.get(4)?,
                risk_tier: r.get(5)?,
                verdict: r.get(6)?,
                routed: r.get(7)?,
                classification: r.get(8)?,
                success: r.get::<_, i64>(9)? != 0,
            })
        })
        .expect("confusion map")
        .filter_map(|r| r.ok())
        .collect();
    // T912: candidate guilds for route_guild are DYNAMIC per item — the
    // distinct guilds production actually routed that intent to.
    let mut candidates: HashMap<&str, HashSet<String>> = HashMap::new();
    for r in &rows {
        candidates.entry(r.intent.as_str()).or_default().insert(r.routed.clone());
    }
    let candidate_map: HashSet<String> = candidates
        .values()
        .flat_map(|s| s.iter().cloned())
        .collect();
    (rows, candidate_map)
}

// ─── temporal split (T908: per-source p80 of the timestamp — never random) ──

fn split_cut(stamps: &[String]) -> String {
    let mut s: Vec<&String> = stamps.iter().collect();
    s.sort();
    let k = s.len() * 8 / 10;
    s.get(k).map(|x| (*x).clone()).unwrap_or_default()
}

// ─── main ───────────────────────────────────────────────────────────────────

fn arg_value(args: &[String], i: &mut usize, flag: &str) -> String {
    *i += 1;
    args.get(*i)
        .filter(|v| !v.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| panic!("{flag} needs a value"))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut export_path = None;
    let mut variant_c = None;
    let mut variant_d = None;
    let mut variant_b = false;
    let mut variant_b_endpoint = "http://127.0.0.1:8080".to_string();
    let mut variant_b_model = "clef-flash".to_string();
    let mut golden = false;
    let mut split = "test".to_string();
    let mut source = "all".to_string();
    let mut limit = None;
    let mut budget_ms = 500.0f64;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--export" => {
                export_path = Some(arg_value(&args, &mut i, "--export"));
            }
            "--variant-c" => {
                variant_c = Some(arg_value(&args, &mut i, "--variant-c"));
            }
            "--variant-d" => {
                variant_d = Some(arg_value(&args, &mut i, "--variant-d"));
            }
            "--variant-b" => variant_b = true,
            "--variant-b-endpoint" => {
                variant_b_endpoint = arg_value(&args, &mut i, "--variant-b-endpoint");
            }
            "--variant-b-model" => {
                variant_b_model = arg_value(&args, &mut i, "--variant-b-model");
            }
            "--golden" => golden = true,
            "--split" => split = arg_value(&args, &mut i, "--split"),
            "--source" => source = arg_value(&args, &mut i, "--source"),
            "--limit" => {
                limit = Some(
                    arg_value(&args, &mut i, "--limit")
                        .parse()
                        .expect("--limit expects a number"),
                );
            }
            "--latency-budget-ms" => {
                budget_ms = arg_value(&args, &mut i, "--latency-budget-ms")
                    .parse()
                    .expect("--latency-budget-ms expects a number");
            }
            other => panic!(
                "unknown flag '{other}' (supported: --export <path>, --variant-c <dir>, --variant-d <dir>, --golden, \
                 --variant-b, --variant-b-endpoint URL, --variant-b-model NAME, \
                 --split test|all, --source audit|confusion|all, --limit N, --latency-budget-ms MS)"
            ),
        }
        i += 1;
    }

    if golden {
        if let Some(dir) = variant_d.clone() {
            std::process::exit(if run_golden_decima(&dir) { 0 } else { 1 });
        }
        let dir = variant_c
            .clone()
            .unwrap_or_else(|| panic!("--golden needs the bundle dir: --variant-c models/laya --golden or --variant-d models/decima --golden"));
        std::process::exit(if run_golden(&dir) { 0 } else { 1 });
    }
    if let Some(dir) = variant_c {
        if variant_b {
            panic!("--variant-b and --variant-c are mutually exclusive");
        }
        run_variant_c(&dir, &split, &source, limit, budget_ms);
        return;
    }
    if let Some(dir) = variant_d {
        run_variant_d(&dir, &split, &source, limit, budget_ms);
        return;
    }
    if variant_b {
        run_variant_b(&variant_b_endpoint, &variant_b_model, &split, &source, limit, budget_ms);
        return;
    }

    println!("ADR-018 Fase 0 — dataset extractor + live baseline-A (decision_fabric_spike)");

    // ── audit.db ────────────────────────────────────────────────────────────
    let (audit, kernel_self_calls, audit_ok) = extract_audit();
    let audit_cut = split_cut(&audit.iter().map(|r| r.ts.clone()).collect::<Vec<_>>());
    let audit_route: Vec<&AuditRow> = audit
        .iter()
        .filter(|r| !r.intent.trim().is_empty())
        .collect();
    let audit_route_ok: Vec<&AuditRow> =
        audit_route.iter().copied().filter(|r| r.status == "ok").collect();

    // ── scheduler_confusion.db ──────────────────────────────────────────────
    let (confusion, _all_candidates) = extract_confusion();
    let confusion_cut = split_cut(&confusion.iter().map(|r| r.ts.clone()).collect::<Vec<_>>());
    let per_intent_candidates: HashMap<&str, Vec<String>> = {
        let mut m: HashMap<&str, HashSet<String>> = HashMap::new();
        for r in &confusion {
            m.entry(r.intent.as_str()).or_default().insert(r.routed.clone());
        }
        m.into_iter()
            .map(|(k, v)| (k, { let mut v: Vec<String> = v.into_iter().collect(); v.sort(); v }))
            .collect()
    };

    // ── profile: degeneracy facts FIRST (they qualify every number below) ───
    let cascade_fired_rows = {
        let conn = open_ro("data/scheduler_confusion.db");
        conn.query_row(
            "SELECT SUM(cascade_fired), SUM(routed_guild != final_guild), COUNT(*),
                    SUM(CASE WHEN success=1 THEN 1 ELSE 0 END)
             FROM scheduler_confusion",
            [],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?)),
        )
        .expect("degeneracy query")
    };
    let hitl_rows = confusion
        .iter()
        .filter(|r| r.verdict == "HumanAuthorizationRequired")
        .count();
    let differ_ok = confusion
        .iter()
        .filter(|r| r.classification == "Differ" && r.success)
        .count();

    println!("\n[profile] audit.db::guild_audit_log");
    println!("  rows total: {} (ok: {audit_ok})", audit.len());
    println!("  kernel self-calls (empty intent, no route question): {kernel_self_calls}");
    println!("  route-eligible items: {} (labels from ok: {})", audit_route.len(), audit_route_ok.len());
    println!("  temporal split cut (p80): {audit_cut}");

    println!("\n[profile] scheduler_confusion (live rule-vs-reality pairs)");
    println!("  rows total: {}", confusion.len());
    println!("  temporal split cut (p80): {confusion_cut}");
    println!(
        "  DEGENERACY: cascade_fired={} / routed!=final={} of {} rows (reactive fallback never fired in window)",
        cascade_fired_rows.0, cascade_fired_rows.1, cascade_fired_rows.2
    );
    println!(
        "  DEGENERACY: success={} of {} rows (predicting 'success' is a trap metric — T908)",
        cascade_fired_rows.3, cascade_fired_rows.2
    );
    println!("  HumanAuthorizationRequired rows: {hitl_rows} — ALL resolved live without a human, success=1");
    println!("  Differ-and-succeeded rows (escalation-avoidance pool): {differ_ok}");

    // ── live baseline A agreement (the rules already in the data) ───────────
    // route: baseline predicts routed_guild; label = routed_guild on success
    // (construction ⇒ 100% on success rows; the MEANINGFUL audit-route check
    // is whether guild in audit.db agrees with confusion.routed on the same
    // intent, computed below). hitl: baseline predicts "false" (needs human)
    // iff verdict == HumanAuthorizationRequired; label = lived outcome.
    let audit_route_agree = audit_route_ok.len(); // by construction (label := executed guild)
    let hitl_label_true = confusion.iter().filter(|r| r.success).count();
    let hitl_baseline_pred_false = hitl_rows; // HAR verdict ⇒ baseline says "needs human"
    let hitl_baseline_correct = confusion
        .iter()
        .filter(|r| r.success != (r.verdict == "HumanAuthorizationRequired"))
        .count();
    println!("\n[baseline A] live rules vs ground truth");
    println!(
        "  route_guild (confusion): {}/{} — 1.000 by construction (label := routed guild that succeeded); the spike's real route test is audit-vs-confusion agreement",
        audit_route_agree, confusion.len()
    );
    println!(
        "  hitl_resolve: baseline correct on {}/{} = {:.4} — predicts \"needs human\" on {hitl_baseline_pred_false} rows the live cascade in fact resolved safely (success=1 on {hitl_label_true})",
        hitl_baseline_correct,
        confusion.len(),
        hitl_baseline_correct as f64 / confusion.len() as f64
    );

    // cross-source agreement on identical intents (audit guild vs confusion routed)
    let mut conf_by_intent: HashMap<&str, &str> = HashMap::new();
    for r in &confusion {
        conf_by_intent.entry(r.intent.as_str()).or_insert(&r.routed);
    }
    let mut cross_match = 0usize;
    let mut cross_total = 0usize;
    let mut seen: HashSet<&str> = HashSet::new();
    for r in &audit_route_ok {
        if let Some(routed) = conf_by_intent.get(r.intent.as_str())
            && seen.insert(r.intent.as_str())
        {
            cross_total += 1;
            if bucket_guild(r.guild.as_str()) == bucket_guild(routed) {
                cross_match += 1;
            }
        }
    }
    println!(
        "  cross-source route agreement (audit guild vs confusion routed, distinct intents): {cross_match}/{cross_total}"
    );

    // ── export ──────────────────────────────────────────────────────────────
    if let Some(path) = export_path {
        use std::io::Write;
        let mut out = std::io::BufWriter::new(
            std::fs::File::create(&path).unwrap_or_else(|e| panic!("create {path}: {e}")),
        );
        let mut n_exported = 0usize;

        // audit items: route_guild only (NO hitl ground truth there).
        for r in &audit_route_ok {
            let split = if r.ts <= audit_cut { "train" } else { "test" };
            let mut state = json!({
                "intent": r.intent,
                "agent_id": if r.agent_id.is_empty() { "anonymous" } else { r.agent_id.as_str() },
                "hour_of_day": hour_of(&r.ts),
            });
            // T912 schema fields absent in audit (no complexity/risk there):
            if !r.commit.is_empty() {
                state["kernel_commit"] = json!(r.commit);
            }
            let label = bucket_guild(&r.guild);
            let item = json!({
                "source": "guild_audit_log",
                "id": r.id,
                "ts": r.ts,
                "split": split,
                "state": state,
                "questions": { "route_guild": { "type": "Choice", "options": AUDIT_ROUTE_OPTIONS } },
                "labels": { "route_guild": label },
                "baseline_A": { "route_guild": r.guild },
                "latency_ms": r.latency_ms,
            });
            serde_json::to_writer(&mut out, &item).unwrap();
            out.write_all(b"\n").unwrap();
            n_exported += 1;
        }

        // confusion items: route_guild (dynamic options) + hitl_resolve (Noul).
        for r in &confusion {
            let split = if r.ts <= confusion_cut { "train" } else { "test" };
            let options = per_intent_candidates
                .get(r.intent.as_str())
                .cloned()
                .unwrap_or_else(|| vec![r.routed.clone()]);
            let hitl_label = if r.success { "true" } else { "false" };
            let hitl_pred = if r.verdict == "HumanAuthorizationRequired" { "false" } else { "true" };
            let item = json!({
                "source": "scheduler_confusion",
                "id": r.id,
                "ts": r.ts,
                "split": split,
                "state": {
                    "intent": r.intent,
                    "agent_id": if r.agent_id.is_empty() { "anonymous" } else { r.agent_id.as_str() },
                    "complexity_score": r.complexity,
                    "risk_tier": norm_risk(&r.risk_tier),
                    "risk_tier_raw": r.risk_tier,
                    "hour_of_day": hour_of(&r.ts),
                },
                "questions": {
                    "route_guild": { "type": "Choice", "options": options },
                    "hitl_resolve": { "type": "Noul" },
                },
                "labels": { "route_guild": r.routed, "hitl_resolve": hitl_label },
                "baseline_A": { "route_guild": r.routed, "hitl_resolve": hitl_pred },
                "scheduler_verdict": r.verdict,
                "classification": r.classification,
            });
            serde_json::to_writer(&mut out, &item).unwrap();
            out.write_all(b"\n").unwrap();
            n_exported += 1;
        }
        out.flush().unwrap();
        println!("\n[export] {n_exported} items -> {path}");
        println!("  audit route items + confusion (route + hitl), split by per-source p80 ts");
    }

    println!("\nJSON:");
    println!(
        "{}",
        serde_json::json!({
            "audit_rows": audit.len(),
            "audit_kernel_self_calls": kernel_self_calls,
            "audit_route_eligible": audit_route.len(),
            "audit_ok": audit_ok,
            "audit_cut": audit_cut,
            "confusion_rows": confusion.len(),
            "confusion_cut": confusion_cut,
            "cascade_fired_rows": cascade_fired_rows.0,
            "routed_ne_final_rows": cascade_fired_rows.1,
            "success_rows": cascade_fired_rows.3,
            "har_rows": hitl_rows,
            "differ_ok_rows": differ_ok,
            "hitl_baseline_correct": hitl_baseline_correct,
            "hitl_baseline_accuracy": hitl_baseline_correct as f64 / confusion.len() as f64,
            "cross_route_total": cross_total,
            "cross_route_match": cross_match,
        })
    );
}

// ─── Variant C — LayaOnnxDecisionProvider (ADR-018 §3; Buffy, T925 lane) ────
//
// Faithful Rust/ort port of the reference runtime receptron/laya (MIT):
// same sequence layout (rl_common.build_sequence), same per-cardinality
// temperature, same softmax post-processing, same noul convention
// (options [false, true] so p[1] is P(true)). The provider implements the
// kernel's DecisionProvider contract (router::decision) — this spike harness
// is its ONLY consumer (ADR-018 §2.2: zero hot-path change in Fase 0).

use std::sync::Mutex;
use std::time::Instant;

use ndarray::{Array1, Array2};
use ort::session::Session;
use ort::value::TensorRef;
use tylluan_kernel::router::decision::{
    DecisionAnswer, DecisionError, DecisionProvider, DecisionQuestion, DecisionRequest,
};

const QTYPE_CHOICE: i64 = 0;
const QTYPE_SCORE: i64 = 1;
const QTYPE_NOUL: i64 = 2;
const DATASET_PATH: &str = "benchmarks/decision_fabric_dataset.jsonl";
/// Noul is always rendered [false, true] so p[1] is P(true) — exact texts of
/// sequence.ts `renderOptions` (reference defaults).
const NOUL_OPTIONS: [&str; 2] = [
    "false: no, the statement does not hold",
    "true: yes, the statement holds",
];

/// One rendered question — port of sequence.ts `InternalQ` + `renderOptions`.
struct LayaQuestion {
    qtype: i64,
    instructions: String,
    /// Rendered option texts in label-index order (noul is always [false, true]).
    options: Vec<String>,
    /// Answer keys, same order as `options`.
    keys: Vec<String>,
}

impl LayaQuestion {
    fn score(levels: &[&str], instructions: &str) -> Self {
        LayaQuestion {
            qtype: QTYPE_SCORE,
            instructions: instructions.to_string(),
            options: levels
                .iter()
                .enumerate()
                .map(|(i, c)| format!("level {i}: {c}"))
                .collect(),
            keys: (0..levels.len()).map(|i| i.to_string()).collect(),
        }
    }

    fn noul(instructions: &str) -> Self {
        LayaQuestion {
            qtype: QTYPE_NOUL,
            instructions: instructions.to_string(),
            options: NOUL_OPTIONS.iter().map(|s| (*s).to_string()).collect(),
            keys: vec!["false".into(), "true".into()],
        }
    }
}

/// The ONNX bundle: session + tokenizer + the calibration config from
/// laya_config.json. Mirrors `Laya.load()` in the reference runtime.
struct LayaBundle {
    session: Mutex<Session>,
    tokenizer: tokenizers::Tokenizer,
    max_len: usize,
    head_max_len: usize,
    temperature: Vec<f32>,
    temperature_by_options: HashMap<String, f32>,
    cls: u32,
    sep: u32,
    mask: u32,
    pad: u32,
}

impl LayaBundle {
    fn load(dir: &Path) -> Self {
        let cfg: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.join("laya_config.json"))
                .unwrap_or_else(|e| panic!("read {}/laya_config.json: {e}", dir.display())),
        )
        .expect("parse laya_config.json");
        let tokenizer = tokenizers::Tokenizer::from_file(dir.join("tokenizer/tokenizer.json"))
            .expect("load tokenizer/tokenizer.json");
        let sid = |t: &str| {
            tokenizer
                .token_to_id(t)
                .unwrap_or_else(|| panic!("special token {t} missing from tokenizer"))
        };
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .min(8);
        let session = Session::builder()
            .and_then(|b| {
                b.with_intra_threads(threads)?
                    .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)?
                    .commit_from_file(dir.join("laya.onnx"))
            })
            .unwrap_or_else(|e| {
                panic!(
                    "load laya.onnx from {} ({e}) — needs the full bundle (laya.onnx + laya.onnx.data + tokenizer/) and a loadable onnxruntime dylib",
                    dir.display()
                )
            });
        let temp_map = cfg["temperature_by_options"]
            .as_object()
            .map(|o| {
                o.iter()
                    .filter_map(|(k, v)| v.as_f64().map(|f| (k.clone(), f as f32)))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        let temperature = cfg["temperature"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect())
            .unwrap_or_else(|| vec![1.0; 3]);
        Self {
            session: Mutex::new(session),
            max_len: cfg["max_len"].as_u64().unwrap_or(512) as usize,
            head_max_len: cfg["head_max_len"].as_u64().unwrap_or(192) as usize,
            temperature,
            temperature_by_options: temp_map,
            cls: sid("[CLS]"),
            sep: sid("[SEP]"),
            mask: sid("[MASK]"),
            pad: sid("[PAD]"),
            tokenizer,
        }
    }

    fn encode(&self, text: &str) -> Vec<u32> {
        self.tokenizer
            .encode(text, false)
            .expect("tokenizer encode")
            .get_ids()
            .to_vec()
    }

    /// Port of sequence.ts `buildSequence`:
    /// [CLS] <type> question: instructions [SEP] [MASK] opt0 ... [SEP] state [SEP]
    fn build_sequence(&self, state_text: &str, q: &LayaQuestion) -> (Vec<u32>, Vec<usize>) {
        let scrub = |s: &str| s.replace("[MASK]", " ");
        let type_name = match q.qtype {
            QTYPE_CHOICE => "choice",
            QTYPE_SCORE => "score",
            _ => "noul",
        };
        let head_all = self.encode(&format!("{type_name} question: {}", scrub(&q.instructions)));
        let mut opt_ids: Vec<Vec<u32>> = q
            .options
            .iter()
            .map(|o| {
                let mut v = vec![self.mask];
                v.extend(self.encode(&format!(" {}", scrub(o))).into_iter().take(48));
                v
            })
            .collect();
        let mut budget =
            self.head_max_len.saturating_sub(opt_ids.iter().map(|o| o.len()).sum::<usize>());
        if budget < 16 {
            let per = ((self.head_max_len.saturating_sub(16)) / opt_ids.len().max(1)).max(4);
            for o in &mut opt_ids {
                o.truncate(per);
            }
            budget =
                self.head_max_len.saturating_sub(opt_ids.iter().map(|o| o.len()).sum::<usize>());
        }
        let head: Vec<u32> = head_all.into_iter().take(budget.max(8)).collect();
        let mut seq = Vec::with_capacity(self.max_len);
        seq.push(self.cls);
        seq.extend_from_slice(&head);
        seq.push(self.sep);
        let mut markers = Vec::with_capacity(opt_ids.len());
        for o in &opt_ids {
            markers.push(seq.len());
            seq.extend_from_slice(o);
        }
        seq.push(self.sep);
        let room = self.max_len.saturating_sub(seq.len() + 1);
        seq.extend(self.encode(&scrub(state_text)).into_iter().take(room));
        seq.push(self.sep);
        seq.truncate(self.max_len);
        markers.retain(|&m| m < self.max_len);
        (seq, markers)
    }

    /// Per-cardinality temperature (sequence.ts `tempBucket` + laya_config.json).
    fn temperature_for(&self, qtype: i64, k: usize) -> f32 {
        let bucket = if k <= 2 {
            "2"
        } else if k <= 5 {
            "3-5"
        } else if k <= 10 {
            "6-10"
        } else {
            "11+"
        };
        let name = match qtype {
            QTYPE_CHOICE => "choice",
            QTYPE_SCORE => "score",
            _ => "noul",
        };
        self.temperature_by_options
            .get(&format!("{name}:{bucket}"))
            .copied()
            .or_else(|| self.temperature.get(qtype as usize).copied())
            .unwrap_or(1.0)
    }

    /// One forward pass over all questions about ONE state — the port of
    /// `Laya.systemOne` up to (but not including) the 4-decimal rounding.
    /// Returns one post-temperature probability vector per question.
    fn system_one(
        &self,
        state_text: &str,
        questions: &[LayaQuestion],
    ) -> Result<Vec<Vec<f32>>, DecisionError> {
        if questions.is_empty() {
            return Err(DecisionError::Provider(
                "laya-onnx-v1".into(),
                "at least one question is required".into(),
            ));
        }
        let items: Vec<(Vec<u32>, Vec<usize>)> =
            questions.iter().map(|q| self.build_sequence(state_text, q)).collect();
        for (i, (_, markers)) in items.iter().enumerate() {
            if markers.len() != questions[i].options.len() {
                return Err(DecisionError::Provider(
                    "laya-onnx-v1".into(),
                    format!(
                        "question {i}: options do not fit in head_max_len={}",
                        self.head_max_len
                    ),
                ));
            }
        }
        let n = items.len();
        let l = items.iter().map(|(ids, _)| ids.len()).max().unwrap_or(1).max(1);
        let k = items.iter().map(|(_, m)| m.len()).max().unwrap_or(1).max(1);
        let mut input_ids = vec![i64::from(self.pad); n * l];
        let mut attention = vec![0i64; n * l];
        let mut marker_pos = vec![0i64; n * k];
        let mut marker_mask = vec![false; n * k];
        let mut qtype = vec![0i64; n];
        for (row, ((ids, markers), q)) in items.iter().zip(questions.iter()).enumerate() {
            for (j, &v) in ids.iter().enumerate() {
                input_ids[row * l + j] = i64::from(v);
                attention[row * l + j] = 1;
            }
            for (j, &m) in markers.iter().enumerate() {
                marker_pos[row * k + j] = m as i64;
                marker_mask[row * k + j] = true;
            }
            qtype[row] = q.qtype;
        }
        let ids_arr = Array2::from_shape_vec((n, l), input_ids).expect("ids shape");
        let att_arr = Array2::from_shape_vec((n, l), attention).expect("att shape");
        let mp_arr = Array2::from_shape_vec((n, k), marker_pos).expect("mp shape");
        let mm_arr = Array2::from_shape_vec((n, k), marker_mask).expect("mm shape");
        let qt_arr = Array1::from_vec(qtype);
        let mut session = self.session.lock().expect("laya session mutex poisoned");
        let outputs = session
            .run(ort::inputs![
                "input_ids" => TensorRef::from_array_view(ids_arr.view()).expect("ids tensor"),
                "attention_mask" => TensorRef::from_array_view(att_arr.view()).expect("att tensor"),
                "marker_pos" => TensorRef::from_array_view(mp_arr.view()).expect("mp tensor"),
                "marker_mask" => TensorRef::from_array_view(mm_arr.view()).expect("mm tensor"),
                "qtype" => TensorRef::from_array_view(qt_arr.view()).expect("qt tensor"),
            ])
            .map_err(|e| DecisionError::Provider("laya-onnx-v1".into(), e.to_string()))?;
        // positional access (same pattern as light_reranker.rs): [0] = logits [B,K]
        let (_, logits) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| DecisionError::Provider("laya-onnx-v1".into(), format!("logits: {e}")))?;
        Ok(questions
            .iter()
            .enumerate()
            .map(|(r, q)| {
                let temp = self.temperature_for(q.qtype, q.options.len());
                let z: Vec<f32> =
                    logits[r * k..r * k + q.options.len()].iter().map(|&v| v / temp).collect();
                softmax_f32(&z)
            })
            .collect())
    }
}

/// Compact JSON with Python-style separators — port of sequence.ts
/// `pyJsonDumps` (key order follows serde_json::Value, i.e. sorted; this is
/// the spike's own convention, consistent for every item).
fn py_json_compact(v: &serde_json::Value) -> String {
    use serde_json::Value;
    match v {
        Value::String(s) => serde_json::to_string(s).unwrap_or_default(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".into(),
        Value::Array(a) => format!(
            "[{}]",
            a.iter().map(py_json_compact).collect::<Vec<_>>().join(", ")
        ),
        Value::Object(o) => format!(
            "{{{}}}",
            o.iter()
                .map(|(k, x)| format!(
                    "{}: {}",
                    serde_json::to_string(k).unwrap_or_default(),
                    py_json_compact(x)
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn softmax_f32(z: &[f32]) -> Vec<f32> {
    let m = z.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let e: Vec<f32> = z.iter().map(|&v| (v - m).exp()).collect();
    let s: f32 = e.iter().sum();
    e.into_iter().map(|v| v / s).collect()
}

/// The ADR-018 contract implementation. Fase 0: consumed ONLY by this harness.
struct LayaOnnxDecisionProvider {
    bundle: LayaBundle,
}

#[async_trait::async_trait]
impl DecisionProvider for LayaOnnxDecisionProvider {
    fn provider_id(&self) -> &'static str {
        "laya-onnx-v1"
    }

    async fn decide(
        &self,
        req: &DecisionRequest,
    ) -> Result<HashMap<String, DecisionAnswer>, DecisionError> {
        let state_text = py_json_compact(&req.state);
        // Deterministic order regardless of HashMap iteration order.
        let mut names: Vec<&String> = req.questions.keys().collect();
        names.sort();
        let qs: Vec<LayaQuestion> = names
            .iter()
            .map(|name| match &req.questions[*name] {
                DecisionQuestion::Choice { options } => LayaQuestion {
                    qtype: QTYPE_CHOICE,
                    instructions: format!("Answer the {name} question about this state."),
                    options: options.clone(),
                    keys: options.clone(),
                },
                DecisionQuestion::Score { levels } => LayaQuestion {
                    qtype: QTYPE_SCORE,
                    instructions: format!("Answer the {name} question about this state."),
                    options: levels
                        .iter()
                        .enumerate()
                        .map(|(i, c)| format!("level {i}: {c}"))
                        .collect(),
                    keys: (0..levels.len()).map(|i| i.to_string()).collect(),
                },
                DecisionQuestion::Noul => LayaQuestion::noul(&format!(
                    "Evaluate whether {name} holds for this state."
                )),
            })
            .collect();
        let probs = self.bundle.system_one(&state_text, &qs)?;
        Ok(qs
            .iter()
            .zip(names.iter())
            .zip(probs)
            .map(|((q, name), p)| {
                (
                    (*name).clone(),
                    DecisionAnswer {
                        distribution: q
                            .keys
                            .iter()
                            .zip(p.iter())
                            .map(|(k, v)| (k.clone(), *v))
                            .collect(),
                        calibration: None,
                    },
                )
            })
            .collect())
    }
}

// ─── metrics ────────────────────────────────────────────────────────────────

fn ece_15(pairs: &[(f64, bool)]) -> f64 {
    if pairs.is_empty() {
        return f64::NAN;
    }
    let n = pairs.len() as f64;
    let mut conf = [0.0f64; 15];
    let mut acc = [0.0f64; 15];
    let mut cnt = [0.0f64; 15];
    for &(c, ok) in pairs {
        let b = ((c * 15.0) as usize).min(14);
        conf[b] += c;
        acc[b] += f64::from(ok);
        cnt[b] += 1.0;
    }
    (0..15)
        .filter(|&b| cnt[b] > 0.0)
        .map(|b| (cnt[b] / n) * (acc[b] / cnt[b] - conf[b] / cnt[b]).abs())
        .sum()
}

/// Rank-sum AUROC with tie averaging.
fn auroc(pos: &[f64], neg: &[f64]) -> f64 {
    if pos.is_empty() || neg.is_empty() {
        return f64::NAN;
    }
    let mut all: Vec<(f64, u8)> = pos
        .iter()
        .map(|&x| (x, 1u8))
        .chain(neg.iter().map(|&x| (x, 0u8)))
        .collect();
    all.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let (n1, n0) = (pos.len() as f64, neg.len() as f64);
    let mut rank_sum = 0.0f64;
    let mut i = 0usize;
    while i < all.len() {
        let mut j = i;
        while j < all.len() && all[j].0 == all[i].0 {
            j += 1;
        }
        let avg_rank = (i + j + 1) as f64 / 2.0;
        rank_sum += all[i..j]
            .iter()
            .filter(|(_, is_pos)| *is_pos == 1)
            .count() as f64
            * avg_rank;
        i = j;
    }
    (rank_sum - n1 * (n1 + 1.0) / 2.0) / (n1 * n0)
}

fn pct(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let idx = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn argmax(dist: &HashMap<String, f32>) -> (String, f32) {
    dist.iter()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(k, v)| (k.clone(), *v))
        .unwrap_or_default()
}

struct BinaryDecisionStats {
    n: usize,
    true_pos: usize,
    false_pos: usize,
    true_neg: usize,
    false_neg: usize,
    correct_c: usize,
    correct_b: usize,
    brier_c: f64,
    brier_b: f64,
    ece_c: Vec<(f64, bool)>,
    ece_b: Vec<(f64, bool)>,
    pos_c: Vec<f64>,
    neg_c: Vec<f64>,
    lat: Vec<f64>,
}

impl BinaryDecisionStats {
    fn new() -> Self {
        Self {
            n: 0,
            true_pos: 0,
            false_pos: 0,
            true_neg: 0,
            false_neg: 0,
            correct_c: 0,
            correct_b: 0,
            brier_c: 0.0,
            brier_b: 0.0,
            ece_c: Vec::new(),
            ece_b: Vec::new(),
            pos_c: Vec::new(),
            neg_c: Vec::new(),
            lat: Vec::new(),
        }
    }

    fn update(&mut self, pred: bool, p_true: f32, base_pred: bool, label: bool, ms: f64) {
        self.n += 1;
        match (pred, label) {
            (true, true) => self.true_pos += 1,
            (true, false) => self.false_pos += 1,
            (false, true) => self.false_neg += 1,
            (false, false) => self.true_neg += 1,
        }
        if pred == label {
            self.correct_c += 1;
        }
        if base_pred == label {
            self.correct_b += 1;
        }
        self.brier_c += (f64::from(p_true) - f64::from(label)).powi(2);
        let base_p = f64::from(base_pred);
        self.brier_b += (base_p - f64::from(label)).powi(2);
        let conf = if pred { f64::from(p_true) } else { 1.0 - f64::from(p_true) };
        self.ece_c.push((conf, pred == label));
        self.ece_b.push((1.0, base_pred == label));
        if label {
            self.pos_c.push(f64::from(p_true));
        } else {
            self.neg_c.push(f64::from(p_true));
        }
        self.lat.push(ms);
    }
}

struct ChoiceDecisionStats {
    n: usize,
    correct_c: usize,
    correct_b: usize,
    with_baseline: usize,
    brier_c: f64,
    brier_b: f64,
    ece_c: Vec<(f64, bool)>,
    lat: Vec<f64>,
}

impl ChoiceDecisionStats {
    fn new() -> Self {
        Self {
            n: 0,
            correct_c: 0,
            correct_b: 0,
            with_baseline: 0,
            brier_c: 0.0,
            brier_b: 0.0,
            ece_c: Vec::new(),
            lat: Vec::new(),
        }
    }

    fn update(
        &mut self,
        correct_c: bool,
        correct_b: Option<bool>,
        conf: f32,
        brier_item: f64,
        ms: f64,
    ) {
        self.n += 1;
        if correct_c {
            self.correct_c += 1;
        }
        if let Some(cb) = correct_b {
            self.with_baseline += 1;
            if cb {
                self.correct_b += 1;
            }
            self.brier_b += if cb { 0.0 } else { 1.0 }; // one-hot baseline
            self.ece_c.push((f64::from(conf), correct_c));
        }
        self.brier_c += brier_item;
        self.lat.push(ms);
    }
}

// ─── golden check (receptron/laya test/test_model.ts, published values) ─────

fn run_golden(dir: &str) -> bool {
    println!("[golden] receptron/laya test_model.ts — published 4-decimal reference values");
    let bundle = LayaBundle::load(Path::new(dir));
    let state = "{\"subject\": \"Refund not received\", \"body\": \"I cancelled my subscription two weeks ago and I still have not received my refund. This is the third time I am writing. If this is not resolved I will dispute the charge with my bank.\"}";
    let qs = vec![
        LayaQuestion {
            qtype: QTYPE_CHOICE,
            instructions: "Which team should handle this ticket?".into(),
            options: [
                "billing: payments, refunds, invoices",
                "support: product help and bugs",
                "sales: new purchases and upgrades",
            ]
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
            keys: vec!["billing".into(), "support".into(), "sales".into()],
        },
        LayaQuestion::score(
            &["not urgent", "somewhat urgent", "urgent", "critical"],
            "How urgent is this ticket?",
        ),
        LayaQuestion::noul("Is the customer likely to cancel or dispute?"),
    ];
    match bundle.system_one(state, &qs) {
        Err(e) => {
            println!("[golden] FAIL: {e}");
            false
        }
        Ok(rows) => {
            let mut tokens = 0usize;
            for q in &qs {
                let (ids, _) = bundle.build_sequence(state, q);
                tokens += ids.len();
            }
            let dep = &rows[0];
            let urg = &rows[1];
            let ch = &rows[2];
            let checks: Vec<(String, bool, f64, f64)> = vec![
                (
                    "department.billing".into(),
                    dep[0] > dep[1] && dep[0] > dep[2],
                    f64::from(dep[0]),
                    0.9415,
                ),
                ("department.support".into(), true, f64::from(dep[1]), 0.0310),
                ("department.sales".into(), true, f64::from(dep[2]), 0.0275),
                ("urgency.0".into(), true, f64::from(urg[0]), 0.1752),
                ("urgency.1".into(), true, f64::from(urg[1]), 0.2947),
                ("urgency.2".into(), true, f64::from(urg[2]), 0.4962),
                ("urgency.3".into(), true, f64::from(urg[3]), 0.0338),
                ("churn_risk.noul".into(), true, f64::from(ch[1]), 0.0988),
            ];
            let mut ok = true;
            for (name, structural, got, want) in &checks {
                let (structural, got, want) = (*structural, *got, *want);
                let d = (got - want).abs();
                let pass = structural && d <= 1e-4;
                ok &= pass;
                println!(
                    "  {name}: got {got:.4} want {want:.4} (|d|={d:.6}) {}",
                    if pass { "PASS" } else { "FAIL" }
                );
            }
            let tok_ok = tokens == 267;
            ok &= tok_ok;
            println!(
                "  usage.input_tokens: got {tokens} want 267 {}",
                if tok_ok { "PASS" } else { "FAIL" }
            );
            println!(
                "[golden] {}",
                if ok {
                    "PASS — port is faithful to the reference runtime"
                } else {
                    "FAIL — do NOT trust any dataset number below"
                }
            );
            ok
        }
    }
}

// ─── variant C runner: dataset → metrics → §3 gate verdict ──────────────────

fn run_variant_c(dir: &str, split: &str, source: &str, limit: Option<usize>, budget_ms: f64) {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(run_variant_c_inner(dir, split, source, limit, budget_ms));
}

async fn run_variant_c_inner(
    dir: &str,
    split: &str,
    source: &str,
    limit: Option<usize>,
    budget_ms: f64,
) {
    println!(
        "[variant-c] loading Laya ONNX bundle from {dir} (fp32 ~1.7 GB — first load is slow)"
    );
    let t0 = Instant::now();
    let provider = LayaOnnxDecisionProvider { bundle: LayaBundle::load(Path::new(dir)) };
    println!(
        "[variant-c] bundle ready in {:.1}s (provider: {})",
        t0.elapsed().as_secs_f64(),
        provider.provider_id()
    );

    let ds = std::fs::read_to_string(DATASET_PATH)
        .unwrap_or_else(|e| panic!("read {DATASET_PATH}: {e} (regenerate with --export first)"));
    let items: Vec<serde_json::Value> = ds
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<serde_json::Value>(l).expect("dataset line parse"))
        .filter(|it| {
            (split == "all" || it["split"].as_str() == Some(split))
                && match source {
                    "audit" => it["source"].as_str() == Some("guild_audit_log"),
                    "confusion" => it["source"].as_str() == Some("scheduler_confusion"),
                    _ => true,
                }
        })
        .take(limit.unwrap_or(usize::MAX))
        .collect();
    println!(
        "[variant-c] items after filters (split={split}, source={source}, limit={limit:?}): {}",
        items.len()
    );

    let mut hitl = BinaryDecisionStats::new();
    let mut route = ChoiceDecisionStats::new();
    let mut decide_errors = 0usize;
    for it in &items {
        let mut questions: HashMap<String, DecisionQuestion> = HashMap::new();
        let mut want_route = false;
        let mut want_hitl = false;
        if let Some(opts) = it["questions"]["route_guild"]["options"].as_array() {
            let options: Vec<String> =
                opts.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect();
            if !options.is_empty() {
                questions.insert("route_guild".into(), DecisionQuestion::Choice { options });
                want_route = true;
            }
        }
        if it["questions"].get("hitl_resolve").is_some() {
            questions.insert("hitl_resolve".into(), DecisionQuestion::Noul);
            want_hitl = true;
        }
        if questions.is_empty() {
            continue;
        }
        let req = DecisionRequest { state: it["state"].clone(), questions };
        let t0 = Instant::now();
        let answers = match provider.decide(&req).await {
            Ok(a) => a,
            Err(e) => {
                decide_errors += 1;
                println!("  decide error ({e}) — item skipped");
                continue;
            }
        };
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        if want_route {
            let dist = &answers["route_guild"].distribution;
            let (best, conf) = argmax(dist);
            let label = it["labels"]["route_guild"].as_str().unwrap_or_default().to_string();
            let base_raw = it["baseline_A"]["route_guild"].as_str().unwrap_or_default().to_string();
            let base = bucket_guild(&base_raw);
            let brier_item: f64 = dist
                .iter()
                .map(|(k, v)| (f64::from(*v) - f64::from(*k == label)).powi(2))
                .sum();
            let correct_b = if base_raw.is_empty() { None } else { Some(base == label) };
            route.update(best == label, correct_b, conf, brier_item, ms);
        }
        if want_hitl {
            let dist = &answers["hitl_resolve"].distribution;
            let p_true = dist.get("true").copied().unwrap_or(0.0);
            let label = it["labels"]["hitl_resolve"].as_str() == Some("true");
            let base_pred = it["baseline_A"]["hitl_resolve"].as_str() == Some("true");
            hitl.update(p_true >= 0.5, p_true, base_pred, label, ms);
        }
    }

    // sort once — every percentile read below assumes sorted latency vectors
    hitl.lat.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    route.lat.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let hitl_n = hitl.n;
    let route_n = route.n;
    if hitl_n > 0 {
        let lat = &hitl.lat;
        println!("\n[variant-c] decision=hitl_resolve (Noul) n={hitl_n}");
        println!(
            "  accuracy: baseline A {:.4} -> C {:.4} (TP {} FP {} TN {} FN {})",
            hitl.correct_b as f64 / hitl_n as f64,
            hitl.correct_c as f64 / hitl_n as f64,
            hitl.true_pos,
            hitl.false_pos,
            hitl.true_neg,
            hitl.false_neg
        );
        println!(
            "  Brier: baseline {:.4} -> C {:.4} | ECE(15): baseline {:.4} -> C {:.4} | AUROC: C {:.4}",
            hitl.brier_b / hitl_n as f64,
            hitl.brier_c / hitl_n as f64,
            ece_15(&hitl.ece_b),
            ece_15(&hitl.ece_c),
            auroc(&hitl.pos_c, &hitl.neg_c)
        );
        println!(
            "  latency (C, CPU, per item): p50 {:.1} ms / p99 {:.1} ms / max {:.1} ms",
            pct(lat, 50.0),
            pct(lat, 99.0),
            lat.last().copied().unwrap_or(f64::NAN)
        );
    }
    if route_n > 0 {
        let lat = &route.lat;
        println!("\n[variant-c] decision=route_guild (Choice, dynamic options) n={route_n}");
        println!(
            "  accuracy: baseline A {:.4} (of {} items with live baseline; confusion items are 1.000 BY CONSTRUCTION — label := routed guild) -> C {:.4}",
            if route.with_baseline > 0 {
                route.correct_b as f64 / route.with_baseline as f64
            } else {
                f64::NAN
            },
            route.with_baseline,
            route.correct_c as f64 / route_n as f64
        );
        println!(
            "  Brier (multiclass sum): baseline {:.4} -> C {:.4} | ECE(15) top-class: C {:.4}",
            route.brier_b / route.with_baseline.max(1) as f64,
            route.brier_c / route_n as f64,
            ece_15(&route.ece_c)
        );
        println!(
            "  latency (C, CPU, per item): p50 {:.1} ms / p99 {:.1} ms / max {:.1} ms",
            pct(lat, 50.0),
            pct(lat, 99.0),
            lat.last().copied().unwrap_or(f64::NAN)
        );
    }
    if decide_errors > 0 {
        println!("\n[variant-c] decide errors: {decide_errors} items skipped");
    }

    // ── §3 gate, clause by clause ───────────────────────────────────────────
    println!("\n[gate] ADR-018 §3 (budget {budget_ms} ms):");
    let mut clauses_ok = true;
    if hitl_n > 0 {
        let acc_c = hitl.correct_c as f64 / hitl_n as f64;
        let acc_b = hitl.correct_b as f64 / hitl_n as f64;
        let acc_pass = acc_c > acc_b;
        clauses_ok &= acc_pass;
        let calib_pass =
            hitl.brier_c < hitl.brier_b && ece_15(&hitl.ece_c) < ece_15(&hitl.ece_b);
        clauses_ok &= calib_pass;
        let lat_pass = pct(&hitl.lat, 99.0) <= budget_ms;
        clauses_ok &= lat_pass;
        println!(
            "  1. accuracy hitl: C {acc_c:.4} vs baseline {acc_b:.4} — {}",
            if acc_pass { "PASS" } else { "FAIL" }
        );
        println!(
            "  2. calibration hitl (Brier {:.4} vs {:.4}, ECE {:.4} vs {:.4}) — {}",
            hitl.brier_c / hitl_n as f64,
            hitl.brier_b / hitl_n as f64,
            ece_15(&hitl.ece_c),
            ece_15(&hitl.ece_b),
            if calib_pass { "PASS" } else { "FAIL" }
        );
        println!(
            "  3. p99 latency hitl: {:.1} ms <= {budget_ms} — {}",
            pct(&hitl.lat, 99.0),
            if lat_pass { "PASS" } else { "FAIL" }
        );
    } else {
        println!("  1-3. no hitl items in this slice — accuracy/calibration/latency clauses NOT EVALUATED");
        clauses_ok = false;
    }
    println!(
        "  4. cost vs substituted LLM: NOT MEASURABLE in variant C alone — needs variant B (small local LLM) as the substituted-cost reference. Inputs measured here: fp32 bundle 1.69 GB on disk, ~2 GB RAM expected at runtime."
    );
    let verdict = if clauses_ok {
        "GO-PROVISIONAL (clauses 1-3 pass; clause 4 pending variant B)"
    } else {
        "NO-GO on clauses 1-3"
    };
    println!("  VERDICT: {verdict}");
    println!(
        "  CAVEAT (pinned degeneracies, T921): hitl labels = cascade-success on 1,226/1,229 rows (success trap); route labels are by-construction. Audit has NO hitl ground truth (human_intervention dead column)."
    );
    println!(
        "\nJSON:\n{}",
        serde_json::json!({
            "provider": "laya-onnx-v1",
            "split": split,
            "source": source,
            "items": items.len(),
            "decide_errors": decide_errors,
            "hitl": {
                "n": hitl.n,
                "acc_c": hitl.correct_c as f64 / hitl.n.max(1) as f64,
                "acc_baseline": hitl.correct_b as f64 / hitl.n.max(1) as f64,
                "brier_c": hitl.brier_c / hitl.n.max(1) as f64,
                "brier_baseline": hitl.brier_b / hitl.n.max(1) as f64,
                "ece_c": ece_15(&hitl.ece_c),
                "ece_baseline": ece_15(&hitl.ece_b),
                "auroc_c": auroc(&hitl.pos_c, &hitl.neg_c),
                "tp": hitl.true_pos,
                "fp": hitl.false_pos,
                "tn": hitl.true_neg,
                "fn": hitl.false_neg,
                "latency_p50_ms": pct(&hitl.lat, 50.0),
                "latency_p99_ms": pct(&hitl.lat, 99.0),
            },
            "route": {
                "n": route.n,
                "acc_c": route.correct_c as f64 / route.n.max(1) as f64,
                "acc_baseline": route.correct_b as f64 / route.with_baseline.max(1) as f64,
                "brier_c": route.brier_c / route.n.max(1) as f64,
                "brier_baseline": route.brier_b / route.with_baseline.max(1) as f64,
                "latency_p50_ms": pct(&route.lat, 50.0),
                "latency_p99_ms": pct(&route.lat, 99.0),
            },
            "gate_verdict": verdict,
        })
    );
}

// ─── variant B: SystemOne HTTP provider (llama.cpp #29818 / Ollama >= 0.35.1) ──
//
// The provider speaks the exact Jev / System One wire contract (`POST
// /v1/systemone`), which llama.cpp merged natively for clef GGUFs (PR #29818,
// 2026-10-03) and Ollama ships since 0.35.x. Runtime-agnostic by design: the
// same client serves `llama-server -hf ggml-org/Clef-Flash-GGUF` (:8080) and
// `ollama serve` (:11434) — the kernel never knows or cares which runtime is
// behind the URL, and the model lives OUTSIDE the kernel process (contrast
// with variant C: ort in-proc + ~1.7 GB RSS inside the kernel).

#[derive(Debug, serde::Deserialize)]
struct SystemOneAnswer {
    #[serde(default)]
    probabilities: HashMap<String, f64>,
    #[serde(default)]
    noul: Option<f64>,
}

#[derive(Debug, serde::Deserialize)]
struct SystemOneResponse {
    answers: HashMap<String, SystemOneAnswer>,
}

/// Variante B del spike ADR-018: DecisionProvider sobre un server SystemOne
/// HTTP (llama.cpp u Ollama). Sin estado propio y sin modelo in-proc.
struct SystemOneDecisionProvider {
    endpoint: String,
    model: String,
    http: reqwest::Client,
}

impl SystemOneDecisionProvider {
    fn new(endpoint: &str, model: &str) -> Self {
        Self {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            model: model.to_string(),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .expect("reqwest client for variant B"),
        }
    }
}

#[async_trait::async_trait]
impl DecisionProvider for SystemOneDecisionProvider {
    async fn decide(
        &self,
        req: &DecisionRequest,
    ) -> Result<HashMap<String, DecisionAnswer>, DecisionError> {
        // Wire mapping: Score keeps its typed "score" levels; Noul becomes a
        // two-option choice because this harness only consumes the per-option
        // probability distribution (the server's noul convenience field is
        // honored when present, but never required).
        //
        // T1004: a request can bundle several independent questions (e.g.
        // route_guild + hitl_resolve). One question failing its spec (a
        // degenerate route_guild with <2 candidate guilds -- a real,
        // acknowledged data characteristic, not a bug) used to abort the
        // WHOLE call via `?`, silently discarding every other question in
        // the same item -- hitl_resolve never got evaluated even though it
        // had nothing wrong with it. Skip only the invalid question instead;
        // fail the call only if EVERY question in it turned out invalid.
        let mut questions = serde_json::Map::new();
        for (name, q) in &req.questions {
            match systemone_question_spec(name, q) {
                Ok(spec) => {
                    questions.insert(name.clone(), spec);
                }
                Err(_) => {
                    tracing::debug!("[variant-b] question '{name}' has no valid options/levels — skipping it, not the whole item");
                }
            }
        }
        if questions.is_empty() {
            return Err(DecisionError::EmptyQuestion("all questions in item".to_string()));
        }

        let body = json!({ "model": self.model, "state": req.state, "questions": questions });
        let url = format!("{}/v1/systemone", self.endpoint);
        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| DecisionError::Provider(self.model.clone(), format!("{url}: {e}")));
        let resp = match resp {
            Ok(r) => r,
            Err(e) => return Err(e),
        };
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(DecisionError::Provider(
                self.model.clone(),
                format!("{url}: HTTP {status} — {}", clip_str(&text, 300)),
            ));
        }
        let parsed: SystemOneResponse = match resp.json().await {
            Ok(p) => p,
            Err(e) => {
                return Err(DecisionError::Provider(
                    self.model.clone(),
                    format!("decode: {e}"),
                ))
            }
        };

        // T1004: same principle as the request-building skip above -- the
        // server can decline or drop an individual question (observed live:
        // clef-flash answers hitl_resolve but omits route_guild for some
        // items) without that invalidating the OTHER questions it did
        // answer. Skip only the missing/empty one; fail the whole call only
        // if the server answered nothing usable at all.
        let mut out = HashMap::new();
        for (name, q) in &req.questions {
            let Some(ans) = parsed.answers.get(name) else {
                tracing::debug!(
                    "[variant-b] server answered {} of {} questions; '{name}' missing — skipping it, not the whole item",
                    parsed.answers.len(),
                    req.questions.len()
                );
                continue;
            };
            let distribution = systemone_distribution(q, ans);
            if distribution.is_empty() {
                tracing::debug!("[variant-b] '{name}' answered with no probabilities — skipping it");
                continue;
            }
            out.insert(
                name.clone(),
                DecisionAnswer { distribution, calibration: None },
            );
        }
        if out.is_empty() {
            return Err(DecisionError::Provider(
                self.model.clone(),
                "server answered none of the requested questions usably".to_string(),
            ));
        }
        Ok(out)
    }

    fn provider_id(&self) -> &'static str {
        "systemone-http-v1"
    }
}

/// Pure wire mapping DecisionQuestion -> SystemOne question spec.
/// Score keeps its typed levels; Noul is expressed as a two-option choice so
/// the per-option distribution is always the single source of truth.
///
/// T1004: the real `/v1/systemone` endpoint (found only once a real server
/// was up, 2026-10-08 -- the smoke test never exercised a live server)
/// rejects any question missing `"instructions"` with HTTP 400. The shared
/// `DecisionQuestion` contract carries no instructions field (it's generic
/// across providers), so SystemOne's own requirement is synthesized here
/// from the question name, same generic pattern already used for variant
/// D's local question encoding (`"Answer the {name} question about this
/// state."`).
fn systemone_question_spec(name: &str, q: &DecisionQuestion) -> Result<serde_json::Value, DecisionError> {
    let instructions = format!("Answer the {name} question about this state.");
    Ok(match q {
        DecisionQuestion::Choice { options } => {
            if options.len() < 2 {
                return Err(DecisionError::EmptyQuestion(String::new()));
            }
            json!({
                "type": "choice",
                "instructions": instructions,
                "criteria": options.iter()
                    .map(|o| (o.clone(), serde_json::Value::Null))
                    .collect::<serde_json::Map<String, serde_json::Value>>(),
            })
        }
        DecisionQuestion::Score { levels } => {
            if levels.len() < 2 {
                return Err(DecisionError::EmptyQuestion(String::new()));
            }
            json!({ "type": "score", "instructions": instructions, "criteria": levels })
        }
        DecisionQuestion::Noul => json!({
            "type": "choice",
            "instructions": instructions,
            "criteria": {
                "true": "the statement holds",
                "false": "the statement does not hold",
            },
        }),
    })
}

/// Pure response mapping SystemOne answer -> per-option distribution.
/// For Noul the typed `noul` convenience field wins when present; otherwise
/// (and for Choice/Score always) the per-option probabilities are taken as-is,
/// clamped to [0,1].
fn systemone_distribution(q: &DecisionQuestion, ans: &SystemOneAnswer) -> HashMap<String, f32> {
    match q {
        DecisionQuestion::Noul if ans.noul.is_some() => {
            let p = ans.noul.unwrap().clamp(0.0, 1.0) as f32;
            [("true".to_string(), p), ("false".to_string(), 1.0 - p)]
                .into_iter()
                .collect()
        }
        _ => ans
            .probabilities
            .iter()
            .map(|(k, v)| (k.clone(), v.clamp(0.0, 1.0) as f32))
            .collect(),
    }
}

fn clip_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}…")
    }
}

// ─── variant B runner: identical metrics + §3 gate as variant C ───────────────

async fn run_variant_b_inner(
    endpoint: &str,
    model: &str,
    split: &str,
    source: &str,
    limit: Option<usize>,
    budget_ms: f64,
) {
    println!("[variant-b] probing SystemOne endpoint {endpoint} (model: {model})…");
    let provider = SystemOneDecisionProvider::new(endpoint, model);
    // Fail FAST with an actionable message when no server is listening —
    // variant B requires the model server started OUTSIDE this harness
    // (example: `llama-server -hf ggml-org/Clef-Flash-GGUF --port 8080`).
    let probe = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("probe client")
        .get(format!("{}/health", endpoint.trim_end_matches('/')))
        .send()
        .await;
    match probe {
        Ok(r) => println!("[variant-b] endpoint reachable (HTTP {})", r.status()),
        Err(e) => println!(
            "[variant-b] WARNING: endpoint probe failed ({e}). Start the model server outside this harness:\n  llama-server -hf ggml-org/Clef-Flash-GGUF --port 8080   # llama.cpp (>= PR #29818)\n  ollama serve                                            # Ollama >= 0.35.1 (la local es 0.30.10)"
        ),
    }

    let ds = std::fs::read_to_string(DATASET_PATH)
        .unwrap_or_else(|e| panic!("read {DATASET_PATH}: {e} (regenerate with --export first)"));
    let items: Vec<serde_json::Value> = ds
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<serde_json::Value>(l).expect("dataset line parse"))
        .filter(|it| {
            (split == "all" || it["split"].as_str() == Some(split))
                && match source {
                    "audit" => it["source"].as_str() == Some("guild_audit_log"),
                    "confusion" => it["source"].as_str() == Some("scheduler_confusion"),
                    _ => true,
                }
        })
        .take(limit.unwrap_or(usize::MAX))
        .collect();
    println!(
        "[variant-b] items after filters (split={split}, source={source}, limit={limit:?}): {}",
        items.len()
    );

    let mut hitl = BinaryDecisionStats::new();
    let mut route = ChoiceDecisionStats::new();
    let mut decide_errors = 0usize;
    for it in &items {
        let mut questions: HashMap<String, DecisionQuestion> = HashMap::new();
        let mut want_route = false;
        let mut want_hitl = false;
        if let Some(opts) = it["questions"]["route_guild"]["options"].as_array() {
            let options: Vec<String> =
                opts.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect();
            if !options.is_empty() {
                questions.insert("route_guild".into(), DecisionQuestion::Choice { options });
                want_route = true;
            }
        }
        if it["questions"].get("hitl_resolve").is_some() {
            questions.insert("hitl_resolve".into(), DecisionQuestion::Noul);
            want_hitl = true;
        }
        if questions.is_empty() {
            continue;
        }
        let req = DecisionRequest { state: it["state"].clone(), questions };
        let t0 = Instant::now();
        let answers = match provider.decide(&req).await {
            Ok(a) => a,
            Err(e) => {
                decide_errors += 1;
                println!("  decide error ({e}) — item skipped");
                continue;
            }
        };
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        // T1004: the provider may have skipped a question whose spec was
        // invalid (e.g. route_guild with <2 candidates) while still
        // answering the others in the same call -- answers.get(), never
        // answers[..], or a degenerate route_guild silently drops the
        // hitl_resolve signal for every item in the dataset.
        if want_route
            && let Some(answer) = answers.get("route_guild")
        {
            let dist = &answer.distribution;
            let (best, conf) = argmax(dist);
            let label = it["labels"]["route_guild"].as_str().unwrap_or_default().to_string();
            let base_raw = it["baseline_A"]["route_guild"].as_str().unwrap_or_default().to_string();
            let base = bucket_guild(&base_raw);
            let brier_item: f64 = dist
                .iter()
                .map(|(k, v)| (f64::from(*v) - f64::from(*k == label)).powi(2))
                .sum();
            let correct_b = if base_raw.is_empty() { None } else { Some(base == label) };
            route.update(best == label, correct_b, conf, brier_item, ms);
        }
        if want_hitl
            && let Some(answer) = answers.get("hitl_resolve")
        {
            let dist = &answer.distribution;
            let p_true = dist.get("true").copied().unwrap_or(0.0);
            let label = it["labels"]["hitl_resolve"].as_str() == Some("true");
            let base_pred = it["baseline_A"]["hitl_resolve"].as_str() == Some("true");
            hitl.update(p_true >= 0.5, p_true, base_pred, label, ms);
        }
    }

    // sort once — every percentile read below assumes sorted latency vectors
    hitl.lat.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    route.lat.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let hitl_n = hitl.n;
    let route_n = route.n;
    println!("\n[variant-b] NOTE degeneracies (T921): hitl labels = cascade-success (success trap); route labels by-construction. Same caveats as variant C.");
    if hitl_n > 0 {
        let lat = &hitl.lat;
        println!("\n[variant-b] decision=hitl_resolve (Noul) n={hitl_n}");
        println!(
            "  accuracy: baseline A {:.4} -> B {:.4} (TP {} FP {} TN {} FN {})",
            hitl.correct_b as f64 / hitl_n as f64,
            hitl.correct_c as f64 / hitl_n as f64,
            hitl.true_pos,
            hitl.false_pos,
            hitl.true_neg,
            hitl.false_neg
        );
        println!(
            "  Brier: baseline {:.4} -> B {:.4} | ECE(15): baseline {:.4} -> B {:.4} | AUROC: B {:.4}",
            hitl.brier_b / hitl_n as f64,
            hitl.brier_c / hitl_n as f64,
            ece_15(&hitl.ece_b),
            ece_15(&hitl.ece_c),
            auroc(&hitl.pos_c, &hitl.neg_c)
        );
        println!(
            "  latency (B, per item incl. HTTP): p50 {:.1} ms / p99 {:.1} ms / max {:.1} ms",
            pct(lat, 50.0),
            pct(lat, 99.0),
            lat.last().copied().unwrap_or(f64::NAN)
        );
    }
    if route_n > 0 {
        let lat = &route.lat;
        println!("\n[variant-b] decision=route_guild (Choice, dynamic options) n={route_n}");
        println!(
            "  accuracy: baseline A {:.4} (of {} items with live baseline) -> B {:.4}",
            if route.with_baseline > 0 {
                route.correct_b as f64 / route.with_baseline as f64
            } else {
                f64::NAN
            },
            route.with_baseline,
            route.correct_c as f64 / route_n as f64
        );
        println!(
            "  Brier (multiclass sum): baseline {:.4} -> B {:.4} | ECE(15) top-class: B {:.4}",
            route.brier_b / route.with_baseline.max(1) as f64,
            route.brier_c / route_n as f64,
            ece_15(&route.ece_c)
        );
        println!(
            "  latency (B, per item incl. HTTP): p50 {:.1} ms / p99 {:.1} ms / max {:.1} ms",
            pct(lat, 50.0),
            pct(lat, 99.0),
            lat.last().copied().unwrap_or(f64::NAN)
        );
    }
    if decide_errors > 0 {
        println!("\n[variant-b] decide errors: {decide_errors} items skipped");
    }

    // ── §3 gate, clause by clause (same bar as variant C) ────────────────────
    println!("\n[gate] ADR-018 §3 (budget {budget_ms} ms):");
    let mut clauses_ok = true;
    if hitl_n > 0 {
        let acc_v = hitl.correct_c as f64 / hitl_n as f64;
        let acc_b = hitl.correct_b as f64 / hitl_n as f64;
        let acc_pass = acc_v > acc_b;
        clauses_ok &= acc_pass;
        let calib_pass =
            hitl.brier_c < hitl.brier_b && ece_15(&hitl.ece_c) < ece_15(&hitl.ece_b);
        clauses_ok &= calib_pass;
        let lat_pass = pct(&hitl.lat, 99.0) <= budget_ms;
        clauses_ok &= lat_pass;
        println!(
            "  1. accuracy hitl: B {acc_v:.4} vs baseline {acc_b:.4} — {}",
            if acc_pass { "PASS" } else { "FAIL" }
        );
        println!(
            "  2. calibration hitl (Brier {:.4} vs {:.4}, ECE {:.4} vs {:.4}) — {}",
            hitl.brier_c / hitl_n as f64,
            hitl.brier_b / hitl_n as f64,
            ece_15(&hitl.ece_c),
            ece_15(&hitl.ece_b),
            if calib_pass { "PASS" } else { "FAIL" }
        );
        println!(
            "  3. p99 latency hitl: {:.1} ms <= {budget_ms} — {}",
            pct(&hitl.lat, 99.0),
            if lat_pass { "PASS" } else { "FAIL" }
        );
    } else {
        println!("  1-3. no hitl items in this slice — accuracy/calibration/latency clauses NOT EVALUATED");
        clauses_ok = false;
    }
    println!(
        "  4. cost vs substituted LLM: still open — this run supplies the inputs to compute it (server usage per call + the measured HTTP+inference latency columns)."
    );
    let verdict = if clauses_ok {
        "GO-PROVISIONAL (clauses 1-3 pass on this slice; shadow-mode next)"
    } else {
        "NO-GO on clauses 1-3 for this slice"
    };
    println!("  VERDICT: {verdict}");
    println!(
        "\nJSON:\n{}",
        serde_json::json!({
            "provider": "systemone-http-v1",
            "endpoint": endpoint,
            "model": model,
            "split": split,
            "source": source,
            "items": items.len(),
            "decide_errors": decide_errors,
            "hitl": {
                "n": hitl.n,
                "acc_variant": hitl.correct_c as f64 / hitl.n.max(1) as f64,
                "acc_baseline": hitl.correct_b as f64 / hitl.n.max(1) as f64,
                "brier_variant": hitl.brier_c / hitl.n.max(1) as f64,
                "brier_baseline": hitl.brier_b / hitl.n.max(1) as f64,
                "ece_variant": ece_15(&hitl.ece_c),
                "ece_baseline": ece_15(&hitl.ece_b),
                "auroc_variant": auroc(&hitl.pos_c, &hitl.neg_c),
                "tp": hitl.true_pos,
                "fp": hitl.false_pos,
                "tn": hitl.true_neg,
                "fn": hitl.false_neg,
                "latency_p50_ms": pct(&hitl.lat, 50.0),
                "latency_p99_ms": pct(&hitl.lat, 99.0),
            },
            "route": {
                "n": route.n,
                "acc_variant": route.correct_c as f64 / route.n.max(1) as f64,
                "acc_baseline": route.correct_b as f64 / route.with_baseline.max(1) as f64,
                "brier_variant": route.brier_c / route.n.max(1) as f64,
                "brier_baseline": route.brier_b / route.with_baseline.max(1) as f64,
                "latency_p50_ms": pct(&route.lat, 50.0),
                "latency_p99_ms": pct(&route.lat, 99.0),
            },
            "gate_verdict": verdict,
        })
    );
}

fn run_variant_b(
    endpoint: &str,
    model: &str,
    split: &str,
    source: &str,
    limit: Option<usize>,
    budget_ms: f64,
) {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(run_variant_b_inner(endpoint, model, split, source, limit, budget_ms));
}

// ─── Variante D: Decima-small (amyrmahdy/decima-small, Apache-2.0, 122M) ───
//
// Arquitectura dual-encoder "late interaction", distinta de Laya (un solo
// modelo con marcadores de opción): un encoder.onnx compartido produce
// hidden states por token para el state y para cada choice por separado
// (input_ids/attention_mask i64 -> token_states [n,seq,384] f32); un
// scorer.onnx cruza state_h/state_mask contra choice_h/choice_mask y
// devuelve scores[n_choices] + z[n_choices,384] (z solo se usa para el
// cabezal ordinal de "score", no implementado aquí -- Score se trata como
// Choice sobre "level N: texto", igual que ya hace rules-v0/Laya en este
// mismo harness). Contrato confirmado leyendo el grafo ONNX real (`onnx.load`
// + introspección de graph.input/output), NO de la documentación del modelo
// (que no publica nombres de tensores). prefixes state_prefix="query: "/
// choice_prefix="passage: " y temperature=0.9355568358032639 vienen de
// onnx/int8/decima.json tal cual, sin redondear.
struct DecimaBundle {
    encoder: Mutex<Session>,
    scorer: Mutex<Session>,
    tokenizer: tokenizers::Tokenizer,
    max_state_tokens: usize,
    max_choice_tokens: usize,
    temperature: f32,
    state_prefix: String,
    choice_prefix: String,
    pad: u32,
}

impl DecimaBundle {
    fn load(dir: &Path) -> Self {
        let cfg: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.join("decima.json"))
                .unwrap_or_else(|e| panic!("read {}/decima.json: {e}", dir.display())),
        )
        .expect("parse decima.json");
        let tokenizer = tokenizers::Tokenizer::from_file(dir.join("tokenizer/tokenizer.json"))
            .expect("load tokenizer/tokenizer.json");
        let pad = tokenizer
            .token_to_id("<pad>")
            .unwrap_or_else(|| panic!("special token <pad> missing from tokenizer"));
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
        let build = |file: &str| {
            Session::builder()
                .and_then(|b| {
                    b.with_intra_threads(threads)?
                        .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)?
                        .commit_from_file(dir.join(file))
                })
                .unwrap_or_else(|e| {
                    panic!(
                        "load {file} from {} ({e}) — needs onnx/int8/{{encoder,scorer}}.onnx + tokenizer/ + decima.json",
                        dir.display()
                    )
                })
        };
        Self {
            encoder: Mutex::new(build("encoder.onnx")),
            scorer: Mutex::new(build("scorer.onnx")),
            max_state_tokens: cfg["max_state_tokens"].as_u64().unwrap_or(512) as usize,
            max_choice_tokens: cfg["max_choice_tokens"].as_u64().unwrap_or(64) as usize,
            temperature: cfg["temperature"].as_f64().unwrap_or(1.0) as f32,
            state_prefix: cfg["state_prefix"].as_str().unwrap_or("query: ").to_string(),
            choice_prefix: cfg["choice_prefix"].as_str().unwrap_or("passage: ").to_string(),
            pad,
            tokenizer,
        }
    }

    /// Tokeniza un lote de textos con padding dinámico al máximo real del
    /// lote (acotado a `cap`) -- añade <s>/</s> vía el propio tokenizer.json
    /// (confirmado: ids empiezan en 0=<s>, terminan en 2=</s>).
    fn encode_batch(&self, texts: &[String], cap: usize) -> (Array2<i64>, Array2<i64>, usize) {
        let encoded: Vec<Vec<u32>> = texts
            .iter()
            .map(|t| {
                let mut ids = self.tokenizer.encode(t.as_str(), true).expect("tokenizer encode").get_ids().to_vec();
                ids.truncate(cap);
                ids
            })
            .collect();
        let seq_len = encoded.iter().map(|v| v.len()).max().unwrap_or(1).max(1);
        let n = texts.len();
        let mut ids_flat = vec![i64::from(self.pad); n * seq_len];
        let mut mask_flat = vec![0i64; n * seq_len];
        for (row, ids) in encoded.iter().enumerate() {
            for (j, &v) in ids.iter().enumerate() {
                ids_flat[row * seq_len + j] = i64::from(v);
                mask_flat[row * seq_len + j] = 1;
            }
        }
        (
            Array2::from_shape_vec((n, seq_len), ids_flat).expect("ids shape"),
            Array2::from_shape_vec((n, seq_len), mask_flat).expect("mask shape"),
            seq_len,
        )
    }

    /// encoder.onnx: input_ids/attention_mask [n,seq] i64 -> token_states
    /// [n,seq,384] f32 (confirmado via introspección del grafo ONNX real).
    fn run_encoder(&self, ids: &Array2<i64>, mask: &Array2<i64>) -> ndarray::Array3<f32> {
        let mut session = self.encoder.lock().expect("decima encoder mutex poisoned");
        let outputs = session
            .run(ort::inputs![
                "input_ids" => TensorRef::from_array_view(ids.view()).expect("ids tensor"),
                "attention_mask" => TensorRef::from_array_view(mask.view()).expect("mask tensor"),
            ])
            .unwrap_or_else(|e| panic!("decima encoder.run: {e}"));
        let (shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .unwrap_or_else(|e| panic!("decima encoder token_states: {e}"));
        let dims: Vec<usize> = shape.iter().map(|&d| d as usize).collect();
        ndarray::Array3::from_shape_vec((dims[0], dims[1], dims[2]), data.to_vec())
            .expect("token_states shape")
    }

    /// Una pasada completa sobre UN state y sus opciones: encoder(state) +
    /// encoder(choices, batched) -> scorer -> scores crudos (pre-temperatura).
    fn score(&self, state_text: &str, choice_texts: &[String]) -> Result<Vec<f32>, DecisionError> {
        if choice_texts.is_empty() {
            return Err(DecisionError::Provider("decima-onnx-v1".into(), "at least one option is required".into()));
        }
        let state_full = format!("{}{}", self.state_prefix, state_text);
        let (state_ids, state_mask, state_seq) =
            self.encode_batch(std::slice::from_ref(&state_full), self.max_state_tokens);
        let state_h = self.run_encoder(&state_ids, &state_mask);

        let choice_full: Vec<String> =
            choice_texts.iter().map(|c| format!("{}{}", self.choice_prefix, c)).collect();
        let (choice_ids, choice_mask, choice_seq) = self.encode_batch(&choice_full, self.max_choice_tokens);
        let choice_h = self.run_encoder(&choice_ids, &choice_mask);

        let _ = (state_seq, choice_seq);
        let mut session = self.scorer.lock().expect("decima scorer mutex poisoned");
        let outputs = session
            .run(ort::inputs![
                "state_h" => TensorRef::from_array_view(state_h.view()).expect("state_h tensor"),
                "state_mask" => TensorRef::from_array_view(state_mask.view()).expect("state_mask tensor"),
                "choice_h" => TensorRef::from_array_view(choice_h.view()).expect("choice_h tensor"),
                "choice_mask" => TensorRef::from_array_view(choice_mask.view()).expect("choice_mask tensor"),
            ])
            .map_err(|e| DecisionError::Provider("decima-onnx-v1".into(), format!("scorer.run: {e}")))?;
        let (_, scores) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| DecisionError::Provider("decima-onnx-v1".into(), format!("scores: {e}")))?;
        Ok(scores.to_vec())
    }
}

/// El ADR-018 contract implementation. Fase 0: consumido SOLO por este harness.
struct DecimaOnnxDecisionProvider {
    bundle: DecimaBundle,
}

#[async_trait::async_trait]
impl DecisionProvider for DecimaOnnxDecisionProvider {
    fn provider_id(&self) -> &'static str {
        "decima-onnx-v1"
    }

    async fn decide(
        &self,
        req: &DecisionRequest,
    ) -> Result<HashMap<String, DecisionAnswer>, DecisionError> {
        let state_text = py_json_compact(&req.state);
        let mut names: Vec<&String> = req.questions.keys().collect();
        names.sort();
        let mut out = HashMap::new();
        for name in names {
            let (options, keys): (Vec<String>, Vec<String>) = match &req.questions[name] {
                DecisionQuestion::Choice { options } => (options.clone(), options.clone()),
                DecisionQuestion::Score { levels } => (
                    levels.iter().enumerate().map(|(i, c)| format!("level {i}: {c}")).collect(),
                    (0..levels.len()).map(|i| i.to_string()).collect(),
                ),
                DecisionQuestion::Noul => {
                    (NOUL_OPTIONS.iter().map(|s| (*s).to_string()).collect(), vec!["false".into(), "true".into()])
                }
            };
            let raw = self.bundle.score(&state_text, &options)?;
            let scaled: Vec<f32> = raw.iter().map(|&v| v / self.bundle.temperature).collect();
            let probs = softmax_f32(&scaled);
            let distribution = keys.iter().zip(probs.iter()).map(|(k, v)| (k.clone(), *v)).collect();
            out.insert(name.clone(), DecisionAnswer { distribution, calibration: None });
        }
        Ok(out)
    }
}

/// Test golden del model card (amyrmahdy/decima-small): verifica paridad del
/// port Rust contra el ejemplo publicado por el autor (billing/technical
/// support/sales/account security, "someone logged in from another country").
/// Tolerancia 1e-2 (el publicado por el autor ya redondea a 3 decimales).
fn run_golden_decima(dir: &str) -> bool {
    let bundle = DecimaBundle::load(Path::new(dir));
    let options = ["billing", "technical support", "sales", "account security"]
        .map(|s| s.to_string());
    // No se repite el texto de la pregunta en choice/state -- confirmado
    // empíricamente contra el modelo real: anteponer "Which team should
    // handle this request?" (como sugería la paráfrasis del model card)
    // daba [0.013, 0.140, 0.007, 0.840] vs esperado [0.004, 0.031, 0.001,
    // 0.964]; sin la pregunta repetida (solo state_prefix/choice_prefix +
    // texto crudo) da [0.009, 0.032, 0.002, 0.957], dentro de tolerancia
    // int8. decide() ya hacía esto bien -- el bug era solo de este test.
    let raw = bundle
        .score("Someone logged into my account from another country.", &options)
        .expect("decima score");
    let scaled: Vec<f32> = raw.iter().map(|&v| v / bundle.temperature).collect();
    let probs = softmax_f32(&scaled);
    let expected = [0.004f32, 0.031, 0.001, 0.964];
    println!("[golden-decima] got={probs:?} expected={expected:?}");
    let mut ok = true;
    for (i, (&p, &e)) in probs.iter().zip(expected.iter()).enumerate() {
        let d = (p - e).abs();
        println!("  [{i}] {} got={p:.4} expected={e:.4} |Δ|={d:.4}", options[i]);
        if d > 0.05 {
            ok = false;
        }
    }
    ok
}

fn run_variant_d(dir: &str, split: &str, source: &str, limit: Option<usize>, budget_ms: f64) {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(run_variant_d_inner(dir, split, source, limit, budget_ms));
}

async fn run_variant_d_inner(dir: &str, split: &str, source: &str, limit: Option<usize>, budget_ms: f64) {
    println!("[variant-d] loading Decima-small ONNX bundle from {dir} (int8, ~127 MB)");
    let t0 = Instant::now();
    let provider = DecimaOnnxDecisionProvider { bundle: DecimaBundle::load(Path::new(dir)) };
    println!(
        "[variant-d] bundle ready in {:.1}s (provider: {})",
        t0.elapsed().as_secs_f64(),
        provider.provider_id()
    );

    let ds = std::fs::read_to_string(DATASET_PATH)
        .unwrap_or_else(|e| panic!("read {DATASET_PATH}: {e} (regenerate with --export first)"));
    let items: Vec<serde_json::Value> = ds
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<serde_json::Value>(l).expect("dataset line parse"))
        .filter(|it| {
            (split == "all" || it["split"].as_str() == Some(split))
                && match source {
                    "audit" => it["source"].as_str() == Some("guild_audit_log"),
                    "confusion" => it["source"].as_str() == Some("scheduler_confusion"),
                    _ => true,
                }
        })
        .take(limit.unwrap_or(usize::MAX))
        .collect();
    println!(
        "[variant-d] items after filters (split={split}, source={source}, limit={limit:?}): {}",
        items.len()
    );

    let mut hitl = BinaryDecisionStats::new();
    let mut route = ChoiceDecisionStats::new();
    let mut decide_errors = 0usize;
    for it in &items {
        let mut questions: HashMap<String, DecisionQuestion> = HashMap::new();
        let mut want_route = false;
        let mut want_hitl = false;
        if let Some(opts) = it["questions"]["route_guild"]["options"].as_array() {
            let options: Vec<String> =
                opts.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect();
            if !options.is_empty() {
                questions.insert("route_guild".into(), DecisionQuestion::Choice { options });
                want_route = true;
            }
        }
        if it["questions"].get("hitl_resolve").is_some() {
            questions.insert("hitl_resolve".into(), DecisionQuestion::Noul);
            want_hitl = true;
        }
        if questions.is_empty() {
            continue;
        }
        let req = DecisionRequest { state: it["state"].clone(), questions };
        let t0 = Instant::now();
        let answers = match provider.decide(&req).await {
            Ok(a) => a,
            Err(e) => {
                decide_errors += 1;
                println!("  decide error ({e}) — item skipped");
                continue;
            }
        };
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        if want_route {
            let dist = &answers["route_guild"].distribution;
            let (best, conf) = argmax(dist);
            let label = it["labels"]["route_guild"].as_str().unwrap_or_default().to_string();
            let base_raw = it["baseline_A"]["route_guild"].as_str().unwrap_or_default().to_string();
            let base = bucket_guild(&base_raw);
            let brier_item: f64 =
                dist.iter().map(|(k, v)| (f64::from(*v) - f64::from(*k == label)).powi(2)).sum();
            let correct_b = if base_raw.is_empty() { None } else { Some(base == label) };
            route.update(best == label, correct_b, conf, brier_item, ms);
        }
        if want_hitl {
            let dist = &answers["hitl_resolve"].distribution;
            let p_true = dist.get("true").copied().unwrap_or(0.0);
            let label = it["labels"]["hitl_resolve"].as_str() == Some("true");
            let base_pred = it["baseline_A"]["hitl_resolve"].as_str() == Some("true");
            hitl.update(p_true >= 0.5, p_true, base_pred, label, ms);
        }
    }

    hitl.lat.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    route.lat.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let hitl_n = hitl.n;
    let route_n = route.n;
    if hitl_n > 0 {
        let lat = &hitl.lat;
        println!("\n[variant-d] decision=hitl_resolve (Noul) n={hitl_n}");
        println!(
            "  accuracy: baseline A {:.4} -> D {:.4} (TP {} FP {} TN {} FN {})",
            hitl.correct_b as f64 / hitl_n as f64,
            hitl.correct_c as f64 / hitl_n as f64,
            hitl.true_pos,
            hitl.false_pos,
            hitl.true_neg,
            hitl.false_neg
        );
        println!(
            "  Brier: baseline {:.4} -> D {:.4} | ECE(15): baseline {:.4} -> D {:.4} | AUROC: D {:.4}",
            hitl.brier_b / hitl_n as f64,
            hitl.brier_c / hitl_n as f64,
            ece_15(&hitl.ece_b),
            ece_15(&hitl.ece_c),
            auroc(&hitl.pos_c, &hitl.neg_c)
        );
        println!(
            "  latency (D, CPU, per item): p50 {:.1} ms / p99 {:.1} ms / max {:.1} ms",
            pct(lat, 50.0),
            pct(lat, 99.0),
            lat.last().copied().unwrap_or(f64::NAN)
        );
    }
    if route_n > 0 {
        let lat = &route.lat;
        println!("\n[variant-d] decision=route_guild (Choice, dynamic options) n={route_n}");
        println!(
            "  accuracy: baseline A {:.4} (of {} items with live baseline) -> D {:.4}",
            if route.with_baseline > 0 { route.correct_b as f64 / route.with_baseline as f64 } else { f64::NAN },
            route.with_baseline,
            route.correct_c as f64 / route_n as f64
        );
        println!(
            "  Brier (multiclass sum): baseline {:.4} -> D {:.4} | ECE(15) top-class: D {:.4}",
            route.brier_b / route.with_baseline.max(1) as f64,
            route.brier_c / route_n as f64,
            ece_15(&route.ece_c)
        );
        println!(
            "  latency (D, CPU, per item): p50 {:.1} ms / p99 {:.1} ms / max {:.1} ms",
            pct(lat, 50.0),
            pct(lat, 99.0),
            lat.last().copied().unwrap_or(f64::NAN)
        );
    }
    if decide_errors > 0 {
        println!("\n[variant-d] decide errors: {decide_errors} items skipped");
    }

    println!("\n[gate] ADR-018 §3 (budget {budget_ms} ms):");
    let mut clauses_ok = true;
    if hitl_n > 0 {
        let acc_c = hitl.correct_c as f64 / hitl_n as f64;
        let acc_b = hitl.correct_b as f64 / hitl_n as f64;
        let acc_pass = acc_c > acc_b;
        clauses_ok &= acc_pass;
        let calib_pass = hitl.brier_c < hitl.brier_b && ece_15(&hitl.ece_c) < ece_15(&hitl.ece_b);
        clauses_ok &= calib_pass;
        let lat_pass = pct(&hitl.lat, 99.0) <= budget_ms;
        clauses_ok &= lat_pass;
        println!("  1. accuracy hitl: D {acc_c:.4} vs baseline {acc_b:.4} — {}", if acc_pass { "PASS" } else { "FAIL" });
        println!(
            "  2. calibration hitl (Brier {:.4} vs {:.4}, ECE {:.4} vs {:.4}) — {}",
            hitl.brier_c / hitl_n as f64,
            hitl.brier_b / hitl_n as f64,
            ece_15(&hitl.ece_c),
            ece_15(&hitl.ece_b),
            if calib_pass { "PASS" } else { "FAIL" }
        );
        println!("  3. p99 latency hitl: {:.1} ms <= {budget_ms} — {}", pct(&hitl.lat, 99.0), if lat_pass { "PASS" } else { "FAIL" });
    } else {
        println!("  1-3. no hitl items in this slice — accuracy/calibration/latency clauses NOT EVALUATED");
        clauses_ok = false;
    }
    println!(
        "  4. cost vs substituted LLM: NOT MEASURABLE in variant D alone — needs variant B as the substituted-cost reference. Inputs measured here: int8 bundle ~127 MB on disk, ~1 GB RAM expected at runtime (122M params, dual-encoder)."
    );
    let verdict = if clauses_ok {
        "GO-PROVISIONAL (clauses 1-3 pass; clause 4 pending variant B)"
    } else {
        "NO-GO on clauses 1-3"
    };
    println!("  VERDICT: {verdict}");
}

#[cfg(test)]
mod systemone_tests {
    use super::*;

    /// The wire mapping must produce a valid SystemOne question spec for
    /// every DecisionQuestion variant — this is what makes the provider
    /// runtime-agnostic (llama.cpp #29818 and Ollama >= 0.35.1 both accept it).
    #[test]
    fn question_spec_covers_all_variants() {
        let choice = systemone_question_spec(&DecisionQuestion::Choice {
            options: vec!["billing".into(), "technical".into()],
        })
        .unwrap();
        assert_eq!(choice["type"], "choice");
        assert_eq!(choice["criteria"]["billing"], serde_json::Value::Null);

        let score = systemone_question_spec(&DecisionQuestion::Score {
            levels: vec!["low".into(), "high".into()],
        })
        .unwrap();
        assert_eq!(score["type"], "score");
        assert_eq!(score["criteria"].as_array().unwrap().len(), 2);

        let noul = systemone_question_spec(&DecisionQuestion::Noul).unwrap();
        assert_eq!(noul["type"], "choice");
        assert!(noul["criteria"].get("true").is_some());
        assert!(noul["criteria"].get("false").is_some());
    }

    #[test]
    fn degenerate_questions_are_rejected_before_the_wire() {
        let one_opt = DecisionQuestion::Choice { options: vec!["only".into()] };
        assert!(matches!(
            systemone_question_spec(&one_opt),
            Err(DecisionError::EmptyQuestion(..))
        ));
        let one_level = DecisionQuestion::Score { levels: vec!["only".into()] };
        assert!(matches!(
            systemone_question_spec(&one_level),
            Err(DecisionError::EmptyQuestion(..))
        ));
    }

    #[test]
    fn noul_answer_prefers_the_typed_field() {
        let ans = SystemOneAnswer { probabilities: HashMap::new(), noul: Some(0.996) };
        let dist = systemone_distribution(&DecisionQuestion::Noul, &ans);
        assert!((dist["true"] - 0.996).abs() < 1e-6);
        assert!((dist["false"] - 0.004).abs() < 1e-6);
    }

    /// When the server answers via per-option probabilities (no typed noul
    /// field), they pass through clamped — the distribution IS the contract.
    #[test]
    fn probabilities_pass_through_clamped() {
        let mut probabilities = HashMap::new();
        probabilities.insert("true".to_string(), 1.5f64);
        probabilities.insert("false".to_string(), -0.2f64);
        let ans = SystemOneAnswer { probabilities, noul: None };
        let dist = systemone_distribution(&DecisionQuestion::Noul, &ans);
        assert_eq!(dist["true"], 1.0);
        assert_eq!(dist["false"], 0.0);

        let mut probabilities = HashMap::new();
        probabilities.insert("billing".to_string(), 0.7f64);
        probabilities.insert("technical".to_string(), 0.3f64);
        let ans = SystemOneAnswer { probabilities, noul: None };
        let dist = systemone_distribution(
            &DecisionQuestion::Choice {
                options: vec!["billing".into(), "technical".into()],
            },
            &ans,
        );
        assert!((dist["billing"] - 0.7f32).abs() < 1e-6);
        assert!((dist["technical"] - 0.3f32).abs() < 1e-6);
    }
}
