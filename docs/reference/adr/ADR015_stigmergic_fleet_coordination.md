# ADR-015 — Stigmergic Fleet Coordination: coordinación emergente por calor semántico y huellas de trabajo

**Status:** Proposed (pendiente de validación del Tech Lead — Claude Code). Documento de diseño formal — **cero cambios de kernel** hasta revisión línea a línea del TL.
**Date:** 2026-09-22
**Authors:** Antigravity (documento y formalización de diseño; directiva del Tech Lead Claude Code en T713, verificación técnica previa de Buffy en T712; principio bio-inspirado enunciado por José)
**Depends on:** ADR-009 (contrato declarativo de agentes), ADR-013 (contrato de no-depredación), ADR-014 (tejido de capacidades y scheduler cognitivo), WORK_PROTOCOL.md §7 (observabilidad, techos de tiempo y resiliencia en tareas programadas), CONTRACT-01 (exactamente 5 tools soberanos)
**Implements:** Coordinación emergente y descentralizada de flota multi-agente basada en rastros estigmérgicos, extendiendo el sustrato matemático preexistente en SilvaDB (`crates/tylluan-kernel/src/memory/silva/decay.rs`, `schema.rs`) a la asignación asíncrona de atención y zonas de trabajo.

---

## Context

La orquestación centralizada tradicional en sistemas multi-agente adolece de fragilidades estructurales críticas:
1. **Cuello de botella de coordinación síncrona**: Depender de un orquestador que centraliza todas las decisiones genera latencia, sobrecoste exponencial en tokens y vulnerabilidad a fallos catastróficos por punto único de fallo (SPOF).
2. **Amplificación de errores y cascada**: En enjambres sin compuertas rígidas de validación, la propagación descontrolada de alucinaciones o errores de contexto puede escalar hasta 17.2x (frente a 4.4x con arbitraje estructurado).
3. **Bloqueos operativos en runtimes heterogéneos**: El incidente de la flota del 2026-09-22 (donde procesos huérfanos de CLI bloquearon el loop autónomo durante horas) demostró que el acoplamiento rígido entre procesos debe ser sustituido por bucles desacoplados con observación indirecta a través del entorno.

### El Principio Bio-Inspirado (Stigmergy)
En la naturaleza (colonias de hormigas, termitas, sistemas de moho mucilaginoso), la coordinación a gran escala no se logra mediante llamadas síncronas de mando, sino mediante **estigmergia**: modificación y lectura de trazas físicas y químicas dejadas en el entorno compartido (Pierre-Paul Grassé, 1959; Bonabeau et al., 1999).

En Tylluan, el sustrato estigmérgico ya es una realidad operativa en el kernel Rust (`crates/tylluan-kernel/src/memory/silva/decay.rs`), implementado con rigor matemático y empírico:
- **Depósito activo (`touch_node`)**: 11 puntos de llamada en MCP tools soberanos (`tylluan_do`, `tylluan_recall`, `tylluan_remember`, `tylluan_think`, `tylluan_graph`), `api_guilds` y enrutamiento registran interacciones en la tabla `node_traces`.
- **Propagación topológica y semántica (`diffuse_heat`)**: Acceder a un nodo difunde calor a sus vecinos de grafo (1-hop, hasta 10 nodos) y a nodos con similitud vectorial semántica (coseno > 0.85, 1 a 3 trazas proporcionales).
- **Lectura normalizada (`get_stigmergy_heat`)**: Cálculo de calor relativo en ventanas temporales (`heat = count / (hours * 10.0)`, con límite superior de 2.0).
- **Evaporación y mantenimiento (`prune_old_traces`)**: Purga periódica de trazas con antigüedad mayor a `keep_days` para evitar la saturación del medio.

Este ADR formaliza la extensión de este mecanismo bio-inspirado para resolver la coordinación de trabajo y atención de la flota completa sin violar los invariantes soberanos de Tylluan.

---

## Non-goals

- **No añadir herramientas MCP adicionales**: CONTRACT-01 es sagrado e inviolable (exactamente 5 sovereign tools: `tylluan_do`, `tylluan_remember`, `tylluan_recall`, `tylluan_think`, `tylluan_graph`). La estigmergia opera en la infraestructura del kernel y del middleware, nunca como una sexta herramienta visible.
- **No permitir ejecución destructiva ciega**: La estigmergia es un mecanismo de **enrutamiento de atención y descubrimiento de trabajo** ("¿qué zona requiere inspección/consolidación?"), JAMÁS un permiso de ejecución autónoma sin compuertas.
- **No reemplazar a Coloquio**: La base de datos de Coloquio (`data/mailbox.db`) sigue siendo el registro inmutable y auditable de comunicación explícita, decisiones y arbitraje humano/TL. La estigmergia proporciona la capa de prioridad y resonancia sobre dicho registro.
- **Cero código Rust en este ADR**: Este documento define la arquitectura, las fórmulas y los límites. La implementación de kernel será ejecutada en un ciclo dedicado por Deep tras la aprobación de este ADR.

---

## Design

```
┌────────────────────────────────────────────────────────────────────────┐
│                        TYLLUAN STIGMERGIC FABRIC                       │
├────────────────────────────────────────────────────────────────────────┤
│                                                                        │
│   [ Agente A ] ──touch──► ┌──────────────────────┐ ◄──touch── [ Agente B ] │
│                           │    SHARED MEDIUM     │                     │
│   [ Agente C ] ──touch──► │  (SilvaDB + Traces)  │ ◄──touch── [ Agent D ]  │
│                           └──────────┬───────────┘                     │
│                                      │                                 │
│                         ┌────────────┴────────────┐                    │
│                         ▼                         ▼                    │
│                 [ Semantic Heat ]        [ Workprints Heat ]           │
│                 (Memory Nodes)           (Zones / Modules)             │
│                         │                         │                    │
│                         └────────────┬────────────┘                    │
│                                      │                                 │
│                                      ▼                                 │
│                          ATTENTION & TASK ROUTING                      │
│                                      │                                 │
│                                      ▼                                 │
│                   ┌──────────────────────────────────────┐             │
│                   │        STRICT VALIDATION GATES       │             │
│                   │  - ADR-013 (Non-Predation Gate)      │             │
│                   │  - WORK_PROTOCOL §7 (Observability)  │             │
│                   │  - Canonical CI Suite (verify.sh)    │             │
│                   └──────────────────────────────────────┘             │
│                                                                        │
└────────────────────────────────────────────────────────────────────────┘
```

### 1. Extensión de Entidades Estigmérgicas: De Nodos a Zonas de Trabajo (`Workprints`)

Actualmente, las trazas estigmérgicas se registran exclusivamente a nivel de nodos de memoria (`node_traces.node_id`). La coordinación de flota requiere extender la señal a tres niveles de granularidad:

| Nivel | Identificador de Rastreo | Propósito | Ejemplo |
|---|---|---|---|
| **Nivel 1 (Memoria)** | `node_id` (UUID o URI) | Consolidación semántica, FSRS review, retención. | `doc:memory_lifecycle` |
| **Nivel 2 (Zonas de Código)** | `zone_id` (`zone:<subsystem>`) | Detección de zonas de alto tráfico, contención o abandono. | `zone:crates/tylluan-kernel/transport` |
| **Nivel 3 (Tareas Coloquio)** | `task_id` (`task:<turn_or_slug>`) | Temperatura de debate, resolución de bloqueos y pull autónomo. | `task:p1_recall_c8_deep` |

#### Mecánica de Registro Unificada
Sin romper el esquema relacional de SQLite, la tabla `node_traces` (`schema.rs:408`) o su extensión `work_traces` registrará los eventos con metadatos de autoría y tipo:
```sql
CREATE TABLE IF NOT EXISTS work_traces (
    trace_id INTEGER PRIMARY KEY AUTOINCREMENT,
    target_uri TEXT NOT NULL,         -- node_id, zone_id o task_id
    target_kind TEXT NOT NULL,        -- 'memory', 'zone', 'task'
    agent_id TEXT NOT NULL,          -- 'deep', 'buffy', 'claude-code', 'antigravity', 'jose'
    trace_type TEXT NOT NULL,        -- 'read', 'write', 'verify', 'block', 'diffuse'
    weight REAL DEFAULT 1.0,         -- Intensidad de la señal
    touched_at INTEGER NOT NULL      -- Timestamp Unix en segundos
);
CREATE INDEX IF NOT EXISTS idx_work_traces_target ON work_traces(target_uri, touched_at DESC);
CREATE INDEX IF NOT EXISTS idx_work_traces_agent ON work_traces(agent_id, touched_at DESC);
```

---

### 2. Dinámica del Calor: Normalización Lineal por Ventana vs Decaimiento Exponencial

Uno de los matices clave señalados por Buffy (T712) y el TL Claude Code (T713) es la función de decaimiento matemático del calor.

#### A. Modelo Actual (Lineal por Ventana de Tiempo)
El kernel actual calcula el calor como:
$$\text{Heat}_{\text{window}}(x) = \min\left(2.0, \frac{\text{count}_{\text{window}}(x)}{10 \cdot \Delta t_{\text{hours}}}\right)$$
- **Ventaja**: Computacionalmente casi instantáneo en SQLite (`COUNT(*) WHERE touched_at >= now - window`). O(1) con índice B-Tree.
- **Limitación**: Señal "en escalón" (un rastro ocurrido hace 59 minutos tiene el mismo peso que uno de hace 10 segundos, pero desaparece bruscamente al minuto 61).

#### B. Modelo de Decaimiento Continuo Exponencial (Bio-Inspirado)
El decaimiento biológico de feromonas sigue una ley de decaimiento exponencial continuo:
$$H(t) = \sum_{i=1}^{N} w_i \cdot e^{-\lambda (t - t_i)}$$
Donde:
- $t_i$: Instante de depósito de la huella $i$.
- $w_i$: Peso del rastro (1.0 para accesos directos, 0.3 para difusión semántica).
- $\lambda$: Constante de evaporación ($\lambda = \frac{\ln 2}{T_{1/2}}$). Para tareas de ingeniería, la vida media de atención $T_{1/2}$ óptima se sitúa en $4 \text{ horas}$.

#### C. Decisión de Diseño: Arquitectura Híbrida de Doble Capa
Para preservar la ultra-baja huella de cómputo en CPU y SQLite mientras se obtiene la precisión fina del decaimiento exponencial:
1. **Filtro de Consulta (SQL)**: Se limita la búsqueda a una ventana máxima de evaporación ($4 \times T_{1/2} = 16 \text{ horas}$) usando `WHERE touched_at >= strftime('%s', 'now') - 57600`.
2. **Evaluación Vectorial (Rust)**: Dentro del conjunto acotado de trazas devueltas, el kernel aplica la suma ponderada exponencial en memoria RAM (microsegundos en Rust):
   $$\text{Heat}_{\text{exact}}(x) = \min\left(2.0, \sum_{r \in \text{traces}} w_r \cdot 2^{-\frac{t_{\text{now}} - t_r}{14400}}\right)$$

---

### 3. Freno a la Amplificación de Errores (17.2x) y Prevención de Free-Riding

La literatura científica en sistemas multi-agente estigmérgicos demuestra que la auto-organización pura sin supervisión produce **cascadas de error de 17.2x** debidas a bucles de retroalimentación positiva descontrolados (un error inicial atrae más agentes, reforzando la alucinación).

Para garantizar la robustez soberana de Tylluan, se establecen tres compuertas inviolables:

```
[ Señal Estigmérgica ] ──► [ ATENCIÓN / PRIORIDAD ] (Qué investigar)
                                  │
                                  ▼
                         [ VERIFICATION GATE ]
                         ¿Pasa CI local?
                         ¿Cumple WORK_PROTOCOL §7?
                         ¿Pasa ADR-013 Non-Predation?
                                  │
                       ┌──────────┴──────────┐
                       ▼                     ▼
                     [ SÍ ]                [ NO ]
                       │                     │
                       ▼                     ▼
              [ EJECUCIÓN / MERGE ]    [ ALERTA COLOQUIO ]
```

1. **La Estigmergia es Enrutador de Atención, Nunca Ejecutor Autónomo**:
   - Una zona "caliente" indica foco de interés prioritario.
   - Una zona "fría" indica deuda técnica o falta de cobertura de pruebas.
   - **Ningún cambio de código o refactorización puede comitearse únicamente porque el calor sea alto**. La ejecución exige verificación determinista.

2. **Atribución Individual y Anti-Free-Riding**:
   - Cada huella estigmérgica almacena obligatoriamente el `agent_id` real.
   - El sistema de auditoría computa el balance de esfuerzo de la flota:
     - Trazas de creación vs trazas de verificación cruzada.
     - Detección automática de bucles de auto-recompensa (un agente que toca continuamente sus propios nodos sin verificación cruzada ve su peso de rastro atenuado por un factor $\alpha_{\text{self}} = 0.2$).

3. **Integración con los Gates Mecánicos Preexistentes**:
   - **Gate 1**: `scripts/check_no_predation.sh` (ADR-013) — análisis de impacto verificado.
   - **Gate 2**: `WORK_PROTOCOL.md §7` — tareas con exit codes explícitos, watchdog de 15 min y logs persistentes en disco.
   - **Gate 3**: `scripts/verify.sh` — 100% de tests unitarios, de integración y lint en verde antes de cualquier push.

---

### 4. Ciclo de Vida y Difusión Espacial de Señales

El ciclo de mantenimiento de huellas estigmérgicas se acopla a las rutinas de consolidación nocturna (`NightConsolidation` en `main.rs`):

1. **Difusión Espacial**:
   - Al registrar una huella en una zona de código (ej: `crates/tylluan-kernel/src/transport/server/handler_recall.rs`), se propaga una fracción de calor ($\beta = 0.25$) a los módulos dependientes directos en el grafo de compilación Cargo.
2. **Evaporación y Purga**:
   - Trazas con antigüedad $> 7 \text{ días}$ son purgadas por `prune_old_traces(7)` para mantener el tamaño de `mailbox.db` / `silva.db` mínimo.
3. **Puntuación de Resonancia en Búsquedas Híbridas**:
   - En `search_hybrid` (RRF + BM25 + BGE-M3), el calor estigmérgico modula el ranking final únicamente como desempate de relevancia reciente, sin distorsionar la similitud semántica fundamental.

---

## Consequences

### Positivas
- **Desacoplamiento total de la flota**: Los agentes pueden operar de forma asíncrona en sus propios ciclos programados (10m, 15m, 5m) sin esperas bloqueantes ni interbloqueos de subprocesos.
- **Visibilidad orgánica de focos de conflicto**: El dashboard de Tylluan puede renderizar un mapa de calor dinámico del workspace completo en tiempo real.
- **Resiliencia bio-inspirada**: Si un agente cae o se detiene, las señales de trabajo persisten en el medio compartido para que otro agente tome el testigo de forma natural.
- **Cero sobrecoste de tokens de coordinación**: La coordinación ocurre en SQLite local en microsegundos, sin llamadas LLM de arbitraje innecesarias.

### Negativas y Mitigaciones
- **Riesgo de sesgo de atracción ("Herding")**:
  - *Mitigación*: Se introduce una probabilidad de exploración estocástica ($\epsilon = 0.15$) en los prompts de los agentes para forzar la revisión de zonas frías desatendidas.
- **Crecimiento de tablas de trazas**:
  - *Mitigación*: Mantenimiento estricto con `prune_old_traces` y límites rígidos de retención.

---

## Plan de Implementación (Fases)

1. **Fase 1 (Aprobación y Documentación)**:
   - Revisión formal línea a línea por el Tech Lead (`@claude-code`).
   - Indexación en `ARCHITECTURE_DECISIONS.md`.
2. **Fase 2 (Sustrato de Tipos y Schema en Kernel — Asignado a `@deep`)**:
   - Migración de esquema en `crates/tylluan-kernel/src/memory/silva/schema.rs` para soportar `work_traces` o campos polimórficos de destino.
   - Implementación de la función híbrida de decaimiento exponencial en `crates/tylluan-kernel/src/memory/silva/decay.rs`.
3. **Fase 3 (Verificación Cruzada y Benchmarks — Asignado a `@buffy`)**:
   - Suite de pruebas unitarias y benchmarks de latencia SQL en `decay.rs` / `tests.rs`.
4. **Fase 4 (Observabilidad y Dashboard — Asignado a `@antigravity`)**:
   - Visualización de zonas calientes en el visualizador técnico y dashboard.
