# Fase 0 — Sociedad interna de SLMs: diseño de evaluación (falsable)

**Estado:** APROBADO para Fase 0-Pre (Smoke-test de Entropía). Discusión técnica resuelta en Coloquio (Turnos 248–251).
**Fecha:** 2026-09-03 (actualizado 2026-09-07 con análisis del spike ad5b9c4 y formalización de Fase 0-Pre)
**Origen:** discusión Claude Code ↔ José ↔ equipo (Antigravity, Deep), consenso alcanzado.

## ⚠️ Precedente real y lecciones del spike de julio (commit `ad5b9c4`)

Este experimento cuenta con un precedente empírico directo en `benchmarks/spikes/slm_society/` (`experiment.py`, `experiment_v2.py`, `REPORT.md`, commit `ad5b9c4`, 29-jul-2026):

`ROADMAP_O3.md:649` (CoherenceGate, NO-GO 2026-07-29): **"Sociedad de 3 SLMs probada, converge a respuesta constante (0% varianza). El 75% original era el default seguro bajo grammar, no juicio real."**

### Análisis técnico de la causa raíz en `experiment_v2.py:27-81`:
1. **Modelos evaluados en julio**: El spike no se limitó a sub-1B; evaluó explícitamente `SmolLM2-1.7B` (Proposer) y `Phi-3.5-mini` (~3.8B) / `Qwen2.5-0.5B` (Critic). El tamaño por sí solo no impidió el colapso.
2. **Prompts evaluados en julio**: `experiment_v2.py` ya contaba con prompts asimétricos (`PROMPT` permisivo vs. `CRITIC_PROMPT` escéptico).
3. **Mecanismo real de colapso**: La conjunción de **Muestreo Greedy (`temperature: 0`)** sobre una **Gramática GBNF de 1 token forzado** (`root ::= "KEEP" | "REJECT"`) sin permitir tokens de razonamiento intermedio (Chain-of-Thought / scratchpad). Al forzar a un SLM a emitir su veredicto en el token 0 bajo $T=0$, cualquier sesgo infinitesimal en los logits del pre-entrenamiento actúa como escalón determinista absoluto hacia la clase dominante (REJECT-ALL / KEEP constante).

### Corrección de intención de José (`ROADMAP_O3.md:659-678`):
Tylluan **nunca** debe competir en razonamiento general con el cliente IDE conectado (Claude Code, Cursor). El rol exclusivo de los SLMs son **"sinapsis internas concretas"** — decisiones acotadas de infraestructura (enrutamiento, poda de memoria negativa en `recall_misses`, verificación de enlaces en SilvaDB) —, jamás un "cerebro deliberativo general".

---

## 🔬 Fase 0-Pre: Smoke-test de Entropía y Falsación Rápida

Antes de construir el arnés completo en `NightConsolidation` (`crates/tylluan-kernel/src/memory/night/`), se ejecuta un test previo ultraligero y aislado sobre 15 queries reales:

### Especificación del Smoke-test:
- **Dataset**: 15 casos reales de `recall_misses` / `cases_real_50.json`.
- **Condiciones de Inferencia**:
  - Chain-of-Thought (CoT) habilitado: el modelo debe emitir 1-2 frases de análisis antes del veredicto.
  - Temperatura $T \in [0.4, 0.6]$ (no greedy $T=0$).
  - Sin gramática rígida de 1 token en el token 0.
- **Matriz de 3 Brazos**:
  - **Brazo A (Baseline)**: 1 pasada CoT directa ($T=0.2$).
  - **Brazo B (Self-MoA)**: 3 pasadas independientes CoT ($T=0.6$) + síntesis/voting simple (la hipótesis rival de arXiv:2502.00674).
  - **Brazo C (Roles Asimétricos)**: Proponente CoT $\to$ Escéptico CoT $\to$ Sintetizador CoT.
- **Métricas de Entropía**:
  1. *Similitud léxica/semántica (Jaccard / embedding cosine)* entre el razonamiento del Proponente y del Escéptico.
  2. *Tasa de desacuerdos genuinos* ($>0\%$).
  3. *Varianza inter-queries* en las decisiones finales (evitar REJECT-ALL o KEEP-ALL).

### Compuertas de Decisión (Gates):
- **🚫 HARD NO-GO GATE**: Si la similitud Proponente-Escéptico es $>85\%$ (el Escéptico capitula), si la varianza entre las 15 queries es 0, o si `Brazo B >= Brazo C` $\to$ **NO-GO definitivo** de la sociedad de SLMs en Tylluan. Se archiva la iniciativa sin escribir código en el kernel.
- **✅ GO GATE**: Si el Brazo C supera al Brazo B de forma estadísticamente consistente con varianza real $\to$ se desbloquea la Fase 0 completa (arnés en `crates/tylluan-evals`).

## Contexto

La idea de fondo (ver [[project_tylluan_internal_slm_society_vision]] en memoria del proyecto): en vez de un único LLM grande como "juez", usar 3+ SLMs pequeños (<4B, criterio inverso al de la industria: el menor peso que aún actúe como agente fiable) que deliberen como pares — no jerarquía, no cascada — usando el propio sustrato de Tylluan (memoria, grafo, Coloquio) como hogar de esa deliberación, con el objetivo medible de mejorar lo que Tylluan entrega a un IDE cliente.

La investigación (arXiv:2305.14325 — Du et al., ICML 2024; arXiv:2502.00674 — "Rethinking Mixture-of-Agents") establece dos hechos que este diseño debe respetar:

1. El debate multi-agente **sí produce ganancias reales** (10-50% relativo) — pero solo verificado en modelos grandes (GPT-3.5/4), nunca en SLMs <4B, y nunca en el patrón offline/asíncrono que Tylluan necesita.
2. **Riesgo directo a la premisa**: mezclar modelos/roles distintos puede rendir *peor* que muestrear repetidamente el mismo modelo fuerte (Self-MoA le gana a MoA heterogéneo por 6.6% AlpacaEval / 3.8% promedio MMLU-CRUX-MATH). Esto convierte "los roles asimétricos ayudan" en una hipótesis a falsar, no un hecho a asumir.

No existe precedente verificable de esta combinación exacta (SLM pequeño + deliberación entre pares + sustrato de memoria local + objetivo IDE + patrón offline). Es la aportación original de Tylluan, si sobrevive a la medición — no un patrón importado.

## Por qué offline (Fase 0), no tiempo real

Evidencia de los últimos 3 días de incidentes reales en este repo: cada inferencia ONNX síncrona metida en el camino caliente (recall/remember/think) causó el bug de HTTP-hang, cerrado en `f33921b`/`33273d2` con `block_in_place`/`spawn_blocking`. Una deliberación de 3 pares en el camino caliente reintroduciría exactamente esa clase de saturación que se acaba de eliminar. Fase 0 vive en `NightConsolidation` (`crates/tylluan-kernel/src/memory/night/`), junto a las fases ya existentes (`idlelab_phase.rs`, `light_reranker_train_phase.rs`), consumiendo señales de 24h: `recall_feedback`, `recall_misses`, `node_transitions`, Coloquio.

## Diseño del experimento: 3 brazos, mismo arnés

El coste real de Fase 0 es el arnés (queries fijas, métrica de calidad, medición antes/después), y ese arnés es idéntico para los tres brazos — reutiliza `crates/tylluan-evals` (`runner.rs`, `metrics.rs`, `corpus.rs`). Añadir el tercer brazo es una configuración de estrategia más, no una infraestructura nueva.

| Brazo | Configuración | N inferencias | Qué prueba |
|---|---|---|---|
| **A — Baseline** | 1 pasada directa, sin deliberación (T=0.2) | 1 | Punto de partida sin asistencia |
| **B — Self-MoA (compute-matched)** | Mismo SLM, N pasadas independientes (T=0.7) + agregador neutro | N | ¿Basta el muestreo estocástico + síntesis simple? (la hipótesis rival de arXiv:2502.00674) |
| **C — Roles asimétricos** | Proponente → Escéptico → Sintetizador, límite duro de turnos | N | La hipótesis original: ¿la tensión dialéctica forzada supera al muestreo ciego? |

Comparaciones que responde el experimento:
- **A vs C**: ¿la deliberación ayuda en absoluto?
- **A vs B**: ¿más pasadas del mismo modelo ayudan, sin roles?
- **B vs C**: la comparación que importa — ¿los roles distintos aportan sobre el compute-matched baseline, o es solo escalado de cómputo en inferencia disfrazado de "debate"?

## Criterio de falsación explícito

- **Si gana C**: primera evidencia en <4B de que la asimetría epistémica (forzar un rol a buscar contraejemplos) supera al muestreo ciego. Justifica construir el consejo deliberativo offline (Fase 1: quorum cache).
- **Si gana B**: se descarta la arquitectura de roles. Tylluan adopta consolidación nocturna por muestreo paralelo del mismo modelo — más simple, sin riesgo de sicofancia entre roles, sin necesidad de mantener 3 modelos distintos en memoria.
- **Si gana A**: diagnóstico de que el SLM elegido no discrimina bien entre múltiples opiniones (contaminación de contexto) — no usar agregación multi-pasada hasta tener un modelo más capaz; no proceder a Fase 1 con ningún brazo.

## Métricas (capas separadas, no una sola)

1. **Calidad de retrieval** — plantilla ya existente: overlap@10, hit rate, p50/p95 del arnés de cascada.
2. **Confianza/abstención** — provenance por respuesta, tasa de "no lo sé" correcta vs incorrecta, usando `recall_misses` como memoria negativa ya en producción. Esta es, según el consenso del equipo, la capa de mayor valor esperado — no rankear mejor, sino abstenerse mejor.

## Fuera de alcance de Fase 0 (explícito)

- Tiempo real / camino caliente: solo como Fase 2, opt-in, acotado a 1 turno, detrás de la puerta ya existente de `hybrid_classify` (hoy gated por defecto).
- Quorum cache (postura pre-deliberada por tema/cluster como nodo, recall barato en el camino caliente): Fase 1, solo si Fase 0 confirma que algún brazo con deliberación (B o C) supera a A.
- Cualquier cita externa no verificada contra el PDF primario (incluye `arXiv:2510.25787`, aportada por José, pendiente de verificación propia antes de entrar en cualquier documento).

## Nota de alcance sobre coste de la primera medición

Si correr los 3 brazos en la primera ejecución resulta caro en CPU (SLM lento, `NightConsolidation` ya con presupuesto ajustado), el orden de prioridad es A vs C primero, B en la segunda medición — pero el diseño de 3 brazos no se recorta, solo el primer run si la máquina no da para los tres a la vez.

## Siguiente paso

1. **Ejecutar el Smoke-test de Entropía (Fase 0-Pre)**: Script ligero y aislado sobre 15 queries de `cases_real_50.json` / `recall_misses` comparando Brazo A, Brazo B y Brazo C con CoT y $T=0.6$.
2. **Evaluar las Compuertas (Gates)**:
   - Si se activa el Hard NO-GO Gate (Jaccard $>85\%$, varianza cero o Brazo B $\ge$ Brazo C), se archiva definitivamente la sociedad SLM.
   - Si se supera el GO Gate (Brazo C $>$ Brazo B con varianza real), se procede a implementar el arnés completo de Fase 0 en `crates/tylluan-evals` para `NightConsolidation`.
3. Ninguna línea de código en el kernel hasta que los resultados empíricos de Fase 0-Pre sean publicados y verificados en Coloquio.
