# T-DSH-45 — `is_exec_like_tool` fuera de `guilds/core` + bypass de allowlist de `mcp_bridge` + propuesta declarativa

- **Agente:** dsh
- **Fecha:** 2026-10-23
- **Base:** `E:\tylluan-wt-dsh` en `--detach origin/main` **`decc315`** (push 115). El diff de T-DSH-44 (test `exec_like_tool_names.rs` + informe) sigue **suelto y separado**; esta tarea añade ficheros nuevos y no lo toca.
- **Encargo (turno 1580):** (1) extender el escáner de T-DSH-44 a TODOS los guilds fuera de `guilds/core` (misma tabla FP/FN); (2) `mcp_bridge`: leer la allowlist por defecto (`mcp_bridge.py:46`) y comprobar si el llamador puede pasar `enforce_allowlist=False` por `mcp_call`, citando `ruta:línea`; (3) propuesta concreta, sin implementar, del campo declarativo en `CAPABILITIES` (nombre, forma, ejemplo para bash/browser/mcp_bridge y cómo lo leería `enforce_capabilities`).

## (1) Escáner extendido a los guilds fuera de core

`tests/exec_like_tool_names_all_guilds.rs` (nuevo): escanea recursivamente `guilds/builders`, `guilds/scholars`, `guilds/wardens`, `guilds/watchers` (excluye `guilds/core`, que cubre T-DSH-44). Misma heurística literal (`guild_process.rs:175-180`), mismo strip de comentarios/docstrings que T-DSH-44, **más**:

- patrones nuevos: `asyncio.create_subprocess_exec`, `asyncio.create_subprocess_shell`, `asyncio.subprocess`, `run_command(` (helper compartido), además de los 8 de T-DSH-44;
- **strip de literales de cadena**: `code.code_security_scan` parecía ejecutar porque el fichero contiene patrones regex como `(r"subprocess\s*\.\s*shell\s*=\s*True", ...)`; al vaciar strings deja de contar (no ejecuta: escanea código).

Resultado: **84 herramientas `@mcp.tool`** fuera de core; **1 falso positivo** y **30 falsos negativos**.

### Falsos positivos (exec-like que NO ejecutan procesos): 1

| herramienta | `ruta:línea` | por qué es exec-like | ejecución real |
|---|---|---|---|
| `database.db_execute_write` | `guilds/builders/plugins/database.py:283` | contiene `execute` | ninguna de proceso: escribe en SQLite con `cursor.execute(...)` (`database.py:311`); el "execute" es SQL, no shell. **Falso positivo claro** |

`database.py` **no declara `CAPABILITIES`** (0 coincidencias), así que hoy no se aplica contención; el FP es de clase (si se declarara `process_execution=false`, esta herramienta legítima quedaría bloqueada).

### Falsos negativos (no exec-like pero SÍ ejecutan procesos): 30

Agrupados; todos ejecutan por `asyncio.create_subprocess_*` o por el helper compartido `utils.run_command` (`guilds/builders/plugins/utils.py:64` → `asyncio.create_subprocess_exec` en `utils.py:92`):

| familia | herramientas | `ruta:línea` | cita de ejecución |
|---|---|---|---|
| git (7) | `git_add`, `git_branch`, `git_commit`, `git_diff`, `git_log`, `git_status`, `git_status_quick` | `guilds/builders/plugins/git.py` (defs 32-220) | `asyncio.create_subprocess_exec(` en `git.py:45`; `asyncio.create_subprocess_shell(` en `git.py:101` |
| docker (5) | `docker_images`, `docker_logs`, `docker_ps`, `docker_status`, `docker_stop` | `guilds/builders/plugins/docker.py` | `asyncio.create_subprocess_exec(` en `docker.py:23` |
| ffmpeg (6) | `audio_extract`, `media_probe`, `video_add_text`, `video_concat`, `video_resize`, `video_trim` | `guilds/builders/plugins/ffmpeg_tools.py` | `await utils.run_command(cmd, ...)` en `ffmpeg_tools.py:24`; helper en `utils.py:64`/`:92` |
| clipboard (2) | `clipboard_read`, `clipboard_write` | `guilds/builders/plugins/clipboard_tools.py` | `utils.run_command([...])` en `clipboard_tools.py:16-40` (powershell/pbpaste/xclip) |
| screenshot (1) | `screenshot_capture` | `guilds/builders/plugins/screenshot_tools.py` | `utils.run_command([...])` en `screenshot_tools.py:33-37` |
| sandbox (3) | `sandbox_ingest`, `sandbox_python`, `sandbox_status` | `guilds/builders/plugins/sandbox.py` | `asyncio.create_subprocess_exec(` en `sandbox.py:66` y `:372`; `utils.run_command(["docker", ...])` en `:42/:87/:105/:220` |
| search (2) | `search_code_structure`, `search_content` | `guilds/scholars/plugins/search.py` | `await utils.run_command(args, ...)` en `search.py:121` (ripgrep/otros binarios) |
| biome (2) | `biome_format_file`, `biome_lint_file` | `guilds/wardens/plugins/biome_warden.py` | `utils.run_command([...])` en `biome_warden.py:26` y `:38` |
| cron (2) | `cron_schedule`, `cron_cancel` | `guilds/watchers/plugins/cron_scheduler.py` | `utils.run_command(["cmd", "/c", job["command"]], ...)` en `cron_scheduler.py:52` — **el comando lo elige el llamador y se ejecuta al disparar** |

Fuera de core el nombre acierta aún menos que dentro: **30/84 ≈ 36 % de herramientas ejecutan procesos con nombre que no lo dice**; y 20+ de ellas lo hacen por un helper compartido (`utils.run_command`) que el escáner detecta por patrón, no por resolución de imports (ver No verificado).

## (2) `mcp_bridge`: la allowlist por defecto y el bypass del llamador

Lectura de `guilds/core/mcp_bridge.py` (verificada por el test `mcp_bridge_allowlist_is_bypassable_by_the_caller`):

- **Allowlist por defecto** (`mcp_bridge.py:48-50`): `_STDIO_ALLOWLIST = frozenset({"node", "npx", "uv", "python", "python3", "gk"})`; se aplica solo cuando `enforce_allowlist=True` (por defecto).
- **El llamador SÍ puede saltársela**: `mcp_call` expone el parámetro `enforce_allowlist: bool = True` (`mcp_bridge.py:136-141`; docstring: "Set False for ad-hoc dev usage", `:154-156`). Si el llamador pasa `False` con `server_url="stdio://<binario arbitrario> args"`, no se comprueba la allowlist, se lanza ese binario y además se le pasa **el entorno completo** del proceso: `env={**os.environ} if not enforce_allowlist else {**_MINIMAL_ENV}` (`mcp_bridge.py:90`).
- **Cadena de propagación del valor**: `mcp_call` → `_do_call(server_url, tool_name, args, enforce_allowlist)` (`:167`) → `_safe_iter_session` (`:257`) → `_iter_session` (`:246-252`, extrae `command = server_url[len("stdio://"):]`) → `_session_stdio(command, enforce_allowlist)` (`:76`) → `StdioServerParameters(command=parts[0], ...)` (`:87-88`) → `stdio_client(params)` (`:92`).
- El propio mensaje de bloqueo lo anuncia: "🚫 BLOCKED by mcp_bridge allowlist: ... **Pass enforce_allowlist=False to bypass (dev only)**" (`mcp_bridge.py:82-86`); y cuando se usa, `mcp_call` deja un `logging.warning("mcp_call: enforce_allowlist=False — bypassing allowlist for stdio://")` (`mcp_bridge.py:162`).
- `mcp_list_tools` (`:101`) y `mcp_ping` (`:181`) exponen el mismo parámetro.
- **`mcp_bridge.py` no declara `CAPABILITIES`** (`:0` coincidencias): `enforce_capabilities` no lo evalúa, así que la única barrera es la allowlist que el propio llamador puede desactivar.

**Respuesta a la pregunta del encargo: sí, el llamador puede pasar `enforce_allowlist=False` por la herramienta `mcp_call` y ejecutar un binario arbitrario con entorno completo; está documentado como "dev only" pero es un argumento de herramienta (el schema de FastMCP lo expone) y no hay contención en el kernel porque el guild no declara capacidades.** Esto convierte a `mcp_bridge` en un candidato prioritario del campo declarativo de (3): sus tres herramientas son ejecutoras con argumento-comando y merecen `process_execution=false` + `exec_tools` + tratamiento del bypass.

## (3) Propuesta: campo declarativo `exec_tools` en `CAPABILITIES` (sin implementar)

**Nombre y forma.** Un único campo opcional por guild, `exec_tools`, cuyas claves son nombres de herramienta (los mismos que expone `@mcp.tool`). Valor por herramienta: objeto con `command_arg` (nombre del argumento que lleva un comando, si lo hay) y `command_prefix` (si el comando va embebido tras un prefijo, p. ej. `stdio://`), o `{}` si ejecuta sin comando del llamador:

```python
CAPABILITIES = {
    "process_execution": False,
    "exec_tools": {
        "browser_navigate": {"why": "lanza el navegador del sistema"},
        "search_web": {"why": "delega en browser_navigate"},
    },
}
```

**Ejemplos pedidos:**

```python
# guilds/builders/plugins/bash.py — ejecutor legítimo: basta el flag actual
CAPABILITIES = {"process_execution": True}

# guilds/core/browser.py — hoy process_execution=False (si se le declara caps);
# las dos tools lanzan un proceso aunque su nombre no lo diga
CAPABILITIES = {
    "process_execution": False,
    "exec_tools": {
        "browser_navigate": {"why": "subprocess.run(browser_path, ...)"},
        "search_web": {"why": "via browser_navigate"},
    },
}

# guilds/core/mcp_bridge.py — el comando lo aporta el llamador; declarar también
# el argumento y el requisito de allowlist
CAPABILITIES = {
    "process_execution": False,
    "exec_tools": {
        "mcp_list_tools": {"command_arg": "server_url", "command_prefix": "stdio://"},
        "mcp_call":       {"command_arg": "server_url", "command_prefix": "stdio://",
                           "allowlist_arg": "enforce_allowlist"},
        "mcp_ping":       {"command_arg": "server_url", "command_prefix": "stdio://"},
    },
}
```

**Cómo lo leería `enforce_capabilities`** (`guild_process.rs:185-250`), sin cambiar firmas:

1. `let declared = caps.get("exec_tools").and_then(|v| v.get(tool_name));`
2. **Ejecuta** = `declared.is_some()`; si el campo `exec_tools` **no existe** en la guild, mantener la heurística de nombre actual como *fallback* de compatibilidad (así este cambio es aditivo y no rompe guilds sin migrar). Con el campo presente, el nombre deja de importar (muere el caso `truncate`/`get_execution_status`/`database.db_execute_write`).
3. Para `process_execution=false`: bloquear si el tool está en `exec_tools` (en vez de `is_exec_like_tool`).
4. Para allowlist no vacía: extraer el binario **solo** del argumento declarado (`command_arg`), tras quitar `command_prefix` y `shlex`-dividir; comparar contra la allowlist. No inferir de la mera presencia de `command` (lección de T-DSH-43: el router lo siembra en toda llamada).
5. Caso especial `allowlist_arg` (mcp_bridge): el bypass es un argumento booleano del llamador; con `exec_tools` el kernel puede (a) exigir allowlist siempre para `stdio://` y no exponer el bypass, o (b) permitirlo solo si un flag de configuración lo autoriza. Al menos, la decisión pasa a ser explícita y auditable.
6. `catalog.rs::extract_capabilities` ya parsea JSON anidado por conteo de llaves (T-DSH-42), así que el campo llega al `GuildDescriptor` sin tocar el parser; el trabajo es de esquema y de migración de guilds.

**Migración mínima**: añadir `exec_tools` primero a mcp_bridge, browser, comfy_ui, llama_backend, git, docker, ffmpeg_tools, clipboard_tools, screenshot_tools, sandbox, search (scholars), biome_warden y cron_scheduler (las 30 FN + 1 FP de las tablas), y a bash dejarlo como está (`process_execution: true`).

## Verificación

| criterio | resultado |
|---|---|
| `cargo test -p tylluan-kernel --test exec_like_tool_names_all_guilds -- --nocapture` | **2 passed / 0 failed** (0,34 s): tabla FP/FN + auditoría de allowlist |
| `cargo clippy -p tylluan-kernel --all-targets -j 3 -- -D warnings` | **exit 0** |
| `check_test_count.sh` | exit 0 (1274; test de integración no cuenta) |
| `check_doc_links.py` | exit 0 |
| sin `cfg(unix)`/`cfg(windows)` en ninguna función; sin `src`; sin kernel | ok |

## Impact

- **Solo lectura + test nuevo**: cero cambios en `src/`, guilds, configuración o corpus; no arranca el kernel ni toca puertos; solo lee `.py` (~0,4 s).
- **Para la contención**: completa la medición de T-DSH-44 y agrava el diagnóstico: fuera de core, **30/84 (36 %) herramientas ejecutan procesos con nombre no-exec-like** y 1 exec-like es SQL (no proceso). La heurística de nombre es insostenible como criterio único; el campo `exec_tools` propuesto es aditivo (fallback al nombre si falta) y cubre exactamente los FP/FN medidos.
- **Seguridad de mcp_bridge**: documenta que el bypass de allowlist es **controlado por el llamador** (`mcp_call(enforce_allowlist=False, server_url="stdio://...")`), con ejecución de binario arbitrario y entorno completo, y sin `CAPABILITIES` que lo frene. No se explota: es auditoría de código; la mitigación propuesta es declarar `exec_tools` con `allowlist_arg` y no exponer el bypass (o gatearlo por configuración).

## No verificado

- **Escáner heurístico**: los 30 FN se detectan por patrones literales (`asyncio.create_subprocess_*`, `utils.run_command(`) y transitividad **del mismo fichero**; el helper compartido `utils.run_command` se reconoce por el patrón `run_command(`, no resolviendo imports entre ficheros. No sigue `getattr`/callbacks, ni hilos/tareas diferidas (p. ej. `cron_schedule` ejecuta el comando en un timer, `cron_scheduler.py:52` se cita por el helper que corre el comando).
- **`code_security_scan`**: apareció como FN en una primera pasada por contener regex (`r"subprocess\\s*\\.\\s*shell"`); al ignorar literales de cadena se descartó. Puede haber casos análogos en otros ficheros con patrones exec-like dentro de strings no cubiertos.
- **No ejecuté nada en vivo**: ni el kernel ni las herramientas; la allowlist/bypass se auditan por lectura del código (las líneas citadas son las reales del fichero, verificadas por el test con `assert_eq!` sobre los números de línea 48/136/162/167/87/92).
- **Exposición en el schema MCP**: que FastMCP publique `enforce_allowlist` como argumento de herramienta se infiere de la firma de `mcp_call` (no se introspeccionó el schema en vivo); si el cliente lo omitiera, el bypass no se usaría, pero la puerta existe.
- **Alcance**: solo `@mcp.tool` de `guilds/builders`, `scholars`, `wardens`, `watchers` (no árbol raíz `guilds/*.py`, no otras rutas); no se miden guilds externos/MCP de terceros.
- **Medido en mi Windows** sobre `decc315`; el test depende de encontrar `guilds/` vía `CARGO_MANIFEST_DIR/../..`.
