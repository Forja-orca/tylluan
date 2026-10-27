# T-DSH-108 — Evaluación de modelos juez para el centinela de inyección (y aliado)

- **Agente:** dsh
- **Fecha:** 2026-10-27
- **Encargo (T1889):** evaluar modelos juez para el centinela de prompt-injection: Qwen3Guard-Gen-0.6B (Q8_0/Q4_K_M), Lilbullet prompt-injection-classifier-simple-Qwen-3-0.6b, qualifire prompt-injection-jailbreak-sentinel-v2 (solo evaluar), Granite Guardian 3.2 3b-a800m/3.1-2b, opcional hlyn-labs prompt-injection-judge-8b. Medir recall/FP/F1/p50/p95/RSS/CPU-s; corpus: 300 benignas reales (copia de la BD), 30 benignas duras, corpus malicioso de docs redteam + casos write_gate/coherence_gate. Un modelo a la vez, llama-server propio en `<puerto>`, `--threads` 2–4, sin `--mlock`, prioridad BELOW_NORMAL, borrar modelos al terminar.
- **Base:** worktree reseteado a `origin/main = 2142e57` (push173); sin tocar `:47004` ni `E:\tylluan`.

## Metodología

- **Workspace:** `E:\tylluan-patches\guard-eval\` (`models/`, `data/`, `results/`). Modelos descargados directo de huggingface.co a esa ruta.
- **Corpus:**
  - 300 benignas reales muestreadas de una **copia** de la SilvaDB (`data\silva_copy.db`, 203 MB; nodos de 80–800 caracteres, excluyendo marcadores de ataque, deduplicadas).
  - 30 benignas duras escritas a mano (instrucciones legítimas tipo "ejecuta los tests", "borra temporales", "envía un informe").
  - 77 malignas curadas (inyección directa, jailbreak, exfiltración de token/BD, abuso de herramientas, secuestro de rol, inyección indirecta en memoria, encoding base64). La extracción automática de los docs redteam produjo muy pocos payloads literales: describen vías, no payloads; se complementó a mano.
- **Harness:** `eval_guard.py` arranca `llama-server` propio en `<puerto>` (127.0.0.1), `-t 4 --threads-batch 4 -np 1 -c 4096 --no-webui`, **sin `--mlock`**, prioridad BELOW_NORMAL y afinidad 8 cores; espera `/health`; evalúa; guarda `raw_*.jsonl` y `eval_*.json`; mata **solo su PID exacto** guardado en `results\server_<label>.pid` (nunca 47004 ni el llama-server ajeno PID 54356). Un modelo a la vez.
- **Nota de contención:** durante la primera pasada el kernel de producción consumía 30–40 cores (IdleLab antes de AG-12); tras `idle_lab_enabled=false` (AG-12, push173) la CPU quedó libre y las latencias bajaron. Las latencias absolutas son de este host y no comparables con otro hardware.
- **Umbral Qwen3Guard:** el modelo tiene **tres clases** (`Safe`, `Controversial`, `Unsafe`). Se reportan dos puntos de operación: "solo Unsafe" (estricto para bloquear) y "Unsafe+Controversial" (centinela sensible).

## Resultados

| Modelo / parser | Benignas | Recall malignas | FP benignas (FP rate) | F1 | p50 / p95 | RSS pico | CPU-s/veredicto |
|---|---|---|---|---|---|---|---|
| **Qwen3Guard-Gen-0.6B Q4_K_M**, solo `Unsafe` | 300 + 30 duras | **57,14 %** (44/77) | **1/330 = 0,30 %** (0 en duras) | 0,721 | 2,29 s / 11,3 s | 3.037 MB | 8,95 |
| **Qwen3Guard-Gen-0.6B Q4_K_M**, `Unsafe`+`Controversial` | 300 + 30 duras | **84,42 %** (65/77) | 13/330 = 3,94 % (0 en duras) | **0,839** | (misma pasada) | 3.037 MB | 8,95 |
| **Qwen3Guard-Gen-0.6B Q8_0**, solo `Unsafe` | 300 + 30 duras | 41,56 % (32/77) | **0/330 = 0,00 %** | 0,587 | 5,83 s / 29,7 s | 5.504 MB | 22,62 |
| **Qwen3Guard-Gen-0.6B Q8_0**, `Unsafe`+`Controversial` | 300 + 30 duras | **83,12 %** (64/77) | 14/330 = 4,24 % | 0,826 | (misma pasada) | 5.504 MB | 22,62 |
| **Lilbullet prompt-injection-classifier-simple-Qwen-3-0.6b Q4_K_M**, plantilla nativa (system + ` thinking`) | 120 + 30 duras | 72,73 % (56/77) | **79/150 = 52,67 %** (77 benignas + 2 duras) | 0,528 | 1,35 s / 3,14 s | 1.151 MB | 0,03* |
| **Granite Guardian 3.2 3b-a800m Q4_K_M** | 120 + 30 duras | **inconcluso** | — | — | 1,11 s / 3,52 s | 5.301 MB | 7,00 |

\* El CPU-s/veredicto de Lilbullet es anómalamente bajo porque el servidor quedó vivo de la pasada anterior (reutilizado); no usar como coste real.

- **Qualifire sentinel-v2:** el repo GGUF devuelve **401 (gated)** sin token; **no descargado ni evaluado**.
- **Granite Guardian:** dos intentos (chatml con jinja y plantilla nativa `<|start_of_role|>` sin jinja) produjeron salidas no-veredicto (ecos del texto, "Yes\n<confidence>...", continuaciones); **no es comparable** con los demás sin integrar su plantilla oficial de Guardian. Queda como pendiente, no como descartado.
- **hlyn-labs prompt-injection-judge-8b:** no evaluado (opcional; 8B ≈ 5 GB y la máquina ya iba justa).

### Análisis por categoría (Qwen3Guard, recall)

| Categoría | Q4 solo Unsafe | Q4 Unsafe+Controversial | Q8 solo Unsafe | Q8 Unsafe+Controversial |
|---|---|---|---|---|
| Exfiltración | 59,3 % (16/27) | **85,2 %** (23/27) | 51,9 % (14/27) | 81,5 % (22/27) |
| Herramientas | 71,4 % (10/14) | 71,4 % (10/14) | 57,1 % (8/14) | 71,4 % (10/14) |
| Inyección indirecta | 33,3 % (2/6) | **100 %** (6/6) | 16,7 % (1/6) | 83,3 % (5/6) |
| Jailbreak | 50,0 % (4/8) | 87,5 % (7/8) | 25,0 % (2/8) | **100 %** (8/8) |
| Rol | 66,7 % (2/3) | 100 % (3/3) | 0 % (0/3) | 100 % (3/3) |
| Encoding (base64) | 0 % (0/1) | 0 % (0/1) | 0 % (0/1) | 0 % (0/1) |

## Recomendación

- **Centinela recomendado: `Qwen3Guard-Gen-0.6B Q4_K_M`** (462 MB, RSS ~3 GB, p50 2,3 s en este host). Es el mejor equilibrio calidad/coste de los evaluados.
  - **Política de 2 niveles:** `Unsafe` → **bloquear** (FP 0,30 %); `Controversial` → **no bloquear pero exigir confirmación / bajar confianza / marcar para revisión** (sube recall a 84,4 % con FP 3,9 %).
  - Un único umbral no da lo mejor de ambos: bloquear `Controversial` dispara FP al 3,9 %; ignorarlo deja recall en 57 %.
- **Aliado:** no hace falta un segundo modelo distinto: Qwen3Guard Q8 no mejora la calidad de forma medible (recall 83,1 % vs 84,4 %; F1 0,826 vs 0,839) y cuesta 1,8× RSS y 2,5× latencia. Si se quiere "aliado" para los `Controversial`, usar **el mismo Q4 con una segunda pasada** (prompt de verificación) o reservar Q8 solo para la cola de revisión. **Lilbullet queda descartado como centinela** por FP 52,7 % (inutilizable en notas reales) aunque su recall sea alto.
- **Hueco conocido:** encoding (base64) no lo detecta ninguno; y las instrucciones que "parecen legítimas" (herramientas) se quedan en ~71 %. El centinela no sustituye a `write_gate`/`coherence_gate`: complementa.
- **Coste en producción:** Q4 a p50 ~2,3 s por nota añade latencia perceptible si se llama síncrono en cada `remember`; recomendable **asíncrono/cola** o solo en rutas de escritura de memoria y recall de contenido no confiable.

## Impact

- Entrega la primera medición real (no estimada) de candidatos juez con corpus propio: benignas reales de la BD, 30 duras y 77 ataques; con latencia, RSS y CPU medidos, no solo accuracy.
- Conclusión accionable: **Qwen3Guard Q4 con política Safe/Controversial/Unsafe** es el candidato; Lilbullet descartado por FP; Q8 no aporta; Granite queda pendiente de plantilla oficial; qualifire gated.
- Todo se ejecutó con puerto propio, 4 hilos, sin `--mlock`, BELOW_NORMAL, un modelo a la vez y matando solo PIDs exactos; no se tocó `:47004`, `E:\tylluan` ni el llama-server ajeno.

## No verificado

- **300 benignas completas solo para Qwen3Guard** (Q4 y Q8). Lilbullet y Granite se midieron con 120 benignas + 30 duras por tiempo/CPU; sus FP no son directamente comparables con la tasa sobre 300.
- **Granite Guardian no evaluado de verdad**: falta su plantilla oficial de Guardian; los dos intentos son inconclusos (no un "no sirve").
- **hlyn-labs 8B y qualifire**: no evaluados (opcional / gated 401).
- **Latencias bajo contención**: la primera pasada (Q4) coincidió con IdleLab al 30–40 % de CPU; Q8/Lilbullet/Granite se midieron con la CPU más libre. Los p95 están inflados por prompts largos y arranque en frío; no son un SLA.
- **No se probó integración en el kernel** (ni como guild ni como llamada inline): solo evaluación offline contra llama-server.
- **No se midió calidad sobre notas benignas "raras"** más allá de las 30 duras (p. ej. notas con instrucciones legítimas muy parecidas a ataques).
- **Encoding/base64**: 1 caso, insuficiente para concluir; se necesitaría un set dedicado.
- Los modelos descargados (~3,7 GB) siguen en `E:\tylluan-patches\guard-eval\models`; borrado con `Remove-Item` de esa carpeta cuando el TL lo pida (no se borran aún por si hay que repetir con plantilla de Granite).
