# Cognitive Scheduler: Diseño de Orquestación Multidimensional

**Estado:** PROPUESTA TÉCNICA FORMALIZADA (2026-09-08) — Diseñado por Antigravity (Fase P0 #3 del Roadmap v1.0, ver `EXTERNAL_AUDIT_2026-09-08_v1_roadmap.md`).  
**Fecha:** 2026-09-08  
**Origen:** Auditoría técnica v1.0 (Sección 11 & Fase P0 #3), cruzado con la implementación real de `crates/tylluan-kernel/src/router/complexity.rs`, `catalog.rs` y el contrato G6 `DESIGN_guild_identity_gate.md`.

---

## 1. Problema Real y Diagnóstico del Estado Actual

En la arquitectura actual de Tylluan, la toma de decisiones sobre *cómo*, *dónde* y *con qué garantías* ejecutar una tarea recae exclusivamente en un modelo unidimensional de complejidad sintáctica (`complexity.rs`):

```text
                                  ESTADO ACTUAL (Unidimensional)
                                  
   Intent ──► complexity.rs ──► Score [0.0 - 1.0] ──► Cascade:
                (Sintaxis/MLP)                          ├── < 0.4: Direct Guild
                                                        ├── 0.4..0.6: Reactive Fallback
                                                        └── >= 0.6: Proactive Coordinator
```

### Limitaciones Críticas del Modelo Unidimensional:
1. **Acoplamiento Falso entre Complejidad y Riesgo:**  
   Un comando destructivo simple como *"borra la base de datos de producción y limpia el disco"* tiene un `score_complexity < 0.2` (pocas palabras, sin conectores multi-paso), por lo que se despacha como `Direct Guild` sin ninguna salvaguarda reforzada. A la inversa, una consulta de sólo lectura rica en detalles ("*dame un resumen comparando A y B en los últimos 3 meses*") puntúa `score >= 0.7` y se envía al coordinador pesado.
2. **Ceguera de Presupuesto de Latencia (*Latency Budget*):**  
   No existe noción de cuánto tiempo está dispuesto a esperar el cliente (MCP en vivo vs job de background vs A2A). Un recall interactivo puede disparar una cascada lenta sin advertencia.
3. **Ceguera de Recursos de Cómputo (*Compute Budget / Contention*):**  
   No evalúa si el modelo ONNX está saturado, si la GPU está ocupada por un entrenamiento local (ej. Unsloth), o si un peer de la federación (`tylluan-link`) dispone de aceleración hardware dedicada.
4. **Ausencia de Clases de Ejecución Deterministas:**  
   El routing pasa directo de *"un guild Python"* a *"orquestación multi-agente en coordinator.py"*, sin niveles intermedios como ejecución rápida determinista en Rust (`FAST`), delegación P2P con Noise NK (`REMOTE`), o solicitud obligatoria de autorización humana (`HITL`).

---

## 2. Principios Invariantes del Cognitive Scheduler

1. **Determinismo Estricto en el Despacho:**  
   El Scheduler no invoca un LLM generativo para decidir cómo agendar (lo cual introduciría latencia estocástica y fallos de parseo en el camino crítico). Es una máquina de decisión determinista en Rust, ejecutada en $<0.5\text{ ms}$ con cero asignaciones pesadas.
2. **Precedencia del Riesgo sobre la Complejidad:**  
   El nivel de riesgo y la irreversibilidad de una acción **siempre dominan** sobre la simplicidad sintáctica. Ninguna acción con `RiskLevel::Destructive` puede despacharse por la vía rápida sin pasar por las políticas de autorización correspondientes.
3. **Conciencia de Presupuesto y Degradación Elegante:**  
   Si el cómputo local está saturado o el `LatencyBudget` es estricto, el Scheduler degrada a la mejor aproximación determinista o delega a un nodo peer capacitado, en lugar de bloquear el hilo de ejecución.
4. **Desacoplamiento Total del Hot Path:**  
   Las tareas que no tienen un plazo de respuesta interactivo se agendan como `ExecutionClass::OfflineNightBatch` para ser procesadas en `NightConsolidation`.

---

## 3. Dimensiones Ortogonales del Scheduler

El Cognitive Scheduler evalúa 5 dimensiones independientes antes de emitir una decisión de despacho:

```text
┌──────────────────────────────────────────────────────────────────────────────────┐
│                            DIMENSIONES DE EVALUACIÓN                             │
├───────────────────────┬──────────────────────────────────────────────────────────┤
│ 1. Complejidad        │ Sintáctica (conectores, pasos) + Semántica (entidades)   │
│ 2. Perfil de Riesgo   │ Read / IdempotentWrite / Destructive / Exfiltration      │
│ 3. Latency Budget     │ Presupuesto temporal estricto (Instant / Interactive /   │
│                       │ Batch / Deferred)                                        │
│ 4. Compute Budget     │ Estado de CPU/GPU, contención de mutex ONNX, memoria     │
│ 5. Reversibilidad     │ Rollback-capable (vía undo_last_action) vs Irreversible │
└───────────────────────┴──────────────────────────────────────────────────────────┘
```

### A. Complejidad Cognitiva (`ComplexityScore`)
Refina la base existente de `complexity.rs`:
* **Baja ($<0.35$):** Operación unitaria atómica (un solo verbo, un solo target).
* **Media ($0.35 - 0.65$):** Tarea compuesta con dependencias lineales (ej. "busca X y luego filtra por Y").
* **Alta ($>0.65$):** Tarea no lineal, síntesis abierta, razonamiento dialéctico o planificación multi-herramienta.

### B. Perfil de Riesgo (`RiskProfile`)
Derivado del contrato del guild (`guild.toml` de G6) y de los verbos de intención:
* **`Tier 0 (Safe Read)`:** Búsqueda en memoria, status, lectura de archivos permitidos, métricas.
* **`Tier 1 (Idempotent Write)`:** Guardar memoria temporal, crear logs, actualizar checkpoints locales reversibles.
* **`Tier 2 (State Mutation / External Side-Effect)`:** Modificar archivos de código, compilar, llamadas de red salientes (HTTP/Scrapling).
* **`Tier 3 (Critical / Destructive / Security-Sensitive)`:** Borrado de archivos, ejecución de comandos bash sin sandbox, modificación de claves/tokens, mutación de políticas de acceso.

### C. Presupuesto de Latencia (`LatencyBudget`)
* **`Instant (<50ms)`:** Respuestas de health, lecturas en caché, operaciones deterministas en memoria.
* **`Interactive (<500ms)`:** Consultas de retrieval híbrido (`tylluan_recall`), enrutamiento rápido de guilds.
* **`Standard (<5000ms)`:** Inferencia con modelos locales pequeños (BGE-M3, SLMs ligeros), tool execution compleja.
* **`Deferred / Nightly (Sin límite)`:** Consolidación de memoria, optimización FSRS, análisis de fallos en background.

---

## 4. Estructuras de Datos Canónicas en Rust

Ubicación proyectada: `crates/tylluan-kernel/src/router/scheduler/types.rs`

```rust
use std::time::Duration;
use serde::{Deserialize, Serialize};

/// Perfil de riesgo de la acción propuesta
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RiskTier {
    /// Lectura segura sin mutación de estado
    SafeRead = 0,
    /// Escritura idempotente y reversible
    IdempotentWrite = 1,
    /// Mutación de estado o efecto secundario de red
    StateMutation = 2,
    /// Acción potencialmente destructiva, privilege escalation o borrado
    CriticalDestructive = 3,
}

/// Presupuesto temporal asignado a la tarea
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LatencyClass {
    /// Menor a 50ms (solo estructuras en memoria / cache)
    Instant,
    /// Menor a 500ms (path interactivo MCP)
    Interactive,
    /// Menor a 5s (ejecución estándar de herramientas)
    Standard,
    /// Desacoplado / Asíncrono (sin deadline estricto)
    Deferred,
}

/// Clase de ejecución decidida por el Scheduler
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionClass {
    /// Despacho local determinista ultrarrápido (<20ms, sin subprocesos)
    FastLane { target_handler: String },
    /// Despacho estándar a un Guild FastMCP local
    StandardGuild { guild_id: String, tool_name: String },
    /// Orquestación multi-paso deliberativa vía coordinator.py
    DeliberativeCoordinator { plan_mode: bool },
    /// Requiere aprobación humana explícita antes de ejecutar (HITL Grant)
    HumanAuthorizationRequired { reason: String, suggested_action: String },
    /// Delegación remota P2P vía Noise NK a un peer de la federación
    RemoteMeshPeer { peer_id: String, capability: String },
    /// Tarea encolada para el ciclo de consolidación nocturna
    OfflineNightBatch { job_type: String },
}

/// Contexto integral de planificación recibido por el Scheduler
///
/// Corrección 2026-09-11 (hallazgo verificado del auditor externo, Parte 4):
/// la versión original de este struct no llevaba ninguna señal de riesgo o
/// de presupuesto de recursos como *entrada* -- `RiskTier` solo existía en
/// `SchedulingDecision` (la salida), calculado de la nada. Estos dos campos
/// se añaden apuntando a fuentes que ya existen en producción, no a
/// abstracciones nuevas:
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContext {
    pub intent: String,
    pub caller_agent_id: String,
    pub latency_class: LatencyClass,
    pub allow_remote_mesh: bool,
    pub requires_rollback: bool,
    /// Riesgo por herramienta ya resuelto por `check_tool_risk()` (logic.rs),
    /// que a su vez consulta `TOOL_METADATA` (registry/tools.rs, 151 tools,
    /// decisión de arquitectura en §8). El Scheduler NO debe recalcular esto
    /// -- se limita a recibirlo y dejar que domine sobre `complexity_score`.
    pub tool_risk_hint: Option<RiskTier>,
    /// Snapshot del presupuesto de trabajo de fondo compartido
    /// (`memory/background_budget.rs`, semáforo de 2 permits por defecto).
    /// Permite al Scheduler preferir `OfflineNightBatch`/`RemoteMeshPeer`
    /// sobre `DeliberativeCoordinator` cuando el presupuesto ya está agotado,
    /// en vez de encolar más trabajo pesado sobre el mismo cuello de botella
    /// que causó el incidente GraphRAG (~76% CPU sostenido).
    pub background_budget_available: bool,
}

/// Veredicto completo emitido por el Cognitive Scheduler
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulingDecision {
    pub execution_class: ExecutionClass,
    pub complexity_score: f64,
    pub risk_tier: RiskTier,
    pub latency_budget: Duration,
    pub explanation: String,
}
```

---

## 5. Matriz de Decisión del Scheduler

El algoritmo evalúa la tupla `(ComplexityScore, RiskTier, LatencyClass, SystemLoad)` mediante una tabla de verdad determinista:

```
┌─────────────────┬───────────┬──────────────┬──────────────────────────────┬────────────────────────┐
│ Complexity      │ Risk Tier │ Latency      │ Execution Class              │ Justificación Técnica  │
├─────────────────┼───────────┼──────────────┼──────────────────────────────┼────────────────────────┤
│ Baja (<0.35)    │ SafeRead  │ Instant/Int. │ FastLane                     │ Despacho directo <20ms │
│ Baja (<0.35)    │ Mutation  │ Interactive  │ StandardGuild (Auto-Commit)  │ Ejecución unitaria     │
│ Cualquiera      │ Critical  │ Cualquiera   │ HumanAuthorizationRequired   │ Riesgo domina siempre  │
│ Media (0.35-0.6)│ SafeRead  │ Standard     │ StandardGuild (w/ Fallback)  │ Intento directo + c-brk│
│ Alta (>0.65)    │ Safe/Mut. │ Standard     │ DeliberativeCoordinator      │ Requiere planificación │
│ Alta (>0.65)    │ SafeRead  │ Interactive  │ FastLane / Direct Fallback   │ Latency budget acota   │
│ Cualquiera      │ Safe/Mut. │ Deferred     │ OfflineNightBatch            │ Batch background       │
│ Saturado Local  │ SafeRead  │ Standard     │ RemoteMeshPeer (Noise NK)    │ Offloading federado    │
└─────────────────┴───────────┴──────────────┴──────────────────────────────┴────────────────────────┘
```

---

## 6. Integración con los Subsistemas de Tylluan

```mermaid
flowchart TD
    ClientIntent[MCP / A2A / CLI Client Intent] --> Scheduler[Cognitive Scheduler Engine]
    
    subgraph Inputs [Metadatos y Estado del Sistema]
        Contract[TOOL_METADATA / check_tool_risk: registry/tools.rs] --> Scheduler
        Budget[background_budget snapshot: memory/background_budget.rs] --> Scheduler
        Complexity[Linguistic Scorer: complexity.rs] --> Scheduler
        Caps[Peer Capabilities & Hardware: tylluan-link] --> Scheduler
        Load[System Load & ONNX Locks: HttpState] --> Scheduler
    end
    
    Scheduler --> Decision{ExecutionClass}
    
    Decision -->|FastLane| NativeRust[Native Rust In-Memory Dispatch]
    Decision -->|StandardGuild| PythonGuild[FastMCP Local Guild Process]
    Decision -->|DeliberativeCoordinator| Coord[coordinator.py Multi-Step Engine]
    Decision -->|HumanAuthorizationRequired| HITL[HITL Gate / Grant Store]
    Decision -->|RemoteMeshPeer| P2P[Noise NK TCP Remote Dispatch]
    Decision -->|OfflineNightBatch| Night[NightConsolidation Queue]
```

### A. Conexión con `G6 (Capability Contracts)`
El Scheduler lee `TOOL_METADATA` de `registry/tools.rs` como fuente de `RiskLevel` por tool (decisión verificada en T276). Si un tool tiene `RiskLevel::High`, el Scheduler escala el `RiskTier` a `CriticalDestructive` independientemente de la complejidad sintáctica. Ver sección 8 para la decisión completa de arquitectura de riesgo.

### B. Conexión con `tylluan_do` y Ejecución Transaccional
En `crates/tylluan-kernel/src/transport/server/handler_do.rs`:
1. El handler consulta `scheduler.evaluate(&task_context)`.
2. Si el Scheduler retorna `HumanAuthorizationRequired`, `handler_do` genera un `ActionID` pendiente en el `GrantStore` y devuelve el plan propuesto con status `PENDING_APPROVAL` sin mutar el sistema.
3. Si retorna `FastLane` o `StandardGuild`, ejecuta bajo el contrato transaccional `PLAN -> EXECUTE -> VERIFY -> COMMIT`.

### C. Conexión con `tylluan-link` (Federación P2P)
Si el `ComputeBudget` local está agotado (ej. CPU al 100% o contención de memoria) y la tarea es `SafeRead` con `allow_remote_mesh = true`, el Scheduler consulta el `CapabilityRegistry` de `tylluan-link` y despacha la tarea a un peer remoto vía `execute_remote_tcp()` con cifrado Noise NK de forma completamente transparente.

---

## 7. Plan de Implementación y Criterios de Aceptación (DoD)

### Fase 1: Extracción y Tipado Modular (Semana 1)
*   Crear módulo `crates/tylluan-kernel/src/router/scheduler/`.
*   Migrar y refactorizar las heurísticas de `complexity.rs` hacia el evaluador multidimensional.
*   Implementar `RiskAnalyzer` basado en patrones de verbos y metadatos de guild.
*   *DoD:* 100% de tests unitarios verificando la matriz de decisión (30+ tests).

### Fase 2: Cableado con `handler_do` y `catalog.rs` (Semana 2)
*   Integrar `SchedulingDecision` en el pipeline de ejecución de `handler_do.rs`.
*   Sustituir el bloque cascade hardcodeado de `complexity.rs` por la llamada al nuevo Scheduler.
*   Integrar la validación con los contratos de G6.
*   *DoD:* `cargo check -p tylluan-kernel` y tests E2E de routing pasando en verde.

### Fase 3: Benchmarks de Latencia y Overhead (Semana 3)
*   Añadir benchmark de micro-latencia en `crates/tylluan-evals`: el Scheduler debe resolver cualquier decisión en $<0.5\text{ ms}$ en CPU de referencia.
*   Verificar que ninguna acción destructiva pueda bypassar el `RiskTier::CriticalDestructive`.
*   *DoD:* CI en verde, documentación sincronizada en `STATUS.md` y visualizador.

---

## 8. Decisión de Arquitectura de Riesgo (Revisión G6, 2026-09-09)

**Estado:** Decisión tomada tras revisión cruzada Buffy ↔ José (T276).

### Problema
El diseño original asumía que el riesgo se derivaba de `guild.toml` (un manifiesto por guild que no existe). La auditoría descubrió que **ya existe una fuente de riesgo en producción**: `TOOL_METADATA` en `registry/tools.rs` (25 tools con `RiskLevel::Low/Medium/High`), consumida por `check_tool_risk()` en `logic.rs` con una cadena de resolución de 3 niveles:

```text
1. kernel_tools() → risk field explícito
2. TOOL_METADATA → risk_level por nombre de tool
3. Descripción del tool → parsea "[RISK: HIGH]" o approval="always"
4. Default → Medium
```

### Hallazgos de la revisión

1. **`guild.toml` no existe** — G6 se implementó como `tools/guild_catalog.py` + `tylluan.toml`, no como ficheros `.toml` por guild.
2. **G6 no tiene campo de riesgo** — `GuildEntry` tiene `category` pero no `risk`.
3. **`TOOL_METADATA` ya cubre 25 tools de 9 guilds** — pero ~21 guilds sin ninguna entrada, cayendo a `Medium` por defecto.
4. **El dashboard tiene `detectDestructiveKeywords`** en TypeScript (9 regex) — tercera fuente potencialmente contradictoria.
5. **`complexity.rs` no sabe nada de riesgo** — solo scoring sintáctico.

### Decisión

**El riesgo es por tool, no por guild.** `TOOL_METADATA` ya lo demuestra: dentro del mismo guild `docker`, `docker_exec` es High, `docker_run`/`docker_stop` son Medium, y `docker_ps`/`docker_images`/`docker_status` son Low. Un RiskTier por guild sería demasiado grosero.

**Acciones:**
1. Completar `TOOL_METADATA` para los ~21 guilds sin cobertura (mecánico, mismo patrón).
2. El Cognitive Scheduler debe leer `TOOL_METADATA` como fuente de `RiskLevel`, no inventar su propio mapping por guild.
3. Unificar los 3 niveles actuales (Low/Medium/High) con los 4 tiers del Scheduler (SafeRead/IdempotentWrite/StateMutation/CriticalDestructive) — mapeo natural: Low→SafeRead, Medium→IdempotentWrite/StateMutation, High→CriticalDestructive.
4. `detectDestructiveKeywords` del dashboard se mantiene como capa de presentación (detección por intent en el UI), no como fuente de verdad.

### No-go
- No crear RiskTier por guild como fuente de verdad (ya hay una fuente mejor: por tool).
- No crear `guild.toml` por guild (G6 ya resolvió esto con `guild_catalog.py`).
- No añadir una cuarta fuente de riesgo.

---

## 9. Conclusión

El **Cognitive Scheduler** transforma a Tylluan de un enrutador sintáctico reactivo a un **sistema operativo cognitivo consciente de riesgo, presupuesto y hardware**, resolviendo la deuda de contención y sentando las bases deterministas para Tylluan v1.0.

La fuente de riesgo es `TOOL_METADATA` (por tool, no por guild) — decisión documentada en la sección 8.
