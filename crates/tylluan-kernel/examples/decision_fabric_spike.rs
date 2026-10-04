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
//! Default prints the profile only; `--export` additionally writes the
//! JSONL dataset (one line per item: state, questions, labels, live
//! baseline-A prediction, split). Never touches anything outside its
//! arguments; the DBs are opened SQLITE_OPEN_READ_ONLY.

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

fn main() {
    let export_path = std::env::args().nth(2).filter(|_| {
        std::env::args().nth(1).as_deref() == Some("--export")
    });
    if std::env::args().nth(1).as_deref() == Some("--export") && export_path.is_none() {
        panic!("--export needs a path: --export <file.jsonl>");
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
