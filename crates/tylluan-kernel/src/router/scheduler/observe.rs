//! Cognitive Scheduler — Phase 3: observation-mode wiring into `tylluan_do`.
//!
//! Exactly the pattern CoherenceGate Layer 4 used at first (see the
//! `coherence_gate_hybrid_enabled` flag in `transport/server/mod.rs`): record
//! the Scheduler's verdict and NOTHING else. The verdict of
//! [`observe_scheduling`] is **never** read back by dispatch — `guild_name`
//! and tool selection flow exactly as before this module existed. Any change
//! to observable routing from this code is a bug, by contract.
//!
//! Integration point: `handler_do/mod.rs`, right after Stage 1 resolves
//! `ResolvedToolCall` and BEFORE the `plan_mode` branch — so both the plan
//! path and the execute path get observed. This is deliberately the END of
//! Stage 1, not the line right after `resolve_guild_name()`: between those
//! two points the kernel may re-route (`bash_execute` override, coloquio
//! tool hints) and re-select the tool. Observing earlier would feed `decide()`
//! pre-dispatch values (e.g. tool `search` before it became `bash_execute`)
//! — verdicts about tools that were never actually called. Every input
//! logged here is real dispatch data.
//!
//! Complexity source: `crate::router::complexity::score_complexity` — the
//! same pure, syntactic scorer the live cascade already uses
//! (`handler_do/routing.rs:56`), recomputed here on the identical
//! `routing_intent` string. This is reuse, not a new heuristic: the design
//! doc (§5, `docs/architecture/DESIGN_cognitive_scheduler.md`) names
//! `complexity.rs` as the score's source, and `score_complexity` is
//! deterministic, so the observed score equals the one the cascade saw.

use tracing::info;

use super::decision::decide;
use super::types::{LatencyClass, RiskTier, SchedulingDecision, TaskContext};

/// Map `registry::tools::RiskLevel` (TOOL_METADATA domain, Low|Medium|High)
/// to the Scheduler's `RiskTier`. The two enums are different contracts (§4/§8
/// of the design doc) and must not be merged. The mapping is exact over the
/// 3-level source domain; note the 4th target tier (`IdempotentWrite`) is
/// unreachable from a 3-level source — a tool's idempotency is not something
/// `TOOL_METADATA` expresses today, so nothing here may fabricate it.
fn risk_level_to_tier(level: crate::registry::tools::RiskLevel) -> RiskTier {
    match level {
        crate::registry::tools::RiskLevel::Low => RiskTier::SafeRead,
        crate::registry::tools::RiskLevel::Medium => RiskTier::StateMutation,
        crate::registry::tools::RiskLevel::High => RiskTier::CriticalDestructive,
    }
}

/// Inferred latency class (DOCUMENTED ASSUMPTION — no measured latency exists
/// at Stage-1 time; real end-to-end latency is Antigravity's TEB workstream,
/// not this one). Heuristic: `plan=true` is the M31-P2 interactive approval
/// path (500ms-class response expected) → `Interactive`; normal dispatch runs
/// guild executions in the 2-8s/embedding class, so `Standard`'s 5s ceiling
/// is the realistic budget. `Instant`/`Deferred` require timing knowledge
/// Stage 1 does not have, so they are intentionally never produced here
/// rather than guessed.
fn infer_latency_class(plan_mode: bool) -> LatencyClass {
    if plan_mode { LatencyClass::Interactive } else { LatencyClass::Standard }
}

/// Build the TaskContext for a live `tylluan_do` Stage-1 dispatch.
///
/// Every field comes from data the dispatcher already resolved — nothing
/// synthesized:
/// - `intent`: the exact intent string entering Stage 1.
/// - `caller_agent_id`: the MCP agent_id, or "anonymous" (the same value the
///   dispatcher already uses in tool_args and notify payloads).
/// - `tool_risk_hint`: resolved via `TylluanServer::check_tool_risk` —
///   kernel tool table → `TOOL_METADATA` → guild descriptor → default
///   `Medium` (logic.rs). `IdempotentWrite` and `allow_remote_mesh=true` /
///   `requires_rollback=false` are documented unknowns at this point:
///   tylluan_do has no per-tool idempotency contract and no mesh-delegation
///   flag in its MCP schema, and a GuildDescriptor's rollback contract is
///   only consulted post-execution (Stage 5), never before dispatch.
/// - `background_budget_available`: read-only snapshot of the shared
///   semaphore (never acquires). A server built without a budget (tests,
///   embedders) reports `true` — "no semaphore exists" must NOT read as
///   "saturated", which would fabricate the RemoteMeshPeer row for every
///   observation.
pub async fn build_task_context(
    server: &crate::transport::server::TylluanServer,
    intent: &str,
    agent_id: Option<&str>,
    tool_name: &str,
    plan_mode: bool,
) -> TaskContext {
    let risk = server.check_tool_risk(tool_name).await;
    TaskContext {
        intent: intent.to_string(),
        caller_agent_id: agent_id.unwrap_or("anonymous").to_string(),
        latency_class: infer_latency_class(plan_mode),
        allow_remote_mesh: false,
        requires_rollback: false,
        tool_risk_hint: Some(risk_level_to_tier(risk)),
        background_budget_available: server
            .background_budget
            .as_ref()
            .map(|b| b.available())
            .unwrap_or(true),
    }
}

/// Everything Stage 1 already resolved about one dispatch. Bundled so the
/// observation call site reads as data, not an argument list.
pub struct SchedulingObservation<'a> {
    pub intent: &'a str,
    /// The `strip_ctx_prefix`ed string the semantic router actually scored.
    pub routing_intent: &'a str,
    pub agent_id: Option<&'a str>,
    pub guild_name: &'a str,
    pub tool_name: &'a str,
    pub routing_trace: &'a [String],
    pub plan_mode: bool,
}

/// Observe a scheduling verdict for one `tylluan_do` dispatch. Observation
/// only: the returned decision is logged (with the routing trace attached)
/// and dropped by the caller — dispatch never reads it. Pure with respect to
/// routing state: reads `check_tool_risk` and the budget snapshot, writes
/// nothing the dispatcher consumes.
pub async fn observe_scheduling(
    server: &crate::transport::server::TylluanServer,
    obs: &SchedulingObservation<'_>,
) -> SchedulingDecision {
    let ctx = build_task_context(server, obs.intent, obs.agent_id, obs.tool_name, obs.plan_mode).await;
    // Same pure scorer the live cascade uses (routing.rs:56), same input
    // string → identical score, no second opinion invented.
    let complexity_score = crate::router::complexity::score_complexity(obs.routing_intent);
    let decision = decide(&ctx, complexity_score);

    info!(
        target: "scheduler::observe",
        intent = %obs.intent,
        agent_id = %ctx.caller_agent_id,
        routed_guild = %obs.guild_name,
        routed_tool = %obs.tool_name,
        routing_trace = ?obs.routing_trace,
        plan_mode = obs.plan_mode,
        tool_risk_hint = ?ctx.tool_risk_hint,
        latency_class = ?ctx.latency_class,
        background_budget_available = ctx.background_budget_available,
        complexity_score = decision.complexity_score,
        verdict = ?decision.execution_class,
        risk_tier = ?decision.risk_tier,
        latency_budget_ms = decision.latency_budget.as_millis() as u64,
        explanation = %decision.explanation,
        OBSERVATION_ONLY = true,
        "scheduler verdict (observation only — dispatch unaffected)"
    );
    decision
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Same construction as handler_do's own `test_server()`: a kernel with
    /// real sovereign-tool resolution available (that is where
    /// `check_tool_risk` resolves from in these tests — real TOOL_METADATA
    /// entries, no stubs, no spawned guild processes).
    async fn kernel_server() -> crate::transport::server::TylluanServer {
        use tokio::sync::{broadcast, RwLock};
        let matcher = crate::router::matcher::GuildMatcher::new(crate::router::catalog::builtin_catalog());
        let (tx, _) = broadcast::channel(16);
        let node_router = crate::memory::agent_nodes::AgentNodeRouter::new(tx);
        let reg = crate::registry::guild_process::GuildRegistry::new(
            std::path::PathBuf::from("."), 300, Default::default(), 3,
        );
        let doctor = std::sync::Arc::new(crate::doctor::Doctor::new(
            std::sync::Arc::new(RwLock::new(reg)),
            std::sync::Arc::new(crate::memory::hybrid::HybridMemory::in_memory().await.unwrap()),
            std::sync::Arc::new(crate::memory::silva::SilvaDB::in_memory().await.unwrap()),
            std::sync::Arc::new(std::sync::Mutex::new(crate::curriculum::CurriculumLearner::new_in_memory(5).unwrap())),
        ));
        crate::transport::server::TylluanServer::new(
            std::sync::Arc::new(RwLock::new(
                crate::registry::guild_process::GuildRegistry::new(
                    std::path::PathBuf::from("."), 300, Default::default(), 3,
                ),
            )),
            std::sync::Arc::new(matcher),
            std::sync::Arc::new(crate::memory::hybrid::HybridMemory::in_memory().await.unwrap()),
            std::sync::Arc::new(crate::memory::silva::SilvaDB::in_memory().await.unwrap()),
            std::sync::Arc::new(crate::memory::mailbox::Mailbox::in_memory().await.unwrap()),
            doctor,
            node_router,
        )
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn real_tyldoan_context_bash_execute_resolves_critical_risk() {
        // The brief's required real case: a destructive-sounding intent routed
        // to the bash guild resolves tool `bash_execute`, whose TOOL_METADATA
        // entry is risk_level High. The scheduler input must reflect REAL
        // data — CriticalDestructive, not a dummy value — and decide() must
        // then emit the risk-dominance verdict (HumanAuthorizationRequired,
        // §3), which in observation mode lands only in logs.
        let server = kernel_server().await;
        let ctx = build_task_context(
            &server, "borra la base de datos de produccion", Some("claude-code"), "bash_execute", false,
        ).await;
        assert_eq!(ctx.intent, "borra la base de datos de produccion");
        assert_eq!(ctx.caller_agent_id, "claude-code");
        assert_eq!(ctx.tool_risk_hint, Some(RiskTier::CriticalDestructive));
        assert_eq!(ctx.latency_class, LatencyClass::Standard);
        let complexity = crate::router::complexity::score_complexity("borra la base de datos de produccion");
        let decision = decide(&ctx, complexity);
        assert!(matches!(
            decision.execution_class,
            super::super::types::ExecutionClass::HumanAuthorizationRequired { .. }
        ));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn real_tyldoan_context_memory_search_resolves_safe_read() {
        // Read-only tool without an agent_id: real TOOL_METADATA Low →
        // SafeRead, and the same "anonymous" fallback the dispatcher uses.
        let server = kernel_server().await;
        let ctx = build_task_context(&server, "busca en memoria el plan del milestone", None, "memory_search", false).await;
        assert_eq!(ctx.caller_agent_id, "anonymous");
        assert_eq!(ctx.tool_risk_hint, Some(RiskTier::SafeRead));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn unknown_tool_defaults_to_state_mutation_conservatively() {
        // check_tool_risk defaults unknown tools to Medium → StateMutation,
        // which matches the decision matrix's own None handling (§5: an
        // unresolved hint is treated as StateMutation, never SafeRead).
        let server = kernel_server().await;
        let ctx = build_task_context(&server, "algo", None, "tool_that_does_not_exist", false).await;
        assert_eq!(ctx.tool_risk_hint, Some(RiskTier::StateMutation));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn plan_mode_infers_interactive_latency_class() {
        let server = kernel_server().await;
        let planned = build_task_context(&server, "lista archivos", None, "read_file", true).await;
        assert_eq!(planned.latency_class, LatencyClass::Interactive);
        let normal = build_task_context(&server, "lista archivos", None, "read_file", false).await;
        assert_eq!(normal.latency_class, LatencyClass::Standard);
    }

    #[test]
    fn risk_level_mapping_is_exact_not_invented() {
        // Exact over the 3-level source domain; IdempotentWrite is
        // deliberately unreachable (documented on risk_level_to_tier).
        assert_eq!(risk_level_to_tier(crate::registry::tools::RiskLevel::Low), RiskTier::SafeRead);
        assert_eq!(risk_level_to_tier(crate::registry::tools::RiskLevel::Medium), RiskTier::StateMutation);
        assert_eq!(risk_level_to_tier(crate::registry::tools::RiskLevel::High), RiskTier::CriticalDestructive);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn observe_scheduling_is_deterministic_and_dispatch_pure() {
        // Two identical observations must produce identical verdicts
        // (determinism), and the observation path takes no routing state it
        // could mutate — its only outputs are the log line and the returned
        // decision the caller drops.
        let server = kernel_server().await;
        let trace = vec!["pre-gate".to_string(), "verb".to_string()];
        let mk_obs = || SchedulingObservation {
            intent: "run git status",
            routing_intent: "run git status",
            agent_id: Some("buffy"),
            guild_name: "bash",
            tool_name: "bash_execute",
            routing_trace: &trace,
            plan_mode: false,
        };
        let d1 = observe_scheduling(&server, &mk_obs()).await;
        let d2 = observe_scheduling(&server, &mk_obs()).await;
        assert_eq!(d1.risk_tier, RiskTier::CriticalDestructive);
        assert_eq!(d1.complexity_score, d2.complexity_score);
        assert_eq!(d1.execution_class, d2.execution_class);
    }
}
