//! # Complexity Scoring
//!
//! Heuristic (rule-based, no ONNX) detection of intent complexity.
//!
//! ## Cascade logic (M20)
//!
//! | Score range  | Action                          |
//! |--------------|----------------------------------|
//! | >= 0.6       | **Proactive** — route to coordinator directly |
//! | 0.4 .. 0.6   | **Reactive** — try direct guild, fallback to coordinator on failure |
//! | < 0.4        | **Direct** — route to best-matching guild |
//!
//! Heuristics are pure keyword/token analysis — zero ONNX inference.
//! Complex intent prototypes are lazily computed via OnceLock and cached
//! forever (Observation 2 — lazy semantic prototypes deferred to M20-B).
//!
//! ## Prerequisito (Observation 1)
//! Before any cascade dispatch to `coordinator`, the caller MUST verify
//! `registry.has_guild("coordinator")`. Degradación elegante si no existe.
//!
//! ## Delegation gate (WS2, 2026-09-13 external audit — coordinator hijack)
//! Complexity ≠ delegation permission. A high complexity score may *suggest*
//! coordination, but it does not confer authority to delegate: the J-13
//! benchmark's manual triage caught a complex non-delegation intent being
//! swallowed by the cascade and dying on coordinator's required `task` arg.
//! Since 2026-09-13 every cascade dispatch to coordinator additionally
//! requires `wants_delegation(&intent)` — an explicit delegation/orchestration
//! signal in the intent itself (or a coordinator worker calling its own
//! subtasks, checked separately by the caller). `coordinator_eligible()`
//! bundles both conditions so call sites cannot legally bypass the gate.

fn count_numbered_prefixes(text: &str) -> usize {
    let mut count = 0;
    for token in text.split_whitespace() {
        let trimmed = token.trim_matches(|c: char| c == '(' || c == '[' || c == '{');
        let has_number = trimmed
            .trim_end_matches(['.', ')', ']'])
            .parse::<u32>()
            .is_ok();
        if has_number && trimmed.contains('.') || trimmed.contains(')') {
            count += 1;
        }
    }
    count
}

/// Extract the 6 features used by the MLP complexity model.
/// Returns [word_count_norm, has_multi_step, numbered_norm,
///          has_complex_verb, compound_ratio, is_simple].
pub fn extract_mlp_features(intent: &str) -> [f64; 6] {
    let lower = intent.trim().to_lowercase();
    let word_count = lower.split_whitespace().count();

    let multi_step_signals = [
        "and then", "then ", "after that", "finally", "meanwhile",
        "y luego", "luego ", "después", "despues", "finalmente",
        "meanwhile", "subsequently", "following that", "next ",
        "in parallel", "simultaneously", "at the same time",
        "once that", "once done",
    ];
    let has_multi_step = if multi_step_signals.iter().any(|s| lower.contains(s)) { 1.0 } else { 0.0 };

    let numbered = count_numbered_prefixes(&lower);
    let numbered_norm = if numbered >= 2 { 1.0 } else if numbered == 1 { 0.5 } else { 0.0 };

    let enum_words = ["first", "second", "third", "fourth", "next", "last",
                       "primero", "segundo", "tercero", "siguiente", "último",
                       "step 1", "step 2", "paso 1", "paso 2",
                       "firstly", "secondly", "thirdly"];
    let has_enumeration = if enum_words.iter().any(|w| lower.contains(w)) { 1.0 } else { 0.0 };

    let synthesis_signals = [
        "synthesize", "synthesise", "synthesis",
        "summarize", "summarise", "summary", "sum up",
        "combine", "merge", "unify", "consolidate",
        "wrap up", "conclude", "finalize", "recap",
        "put it together", "collect results",
        "generar resumen", "resumir", "sintetiza", "sintetizar",
        "combinar", "unificar", "consolidar",
        "dame un resumen", "resume todo", "resume", "resuma",
    ];
    let has_synthesis = if synthesis_signals.iter().any(|s| lower.contains(s)) { 1.0 } else { 0.0 };
    let has_complex_verb = if has_enumeration > 0.0 || has_synthesis > 0.0 { 1.0 } else { 0.0 };

    let and_count = lower.matches(" and ").count();
    let comma_count = lower.matches(", ").count();
    let compound_actions = (and_count + comma_count) as f64;
    let compound_ratio = (compound_actions / word_count.max(1) as f64).clamp(0.0, 1.0);

    let word_count_norm = (word_count as f64 / 30.0).clamp(0.0, 1.0);

    let simple_triggers = [
        "list ", "show ", "run ", "echo ", "pwd ", "ls ", "cat ",
        "status", "health", "ping",
        "busca ", "encuentra ", "lista ", "muestra ",
        "ejecuta ", "compila ",
    ];
    let is_shell_cmd = lower.len() < 30 && !lower.contains(' ');
    let is_simple_verb = simple_triggers.iter().any(|t| lower.starts_with(t)) && word_count <= 5;
    let is_simple = if is_shell_cmd || is_simple_verb { 1.0 } else { 0.0 };

    [word_count_norm, has_multi_step, numbered_norm, has_complex_verb, compound_ratio, is_simple]
}

/// Score intent complexity on a 0.0–1.0 scale.
/// Higher = more likely multi-step / synthesis / complex.
pub fn score_complexity(intent: &str) -> f64 {
    let lower = intent.trim().to_lowercase();
    let word_count = lower.split_whitespace().count();
    if word_count < 3 {
        return 0.0; // too short to be complex
    }
    let mut score = 0.0_f64;

    // ── Multi-step connectors ─────────────────────────────────────────────
    let multi_step_signals = [
        "and then", "then ", "after that", "finally", "meanwhile",
        "y luego", "luego ", "después", "despues", "finalmente",
        "meanwhile", "subsequently", "following that", "next ",
        "in parallel", "simultaneously", "at the same time",
        "once that", "once done",
    ];
    for signal in &multi_step_signals {
        if lower.contains(signal) {
            score += 0.35;
            break;
        }
    }

    // ── Numbered lists ────────────────────────────────────────────────────
    let numbered = count_numbered_prefixes(&lower);
    if numbered >= 2 {
        score += 0.30;
    } else if numbered == 1 {
        score += 0.15;
    }

    // ── Enumeration words ─────────────────────────────────────────────────
    let enum_words = ["first", "second", "third", "fourth", "next", "last",
                       "primero", "segundo", "tercero", "siguiente", "último",
                       "step 1", "step 2", "paso 1", "paso 2",
                       "firstly", "secondly", "thirdly"];
    for w in &enum_words {
        if lower.contains(w) {
            score += 0.20;
            break;
        }
    }

    // ── Synthesis / summary signals ───────────────────────────────────────
    let synthesis_signals = [
        "synthesize", "synthesise", "synthesis",
        "summarize", "summarise", "summary", "sum up",
        "combine", "merge", "unify", "consolidate",
        "wrap up", "conclude", "finalize", "recap",
        "put it together", "collect results",
        "generar resumen", "resumir", "sintetiza", "sintetizar",
        "combinar", "unificar", "consolidar",
        "dame un resumen", "resume todo", "resume", "resuma",
    ];
    for signal in &synthesis_signals {
        if lower.contains(signal) {
            score += 0.25;
            break;
        }
    }

    // ── Multiple commas or "and" suggesting compound tasks ────────────────
    let and_count = lower.matches(" and ").count();
    let comma_count = lower.matches(", ").count();
    let compound_actions = and_count + comma_count;
    if compound_actions >= 3 {
        score += 0.25 * (compound_actions as f64).min(4.0) / 4.0;
    } else if compound_actions >= 1 {
        score += 0.10;
    }

    // ── Sentence length bonus (longer = more likely complex) ──────────────
    if word_count >= 10 {
        score += 0.10;
    }
    if word_count >= 20 {
        score += 0.10;
    }

    // ── Simple intent discounts ───────────────────────────────────────────
    let simple_triggers = [
        "list ", "show ", "run ", "echo ", "pwd ", "ls ", "cat ",
        "status", "health", "ping",
        "busca ", "encuentra ", "lista ", "muestra ",
        "ejecuta ", "compila ",
    ];
    let is_simple_verb = simple_triggers.iter().any(|t| lower.starts_with(t));
    let is_shell_cmd = lower.len() < 30 && !lower.contains(' ');
    if is_shell_cmd {
        score = 0.0;
    } else if is_simple_verb && word_count <= 5 {
        score *= 0.5; // halve the score for short simple commands
    }

    score.clamp(0.0, 1.0)
}

/// Explicit delegation/orchestration signals. An intent that matches any of
/// these is *asking* for orchestration (or is a coordinator worker dispatching
/// its own subtasks, which the caller detects separately). Multilingual EN/ES,
/// same convention as every other signal list in this file.
const DELEGATION_SIGNALS: &[&str] = &[
    // EN
    "coordinate ", "coordinator ", "delegate ", "delegate to",
    "orchestrate ", "orchestration", "fan out", "fan-out",
    "dispatch to coordinator", "via coordinator", "through coordinator",
    "break this down", "break down this", "split this into subtasks",
    "split into subtasks", "multi-agent", "spawn agents", "spawn subtasks",
    "run in parallel with agents", "work as a team", "team of agents",
    // ES
    "coordina ", "coordinador ", "delega ", "delegar en",
    "orquesta ", "orquestación", "orquestacion", "reparte entre agentes",
    "divide en subtareas", "divide la tarea", "descompón la tarea",
    "descompon la tarea", "equipo de agentes", "agentes en paralelo",
];

/// True when the intent itself requests delegation/orchestration.
///
/// This is the WS2 delegation gate: the proactive and reactive cascades must
/// not route to coordinator on complexity alone. Deliberately a substring
/// check on the trimmed-lowercase intent — pure, no I/O, no ML, cheap enough
/// to run on every dispatch. False negatives degrade gracefully (the intent
/// routes to its best guild, which is the pre-cascade behavior); false
/// positives only widen coordinator access for intents that literally asked
/// for coordination.
pub fn wants_delegation(intent: &str) -> bool {
    let lower = intent.trim().to_lowercase();
    if lower.is_empty() {
        return false;
    }
    DELEGATION_SIGNALS.iter().any(|s| lower.contains(s))
}

/// Single decision point for cascade eligibility (proactive AND reactive).
/// Bundles complexity, the delegation gate and the explicit-hint override so
/// call sites cannot reassemble the pieces differently.
///
/// - `has_explicit_hint`: the caller asked for coordinator by name (guild
///   hint). Explicit requests always win — that is a user decision, not a
///   heuristic.
/// - `is_coordinator_worker`: the caller IS coordinator dispatching subtasks.
///   Its internal fan-out must never be gated by its own heuristics.
pub fn coordinator_eligible(
    blended_score: f64,
    intent: &str,
    has_explicit_hint: bool,
    is_coordinator_worker: bool,
) -> bool {
    if has_explicit_hint || is_coordinator_worker {
        return true;
    }
    blended_score >= 0.4 && wants_delegation(intent)
}

/// Return the cascade action for a given complexity score.
pub fn cascade_action(score: f64) -> CascadeAction {
    if score >= 0.6 {
        CascadeAction::Proactive
    } else if score >= 0.4 {
        CascadeAction::Reactive
    } else {
        CascadeAction::Direct
    }
}

/// Blend heuristic complexity score with MLP-derived score.
/// Gracefully degrades to heuristic-only when mlp_score is None.
///
/// The blend is conservative: MLP adds at most 40% weight, heuristic
/// retains at least 60% weight. This ensures the MLP can only refine,
/// never override, the heuristic's safety bounds.
pub fn blend_with_mlp(heuristic: f64, mlp: Option<f64>) -> f64 {
    const HEURISTIC_WEIGHT: f64 = 0.6;
    const MLP_WEIGHT: f64 = 0.4;
    match mlp {
        Some(m) if (0.0..=1.0).contains(&m) => {
            (HEURISTIC_WEIGHT * heuristic + MLP_WEIGHT * m).clamp(0.0, 1.0)
        }
        _ => heuristic,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CascadeAction {
    /// Route directly to coordinator — intent is clearly complex
    Proactive,
    /// Try direct guild first; if it fails, fall back to coordinator
    Reactive,
    /// Route directly to the best-matching guild — intent is simple
    Direct,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_simple_intent_is_zero() {
        assert_eq!(score_complexity("hello"), 0.0);
        assert_eq!(score_complexity("hi"), 0.0);
        assert_eq!(score_complexity("ls -la"), 0.0);
        assert_eq!(score_complexity("pwd"), 0.0);
    }

    #[test]
    fn test_simple_verb_is_low() {
        let s = score_complexity("list files");
        assert!(s < 0.4, "simple list should be < 0.4, got {s}");
    }

    #[test]
    fn test_multi_step_connector_is_high() {
        let s = score_complexity("research this topic and then write a summary");
        assert!(s >= 0.35, "multi-step should be >= 0.35, got {s}");
    }

    #[test]
    fn test_numbered_steps() {
        let s = score_complexity("1. install deps 2. run tests 3. deploy");
        assert!(s >= 0.30, "numbered steps should be >= 0.30, got {s}");
    }

    #[test]
    fn test_synthesis_intent() {
        let s = score_complexity("synthesize the results into a report");
        assert!(s >= 0.25, "synthesis should be >= 0.25, got {s}");
    }

    #[test]
    fn test_compound_with_commas() {
        let s = score_complexity("check git status, run tests, push to main, and deploy");
        assert!(s >= 0.1, "compound task should be >= 0.1, got {s}");
    }

    #[test]
    fn test_long_multi_step_scores_proactive() {
        let s = score_complexity("research Rust async patterns, then implement a proof of concept, then write tests, and finally document the results");
        assert!(s >= 0.6, "long multi-step should be proactive (>= 0.6), got {s}");
    }

    #[test]
    fn test_cascade_action_proactive() {
        assert_eq!(cascade_action(0.6), CascadeAction::Proactive);
        assert_eq!(cascade_action(0.8), CascadeAction::Proactive);
    }

    #[test]
    fn test_cascade_action_reactive() {
        assert_eq!(cascade_action(0.4), CascadeAction::Reactive);
        assert_eq!(cascade_action(0.55), CascadeAction::Reactive);
    }

    #[test]
    fn test_cascade_action_direct() {
        assert_eq!(cascade_action(0.0), CascadeAction::Direct);
        assert_eq!(cascade_action(0.35), CascadeAction::Direct);
    }

    #[test]
    fn test_synthesis_spanish() {
        let s = score_complexity("sintetiza los resultados");
        assert!(s >= 0.25, "spanish synthesis should be >= 0.25, got {s}");
    }

    #[test]
    fn test_multi_step_spanish() {
        let s = score_complexity("investiga esto y luego escribe un resumen");
        assert!(s >= 0.35, "spanish multi-step should be >= 0.35, got {s}");
    }

    #[test]
    fn test_very_long_simple_still_boosted() {
        let s = score_complexity("show me the current git status of the main branch in the repository");
        assert!(s >= 0.1, "long sentence gets length bonus");
    }

    #[test]
    fn test_blend_mlp_absent_falls_back_to_heuristic() {
        assert_eq!(blend_with_mlp(0.5, None), 0.5);
        assert_eq!(blend_with_mlp(0.0, None), 0.0);
        assert_eq!(blend_with_mlp(0.8, None), 0.8);
    }

    #[test]
    fn test_blend_mlp_valid_combines_correctly() {
        let result = blend_with_mlp(0.5, Some(0.5));
        let expected = 0.6 * 0.5 + 0.4 * 0.5;
        assert!((result - expected).abs() < 1e-10);
    }

    #[test]
    fn test_blend_mlp_out_of_range_ignored() {
        assert_eq!(blend_with_mlp(0.5, Some(-0.1)), 0.5);
        assert_eq!(blend_with_mlp(0.5, Some(1.5)), 0.5);
    }

    #[test]
    fn test_blend_mlp_clamps_to_unit_interval() {
        let result = blend_with_mlp(0.9, Some(0.9));
        assert!(result <= 1.0);
        assert!(result >= 0.0);
    }

    #[test]
    fn test_blend_with_mlp_heuristic_dominant() {
        let high_heuristic_low_mlp = blend_with_mlp(0.8, Some(0.2));
        let low_heuristic_high_mlp = blend_with_mlp(0.2, Some(0.8));
        assert!(high_heuristic_low_mlp > low_heuristic_high_mlp,
            "heuristic (60%) should dominate over mlp (40%)");
    }

    #[test]
    fn test_blend_mlp_preserves_cascade_boundaries() {
        let near_proactive = blend_with_mlp(0.55, Some(0.68));
        assert!(near_proactive >= 0.6, "blend near proactive boundary should cross it: {near_proactive}");
        let near_direct = blend_with_mlp(0.35, Some(0.0));
        assert!(near_direct < 0.4, "blend near direct boundary should stay under: {near_direct}");
    }

    // ── WS2 delegation gate (coordinator hijack fix) ─────────────────────

    #[test]
    fn test_wants_delegation_positive_signals() {
        for intent in [
            "delegate this research to the coordinator",
            "coordinate the migration across the team",
            "orchestrate the deployment with a team of agents",
            "break this down into subtasks and run them",
            "coordina la revision del PR con el equipo",
            "delega en el coordinador la busqueda",
            "divide en subtareas y ejecuta en paralelo",
        ] {
            assert!(wants_delegation(intent), "'{intent}' must signal delegation");
        }
    }

    #[test]
    fn test_wants_delegation_negative_complex_but_not_delegation() {
        // Complex intents that never asked to be coordinated — the exact
        // J-13 hijack class. High score, no delegation signal.
        for intent in [
            "research Rust async patterns, then implement a proof of concept, then write tests, and finally document the results",
            "1. clone the repo 2. build the kernel 3. run the full test suite",
            "summarize the incident report, extract lessons, and update the runbook",
        ] {
            assert!(!wants_delegation(intent), "'{intent}' is complex but must NOT signal delegation");
            let score = score_complexity(intent);
            assert!(score >= 0.3, "sanity: '{intent}' should still be complex, got {score}");
        }
    }

    #[test]
    fn test_wants_delegation_edge_cases() {
        assert!(!wants_delegation(""), "empty intent never delegates");
        assert!(!wants_delegation("   "), "whitespace-only never delegates");
        assert!(wants_delegation("  COORDINATE THE RELEASE  "), "case/whitespace insensitive");
    }

    #[test]
    fn test_coordinator_eligible_truth_table() {
        let complex_delegation = "delegate this to the coordinator: research, plan, execute";
        let complex_plain = "research Rust async patterns, then implement a proof of concept, then write tests, and finally document the results";

        // Complex + explicit delegation signal → eligible.
        assert!(coordinator_eligible(0.7, complex_delegation, false, false));
        // Complex WITHOUT delegation signal → NOT eligible (the hijack fix).
        assert!(!coordinator_eligible(0.7, complex_plain, false, false));
        // Delegation signal but sub-threshold complexity → NOT eligible.
        assert!(!coordinator_eligible(0.3, complex_delegation, false, false));
        // Explicit coordinator hint overrides everything.
        assert!(coordinator_eligible(0.0, complex_plain, true, false));
        // Coordinator worker fan-out overrides everything.
        assert!(coordinator_eligible(0.0, complex_plain, false, true));
        // Exactly at the reactive threshold with signal → eligible.
        assert!(coordinator_eligible(0.4, complex_delegation, false, false));
        // Live hijack regressions (J-13 audit 2026-09-13): these intents were
        // routed to coordinator by the pre-WS2 complexity≥0.6 rule and failed
        // with "requires argument(s): task". The gate must keep them out.
        assert!(!coordinator_eligible(
            0.7,
            "create a new branch called feature/vector-tiering",
            false,
            false
        ));
        assert!(!coordinator_eligible(
            0.7,
            "audit docker container configuration for insecure settings",
            false,
            false
        ));
        // Same intents with an explicit delegation ask → eligible.
        assert!(coordinator_eligible(
            0.7,
            "delegate to the coordinator: create a new branch called feature/vector-tiering",
            false,
            false
        ));
    }

    #[test]
    fn test_existing_cascade_scores_unchanged() {
        // The gate changes WHO reaches coordinator, not the scores themselves:
        // existing complexity tests keep passing untouched.
        let s = score_complexity("research Rust async patterns, then implement a proof of concept, then write tests, and finally document the results");
        assert!(s >= 0.6, "scoring behavior must be unchanged by WS2: {s}");
    }
}
