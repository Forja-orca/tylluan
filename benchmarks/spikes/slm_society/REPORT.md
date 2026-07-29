# REPORT: Sociedad Interna de SLLMs — Spike Results & Verdict

> **Fecha**: 2026-07-29  
> **Veredicto**: 🚫 **NO-GO para CoherenceGate Layer 4** (Infraestructura de Sociedad Multi-Agente conservada para tareas asíncronas / Night Consolidation / Routing).

---

## 1. Contexto & Hipótesis

Se evaluó si una **Sociedad Interna de SLLMs (<2B)** con 3 modelos generativos distintos operando en protocolo **Proposer-Critic-Synthesizer** podía superar el 75% ruidoso del juez único de 0.5B sobre los 52 casos reales de CoherenceGate (`cases_real_50.json`).

---

## 2. Hallazgo Empírico

1. **Incapacidad de Juicio Relevante Matizado en <2B**:
   - Modelos pequeños (<2B) colapsan hacia la etiqueta mayoritaria de la gramática GBNF o alucinaciones especulares (*echoing* de la entrada), sin demostrar comprensión semántica real de la relevancia de memorias.
2. **Análisis de Falsa Precisión (75% Baseline Juez Único)**:
   - Se confirmó que el 75% obtenido previamente por un juez único de 0.5B no representaba razonamiento conceptual profundo, sino una inclinación probabilística hacia la clase por defecto bajo la restricción del parser.
3. **Latencia**:
   - La evaluación secuencial multi-modelo acumuló una latencia elevada e inviable para la ruta caliente de recuperación (*recall hot path*).

---

## 3. Decisiones de Arquitectura

1. **NO-GO para CoherenceGate Layer 4 vía Sociedad <2B**:
   - Se descarta la sustitución directa de CoherenceGate Layer 4 por la sociedad de 3 modelos pequeños.
2. **Reorientación Híbrida de CoherenceGate Layer 4**:
   - CoherenceGate Layer 4 se replantea como un **filtro híbrido (Reglas Deterministas + Heurísticas)**, reservando la inferencia generativa únicamente para la franja de ambigüedad genuina donde las capas 1-3 no entreguen una puntuación concluyente.
3. **Conservación de la Infraestructura Multi-Agente**:
   - El arnés y los scripts de servidor (`benchmarks/spikes/slm_society/experiment.py`, `start_society_servers.py`) se preservan en la suite de pruebas como infraestructura reutilizable para tareas asíncronas en segundo plano (p. ej., agregaciones en *Night Consolidation* o arbitraje de intenciones).
