# T-DSH-47 — Declaraciones `exec_tools` para el resto de guilds con ejecución real (propuesta lista para pegar)

- **Agente:** dsh
- **Fecha:** 2026-10-23
- **Base:** worktree `E:\tylluan-wt-dsh` en `a85f4d7` **con el diff de T-DSH-46 sin commitear** (TL lo revisó y lo empuja el próximo ciclo); `git fetch` da `origin/main = 6c58a36` (push 118, D-32 de Deep). No se ha movido el HEAD para no perder el diff de T-DSH-46. T-DSH-47 añade solo un test nuevo y este informe.
- **Encargo (turno 1590):** cerrar la deuda del barrido: declarar en informe (sin implementar) la lista exacta de `exec_tools` para el resto de guilds con ejecución real de las tablas FN (git, docker, ffmpeg_tools, clipboard_tools, screenshot_tools, sandbox, search, biome_warden, cron_scheduler, comfy_ui, llama_backend), con `command_arg` donde el llamador aporta el comando (cron_schedule en especial) y marcar cuáles NO deberían entrar en Strict; test que compruebe que cada herramienta nombrada existe en su `.py` y que ninguna FN de las tablas queda sin declarar.

## Universo: 33 herramientas (de dónde salen)

- **T-DSH-44** (core): 8 FN − `browser_navigate`, `search_web`, `mcp_call`, `mcp_list_tools`, `mcp_ping` (browser/mcp_bridge ya declarados en T-DSH-46) = **3** (`comfy_ui`×2 + `llama_backend`×1).
- **T-DSH-45** (fuera de core): **30** (git 7, docker 5, ffmpeg 6, clipboard 2, screenshot 1, sandbox 3, search 2, biome 2, cron 2).
- Total **33**. Excluido `database.db_execute_write`: es el **falso positivo** de T-DSH-45 (SQL, no proceso), no una FN.

`tests/exec_tools_declarations_cover_fn_table.rs` fija exactamente estas 33 y comprueba contra los `.py`: (a) cada herramienta existe como `@mcp.tool`; (b) `FN_TABLE ∩ declaradas = 33`, sin huecos ni sobrantes; (c) solo `cron_schedule` tiene `command_arg` y es la única marcada para contención estricta; (d) su firma real recibe `command: str`.

## Bloque recomendado para contención estricta: `cron_scheduler.py`

Es la única herramienta del barrido donde **el comando lo elige el llamador** y se ejecuta después en cada disparo (`utils.run_command(["cmd", "/c", job["command"]], ...)` en `cron_scheduler.py:52`):

```python
# guilds/watchers/plugins/cron_scheduler.py — recomendado con contención
CAPABILITIES = {
    "process_execution": False,
    "exec_tools": {
        "cron_schedule": {
            "command_arg": "command",
            "why": "el comando lo aporta el llamador y se ejecuta en cada disparo (cron_scheduler.py:52)",
        },
        "cron_cancel": {
            "why": "ejecucion indirecta al cargar/arrancar timers (_init -> utils.run_command); "
                   "declararla bajo process_execution=false bloquearia cancelar (uso normal)",
        },
    },
}
```

**Ojo con `cron_cancel`**: declararla aquí bajo `process_execution=false` la bloquearía (aunque no reciba comando del llamador), porque el escáner la ve ejecutar al cargar timers existentes. Si se aplica el bloque tal cual, hay que decidir: (a) aceptar ese bloqueo como parte de Strict, o (b) usar la variante exenta de abajo para `cron_cancel` (dejando la contención solo en `cron_schedule`).

## Bloques exentos de Strict (uso normal del producto)

Si se declaran con `process_execution=false`, Strict/`capabilities_enforce` bloquearía funcionalidad normal (git, docker, vídeo, inferencia, portapapeles, capturas, búsqueda, biome). La propuesta es declararlos con **`process_execution: True`** y `exec_tools` como metadato (documenta qué herramientas ejecutan sin bloquearlas):

```python
# guilds/builders/plugins/git.py — EXENTO de Strict (producto normal)
CAPABILITIES = {
    "process_execution": True,
    "exec_tools": {
        "git_add": {}, "git_branch": {}, "git_commit": {}, "git_diff": {},
        "git_log": {}, "git_status": {}, "git_status_quick": {},
    },
}

# guilds/builders/plugins/docker.py — EXENTO
CAPABILITIES = {
    "process_execution": True,
    "exec_tools": {
        "docker_images": {}, "docker_logs": {}, "docker_ps": {},
        "docker_status": {}, "docker_stop": {},
    },
}

# guilds/builders/plugins/ffmpeg_tools.py — EXENTO
CAPABILITIES = {
    "process_execution": True,
    "exec_tools": {
        "audio_extract": {}, "media_probe": {}, "video_add_text": {},
        "video_concat": {}, "video_resize": {}, "video_trim": {},
    },
}

# guilds/builders/plugins/clipboard_tools.py — EXENTO
CAPABILITIES = {
    "process_execution": True,
    "exec_tools": {"clipboard_read": {}, "clipboard_write": {}},
}

# guilds/builders/plugins/screenshot_tools.py — EXENTO
CAPABILITIES = {
    "process_execution": True,
    "exec_tools": {"screenshot_capture": {}},
}

# guilds/builders/plugins/sandbox.py — EXENTO (el sandbox ES la contención)
CAPABILITIES = {
    "process_execution": True,
    "exec_tools": {"sandbox_ingest": {}, "sandbox_python": {}, "sandbox_status": {}},
}

# guilds/scholars/plugins/search.py — EXENTO
CAPABILITIES = {
    "process_execution": True,
    "exec_tools": {"search_code_structure": {}, "search_content": {}},
}

# guilds/wardens/plugins/biome_warden.py — EXENTO
CAPABILITIES = {
    "process_execution": True,
    "exec_tools": {"biome_format_file": {}, "biome_lint_file": {}},
}

# guilds/core/comfy_ui.py — EXENTO (ffmpeg del montaje de vídeo)
CAPABILITIES = {
    "process_execution": True,
    "exec_tools": {"generate_short": {}, "generate_documentary_video": {}},
}

# guilds/core/llama_backend.py — EXENTO (auto-arranque de llama-server = inferencia)
CAPABILITIES = {
    "process_execution": True,
    "exec_tools": {"query_model": {}},
}
```

## Resumen de la clasificación

| guild/fichero | herramientas | `command_arg` | Strict |
|---|---|---|---|
| `watchers/plugins/cron_scheduler.py` | `cron_schedule`, `cron_cancel` | `cron_schedule: command` | **sí** (recomendado; ver matiz de `cron_cancel`) |
| `builders/plugins/git.py` | 7 | — | no (producto normal) |
| `builders/plugins/docker.py` | 5 | — | no |
| `builders/plugins/ffmpeg_tools.py` | 6 | — | no |
| `builders/plugins/clipboard_tools.py` | 2 | — | no |
| `builders/plugins/screenshot_tools.py` | 1 | — | no |
| `builders/plugins/sandbox.py` | 3 | — | no (el sandbox ES la contención) |
| `scholars/plugins/search.py` | 2 | — | no |
| `wardens/plugins/biome_warden.py` | 2 | — | no |
| `core/comfy_ui.py` | 2 | — | no |
| `core/llama_backend.py` | 1 | — | no |

## Verificación

| criterio | resultado |
|---|---|
| `cargo test -p tylluan-kernel --test exec_tools_declarations_cover_fn_table -- --nocapture` | **1 passed / 0 failed** (0,02 s); tabla + `declaradas=33 FN_tabla=33 sin_declarar=[]` |
| `cargo clippy -p tylluan-kernel --all-targets -j 3 -- -D warnings` | **exit 0** |
| `check_test_count.sh` | exit 0 (1278; test de integración no cuenta) |
| `check_doc_links.py` | exit 0 |
| sin `src`, sin tocar `.py`, sin kernel | ok |

## Impact

- **Solo lectura + test nuevo**: cero cambios en `src/`, guilds, configuración o corpus; no arranca el kernel; ~0,02 s. No modifica ningún `.py` (los bloques son propuesta).
- **Cierra el barrido**: deja 33/33 FN documentadas y un test que falla si la propuesta deja de corresponderse con los `.py` (herramienta renombrada/eliminada) o si la tabla FN cambia sin actualizar la propuesta.
- **Decisión preparada**: separa lo que debe contener Strict (`cron_schedule`, comando del llamador) de lo que es uso normal del producto (los otros 32), evitando el efecto "seguridad que estorba". Si el TL prefiere contención total, basta cambiar `process_execution` a `False` en los bloques exentos y aceptar los bloqueos (documentado).
- **Aviso de integración**: mi worktree sigue en `a85f4d7` + diff de T-DSH-46 (no integrado aún); `origin/main` en `6c58a36`. El test nuevo no depende de ese diff, pero el recuento de lib (1191) sí lo incluye.

## No verificado

- **Recomendación de exención no validada en vivo**: no se ejecutó el kernel con Strict/Balanced ni se comprobó que esos 32 bloqueos rompan el producto; es un juicio por dominio (git/docker/vídeo/inferencia son flujos normales), no una medición.
- **Tests de las tablas hardcodeadas**: el test fija las 33 FN de las tablas de T-DSH-44/45 **a mano**; no vuelve a ejecutar el escáner (si aparece una FN nueva, el test no la detecta por sí solo; habría que actualizar las dos listas).
- **`cron_cancel`**: su ejecución es indirecta (vía `_init` que carga timers) según el escáner; no se verificó en runtime que cancelar ejecute realmente algo ni qué pasa si se bloquea.
- **`sandbox`**: se clasifica como exento por ser el mecanismo de aislamiento; no se auditó su interacción con Strict (un sandbox bloqueado por Strict sería paradójico, pero no se probó).
- **Los bloques no están aplicados**: el catálogo sigue igual; los números de línea y el estado real de `CAPABILITIES` en esas guilds no cambian.
- **Medido en mi Windows** sobre `a85f4d7` + diff T-DSH-46, no sobre `origin/main` `6c58a36`.
