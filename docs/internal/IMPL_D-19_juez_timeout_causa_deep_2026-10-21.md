# D-19 — Fallos del juez: causa real (log de B-6)

- **Fecha:** 2026-10-21
- **Agente:** Deep (OpenCode)
- **Cola:** `docs/internal/COLA_deep_2026-10-21.md`, ítem **D-19**
- **Log analizado:** `E:\tylluan-patches\deep\kernel_log_juez_fase3.txt` (724 líneas, drill B-4 fase 3, commit `cad1477`)
- **Patch:** `E:\tylluan-patches\deep\D-19.patch` (2 archivos, +37/−3)
- **Veredicto:** **C3 refutada**; causa real demostrada y arreglada (el arreglo mínimo cabía en dos líneas de lista).

## Tabla `fallo → causa → evidencia`

| Fallo (30, todos iguales) | Causa | Evidencia (línea del log) |
|---|---|---|
| `Guild 'llama_backend' timed out after 60s for tool 'query_model'` | **Techo de 60 s del proxy** (`analysis_guild_ms`) aplicado a una guild de inferencia en CPU, mientras el actor se saltaba su propio timeout por `tool_timeout=None` | 30 líneas `ERROR tylluan_kernel::registry::proxy: … timed out after 60s` (p. ej. 135, 143, 171, 176, 344, 374, 400, 451, 459, 462, 494, 502, 510, 524, 529, 550, 567, 572, 577, 582, 587, 596, 603, 608, 613, 616, 619, 644, 684, 714) |
| El actor registra "unlimited timeout" para la MISMA llamada | Dos capas de timeout con criterios distintos: el actor salta el suyo si `tool_timeout.is_none()`, el proxy **no** | `INFO … [Retry Loop] … (CPU inference, unlimited timeout)` (116, 121, 129, 134, …) contra el `ERROR … timed out after 60s` de 2 líneas después (135) |
| "timed out after 60s" con reloj de 13-20 s | **Artefacto de medición de B-6**, no causa: varias llamadas `query_model` conviven en vuelo y la tabla del informe anterior emparejó cada timeout con el dispatch *más cercano*, no con el suyo | 82 `Guild tool call dispatch` y 30 timeouts: 03:22:36.333 dispatch → 03:23:36.337 timeout = **60.004 s**; 03:22:44.421 → 03:23:44.437 = **60.016 s**; 03:24:25.064 → 03:25:25.101 = **60.038 s**. Los deltas inflados (73 s, 86 s, 113 s, 120 s) son llamadas que esperaron turno en la cola del servidor |
| `OLLAMA_HOST=0.0.0.0:11434 … not reachable` ×84 | **Ruido informativo**: el guild cae a `llama-server.exe` local y lo dice. No precede ni causa ningún fallo | aparece también en las llamadas que terminan bien (las 52 sin timeout) |
| "arranque de un segundo servidor" (C3) | **Refutado**: nunca hay dos `llama-server`. Un solo `Starting: …llama-server.exe` en todo el log y 8 reactivaciones que **reutilizan** el mismo puerto | 1× `[llama_backend] Starting: … llama-server.exe --port 9000 …` (92); 8× `Port 9000 in use by a compatible server, reusing it` (126, 238, 335, 440, 601, 643, 683, 713) |
| "ventana de arranque de 15 s" (C3) | **Refutado** como causa de estos 30 fallos: 15 s es `system_guild_ms`, y `llama_backend` **nunca** cae en la categoría `system` (su categoría es `analysis`, 60 s) | `guild_process.rs::guild_timeout_category`: SYSTEM = `bash, git, filesystem, monitor`; `llama_backend` no está en SYSTEM ni en HEAVY → Analysis |

**Conteo exacto:** 30 `ERROR … timed out after 60s`, 30 `ERROR Tool call 'query_model' failed`, 31 WARN del actor (30 de ellos "returned error on attempt 1"), 82 dispatches → **30 fallos / 52 llamadas completadas**. Ningún otro tipo de error: ni crash, ni OOM, ni conexión rechazada. Todos los fallos son el mismo string del proxy.

## Causa raíz (confirmada en código)

1. `guild_list.rs:43` registra `llama_backend` con `tool_timeout = None` → el **actor** no aplica timeout (`actor.rs:240-262`, log "CPU inference mode, no timeout").
2. El proxy sí aplica su propio `tokio::time::timeout` (`proxy.rs:244-249`) con `tool_call_timeout = effective_tool_call_secs("llama_backend")` (`guild_process.rs:973-995`).
3. `llama_backend` **no estaba** en `UNLIMITED_PATIENCE_GUILDS` (`guild_process.rs:969`), así que caía en `Analysis` → `analysis_guild_ms` = **60 000 ms** (`config.rs:1957`).
4. `UNLIMITED_PATIENCE_GUILDS` ya documentaba este exacto fallo ("main.rs's `tool_timeout = None` override only skips the OUTER guild-level wrapper … it does nothing about the INNER proxy-level timeout") pero la lista se quedó sin la única guild de inferencia LLM local del proyecto.
5. `main.rs:1051` (`cpu_inference_guilds`) tenía la misma omisión; el comentario de `guild_process.rs` exige mantenerlas en sintonía a mano.

La inferencia en CPU de un 0,5B con `--threads 56 --mlock` supera los 60 s con regularidad (sobre todo al reactivar la guild: 8 de las 9 reactivaciones encuentran el server ya vivo pero con el modelo paginado fuera), así que el techo de 60 s corta llamadas que estaban progresando.

## Arreglo mínimo (implementado, porque la causa quedó demostrada)

- `guild_process.rs`: `"llama_backend"` añadida a `UNLIMITED_PATIENCE_GUILDS` (con el comentario de por qué y qué log lo demuestra).
- `main.rs`: `"llama_backend"` añadida a `cpu_inference_guilds` para que ambas listas vuelvan a coincidir.
- Tests: el test existente `unlimited_patience_guilds_keep_flat_timeout_not_heavy_category` ahora incluye `llama_backend`, y se añade `llama_backend_keeps_flat_timeout_despite_analysis_category` (comprueba además que la ventana de 15 s del sistema nunca aplica).

Efecto: el proxy pasa de 60 s a `tool_call_secs` (3 600 s por defecto), que es la misma paciencia que el actor ya creía tener. No se toca ningún otro guild, ni `dev_mode`, ni puertos, ni tokens.

## Verificación

- `cargo test -p tylluan-kernel --lib registry::guild_process` → **28 passed** (incluye el test nuevo).
- `cargo test -p tylluan-kernel --lib` → **1142 passed / 0 failed / 4 ignored** (29.68 s). Una pasada anterior dio `1141 passed; 1 failed` (el flake conocido de D-20, `embedding_fingerprint_index_tests::fingerprint_gate_cost`, que se aborda en el ítem siguiente).
- `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` → EXIT 0.

## No verificado

- **No he reproducido el drill en vivo** (haría falta `llama-server` real y el corpus de 30 llamadas): la demostración es del log + código, que es lo que pedía la cola.
- **El arreglo no se ha probado contra el servidor real**: predice que los 30 fallos desaparecen porque el corte era exactamente el techo de 60 s; un drill de fase 3 repetido lo confirmaría.
- El mensaje `"CPU inference mode, no timeout"` del actor sigue siendo engañoso cuando el proxy aplica un techo (solo para guilds que no estén en las dos listas). No lo toco aquí para no ampliar la tajada.
- `main.rs` y `guild_process.rs` mantienen la lista duplicada a mano (deuda ya documentada en el código, anterior a este ítem).

## Impact

- **Comportamiento:** `llama_backend` deja de abortar a los 60 s y hereda la paciencia de inferencia en CPU (3 600 s por defecto). Sin cambios para el resto de guilds; `docker` sigue con 180 s y `bash` con 15 s (el test lo verifica).
- **Riesgo:** una guild colgada puede retener su semáforo hasta 1 h. Es el mismo riesgo que ya asumían `vision`/`deep_analysis`/`knowledge`/`comfy_ui`/`n8n_bridge` por contrato, y el que el proyecto declara como invariante ("NUNCA reducir timeouts para guilds de inferencia").
- **Sin dependencias nuevas, sin procesos, sin cambios de contrato HTTP.**

## Siguiente

`D-20` (el test inestable que acaba de fallar una vez en esta misma sesión).
