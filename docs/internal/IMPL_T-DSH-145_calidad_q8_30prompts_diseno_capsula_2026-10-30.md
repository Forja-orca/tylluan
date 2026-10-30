# T-DSH-145 — A/B de calidad q8_0 vs f16 con 30 prompts reales de Coloquio + diseño mínimo de cápsula por agente

- Agente: dsh
- Fecha: 2026-10-30
- Base: `origin/main = 3890ec6`/`aca8935` (push209/210). Sin código: medición en test-run propio + propuesta de diseño.
- Encargo (T2174): A/B de calidad `q8_0` vs `f16` con **30 prompts reales de Coloquio** (juez = comparación de contenido, no solo igualdad exacta) y **propuesta de diseño mínimo de cápsula por agente** (qué se guarda, dónde, cuándo se invalida).

## 1. A/B de calidad (30 prompts reales de Coloquio)

### Montaje
- 30 prompts extraídos de mensajes reales del canal `general` (historial hasta T2150, autores claude-code/antigravity/hermes/jose; instrucciones y peticiones de estado reales, 30-800 caracteres). Guardados en `t145\prompts.json` con turn y autor.
- Modelo: `Qwen3Guard-Gen-0.6B.Q4_K_M.gguf`, `-c 1024 -np 1`, `temperature=0`, `top_k=1`, `n_predict=64`.
- Dos servidores: **f16** (default) y **q8_0** (`--cache-type-k q8_0 --cache-type-v q8_0`); mismas 30 prompts, mismo orden.
- Juez de contenido: `LFM2.5-1.2B-Instruct-Q8_0`, prompt imparcial con las dos respuestas en **dos órdenes** (A/B y B/A), `n_predict=6`, `temperature=0`; se mapea el ganador a f16/q8/empate.

### Resultados

| métrica | valor |
|---|---|
| Salidas **idénticas** (texto exacto) | **22/30 (73,3 %)** |
| Jaccard medio de tokens (contenido) | **0,475** |
| Juez: victorias **f16** | **1** |
| Juez: victorias **q8_0** | **1** |
| Juez: **empates** | **28** |

- En 8/30 prompts la salida difiere textualmente; la similitud de tokens media es 0,475 (contenido parcialmente solapado).
- El juez dio **28 empates**, 1 victoria para cada variante: **no hay evidencia de degradación consistente** de q8_0 en este set, pero el juez es **débil** (ver limitaciones): sus respuestas crudas fueron ruidosas (`"RESPUESTA 1"`, `"EMPATE"`, `"A1: LA LISTA"`), así que los conteos de preferencia son solo indicativos.
- Diferencia con T-DSH-143 (2/10 idénticas): allí eran prompts genéricos de conocimiento con 24 tokens; aquí son instrucciones/estados reales con 64 tokens y más contexto de salida → más coincidencia textual.

### Lectura
- **q8_0 no muestra degradación de contenido medible con este juez** (28/30 empates) y la coincidencia textual es alta (22/30). Pero el juez es demasiado débil para concluir no-inferioridad: hace falta un juez mayor o evaluación humana/tarea específica.
- Recomendación operativa: **f16 por defecto** para cápsulas (decisión del TL en T2174), q8_0 solo tras un A/B con juez fiable; el ahorro de q8_0 (47 % de cápsula, ~52 MB RAM) queda como opción pendiente de validar.

## 2. Propuesta de diseño mínimo de cápsula por agente

### Qué se guarda
- **Estado KV del slot de llama.cpp** (`POST /slots/{id}?action=save`) del contexto del agente en un canal, en **f16** por defecto (q8_0 solo si un A/B lo valida).
- **Sidecar JSON** con metadatos (no contenido): `agent_id`, `channel_id`, `turn_watermark`, `model_id`, `model_hash`, `ctx_size`, `llama_build`, `created_at`, `size_bytes`, `sha256`.
- No se guarda texto ni se duplica memoria en Silva; la cápsula es un fichero de estado opaco.

### Dónde
- `DATA_DIR/capsules/<agent_id>/<channel_id>-<turn>.bin` + `.json` (o `D:\tylluan-capsules\...` si se separa del DATA_DIR), **fuera de git** y con permisos locales. Un fichero por agente+canal (se sobrescribe el anterior = mínimo espacio), o N últimos si se quiere historial.

### Cuándo se invalida (borrar/ignorar)
1. Cambia `model_id`/`model_hash`, `ctx_size` o la **build de llama.cpp** (la cápsula no es portable).
2. `sha256` del fichero no coincide o el fichero está corrupto/truncado.
3. TTL configurable (p. ej. 24-72 h) o `turn_watermark` por detrás del último turno del canal en más de N turnos.
4. El agente cambia de identidad/rol (`.tylluan/agents.toml`) o de canal.
5. Se supera un tope de tamaño por agente (p. ej. 2 GB) → LRU por `created_at`.
6. El servidor no soporta `--slot-save-path` o no está vivo al restaurar (fallback a prompt en frío).

### Flujo mínimo (MVP)
1. Arrancar `llama-server` con `--slot-save-path <dir>` (f16, `-np 1`).
2. Tras procesar el contexto del agente en el canal: `save` → sidecar con hash/watermark.
3. Al iniciar sesión del agente: validar metadatos/hash/TTL → `restore` → prompt; si falla, prompt en frío y `save` al terminar.
4. Métricas: aciertos/fallos de restore, ms ahorrados (vs prompt frío), GB en disco por agente.

### Riesgos y límites
- La cápsula **contiene el contexto en forma de KV** (material sensible): tratarla como dato privado local, no sincronizarla ni subirla; no es redactable a posteriori.
- Tamaño: ~196 KB/token en 1.3B/4096 (T-DSH-143), ~115 KB/token en 0.6B/1024 (T-DSH-142) → con prompts largos son cientos de MB por cápsula; el tope/TTL es obligatorio.
- Solo aplica a modelos locales servidos por llama.cpp; no a ONNX/embeddings ni a modelos remotos.
- Restaurar no evita el coste de generación, solo el prompt processing (T-DSH-143: 18,2 s → 0,14 s).

## Impact

- Calidad: q8_0 **no muestra degradación consistente** en 30 prompts reales (28/30 empates del juez; 22/30 salidas idénticas), pero el juez es débil → no se puede declarar no-inferioridad; f16 sigue siendo el default seguro.
- Diseño: hay una propuesta mínima concreta (qué/dónde/cuándo) para cápsula por agente, con invalidación por modelo/ctx/build/hash/TTL/watermark/tope y fallback a frío; no implementada (solo propuesta).

## No verificado

- **Juez débil**: LFM2.5-1.2B con `n_predict=6` produjo veredictos ruidosos (textos como `"RESPUESTA 1"`/`"EMPATE"`), así que 28 empates no equivalen a no-inferioridad; falta un juez mayor o evaluación humana.
- 30 prompts son instrucciones/estados reales de Coloquio, no pares pregunta-respuesta con ground truth; no hay métrica de corrección, solo coincidencia textual/Jaccard y preferencia del juez.
- Una sola corrida; `n_predict=64` (respuestas cortas); no se probaron respuestas largas ni tareas de código.
- El A/B se hizo con 0.6B Q4_K_M; no se repitió con 1.3B/4B ni con otros `ctx_size`.
- La propuesta de diseño no está implementada ni probada en un flujo real de agente; los topes (2 GB, TTL 24-72 h) son valores propuestos, no medidos.
- No se tocó `E:\tylluan` ni el kernel de producción (puerto de producción); llama-server de prueba en 47100, parado al terminar.
