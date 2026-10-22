# T-DSH-44 — ¿Acierta `is_exec_like_tool` con los nombres reales de `guilds/core`?

- **Agente:** dsh
- **Fecha:** 2026-10-23
- **Base:** `E:\tylluan-wt-dsh` en `--detach origin/main` **`decc315`** (push 115: T-DSH-42 + D-35b integrados); worktree limpio antes de empezar.
- **Encargo (turno 1577):** medir la heurística `is_exec_like_tool` (`guild_process.rs:175`) sobre **todas** las herramientas reales de los `.py` de `guilds/core` (parseando `@mcp.tool` / `def`): (1) falsos positivos (exec-like y no deberían serlo, p. ej. `'run'` dentro de otra palabra como `truncate`); (2) falsos negativos (no exec-like pero ejecutan procesos o aceptan un argumento que se ejecuta; buscar `subprocess`/`os.system`/`Popen`). Entregar `tests/exec_like_tool_names.rs` con tabla `--nocapture` y aserción viva; informe con `## Impact` y `No verificado`; sin tocar `src`, sin kernel; citas literales `ruta:línea`.

## Método

`tests/exec_like_tool_names.rs` (nuevo; solo lee ficheros) replica la heurística **literal**:

```rust
// guild_process.rs:175-180
tn.contains("execute") || tn.contains("run")
    || tn.contains("exec") || tn.contains("spawn")
    || tn.contains("shell") || tn.contains("command")
```

y escanea `guilds/core/*.py`:

- localiza defs de nivel 0 (`def` / `async def`, firma multilínea incluida) y marca `is_tool` por decorador contiguo (`@mcp.tool`, `@app.tool`, `@tool`);
- **strip de comentarios y docstrings** antes de buscar patrones (importante: `night_reasoner.py:104-117` y `llama_backend.py:560-568` mencionan `subprocess` solo en documentación);
- patrones de proceso: `subprocess.`, `os.system`, `os.popen`, `Popen(`, `check_output`, `check_call`, `shell=True`, `stdio_client(`;
- **transitividad**: si el cuerpo llama a un helper del mismo fichero que ejecuta procesos, se cuenta (p. ej. `generate_short` → `_ken_burns_clip`);
- **argumento ejecutado**: un parámetro de la herramienta aparece en el cuerpo (o en la cadena de helpers llamados) que construye `StdioServerParameters`/`stdio_client`/`subprocess` (`mcp_bridge`).

Resultado: **65 herramientas `@mcp.tool`** en `guilds/core`; **2 falsos positivos** y **8 falsos negativos locales** (más 10 coincidencias de subcadena en helpers no-herramienta).

## (1) Falsos positivos: exec-like que NO ejecutan procesos

| herramienta | `ruta:línea` | por qué es exec-like | ejecución real |
|---|---|---|---|
| `n8n_bridge.execute_workflow` | `guilds/core/n8n_bridge.py:144` | contiene `execute` | ninguna local: hace HTTP a n8n (no `subprocess`); **ejecuta un workflow remoto**, no un proceso |
| `n8n_bridge.get_execution_status` | `guilds/core/n8n_bridge.py:244` | contiene `exec` (de "execution") | ninguna: consulta estado de una ejecución (lectura) → **falso positivo claro** |

Matiz importante de impacto: `n8n_bridge.py` **no declara `CAPABILITIES`** (0 coincidencias en el fichero), así que `enforce_capabilities` no lo mira hoy (caps `None` → sin contención). Son FP **de clase** (si mañana se le declaran caps con `process_execution=false`, `get_execution_status` quedaría bloqueada por su nombre).

## (2) Falsos negativos: no exec-like pero ejecutan procesos

Medidos por el test (directo o por helper del mismo fichero, con líneas donde aparece el patrón):

| herramienta | `ruta:línea` | ejecuta | cita |
|---|---|---|---|
| `browser.browser_navigate` | `guilds/core/browser.py:134` | lanza el navegador | `subprocess.run(` en `browser.py:149`; `subprocess.TimeoutExpired` en `browser.py:162` |
| `browser.search_web` | `guilds/core/browser.py:203` | ídem (transitivo) | llama a `browser_navigate(...)`; patrón en `browser.py:149/162` |
| `comfy_ui.generate_short` | `guilds/core/comfy_ui.py:842` | ffmpeg local (montaje de vídeo) | `_ken_burns_clip(` → `subprocess.run(cmd, ...)` en `comfy_ui.py:385-416`; `_crossfade_concat` en `:428/:453`; `_add_audio_track` en `:472` |
| `comfy_ui.generate_documentary_video` | `guilds/core/comfy_ui.py:1023` | ffmpeg local | llama a `_ken_burns_clip(` y `_crossfade_concat(`; patrones en `comfy_ui.py:416/428/453` |
| `llama_backend.query_model` | `guilds/core/llama_backend.py:714` | lanza `llama-server` | `await _start_llama_server()` en `llama_backend.py:762` → `_start_llama_server_locked` → `subprocess.Popen(` en `:638-641` |
| `mcp_bridge.mcp_call` | `guilds/core/mcp_bridge.py:136` | proceso elegido por el llamador (`stdio://comando`) | `_do_call` → `_iter_session` → `_session_stdio`: `StdioServerParameters(command=parts[0], ...)` en `mcp_bridge.py:87-88` y `stdio_client(params)` en `:92`; allowlist por defecto (`enforce_allowlist=True`, lista en `:46`) |
| `mcp_bridge.mcp_list_tools` | `guilds/core/mcp_bridge.py:99` | ídem vía `_fetch_tools` | mismo `stdio_client(` en `mcp_bridge.py:92` |
| `mcp_bridge.mcp_ping` | `guilds/core/mcp_bridge.py:179` | ídem vía `_fetch_tools` | mismo `stdio_client(` en `mcp_bridge.py:92` |

Estos 8 tienen `process_execution` declarado `false`/sin declarar sólo en algunos casos; para los `mcp_bridge` **no hay CAPABILITIES** hoy, así que la contención no los ve. Si se les declararan caps, el nombre no avisaría de que ejecutan.

### Casos indirectos que el escáner NO cuenta como locales (documentados, no asertados)

- `night_reasoner.reason_about` (`night_reasoner.py:329`) → `_reason_with_llama` → guild `llama_backend`, que **auto-arranca `llama-server`**; la propia docstring lo dice (`night_reasoner.py:104-117`) y el gate es `[security] coherence_gate_hybrid_enabled` (default `false`). Es ejecución **cross-guild**, no un `subprocess` en este fichero; por eso no entra en el conteo local.
- `scheduler.schedule` (`scheduler.py:136`) arranca un hilo de fondo que sondea cada 30 s (`scheduler.py:101-120`) y en su ruta de disparo llama a `_run_deepeval_once` (`scheduler.py:171`) → `subprocess.run(` en `scheduler.py:179`. Es ejecución **diferida en hilo**, fuera del cuerpo síncrono que mide el escáner.

## (3) Subcadenas exec-like en defs NO-herramienta (fragilidad del mecanismo)

Coincidencias por subcadena que hoy no son invocables por MCP (helpers internos), pero muestran el riesgo del criterio si alguno pasara a ser herramienta:

| `ruta:línea` | def | subcadena |
|---|---|---|
| `guilds/core/browser.py:71` | `ensure_browser_running` | `run` |
| `guilds/core/browser.py:102` | `send_cdp_command` | `command` |
| `guilds/core/coloquio.py:336` | `_truncate` | `run` (dentro de "t**run**cate") |
| `guilds/core/coloquio_digest.py:279` | `_run_reasoning_background` | `run` |
| `guilds/core/scheduler.py:171` | `_run_deepeval_once` | `run` |
| `guilds/core/utils.py:64` | `run_command` | `run` (este sí ejecuta: `subprocess.run` en `utils.py:88-89`) |
| `guilds/core/utils.py:165` | `find_executable` | `exec` |
| `guilds/core/utils.py:177` | `truncate_output` | `run` (dentro de "t**run**cate") |
| `guilds/core/utils.py:272` | `safe_mcp_run` | `run` |
| `guilds/core/vision.py:338` | `_run_vision` | `run` |

## Conclusión

Con el criterio actual (nombre = subcadena), sobre las 65 herramientas reales de `guilds/core`: **2 falsos positivos** (bloquearían legítimas si tuvieran caps, como `get_execution_status`) y **8 falsos negativos locales** (dejarían pasar ejecutores reales: navegador, ffmpeg, `llama-server`, stdio MCP con comando del llamador). Además hay 10 helpers cuya subcadena coincide por accidente (`truncate` ⊃ `run`).

Es decir: **el nombre es una señal débil en ambos sentidos**. No implemento nada (encargo read-only); si se quiere endurecer sin falsos bloqueos, el criterio debería ser **declarativo por herramienta** en lugar de inferido del nombre, p. ej.:

- campo opcional en `CAPABILITIES` (p. ej. `exec_tools: [...]` / `command_arg` por herramienta) que diga qué herramientas ejecutan y con qué argumento; el nombre queda como heurística de último recurso;
- o allowlist explícita de herramientas exec por guild revisada contra la tabla de arriba (los 8 FN y los 2 FP son la lista de trabajo);
- o derivarlo del `inputSchema` de la herramienta si declara un argumento `command`, con la advertencia ya conocida de T-DSH-43 (el router siembra `command` en toda llamada, así que el schema debe marcar la semántica, no la mera presencia).

Cualquiera de las tres evita el problema de `truncate`/`execution` sin debilitar la contención.

## Verificación

| criterio | resultado |
|---|---|
| `cargo test -p tylluan-kernel --test exec_like_tool_names -- --nocapture` | **1 passed / 0 failed** (0,87 s); tabla y listas impresas |
| `cargo clippy -p tylluan-kernel --all-targets -j 3 -- -D warnings` | **exit 0** |
| `check_test_count.sh` | exit 0 (1274; test de integración no cuenta) |
| `check_doc_links.py` / `check_docs_reality.sh` | links 0; reality solo el hallazgo preexistente `ROADMAP_O3.md:42` (artefacto WSL) |
| sin `cfg(unix)`/`cfg(windows)` en ninguna función; sin `src`; sin kernel | ok |

## Impact

- **Solo lectura + test nuevo**: cero cambios en `src/`, guilds, configuración o corpus; no arranca el kernel ni toca puertos; solo lee `.py` del repo (~0,9 s).
- **Para la contención**: cuantifica la calidad del criterio de D-35b (nombre) sobre el catálogo real: 2 FP / 8 FN locales sobre 65 herramientas, con la lista concreta y citada. Deja una base para decidir si se pasa a criterio declarativo antes de encender `capabilities_enforce` en producción.
- **Aserción viva**: fija el resultado de hoy (2 FP, 8 FN, 10 subcadenas en helpers); si un guild añade una herramienta exec-like o un ejecutor con nombre inocente, el test falla y obliga a revisar la lista.

## No verificado

- **Cobertura**: solo `guilds/core` (encargo); no `guilds/builders/`, `guilds/integrations/` ni el resto del árbol. La foto puede cambiar si se añaden tools allí.
- **Escáner heurístico**: detecta procesos por patrones literales y llamadas directas del mismo fichero; no sigue dispatch dinámico (`getattr`, callbacks, imports de otros `.py`), ni hilos/tareas diferidas (los dos casos indirectos de arriba van documentados aparte, no asertados), ni decoradores no contiguos raros.
- **`n8n_bridge` FP sin efecto hoy**: no declara `CAPABILITIES`, así que la contención no lo evalúa; el FP es de clase, no de runtime actual.
- **`mcp_bridge`**: el arg-exec está mitigado por `enforce_allowlist=True` por defecto (lista en `mcp_bridge.py:46`); no verifiqué qué permite exactamente esa lista ni si el llamador puede pasarla a `False`.
- **No ejecuté ninguna herramienta** ni el kernel: la clasificación "ejecuta" sale del código, no de una llamada real; no hay medición de FP/FN en runtime.
- **Medido en mi Windows** sobre `decc315`; el test lee rutas relativas a `CARGO_MANIFEST_DIR/../..` (funciona en el worktree y en CI con el árbol completo; en otro layout fallaría con "no se pudo leer .../guilds/core").
