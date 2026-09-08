# Auditoría externa + propuesta Tylluan v1.0 (2026-09-08)

**Estado:** referencia viva, no plan vinculante. José: "no lo seguiremos a pies juntillas, pero es importante mantener estos datos... hasta que ya no sea útil y se pueda borrar." Guardar íntegro con fecha; no resumir ni recortar en futuras ediciones de este archivo — si algo queda obsoleto, márquese como tal en línea, no se borre la sección.

**Origen:** auditoría técnica externa (no-flota, modelo tipo GPT vía ChatGPT) del repositorio real `Forja-orca/tylluan` en `main`, cruzando arquitectura, código Rust/Python, benchmarks, CI, seguridad, issues, PRs, roadmap y commits recientes — no una lectura de README.

**Verificación por Claude Code (2026-09-08), antes de aceptar el informe**: spot-check de 4 afirmaciones técnicas concretas y falsables, todas confirmadas contra el código/datos reales del repo:
1. Latencias del baseline CPU (`benchmarks/results/latency_baseline_20260907.json`) — coinciden exactamente.
2. Conteo de tests/unwraps (808 total, 31 unwraps cat-a, 0 lock-poison) — coincide con `STATUS.md` verificado ese mismo día.
3. Ubicación del cache de embeddings (`QueryEmbeddingCache` consultado después de que el caller ya llamó a `embed()`, no antes) — coincide exactamente con el hallazgo que Claude Code ya había documentado en `search.rs` de forma independiente, sin que el auditor externo tuviera acceso a esa nota.
4. Fórmula de penalización de grado en PageRank (`pr_score / (1.0 + degree * 0.1)`, `graph.rs:757`) — confirmada por grep directo, coincide con la narrativa de "se cambió de boost a penalización".

**Veredicto de fiabilidad**: informe real, con trabajo de verdad detrás, no genérico ni fabricado. Las notas cualitativas no verificables (tamaño exacto del repo, estado de issues/PRs en GitHub) se toman con más cautela por ser de menor riesgo si están mal. El marco conceptual (Cognitive Scheduler, Trust as primitive, Evidence Graph, Capability Contracts, Sleep Cycle) son *propuestas de diseño*, no hechos sobre el estado actual — well fundamentadas, pero no confundir con lo que "ya existe".

**Coincidencia con principios ya establecidos del proyecto**: el vocabulario es nuevo pero la dirección no contradice nada ya decidido — "Capability Contracts" es esencialmente G6 (ya en marcha); "Cognitive Scheduler" coincide con el principio fundacional de SLMs como "sinapsis" (no razonador general); la insistencia en mantener la sociedad SLM fuera del camino caliente coincide con la corrección de intención de José del 2026-07-28.

---

## Parte 1 — Auditoría técnica del estado real

[Contenido íntegro del informe, pegado sin editar por José el 2026-09-08]

He hecho una **auditoría técnica del repositorio real `Forja-orca/tylluan` en `main`**, no una lectura superficial del README. He cruzado arquitectura, código Rust/Python, benchmarks, CI, seguridad, issues, PRs, roadmap y los commits más recientes.

### Veredicto ejecutivo

**Tylluan ya no es un simple proyecto de "memoria para agentes".** Técnicamente se ha convertido en una especie de **kernel local de continuidad + memoria + routing + ejecución + coordinación + federación para agentes**, con una arquitectura bastante más ambiciosa que la mayoría de proyectos de agent-memory.

Valoración:

| Área | Nota | Diagnóstico |
| --- | ---: | --- |
| Arquitectura | 9.0/10 | Muy ambiciosa y sorprendentemente coherente |
| Memoria/RAG | 8.8/10 | Uno de los componentes más maduros |
| Routing | 8.0/10 | Funcional, pero todavía no suficientemente inteligente |
| Ejecución de herramientas | 8.5/10 | Potente, especialmente vía guilds |
| Multi-agente | 8.5/10 | Coloquio + contracts + coordinator |
| P2P/Federation | 8.3/10 | Muy interesante, pero todavía necesita endurecimiento |
| Seguridad | 8.4/10 | Mucho trabajo real; todavía no es producto hardened |
| Observabilidad | 8.7/10 | Muy superior a la media de proyectos de agentes |
| Tests/CI | 9.2/10 | Excelente disciplina para un proyecto de este tamaño |
| UX/onboarding | 7.3/10 | Ha mejorado mucho, pero Python/guilds siguen complicando |
| Rendimiento | 6.8/10 | Es ahora mismo el cuello de botella principal |
| Madurez producto | 6.5/10 | Investigación avanzada, todavía no v1 |
| Potencial | 9.5/10 | Aquí está lo realmente interesante |

**Conclusión del auditor:** no añadir simplemente más funcionalidades. El siguiente salto de Tylluan debería ser convertir todo lo que ya existe en un sistema operativo cognitivo para agentes, reduciendo latencia, complejidad y acoplamiento.

### 1. Qué es realmente Tylluan

No es "un RAG con herramientas". Es más parecido a un microkernel local para agentes autónomos, donde memoria, identidad, herramientas, coordinación y confianza son subsistemas del kernel.

```text
                  ┌───────────────────────────┐
                  │       AI CLIENT            │
                  │ Claude / Cursor / VSCode   │
                  │ OpenClaw / A2A / etc.      │
                  └─────────────┬─────────────┘
                                │
                         MCP / A2A / HTTP
                                │
                  ┌─────────────▼─────────────┐
                  │       TYLLUAN KERNEL       │
                  │       Rust / Tokio          │
                  ├────────────────────────────┤
                  │ Sovereign Tool Interface   │
                  │ Intent Router              │
                  │ Complexity Cascade         │
                  │ Coordinator                │
                  │ Trust / HITL                │
                  │ Coherence Gate              │
                  │ Session Continuity         │
                  ├────────────────────────────┤
                  │          SILVADB            │
                  │ SQLite/WAL                  │
                  │ FTS5/BM25                   │
                  │ BGE-M3                      │
                  │ HNSW                        │
                  │ Graph / PageRank            │
                  │ FSRS lifecycle              │
                  ├────────────────────────────┤
                  │      GUILD FABRIC           │
                  │  Python / FastMCP processes │
                  ├────────────────────────────┤
                  │        MESH / P2P           │
                  │ Kademlia                    │
                  │ Gossip                      │
                  │ Noise NK                    │
                  │ Remote dispatch             │
                  └─────────────┬──────────────┘
                                │
                    ┌───────────▼───────────┐
                    │ OTHER TYLLUAN NODES   │
                    └───────────────────────┘
```

### 2. Magnitud real del proyecto

Workspace Rust: `tylluan-kernel`, `tylluan-common`, `tylluan-gui`, `tylluan-cli`, `tylluan-link`, `tylluan-evals`, `tylluan-fsrs`. Más guilds Python/FastMCP, dashboard React, scripts de evaluación, benchmarks, CI/CD, instaladores, Docker, esquemas API, tests E2E, documentación arquitectónica, gobernanza para agentes de código. ~467 MB según GitHub (no verificado por Claude Code).

808 tests Rust/lib pasando (727 kernel + 69 link + 12 fsrs, verificado). CI incluye build, tests, Clippy, cargo-deny, Python, dashboard, seguridad, ARM64, install smoke, Docker smoke y claims gate.

"El problema ya no es '¿funciona el código?'. El problema empieza a ser: ¿la arquitectura está usando inteligentemente toda la capacidad que hemos construido? Todavía no."

### 3. La memoria es probablemente el componente más fuerte

```text
Query
 ├── BM25 / FTS5
 ├── BGE-M3 semantic
 ├── ANN / HNSW
 ├── graph traversal
 ├── PageRank
 ├── degree penalty
 ├── FSRS lifecycle
 └── RRF
       ↓
   candidate set
       ↓
 CoherenceGate
```

Más memoria episódica, persona del agente, preferencias, provenance, confidence, evidence, lifecycle, archived memories, reinforcement, recall feedback, contradiction detection, graph relationships.

LongMemEval-S (50 preguntas): Recall@1 46%, Recall@5 82%, Recall@10 90%, MRR/R-Precision 46%. "Eso no significa que Tylluan tenga '82% de inteligencia de memoria'. Significa específicamente que el retriever encuentra el target correcto en top-5 en 82% de los casos." El proyecto detectó y corrigió la interpretación engañosa de Precision@5 para single-needle — "señal de madurez científica" (nota: esto ya estaba cerrado por el equipo el 2026-09-03, verificado en este mismo ciclo de conversación).

### 4. Debilidad en la memoria: demasiadas capas sin calibrar conjuntamente

```text
BM25 + embedding + HNSW + graph + PageRank + RRF + entity boost + FSRS + CoherenceGate + reranker
```

"Cada componente individual mejora algo, pero el sistema completo se vuelve difícil de calibrar." El proyecto ya corrigió un sesgo real de degree centrality (`score * (1 + degree * 0.1)` favorecía hubs genéricos → cambiado a penalización, verificado por Claude Code en `graph.rs:757`). Principio propuesto: "ninguna nueva capa de retrieval entra por intuición arquitectónica; entra solamente si mejora un benchmark segmentado."

### 5. El gran problema actual: latencia

Baseline CPU del 2026-09-07: `do` p50≈501ms, p95≈30.8s, p99≈102.8s. Causa: comportamiento de cold-start/contention — el primer tick del Agnostic Reindexer se ejecutaba inmediatamente al arrancar y competía por el modelo ONNX con las primeras operaciones reales. Corregido el mismo 2026-09-08 (retraso de 600s del primer tick, ver `STATUS.md`). "Exactamente el tipo de bug que aparece cuando pasas de 'arquitectura correcta' a 'sistema vivo'."

### 6. Cache de embeddings mal ubicado

```text
caller → embed() → search_hybrid() → cache lookup   [actual, ineficaz]
query → normalize → CACHE HIT/MISS → (ONNX si miss) → cache → result   [propuesto]
```

"El embedding cache debería vivir en el choke point común antes de cualquier inferencia, no dentro del algoritmo de retrieval." (Nota: coincide exactamente con el hallazgo ya documentado por Claude Code en `search.rs` el 2026-09-08, de forma independiente al auditor.)

### 7. Routing: funciona, pero hay oportunidad

Hybrid routing ≈41%, keyword ≈32%, embedding ≈20% (benchmark externo/difícil) vs 56-64% en dataset interno curado — dataset shift real. Propuesta: no un LLM grande para cada decisión de routing, sino aprender del historial real (`recall_feedback`, Signal Loop) hacia un clasificador local pequeño + ruta de incertidumbre al coordinator.

### 8. SLM society: interesante, pero NO en el hot path

El experimento de julio (NO-GO) mostró convergencia a respuesta constante. "Una 'sociedad de agentes' no es automáticamente mejor que un modelo." La Fase 0 actual (A=baseline, B=Self-MoA, C=roles asimétricos con criterios de falsación) "sí tiene sentido". Lugar natural: NightConsolidation, análisis offline, no cada recall/tool call/routing.

### 9-10. CoherenceGate: trust score, no allow/deny — pero riesgo de inferencia implícita

Capas 1-4 (patrones conocidos, provenance, drift semántico, clasificación LLM opcional) + enforcement (REJECT→cuarentena, KEEP_SOFT→peso×0.5). El LLM judge fue suficientemente peligroso para requerir `coherence_gate_hybrid_enabled=false` por defecto — un recall normal podía arrancar inferencia pesada sin opt-in. Regla propuesta: "ningún componente de inteligencia pesada debería ejecutarse implícitamente desde una operación aparentemente barata." Tres clases de coste propuestas: FAST (<10-50ms, determinista/modelo pequeño), NORMAL (<1s, embedding/reranker), HEAVY (>1s, LLM generativo/sociedad/investigación).

### 11. Cognitive Scheduler (propuesta central)

El routing debería saber no solo "qué guild" sino coste, riesgo, hardware, latencia, energía, privacidad, urgencia:

```text
TASK → capability, risk, latency budget, compute budget, privacy level,
       required confidence, reversibility
    ↓
Scheduler → local CPU / local GPU / remote peer / guild / SLM / LLM / HITL
```

### 12-13. Guilds: deuda de catálogo, propuesta de Capability Contract

Catálogo puede divergir del filesystem (confirmado por el propio ciclo I-7/G6 de este proyecto: scrapling/scrapling_web, scheduler, council, whats_new, vision_moondream — todos hallazgos reales de este mismo ciclo de trabajo, no del auditor). Propuesta: manifiesto `guild.toml` como fuente de verdad única, generando catálogo/router/seguridad/docs/dashboard/benchmarks automáticamente, con campos de identity, capability, risk, execution class, rollback, permissions.

### 14. `plan → act → verify → undo` como pieza estratégica

Tylluan ya tiene `plan=true` y `undo_last_action` con rollback contractual — propuesta de formalizarlo como "transactional agency": PLAN → intent → actions → risk evaluation → HITL si aplica → EXECUTE → evidence → VERIFY → COMMIT (o ROLLBACK si falla).

### 15-16. A2A + MCP + Mesh: buena separación ya existente; federación aún no "planetary"

MCP (agente↔Tylluan), A2A (Tylluan↔agentes externos), Mesh Noise/Kademlia/Gossip (Tylluan↔Tylluan) — separación arquitectónica correcta, mantener. El objetivo de "1 millón de nodos" del roadmap requiere antes demostrar escalado incremental (10→100→1000→10000) con churn, particiones, nodos bizantinos, etc. Sin BFT/identity trust serio, llamarlo "sovereign federated mesh", no "planetary trust network" todavía.

### 17-18. Seguridad: mejor de lo que parece, pero vigilar SQLCipher

Trabajo real: CodeQL, cargo-deny, claims gate, security tests, SQLCipher, keychain, Noise NK, provenance, quarantine, HITL, write gates, CODEOWNERS, SHA-pinned Actions. El hallazgo de `panic = "abort"` invalidando un `catch_unwind` supuesto es "auditoría real". 31 unwraps de producción, todos clasificados seguros; 0 casos de lock-poisoning restantes (verificado, coincide con el ciclo de esta semana).

Punto de vigilancia: verificar que TODOS los stores con material sensible (no solo `silva.db`) pasan por el mismo `open_db()` seguro — audit, mailbox, peer state, session state, cache, logs, artifacts, guild output.

### 19-21. Lifecycle de memoria como "sistema metabólico"; NightConsolidation como "Sleep Cycle"

`active → quiet → consolidated → archived` (mejor que "memoria → decay → delete"), FSRS para retrievability. Propuesta conceptual: NightConsolidation no es "tarea nocturna", es el ciclo REPLAY/CONSOLIDATE/COMPRESS/DEDUPE/RESOLVE/GENERALIZE/EVALUATE/LEARN que une Signal Loop + Memory Lifecycle + CoherenceGate + Routing Dataset + SLM Society + FSRS — hoy piezas separadas.

### 22. Dashboard como "Cognitive Control Plane"

No una UI más — el operador debería poder ver qué está haciendo Tylluan, por qué, con qué memoria, qué riesgo, poder pararlo/deshacerlo.

### 23-24. Demasiadas superficies; propuesta de "arquitectura de invariantes"

Rust kernel + Python guilds + React dashboard + Tauri experimental + MCP + A2A + HTTP + SSE + P2P + Noise + Kademlia + Gossip + SQLite + FTS5 + BGE-M3 + HNSW + FSRS + llama.cpp + ONNX + DirectML + CUDA + FastMCP + CLI + Docker + installers. "Cada subsistema adicional aumenta la superficie de estados imposibles" — coincide con el historial real de bugs de este proyecto (puertos incorrectos, guild names divergentes, configs muertas, docs obsoletas, headers MCP, ONNX provider, locks, CI drift, cache mal ubicado). Propuesta: 10 invariantes explícitos en el kernel (todo pasa por policy evaluation, toda memoria tiene provenance, ninguna inferencia pesada arranca implícitamente, ningún guild sin contrato válido, etc.), con tests automáticos por invariante.

### 25-27. Estado de issues/PRs (no verificado por Claude Code) y documentación como sistema distribuido

"El repositorio está siendo desarrollado como laboratorio controlado" (no cola de usuarios reportando bugs) — ventaja: se puede rediseñar antes de tener base de usuarios que obligue a conservar malas decisiones. Documentación real vs código ya ha divergido varias veces (STATUS≠code, catalog≠guilds) — propuesta: generar documentación desde tipos Rust/manifiestos, no escribirla a mano.

### 28-31. Métricas propuestas (no implementadas)

- **Capability Coverage**: registered vs tested vs executable vs rollback-capable vs HITL-certified guilds.
- **Autonomous Success Rate**: tareas completadas correctamente / tareas intentadas, segmentado por guild/agente/tipo/riesgo/hardware/modelo.
- **Memory Utility Score**: memoria recuperada → usada → ayudó → cambió el resultado (Recall@5 no dice si la memoria ayudó).
- **Cognitive ROI**: agente base vs agente+Tylluan en éxito, tokens, tool calls, alucinación, recuperación, intervención humana.

### 32. Qué NO haría ahora (según el auditor)

Más guilds, más protocolos, otro vector DB, otro LLM coordinator, sociedad de SLMs en tiempo real, más features de dashboard, perseguir "1 millón de nodos" ya.

---

## Parte 2 — Propuesta Tylluan v1.0 (congelación de alcance + roadmap de 16 semanas)

Respuesta del mismo auditor a la pregunta "¿deberíamos congelar funcionalidades para v1.0?":

**Regla de oro propuesta**: "Tylluan v1.0 no debe tener más órganos; debe hacer que los órganos existentes funcionen como un sistema."

Ciclo objetivo:

```text
INTENT → MEMORY+TRUST → COGNITIVE ROUTER → PLAN → POLICY/HITL → EXECUTE
       → VERIFY+EVIDENCE → MEMORY/SIGNAL → SLEEP/LEARN
```

Cada operación debe responder: qué hizo → por qué → con qué conocimiento → con qué autoridad → con qué resultado.

### Fase 0 — Congelación de alcance (1-2 días)

Congelar: nuevas guilds salvo bloqueo funcional, nuevos protocolos, nuevas DBs, nuevos modelos, nuevas capas de UI, nueva "sociedad" en runtime.
Mantener: kernel Rust, SilvaDB, guilds, MCP, A2A, mesh, HITL, CoherenceGate, Signal Loop, dashboard.
Crear: `docs/v1/V1_SCOPE.md`, `V1_INVARIANTS.md`, `V1_SLO.md`, `V1_THREAT_MODEL.md`, `V1_CAPABILITIES.md`.

Definición: `Tylluan v1.0 = capability-complete + performance-bounded + observable + recoverable + security-audited`.

### Fases P0 (semanas 1-6)

1. **Kernel predecible** (sem 1-2): workers separados (fast lane / embedding / LLM / heavy) con cola, timeout, límite de concurrencia, cancelación, métricas; QueryEmbeddingCache movido antes de la inferencia con objetivo >80% hit rate; presupuesto de latencia explícito por tipo de operación (health <10ms, memory exact <20ms, hybrid warm <100ms, routing <100ms, simple tool <500ms); `ComputeBudget` con slots de cpu/gpu/embedding/llm/network que el scheduler nunca excede.
2. **Capability Contracts** (sem 2-3): `guild.toml` como fuente de verdad única generando router metadata, security policy, dashboard, catalog, docs, benchmarks automáticamente. Capability Matrix visible en dashboard.
3. **Cognitive Scheduler** (sem 3-5): scheduler determinista (no otro modelo todavía) que separa complexity de risk de cost de uncertainty; clases de ejecución FAST/NORMAL/HEAVY/HUMAN/REMOTE.
4. **Transactional Agent Execution** (sem 5-6): PLAN→AUTHORIZE→EXECUTE→VERIFY→COMMIT (o →FAIL→ROLLBACK), cada acción con ActionID/ParentPlanID/AgentID/GuildID/Risk/Authorization/Input/Output/Evidence/Rollback/Status/Timestamp; idempotency_key para operaciones peligrosas.
5. **Evidence Graph** (sem 6-7): Claim→evidence/source/agent→confidence→decision→action, cadena auditable de "por qué Tylluan hizo esto".

### Fases P1 (semanas 7-14)

6. **Trust unificado** (sem 7-8): `TrustSubject` (Agent/Memory/Guild/Peer/Model/Evidence/Action) con `TrustScore` multidimensional (identity, behavior, provenance, recent_activity, capability), no un solo número.
7. **Memory v1** (sem 8-9): no más algoritmos de retrieval nuevos, optimizar los existentes; Memory Utility Benchmark; estados explícitos SUPPORTED/CONTRADICTED/UNCERTAIN/SUPERSEDED/ARCHIVED para memoria contradictoria.
8. **Signal Loop → Sleep Cycle** (sem 9-11): unifica Signal Loop+FSRS+Memory Lifecycle+CoherenceGate+Routing feedback+Dataset generation+SLM research. Regla: "el Sleep Cycle nunca modifica directamente producción sin validación" — genera cambios propuestos, luego evaluate→accept→commit o reject.
9. **Learning Loop** (sem 11-12): dataset real de ejecuciones (intent/route/guild/alternativas/memoria/acción/outcome/latencia/intervención humana/éxito) → evaluación → modelo de routing pequeño → A/B → promote.
10. **SLM Society** (después de lo anterior, solo offline): analizar errores, comparar estrategias, encontrar contradicciones, generar candidatos de memoria, evaluar rutas, producir hipótesis. Nunca permitir escribir memoria de producción, ejecutar tools arbitrarias, cambiar política o modificar trust sin pasar por gates.
11. **Federation v1** (sem 12-14): validar escalado incremental 1→10→100→1000 nodos (no 1M todavía), medir lookup latency, gossip convergence, churn, recuperación de particiones, resistencia a replay, rotación de identidad, revocación de claves, aislamiento de peers maliciosos. Estados de peer: UNKNOWN→DISCOVERED→CHALLENGED→TRUSTED→DEGRADED→QUARANTINED.
12. **Dashboard como Cognitive Control Plane** (sem 15): reestructurar (no añadir pantallas) alrededor de NOW/WHY/SAFETY/MEMORY/MESH/LEARNING.

### Métricas de aceptación de v1.0 (no "todos los tests pasan")

- **Reliability**: cero crashes críticos conocidos, cero rutas inseguras por defecto, cero spawn de workers sin límite.
- **Latency**: recall simple p95 <300ms, routing simple p95 <150ms, health p95 <50ms (hardware de referencia a definir).
- **Memory**: Recall@5 >90%, Memory Utility >70% (objetivos iniciales, no garantías universales).
- **Routing**: >90% selección correcta de capability en benchmark propio representativo; ruta incorrecta debe ser detectable y recuperable.
- **Execution**: >95% verificación exitosa en tareas deterministas.
- **Safety**: 100% acciones de riesgo con gate, 100% ejecuciones auditables, 100% acciones rollback-capable correctamente anunciadas.
- **Federation**: primera meta 1.000 nodos simulados antes de reclamar escalabilidad masiva.

### Test pyramid v1 + chaos testing

Real Agents E2E → Scenario tests → Integration → Unit/property, más chaos tests explícitos (kill worker, disconnect peer, corrupt cache, timeout LLM, drop network, restart kernel, lock database, duplicate/replay message) y una suite de Crash Recovery (crash en cada punto del ciclo plan/execute/commit/rollback/memory write/federation sync/consolidation). Propiedad buscada: "nunca dejar una acción en un estado ambiguo sin posibilidad de reconstrucción".

### Security v1

Auditoría externa idealmente sobre: Rust memory safety, authentication, uso del protocolo Noise, SQLCipher, operaciones de filesystem, ejecución de comandos, MCP, A2A, P2P, sandboxing. Fuzzing de parsers de protocolo, inputs de guilds, mensajes MCP/A2A, serialización de memoria, configuración.

### Explícitamente fuera de v1.0

Red de 1M nodos, auto-modificación autónoma estilo AGI, sociedad SLM como cerebro principal, nuevo vector DB, nuevo framework de agentes, decenas de guilds adicionales, dependencia cloud, reemplazar el kernel Rust.

### Roadmap compacto (16 semanas)

```text
S1  Freeze + invariants + SLO
S2  Performance / ONNX / cache
S3  Capability Contracts
S4  Cognitive Scheduler
S5  Transactional execution
S6  Evidence Graph
S7  Trust primitive
S8  Memory Utility
S9  Sleep Cycle
S10 Learning Loop
S11 Routing model evaluation
S12 Federation hardening
S13 Chaos / recovery
S14 Security audit
S15 Dashboard control plane
S16 v1.0 release candidate
```

### Definición de éxito propuesta para v1.0

"Un agente externo puede conectarse a Tylluan, darle una tarea real, utilizar memoria persistente, seleccionar capacidades, planificar, pedir aprobación cuando corresponde, ejecutar herramientas, verificar el resultado, recuperarse de errores, dejar evidencia auditable y aprender de la ejecución posterior, todo ello sin depender de servicios cloud para que el kernel funcione."

Orden de prioridad que el auditor no cambiaría: Performance → Capability Contracts → Cognitive Scheduler → Transactional Execution → Evidence/Trust → Sleep/Learning → Federation.

**Pregunta de diseño planteada por el auditor, sin responder todavía**: ¿v1.0 debe ser principalmente un runtime para un agente local (single-node first) o el primer nodo de una red Tylluan federada (mesh-first)?

---

## Decisión de José (2026-09-08)

"No lo seguiremos a pies juntillas, pero es importante mantener estos datos... nos da una hoja de ruta clara para no seguir a oscuras. Siempre generaremos nuestras mejoras sobre todo esto. Es un informe de una auditoría, pero nosotros lo vivimos, el equipo lo vive. No tenemos prisa."

Además: instrucción explícita de convertir el vocabulario interno del equipo a vocabulario profesional a partir de ahora (ver `feedback_professional_vocabulary_adoption.md` en la memoria del proyecto).
