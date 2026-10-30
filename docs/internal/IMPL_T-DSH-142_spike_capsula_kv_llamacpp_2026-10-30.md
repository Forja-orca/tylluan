# T-DSH-142 — Spike: cápsula KV de modelos locales con llama.cpp (`--slot-save-path` + `--cache-type-k/v`)

- Agente: dsh
- Fecha: 2026-10-30
- Base: `origin/main = ef5bc1b` (push207). Sin código: spike de medición en test-run propio. Sin tocar producción.
- Encargo (T2164): spike `llama.cpp slot-save-path` + `cache-type-k/v` para cápsula KV de modelos locales (**solo medir**, test-run propio), informe con cifras y comando.

## Entorno

- Binario: `C:\Users\FoRJa\.cache\tylluan\llama-cpp-b11507\llama-server.exe` → `version: 0.6.0-dev (build 11507, commit c811cb8f0)`, Clang 20.1.8 x86_64.
- Modelo: `C:\Users\FoRJa\.cache\tylluan\models\Qwen3Guard-Gen-0.6B.Q4_K_M.gguf` (461,8 MB).
- Puerto propio **47098**, `-c 1024 -np 1`, `--slot-save-path t142\slots`; nada de producción.
- Capacidades verificadas en `--help`: `--slot-save-path PATH` ("path to save slot kv cache"), `-ctk/--cache-type-k`, `-ctv/--cache-type-v`, endpoints `/slots/{id}?action=save|restore`.
- Prompt de prueba: ~200 tokens de contexto + pregunta; generación `n_predict=32`, `temperature=0`.

## Comando (variante f16, la de por defecto)

```
llama-server.exe -m Qwen3Guard-Gen-0.6B.Q4_K_M.gguf --port 47098 -c 1024 -np 1 \
  --slot-save-path <dir>\slots
# guardar:  POST /slots/0?action=save    {"filename":"capsule-f16.bin"}
# restaurar: POST /slots/0?action=restore {"filename":"capsule-f16.bin"}
```

Variante cuantizada: añadir `--cache-type-k q8_0 --cache-type-v q8_0`.

## Resultados

| métrica | f16 (default) | q8_0 (K y V) |
|---|---|---|
| Fichero de cápsula | **27,9 MB** (29.250.244 B) | **14,82 MB** (15.541.444 B) → **−47 %** |
| Tokens guardados (`n_saved`) | 255 | 255 |
| Guardar (interno / HTTP) | 48,5 ms / 59,1 ms | 34,7 ms / 36,8 ms |
| Restaurar (interno / HTTP) | 54,0 ms / 57,0 ms | 20,0 ms / 22,1 ms |
| `prompt_ms` en frío | 538,1 ms | 567,4 ms |
| `prompt_ms` tras restaurar | **49,5 ms** (×10,9 más rápido) | **47,3 ms** (×12,0 más rápido) |
| `predicted_ms` (32 tokens) | 1.520,7 ms | 1.235,6 ms |
| RAM privada del servidor (reposo) | 474,7 MB | **422,5 MB** (−52 MB) |
| Working set (reposo) | 764,6 MB | 712,7 MB (−52 MB) |
| Hilos | 90 | 90 |
| Arranque | 5,41 s | 3,35 s |
| Salida generada (32 tokens, temp=0) | idéntica | idéntica |

- **Tamaño por token** (útil para extrapolar): f16 ≈ **114,7 KB/token** (29.250.244/255); q8_0 ≈ **60,9 KB/token**. Con `-c 4096` y el mismo modelo, una cápsula llena sería ≈ 448 MB (f16) / 238 MB (q8_0) — la cápsula crece linealmente con el contexto realmente usado, no con `n_ctx`.
- **La restauración evita el prompt processing**: 538 ms → 49,5 ms en f16; el coste de generación posterior no cambia (la cápsula es el estado KV del prompt, no del muestreo).

## Lectura para Tylluan

- `--slot-save-path` + `/slots/0?action=save|restore` **funciona** en el llama.cpp que ya cachea el proyecto (build 11507): guardar/restaurar es barato (decenas de ms) y ahorra ~90 % del coste de prompt en la reanudación.
- `--cache-type-k/v q8_0` **reduce la cápsula un 47 % y ~52 MB de RAM** a este contexto, con salida idéntica en la prueba; el coste es una posible pérdida de precisión no medida aquí (no se midió perplejidad ni calidad en tareas largas).
- **No hay wiring actual en Tylluan**: `grep` de `slot-save-path|cache-type-k|cache-type-v` en `crates/` = 0 referencias; hoy el backend de llama no usa estas flags. Integrarlo exigiría pasarlas al arrancar `llama-server` y exponer save/restore en el ciclo de vida de la conversación.
- Limitaciones de la cápsula: **atada al modelo, a la config de contexto y a la build de llama.cpp**; no es portable entre modelos ni sirve si cambia `n_ctx`/cuantización. Además el servidor debe seguir vivo (la cápsula es un fichero de estado de slot, no un formato de memoria de Tylluan).

## Impact

- Viabilidad confirmada con cifras: cápsula KV de ~28 MB (f16) / ~15 MB (q8_0) para 255 tokens, guardar/restaurar en 22-59 ms, y reanudación ~11-12× más rápida en prompt (538→49 ms). q8_0 ahorra 47 % de fichero y ~52 MB de RAM con salida idéntica en la prueba.
- Sin cambios de código ni de producción; es un spike de medición. Si se decide integrar, el siguiente paso sería un A/B de calidad con q8_0 y el wiring de las flags en el arranque de `llama-server` (fuera de esta tarea).

## No verificado

- Una sola corrida por variante, un solo modelo pequeño (0.6B Q4_K_M) y un contexto de 1.024: no se midió con 4K/8K ni con modelos de 7B+; la extrapolación por token es aritmética, no medida.
- No se midió calidad/perplejidad con `q8_0` en tareas reales; solo se comparó la salida de 32 tokens de un prompt (idéntica).
- No se probó save/restore tras reiniciar el servidor (la cápsula se guarda en disco, pero no se verificó restore con otro proceso/build) ni concurrencia de slots (`-np 1`).
- No se midió el coste de escribir la cápsula en disco lento ni el impacto de ficheros de cientos de MB con contexto grande.
- No se probó la integración con Tylluan (no hay wiring hoy; `grep` = 0).
- No se tocó `E:\tylluan` ni el kernel de producción (puerto de producción); llama-server de prueba en 47098, parado al terminar.
