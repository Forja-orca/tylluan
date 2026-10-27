# T-DSH-107 — Cierre de "no verificado" de T-DSH-106: segundo camino auditado + respawn real medido

- **Agente:** dsh
- **Fecha:** 2026-10-27
- **Encargo (T1888):** (a) auditar en `proxy.rs` el segundo camino citado (re-registro/Drop sin kill) con `fichero:línea` y decir si queda fuga; (b) en un kernel `test-run-*` (puerto y DATA_DIR propios, guilds reales, un ciclo de respawn forzado) contar procesos del guild antes/después del fix y pegar la tabla. Sin bajar timeouts.
- **Extra (T1889):** registrar el monitor de CPU como tarea de usuario al iniciar sesión y dejar el comando de baja.

## (a) Auditoría del segundo camino — CONFIRMADO, quedaba fuga

- **No existía `Drop`** para `GuildProcess` ni `McpProxy` (`git grep 'impl Drop for GuildProcess\|impl Drop for McpProxy'` → vacío) ni **`kill_on_drop`** en la base (`git grep kill_on_drop` → vacío).
- Camino real de re-registro en runtime:
  - `POST /api/v1/guilds/register` → **`api_guilds.rs:24`** `state.registry.register(...)`.
  - `RegistryMessage::Register` → **`actor.rs:123-124`** `registry.register(...)` (mensaje enviado también desde `actor.rs:551`).
  - **`guild_process.rs:1314`** `pub fn register(...)` → `self.guilds.insert(name, GuildProcess::new(...))`: **reemplaza** la entrada y dropea el `GuildProcess` antiguo, incluido su `McpProxy`/proceso hijo, **sin matarlo** (no hay Drop). Después, `POST /api/v1/guilds/{name}/start` arranca el nuevo registro → **dos procesos del mismo guild**.
- **Reproducido en test-run** (abajo): con el binario viejo, re-registrar `llama_backend` en marcha + `/start` deja **2 procesos**; el antiguo sigue vivo.
- **Fix inicial (diff sin commitear):** guarda `skip_reregistration_of_running` aplicada a `register`, `register_v2`, `register_http_mcp`, `register_sse_mcp` y `register_external`: si el guild **ya está en marcha**, se ignora el re-registro (se conserva el proceso vivo) en vez de reemplazarlo y filtrar el proceso.

## (a.2) CAMBIO PEDIDO (T1890): descriptor-aware, no ignorar siempre

- **Motivo:** ignorar siempre el re-registro de un guild en marcha podía **ocultar un cambio legítimo de configuración** (módulo/comando/args/env/urls/headers).
- **Implementación final** (`guild_process.rs`, diff sin commitear):
  - `GuildLauncher` pasa a `#[derive(..., PartialEq, Eq)]` para comparar descriptores.
  - `enum ReregistrationAction { Replace, IgnoreIdentical, KillThenReplace }`.
  - Función pura `reregistration_action_for(existing_running, same_descriptor)` (L1077) + método `GuildRegistry::reregistration_action(name, &next_launcher)` (L1349) que compara `existing.launcher == *next`.
  - En los 5 `register_*`: si `IgnoreIdentical` → log y no tocar el proceso; si `KillThenReplace` → **matar el proceso antiguo** (mismo camino que T-DSH-106: `McpProxy::kill_abandoned_child(name)`) y sustituir el descriptor; si `Replace` (no existe o no corre) → insertar como siempre.
  - Tests de las dos ramas (en `mod tests`): `t_dsh_107_running_guild_with_same_descriptor_is_ignored`, `t_dsh_107_running_guild_with_different_descriptor_kills_then_replaces`, `t_dsh_107_stopped_guild_is_replaced_regardless_of_descriptor`. Antes del cambio, el caso "running + descriptor distinto" era **rojo** (se ignoraba siempre).

## (b) Test-run real: respawn forzado y re-registro

Montaje: directorio `t107` con **junctions a guilds y .venv reales** (`E:\tylluan\guilds`, `E:\tylluan\.venv`), BD subset copiada, config mínima, **puerto 47022**, DATA_DIR propio, `[guilds.core] always_on=["llama_backend"]` (guild real `llama_backend v1.30.0`, handshake OK), prioridad BelowNormal, afinidad 8 cores. Nunca se tocó 47004.

### Tabla A — re-registro en marcha (segundo camino)

| Binario | Antes (hijos del kernel con `guilds.core.llama_backend`) | Tras `POST /guilds/register` | Tras `POST /guilds/llama_backend/start` | ¿Fuga? |
|---|---|---|---|---|
| **release viejo** (pre-fix) | 1 (`49404`) | 1 (`49404`) | **2 (`49404`, `49156`)** | **SÍ** |
| **debug con fix** (guarda re-registro) | 1 (`25064`) | 1 (`25064`) | **1 (`25064`)** | **NO** |

- El `register` por sí solo no mata el proceso (el `insert` dropea el proxy pero el hijo sigue vivo); el segundo proceso aparece al `/start` del nuevo registro → exactamente la fuga observada (pares acumulándose).

### Tabla B — respawn por transporte muerto (matar el hijo y llamar)

| Binario | Antes | Tras matar el hijo | Tras `query_model` (respawn) | ¿Fuga? |
|---|---|---|---|---|
| release viejo (pre-fix) | 1 (`19820`) | 0 | 1 (`48612`) | no en este camino |
| debug con fix | 1 (`51488`) | 0 | 1 (`11768`) | no |

- La ruta de **transport failure → kill + respawn** de `actor.rs` ya limpiaba bien (el `kill_guild_if_idle` mata el proxy muerto antes del nuevo spawn); **no era el camino de la fuga**. El camino confirmado es el **re-registro** (y, por código, cualquier `Drop` sin kill; ver No verificado).

## (c) Monitor de CPU persistente (T1889)

- `schtasks /create /sc onlogon ...` → **"Error: Acceso denegado"**: crear una tarea `onlogon` requiere elevación (no admin en esta sesión).
- **Alternativa sin admin ni instalación (hecha):** entrada de inicio de usuario
  `C:\Users\FoRJa\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup\dsh_prod_cpu_monitor.cmd`
  que ejecuta `powershell -NoProfile -ExecutionPolicy Bypass -File E:\tylluan-patches\monitor_prod_cpu.ps1`. Al iniciar sesión, el monitor arranca solo (misma semántica que `onlogon`, sin privilegios).
- **Comando de baja:**
  `Remove-Item 'C:\Users\FoRJa\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup\dsh_prod_cpu_monitor.cmd'`
  (y para parar la instancia actual: `Stop-Process -Id <pid del monitor>`; el script no toca nada más).

## Rebase y verificación (tras push171 `ccbf3d5`)

- **Rebase hecho:** worktree reseteado a `origin/main = ccbf3d5` (v0.17.2); se descartaron los diffs locales viejos:
  - `proxy.rs` de T-DSH-106 **ya está en main** (contiene `is_guild_orphan` + sus tests; el `kill_on_drop(true)` local que probé en (a) se descarta por redundante/ineficaz);
  - T-DSH-96 (`SEMANTIC_SCOPE`) **ya está en main**;
  - el hook de umbral de T-DSH-99 **no está** (descartado, correcto).
- **Único diff local actual:** `crates/tylluan-kernel/src/registry/guild_process.rs` (+155/−20, solo T-DSH-107), sin tocar timeouts.

```
cargo test -p tylluan-kernel --lib t_dsh_107          → 3 passed; 0 failed (1305 filtered)
cargo test -p tylluan-kernel --lib                    → 1304 passed; 0 failed; 4 ignored (48,59 s)
cargo clippy -p tylluan-kernel --all-targets -- -D warnings → exit 0
git diff --stat → guild_process.rs +155/−20 (único fichero)
```

- Las tablas A/B de arriba se midieron **antes** del rebase con el binario release viejo vs un debug con la guarda inicial (ignorar siempre). Tras el CAMBIO PEDIDO, la rama "descriptor distinto" es nueva y está cubierta por **tests unitarios** (no se re-ejecutó el test-run end-to-end con el binario rebasado).

## Impact

- **Cierra el "no verificado" principal de T-DSH-106 con reproducción real**: el segundo camino (re-registro de un guild en marcha) **sí filtraba** — binario viejo: 1→2 procesos; con la guarda: 1→1. No es teoría: se forzó con `POST /guilds/register` + `/start` sobre `llama_backend` real en un test-run con puerto/DATA_DIR propios.
- **CAMBIO PEDIDO T1890 aplicado y verificado** sobre `origin/main ccbf3d5`: ya no se ignora siempre — descriptor idéntico se ignora, descriptor distinto **mata el proceso antiguo y sustituye** (evita ocultar cambios legítimos), con 3 tests unitarios de las ramas en verde; lib 1304 passed / 0 failed y clippy `-D warnings` exit 0.
- El respawn por transporte muerto (matar el hijo) **no** era el camino de la fuga (ambos binarios quedan en 1): acota el diagnóstico al re-registro/`Drop` sin kill, coherente con los pares que crecían cada 10–100 min (cada re-registro/registro lazy).
- Deja el monitor de CPU persistente al iniciar sesión **sin admin ni instalar nada** vía carpeta Startup (schtasks `onlogon` denegado por permisos), con comando de baja.

## No verificado

- **La rama `KillThenReplace` (descriptor distinto) está cubierta por test unitario, no por test-run end-to-end**: no se ha vuelto a ejecutar el kernel real con `/guilds/register` cambiando el módulo/comando tras el CAMBIO PEDIDO; la tabla A mide solo el caso "mismo descriptor" (ignorar) con el binario previo al cambio.
- **No se reprodujo el camino de handshake timeout** (guild que arranca pero no responde el handshake): haría falta un guild de prueba que acepte el proceso y no hable MCP; el fix de T-DSH-106 (reaping de huérfanos >10 MB) está pensado también para esa vía, pero no medida.
- **No se auditó todo el ciclo de vida**: quedan posibles `Drop` sin kill (p.ej. `guilds.clear()`, shutdown abrupto del kernel); `kill_on_drop(true)` se probó ineficaz en el test de re-registro (no bastó) y se descartó del diff; la guarda por descriptor cubre el re-registro.
- Solo se probó **`llama_backend`** (guild real de inferencia); otros guilds stdio no se ejercitaron.
- La tabla usa conteo de procesos hijos por `CommandLine` (no historial de PIDs del kernel, que no los registra); el estado "antes" es una muestra puntual.
- La entrada de Startup **no se ha probado con un cierre/inicio de sesión real** (solo se verificó que el `.cmd` y el script existen y el script funciona lanzado a mano).
- `schtasks onlogon` quedó **denegado** por permisos: si José/TL lo prefieren como tarea real, hace falta crearla elevada.
- No se ha validado en perfil de producción (guilds + external_mcp) durante horas; no se tocaron timeouts.
