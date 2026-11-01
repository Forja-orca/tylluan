# ADR-028: Modelo de decisión generalista local en CPU (F10)

- **Estado:** 📐 **PROPUESTO** — borrador del responsable del cierre (2026-11-01). No se integra ningún modelo hasta que pase el protocolo de medición (§4) y el dueño fije el presupuesto de memoria.
- **Frente:** F10 del roadmap. Respeta la decisión previa: ONNX sigue en el camino caliente de embeddings; llama.cpp es un sidecar efímero para generación; nada de nube por defecto.

## 1. Contexto
Tylluan necesita un modelo pequeño que tome decisiones locales (clasificar intención, elegir herramienta, valorar si un texto parece una inyección, resumir, extraer JSON) en CPU, con el mínimo consumo, en equipos modestos (objetivo de diseño: Raspberry Pi 4/5 o un portátil antiguo sin GPU).

## 2. Candidatos (fichas de Hugging Face y documentación de llama.cpp abiertas en esta sesión)
| Modelo | Tamaño | Licencia | RAM aprox. en Q4* | Dato abierto |
|---|---|---|---|---|
| Granite 4.0 H 1B (primario) | ~1B | Apache 2.0 | 0,8–1 GB | IFEval 78,53 (media), BFCL v3 50,21, herramientas en formato OpenAI |
| Qwen3-1.7B (reserva) | 1,7B | Apache 2.0 | ~1 GB | Contexto 32k, herramientas documentadas; sin cifras en la ficha |
| SmolLM3-3B (escalón de calidad) | 3B | Apache 2.0 | ~1,9 GB | IFEval 76,7; BFCL 92,3 (versión distinta, no comparable) |
| Qwen3-0.6B (ultraligero) | 0,6B | Apache 2.0 | ~0,4 GB | Mismo soporte de herramientas; sin cifras |

*Estimación: parámetros × ~0,61 GB por mil millones (Q4_K_M ≈ 4,89 bits por peso). No medido; no incluye caché KV ni el runtime.

Descartados: Gemma 3 (licencia con términos propios), LFM2 (licencia con umbral de ingresos), Phi-4-mini (la ficha avisa de nombres de función inventados).

**Riesgo principal:** que la arquitectura híbrida de Granite cargue en la compilación de llama.cpp que usamos **no está verificado**; si no carga, la reserva pasa a primario.

## 3. Decisión propuesta
1. Ejecutar el modelo con `llama-server` efímero, un solo hueco, arrancado bajo demanda y parado al terminar, con hilos y memoria limitados.
2. Forzar la salida con **gramática GBNF o `json_schema`**. Las palabras clave del esquema no soportadas se ignoran en silencio: la salida se **valida siempre en código**.
3. Para detectar inyecciones, un clasificador pequeño (p. ej. Prompt Guard 2, 86M) encaja mejor que un generador de 1B en el camino caliente; convertirlo a ONNX no está verificado.

## 4. Protocolo de medición (propio, artesanal, sin LLM-juez)
1. Corpus: 50 casos reales de Tylluan, anonimizados, con la respuesta correcta escrita a mano: 15 intención, 10 herramienta (con «ninguna» como opción), 10 inyección sí/no balanceado, 5 resumen, 10 extracción JSON. Se guarda **fuera del repositorio público**.
2. Entorno fijo: llama.cpp de versión fija, temperatura 0, semilla fija, misma máquina; se anotan CPU, RAM e hilos.
3. Los cuatro candidatos en Q4_K_M; el primario también en Q5_K_M e IQ4_XS.
4. Dos modos: texto libre y con gramática o esquema. Se miden por separado el JSON válido y el acierto.
5. Por caso: acierto manual, JSON válido, latencia, tokens/s y pico de RAM.
6. Barrido de hilos 1, 2 y 4 en el primario; 3 repeticiones, mediana.
7. **Criterio de paso:** ≥ 90 % de JSON válido con gramática, acierto manual igual o superior al de la reserva, y pico de RAM dentro del presupuesto que fije el dueño.
8. Si nada lo cumple: **NO-GO documentado** con el dato.

## 5. Riesgos
Cifras de fichas autoinformadas y de versiones distintas; sin cifras abiertas de llama.cpp en Raspberry Pi (solo de Ollama: p. ej. ~10 tokens/s con un modelo de 1,5B en Pi 5); un modelo de 1B puede dar falsos positivos y negativos en inyección; la velocidad cae con la temperatura de un Pi sin refrigeración.

## 6. No verificado
Que Granite 4.0 H cargue en nuestra compilación; tokens/s y RAM reales de los cuatro candidatos en llama.cpp; el efecto de cada cuantización en la calidad; los indicadores `--no-mmap` y `--mlock` en nuestra compilación; la conversión a ONNX de Prompt Guard 2.

## Impact
Documento de diseño: no cambia código. Fija el protocolo de medición y el criterio de NO-GO antes de elegir modelo.
