//! Cognitive Scheduler — Phase 2: the decision matrix (§5 of the design doc).
//!
//! `decide()` is a pure function: `(TaskContext, complexity_score) -> SchedulingDecision`,
//! with no I/O and no side effects, so every row of the design's truth table can be
//! tested in isolation before anything gets wired to `handler_do`. That wiring is a
//! deliberately separate, later step (per this cycle's task assignment).

use std::time::Duration;

use super::types::{ExecutionClass, LatencyClass, RiskTier, SchedulingDecision, TaskContext};

/// §3: risk must always dominate over syntactic complexity. A `CriticalDestructive`
/// tool hint short-circuits every other rule, regardless of how simple the intent's
/// wording looks -- this is what stops a short, low-complexity destructive command
/// ("borra la base de datos de producción") from ever reaching a fast, unguarded path.
fn risk_dominates(ctx: &TaskContext) -> Option<SchedulingDecision> {
    if ctx.tool_risk_hint == Some(RiskTier::CriticalDestructive) {
        return Some(SchedulingDecision {
            execution_class: ExecutionClass::HumanAuthorizationRequired {
                reason: "tool_risk_hint is CriticalDestructive".to_string(),
                suggested_action: "review and approve via approve_action".to_string(),
            },
            complexity_score: 0.0, // irrelevant here -- risk alone decided this
            risk_tier: RiskTier::CriticalDestructive,
            latency_budget: latency_budget_for(ctx.latency_class),
            explanation: "risk dominates syntactic complexity (§3): critical risk always \
                requires human authorization, independent of complexity_score."
                .to_string(),
        });
    }
    None
}

/// Real, fixed latency ceilings from §4's `LatencyClass` doc comments.
fn latency_budget_for(class: LatencyClass) -> Duration {
    match class {
        LatencyClass::Instant => Duration::from_millis(50),
        LatencyClass::Interactive => Duration::from_millis(500),
        LatencyClass::Standard => Duration::from_secs(5),
        // "sin deadline estricto" -- an hour is a documented, generous ceiling for
        // background/night work, not a claim that anything actually takes that long.
        LatencyClass::Deferred => Duration::from_secs(3600),
    }
}

/// A `tool_risk_hint` of `None` means no `TOOL_METADATA` entry resolved a risk level
/// for this tool (§8: ~21 of 46 guilds still lack full coverage as of this cycle).
/// Treating an *unknown* risk as `SafeRead` would be the wrong direction to guess in
/// for a scheduler whose whole premise is "risk dominates" -- so an unresolved hint
/// is treated as the same tier as `StateMutation` for decision purposes: conservative,
/// but not maximally so (that would make every unclassified tool require human
/// authorization, which `TOOL_METADATA`'s own default-to-`Medium` fallback chain in
/// `check_tool_risk()` already deliberately avoids).
fn effective_risk(ctx: &TaskContext) -> RiskTier {
    ctx.tool_risk_hint.unwrap_or(RiskTier::StateMutation)
}

/// The §5 decision matrix. Evaluated top-to-bottom, first match wins -- mirrors the
/// design doc's own table order, with the risk-dominance short-circuit (§3) always
/// checked first since the doc states it must win regardless of row order below it.
pub fn decide(ctx: &TaskContext, complexity_score: f64) -> SchedulingDecision {
    if let Some(decision) = risk_dominates(ctx) {
        return decision;
    }

    let risk = effective_risk(ctx);
    let latency_budget = latency_budget_for(ctx.latency_class);

    // "Cualquiera / Deferred → OfflineNightBatch" -- a deferred latency class always
    // routes to the night batch queue, independent of complexity or risk (short of
    // the CriticalDestructive short-circuit already handled above).
    if ctx.latency_class == LatencyClass::Deferred {
        return SchedulingDecision {
            execution_class: ExecutionClass::OfflineNightBatch {
                job_type: "scheduler-deferred".to_string(),
            },
            complexity_score,
            risk_tier: risk,
            latency_budget,
            explanation: "latency_class=Deferred routes to the night consolidation queue \
                regardless of complexity or non-critical risk (§5)."
                .to_string(),
        };
    }

    // "Saturado Local / SafeRead / Standard → RemoteMeshPeer" -- only offered when the
    // task actually allows remote delegation; a caller that set allow_remote_mesh=false
    // must never be silently routed off-box.
    if !ctx.background_budget_available
        && risk == RiskTier::SafeRead
        && ctx.latency_class == LatencyClass::Standard
        && ctx.allow_remote_mesh
    {
        return SchedulingDecision {
            execution_class: ExecutionClass::RemoteMeshPeer {
                peer_id: String::new(), // resolved by the mesh dispatcher, not the Scheduler
                capability: "guild-dispatch".to_string(),
            },
            complexity_score,
            risk_tier: risk,
            latency_budget,
            explanation: "background_budget exhausted locally, safe-read + standard latency \
                + remote mesh allowed -> offload instead of queuing onto the same \
                bottleneck that caused the GraphRAG incident (§5)."
                .to_string(),
        };
    }

    let execution_class = match (complexity_score, risk, ctx.latency_class) {
        // Baja (<0.35) / SafeRead / Instant|Interactive -> FastLane
        (c, RiskTier::SafeRead, LatencyClass::Instant | LatencyClass::Interactive) if c < 0.35 => {
            ExecutionClass::FastLane {
                target_handler: "direct".to_string(),
            }
        }
        // Baja (<0.35) / Mutation / Interactive -> StandardGuild (auto-commit)
        (c, RiskTier::IdempotentWrite | RiskTier::StateMutation, LatencyClass::Interactive)
            if c < 0.35 =>
        {
            ExecutionClass::StandardGuild {
                guild_id: String::new(),
                tool_name: String::new(),
            }
        }
        // Media (0.35-0.6) / SafeRead / Standard -> StandardGuild (con fallback implícito
        // en el dispatcher real, no en este veredicto)
        (c, RiskTier::SafeRead, LatencyClass::Standard) if (0.35..=0.6).contains(&c) => {
            ExecutionClass::StandardGuild {
                guild_id: String::new(),
                tool_name: String::new(),
            }
        }
        // Alta (>0.65) / Safe|Mutation / Standard -> DeliberativeCoordinator
        (
            c,
            RiskTier::SafeRead | RiskTier::IdempotentWrite | RiskTier::StateMutation,
            LatencyClass::Standard,
        ) if c > 0.65 => ExecutionClass::DeliberativeCoordinator { plan_mode: false },
        // Alta (>0.65) / SafeRead / Interactive -> FastLane (fallback directo, presupuesto
        // de latencia manda sobre la complejidad medida)
        (c, RiskTier::SafeRead, LatencyClass::Interactive) if c > 0.65 => ExecutionClass::FastLane {
            target_handler: "direct-fallback".to_string(),
        },
        // Todo lo no cubierto explícitamente por la tabla del diseño cae al guild
        // estándar -- nunca a un atajo no especificado, y nunca silenciosamente
        // a HumanAuthorizationRequired (eso ya se decidió arriba si aplicaba).
        _ => ExecutionClass::StandardGuild {
            guild_id: String::new(),
            tool_name: String::new(),
        },
    };

    SchedulingDecision {
        execution_class,
        complexity_score,
        risk_tier: risk,
        latency_budget,
        explanation: "matched §5 decision matrix row (see router::scheduler::decision::decide \
            for the ordered rule list)."
            .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(latency_class: LatencyClass, risk: Option<RiskTier>, budget_available: bool, allow_remote: bool) -> TaskContext {
        TaskContext {
            intent: "test intent".to_string(),
            caller_agent_id: "test-agent".to_string(),
            latency_class,
            allow_remote_mesh: allow_remote,
            requires_rollback: false,
            tool_risk_hint: risk,
            background_budget_available: budget_available,
        }
    }

    #[test]
    fn critical_risk_always_requires_human_authorization_regardless_of_complexity() {
        // Row: "Cualquiera / Critical / Cualquiera -> HumanAuthorizationRequired".
        // Deliberately pick complexity_score=0.05 (lowest) to prove risk truly
        // dominates -- a short, simple-looking destructive command must not slip
        // through a fast path.
        let c = ctx(LatencyClass::Instant, Some(RiskTier::CriticalDestructive), true, true);
        let decision = decide(&c, 0.05);
        assert!(matches!(
            decision.execution_class,
            ExecutionClass::HumanAuthorizationRequired { .. }
        ));
        assert_eq!(decision.risk_tier, RiskTier::CriticalDestructive);
    }

    #[test]
    fn low_complexity_safe_read_instant_goes_fast_lane() {
        let c = ctx(LatencyClass::Instant, Some(RiskTier::SafeRead), true, false);
        let decision = decide(&c, 0.1);
        assert!(matches!(decision.execution_class, ExecutionClass::FastLane { .. }));
    }

    #[test]
    fn low_complexity_mutation_interactive_goes_standard_guild() {
        let c = ctx(LatencyClass::Interactive, Some(RiskTier::IdempotentWrite), true, false);
        let decision = decide(&c, 0.2);
        assert!(matches!(decision.execution_class, ExecutionClass::StandardGuild { .. }));
    }

    #[test]
    fn medium_complexity_safe_read_standard_goes_standard_guild() {
        let c = ctx(LatencyClass::Standard, Some(RiskTier::SafeRead), true, false);
        let decision = decide(&c, 0.5);
        assert!(matches!(decision.execution_class, ExecutionClass::StandardGuild { .. }));
    }

    #[test]
    fn high_complexity_standard_latency_goes_deliberative_coordinator() {
        let c = ctx(LatencyClass::Standard, Some(RiskTier::SafeRead), true, false);
        let decision = decide(&c, 0.8);
        assert!(matches!(
            decision.execution_class,
            ExecutionClass::DeliberativeCoordinator { .. }
        ));
    }

    #[test]
    fn high_complexity_safe_read_interactive_falls_back_to_fast_lane() {
        // Latency budget dominates a merely-high complexity score when the caller
        // demands an interactive response.
        let c = ctx(LatencyClass::Interactive, Some(RiskTier::SafeRead), true, false);
        let decision = decide(&c, 0.9);
        assert!(matches!(decision.execution_class, ExecutionClass::FastLane { .. }));
    }

    #[test]
    fn deferred_latency_always_goes_offline_night_batch() {
        let c = ctx(LatencyClass::Deferred, Some(RiskTier::StateMutation), true, false);
        let decision = decide(&c, 0.99);
        assert!(matches!(
            decision.execution_class,
            ExecutionClass::OfflineNightBatch { .. }
        ));
    }

    #[test]
    fn deferred_latency_dominates_even_with_low_complexity_and_no_risk_hint() {
        let c = ctx(LatencyClass::Deferred, None, true, false);
        let decision = decide(&c, 0.01);
        assert!(matches!(
            decision.execution_class,
            ExecutionClass::OfflineNightBatch { .. }
        ));
    }

    #[test]
    fn local_saturation_with_safe_read_standard_and_remote_allowed_offloads_to_mesh() {
        let c = ctx(LatencyClass::Standard, Some(RiskTier::SafeRead), false, true);
        let decision = decide(&c, 0.5);
        assert!(matches!(
            decision.execution_class,
            ExecutionClass::RemoteMeshPeer { .. }
        ));
    }

    #[test]
    fn local_saturation_without_remote_mesh_permission_never_offloads() {
        // Same saturated/safe-read/standard combination as the test above, but
        // allow_remote_mesh=false -- must NOT silently route off-box.
        let c = ctx(LatencyClass::Standard, Some(RiskTier::SafeRead), false, false);
        let decision = decide(&c, 0.5);
        assert!(!matches!(decision.execution_class, ExecutionClass::RemoteMeshPeer { .. }));
    }

    #[test]
    fn unresolved_risk_hint_is_treated_as_state_mutation_not_safe_read() {
        // No TOOL_METADATA entry resolved (tool_risk_hint: None) must never be
        // silently treated as the least-risky tier.
        let c = ctx(LatencyClass::Interactive, None, true, false);
        let decision = decide(&c, 0.1);
        assert_eq!(decision.risk_tier, RiskTier::StateMutation);
    }

    #[test]
    fn latency_budget_matches_documented_ceilings() {
        assert_eq!(latency_budget_for(LatencyClass::Instant), Duration::from_millis(50));
        assert_eq!(latency_budget_for(LatencyClass::Interactive), Duration::from_millis(500));
        assert_eq!(latency_budget_for(LatencyClass::Standard), Duration::from_secs(5));
        assert_eq!(latency_budget_for(LatencyClass::Deferred), Duration::from_secs(3600));
    }
}
