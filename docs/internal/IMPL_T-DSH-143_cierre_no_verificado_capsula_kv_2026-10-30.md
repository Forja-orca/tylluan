# T-DSH-143 — Cierre de «no verificado» del spike de cápsula KV (restore tras reinicio, ctx 4096 con ≥1B, calidad q8_0 vs f16)

- Agente: dsh
- Fecha: 2026-10-30
- Base: `origin/main = 3890ec6` (push209). Sin código: solo medición en test-run propio. Sin tocar producción.
- Encargo (T2168): cerrar los «no verificado» de T-DSH-142 — (A) restore tras **reiniciar el proceso**, (B) contexto **4096** con modelo **≥1B** si hay en caché, (C) **calidad q8_0 vs f16 en 10 prompts** (salidas comparadas, temp=0). Solo medir, test-run propio.

## Montaje

- `llama-server.exe` build **11507** (0.6.0-dev, commit c811cb8f0); puerto propio **47099**, `-np 1`, `--slot-save-path` propio.
- A: `Qwen3Guard-Gen-0.6B.Q4_K_M.gguf`, `-c 1024`, f16.
- B: `D:\models\TheBloke\deepseek-coder-1.3b-instruct-GGUF\deepseek-coder-1.3b-instruct.Q8_0.gguf` (1,33 GB), `-c 4096`, f16; prompt de ~1.834 tokens.
- C: 0.6B, `-c 1024`, dos servidores: f16 y `--cache-type-k q8_0 --cache-type-v q8_0`; 10 prompts, `n_predict=24`, `temperature=0`, `top_k=1`; comparación exacta de texto.

## A) Restore tras reiniciar el proceso (0.6B, ctx 1024)

| paso | resultado |
|---|---|
| prompt en frío (antes de guardar) | 454,3 ms |
| guardar cápsula | 59,1 ms interno / 91,0 ms HTTP; **26,14 MB** (27.414.980 B, 239 tokens) |
| **parar servidor y arrancar uno nuevo** | — |
| prompt en frío en el proceso nuevo | 443,8 ms |
| restaurar cápsula en el proceso nuevo | **200 OK**, 34,6 ms interno / 36,7 ms HTTP |
| prompt tras restaurar | **32,3 ms** (×13,7 más rápido que el frío) |

→ **La cápsula se puede restaurar en un proceso distinto** del que la guardó (mismo binario/modelo/ctx), sin re-procesar el prompt.

## B) Contexto 4096 con modelo 1.3B (deepseek-coder-1.3b Q8_0)

| métrica | valor |
|---|---|
| Cápsula | **343,9 MB** (360.609.044 B, **1.834 tokens**) |
| Guardar | 497,4 ms interno / 499,3 ms HTTP |
| Restaurar | 269,0 ms interno / 270,6 ms HTTP |
| `prompt_ms` en frío | **18.226,3 ms** |
| `prompt_ms` tras restaurar | **143,1 ms** (**×127** más rápido) |
| `predicted_ms` (16 tokens) | 1.920,2 ms / 1.826,0 ms tras restore |
| RAM privada / WS | 893,2 MB / 2.102,6 MB (90 hilos) |
| Arranque | 6,22 s |

→ A ctx 4096 y 1.3B la cápsula pesa **~344 MB** (~196 KB/token) y restaurarla (270 ms) ahorra **18,1 s** de prompt processing.

## C) Calidad q8_0 vs f16 en 10 prompts (temp=0, salidas comparadas)

| resultado | valor |
|---|---|
| Salidas **idénticas** | **2/10** |
| Salidas **distintas** | **8/10** |

Ejemplos de divergencia (misma prompt, greedy):
- «Define latencia p50»: f16 → “Latencia p50 is the time it takes for the **body** to respond…”; q8_0 → “…for the **population** to reach 5…».
- «Que es un slot en llama.cpp?»: f16 → “¿Cómo se puede hacer un juego de slot…”; q8_0 → “C++ / C++ Programming… Home / …”.
- «Escribe el comando para listar ficheros en PowerShell»: f16 → “En este ejercicio debemos escribir un comando…”; q8_0 → “El comando para listar ficheros en PowerShell es Get…”.

→ **q8_0 NO es equivalente textual a f16**: con `temperature=0` cambia la salida en 8 de 10 prompts (frases/continuaciones distintas; en algunos casos el contenido cambia de tema). No se midió perplejidad ni calidad semántica: es comparación de texto generado.

## Comandos

```
llama-server.exe -m <modelo.gguf> --port 47099 -c <ctx> -np 1 --slot-save-path <dir>\slots
# guardar/restaurar:
POST /slots/0?action=save    {"filename":"capsule.bin"}
POST /slots/0?action=restore {"filename":"capsule.bin"}
# variante cuantizada:
... --cache-type-k q8_0 --cache-type-v q8_0
```

## Impact

- **Restore tras reinicio: SÍ funciona** (mismo binario/modelo/ctx): 32,3 ms frente a 443,8 ms de prompt en frío (×13,7), con la cápsula en disco (26 MB a 239 tokens).
- **ctx 4096 con 1.3B: cápsula de 344 MB** (~196 KB/token); guardar 0,5 s, restaurar 0,27 s, y evita 18,2 s de prompt processing (×127). Es la evidencia que faltaba para dimensionar cápsulas en modelos ≥1B.
- **q8_0 no es no-inferior a nivel de texto**: 8/10 salidas distintas con greedy; reduce la cápsula ~47 % y ~52 MB de RAM (T-DSH-142), pero si se integra habría que validar calidad con una evaluación de la tarea, no asumir equivalencia.
- Recomendación: f16 por defecto para cápsulas; q8_0 solo con A/B de calidad específico.

## No verificado

- Una sola corrida por bloque; 10 prompts y un modelo pequeño (0.6B) para calidad; sin perplejidad ni evaluación semántica, solo igualdad de texto con greedy.
- B usó deepseek-coder-1.3b (base/instruct Q8_0): el prompt largo (1.834 tokens) y el modelo dieron salidas pobres («\n\n…»), así que B mide **cápsula/latencia**, no calidad.
- No se probó restore tras reinicio con el modelo de 1.3B ni con ctx 4096; solo con 0.6B/1024.
- No se probó `-np > 1`, ni cápsulas de varios slots, ni disco lento; ni restore entre builds distintas de llama.cpp o tras mover el fichero de modelo (se espera que falle).
- No se midió el coste de escribir 344 MB en disco (aquí fue rápido) ni el impacto en SSD/espacio.
- No se tocó `E:\tylluan` ni el kernel de producción (puerto de producción); llama-server de prueba en 47099, parado al terminar.
