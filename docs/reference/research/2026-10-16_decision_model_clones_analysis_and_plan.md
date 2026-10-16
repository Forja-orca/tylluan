# Modelos de decisión tipo Jev/System One: cómo funcionan los clones y plan para Tylluan — 2026-10-16

Fecha: 2026-10-16. Borrador del tech lead. Base: tres informes de investigación (scratchpad RESEARCH_A/B/C) contrastados por mí contra las fuentes en los puntos que decidían. Marcas: [VERIFICADO] = lo leí yo en la fuente; [INFORME] = lo dice un informe y no lo comprobé; [NO VERIFICADO] = sin evidencia suficiente.

## 1. La idea común

Un modelo de decisión no genera texto. Recibe un estado (JSON o texto) y una lista de preguntas tipadas, y devuelve en **una sola pasada** una distribución de probabilidad por pregunta.

- `choice`: elegir una opción entre N (hasta 255 en Laya y Jev, 52 en openjev).
- `score`: nivel ordinal (2 a 10 niveles en Jev).
- `noul`: binario con incertidumbre explícita (el dialecto OpenAI lo llama `predicate`).
- No existe el tipo `box` en ninguna fuente primaria [INFORME A]. `refusal` es un tipo de respuesta, no de pregunta.

API común: `POST /v1/systemone` con `state` y `questions{id:{type, instructions, criteria}}`. La respuesta trae `choice` + `probabilities` + `confidence`, o `score` + `legend`, o `noul`. Dialecto OpenAI (`/v1/decisions`): `input` + lista de `questions`; LiteLLM traduce entre ambos.

## 2. Cómo funciona cada uno por dentro

| Modelo | Quién | Arquitectura | Tamaño | Cómo da la probabilidad | Peso/runtime | Licencia |
|---|---|---|---|---|---|---|
| **Jev** | TypeSafe | No publicada (arquitectura, datos y parámetros: no encontrado). Solo se declara RLCD | — | servicio `api.typesafe.ai/v1/systemone`, 64k contexto, 32k por pregunta | solo nube | servicio de pago |
| **Laya** | Convai Innovations (no TypeSafe) [VERIFICADO] | ModernBERT-large (395M) + cabeza de decisión de 2 capas; cada opción se puntúa en su propio token `[MASK]` | 421M (322M multilingüe) | logit por opción; sale sobreconfiado, se recalibra con temperatura por (tipo, nº opciones): ECE 0,466 a 0,081 [INFORME] | ONNX fp32 ~1,7 GB, contexto 512 [VERIFICADO] | código MIT, pesos Apache-2.0 |
| **Clef / Clef-Flash** | Cloudflare | backbone Qwen congelado + LoRA rank 256 + cabeza de esquema conjunto; un prefill sin decodificar | 27,36B / 9,41B BF16 (~19 GB Flash) [VERIFICADO por búsqueda] | logit por opción, softmax por pregunta | ≥41 GB VRAM (Flash) [INFORME]; multimodal | Apache-2.0 |
| **Decima-small** | amyrmahdy (1 estrella, 18 commits, último 2026-10-10) [VERIFICADO] | e5-small multilingüe + scorer de late interaction de 4,7M con media enmascarada | 122M, int8 127 MB, 2 grafos ONNX | una temperatura global por golden-section (T=0,936) | onnxruntime+numpy, ~20 ms en 1 core x86 [INFORME] | Apache-2.0; licencia de algunos datos de entrenamiento NO verificada |
| **Von 1.3** | wfzyx (914 estrellas, 218 commits) [VERIFICADO] | ModernBERT-large con un `[MASK]` por opción, atención independiente por opción (invariante al orden) | 395M | `von calibrate` recalibra en CPU con etiquetas propias | OpenVINO, sin ONNX [VERIFICADO]; 96 ms p50 en 4 vCPU [VERIFICADO] | Apache-2.0 |
| **kev** | jaredpalmer (9,2k estrellas) [VERIFICADO] | LoRA r16 + pointer head sobre Qwen3.5 | 0,8B a 27B | temperatura por checkpoint | endpoints `/v1/systemone`, `/permute`, `/separate`; 0,8B: 0,65 a 0,70 en fuentes nuevas; 4B: 0,82 a 0,84 [VERIFICADO] | Apache-2.0 |
| **Strands Decider 2B** | AWS | Qwen3.5-2B + LoRA + cabeza de lectura | 2B | ECE 0,064 (autoevaluado) | GPU | Apache-2.0 |
| **OpenThai-SystemOne** | comunidad | Qwen3.5-0.8B | 0,8B, GGUF Q4 529 MB | — | Ollama ≥0.35 con `/v1/systemone` nativo [INFORME] | Apache-2.0 |
| **AgentJev, Nimble 9B, Mapika, mpuig** | varios | — | 0,6B a 9B | — | no aptos o sin verificar | mpuig usa etiquetas del teacher Jev: posible problema de términos |

Patrón común: o bien un **encoder pequeño con cabeza por opción** (Laya, Von, Decima: baratos, CPU) o bien un **LLM con LoRA y lectura de logits** (Clef, kev, Strands, Nimble: precisos, pesados). Todos entrenan con destilación o RL sobre datos sintéticos y calibran con temperatura. Ninguno está evaluado de forma independiente: todas las métricas son de sus autores.

## 3. Cómo se sirven

- **llama.cpp** (PR 29818, fusionado) añade `/v1/systemone` para laya, julia-1, lev, openjev y kev [VERIFICADO]. **Decima no está soportado.** La temperatura va en el GGUF; no hay flags propios.
- **vLLM**: `structured_outputs.choice` fuerza salida exacta sin distribución; un PR posterior añade `/v1/systemone` solo para `choice` [INFORME, vía resumen].
- **Ollama ≥0.35** sirve modelos de este tipo [INFORME].
- **LiteLLM** unifica ambos dialectos y traduce `predicate` y `noul` [INFORME]. Decima no figura.
- **Decima** trae su propio servidor stdlib (`decima serve`, puerto 11436) con `/v1/systemone` [INFORME].
- **Clef**: Workers AI o SGLang/vLLM. Punto sin resolver: el informe A dice `/v1/systemone`; la model card de Clef-Flash muestra `/v1/chat/completions` con SGLang [VERIFICADO por búsqueda]. No lo asumo.

Consecuencia: un único cliente HTTP `/v1/systemone` cubre Decima serve, llama-server, Ollama y LiteLLM.

## 4. Qué hace ya Tylluan (verificado en el repo)

- Contrato `DecisionProvider` + `DecisionQuestion {Choice, Score, Noul}` + `RulesDecisionProvider` en `router/decision/` (143 + 258 líneas).
- **Nadie en producción consume ese trait**: `git grep` fuera de `router/decision` no encuentra consumidores. Todo lo demás está en el harness `examples/decision_fabric_spike.rs` (2.422 líneas).
- Ports ONNX en Rust (`ort` + `tokenizers`) de **Laya** (con valores de referencia a 4 decimales), **Decima-small** (con golden) y variante **Clef-Flash**.
- Mediciones propias (memoria, no revalidadas hoy): LAYA 0,351 frente a 0,531 de las reglas (NO-GO); Clef-Flash NO-GO; Decima-small GO-PROVISIONAL (5 aciertos netos en 245 casos, sin test pareado).
- Dependencias: `ort =2.0.0-rc.10` fijado. El informe C cita rc.14 en otros proyectos; no cambiar el pin sin decisión propia.

Es decir: la mitad "medir" está hecha; la mitad "servir en producción con control" no.

## 5. Qué NO sirve para nosotros

- **Jev (nube)**: datos clínicos a la nube prohibidos salvo aprobación por petición (ADR-020).
- **Clef, Strands, kev ≥4B, Nimble**: GPU o ≥19 GB; incompatibles con el ancla de diseño (Raspberry Pi, médico sin red).
- **Von**: buena calidad, pero 395M, sin ONNX y 96 ms en 4 vCPU. Plan B por `von calibrate`, no plan A.
- **Cualquier modelo como puerta de seguridad**: Decima puntuó `rm -rf /var/lib/postgresql/data` como no destructivo (0,722) según su propio README [VERIFICADO]. Refuerza la regla: permisos y seguridad se quedan en reglas deterministas.

## 6. Plan para Tylluan (propuesta, nada implementado)

Reglas: diseño primero, TDD, MCP + REST siempre, modos `off/shadow/advisory/gate`, un contrato solo sube de modo si gana en sombra con test pareado.

| Fase | Entrega | Criterio de cierre |
|---|---|---|
| **1. Esqueleto sin modelo** | `tylluan_decide` (MCP + REST, seis tools, paridad probada), `RulesDecisionProvider`, registro en sombra (tabla con entrada, decisión de reglas, decisión del modelo, latencia) para enrutado del Scheduler y revisión humana. Sin proveedor de modelo todavía | 6 tools en `tools/list`, tests de paridad MCP/REST, interruptor apagado = `status: disabled` |
| **2. Proveedor HTTP `/v1/systemone`** | Cliente único, configurado por el Inference Gateway (ADR-020), apuntando a `decima serve`, llama-server, Ollama o LiteLLM. Adaptador del dialecto `/v1/decisions` | Prueba contra un servidor real en puerto sandbox; timeout y degradación a reglas |
| **3. Proveedor embebido** | Promover el port ONNX de Decima-small del spike a `router/decision/` tras feature/config, carga perezosa, `spawn_blocking`, presupuesto de inferencia compartido | Paridad numérica contra los valores golden ya existentes; sin romper el camino caliente |
| **4. Calibración propia** | Temperature scaling por contrato con los datos del registro en sombra; ECE y Brier medidos en vivo | ECE publicado por contrato; si Decima no calibra, probar Von con `von calibrate` |
| **5. Evaluación pareada y adopción** | Test pareado (McNemar) sobre el registro en sombra; subir a `advisory` solo el contrato que gane | Sube de modo un contrato por decisión explícita de José; si no gana, NO-GO anotado |

Bloqueos antes de la fase 3: verificar la licencia de los datos de entrenamiento de Decima (marcada como posible NC por el informe B, sin comprobar) y congelar los pesos con hash, porque el proyecto tiene 1 estrella y puede desaparecer.

## 7. Lagunas que quedan abiertas

1. Forma de `refusal` y de la respuesta de Workers AI.
2. Si SGLang/vLLM exponen `/v1/systemone` para Clef o solo chat.
3. Licencia de datos de Decima (ANLI, XNLI).
4. Cuantización int8 de Decima (§9 de su informe técnico) no leída completa.
5. Todas las métricas de ECE y precisión son autoevaluadas; no hay verificación independiente.
6. Los WebFetch de los investigadores devolvieron resúmenes; las cifras críticas que usé están marcadas como verificadas solo cuando las leí yo.
