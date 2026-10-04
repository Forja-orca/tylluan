
---

## 6. Resultado de los gates de F1 (2026-10-04) — NO-GO con dato

Ejecutados 2 gates del harness (n=50/celda, kernel de test aislado, mxbai CPU):

| Criterio ADR | Gate 1 (385e873) | Gate 2 (9d86156, fix LRU) | Baseline OFF | Veredicto |
| :--- | :--- | :--- | :--- | :--- |
| do C=1 p50 <= 121ms | 1117ms | 1481ms | 121ms | FAIL |
| do C=4 p50 (base 272ms) | 634ms | 6339ms | 272ms | FAIL |
| do C=8 p50 (base 739ms) | 1101ms | 452ms (p95 11.9s) | 739ms | FAIL |
| 0 rechazos | 0 | 0 | - | PASS |
| 0 nodos stale en ventana | 29 | 4+8 | 0-3 | FAIL |
| recall sin degradar | mejora (C=4 43.3->20.3s, C=8 84.2->38.2s) | degradado (C=4 55.1s) | - | G1 PASS / G2 contaminado |

Lectura honesta:

1. El warm-start (independiente del flag routing) dio la unica mejora real: recall C=4/C=8 en el gate 1 — via LRU del engine y query_embed_cache en el camino que ya existia (embed()).
2. La clase ROUTING via batcher FALLA el criterio de do en ambos gates, incluso con la LRU delante (fix 9d86156). El overhead de ventana+cola del batcher no compensa para embeds que no llegan en rafaga concurrente; y la correlacion stale>0 solo con el flag ON (mecanismo sin confirmar, 0 rechazos registrados) incumple el criterio de stale por si sola.
3. El gate 2 muestra degradacion TAMBIEN en recall (codigo intacto) — la flota compilaba en la misma maquina durante la ventana. El fallo de do es consistente en los 2 gates y se trata como senal real; el resto del gate 2 queda marcado como contaminado.

DECISION (aplicando la regla del propio ADR: fase que falla su gate revierte su flag):

- embed_batching_routing_enabled queda default OFF (ya lo estaba) — F1 NO-GO.
- embed_warm_start queda default ON (beneficio de recall medido en gate 1, riesgo bajo, config-gated para apagarlo).
- El codigo del batcher ROUTING queda gated y documentado; F2 (recall-class) NO se abre.
- Reabrir F1 exigiria: maquina en reposo, ventana libre de builds del equipo, y resolver primero el mecanismo del stale bajo flag ON.
