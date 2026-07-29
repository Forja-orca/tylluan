# CoherenceGate Layer 4 — Briefing

## Estado: NO-GO (2026-07-29)

**Decisión:** Se cierra la vía "sociedad de modelos <2B para CoherenceGate Layer 4" como NO-GO documentado.

**Razón:** El 75% de exactitud del juez único (Qwen3-0.6B) no era comprensión real — era el modelo cayendo en el default seguro bajo gramática forzada (KEEP/REJECT). Esto también cuestiona si los benchmarks 75%/55% previos reflejaban juicio real o "adivinar la clase mayoritaria" disfrazado por grammar.

**Evidencia:**
- `results_real_model_v3.json`: 52 casos, 3 runs por caso, 0% varianza — confirmación de que el modelo no está discriminando
- Prompt v3 con reasoning detallado: mismo resultado
- Resultado: NO-GO para juicio matizado de relevancia con modelos <2B

## Qué se preserva

Infraestructura lista para reutilizar en otros puntos de inserción donde la tarea sea menos exigente:
- `start_society_servers.py` — 3 servidores en puertos 9001/9002/9003
- `experiment.py` — benchmark con varianza y GO/NO-GO verdict
- Modelos verificados:
  - Proposer: `bartowski/SmolLM2-1.7B-Instruct-GGUF` (Q4_K_M)
  - Critic: `bartowski/Phi-3.5-mini-instruct-GGUF` (Q4_K_M)
  - Synthesizer: `lmstudio-community/Qwen3-0.6B-GGUF` (Q4_K_M)

**Otros puntos de inserción posibles:**
- Síntesis de Night Consolidation (resumir episodios del día)
- Juicio de routing (elegir qué guild procesa una request)
- Cualquier tarea donde el output sea más predecible que "juicio matizado de relevancia"

## Nueva dirección para CoherenceGate Layer 4

**Filtro híbrido** — reglas deterministas + grammar para el grueso de casos, modelo pequeño solo para el subconjunto de ambigüedad genuina (donde capas 1-3 no dan señal clara).

Esto reduce la exigencia sobre el modelo pequeño a justo el caso donde puede aportar algo, en vez de pedirle que resuelva la tarea entera.

### Capas 1-3 existentes (deterministas)
1. **Overlap semántico** — similitud coseno entre query embedding y fragmento
2. **Posición del fragmento** — fragmentos al inicio/final rankean más alto
3. **TF-IDF boost** — palabras clave de la query con peso extra

### Layer 4 propuesto (híbrido)
- **Caso claro** (>0.8 overlap o <0.2): decidir por regla, sin modelo
- **Caso ambiguo** (0.2-0.8): invocar modelo pequeño para juicio
- **Fallback**: si el modelo falla o no responde, usar regla por defecto (KEEP)

## Referencia

- Resultados del benchmark: `results_real_model_v3.json`
- Experimento original: `rebenchmark_real_model.py`
- Servidores SLM Society: `../slm_society/start_society_servers.py`
