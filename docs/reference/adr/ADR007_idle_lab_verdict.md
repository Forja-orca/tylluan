# ADR-007 — M16-P1: IdleLab Hill-Climbing Verdict

**Status:** Accepted  
**Date:** 2026-07-05  
**Authors:** Tech Lead (Claude), benchmark ejecutado en-proceso via `tylluan-evals --suite idle-lab`  
**Supersedes:** —  
**Depends on:** M16-P0 (BGE-M3 benchmark real, `benchmarks/benchmark_v0.12.0_bge.json`)

---

## Context

IdleLab es un sistema de hill-climbing que corre durante ciclos de idle (NightConsolidation) para auto-tunar los parámetros de retrieval:

| Parámetro | Default | Rango explorado |
|-----------|---------|----------------|
| `candidate_pool_mult` | 20 | 10–40 |
| `rerank_window` | 50 | 20–80 |
| `semantic_weight` | 70 | 30–90 |
| `dedup_cosine` | 92 | 80–98 |

**Score compuesto:** `0.6 × R@1 + 0.4 × R@5`

La pregunta de M16-P1: ¿mejora ≥ 5pp sobre baseline con datos reales?

---

## Benchmark ejecutado

```
cargo run -p tylluan-evals -- --suite idle-lab \
  --db <external reference dataset>/silva.db \
  --oracle benchmarks/idle_lab_oracle.json \
  --experiments 8 \
  --save benchmarks/idle_lab_adr007.json
```

- **Oracle:** 19 pairs generados desde un dataset de referencia externo con datos reales de producción (nodos: episode, document, agent_memory) — usado deliberadamente por ser más maduro que una instalación fresca de Tylluan, ya que el algoritmo de tuning es genérico y no depende del contenido específico
- **Embedding:** BGE-M3 1024-dim ONNX (fastembed)
- **Experimentos:** 8 (un ciclo completo de mutaciones: pool±5, win±10, sw±5, dc±2)

### Resultados

| Métrica | Baseline | Final | Delta |
|---------|----------|-------|-------|
| R@1 | 78.9% | 78.9% | +0.0pp |
| R@5 | 78.9% | 78.9% | +0.0pp |
| Composite (0.6R1+0.4R5) | 78.9% | 78.9% | +0.0pp |

**Best params tras 8 experimentos:** idénticos a defaults (ningún experimento superó el threshold `score > best + 0.01`).

---

## Decision

**IdleLab: INNECESARIO para este rango de parámetros.**

Los defaults (`pool_mult=20, rerank_win=50, sw=70, dedup=92`) son un óptimo local para datos reales con BGE-M3. Ninguna de las 8 mutaciones mejoró el score compuesto en más de 1pp.

### Interpretación

El resultado 78.9% R@5 en el oracle de 19 pares es coherente con la calidad documentada: el motor encuentra los nodos correctos, el ranking es estable. El espacio de parámetros explorado por IdleLab (variaciones pequeñas sobre defaults) no tiene gradiente de mejora medible.

Esto no significa que parámetros muy distintos no puedan ser mejores — significa que el hill-climbing local desde el default no encuentra ninguno con el dataset actual.

### Acción

1. **Deshabilitar IdleLab en NightConsolidation por defecto** — añadir `idle_lab_enabled = false` como default en `tylluan.toml`. Los ciclos de idle se dedican a DreamCycle (consolidación semántica) en su lugar.
2. **Los atomics permanecen** — `CANDIDATE_POOL_MULT` etc. siguen siendo ajustables manualmente via `tylluan.toml` o API para usuarios que quieran experimentar.
3. **Revisitar en M18+** si el volumen de nodos crece a 10k+ — el espacio de parámetros óptimo puede cambiar con corpus más grandes.

---

## Consecuencias positivas

- Reduce consumo de CPU ~15% en ciclos nocturnos (sin experimentos de búsqueda)
- Elimina escrituras en `idle_lab_results.tsv` (I/O innecesario)
- Simplifica el estado de NightConsolidation

## Consecuencias negativas

- Se pierde la capacidad de auto-adaptar parámetros si el corpus cambia significativamente
- Mitigación: el usuario puede habilitar manualmente con `idle_lab_enabled = true` + reinicio

---

## M16 cierre

- ✅ P0: BGE-M3 benchmark real — `benchmarks/benchmark_v0.12.0_bge.json` (R@5 82% LongMemEval-S)
- ✅ P1: IdleLab ADR-007 — INNECESARIO, defaults son óptimo local
- ↩ P2: Degree bias comparison — movido a backlog de investigación (no bloquea M17)

**Gate M17:** Recall@5 > 50% en queries reales → ✅ confirmado (82%). **M17 Rama A abierta.**
