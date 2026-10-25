# T-DSH-83 — Preparar clientes del repo para `dev_mode=false` sin cambiar su comportamiento actual

- **Agente:** dsh
- **Fecha:** 2026-10-25
- **Base:** `origin/main` `3139a32`, tras `agent_wt.sh reset dsh`. **Diff SIN commitear**; sin commit ni push.
- **Encargo (T1751):** (a) `scripts/coloquio_loop_buffy.py` y `scripts/coloquio_summarizer.py`: añadir `Authorization: Bearer` leyendo `TYLLUAN_TOKEN` o `.tylluan-token` (raíz del repo; sin token, sin cabecera, sin fallar); (b) `tylluan sandbox status/set/guild/session/unset-guild/unset-session`: flag `--token` + env `TYLLUAN_TOKEN`, mismo patrón que `tylluan stop`. Tests unitarios del helper de token (env gana al fichero, fichero ausente ⇒ `None`) en Rust y en Python (unittest sin kernel). Sin tokens reales en ficheros trackeados. Excluidos `api_contracts.rs`, `transport/http/mod.rs`, `api_federation.rs`.

## 1. Cambios (5 ficheros: 3 modificados, 2 nuevos; +108/−10 en los modificados)

| Fichero | Cambio |
|---|---|
| `scripts/kernel_token.py` (**nuevo**) | `resolve_token(env=None, token_file=None)` (línea 24): prioridad **`TYLLUAN_TOKEN`** → **`.tylluan-token`** en la raíz del repo (`TOKEN_FILE`, línea 21); ausente/vacío → `None`; no loguea ni persiste nada. `kernel_headers(extra=None, env=None, token_file=None)` (línea 45): añade `Authorization: Bearer <token>` **solo si hay token**; con `extra` se mergea. Con tests inyectables (mismo módulo usable por los dos scripts). |
| `scripts/coloquio_loop_buffy.py` | Añade bootstrap de `sys.path` + `from kernel_token import kernel_headers` (líneas 34-39) y `http_json` pasa `headers=kernel_headers()` (línea 60). Sin token: petición idéntica a hoy. |
| `scripts/coloquio_summarizer.py` | Igual import (líneas 26-31); `_get` usa `urllib.request.Request(..., headers=kernel_headers(), method="GET")` (línea 108) y `_post_tylluan_remember` usa `kernel_headers({"Content-Type": "application/json"})` (línea 126). Sin token: headers idénticos a hoy. |
| `crates/tylluan-cli/src/main.rs` | `Sandbox` acepta `-t/--token` (línea 141, doc "o set TYLLUAN_TOKEN"); el dispatch lo resuelve con `resolve_cli_token` (línea 1613) y los **6** subcomandos (`status`, `set`, `guild`, `session`, `unset-guild`, `unset-session`) envuelven su petición con `with_optional_bearer` (línea 1629), igual que `stop_instance` (`bearer_auth`). `pick_token` (línea 1620) es la precedencia pura: flag → env → fichero (`.tylluan-token` del **cwd**, el mismo que lee el kernel), con `trim` y descarte de vacíos. Sin token ⇒ misma petición sin cabecera que hoy. |
| `scripts/tests/__init__.py` + `scripts/tests/test_kernel_token.py` (**nuevos**) | 10 tests unittest sin kernel ni red: env gana al fichero, fichero con env ausente/vacío, fichero ausente o en blanco ⇒ `None`, `TOKEN_FILE` en la raíz, sin token ⇒ sin `Authorization` (y `extra` intacto), con token ⇒ `Bearer` + `extra`, env gana al fichero en headers. |

**Ningún valor de token real** en ficheros trackeados: los tests usan literales ficticios (`env-token`, `file-token`, `s3cr3t-from-file`) y el módulo solo lee env/fichero.

## 2. Tests nuevos

- **Rust** (`crates/tylluan-cli/src/main.rs`, `mod sandbox_token_tests`, línea 1640): `pick_token_prefers_flag_then_env_then_file` (1645, incluye "env gana al fichero" y `None` con todo ausente), `pick_token_ignores_empty_and_whitespace` (1663), `sandbox_subcommand_accepts_token_flag` (1673, comprueba con clap que el subcomando expone `--token`).
- **Python** (`scripts/tests/test_kernel_token.py`): 10 tests en `ResolveTokenTests` y `KernelHeadersTests` (líneas 15-81).

## 3. Verificación (salidas reales)

| Comando | Resultado |
|---|---|
| `python -m unittest discover -s scripts -p 'test_*.py'` | **10 tests, OK** (0.159 s) |
| `python -m py_compile scripts/kernel_token.py scripts/coloquio_loop_buffy.py scripts/coloquio_summarizer.py` | **exit 0** |
| `python scripts/coloquio_loop_buffy.py --help` y `python scripts/coloquio_summarizer.py --help` | imprimen usage (import de `kernel_token` OK en ejecución real) |
| `cargo clippy --workspace --all-targets -- -D warnings` | **exit 0** (42,08 s) |
| `cargo test -p tylluan-cli` | **30 passed; 0 failed** (incluye los 3 tests nuevos; `installer_flag_tests` sigue verde) |
| `cargo test -p tylluan-kernel --lib` | **1276 passed; 0 failed; 4 ignored** — **sin cambios** respecto a la base |
| `cargo build -p tylluan-cli` + `tylluan-cli.exe sandbox --help` | muestra `-t, --token <TOKEN>  Bearer token for authenticated instances (or set TYLLUAN_TOKEN)` |
| `scripts/check_test_count.sh` | **exit 0** (1363 = 1276 + 75 + 12; docs ya sincronizadas) |

`check_doc_links.py` exit 0; `check_docs_reality.sh` exit 1 solo por el artefacto WSL conocido (`ROADMAP_O3.md`, línea 42).

## Impact

- **Sin cambio de comportamiento hoy** (`dev_mode=true`): sin `TYLLUAN_TOKEN` y sin `.tylluan-token`, los scripts envían exactamente las mismas peticiones sin cabecera y el CLI sandbox también; el kernel sigue ignorando auth. Los tests fijan esa propiedad (headers vacíos sin token) y la precedencia env > fichero.
- **Preparado para `dev_mode=false`**: los dos scripts de Coloquio (fila 3 de T-DSH-82) y los 6 subcomandos de `tylluan sandbox` (fila 6) enviarán `Authorization: Bearer` cuando el operador exporte `TYLLUAN_TOKEN` o exista `.tylluan-token` en la raíz/cwd, sin tocar más código. El CLI replica el patrón ya usado por `tylluan stop` (`bearer_auth`, `main.rs` `stop_instance`).
- El helper Python es reutilizable (módulo `scripts/kernel_token.py`) y no loguea ni escribe el token; los tests no tocan la red ni el kernel. No se modificó ningún fichero de kernel ni de los excluidos.

## No verificado

- **No arranqué el kernel** (ni con `dev_mode=false`): no hay prueba en vivo de que un kernel con auth acepte estas cabeceras; la verificación es unitaria (helpers) y de compilación/CLI help.
- **No ejecuté los scripts contra el kernel** (solo `--help`, import y tests): no verifiqué el flujo end-to-end de `coloquio_loop_buffy.py --once` / `coloquio_summarizer.py` con token real, ni que no cambien los headers por otras capas (urllib añade los suyos).
- **Ruff** no se ejecutó sobre `scripts/` (el job Python del CI es de los guilds); `py_compile` y unittest pasan, pero no hay lint nuevo.
- **CLI**: no probé una llamada real a `/api/v1/config*` con `--token` (sin kernel); el fallback de fichero del CLI es relativo al **cwd** (igual que el kernel), no a la raíz del repo — documentado en el código, no probado desde otro directorio.
- No verifiqué el comportamiento con `TYLLUAN_TOKEN` definido en el entorno real de la máquina (los tests lo inyectan); el proceso de José podría tenerlo vacío/con espacios, ya cubierto por `trim` y descarte de vacíos.
- Gates: solo los fallos conocidos (artefacto WSL); recuento de tests ya en verde. Sin commit ni push.
