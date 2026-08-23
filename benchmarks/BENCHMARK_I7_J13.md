# 📊 Benchmark I-7 / J-13: Evaluación Empírica Real del Router y Tiebreaker

> **Misión:** Evaluación 100% real y sin simulaciones heurísticas del desempate semántico BGE-M3 (J-13) y el router de Tylluan, ejecutado directamente contra el kernel en vivo (`/api/v1/embed` y `/api/v1/do?plan=true`) sobre el held-out test split de `dataset_i7_routing_curated.json` ($N=73$).

---

## 1. Estado del Entorno de Evaluación

- **Live Kernel:** `http://127.0.0.1:4000/health` → Status: `ok`, Version: `0.16.0`, Commit: `e337836`
- **Git HEAD Local:** `e337836` (verificado con `check_live_kernel_drift.sh`: **0 commits de lag**)
- **Dimensión de Vectores:** 1024 dimensiones reales (BGE-M3 en CPU ONNX).
- **Artefactos crudos:** `benchmarks/benchmark_i7_j13_results.json` + `benchmarks/benchmark_i7_j13_raw_calls.json` (mismo commit, lag 0, desglose completo por categoría e item).

> **Historial de ejecuciones:** la v1 de este benchmark se midió contra `18e70fa` (15+ commits desactualizado, Live Matcher 45.21%, categorías ambiguas 0%). Los datos siguientes corresponden a la ejecución contra el kernel al día (`e337836`) del 2026-08-23 (G5).

---

## 2. Resultados Empíricos en Held-Out Test Set ($N=73$)

| Modelo / Estrategia | Precisión Top-1 | Aciertos / Total | Observaciones |
| :--- | :---: | :---: | :--- |
| **1. Majority Class Baseline** | **2.74%** | 2 / 73 | Trivial baseline; demuestra ausencia de desbalance trivial en test. |
| **2. Pure Keyword Router** | **53.42%** | 39 / 73 | Reglas heurísticas léxicas estrictas (cero embeddings). |
| **3. Pure Semantic BGE-M3 Router** | **49.32%** | 36 / 73 | Cero reglas de palabras clave; 100% similitud coseno sobre vectores de 1024d. |
| **4. Blended Hybrid (55/45 sin Tiebreak)** | **61.64%** | 45 / 73 | 55% similitud coseno real + 45% keyword score normalizado. |
| **5. Hybrid + J-13 Tiebreaker** | **64.38%** | **47 / 73** | Si $\Delta \le 0.15$ entre top-2 híbridos, desempata la similitud semántica. |
| **6. Live Production Matcher (`matcher.rs`)** | **47.95%** | 35 / 73 | Kernel al día (`e337836`) vía `/api/v1/do` con `plan=true`. vs 45.21% en `18e70fa` (+2.74pp). |

---

## 3. Contribución Aislada del Tiebreaker J-13 ($\Delta$)

* **Ganancia Neta:** **$\Delta = +2.74\%$** ($61.64\% \rightarrow 64.38\%$).
* **Flips Positivos (Corregidos por J-13):** $+3$ casos donde el keyword / blend fallaba y la similitud BGE-M3 eligió el guild correcto.
* **Flips Negativos (Degradados por J-13):** $-1$ caso donde el tiebreaker prefirió un guild con mayor similitud semántica engañosa.
* **Resolución Neta:** $+2$ aciertos limpios adicionales.

---

## 4. Desglose Crítico por Categoría de Ambigüedad

| Categoría de Ambigüedad | Muestras ($N$) | Keyword | BGE-M3 Sem | Blended | **J-13 Hybrid** | **Live Matcher (`e337836`)** |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| **`clear_keyword`** | 27 | 81.5% | 70.4% | 88.9% | **88.9%** | 33.3% |
| **`cross_guild_ambiguity`** | 9 | 22.2% | 66.7% | 44.4% | **55.6%** | 11.1% |
| **`semantic_paraphrase`** | 6 | 33.3% | 33.3% | 66.7% | **66.7%** | 16.7% |
| **`historical_real`** | 31 | 41.9% | 29.0% | 41.9% | **45.2%** | **77.4%** |

**Comparación vs kernel viejo (`18e70fa`):** historical_real 74.2% → 77.4% (+3.2pp) | cross_guild_ambiguity 0% → 11.1% (+11.1pp) | semantic_paraphrase 0% → 16.7% (+16.7pp) | clear_keyword 37.0% → 33.3% (-3.7pp). La mejora en categorías ambiguas demuestra que parte de la debilidad previa era del desfase del binario; el gap persistente vs la simulación es debilidad real del stack.

---

## 5. Hallazgos Estructurales (G5, 2026-08-23) — verificación de fuga y causa raíz

**Verificación de fuga de etiqueta (relectura línea por línea del evaluador):** los `== target` solo aparecen en conteos de aciertos post-cálculo; no existe `max(..., 0.85)` ni boost inyectado. La fuga de la v1 fue eliminada. Colinealidad residual del dataset (37/181 items, 20%, nombre del guild en el intent) es limitación del dataset, no del evaluador.

**Causa raíz del 33.3% en `clear_keyword` (18 fallos, desglosados):**
- **7 `unknown`** — problema de REGISTRY/SPAWN, no del matcher: 3 guilds del benchmark no registrados en el runtime (whats_new, vision_moondream, council — verificado contra `/api/v1/guilds`, 42/45) + guilds registrados pero `running: false` con `tools_count: 0` (nunca arrancados, spawn failure → "Unknown guild" reproducido en vivo).
- **11 errores reales de routing** — el matcher elige un guild vecino semánticamente (code→code_reviewer, docker→comfy_ui, git→filesystem, deep_analysis↔deep_web_research, database→bash, monitor→bash, memory→local_llm_proxy). Debilidad real del matcher en ambigüedad: no reproduce el keyword scoring del simulador (matcher.rs trigger phrases vs KEYWORD_RULES del script — gap ya documentado en ROADMAP J-13).

**Conclusión:** la columna "Live Kernel Matcher" mide el stack completo (matcher + registry + spawn + rate-limit), no el matcher puro. Para aislar la contribución del matcher se requiere separar tres columnas: matcher-resuelve / guild-registrado / guild-arranca.

---

## 6. Veredicto

* **J-13 tiebreaker (simulado, sin fuga):** ✅ **GO** — Δ +2.74pp neto, flips +3/-1, datos crudos reproducibles.
* **Matcher real en ambigüedad:** ⚠️ **NO-GO parcial** — la debilidad en `cross_guild_ambiguity`/`semantic_paraphrase` es real (persiste con el kernel al día), aunque menor que lo que sugería el kernel desactualizado.
* **Calidad de la medición:** la columna Live Matcher necesita desglose (matcher vs registry vs spawn) antes de poder atribuirle números al router puro.