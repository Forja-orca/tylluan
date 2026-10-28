# T-DSH-114 — Interruptor del centinela en caliente (MCP + REST), CLI, dashboard, gate de CPU y doctor

- **Agente:** dsh
- **Fecha:** 2026-10-28
- **Base:** `origin/main = a564880` (push176: T-DSH-113 + informe T-DSH-112). Diff sin commitear.
- **Encargo (T1926):** (1) interruptor del centinela expuesto **siempre** por **MCP y REST**: leer estado y cambiar `off|observe|enforce` **en caliente** (sin reiniciar; `sentinel::global()` era `OnceLock`), con la misma auth que las rutas de config; (2) **CLI** `tylluan sentinel status|mode <...>`; (3) **dashboard**: tarjeta con `/health.sentinel` y selector de modo; (4) **salto por CPU**: `[security.sentinel] max_cpu_pct` (default 85) ⇒ `Unavailable` sin lanzar llama-server, contador en `/health`; (5) **doctor/install**: comprobar modelo y llama-server por ruta explícita, **nunca descargar**. Criterios: clippy `-D warnings` 0; lib sin fallos (cifra); `contracts_test` verde; `check_async_guild_io` 0; tests por cada punto; informe sin `:NNNN` (`<puerto>`).

## (1) Interruptor en caliente — MCP + REST

- **Kernel (`security/sentinel.rs`):** `mode` pasa de campo `Copy` a `AtomicU8`; `SentinelMode::as_str/judges`; `SentinelJudge::set_mode(&self, mode)` (cambio sin reinicio) y `sentinel::set_mode(mode)` global que **arranca el reaper de inactividad si se pasa a `observe`/`enforce`** (idempotente con `reaper_started: AtomicBool`). `mode()` lee el atómico; el gate de CPU y la política consultan el modo vivo.
- **REST:** `GET /api/v1/security/sentinel` (estado, mismo JSON que `/health.sentinel`) y `POST /api/v1/security/sentinel/mode` `{"mode":"off|observe|enforce"}` → `200 {ok, previous, mode, state}`; modo inválido → **400**. Registradas en `routes.rs`; `auth.rs::operator_class_required` marca `/api/v1/security/sentinel*` como **clase operador** (mismo gate que `/api/v1/config*`, no abiertas sin token fuera de `dev_mode`). Test de auth añadido.
- **MCP (siempre):** sin herramienta nueva (CONTRACT-01 sigue con 6 sovereign tools). Intents deterministas en `tylluan_do` (`handler_do/sentinel_cmd.rs`, antes del routing a guilds):
  - `sentinel status` / `sentinel estado` / `sentinel` / `centinela` / `estado centinela` / `estado del centinela` → estado.
  - `sentinel mode <x>` / `sentinel <x>` / `modo centinela <x>` / `centinela <x>` → cambio en caliente.
  - Cualquier otro intent (p. ej. `health`) **cae al routing normal** (test de regresión: el prefijo no debe capturar intents ajenos).
- **Tests:** 5 en `sentinel_cmd` (status, mode, inválido cae, ajenos caen, parse case/espacios) + 3 en sentinel (`set_mode_changes_mode_in_hot`, gate CPU, etc.) + auth.

## (2) CLI

- `tylluan sentinel [--port <p>] status` → imprime el JSON de estado.
- `tylluan sentinel [--port <p>] mode <off|observe|enforce>` → POST al kernel, imprime `mode`/`previous` y el estado; modo inválido → error de CLI.
- Token: reutiliza `resolve_cli_token` (flag `--token` > `TYLLUAN_TOKEN` > `.tylluan-token`). `--port` añadido para poder apuntar a un `test-run-*` (por defecto 47004).
- Test unitario `sentinel_mode_validation_matches_kernel`.

## (3) Dashboard

- `OverviewTab.tsx`: nueva tarjeta **“Centinela (write_gate L2)”** que lee `/health` (bloque `sentinel`), muestra `mode`, `loaded`, `verdicts`, `fallbacks`, `cpu_skips` y umbral, y tiene selector **OFF / OBSERVE / ENFORCE** que hace `POST /api/v1/security/sentinel/mode` vía `bridge.fetchRaw` (misma auth que el resto del dashboard).
- Verificado: `pnpm build` (`tsc && vite build`) **OK** y `pnpm lint` **0 errores** (36 warnings preexistentes). Nota: el worktree no tenía `node_modules` y T1872 prohíbe instalar; se usó un junction al `node_modules` del checkout de producción (solo lectura) para poder compilar. En CI el dashboard se construye con su propio `pnpm install`.

## (4) Salto por carga de CPU

- `[security.sentinel] max_cpu_pct` (default **85**, `u8`) documentado en `tylluan.example.toml`.
- En `judge()`, antes de `ensure_server`: CPU global (sysinfo, cacheada 2 s) > `max_cpu_pct` ⇒ **`Unavailable` fail-open sin lanzar `llama-server`**, incrementa `cpu_skips` (nuevo contador).
- `/health.sentinel` y `GET /api/v1/security/sentinel` exponen `cpu_skips` y `max_cpu_pct`; `schemas/health_response.json` actualizado. Tests: `cpu_gate_skips_above_threshold_only` (puro) y `cpu_gate_returns_unavailable_without_calling_server` (servidor falso; si la CPU del host > 0 con umbral 0, no llama al modelo y sube el contador).
- **No se bajan timeouts** (siguen 60 s por defecto).

## (5) doctor

- Nueva comprobación `[8/8] Sentinel`: lee `[security.sentinel]` del `tylluan.toml` (TOML parseado) y, si `mode != off`, verifica que `model_path` y `llama_server_path` **existen como ficheros**; si faltan, marca ❌ y avisa de que **el centinela nunca descarga**. Si `mode = off`, informa de que no requiere modelo. No instala ni descarga nada.

## Verificación

```
cargo clippy -p tylluan-kernel --all-targets -- -D warnings → exit 0
cargo clippy -p tylluan-cli --all-targets -- -D warnings    → exit 0
cargo test -p tylluan-kernel --lib                          → 1338 passed; 0 failed; 4 ignored (40,55 s)
cargo test -p tylluan-kernel --test contracts_test          → 5 passed; 0 failed
python scripts/check_async_guild_io.py                      → exit 0 (73 ficheros, limpio)
dashboard: pnpm build (tsc+vite) OK; pnpm lint 0 errores
git diff --stat → 9 ficheros +390/−22 (+ handler_do/sentinel_cmd.rs nuevo)
```

Lib: 1338 = 1330 previos + 8 nuevos (5 sentinel_cmd + 3 sentinel). CLI: 33 tests OK.

### Verificación en vivo (kernel test-run, puerto/DATA_DIR propios, BM25-only, solo PID exacto)

```
GET  /api/v1/security/sentinel          → 200 {"mode":"off","loaded":false,...}
POST /api/v1/security/sentinel/mode     → 200 {"previous":"off","mode":"observe"}
GET  /health                            → sentinel.mode = "observe"
POST /api/v1/do {"intent":"sentinel status"} → 200, is_error=false, contenido = estado JSON (mode observe)
POST mode enforce                       → 200 {"previous":"observe","mode":"enforce"}
POST mode basura                        → 400 "mode inválido: usa off|observe|enforce"
tylluan sentinel --port <p> status      → exit 0, imprime estado (mode enforce)
tylluan sentinel --port <p> mode observe→ exit 0, "sentinel mode: observe (antes enforce)"
```

## Impact

- El centinela deja de ser “config + reinicio”: se puede **leer y cambiar en caliente** por MCP (`tylluan_do`) y REST con la **misma auth que config**, y por CLI; el reaper se arranca solo al activarlo.
- `observe`/`enforce`/`off` se pueden alternar en producción sin reiniciar (decisión de José/TL sobre activar el juez), y el dashboard muestra el estado y permite cambiarlo.
- El **gate de CPU** evita que el centinela lance un `llama-server` con la máquina saturada (fail-open + contador visible), sin tocar timeouts.
- `doctor` detecta rutas de modelo/llama-server mal configuradas sin descargar nada (coherente con T1872).

## No verificado

- **Persistencia del modo:** el cambio en caliente es **en memoria**; no se escribe en `tylluan.toml`. Tras reiniciar vuelve el `mode` de config. Si el TL quiere persistencia, es un añadido pequeño (no pedido explícitamente).
- **Gate de CPU en vivo** no ejercitado (solo función pura + test condicionado a CPU>0); no se midió con carga real >85 % ni el contador en un kernel bajo carga.
- **MCP real (cliente MCP)**: la ruta de intent se probó vía `POST /api/v1/do` (mismo handler que `tylluan_do`), no con un cliente MCP conectado; el transporte MCP ya está autenticado por diseño.
- **Dashboard**: verificado con `tsc`+`vite build` y `eslint`, no renderizado en navegador ni con tests de UI nuevos (los existentes mockean `/health?verbose=true`; la tarjeta usa `/health` con guardas). El `node_modules` se tomó del checkout de producción (junction) por T1872; CI hará su propia instalación.
- **Auth en vivo**: no se probó con token real (dev_mode del test-run); el gate está cubierto por test unitario de `operator_class_required` y el mismo middleware que config.
- **Reaper arrancado al pasar a on en caliente**: `set_mode` lo arranca, pero no se midió la descarga por inactividad en este ciclo (T-DSH-110 sí lo midió con reaper ya activo).
- **CLI**: `--port` se añadió para probar contra `test-run-*` (por defecto 47004); no se probó contra producción (no se toca).
- **No se probó `enforce` con el modelo real en este ciclo** (T-DSH-110/112 ya lo cubren); aquí el kernel de prueba iba BM25-only y sin cargar el GGUF.
- **doctor**: no probado con un `tylluan.toml` real de producción (solo con el del test-run); la comprobación es de rutas, no valida que el GGUF sea Qwen3Guard.
