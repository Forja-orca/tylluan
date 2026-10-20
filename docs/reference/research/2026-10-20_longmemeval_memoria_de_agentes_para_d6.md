# LongMemEval: qué mide y qué optimizaciones propone (insumo para D-6, recall)

**Fecha:** 2026-10-20 · **Autor:** Claude Code (TL) · **Propósito:** contrastar los fallos medidos por Buffy en `INFORME_recall_hibrido_coloquio_buffy_2026-10-20.md` (R@5 38,3 %; 19/60 consultas con el propio mensaje fuente como primer resultado; seguimientos sin entidad que fallan) con un benchmark público de memoria de asistentes. Solo se cita lo leído de la página del artículo el mismo día.

## Fuente

| Fuente | Qué es |
|---|---|
| [Wu et al., "LongMemEval: Benchmarking Chat Assistants on Long-Term Interactive Memory" (arXiv 2410.10813)](https://arxiv.org/abs/2410.10813) | Benchmark de 500 preguntas sobre historiales de chat escalables; marco indexación / recuperación / lectura |

## Citas literales (resumen de la página, fragmentos cortos)

| Tema | Cita |
|---|---|
| Tamaño | "With 500 meticulously curated questions embedded within freely scalable user-assistant chat histories" |
| Dificultad | "commercial chat assistants and long-context LLMs showing a 30% accuracy drop" al memorizar información en interacciones sostenidas |
| Capacidades | "information extraction, multi-session reasoning, temporal reasoning, knowledge updates, and abstention" |
| Optimizaciones | "session decomposition for value granularity", "fact-augmented key expansion for indexing", "time-aware query expansion for refining the search scope" |

## Qué se parece a lo medido en Tylluan (análisis mío, no cita)

1. **Granularidad del valor indexado.** El artículo propone descomponer por sesión. Nuestro hallazgo 6 de B-3 (una salida de `read_channel` de 50 mensajes guardada como memoria que gana por baja densidad léxica) es el caso contrario: un valor demasiado grueso. Apoya el punto 1 de D-6 (no indexar salidas de lectura).
2. **Claves de indexación enriquecidas.** "Fact-augmented key expansion" sugiere indexar, junto al texto, hechos extraídos. Los seguimientos sin entidad ("¿Por qué existen dos?") no tienen ancla léxica; una clave con el tema de la conversación inmediata podría dársela. No se ha probado en Tylluan.
3. **Abstención** figura entre las capacidades evaluadas. Nuestro CoherenceGate que devuelve "Found N, Showing top 0" es una abstención, pero no distingue "no hay nada" de "hay algo no confirmado". Eso es lo que debe aclarar D-6.3.

## Qué NO se puede concluir

- No he leído la versión completa del artículo (solo la página de resumen): no cito cifras por optimización ni el diseño exacto del benchmark.
- La comparabilidad es limitada: LongMemEval trata de chats usuario-asistente; nuestro corpus son hilos de agentes de ingeniería con mensajes largos y referencias a turnos.
- Ninguna de las tres optimizaciones se ha implementado ni medido aquí. El criterio de entrada sigue siendo el de D-6: runner sobre `benchmarks/recall_manual_v1.json`, R@5 sube y R@1 no baja.

## No verificado

Texto completo del artículo, resultados cuantitativos por optimización, código de referencia del benchmark.
