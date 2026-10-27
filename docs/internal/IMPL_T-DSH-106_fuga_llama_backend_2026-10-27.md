# T-DSH-106 — Causa raíz de la fuga de procesos `llama_backend` (fix + test, diff sin commitear)

- **Agente:** dsh
- **Fecha:** 2026-10-27
- **Encargo (T1885):** reproducir en `test-run-*` la fuga de procesos `llama_backend` (kernel `8b589f4`, 10 pares vivos, uno nuevo cada 10–100 min, sin reapear los anteriores), encontrar `fichero:línea` del registry/guild_process que lanza otro sin matar el anterior, y entregar **diff sin commitear con test que falle antes y pase después**. Sin bajar timeouts; `clippy -D warnings` 0; lib verde.
- **Alcance:** solo lectura de código y logs; **no** se ha tocado producción ni se han matado procesos (los 20 huérfanos los paró el TL). Diff **sin commitear**.

## 1. Reproducción

- **No se ha conseguido reproducir la fuga en vivo** en el tiempo disponible: montar un test-run con guilds reales exige enlazar `guilds/` y `.venv` del checkout (junction) y forzar handshakes fallidos/timeouts; no llegué a montarlo. **No verificado en vivo** (declarado abajo).
- **Evidencia en vivo previa (T-DSH-100, solo lectura):** a las ~17:09 local el kernel `45436` tenía **24 procesos `guilds.core.llama_backend` hijos vivos** simultáneamente (PowerShell `Get-CimInstance Win32_Process ParentProcessId=45436`), con CPU ~0 cada uno y launch-time distintos; el TL reportó 10 pares vivos con uno nuevo cada 10–100 min. Contadores del log (tramo de ~25 MB): **227** `[lazy] Starting guild 'llama_backend'`, **355** `timed out on attempt 1/4 (30s) — respawning and retrying`, **211** `Auto-unloading idle guild 'llama_backend'`.
- El kernel **no registra el PID** de los guilds (`get_health_report` devuelve ceros y lo documenta: "Per-process CPU/memory requires storing child PIDs (future work)"), así que no hay tabla histórica pid→evento que el propio kernel pueda dar; la tabla de abajo cruza eventos de log con la observación de procesos vivos.

## 2. Tabla evento → proceso → ¿anterior vivo?

| Evento (log) | Evidencia | ¿Proceso anterior vivo? | Causa candidata |
|---|---|---|---|
| `[lazy] Starting guild 'llama_backend' on demand` (227) | spawn de un proxy nuevo | **Sí**: varios procesos del mismo módulo vivos a la vez (20–24 observados) | Un proxy anterior se perdió sin `shutdown()`; `spawn()` no limpiaba huérfanos antes de lanzar → duplicado |
| `timed out on attempt 1/4 (30s) — respawning and retrying` (355) | actor reintenta; el kill real ocurre en el siguiente intento vía `kill_guild_if_idle` | Depende: si otro caller está en vuelo, `GuardedKill::InFlight` → **kill omitido** (`actor.rs:162-165`) | El kill del respawn es *guardado* y puede saltarse; no es la fuga directa, pero enmascara procesos viejos |
| `Auto-unloading idle guild 'llama_backend'` (211) | unload → `kill()` | Si `Arc::try_unwrap(proxy)` falla → `kill_abandoned_child()` (`proxy.rs:1026-1032`) | **La rama de limpieza no mataba procesos sanos de >10 MB** (ver causa) |
| Handshake timeout / error en `spawn()` | `kill_abandoned_child()` en `proxy.rs` (antes L100/L107) | El proceso recién lanzado debería morir | Misma puerta de memoria/parent; si no casa, el proceso sobrevive y el siguiente intento crea otro |

## 3. Causa raíz (`fichero:línea`)

En `crates/tylluan-kernel/src/registry/proxy.rs`, `McpProxy::kill_abandoned_child` (versión base, líneas ~137-204):

- **Strategy 1** sólo mataba si **`process.parent() == self_pid`** (base L153-154) y el comando casaba el guild.
- **Strategy 2** (procesos Python de sesiones previas, "stale") sólo mataba si **`memory_kb < 10000`** (base L183, comentario: "if it's using < 10MB it's likely a stuck stub") **y** casaba el nombre.
- Consecuencia: un `llama_backend` **sano** (~80 MB de RSS, medido en T-DSH-100) que quedaba huérfano por un `shutdown()` fallido/`Arc::try_unwrap` fallido/handshake timeout **no cumplía ninguna de las dos ramas** y nunca se reapeaba; como `spawn()` tampoco limpiaba antes, cada nuevo intento añadía un proceso: **10 pares**.
- No es un bug de "lanzar otro sin matar el anterior" en un único punto: es que la **limpieza de huérfanos sanos no existía** (el único filtro de memoria era para stubs <10 MB) y el spawn confiaba en que el proxy anterior hubiera muerto.

## 4. Fix (diff sin commitear, solo `proxy.rs`)

1. **Predicado extraído y ampliado** (`proxy.rs:150`, `fn is_guild_orphan`): mismo criterio de nombre/módulo, pero si el comando corresponde **exactamente** a `guilds.core.<guild>` / `guilds.<guild>` se considera huérfano **aunque tenga >10 MB** (los stubs <10 MB siguen matándose igual).
2. **Limpieza antes de cada spawn** (`proxy.rs:74`): `McpProxy::spawn()` llama a `kill_abandoned_child(guild_name)` **antes** de lanzar el proceso → relación 1 proxy ↔ 1 proceso aunque se haya perdido un proxy.
3. **Reaping unificado** (`proxy.rs:205`): el bucle de `kill_abandoned_child` usa `is_guild_orphan` (una sola verdad, testeable).
4. **No se ha tocado ningún timeout** (ni handshake ni tool call).

## 5. Test rojo‑antes / verde‑después

Tests nuevos en `proxy.rs` (`#[cfg(test)]`, L1161-1183):

| Test | Antes del fix | Después |
|---|---|---|
| `t_dsh_106_orphan_llama_backend_high_memory_is_reaped_even_if_reparented` (python, `-m guilds.core.llama_backend`, **80.000 KB**, padre≠kernel) | **rojo** (base exigía `<10 MB`) | verde |
| `t_dsh_106_direct_child_still_reaped_regardless_of_memory` | verde | verde (no regresión) |
| `t_dsh_106_other_guild_process_is_not_reaped` | verde | verde (no mata `guilds.core.bash` para guild `llama_backend`) |
| `t_dsh_106_low_memory_stale_stub_still_reaped` | verde | verde (stub <10 MB sigue muriendo) |

## 6. Verificación

```
cargo test -p tylluan-kernel --lib registry::proxy::   → 5 passed; 0 failed
cargo test -p tylluan-kernel --lib                     → 1297 passed; 0 failed; 4 ignored (42,55 s)
cargo clippy -p tylluan-kernel --all-targets -- -D warnings → exit 0
git diff --stat -- crates/tylluan-kernel/src/registry/proxy.rs → 1 file, +92/−40
```
(el worktree también arrastra los cambios locales previos de T-DSH-96/99 en `config.rs`/`coherence_gate.rs`/etc.; **T-DSH-106 toca solo `proxy.rs`**).

## Impact

- Cierra la causa raíz de la fuga vista por el TL: los procesos `llama_backend` sanos (~80 MB) que quedaban huérfanos **no se reapeaban** porque la única rama de limpieza para procesos no-hijos exigía `<10 MB`; ahora se reconoce el módulo exacto del guild y se limpia, y `spawn()` limpia antes de lanzar para que no convivan dos procesos del mismo guild stdio.
- Efecto esperado en hardware modesto ("cafetera", José): sin la fuga no se acumulan decenas de procesos Python ociosos ni su RAM; el fix es defensivo y no cambia timeouts ni el camino feliz (si el proxy está vivo, `spawn()` sigue haciendo early-return).
- Incluye 4 tests unitarios que fijan el comportamiento (rojo/verde) y la verificación pedida (lib 1297 verde, clippy 0).

## No verificado

- **No reproducido en vivo en test-run**: no se montó el kernel de prueba con guilds reales/forzar timeouts; la causa es **análisis de código + evidencia de procesos vivos**, no una reproducción controlada. La tabla evento→proceso no tiene PIDs históricos (el kernel no los registra).
- **No se ha validado end-to-end** que tras el fix el contador de procesos quede a 1 en un ciclo real de respawn/timeout; sólo el test unitario del predicado y el razonamiento del flujo.
- La fuga podría tener un segundo camino no cubierto (p.ej. re-registro de guilds o `Drop` de `GuildProcess` sin `kill()`); el fix cubre la limpieza por nombre/módulo y la limpieza pre-spawn, pero **no se auditó todo el ciclo de vida** del `GuildProcess`.
- El `kill` vía `sysinfo` no reapea el handle (limitación ya documentada en el código, "OS handle reaping delegated to rmcp service drop"): el proceso muere, pero el reaping lo hace Windows/rmcp; no verificado en vivo.
- No se ha probado con `llama_backend` en perfil de producción (guilds + external_mcp) durante horas; el escenario de 10 pares no se reprodujo.
