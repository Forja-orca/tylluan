# T-DSH-115 — llama-server del USUARIO opcional: apagado por defecto, modelo a elección, API/CLI, aviso de update y juez de gramática efímero

- **Agente:** dsh
- **Fecha:** 2026-10-28
- **Base:** `origin/main = 399db1f` (push177: arreglo del 9000 + T-DSH-114 + colas Deep/Buffy). Diff sin commitear.
- **Encargo (T1939):** (1) apagado por defecto: ningún código interno arranca el llama-server del usuario (guild `llama_backend`, antes puerto 9000); solo orden explícita; (2) modelo a elección: ruta GGUF o carpeta con modo router (`--models-dir/--models-max`, llama-server b10158), config `[inference.llama]`; (3) endpoints REST + intents de LECTURA por MCP (estado, arrancar, parar, listar modelos); arrancar/parar/cambiar modelo = clase operador, por MCP solo leer; (4) botón conectar/parar en dashboard (coordinado con Antigravity: dsh da la API); (5) autoactualización con aviso, sin cambios silenciosos; instalar solo con orden del usuario; (6) juez de gramática de write_gate y centinela en servidores efímeros aislados (patrón del centinela), sin depender del servidor del usuario. Criterios: clippy 0; lib sin fallos; contracts verde; tests por punto; sin `:NNNN`; prueba real: Tylluan arranca y escribe/recuerda con el servidor del 9000 apagado y sin red.

## (1) Apagado por defecto — ningún código interno lo arranca

- **Guild (`guilds/core/llama_backend.py`):** `query_model` ya **no** llama a `_start_llama_server()`; si el servidor local no está cargado devuelve error explícito: *"llama_backend no esta arrancado. Arrancalo explicitamente (tylluan_do 'llama start', POST /api/v1/inference/llama/start o el boton del dashboard). El servidor es del usuario y no se autoarranca."* El único arranque es la herramienta `start_server` (orden explícita).
- **write_gate:** la capa 2 ya no toca el 9000 (ver punto 6); si su juez efímero no está configurado, se omite (fail-open) y la capa 1 determinista sigue actuando.
- Verificado en vivo: `POST /api/v1/guilds/llama_backend/tools/query_model` sin arrancar → error explícito y **cero procesos `llama-server`** hijos (verificado por WMI filtrando el puerto de config).

## (2) Modelo a elección — `[inference.llama]`

Nuevas claves (config Rust `InferenceLlamaConfig` + lectura Python):
- `model_path: Option<PathBuf>` — GGUF explícito; **nunca se descarga nada**; si la ruta no existe, error claro.
- `models_dir: Option<PathBuf>` — modo **router**: el servidor arranca con `--models-dir` y `--models-max`.
- `models_max: usize` (default 4).
- `llama_server_path` (Python): ruta explícita al binario `llama-server` (evita búsquedas/descargas implícitas).

`tylluan.example.toml` documenta las claves y que el servidor está **apagado por defecto**.

## (3) REST + MCP

- **REST** (`api_v1/api_inference.rs`, rutas en `routes.rs`):
  - `GET /api/v1/inference/llama/status` — lectura real (guild + `server_status`): `running`, `guild_running`, `port`, `mode` (`single|router`), `model_path`, `models_dir`, `models_max`, `models`.
  - `GET /api/v1/inference/llama/models` — modelos disponibles (`.gguf` de `models_dir` o el `model_path`).
  - `POST /api/v1/inference/llama/start` — orden explícita (clase operador); **verifica de verdad** que el servidor queda escuchando (si no, 500 con estado, no un 200 falso).
  - `POST /api/v1/inference/llama/stop` — para el servidor del usuario; idempotente si no corre.
  - Auth: `start`/`stop` añadidos a `operator_class_required` (misma clase que config/centinela); `status`/`models` son lecturas.
- **MCP (solo lectura)** (`handler_do/llama_cmd.rs`, intents deterministas antes del routing): `llama status`, `llama estado`, `llama models`, `llama modelos`, `estado llama`, `modelos llama`, `llama_backend status`. **No** hay arrancar/parar por MCP (clase operador, igual que el ajuste de T-DSH-114). Intents ajenos caen al routing normal (test de prefijos).

## (4) Dashboard — API entregada

La API está lista para que Antigravity pinte el botón conectar/parar (y estado/modelos). Contrato: `GET /api/v1/inference/llama/status` para el estado; `POST /api/v1/inference/llama/start|stop` para el botón. No se toca dashboard en este diff (Antigravity es dueño de la UI en el reparto nuevo).

## (5) Autoactualización con aviso, sin cambios silenciosos

- `tylluan update` **ya no instala por defecto**: sin `--install` solo comprueba e imprime aviso (*"Aviso: hay una version nueva. NO se instala nada sin orden."* + cómo instalar). `--check` sigue siendo solo consulta. Instalar requiere `tylluan update --install` (orden explícita). Helper puro `update_action(check, install)` + test (`update_without_order_only_notifies`).

## (6) Juez de gramática de write_gate en servidor efímero aislado

- Nuevo módulo `security/grammar_judge.rs`: `llama-server` **propio** con el patrón del centinela (CPU, hilos ≤ 4, prioridad baja, Job Object, descarga por inactividad, gate de CPU, fail-open). Config `[security.write_gate_judge]` (mismas claves que `[security.sentinel]`); apagado por defecto (sin `model_path` no arranca nada y la capa 2 se omite).
- `SentinelJudge::raw_completion(prompt, grammar)` reutiliza la cola serie/servidor efímero del centinela y acepta gramática GBNF en la petición.
- `write_gate` ya no llama a `coherence_gate::call_reasoning_backend_with_grammar` (ruta HTTP al 9000, ahora legacy con `#[allow(dead_code)]`); usa `grammar_judge::judge_grammar(content)`.
- Tests: `default_config_is_not_configured`, `judge_returns_error_when_not_configured` (fail-open sin arrancar nada).

## Verificación

```
cargo clippy -p tylluan-kernel --all-targets -- -D warnings → 0
cargo clippy -p tylluan-cli --all-targets -- -D warnings    → 0
cargo test -p tylluan-kernel --lib                          → 1344 passed; 0 failed; 4 ignored (37,84 s)
cargo test -p tylluan-kernel --test contracts_test          → 5 passed
cargo test -p tylluan-cli                                   → 34 passed
python scripts/check_async_guild_io.py                      → exit 0 (73 ficheros)
pytest tests/python/test_llama_backend_*.py (3 ficheros)    → 14 passed
git diff --stat → 12 ficheros +251/−67 (+ 3 ficheros nuevos: grammar_judge.rs, api_inference.rs, llama_cmd.rs)
```

### Verificación en vivo (kernel test-run, puerto/DATA_DIR propios, BM25-only, solo PID exacto)

Config del test-run con `[inference.llama] model_path` a un GGUF real, `llama_server_path` explícito y puerto de servidor propio (no 9000):

```
GET  /api/v1/inference/llama/status   → 200 running=false, guild_running=false, port=<puerto>
POST .../query_model (sin arrancar)   → error explícito; hijos llama-server = []
GET  /api/v1/inference/llama/models   → [GGUF configurado]
POST /api/v1/do {"intent":"llama status"}  → 200, is_error=false, estado JSON
POST /api/v1/do {"intent":"llama models"}  → 200, is_error=false, modelos
POST /api/v1/inference/llama/start    → 200 running=true; hijo llama-server real [PID] en <puerto>
POST /api/v1/inference/llama/stop     → 200 running=false; hijos = [] (apagado)
POST /api/v1/silva/node               → 201 (nodo escrito con el servidor del usuario APAGADO)
GET  /api/v1/silva/recent             → devuelve el nodo escrito
```

## Impact

- El llama-server del usuario deja de ser un proceso que Tylluan arranca solo: **apagado por defecto**, arranque/parada solo por orden explícita (REST/CLI/dashboard) y el kernel sigue operando (memoria/escritura) sin él.
- Modelo configurable por ruta o carpeta (router), sin descargas silenciosas.
- El juez de gramática de write_gate deja de depender del servidor del usuario: corre en servidor efímero propio, aislado y fail-open.
- `tylluan update` deja de instalar sin permiso: avisa y exige `--install`.

## No verificado

- **Prueba "sin red"**: el kernel arrancó BM25-only y escribió/recordó sin llama-server, pero no se cortó la red de la máquina; no se probó explícitamente sin conectividad.
- **`query_model` con un servidor externo ya escuchando en 9000**: si existe un servidor OpenAI-compatible ajeno, la ruta legacy `_get_backend_url` puede responder desde él (no lo arranca Tylluan, pero no se aisló); no se probó el rechazo explícito de backends externos.
- **Modo router** (`models_dir`/`models_max`) no se probó en vivo con una carpeta de varios GGUF; solo se validó la ruta de modelo único y el parseo/estado.
- **Juez de gramática efímero** no se probó en vivo con modelo real (solo tests de configuración/fail-open); no se midió latencia ni descarga por inactividad.
- **Botón del dashboard**: solo se entrega la API; no hay cambio de UI en este diff (Antigravity).
- **CLI**: `tylluan update --install` no se probó contra una release real (solo la lógica/aviso con test); `--check` tampoco en vivo.
- **Python**: ruff no está instalado en el venv local (no se pudo correr); pytest de los 3 ficheros de llama_backend sí (14 passed). CI correrá ruff.
- **Guild**: el config de la guild se resuelve relativo a su propio fichero (comportamiento preexistente); en un `test-run-*` con copia de `guilds/` hay que copiar el árbol completo (no solo `llama_backend.py`) y el TOML no debe llevar BOM (encontrado en vivo: BOM → tomllib falla → la guild cae a defaults puerto 9000). Documentado aquí, no corregido en el producto.
- **No se tocó producción** (`:47004`), ni el 9000 ajeno (PID 55076, LM Studio), ni `E:\tylluan`.
