//! Canonical data types for the Cognitive Scheduler.
//!
//! Phase 1 only: these are the types from `docs/architecture/DESIGN_cognitive_scheduler.md`
//! §4, transcribed verbatim into real Rust. **Nothing wires these into dispatch yet** —
//! that is a deliberately separate, later task. This phase exists so the design's own
//! data contract can be type-checked and tested before any decision logic is built on
//! top of it.
//!
//! `TaskContext.tool_risk_hint` and `.background_budget_available` are the two fields
//! the external auditor's Parte 4 review flagged as missing from the original design —
//! `RiskTier` only existed in `SchedulingDecision` (the output), never as an input the
//! Scheduler could actually receive. See §4 of the design doc for the full rationale.

use std::time::Duration;
use serde::{Deserialize, Serialize};

/// Perfil de riesgo de la acción propuesta.
///
/// `Ord` is derived deliberately: risk must dominate over complexity in any
/// decision matrix built on top of this type (§3 of the design doc), and a
/// derived `Ord` gives that comparison for free and correctly, in declaration
/// order (`SafeRead < IdempotentWrite < StateMutation < CriticalDestructive`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RiskTier {
    /// Lectura segura sin mutación de estado.
    SafeRead,
    /// Escritura idempotente y reversible.
    IdempotentWrite,
    /// Mutación de estado o efecto secundario de red.
    StateMutation,
    /// Acción potencialmente destructiva, privilege escalation o borrado.
    CriticalDestructive,
}

/// Presupuesto temporal asignado a la tarea.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LatencyClass {
    /// Menor a 50ms (solo estructuras en memoria / cache).
    Instant,
    /// Menor a 500ms (path interactivo MCP).
    Interactive,
    /// Menor a 5s (ejecución estándar de herramientas).
    Standard,
    /// Desacoplado / Asíncrono (sin deadline estricto).
    Deferred,
}

/// Clase de ejecución decidida por el Scheduler.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionClass {
    /// Despacho local determinista ultrarrápido (<20ms, sin subprocesos).
    FastLane { target_handler: String },
    /// Despacho estándar a un Guild FastMCP local.
    StandardGuild { guild_id: String, tool_name: String },
    /// Orquestación multi-paso deliberativa vía coordinator.py.
    DeliberativeCoordinator { plan_mode: bool },
    /// Requiere aprobación humana explícita antes de ejecutar (HITL Grant).
    HumanAuthorizationRequired { reason: String, suggested_action: String },
    /// Delegación remota P2P vía Noise NK a un peer de la federación.
    RemoteMeshPeer { peer_id: String, capability: String },
    /// Tarea encolada para el ciclo de consolidación nocturna.
    OfflineNightBatch { job_type: String },
}

/// Contexto integral de planificación recibido por el Scheduler.
///
/// Corrección 2026-09-11 (hallazgo verificado del auditor externo, Parte 4):
/// la versión original de este struct no llevaba ninguna señal de riesgo o
/// de presupuesto de recursos como *entrada* — `RiskTier` solo existía en
/// `SchedulingDecision` (la salida), calculado de la nada. Los dos últimos
/// campos apuntan a fuentes que ya existen en producción, no a abstracciones
/// nuevas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContext {
    pub intent: String,
    pub caller_agent_id: String,
    pub latency_class: LatencyClass,
    pub allow_remote_mesh: bool,
    pub requires_rollback: bool,
    /// Riesgo por herramienta ya resuelto por `check_tool_risk()` (logic.rs),
    /// que a su vez consulta `TOOL_METADATA` (registry/tools.rs, 151 tools,
    /// decisión de arquitectura en §8 del diseño). El Scheduler NO debe
    /// recalcular esto — se limita a recibirlo y dejar que domine sobre
    /// `complexity_score`.
    pub tool_risk_hint: Option<RiskTier>,
    /// Snapshot del presupuesto de trabajo de fondo compartido
    /// (`memory/background_budget.rs`, semáforo de 2 permits por defecto).
    /// Permite preferir `OfflineNightBatch`/`RemoteMeshPeer` sobre
    /// `DeliberativeCoordinator` cuando el presupuesto ya está agotado, en
    /// vez de encolar más trabajo pesado sobre el mismo cuello de botella
    /// que causó el incidente GraphRAG (~76% CPU sostenido).
    pub background_budget_available: bool,
}

/// Veredicto completo emitido por el Cognitive Scheduler.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulingDecision {
    pub execution_class: ExecutionClass,
    pub complexity_score: f64,
    pub risk_tier: RiskTier,
    pub latency_budget: Duration,
    pub explanation: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn risk_tier_ordering_matches_design_doc_dominance_rule() {
        // §3: risk must always dominate over complexity. A derived Ord in
        // declaration order is what makes that comparison trivial and correct
        // for any decision logic built on top of this type later.
        assert!(RiskTier::SafeRead < RiskTier::IdempotentWrite);
        assert!(RiskTier::IdempotentWrite < RiskTier::StateMutation);
        assert!(RiskTier::StateMutation < RiskTier::CriticalDestructive);
        assert_eq!(
            [RiskTier::CriticalDestructive, RiskTier::SafeRead, RiskTier::StateMutation]
                .iter()
                .max()
                .copied(),
            Some(RiskTier::CriticalDestructive)
        );
    }

    #[test]
    fn task_context_round_trips_through_json() {
        let ctx = TaskContext {
            intent: "lee el archivo README.md".to_string(),
            caller_agent_id: "claude-code".to_string(),
            latency_class: LatencyClass::Interactive,
            allow_remote_mesh: false,
            requires_rollback: false,
            tool_risk_hint: Some(RiskTier::SafeRead),
            background_budget_available: true,
        };
        let json = serde_json::to_string(&ctx).expect("serialize TaskContext");
        let restored: TaskContext = serde_json::from_str(&json).expect("deserialize TaskContext");
        assert_eq!(restored.caller_agent_id, "claude-code");
        assert_eq!(restored.tool_risk_hint, Some(RiskTier::SafeRead));
        assert!(restored.background_budget_available);
    }

    #[test]
    fn task_context_tool_risk_hint_defaults_to_none_when_absent() {
        // A caller that hasn't run check_tool_risk() yet (or a tool with no
        // TOOL_METADATA entry) must be representable without inventing a risk
        // level -- None, not a fabricated Low/Medium default at this layer.
        let ctx = TaskContext {
            intent: "".to_string(),
            caller_agent_id: "".to_string(),
            latency_class: LatencyClass::Standard,
            allow_remote_mesh: false,
            requires_rollback: false,
            tool_risk_hint: None,
            background_budget_available: false,
        };
        assert_eq!(ctx.tool_risk_hint, None);
    }

    #[test]
    fn scheduling_decision_serializes_execution_class_variants() {
        let decision = SchedulingDecision {
            execution_class: ExecutionClass::HumanAuthorizationRequired {
                reason: "destructive action requested".to_string(),
                suggested_action: "review and approve via approve_action".to_string(),
            },
            complexity_score: 0.12,
            risk_tier: RiskTier::CriticalDestructive,
            latency_budget: Duration::from_millis(500),
            explanation: "risk dominates syntactic complexity (§3)".to_string(),
        };
        let json = serde_json::to_string(&decision).expect("serialize SchedulingDecision");
        assert!(json.contains("HumanAuthorizationRequired"));
        assert!(json.contains("CriticalDestructive"));
    }
}
