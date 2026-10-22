# T-DSH-46 — D-46: `exec_tools` en `CAPABILITIES` (implementación aditiva) + CAPABILITIES de browser/mcp_bridge

- **Agente:** dsh
- **Fecha:** 2026-10-23
- **Base:** `E:\tylluan-wt-dsh` en `--detach origin/main` **`a85f4d7`** (push 117: T-DSH-44 y T-DSH-45 integrados); worktree limpio antes de empezar.
- **Encargo (turno 1582):** primera implementación de la propuesta de T-DSH-45: (1) `enforce_capabilities` lee `exec_tools` (aditivo, sin cambiar el comportamiento si el campo falta), allowlist solo desde `command_arg` y bloqueo del bypass `allowlist_arg=false`; (2) declarar CAPABILITIES con `exec_tools` **solo** en `guilds/core/browser.py` y `guilds/core/mcp_bridge.py`; (3) unit tests + actualizar matriz si cambia algún veredicto; criterios de tests/clippy/recuento. Prohibido tocar `prepare.rs`, config por defecto ni `capabilities_enforce`.

## (1) `enforce_capabilities` — semántica declarativa (aditiva)

`crates/tylluan-kernel/src/registry/guild_process.rs`:

- Nuevo `enum DeclaredBinary { NotALocalCommand, Missing, Binary(String) }` y helpers:
  - `extract_declared_binary`: lee `exec_tools.<tool>.command_arg`, exige `command_prefix` si está declarado (p. ej. `stdio://`) y extrae el primer token del comando; si el valor no lleva el prefijo → `NotALocalCommand` (una URL SSE no ejecuta proceso local).
  - `allowlist_bypass_requested`: si `exec_tools.<tool>.allowlist_arg` está declarado y el llamador lo pasa `false` → devuelve el nombre del argumento.
- `enforce_capabilities`:
  - `let declared_exec_tools = caps.get("exec_tools").and_then(|v| v.as_object());`
  - **ejecuta** = si hay `exec_tools`, `map.contains_key(tool_name)` (el nombre deja de importar); si **no** hay `exec_tools`, `is_exec_like_tool(params)` **exactamente como antes** (fallback aditivo).
  - Bypass: si la entrada declara `allowlist_arg` y el llamador lo pone a `false` → `CAPABILITY_BLOCKED` **aunque `process_execution` no esté declarado** ("disabling the allowlist is not allowed while capability enforcement is active").
  - `process_execution = []`: bloquea si `ejecuta` (mismo mensaje).
  - allowlist no vacía: solo si `ejecuta`; binario = declarado (`command_arg`) o, en el camino legacy, `command` de siempre; `Binary` no permitido → bloqueo con el mismo mensaje; `Missing` → "no command could be extracted…"; `NotALocalCommand` → permitido (SSE).
  - `process_execution = false`: bloquea si `ejecuta` (mensaje D-35b intacto).
  - `filesystem_scope` sin cambios.

Con el campo ausente, todos los mensajes y veredictos son idénticos a D-35b (los tests existentes lo demuestran).

## (2) CAPABILITIES declaradas (solo dos guilds)

`guilds/core/browser.py` (deprecado/no registrado, pero está en el catálogo):

```python
CAPABILITIES = {
    "process_execution": False,
    "exec_tools": {
        "browser_navigate": {"why": "subprocess.run para lanzar el navegador del sistema"},
        "search_web": {"why": "delega en browser_navigate"},
    },
}
```

`guilds/core/mcp_bridge.py`:

```python
CAPABILITIES = {
    "process_execution": False,
    "exec_tools": {
        "mcp_list_tools": {"command_arg": "server_url", "command_prefix": "stdio://", "why": "..."},
        "mcp_call": {"command_arg": "server_url", "command_prefix": "stdio://",
                     "allowlist_arg": "enforce_allowlist", "why": "..."},
        "mcp_ping": {"command_arg": "server_url", "command_prefix": "stdio://", "why": "..."},
    },
}
```

Ningún otro guild cambia. El parser del catálogo (T-DSH-42) ya soporta JSON anidado (verificado por la matriz: las capabilities parseadas de ambas coinciden con las declaradas en su `.py`).

## (3) Tests

**Nuevos unit tests en `guild_process::tests`** (5):
1. `test_enforce_exec_tools_declared_non_exec_tool_not_blocked` — con `exec_tools`, `get_execution_status` (exec-like por nombre) **no** se bloquea.
2. `test_enforce_exec_tools_declared_exec_tool_blocked` — `browser_navigate` declarada se bloquea con `process_execution=false`.
3. `test_enforce_without_exec_tools_keeps_name_heuristic` — sin `exec_tools` se mantiene la heurística **tal cual** (incluido su falso positivo conocido con `get_execution_status`).
4. `test_enforce_allowlist_arg_false_blocks_bypass_and_command_arg_allowlist` — `allowlist_arg=false` bloquea; allowlist `["node"]` con `stdio://node …` permite y con `stdio://bash …` bloquea; `http://…/sse` no se bloquea (no es comando local).
5. (incluida en la 4) extracción desde `command_arg` + prefijo sólo para el binario declarado.

**Tests de integración actualizados** (y por qué):

| fichero | cambio | motivo |
|---|---|---|
| `capabilities_enforce_matrix.rs` | `catalog_capabilities_are_parsed_after_d35`: `with_caps` pasa de `["bash","vision","websearch"]` a `["bash","browser","mcp_bridge","vision","websearch"]` y el bucle de igualdad incluye browser/mcp_bridge | las dos guilds nuevas ya declaran CAPABILITIES. **Ningún veredicto de la matriz cambia**: la matriz sigue midiendo con las CAPABILITIES declaradas de bash/vision/websearch, y las nuevas no participan en sus casos. |
| `exec_like_tool_names.rs` | pin de líneas frágiles: `browser:71→browser:81` y `browser:102→browser:112` | el bloque CAPABILITIES añade 10 líneas a `browser.py`; los nombres/defs son los mismos. FP/FN sin cambios. |
| `exec_like_tool_names_all_guilds.rs` | el test de `mcp_bridge` ya no afirma "no declara CAPABILITIES"; ahora asserta que **sí** las declara con `exec_tools`, `command_arg=server_url` y `allowlist_arg=enforce_allowlist`; líneas de `mcp_bridge.py` desplazadas +14 (`48→62`, `136→150`, `87→101`, `92→106`, `162→176`, `167→181`) | T-DSH-46 cierra el hallazgo de T-DSH-45; el auditor pasa a fijar el estado nuevo. |

## Salidas

```text
$ cargo test -p tylluan-kernel --lib
test result: ok. 1191 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 68.15s

$ cargo test -p tylluan-kernel --lib enforce
test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 1174 filtered out

$ cargo test -p tylluan-kernel --test capabilities_enforce_matrix
test result: ok. 3 passed; 0 failed; 0 ignored  (catalogo = ["bash","browser","mcp_bridge","vision","websearch"])
$ cargo test -p tylluan-kernel --test cola_d35_acceptance
test result: ok. 3 passed; 0 failed; 0 ignored
$ cargo test -p tylluan-kernel --test router_websearch_args
test result: ok. 2 passed; 0 failed; 0 ignored
$ cargo test -p tylluan-kernel --test exec_like_tool_names
test result: ok. 1 passed; 0 failed; 0 ignored
$ cargo test -p tylluan-kernel --test exec_like_tool_names_all_guilds
test result: ok. 2 passed; 0 failed; 0 ignored

$ cargo clippy -p tylluan-kernel --all-targets -j 3 -- -D warnings
EXIT 0
```

Cifras: `check_test_count.sh --fix` → **1278** (1191 kernel + 75 link + 12 fsrs); `STATUS.md` líneas 10 y 50 actualizadas a mano (1278/1191); `check_doc_links.py` exit 0; `check_docs_reality.sh` sólo el hallazgo preexistente `ROADMAP_O3.md:42` (artefacto WSL).

## Impact

- **Contención (ruta de seguridad):** cambio **aditivo** en `enforce_capabilities`. Sin `exec_tools` nada cambia; con `exec_tools` (solo browser y mcp_bridge hoy) la decisión de "ejecuta" pasa a ser declarativa y deja de depender del nombre. Efectos cuando la contención está activa (Strict, o Balanced con `capabilities_enforce`):
  - `browser.browser_navigate`/`search_web` se tratan como ejecutoras (lo son) y se bloquean con `process_execution=false`; el resto de tools de browser no.
  - `mcp_bridge`: las tres tools stdio se someten a allowlist extrayendo el binario de `server_url` (tras `stdio://`): `node/npx/uv/python/python3/gk` permitidos; otro binario → bloqueado; una URL SSE (`http://…`) no se bloquea (no ejecuta proceso local); `enforce_allowlist=false` del llamador → **bloqueado por `allowlist_arg`** (se cierra el bypass que documentó T-DSH-45).
  - Las guilds sin `exec_tools` (visión, websearch, bash…) conservan el comportamiento de D-35b byte a byte.
- **Catálogo:** browser y mcp_bridge pasan a exponer capabilities (antes `None`); ninguna otra guild cambia. No se tocan `prepare.rs`, `capabilities_enforce` ni defaults.
- **Riesgo declarado:** `browser.py` está marcado deprecated/no registrado en `tylluan.toml`; sus CAPABILITIES solo importan si se reactiva. `mcp_bridge` sí está activo: si alguien dependía de `mcp_call(stdio://<binario fuera de la allowlist>)` en runtime con contención activa, ahora se bloquea (es el objetivo); el bypass queda también bloqueado. El aviso de T-DSH-45 (mientras exista `bash`, el bypass no abría nada nuevo) sigue siendo cierto, pero la coherencia por guild ya es la correcta.

## No verificado

- **Sin kernel en vivo ni perfiles reales**: no se ejecutó `check_capabilities`/perfil Strict/Balanced contra un kernel; los unit tests llaman directamente a `enforce_capabilities` con caps sintéticas y la matriz usa las declaradas. No se probó una llamada MCP real a `mcp_call` con `stdio://` ni con SSE.
- **Allowlist efectiva del guild**: `mcp_bridge.py` sigue aplicando su propia `_STDIO_ALLOWLIST` (node/npx/uv/python/python3/gk) en el guild; el test del kernel cubre la capa de contención, no la del guild (que no se toca aquí).
- **`browser.py` deprecated**: no está registrado en `tylluan.toml`; no se verificó que el kernel real lo cargue, aunque el catálogo sí lo descubre (la matriz lo prueba).
- **Recuento de docs**: 1278/1191 es la cifra mecánica tras añadir 5 unit tests; si el TL prefiere otra línea canónica, es un cambio de una línea.
- **Números de línea de los pines**: los tests de T-DSH-44/45 fijan líneas de `browser.py`/`mcp_bridge.py`; se actualizaron por el desplazamiento del bloque CAPABILITIES. Cualquier edición futura de esos `.py` los moverá otra vez (es intencional: obliga a revisar).
- Medido en mi Windows sobre `a85f4d7`.
