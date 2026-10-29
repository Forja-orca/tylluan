# Servidor verde: qué se puede afirmar y dónde recortar (AG-16, verificado por el TL)

Autor: Antigravity (borrador v1 y v2), corregido por el TL el 2026-10-29. Regla de este documento: solo entra una cita si el TL abrió la fuente y la frase aparece literal. Lo demás queda marcado "no verificado".

## P1. Qué significa "verde" (verificado)

| Fuente | URL | Cita literal verificada | Qué aporta |
|---|---|---|---|
| Green AI, Schwartz et al., 2019 | https://arxiv.org/abs/1907.10597 | "making efficiency an evaluation criterion for research alongside accuracy and related measures" | La eficiencia se evalúa junto a la precisión |
| Strubell et al., 2019 | https://arxiv.org/abs/1906.02243 | "by quantifying the approximate financial and environmental costs of training a variety of recently successful neural network models for NLP" | Cuantifica coste económico y ambiental |
| Software Carbon Intensity (Green Software Foundation) | https://sci.greensoftware.foundation/ | "This specification defines a methodology for calculating the rate of carbon emissions for a software system" | Fórmula de la página: SCI = (O + M) per R |

- La página del SCI no menciona ISO/IEC 21031 ("no normative references"): no se afirma que sea esa norma.
- MLPerf Power: no verificado. La página https://mlcommons.org/ solo enlaza "Power" sin descripción.
- La cifra de unos 626 000 lbs de CO2 de Strubell no aparece en el resumen de arXiv; no se cita.

## P2. ¿Alguien dice ser "el primer servidor verde"? (resultado de Antigravity, no repetido por el TL)
Consultas: `"first green AI server" OR "first green memory server" OR "green agent memory" OR "green AI memory"` y búsquedas por sitio en mem0.ai, getzep.com, letta.com y el repositorio de Letta con las palabras green, carbon, energy y sustainable. Resultado: no encontrado. Conclusión que sí vale: no se debe escribir "el primero" sin evidencia propia.

## P3. Medir consumo sin vatímetro en Windows (no verificado)
Antigravity citó frases de MLCommons, CodeCarbon, Scaphandre y Microsoft Learn que el TL no pudo encontrar en esas páginas (dos URLs dan 404). Solo consta, de la página de Scaphandre, que soporta "Windows 10, 11 and Server 2016/2019/2022". Qué mide cada herramienta, su precisión y si exigen un driver en Windows queda como hipótesis por comprobar: la tarea T-DSH-141 mide RAM, hilos y CPU sin necesitar vatios.

## P4. Dónde recortar RAM y CPU (verificado lo que consta)

| Técnica | Ajuste | Fuente y cita literal verificada | Cómo probarlo aquí |
|---|---|---|---|
| Cuantización a 8 bits de los modelos ONNX | herramienta de cuantización de ONNX Runtime | https://onnxruntime.ai/docs/performance/model-optimizations/quantization.html: "Quantization in ONNX Runtime refers to 8 bit linear quantization of an ONNX model." Ahorro de memoria: estimación sin fuente | embeddings int8 frente a float32; Recall@5 con las 40 preguntas |
| Hilos de ONNX Runtime | `intra_op_num_threads` | https://onnxruntime.ai/docs/performance/tune-performance/threading.html: "Controls the total number of INTRA threads to use to run the model." | variar 4, 8 y 14 hilos; mismo Recall@5, medir tiempo y CPU |
| Parámetros del índice HNSW | solo los que expone el crate `instant_distance` | https://docs.rs/instant-distance/latest/instant_distance/struct.Builder.html: métodos `ef_construction`, `ef_search`, `select_heuristic`, `ml` y `seed`; no hay método para M | reconstruir con otro `ef_search`; Recall@5 y latencia |
| Caché de SQLite | `PRAGMA cache_size` (un valor negativo se toma como KiB) | https://sqlite.org/pragma.html: "the suggested maximum number of database disk pages that SQLite will hold in memory at once" | RAM privada durante un reindex completo |
| Mapeo de memoria de SQLite | `PRAGMA mmap_size` | https://sqlite.org/pragma.html: "Query or change the maximum number of bytes that are set aside for memory-mapped I/O on a single database" | latencia de lectura con las 40 preguntas |

- La cuantización escalar o de producto de vectores no está en `instant_distance`: no aplicable sin cambiar de crate.
- Descargar el modelo tras un tiempo inactivo: idea de Antigravity, sin fuente; se medirá en T-DSH-141.

## Impact
Solo documentación. Fija qué se puede decir (definiciones verificadas, nada de "el primero") y qué recortes probar con A/B de no inferioridad antes de aplicarlos. No cambia código.

## No verificado
- P2 se basa en las búsquedas de Antigravity; el TL no las repitió.
- P3 completo, MLPerf Power, y las cifras de ahorro (75 %, 30-50 %, ±5 %), que Antigravity dio sin fuente.
- Corrección del TL: de los textos de Antigravity se descartaron las frases de MLCommons, CodeCarbon, Scaphandre y Microsoft Learn por no aparecer en las páginas abiertas.
