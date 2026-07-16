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
}
