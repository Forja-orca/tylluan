# B-6 — Entrega del kernel.log del juez (llama_backend) a Deep

**Fecha:** 2026-10-21 · **Agente:** Buffy · **Worktree:** `E:\tylluan-wt-buffy` (HEAD `8001d23`, diff sin commitear, regla 9) · **Código del kernel: NO MODIFICADO.**

## Qué se entregó

`E:\tylluan-patches\deep\kernel_log_juez_fase3.txt` (724 líneas, 107 KB) con el texto literal que Deep necesita para `INVEST_D-10` (36 % de fallos del juez):

- **Texto literal tras `GUILD_ERROR|llama_backend|`** (único detalle que llega al llamador): `Tool call 'query_model' failed in guild 'llama_backend'` — no propaga ni el timeout del proxy ni el stderr del guild.
- **Las 652 líneas `llama_backend`** del log del drill (`test-run-gate/logs/kernel.log`, kernel `cad1477`), incluidas las **30 líneas `GUILD_ERROR`** de la ventana `2026-10-20T03:23:36 → 04:11:28 UTC` (25 de la fase 3 del B-4 + probes del A/B de `max_tokens`), con su contexto: dispatch, retry loop, stderr del guild, timeout y error del proxy, spawn/kill.
- **Tabla de elapsed real por fallo** (dispatch attempt → proxy `timed out after 60s`), 18 filas emparejables.
- **Histograma** de tipos de línea.

Saneado verificado antes de entregar (asserts en el propio script):

- **Sin tokens**: el valor de `.tylluan-token` comprobado contra el contenido — 0 ocurrencias.
- **Sin rutas absolutas del perfil**: toda `C:\Users\<quien sea>` → `C:\Users\<USER>` (9 ocurrencias de la caché de HuggingFace); assert de que no queda ninguna sin sanear y de que la cadena `FoRJa` no aparece.
- Códigos ANSI quitados para que los timestamps parseen.

## Dos pistas ya visibles en el log (para que Deep las contraste)

1. Stderr del guild en cada llamada: `[llama_backend] OLLAMA_HOST=0.0.0.0:11434 set but not reachable at http://127.0.0.1:11434/v1, ignoring` (82 veces).
2. **El mensaje de timeout no cuadra con el reloj**: el proxy loguea `timed out after 60s`, pero el elapsed real dispatch→timeout en varias filas es 13-20 s (la tabla del fichero lo cuantifica por fallo). El timer o el mensaje mienten; lo dejé sin concluir para D-10.

Correlación muestra↔fallo anotada en el fichero: `📥 [MCP] Received payload` en el log original ~2 s antes de cada `Guild tool call dispatch`.

## Aviso en Coloquio

`[COLA buffy] B-6 terminado` publicado en `general` con `@deep`, **HTTP 201** verificado (respuesta del kernel con `coloquio_reminder`). Nota de método: la ruta del endpoint es `/api/v1/coloquio/channels/{id}/post` (no `/messages`, que es GET); el POST se hizo con Python `urllib` porque el `curl` nativo no leía el fichero JSON vía `@ruta` de bash.

## No verificado / limitaciones

- No verificado: si las 30 líneas `GUILD_ERROR` del fichero corresponden 1:1 a las 25 del juez de la fase 3 (hay 5 de probes del A/B mezcladas; la atribución exacta requiere cruzar `phase3_results.jsonl`, cosa que Deep puede hacer con los timestamps que ya tiene).
- No verificado: la causa del desfase 60 s vs 13-20 s (solo constatado el desfase; la causa es para D-10).
- La ruta citada en la cola (`test-run-gate/work/kernel.log`) no existe: el log real es `test-run-gate/logs/kernel.log`; se usó esa (no hizo falta reproducir el drill).

## Impact (ADR-013)

Sin cambios en `src/`, sin tocar el kernel de producción ni sus datos. Artefactos añadidos: este informe en `docs/internal/` y el fichero entregado en `E:\tylluan-patches\deep\` (canal de intercambio de patches, fuera del repo). Un `curl` fallido contra Coloquio no produjo mensaje (la primera attempt devolvió 400 de parseo); el mensaje válido se publicó una sola vez.

## Checks ejecutados

- Verificaciones del saneado (token, rutas de usuario, `FoRJa`): asserts en el script de generación — todos en verde antes de escribir el fichero.
- `scripts/check_docs_reality.sh` / `check_no_predation.sh`: se ejecutan al cierre de la cola 2 sobre el estado final del worktree (este informe es documentación en `docs/internal/`, zona permitida).
- No se ejecutó `cargo test/clippy`: el ítem no tocó código Rust.
