//! Evaluación pareada OFFLINE sobre `decision_shadow_log` — ADR-021 §4,
//! encargo del Coloquio turno 1220 (@buffy, parte B). Este módulo NO toca
//! el kernel: solo lee la tabla con rusqlite en modo solo-lectura.
//!
//! Esquema congelado (Coloquio turno 1217):
//! `decision_shadow_log(id, ts, contract, request_hash, rules_value,
//! model_value NULL, model_probs_json NULL, provider, latency_ms,
//! agreed NULL, mode)` — el `state` NUNCA se guarda en claro, solo su hash
//! (`request_hash`), por eso la verdad de campo (ground truth) llega por un
//! fichero JSON externo `{ request_hash: valor_correcto }` que se une por
//! hash. Sin truth no hay acierto que calcular: el informe sale con
//! `verdict.reason = "insufficient_data"`, nunca un número inventado.
//!
//! Decisiones de medición (documentadas, no ocultas):
//! - **Abstención cuenta como fallo del modelo** en acierto y en McNemar
//!   (conservador: la abstención es de primera clase para seguridad, no un
//!   punto de precisión). Se reporta `abstention_rate` aparte.
//! - **McNemar exacto** bilateral: `p = min(1, 2 · Σ_{i≤min(b,c)} C(n,i) / 2^n)`
//!   con `n = b + c`. `b` = reglas aciertan y modelo falla; `c` = al revés.
//! - **Brier multicategoría** = media de `Σ_k (p_k − o_k)^2` sobre filas con
//!   `model_probs_json` (sin dividir por nº de clases — convención declarada).
//! - **ECE a 10 bins** de ancho igual sobre confianza = probabilidad máxima;
//!   el accuracy de cada bin es `argmax == truth`. Sin `model_probs_json`
//!   no hay ECE: `value = None` y el veredicto exige recalibrar antes de subir.
//! - **Criterio de subida de modo** (ADR-021 §4): modelo gana en acierto
//!   (estrictamente) Y `p < 0.05` Y `ECE ≤ ece_max`. `ece_max` es un PARÁMETRO
//!   (defecto 0.10): el ADR dice "ECE aceptable" sin número — no inventé un
//!   umbral de política, lo dejé configurable y lo digo aquí.
//!
//! Tests unitarios con datos sintéticos de valor conocido dentro del módulo
//! (incluidos explícitamente el caso "sin diferencia" y el caso "modelo
//! peor", que el turno 1220 pide por su nombre).

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};

pub const SHADOW_TABLE: &str = "decision_shadow_log";
pub const DEFAULT_ECE_MAX: f64 = 0.10;
pub const MCNEMAR_ALPHA: f64 = 0.05;
pub const ECE_BINS: usize = 10;
pub const REPORT_VERSION: &str = "paired-eval-decide-v1";

/// Una fila de `decision_shadow_log`. `id` y `ts` no se leen: el cómpuesto
/// (contract, request_hash) es lo que une con la verdad externa, y leer
/// tipos de columna ajenos solo añade fragilidad al contrato SQL.
#[derive(Debug, Clone, PartialEq)]
pub struct ShadowRow {
    pub contract: String,
    pub request_hash: String,
    pub rules_value: String,
    pub model_value: Option<String>,
    pub model_probs: Option<BTreeMap<String, f32>>,
    pub provider: Option<String>,
    pub latency_ms: Option<i64>,
    pub agreed: Option<bool>,
    pub mode: Option<String>,
}

/// `{ request_hash: valor_correcto }`.
pub type TruthMap = BTreeMap<String, String>;

/// Lee las filas del registro en sombra en modo solo-lectura (el kernel
/// puede seguir corriendo sobre la misma BD, como el resto de suites).
/// `CAST(... AS TEXT/INTEGER)` hace el contrato tolerante a que la
/// migración de Deep declare `ts`/`latency_ms` como INTEGER o TEXT.
pub fn load_rows(db_path: &Path, contract: Option<&str>) -> Result<Vec<ShadowRow>> {
    if !db_path.exists() {
        bail!("BD no encontrada: {}", db_path.display());
    }
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .with_context(|| format!("abrir BD solo-lectura {}", db_path.display()))?;

    let sql = format!(
        "SELECT CAST(contract AS TEXT), CAST(request_hash AS TEXT), \
                CAST(rules_value AS TEXT), model_value, model_probs_json, \
                CAST(provider AS TEXT), CAST(latency_ms AS INTEGER), agreed, \
                CAST(mode AS TEXT) \
         FROM {SHADOW_TABLE}{}",
        if contract.is_some() { " WHERE contract = ?1" } else { "" }
    );
    let mut stmt = conn.prepare(&sql).with_context(|| format!("preparar: {sql}"))?;

    let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<ShadowRow> {
        let probs_json: Option<String> = row.get(4)?;
        let model_probs = match probs_json {
            Some(s) if !s.trim().is_empty() => {
                let parsed: BTreeMap<String, f32> = serde_json::from_str(&s).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        4,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?;
                Some(parsed)
            }
            _ => None,
        };
        let agreed_int: Option<i64> = row.get(7)?;
        Ok(ShadowRow {
            contract: row.get(0)?,
            request_hash: row.get(1)?,
            // NULL = las reglas se abstuvieron: nunca coincide con la verdad.
            rules_value: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            model_value: row.get(3)?,
            model_probs,
            provider: row.get(5)?,
            latency_ms: row.get(6)?,
            agreed: agreed_int.map(|v| v != 0),
            mode: row.get(8)?,
        })
    };

    let rows = match contract {
        Some(c) => stmt
            .query_map([c], map_row)
            .with_context(|| format!("ejecutar consulta filtrada por contract={c}"))?
            .collect::<rusqlite::Result<Vec<_>>>()?,
        None => stmt
            .query_map([], map_row)
            .context("ejecutar consulta completa")?
            .collect::<rusqlite::Result<Vec<_>>>()?,
    };
    Ok(rows)
}

/// Fichero de verdad JSON: `{ "request_hash": "valor_correcto" }`.
pub fn load_truth(path: &Path) -> Result<TruthMap> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("leer truth {}", path.display()))?;
    let map: TruthMap =
        serde_json::from_str(&raw).context("truth debe ser un objeto JSON {hash: valor}")?;
    Ok(map)
}

/// McNemar exacto bilateral. Devuelve también los cuatro recuentos
/// discordantes/concordantes para que el informe sea auditable a mano.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct McNemar {
    pub n_pairs: u64,
    pub both_correct: u64,
    pub both_wrong: u64,
    /// Reglas aciertan, modelo falla (incluye abstención del modelo).
    pub b: u64,
    /// Reglas fallan, modelo acierta.
    pub c: u64,
    pub p_value: f64,
}

/// `p = min(1, 2 · P(X ≤ min(b,c)))` con `X ~ Bin(b+c, 0.5)`.
/// Suma en espacio logarítmico (tabla de log-factoriales + max-shift): la
/// recursión lineal desde P(X=0)=2^-n se desborda a 0.0 en f64 para n≳1075
/// (medido: n=1100 daba p=0.0 en la primera versión de este test).
pub fn mcnemar_exact(b: u64, c: u64) -> McNemar {
    let n = b + c;
    let k = b.min(c);
    let p_value = if n == 0 {
        1.0
    } else {
        // ln_fact[i] = ln(i!), calculado una vez, O(n).
        let mut ln_fact = vec![0.0f64; (n + 1) as usize];
        for i in 2..=n {
            ln_fact[i as usize] = ln_fact[(i - 1) as usize] + (i as f64).ln();
        }
        let ln_total = ln_fact[n as usize] - (n as f64) * std::f64::consts::LN_2;
        let mut max_ln = f64::NEG_INFINITY;
        let mut lns: Vec<f64> = Vec::with_capacity(k as usize + 1);
        for i in 0..=k {
            let ln_p = ln_total - ln_fact[i as usize] - ln_fact[(n - i) as usize];
            lns.push(ln_p);
            max_ln = max_ln.max(ln_p);
        }
        let sum = lns.iter().map(|l| (l - max_ln).exp()).sum::<f64>();
        (2.0 * (max_ln + sum.ln()).exp()).min(1.0)
    };
    McNemar {
        n_pairs: n,
        both_correct: 0,
        both_wrong: 0,
        b,
        c,
        p_value,
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct EceBin {
    pub lo: f64,
    pub hi: f64,
    pub count: u64,
    pub avg_confidence: f64,
    pub accuracy: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct EceReport {
    /// `None` sin datos de probabilidad (no se puede evaluar calibración).
    pub value: Option<f64>,
    pub bins: Vec<EceBin>,
}

/// ECE = Σ_bins (n_bins/N)·|accuracy_bins − confianza_media_bins|, 10 bins.
pub fn ece_10bins(confidences: &[f64], correct: &[bool]) -> EceReport {
    assert_eq!(confidences.len(), correct.len(), "conf y correct deben alinearse");
    if confidences.is_empty() {
        return EceReport { value: None, bins: Vec::new() };
    }
    let mut bins: Vec<(u64, f64, u64)> = vec![(0, 0.0, 0); ECE_BINS]; // (count, sum_conf, sum_correct)
    for (&conf, &ok) in confidences.iter().zip(correct) {
        let conf = conf.clamp(0.0, 1.0);
        let idx = ((conf * ECE_BINS as f64) as usize).min(ECE_BINS - 1);
        bins[idx].0 += 1;
        bins[idx].1 += conf;
        bins[idx].2 += if ok { 1 } else { 0 };
    }
    let n = confidences.len() as f64;
    let mut ece = 0.0f64;
    let mut out_bins = Vec::with_capacity(ECE_BINS);
    for (i, (count, sum_conf, sum_ok)) in bins.iter().enumerate() {
        let lo = i as f64 / ECE_BINS as f64;
        let hi = (i + 1) as f64 / ECE_BINS as f64;
        if *count == 0 {
            out_bins.push(EceBin { lo, hi, count: 0, avg_confidence: 0.0, accuracy: 0.0 });
            continue;
        }
        let cnt = *count as f64;
        let avg_conf = sum_conf / cnt;
        let acc = *sum_ok as f64 / cnt;
        ece += (cnt / n) * (acc - avg_conf).abs();
        out_bins.push(EceBin { lo, hi, count: *count, avg_confidence: avg_conf, accuracy: acc });
    }
    EceReport { value: Some(ece), bins: out_bins }
}

/// Brier multicategoría medio: media de Σ_k (p_k − o_k)^2 (one-hot en la
/// verdad). `None` si no hay filas con probabilidades.
pub fn brier_mean(scored: &[(&BTreeMap<String, f32>, &str)]) -> Option<f64> {
    if scored.is_empty() {
        return None;
    }
    let mut total = 0.0f64;
    for (probs, truth) in scored {
        let mut row = 0.0f64;
        for (opt, p) in probs.iter() {
            let o = if opt == truth { 1.0 } else { 0.0 };
            let d = *p as f64 - o;
            row += d * d;
        }
        total += row;
    }
    Some(total / scored.len() as f64)
}

fn percentile(sorted: &[i64], p: f64) -> Option<i64> {
    if sorted.is_empty() {
        return None;
    }
    let idx = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    Some(sorted[idx.min(sorted.len() - 1)])
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LatencySummary {
    pub p50_ms: Option<i64>,
    pub p95_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Verdict {
    pub upgrade_allowed: bool,
    /// `insufficient_data` | `model_worse` | `no_difference` |
    /// `not_significant` | `no_calibration_data` | `ece_above_threshold` |
    /// `upgrade_recommended`
    pub reason: String,
    pub alpha: f64,
    pub ece_max: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ContractEval {
    pub contract: String,
    pub n_rows: u64,
    pub n_labeled: u64,
    pub n_with_probs: u64,
    /// Filas etiquetadas donde el modelo NO respondió (model_value NULL).
    pub n_abstained: u64,
    pub rules_correct: u64,
    pub model_correct: u64,
    pub rules_accuracy: Option<f64>,
    pub model_accuracy: Option<f64>,
    pub abstention_rate: Option<f64>,
    pub mcnemar: Option<McNemar>,
    pub brier: Option<f64>,
    pub ece: EceReport,
    pub latency: LatencySummary,
    pub verdict: Verdict,
}

/// Evalúa UN contrato: une por `request_hash` con la verdad externa y
/// calcula aciertos, McNemar exacto, Brier, ECE y el veredicto de subida.
pub fn evaluate_contract(
    contract: &str,
    rows: &[ShadowRow],
    truth: &TruthMap,
    ece_max: f64,
) -> ContractEval {
    let n_rows = rows.len() as u64;

    // Pares etiquetados: solo filas cuyo hash existe en la verdad externa.
    let mut rules_correct = 0u64;
    let mut model_correct = 0u64;
    let mut b = 0u64;
    let mut c = 0u64;
    let mut both_correct = 0u64;
    let mut both_wrong = 0u64;
    let mut n_labeled = 0u64;
    let mut n_abstained = 0u64;
    let mut confidences: Vec<f64> = Vec::new();
    let mut correct_flags: Vec<bool> = Vec::new();
    let mut scored: Vec<(BTreeMap<String, f32>, String)> = Vec::new();

    for row in rows {
        let Some(expected) = truth.get(&row.request_hash) else { continue };
        n_labeled += 1;
        let r_ok = row.rules_value == *expected;
        // Abstención (model_value NULL) cuenta como fallo del modelo.
        let m_ok = row.model_value.as_deref() == Some(expected.as_str());
        if row.model_value.is_none() {
            n_abstained += 1;
        }
        if r_ok { rules_correct += 1; }
        if m_ok { model_correct += 1; }
        match (r_ok, m_ok) {
            (true, false) => b += 1,
            (false, true) => c += 1,
            (true, true) => both_correct += 1,
            (false, false) => both_wrong += 1,
        }
        if let Some(probs) = &row.model_probs
            && let Some((top, top_p)) = probs.iter().max_by(|a, b| {
                a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal)
            })
        {
            confidences.push(*top_p as f64);
            correct_flags.push(top == expected);
            scored.push((probs.clone(), expected.clone()));
        }
    }

    let mut latencies: Vec<i64> = rows.iter().filter_map(|r| r.latency_ms).collect();
    latencies.sort_unstable();

    let rules_accuracy =
        (n_labeled > 0).then(|| rules_correct as f64 / n_labeled as f64);
    let model_accuracy =
        (n_labeled > 0).then(|| model_correct as f64 / n_labeled as f64);
    let abstention_rate =
        (n_labeled > 0).then(|| n_abstained as f64 / n_labeled as f64);

    let mcnemar = (n_labeled > 0).then(|| McNemar {
        n_pairs: b + c,
        both_correct,
        both_wrong,
        b,
        c,
        p_value: mcnemar_exact(b, c).p_value,
    });

    let brier = {
        let refs: Vec<(&BTreeMap<String, f32>, &str)> = scored
            .iter()
            .map(|(p, t)| (p, t.as_str()))
            .collect();
        brier_mean(&refs)
    };
    let ece = ece_10bins(&confidences, &correct_flags);

    let verdict = if n_labeled == 0 {
        Verdict { upgrade_allowed: false, reason: "insufficient_data".into(), alpha: MCNEMAR_ALPHA, ece_max }
    } else if model_correct < rules_correct {
        Verdict { upgrade_allowed: false, reason: "model_worse".into(), alpha: MCNEMAR_ALPHA, ece_max }
    } else if model_correct == rules_correct {
        Verdict { upgrade_allowed: false, reason: "no_difference".into(), alpha: MCNEMAR_ALPHA, ece_max }
    } else {
        let p = mcnemar_exact(b, c).p_value;
        if p >= MCNEMAR_ALPHA {
            Verdict { upgrade_allowed: false, reason: "not_significant".into(), alpha: MCNEMAR_ALPHA, ece_max }
        } else if ece.value.is_none() {
            Verdict { upgrade_allowed: false, reason: "no_calibration_data".into(), alpha: MCNEMAR_ALPHA, ece_max }
        } else if ece.value.unwrap() > ece_max {
            Verdict { upgrade_allowed: false, reason: "ece_above_threshold".into(), alpha: MCNEMAR_ALPHA, ece_max }
        } else {
            Verdict { upgrade_allowed: true, reason: "upgrade_recommended".into(), alpha: MCNEMAR_ALPHA, ece_max }
        }
    };

    ContractEval {
        contract: contract.to_string(),
        n_rows,
        n_labeled,
        n_with_probs: scored.len() as u64,
        n_abstained,
        rules_correct,
        model_correct,
        rules_accuracy,
        model_accuracy,
        abstention_rate,
        mcnemar,
        brier,
        ece,
        latency: LatencySummary {
            p50_ms: percentile(&latencies, 50.0),
            p95_ms: percentile(&latencies, 95.0),
        },
        verdict,
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GlobalSummary {
    pub rows_total: u64,
    pub contracts_evaluated: u64,
    pub contracts_upgrade_recommended: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PairedReport {
    pub version: &'static str,
    pub alpha: f64,
    pub ece_max: f64,
    pub truth_file: String,
    pub db_path: String,
    pub contracts: Vec<ContractEval>,
    pub global: GlobalSummary,
}

/// Suite completa: agrupa por contrato, evalúa cada uno, emite informe.
pub fn run_paired(
    db_path: &Path,
    contract_filter: Option<&str>,
    truth_path: &Path,
    ece_max: f64,
) -> Result<PairedReport> {
    let rows = load_rows(db_path, contract_filter)?;
    let truth = load_truth(truth_path)?;
    if truth.is_empty() {
        bail!("truth vacío: sin verdad externa no hay acierto que medir");
    }

    let mut by_contract: BTreeMap<String, Vec<ShadowRow>> = BTreeMap::new();
    for row in rows {
        by_contract.entry(row.contract.clone()).or_default().push(row);
    }

    let contracts: Vec<ContractEval> = by_contract
        .iter()
        .map(|(name, rs)| evaluate_contract(name, rs, &truth, ece_max))
        .collect();

    let rows_total = contracts.iter().map(|c| c.n_rows).sum();
    let contracts_evaluated = contracts.len() as u64;
    let upgrade = contracts
        .iter()
        .filter(|c| c.verdict.upgrade_allowed)
        .count() as u64;

    Ok(PairedReport {
        version: REPORT_VERSION,
        alpha: MCNEMAR_ALPHA,
        ece_max,
        truth_file: truth_path.display().to_string(),
        db_path: db_path.display().to_string(),
        contracts,
        global: GlobalSummary {
            rows_total,
            contracts_evaluated,
            contracts_upgrade_recommended: upgrade,
        },
    })
}

/// Resumen legible en consola (mismo estilo que el resto de suites).
pub fn print_summary(report: &PairedReport) {
    println!();
    println!("  ══ EVALUACIÓN PAREADA — decision_shadow_log ══");
    println!("  alpha={:.2}  ece_max={:.2}  filas={}", report.alpha, report.ece_max, report.global.rows_total);
    println!();
    for c in &report.contracts {
        println!("  ── contrato: {} ──", c.contract);
        println!("  filas={}  etiquetadas={}  con_probs={}", c.n_rows, c.n_labeled, c.n_with_probs);
        match (c.rules_accuracy, c.model_accuracy) {
            (Some(r), Some(m)) => println!(
                "  reglas={}/{} ({:.1}%)   modelo={}/{} ({:.1}%)   abstención={:.1}%",
                c.rules_correct, c.n_labeled, r * 100.0,
                c.model_correct, c.n_labeled, m * 100.0,
                c.abstention_rate.unwrap_or(0.0) * 100.0,
            ),
            _ => println!("  sin datos etiquetados para acierto (únete truth por request_hash)"),
        }
        if let Some(mc) = &c.mcnemar {
            println!(
                "  McNemar exacto: b={} c={} (ambos bien={}, ambos mal={}) p={:.6}",
                mc.b, mc.c, mc.both_correct, mc.both_wrong, mc.p_value
            );
        }
        match c.brier {
            Some(b) => println!("  Brier={:.4}", b),
            None => println!("  Brier=n/d (sin probabilidades registradas)"),
        }
        match c.ece.value {
            Some(e) => println!("  ECE(10 bins)={:.4}", e),
            None => println!("  ECE=n/d (sin probabilidades registradas)"),
        }
        let v = &c.verdict;
        if v.upgrade_allowed {
            println!("  VEREDICTO: SUBIR DE MODO permitido ({})", v.reason);
        } else {
            println!("  VEREDICTO: NO subir de modo ({})", v.reason);
        }
        println!();
    }
    println!(
        "  Contratos evaluados: {} — recomendados para subir: {}",
        report.global.contracts_evaluated, report.global.contracts_upgrade_recommended
    );
    println!();
}

// ── Tests unitarios sobre datos sintéticos de valor conocido ────────────────
// El turno 1220 pide explícitamente el caso "sin diferencia" y el caso
// "modelo peor". Aquí se fija también la aritmética de McNemar contra
// valores calculados a mano (22/1024 y 2/1024, binomiales exactas).
#[cfg(test)]
mod tests {
    use super::*;

    fn row(hash: &str, rules: &str, model: Option<&str>) -> ShadowRow {
        ShadowRow {
            contract: "scheduler_routing".into(),
            request_hash: hash.into(),
            rules_value: rules.into(),
            model_value: model.map(|s| s.to_string()),
            model_probs: None,
            provider: Some("rules".into()),
            latency_ms: Some(12),
            agreed: None,
            mode: Some("shadow".into()),
        }
    }

    fn truth_of(pairs: &[(&str, &str)]) -> TruthMap {
        pairs.iter().map(|(h, v)| (h.to_string(), v.to_string())).collect()
    }

    // ── McNemar exacto ──────────────────────────────────────────────────

    #[test]
    fn mcnemar_textbook_b1_c9_is_exact() {
        // n=10: P(X≤1) = (1+10)/1024 → p = 22/1024 = 0.021484375 exacto.
        let m = mcnemar_exact(1, 9);
        assert_eq!(m.n_pairs, 10);
        assert!((m.p_value - 0.021_484_375).abs() < 1e-12, "p={}", m.p_value);
    }

    #[test]
    fn mcnemar_symmetric_is_capped_at_one() {
        // b=c → distribución simétrica: p = 1 (limitada a 1.0, no 1.3125).
        let m = mcnemar_exact(3, 3);
        assert!((m.p_value - 1.0).abs() < 1e-12, "p={}", m.p_value);
    }

    #[test]
    fn mcnemar_no_discordance_is_one() {
        assert_eq!(mcnemar_exact(0, 0).p_value, 1.0);
    }

    #[test]
    fn mcnemar_perfect_separation_is_significant() {
        // b=0, c=10 → p = 2/2^10 = 0.001953125.
        let m = mcnemar_exact(0, 10);
        assert!((m.p_value - 0.001_953_125).abs() < 1e-12, "p={}", m.p_value);
    }

    #[test]
    fn mcnemar_large_n_does_not_underflow() {
        // n=1100 con k=500: la versión lineal desde 2^-n daba p=0.0
        // (underflow f64). La versión logarítmica debe salir en (0,1].
        let m = mcnemar_exact(600, 500);
        assert!(m.p_value > 0.0 && m.p_value <= 1.0, "p={}", m.p_value);
        // Regla normal: z = (500.5−550)/√275 ≈ −2.98 → p bilateral ≈ 0.003.
        assert!(m.p_value > 0.0001 && m.p_value < 0.01, "p={} fuera de rango", m.p_value);
    }

    // ── ECE / Brier ─────────────────────────────────────────────────────

    #[test]
    fn ece_perfect_calibration_is_zero() {
        // conf 0.5 en el bin [0.5,0.6) con accuracy real 0.5 → ECE 0.
        let r = ece_10bins(&[0.5, 0.5], &[true, false]);
        assert!((r.value.unwrap()).abs() < 1e-12);
    }

    #[test]
    fn ece_certain_but_half_right_is_half() {
        // conf 1.0 siempre, acierta 10/20 → ECE = |0.5 − 1.0| = 0.5.
        let conf = vec![1.0; 20];
        let mut ok = vec![false; 20];
        for v in ok.iter_mut().take(10) { *v = true; }
        let r = ece_10bins(&conf, &ok);
        assert!((r.value.unwrap() - 0.5).abs() < 1e-12, "ECE={}", r.value.unwrap());
    }

    #[test]
    fn ece_without_data_is_none() {
        assert!(ece_10bins(&[], &[]).value.is_none());
    }

    #[test]
    fn brier_known_values() {
        let mut probs_perfect = BTreeMap::new();
        probs_perfect.insert("a".to_string(), 1.0f32);
        probs_perfect.insert("b".to_string(), 0.0f32);
        let mut probs_half = BTreeMap::new();
        probs_half.insert("a".to_string(), 0.5f32);
        probs_half.insert("b".to_string(), 0.5f32);
        assert_eq!(brier_mean(&[(&probs_perfect, "a")]), Some(0.0));
        assert_eq!(brier_mean(&[(&probs_half, "a")]), Some(0.5));
        assert_eq!(brier_mean(&[(&probs_perfect, "a"), (&probs_half, "a")]), Some(0.25));
        assert!(brier_mean(&[]).is_none());
    }

    // ── Veredictos end-to-end sobre filas sintéticas ────────────────────

    fn probs_for(value: &str, options: &[&str]) -> BTreeMap<String, f32> {
        options
            .iter()
            .map(|o| (o.to_string(), if *o == value { 1.0 } else { 0.0 }))
            .collect()
    }

    #[test]
    fn verdict_upgrade_when_model_wins_significantly_and_calibrated() {
        // 20 casos etiquetados: reglas aciertan 10, modelo 20 (c=10, b=0).
        let rows: Vec<ShadowRow> = (0..20)
            .map(|i| {
                let expected = if i % 2 == 0 { "a" } else { "b" };
                let rules = if i < 10 { expected } else { "z" };
                ShadowRow {
                    model_probs: Some(probs_for(expected, &["a", "b", "z"])),
                    ..row(&format!("h{i}"), rules, Some(expected))
                }
            })
            .collect();
        let truth: TruthMap = (0..20)
            .map(|i| (format!("h{i}"), if i % 2 == 0 { "a".to_string() } else { "b".to_string() }))
            .collect();
        let ev = evaluate_contract("scheduler_routing", &rows, &truth, DEFAULT_ECE_MAX);
        assert_eq!(ev.rules_correct, 10);
        assert_eq!(ev.model_correct, 20);
        let mc = ev.mcnemar.unwrap();
        assert_eq!((mc.b, mc.c), (0, 10));
        assert!(mc.p_value < 0.05);
        assert!(ev.brier.unwrap() < 1e-6, "one-hot sobre verdad → Brier 0");
        assert!(ev.ece.value.unwrap() < 1e-6, "one-hot sobre verdad → ECE 0");
        assert!(ev.verdict.upgrade_allowed, "{:?}", ev.verdict);
        assert_eq!(ev.verdict.reason, "upgrade_recommended");
    }

    #[test]
    fn verdict_model_worse_never_upgrades_even_if_significant() {
        // Reglas aciertan 20/20; modelo 8/20 → b=12, c=0: significativo
        // EN CONTRA del modelo. Debe dar "model_worse", nunca subir.
        let rows: Vec<ShadowRow> = (0..20)
            .map(|i| {
                let model = if i < 8 { "a" } else { "z" }; // modelo acierta 8
                ShadowRow {
                    model_probs: Some(probs_for(model, &["a", "z"])),
                    ..row(&format!("h{i}"), "a", Some(model))
                }
            })
            .collect();
        let truth: TruthMap = (0..20).map(|i| (format!("h{i}"), "a".to_string())).collect();
        let ev = evaluate_contract("scheduler_routing", &rows, &truth, DEFAULT_ECE_MAX);
        assert_eq!(ev.rules_correct, 20);
        assert_eq!(ev.model_correct, 8);
        let mc = ev.mcnemar.unwrap();
        assert_eq!((mc.b, mc.c), (12, 0));
        assert!(mc.p_value < 0.05, "p={}", mc.p_value);
        assert!(!ev.verdict.upgrade_allowed);
        assert_eq!(ev.verdict.reason, "model_worse");
    }

    #[test]
    fn verdict_no_difference_when_model_echoes_rules() {
        let rows: Vec<ShadowRow> = (0..10)
            .map(|i| row(&format!("h{i}"), "a", Some("a")))
            .collect();
        let truth: TruthMap = (0..10).map(|i| (format!("h{i}"), "a".to_string())).collect();
        let ev = evaluate_contract("scheduler_routing", &rows, &truth, DEFAULT_ECE_MAX);
        assert_eq!(ev.rules_correct, ev.model_correct);
        assert!(!ev.verdict.upgrade_allowed);
        assert_eq!(ev.verdict.reason, "no_difference");
    }

    #[test]
    fn verdict_not_significant_when_gap_is_small() {
        // Verdad = "a" en los 7 casos. h1/h2: reglas bien ("a"), modelo
        // mal ("b") → b=2. h3..h7: reglas mal ("b"), modelo bien ("a") → c=5.
        // n=7, p = 2·(1+7+21)/128 = 0.453125 ≥ 0.05: gana el modelo sin respaldo.
        let rows = vec![
            row("h1", "a", Some("b")), // b: reglas bien, modelo mal
            row("h2", "a", Some("b")), // b
            row("h3", "b", Some("a")), // c
            row("h4", "b", Some("a")), // c
            row("h5", "b", Some("a")), // c
            row("h6", "b", Some("a")), // c
            row("h7", "b", Some("a")), // c
        ];
        let truth = truth_of(&[
            ("h1", "a"), ("h2", "a"), ("h3", "a"), ("h4", "a"),
            ("h5", "a"), ("h6", "a"), ("h7", "a"),
        ]);
        let ev = evaluate_contract("scheduler_routing", &rows, &truth, DEFAULT_ECE_MAX);
        // Reglas aciertan h1/h2 (2); modelo acierta h3..h7 (5).
        assert_eq!((ev.rules_correct, ev.model_correct), (2, 5));
        let mc = ev.mcnemar.as_ref().unwrap();
        assert_eq!((mc.b, mc.c), (2, 5));
        assert!(!ev.verdict.upgrade_allowed);
        assert_eq!(ev.verdict.reason, "not_significant");
    }

    #[test]
    fn verdict_no_calibration_data_blocks_upgrade() {
        // Modelo gana con p significativo pero sin probabilidades registradas:
        // no hay ECE que evaluar → no se sube (recalibrar primero).
        let rows: Vec<ShadowRow> = (0..20)
            .map(|i| {
                let expected = if i % 2 == 0 { "a" } else { "b" };
                let rules = if i < 10 { expected } else { "z" };
                row(&format!("h{i}"), rules, Some(expected))
            })
            .collect();
        let truth: TruthMap = (0..20)
            .map(|i| (format!("h{i}"), if i % 2 == 0 { "a".to_string() } else { "b".to_string() }))
            .collect();
        let ev = evaluate_contract("scheduler_routing", &rows, &truth, DEFAULT_ECE_MAX);
        assert!(ev.brier.is_none() && ev.ece.value.is_none());
        assert!(!ev.verdict.upgrade_allowed);
        assert_eq!(ev.verdict.reason, "no_calibration_data");
    }

    #[test]
    fn abstention_counts_as_model_failure_and_is_reported() {
        let rows = vec![
            row("h1", "a", Some("a")),  // ambos bien
            row("h2", "a", None),       // modelo abstiene → fallo del modelo
            row("h3", "b", None),       // modelo abstiene; reglas mal
            row("h4", "a", Some("a")),
        ];
        let truth = truth_of(&[("h1", "a"), ("h2", "a"), ("h3", "a"), ("h4", "a")]);
        let ev = evaluate_contract("scheduler_routing", &rows, &truth, DEFAULT_ECE_MAX);
        assert_eq!(ev.n_labeled, 4);
        assert_eq!(ev.n_abstained, 2, "abstenciones contadas");
        assert_eq!(ev.rules_correct, 3);
        assert_eq!(ev.model_correct, 2, "abstención NO cuenta como acierto");
        assert!((ev.abstention_rate.unwrap() - 0.5).abs() < 1e-12);
    }

    #[test]
    fn rows_without_truth_are_excluded_from_scoring() {
        let rows = vec![
            row("h1", "a", Some("a")),
            row("sin_verdad", "b", Some("b")), // hash no está en truth → fuera
        ];
        let truth = truth_of(&[("h1", "a")]);
        let ev = evaluate_contract("scheduler_routing", &rows, &truth, DEFAULT_ECE_MAX);
        assert_eq!(ev.n_rows, 2);
        assert_eq!(ev.n_labeled, 1, "solo se puntúa lo unido por hash");
        assert_eq!(ev.rules_correct, 1);
    }

    #[test]
    fn verdict_insufficient_data_without_labels() {
        let rows = vec![row("h1", "a", Some("a"))];
        let truth = truth_of(&[]);
        let ev = evaluate_contract("scheduler_routing", &rows, &truth, DEFAULT_ECE_MAX);
        assert!(!ev.verdict.upgrade_allowed);
        assert_eq!(ev.verdict.reason, "insufficient_data");
        assert!(ev.mcnemar.is_none() && ev.rules_accuracy.is_none());
    }

    // ── Contrato SQL contra el esquema congelado (turno 1217) ───────────

    #[test]
    fn load_rows_reads_the_frozen_schema() {
        let dir = std::env::temp_dir();
        let db = dir.join(format!(
            "paired_eval_test_{}_{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        {
            let conn = rusqlite::Connection::open(&db).unwrap();
            // Nombres de columna EXACTOS del turno 1217. Si Deep se desvía,
            // este test es el primero en gritar.
            conn.execute_batch(
                "CREATE TABLE decision_shadow_log(\
                   id INTEGER PRIMARY KEY, ts INTEGER, contract TEXT NOT NULL, \
                   request_hash TEXT NOT NULL, rules_value TEXT, model_value TEXT, \
                   model_probs_json TEXT, provider TEXT, latency_ms INTEGER, \
                   agreed INTEGER, mode TEXT);",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO decision_shadow_log\
                 (id, ts, contract, request_hash, rules_value, model_value, model_probs_json, provider, latency_ms, agreed, mode)\
                 VALUES (1, 100, 'scheduler_routing', 'h1', 'a', 'a', '{\"a\":0.9,\"b\":0.1}', 'rules+model', 15, 1, 'shadow')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO decision_shadow_log\
                 (id, ts, contract, request_hash, rules_value, model_value, model_probs_json, provider, latency_ms, agreed, mode)\
                 VALUES (2, 101, 'scheduler_routing', 'h2', 'a', NULL, NULL, 'rules', 9, NULL, 'shadow')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO decision_shadow_log\
                 (id, ts, contract, request_hash, rules_value, model_value, model_probs_json, provider, latency_ms, agreed, mode)\
                 VALUES (3, 102, 'human_review', 'h3', 'review', 'review', NULL, 'rules', 4, 1, 'off')",
                [],
            )
            .unwrap();
        }

        let all = load_rows(&db, None).unwrap();
        assert_eq!(all.len(), 3);
        let a = &all[0];
        assert_eq!(a.contract, "scheduler_routing");
        assert_eq!(a.request_hash, "h1");
        assert_eq!(a.rules_value, "a");
        assert_eq!(a.model_value.as_deref(), Some("a"));
        let probs = a.model_probs.as_ref().unwrap();
        assert!((probs["a"] - 0.9).abs() < 1e-6);
        assert_eq!(a.agreed, Some(true));
        assert_eq!(a.latency_ms, Some(15));
        assert_eq!(all[1].model_value, None, "model_value NULL se lee como abstención");
        assert_eq!(all[1].model_probs, None);
        assert_eq!(all[1].agreed, None);
        assert_eq!(all[2].contract, "human_review");

        let filtered = load_rows(&db, Some("scheduler_routing")).unwrap();
        assert_eq!(filtered.len(), 2, "filtro por contrato");

        let truth = truth_of(&[("h1", "a"), ("h2", "a"), ("h3", "review")]);
        let ev = evaluate_contract("scheduler_routing", &filtered, &truth, DEFAULT_ECE_MAX);
        assert_eq!(ev.n_labeled, 2);
        assert_eq!(ev.model_correct, 1, "h2 abstuvo → fallo");

        std::fs::remove_file(&db).ok();
    }

    #[test]
    fn load_rows_missing_table_is_a_clear_error() {
        let db = std::env::temp_dir().join(format!(
            "paired_eval_empty_{}.db",
            std::process::id()
        ));
        {
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.execute_batch("CREATE TABLE otra(x INTEGER);").unwrap();
        }
        let err = load_rows(&db, None).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains(SHADOW_TABLE), "el error debe nombrar la tabla: {msg}");
        std::fs::remove_file(&db).ok();
    }
}
