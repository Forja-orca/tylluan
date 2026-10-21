# T-DSH-26 — Inventario de capacidades REALES de las guilds (insumo del manifiesto)

- **Agente:** dsh
- **Fecha:** 2026-10-21
- **Base:** worktree `E:\tylluan-wt-dsh` en `--detach origin/main` (`f5a505b`); limpio antes de empezar.
- **Encargo (turno 1516):** solo lectura. Inventario de capacidades REALES de las guilds (FS, red, binarios, tiempo, escrituras fuera de su carpeta) como insumo del manifiesto de capacidades, tabla guild × capacidad × evidencia archivo:línea, empezando por bash, code, docker, filesystem, git y browser.

## Método

Lectura estática del código (sin ejecutar ninguna herramienta de guild, sin arrancar procesos): el ejecutor del kernel (`registry/guild_process.rs`), la política de sandbox (`config.rs`), el parser de declaraciones (`router/catalog.rs`) y los 6 `.py`. Para cada capacidad se cita el punto exacto del código. No se modifica nada; este informe es el único artefacto.

## Base del ejecutor (aplica a las 6)

| hecho | evidencia |
|---|---|
| Las guilds arrancan con `python -m <module>`, **cwd = raíz del workspace** (repo), `PYTHONPATH`=workspace, stdin DEVNULL | `guild_process.rs` (línea 441-447) |
| **Heredan todo el entorno del kernel y se les inyecta `TYLLUAN_TOKEN`** (el token con el que el kernel acepta llamadas HTTP; guild-side `kernel_auth.py`) | `guild_process.rs` (línea 478-491) |
| El sandbox Docker solo se aplica si `security.sandbox.enabled = true` **y** el perfil es strict/balanced; `enabled` es **false por defecto** → arranque nativo | `guild_process.rs` (línea 415-418), `config.rs` (línea 1746-1752) |
| Perfil por defecto declarado: **Balanced** (cuando el sandbox está deshabilitado, `load_sandbox_profile()` cae a Balanced) | `config.rs` (línea 1641-1653, 1761-1767) |
| La comprobación de capacidades (`check_capabilities`) solo actúa con perfil no-permissive **y** capacidad declarada; sin declaración → permitido. `CAPABILITIES` se parsea del propio `.py` al catálogo | `guild_process.rs` (línea 548-559), `catalog.rs` (línea 370-374) |
| No hay ficheros de manifiesto en `guilds/` (solo código y `__pycache__`); `CAPABILITIES = {...}` solo existe en 3 guilds del repo (bash, vision, websearch) | `git grep CAPABILITIES` |

## Tabla guild × capacidad × real

| guild | capacidad | ¿declarada? | REAL hoy | evidencia |
|---|---|---|---|---|
| **bash** | ejecución de procesos | sí (`process_execution: True`) | **shell arbitrario**: `powershell -NoProfile -Command` (win32) / `bash -c` (posix) con el string crudo; allowlist del primer token + bloqueo de metacaracteres **solo fuera de comillas**, y el lexer salta los tramos entrecomillados mientras el shell sí los expande → `python -c "..."`/comillas admiten código arbitrario | `bash.py` (línea 25, 157-180, 240-257) |
| bash | FS lectura/escritura | no | cwd validado a `TYLLUAN_ROOT` o cwd (línea 240-246), pero los comandos que ejecuta pueden tocar cualquier ruta con los permisos del usuario del kernel | `bash.py` (línea 240-246), `utils.py` (línea 64-100) |
| bash | escritura fuera de su carpeta | no | `state_checkpoint` escribe `data/checkpoints/bash.json` relativo al cwd (raíz del repo) | `bash.py` (línea 30-50) |
| bash | tiempo | no | timeout por defecto 30 s y **mata el árbol de procesos** en Windows (`taskkill /T /F`) | `bash.py` (línea 199), `utils.py` (línea 64-140) |
| **code** | FS lectura | no | **sin validate_path**: el path sale de argumentos/intención, `abspath`, `rglob`, `open(...)`; puede leer cualquier directorio accesible (p. ej. fuera del workspace) | `code.py` (línea 245-265, 284) |
| code | escritura / procesos / red | no | **ninguna**: herramientas de análisis (`code_parse`, `code_stats`, `code_todos`, `code_security_scan`); sin subprocess, sin red, sin escritura | `code.py` (línea 71-430) |
| **docker** | ejecución de procesos (CLI docker) | no | **sí**: `asyncio.create_subprocess_exec("docker", ...)` | `docker.py` (línea 16-31) |
| docker | contenedores + red | no | `docker run -d [--name] [-p] <image>`: crea contenedores, publica puertos → red saliente/entrante vía contenedor; `docker exec <id> sh -c <command>` ejecuta comandos arbitrarios dentro | `docker.py` (línea 66-92, 105-115) |
| docker | acceso al daemon (riesgo host) | no | el CLI habla con el socket del daemon: capacidad de facto a nivel host (montajes, privileged) aunque estas tools no pasen `-v`/`--privileged` | `docker.py` (línea 16-31); riesgo inherente al daemon, no una tool |
| docker | tiempo | no | timeout 60 s por operación | `docker.py` (línea 31) |
| **filesystem** | FS lectura + escritura | no | dentro de `_FS_ROOT` (= `TYLLUAN_ROOT` o cwd, `filesystem.py` línea 23) y con `validate_path` **symlink-safe** (`resolve()` + `is_relative_to`); `file_write` crea padres y escribe; **no hay tool de borrado** | `filesystem.py` (línea 23, 68, 144-157), `utils.py` (línea 137-150) |
| filesystem | escritura fuera de su carpeta | no | no: bloqueado fuera de `_FS_ROOT` (y `_FS_ROOT` por defecto es la raíz del repo) | `filesystem.py` (línea 144-145) |
| **git** | ejecución de procesos (CLI git) | no | **sí**: `git -c core.fsmonitor=false -c core.hooksPath=/dev/null -c credential.helper= ...` y fallback `create_subprocess_shell` | `git.py` (línea 25-56, 86-110) |
| git | cwd arbitrario (FS fuera del workspace) | no | `work_dir = cwd or os.getcwd()` **sin validate_path**: puede leer/escribir el `.git` de CUALQUIER repo al que apunte `cwd` (`git add`/`git commit` incluidos) | `git.py` (línea 26, 200-230) |
| git | escrituras | no | `git add` y `git commit` escriben índice/objetos/HEAD en el repo apuntado; hooks desactivados y credenciales vaciadas | `git.py` (línea 27-32, 200-230) |
| git | tiempo | no | timeout 45 s (fallback shell 30 s) | `git.py` (línea 56, 110) |
| **browser** | procesos + red | no (y **DEPRECADA: no registrada**) | cabecera `DEPRECATED — not registered in tylluan.toml`; si se reactiva: lanza Edge/Chrome (`Popen`/`run`), abre URLs arbitrarias (red saliente), consulta CDP en `http://localhost:<puerto>/json`; sin escritura de ficheros (screenshot es placeholder) | `browser.py` (línea 12-13, 90, 105-110, 197-208) |
| browser | tiempo | no | timeout 10 s en `browser_navigate` | `browser.py` (línea 130) |

## Lo que el manifiesto debería declarar (síntesis para el ADR-022)

- **bash**: `process_execution: true` (real, ya declarado) — y con shell real, no "un binario allowlisted": el manifiesto debe decir que el allowlist es defensa en profundidad, **no** una frontera de capacidad; añadir `filesystem_scope: ["<workspace>", "data/"]` (checkpoint) y `network_hosts: []` (no restringe red de los comandos: `cargo`/`npm`/`pip`/`python` salen a red).
- **code**: `filesystem_scope: []` declarado no es cierto: lee fuera del workspace; debería ser `filesystem_scope: ["<host>"]` o añadir el validate_path que hoy no tiene.
- **docker**: `process_execution: true`, `network_hosts: ["*"]` y una capacidad explícita nueva del tipo `container_daemon: true` (el daemon es la capacidad host real).
- **filesystem**: `filesystem_scope: ["<workspace>"]` (real y verificado, symlink-safe); sin proceso, sin red.
- **git**: `process_execution: true`, `filesystem_scope: ["<cwd arbitrario>"]` — hoy no valida; el manifiesto debería forzar workspace (o declarar el riesgo) y marcar que desactiva hooks.
- **browser**: declarar **dormant** (no registrada); si se reactiva, `process_execution: true`, `network_hosts: ["*"]`.
- **común**: `env_inherit: true` + `kernel_token: true` (cualquier guild puede llamar a la API del kernel como operador) es una capacidad que hoy no se declara en ningún sitio y debería ser explícita en el manifiesto.

## Impact

- **Solo lectura**: este informe es el único artefacto (diff sin commitear = 1 fichero en `docs/internal/`). Cero cambios en `src/`, `guilds/` o tests; no se arrancó ninguna guild ni el kernel; no se modificó `E:\tylluan`.
- Insumo directo del manifiesto del ADR-022: 3 de 6 guilds no declaran NADA y las 6 heredan entorno + token del kernel; el inventario da las filas y la evidencia línea a línea para decidir qué es declaración obligatoria (process_execution, filesystem_scope, network_hosts, container_daemon, kernel_token) y qué se queda como defensa en profundidad (allowlist de bash, validate_path de filesystem).
- Riesgo más alto del inventario, por orden: `bash` (shell crudo con comillas que evade el escáner), `docker` (daemon = host), `git` (cwd sin validar + escritura), `code` (lectura sin límite), `filesystem` (correcto pero con raíz = repo), `browser` (inerte hoy).

## No verificado

- **Dinámico**: todo es lectura estática; no se ejecutó ninguna tool de guild (ni `bash_execute`, ni `docker run`, ni `git`) para no tocar el sistema ni el repo. Las capacidades "reales" descritas son las que el código permite, no una medición de explotación.
- **Windows vs POSIX**: en POSIX bash usa `bash -c`; el escáner de metacaracteres y el allowlist son los mismos, pero no se probó el bypass con comillas en ambos shells (solo se leyó el código).
- **Docker guild**: no se comprobó la configuración real del daemon en esta máquina (socket, permisos); el riesgo "daemon = host" es inherente y no se midió.
- **Sandbox**: no se ejecutó ningún guild con `security.sandbox.enabled = true` (Docker strict/balanced) para comparar capacidades reales bajo aislamiento; el inventario es del modo por defecto (nativo).
- **`TYLLUAN_TOKEN`**: se leyó la inyección en el spawn, pero no se verificó en un guild vivo que el token esté presente y sea válido (no se arrancó ninguno).
- **browser**: está deprecada y no registrada; sus capacidades se documentan "si se reactiva", no se ejercitaron.
- **code**: no se auditó cada rama de `code_parse`/`code_stats` para confirmar si algún parámetro permite rutas fuera del host (solo se siguió el flujo principal).
