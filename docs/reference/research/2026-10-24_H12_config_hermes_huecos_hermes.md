# H-12 — Cerrar los «no verificados» del informe del TL sobre mi config (hermes)

Asignación: TL claude-code, `docs/internal/COLA_hermes_2026-10-24.md`. Fuente primaria: fuente de Hermes en `%LOCALAPPDATA%\hermes\hermes-agent` (repo en **v0.22.4**, `git describe`) + issue #48820 vía `gh api` (texto guardado en scratch) + `git log`/`merge-base` del propio repo. Cita literal Ctrl+F-able · [I]=inferencia.

| # | Pregunta | Cita literal | Fuente | Estado |
|---|---|---|---|---|
| 1a | Cabeceras MCP HTTP | HTTP headers, e.g. `Authorization` | website/docs/getting-started/nix-setup.md:1016 (`mcpServers.<name>.headers`) | CITADA, grep -F OK |
| 1b | headers soportado en config Python | remote server whose ``url`` / ``headers`` still carry a literal | tools/mcp_tool_config.py:585 (`_require_rendered_remote`) | CITADA, grep -F OK |
| 1c | ¿Expansión de `${VAR}`? | in url/headers is not set in this profile's .env or secret source | website/docs/developer-guide/multiplexing-gateway.md:298-299 | CITADA, grep -F OK |
| 1d | ¿Clave `auth:`? | — | grep `"auth"` en tools/mcp_tool_config.py + mcp_tool.py: 0 resultados; OAuth vive en módulos aparte (`mcp_oauth.py`, `mcp_oauth_manager.py`) | VERIFICADA (ausencia por grep) |
| 2a | Qué escribe `windows-sandbox-fallback.json` | export const WINDOWS_SANDBOX_MARKER_FILENAME = 'windows-sandbox-fallback.json' | apps/desktop/electron/windows-sandbox-fallback.ts:42 | CITADA, grep -F OK |
| 2b | Quién lo invoca | writeSandboxMarker | apps/desktop/electron/main.ts:706 (import de './windows-sandbox-fallback') | CITADA, grep -F OK |
| 2c | Estados | export type SandboxMarkerState = 'booting' | 'fallback' | 'ok' | windows-sandbox-fallback.ts:62 — reasons (ts:64): 'gpu-breakpoint' | 'renderer-crash-loop' | 'boot-loop' | CITADA, grep -F OK |
| 2d | Cuándo entra fallback | BOOT_ABORTS_BEFORE_FALLBACK = 2 | windows-sandbox-fallback.ts:60 — mi marcador observado `{"state":"fallback","reason":"boot-loop"}` = 2 abortos de arrancho consecutivos | CITADA, grep -F OK |
| 2e | Qué cambia en Desktop | Must run before app `ready` so `--no-sandbox` applies to child | apps/desktop/electron/main.ts:1036 — arranca Electron con sandbox Chromium desactivado | CITADA, grep -F OK |
| 2f | Cuánto dura | The fallback is sticky per app version, not forever: after an update | windows-sandbox-fallback.ts:25 — el marcador es por versión: cada update re-sondea el sandbox una vez; si aborta, vuelve a fallback | CITADA, grep -F OK |
| 3a | Issue #48820 | [Windows] Messaging gateway has 3 fragility bugs: no watchdog, env override ignores yaml, aiohttp trust_env misroutes under Scheduled Task | gh api issues/48820 (estado: closed) | CITADA, grep -F OK |
| 3b | Mecanismo Job Object | updater's job teardown on exit would kill the freshly respawned gateway | issue #48820 (comentario reproducción 2026-08-28, guardado en scratch) | CITADA, grep -F OK |
| 3c | Arreglo | fix(cli): symmetric Windows resume-failure handling + atexit disarm | commit 18f4b29daf (2026-09-20) + e9541d213f (start attestation) + 9e041a7428 (test job-teardown) | CITADA, git log OK |
| 3d | ¿Está en v0.22.4? | — | `git merge-base --is-ancestor`: 18f4b29daf ✓ · 9e041a7428 ✓ · e9541d213f ✓ en v0.22.4; 0ad5e937e2 (duplicado en rama) ✗ | VERIFICADA (git) |
| 3e | Comando de recuperación | launch the relaunch step via `hermes gateway start` | issue #48820 (fix aplicado localmente: `_spawn_detached()` en `gateway_windows.py`, `HERMES_GATEWAY_DETACHED=1`) | CITADA, grep -F OK |
| 4a | Raíz del checkpoint | agent._checkpoint_mgr.get_working_dir_for_path(str(resolved_path)) | agent/tool_executor.py:114 — por terminal usa cwd (tool_executor.py:1053) | CITADA, grep -F OK |
| 4b | Cómo elige la raíz | markers = {".git", "pyproject.toml", "package.json", "Cargo.toml", "go.mod", "Makefile", "pom.xml", ".hg", "Gemfile"} | tools/checkpoint_manager.py:1402-1403 (subida desde el fichero editado; gana el dueño registrado más cercano, líneas 1398-1400) | CITADA, grep -F OK |
| 4c | Sin marker en la cadena | return str(candidate) | checkpoint_manager.py:1411 — sin marker: la raíz es el directorio del propio fichero [I] en `D:\hermes-projects` (sin .git) eso hace raíz un directorio amplio | CITADA, grep -F OK |
| 4d | Por qué agotó 60 s | _GIT_TIMEOUT: int = max(10, min(60, env_int("HERMES_CHECKPOINT_TIMEOUT", 30))) | checkpoint_manager.py:161 — tope duro 60 s por git; `git add -A` entra con timeout=_GIT_TIMEOUT * 2 (línea 878, tope 120 s) | CITADA, grep -F OK |
| 4e | Omisión >50k ficheros | Max files to snapshot — skip huge directories to avoid slowdowns. _MAX_FILES = 50_000 | checkpoint_manager.py:163-164 — si se supera: "Checkpoint skipped: >%d files in %s" (línea 1431) y `return False` | CITADA, grep -F OK |

## Veredicto [I]
- 1: `headers:` sí (con `${VAR}` renderizado por perfil; sin valor → fail-closed que nombra la variable); clave `auth:` NO existe — la autenticación MCP es vía headers o OAuth por módulos dedicados.
- 2: mi marcador `fallback/boot-loop` no es un flag permanente: Electron relanza sin sandbox Chromium hasta el siguiente cambio de versión de la app.
- 3: el arreglo del Job Object SÍ está en v0.22.4; la recuperación tras update es `hermes gateway start` (desprendido del job del actualizador).
- 4: el sumidero de 60 s con `cwd = D:\hermes-projects`: [I] o el barrido `rglob` de `_dir_file_count` (sin timeout propio, py:734-744) o el `git add -A` del árbol grande; la guarda de 50 k decide saltarse el checkpoint, no el coste del barrido.

## Gate de citas
grep -F (whitespace normalizado `tr -s '[:space:]'`) de las 19 citas sobre ficheros locales del repo + issue48820.md guardado en scratch: ver salida en la corrida. Repo fuente: v0.22.4 @ 4d04303818.

## Nota Tylluan
- Regla 1: único cambio = este doc, SIN commitear, en tylluan-wt-hermes. `[H-12 COMPLETA]` al Coloquio cuando el kernel vuelva.

## Verificacion del TL (2026-10-24)
Re-ejecutado sobre el codigo local de Hermes v0.22.4 con grep -F (espacios normalizados): 9 citas clave son literales (`WINDOWS_SANDBOX_MARKER_FILENAME`, `BOOT_ABORTS_BEFORE_FALLBACK = 2`, `SandboxMarkerState`, "sticky per app version", `--no-sandbox` en main.ts, `_MAX_FILES = 50_000`, la lista de markers de checkpoint, `_GIT_TIMEOUT`, y "remote server whose"); `git merge-base --is-ancestor` confirma que los tres commits del arreglo del Job Object (18f4b29daf, 9e041a7428, e9541d213f) estan en v0.22.4; y la clave `"auth"` no aparece en `mcp_tool_config.py` ni `mcp_tool.py`. Matices: (1) la fecha del commit 18f4b29daf es 2026-09-18 segun `git log`, no 09-20 como dice la tabla. (2) El arreglo esta en v0.22.4 y, aun asi, el `hermes update` ejecutado la noche del 23-oct sobre v0.22.4 avisó de "could not be verified ... #48820": que el arreglo exista no garantiza que el relanzamiento se verifique en esa ruta; la recuperacion sigue siendo `hermes gateway restart`/`start`. (3) `windows-sandbox-fallback` se refiere al sandbox de Chromium del Desktop (Electron arranca con `--no-sandbox`), no a un aislamiento del agente: no cambia lo que puede hacer el agente, pero si reduce la proteccion del proceso de interfaz hasta la siguiente version de la app. Las filas [I] son inferencias de Hermes.
