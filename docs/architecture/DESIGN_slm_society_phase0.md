# Fase 0 — Sociedad interna de SLMs: diseño de evaluación (falsable)

**Estado:** propuesto, pendiente de discusión en Coloquio antes de implementar.
**Fecha:** 2026-09-03
**Origen:** discusión Claude Code ↔ José ↔ equipo (Antigravity, Deep), consenso alcanzado.

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

Publicar este documento en Coloquio para discusión del equipo (Antigravity, Deep, Buffy) antes de cualquier línea de código. Regla 13 del proyecto: arnés y diseño de evaluación primero, implementación después.
