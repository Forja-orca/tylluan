# ADR-018: Decision Fabric — interfaz unificada de decisión tipada (`DecisionProvider`)

- **Estado:** 📐 **DISEÑO aprobado para Fase 0 (spike de medición), NO para integración** — el tech lead aprueba la investigación acotada, la decisión de convertirla en P0/P1 del núcleo queda condicionada a los números del spike.
- **Fecha:** 2026-10-04
- **Origen:** José comparte System One/Jev (TypeSafe) y Laya (receptron/laya, Apache-2.0) como posible pieza faltante. Antigravity escribe el análisis arquitectónico completo en Coloquio. El tech lead da su propia opinión razonada (verdict: cambio de paradigma genuino, condicional a disciplina de interfaz + shadow-mode). Este ADR formaliza ambos en una decisión accionable.
- **Ámbito:** nuevo trait `DecisionProvider` (ubicación propuesta: `crates/tylluan-kernel/src/router/decision/`), consumido inicialmente solo por el harness de medición (Fase 0) — CERO cambios al camino caliente de producción en esta fase.

---

## 0. Problema real (verificado contra código, no contra la auditoría externa)

Esta misma sesión, auditando el estado real de Scheduler/CapabilityRegistry/Resource Governance (ver STATUS.md, ciclo 2026-10-01/04), se confirmó que Tylluan acumula **9 mecanismos de decisión reales e independientes**: Scheduler (`router/scheduler/decision.rs`, matriz de reglas escrita a mano), CoherenceGate, Stigmergy heat, BWC approval, GraphRAG contradiction flagging, NightConsolidation phase gating, reranker-necessity heuristics, HITL triggers, provider selection del Capability Fabric. Cada uno tiene su propia lógica de umbrales/reglas dispersa, no auditable como conjunto, no calibrada, no medible como artefacto único.

Este es exactamente el riesgo sistémico que auditorías previas (09-08, 09-28) ya señalaron como "demasiados mecanismos sin convergencia". La pregunta correcta no es "¿necesitamos otro subsistema?" — es "¿puede una interfaz común de decisión reemplazar la lógica dispersa sin añadir una décima pieza?".

## 1. Qué es System One / Laya (resumen técnico verificado por el equipo, no de oídas)

- **Paradigma**: en vez de `estado → prompt → LLM → tokens → parser → validación → if/else → acción`, el flujo es `estado → preguntas tipadas (Choice/Score/Noul) → modelo decisional → probabilidades → policy → acción`.
- **Laya** (github.com/he-jev/laya, Apache-2.0): arquitectura no autoregresiva, encoder + cabezas de decisión. Checkpoint inglés: ModernBERT-large, 421M. Checkpoint multilingüe (100+ idiomas): mmBERT-base, 322M. Devuelve `choice`/`score`/`noul` en una sola pasada. **Exportable a ONNX** (bundle de 5 archivos, diferencia de logits vs PyTorch ~1e-5 — verificado por el repo, no por nosotros todavía).
- **Propiedad clave para Tylluan**: no genera texto. La salida es dato tipado (`Decision { choice, probability, calibration }`), no conversación — compatible con el principio ya vigente de "cero-LLM en el camino caliente" (ver `router/scheduler/decision.rs`, matriz determinista pura).
- **Cifras publicadas (no verificadas en nuestro hardware)**: Jev cita 70-500ms y mejoras de hasta 2 órdenes de magnitud en tareas System One (TypeSafe reconoce sesgos de su propio benchmark). Laya cita ~33ms en T4. Ninguna cifra es válida para nuestra decisión hasta medirla en CPU real — por eso Fase 0 es un spike de medición, no una integración.

## 2. Decisión de diseño

### 2.1 Contrato: `DecisionProvider`, nunca `LayaProvider`

```rust
pub enum DecisionQuestion {
    Choice { options: Vec<String> },
    Score { levels: Vec<String> },
    Noul, // binario con incertidumbre explícita
}

pub struct DecisionRequest {
    pub state: serde_json::Value,       // contexto arbitrario (task, agent, memory, risk, etc.)
    pub questions: HashMap<String, DecisionQuestion>,
}

pub struct DecisionAnswer {
    pub distribution: HashMap<String, f32>, // opción/nivel -> probabilidad
    pub calibration: Option<CalibrationMeta>, // ECE/Brier del modelo que respondió, si se conoce
}

#[async_trait::async_trait]
pub trait DecisionProvider: Send + Sync {
    async fn decide(&self, req: &DecisionRequest) -> Result<HashMap<String, DecisionAnswer>, DecisionError>;
    fn provider_id(&self) -> &'static str; // "laya-onnx-v1", "rules-v0", "qwen-small", etc.
}
```

Implementaciones posibles detrás del trait (ninguna obligatoria hoy): `RulesDecisionProvider` (envuelve la lógica actual de `decision.rs` sin cambiarla — baseline A del spike), `LayaOnnxDecisionProvider` (Fase 0), `SmallLlmDecisionProvider` (baseline B del spike). El kernel nunca referencia `Laya` directamente fuera de su propia implementación — si mañana aparece un modelo mejor, se sustituye sin tocar ningún call-site.

### 2.2 Invariantes no negociables

1. **El modelo nunca tiene autoridad directa.** `DecisionAnswer` es un insumo de la política existente (Scheduler, CoherenceGate, etc.), nunca ejecuta una acción por sí mismo.
2. **Shadow-mode obligatorio antes de cualquier cutover** — mismo patrón que el propio Scheduler usa hoy (WS3, modo observación pura). Ningún `DecisionProvider` puede influir una decisión real de producción sin haber corrido en modo sombra el tiempo suficiente para medir su propia calibración en vivo, no solo offline.
3. **Cero cambio al camino caliente en Fase 0.** El spike es un harness aislado (`examples/decision_fabric_spike.rs`, mismo patrón que `p2_measurement.rs` de Buffy) sobre datos reales exportados, no un flag de producción.

## 3. Fase 0 — Decision Fabric Spike (lo único aprobado para ejecutar ahora)

**Dataset**: ~10,000 decisiones reales extraídas de logs/DB de producción existentes (`guild_audit_log`, `scheduler_confusion` ya tiene el emparejamiento decisión-real vs regla — reusar, no reinventar), cubriendo: routing, memory relevance, reranker necessity, risk tier, HITL trigger, provider selection.

**Comparar**: (A) reglas actuales (baseline, cero cambio), (B) LLM pequeño ya disponible localmente, (C) Laya vía ONNX Runtime, (D) Laya + reglas como fallback de baja confianza, (E) Laya + fallback a LLM grande en casos ambiguos.

**Métricas**: accuracy, Brier score, ECE, AUROC, falsos positivos/negativos, p50/p99 de latencia, CPU time, RSS del modelo, y — la métrica de ROI real — **llamadas a LLM evitadas**.

**Gate de aceptación** (antes de considerar Fase 1/integración): Laya (o variante D/E) debe superar la baseline de reglas en accuracy/calibración SIN degradar p99, y debe demostrar que el coste de su propia inferencia + RSS es menor que el coste de las llamadas a LLM que sustituye. Si no pasa este gate, se cierra como NO-GO con dato — mismo criterio de rigor que ya aplicamos a P0-2/reranker A-B.

## 4. Qué NO se hace en este ADR

- No se integra ningún `DecisionProvider` en el camino caliente de producción.
- No se reemplaza `router/scheduler/decision.rs` — el spike lo usa como baseline A, intacto.
- No se añade una décima pieza arquitectónica "porque sí" — si el spike no pasa su gate, Laya/System One queda documentado como investigado y descartado con dato, igual que P0-2.

## 5. Consecuencias

- **Positiva si pasa el gate**: unifica 9 mecanismos de decisión dispersos bajo un contrato medible y calibrado, reduce llamadas a LLM reales, y convierte juicio heurístico implícito en un artefacto auditable (Brier/ECE) — coherente con la cultura de verificación mecánica ya exigida para STATUS.md/tests.
- **Negativa si se hace mal**: una décima pieza que agrava la fragmentación que las auditorías ya señalan como riesgo, y una fuente nueva de opacidad (un clasificador no es tan auditable como una regla escrita a mano) si se omite el shadow-mode.
- **Rollback**: Fase 0 es un harness aislado sin flag de producción — no hay nada que revertir si falla.
