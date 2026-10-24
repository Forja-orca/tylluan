# T-DSH-69 — Rutas HTTP/MCP que construyen paths o lanzan procesos desde entrada externa (solo lectura)

- **Agente:** dsh
- **Fecha:** 2026-10-24
- **Base:** `origin/main` `97f89a4` (reset de worktree antes de empezar). **Sin cambios de código.**
- **Encargo (T1712):** solo lectura; rutas HTTP/MCP que construyen paths de fichero o lanzan procesos (`Command::new`) a partir de entrada externa; tabla ≤10 filas con fichero y línea, cómo se alcanza y arreglo mínimo; `## Impact` y «No verificado». Formato de cita: **«línea N»**, nunca `:N`.
- **Método:** barrido de `Command::new(` en `crates/` y de operaciones de fichero/path en `transport/http` y `transport/server`; lectura del contexto de cada candidato y de la clase de auth en `transport/http/auth.rs` (`operator_class_required`, `is_public_bypass_path`). Evidencias: `E:\tylluan-targets\dsh-t7-data\t69`.

## Tabla (10 filas)

| # | Fichero y línea | Cómo se alcanza | Arreglo mínimo |
|---|---|---|---|
| 1 | `crates/tylluan-kernel/src/transport/http/api_v1/api_mcp.rs`, línea 107 (registro) y 112 (`ensure_running`, que spawnea ya); el spawn es `crates/tylluan-kernel/src/registry/guild_process.rs`, líneas 648-656 | **POST `/api/v1/mcp/external`** con `command`, `args`, `cwd` y `env` en el JSON (auth **operator-class**). El kernel hace `register_external(command, args, cwd, env)` y arranca el proceso; en Windows `Command::new("cmd").arg("/c").arg(command).args(args)` con `current_dir(cwd)` del request | No envolver en `cmd /c`: ejecutar binario directo (o exigir ruta absoluta a `.exe`/`.cmd` conocida), allowlist de ejecutables, `cwd` confinado al workspace y `env` limitado a claves permitidas (PATH/`*_PATH` permiten cargar DLLs/binarios) |
| 2 | `crates/tylluan-kernel/src/transport/http/api_v1/api_mcp.rs`, líneas 213-236 y 270-271 | **PUT `/api/v1/mcp/external/{name}`**: `command`/`args`/`cwd`/`env` del body actualizan el server y se re-registran (y se persisten en config); mismo spawn que la fila 1 | Igual que la fila 1 + auditar el cambio de `command` (quién lo cambió) y exigir confirmación explícita |
| 3 | `crates/tylluan-kernel/src/transport/http/api_v1/api_admin.rs`, líneas 727-732 | **POST `/api/v1/config`** (operator-class): el body se valida solo como TOML y se escribe **tal cual** en `tylluan.toml`; puede meter `[[external_mcp]].command`, `[services].command` o `[wake].command` → proceso arbitrario en el siguiente arranque/ejecución | Allowlist de claves escribibles por API (o quitar el guardado crudo); rechazar secciones que contengan `command`/`args` desde HTTP |
| 4 | `crates/tylluan-kernel/src/registry/service_manager.rs`, líneas 58-85 | Servicios auxiliares: `Command::new(config.command)` + `args` + `env` desde la config; se lanzan al arrancar/health. Alcanzable por la cadena de la fila 3 (o editando `tylluan.toml`) | Validar `services.*.command` contra allowlist al cargar config; rechazar metacaracteres de shell; no heredar `env` arbitrario |
| 5 | `crates/tylluan-kernel/src/security/dispatch_executor.rs`, líneas 103-118 | `Command::new(program)` con argv de `[wake].command` (config), tras aprobación humana BWC-2. Alcanzable por la cadena de la fila 3 + aprobación | Validar `[wake].command` en carga de config (allowlist); ya es bueno que no haya shell ni interpolación (comentario en la línea 115) |
| 6 | `crates/tylluan-kernel/src/transport/server/handler_do/prepare.rs`, líneas 123-153 (`extract_path_from_intent` en `transport/server/utils.rs`, líneas 10-51) | **`tylluan_do` por MCP y POST `/api/v1/do`** (agent-class): el intent libre se parsea y se inyectan `path`, `file_path`, `repo_path` **crudos** en los args de la guild (acepta `/etc/...`, `C:\...`, `../...`, `~/...`); solo `directory` y `cwd` se limitan a `"."` si el path no es directorio (líneas 135-140) | Canonicalizar y exigir `starts_with(workspace_root)` antes de inyectar cualquier campo de path; si no está contenido, no inyectarlo (o inyectar `"."`) |
| 7 | `crates/tylluan-kernel/src/transport/http/api_v1/api_guilds.rs`, líneas 50-82 | **POST `/api/v1/guilds/{guild}/call/{tool}`** (agent-class, no operator): pasa `Json(args)` crudo a la guild (`CallToolRequestParam`, línea 81). El ACL solo se aplica si `acl.roles` **no** está vacío (líneas 55-66): con roles vacío (default) cualquier caller autenticado puede llamar a `bash`/`filesystem` con args arbitrarios | ACL fail-closed (exigir rol si el guild es sensible) y aplicar la misma contención de path de la fila 6 a `filesystem`/`bash` |
| 8 | `crates/tylluan-kernel/src/transport/http/api_v1/api_ingest.rs`, líneas 183-191 | **GET `/api/v1/ingest/files/{filename}`**: filtra `".."`, `/` y `\` pero **no** prefijos de unidad/ADS de Windows (`C:algo`); hace `Path::new("data/ingest").join(filename)` sin `canonicalize` ni `starts_with`, y en Windows un nombre con prefijo de unidad **sustituye** la base → lectura fuera de `data/ingest` | Igual que `sandbox_files_handler` (`api_ops.rs`, líneas 191-210): `canonicalize` + `starts_with(dir)`; rechazar `:` y prefijos de unidad |
| 9 | `crates/tylluan-kernel/src/transport/http/api_v1/api_mcp.rs`, líneas 311-353 | **POST `/api/v1/mcp/external/discover`** (operator-class): lee configs de terceros (Claude/Gemini/otros) y **importa `command`, `args` y `env`** de esos ficheros al registry de MCP externos → se spawnean como procesos; un fichero de perfil manipulado se convierte en ejecución de proceso | Tratar el descubrimiento como importación no confiable: mostrar diff y exigir aprobación humana antes de registrar/spawnear; no importar `env`/`cwd` |
| 10 | `crates/tylluan-kernel/src/transport/http/api_v1/api_ops.rs`, líneas 37-57 | **POST `/api/v1/bash`** (solo `dev_mode`, ruta operator-class): `req.command` se envía tal cual a la guild `bash` (`bash_execute`); ejecución de comandos desde HTTP | Eliminar la ruta (ya marcada deprecada) o exigir HITL/confirmación explícita y ACL de operador también en `dev_mode` |

## Revisado y correcto (no son filas)

- `sandbox_files_handler` (`transport/http/api_v1/api_ops.rs`, líneas 191-210): canonicaliza y exige `starts_with(workspace_root)` → buena referencia para las filas 7 y 8.
- `validate_skill_name` (`api_admin.rs`, líneas 1494-1508): rechaza `.`, separadores y caracteres reservados antes de `join` del nombre de skill (D-37).
- Subida de ingesta (`api_ingest.rs`, líneas 136-139): sanea el basename a `[A-Za-z0-9_-]` y prefija `upload_<ts>_` dentro de `data/ingest` (el `extension` se usa tal cual, pero no puede contener separadores tras parsear el nombre).
- `docker_containers_handler` (`api_ops.rs`, líneas 66-105): argv fijo (`docker version ...`), sin entrada externa.
- `tunnel/mod.rs`: `netsh`/`ipconfig` con argv fijo.
- Guilds internas (`registry/guild_process.rs`, líneas 600-665): `python`/`module_path` vienen del registry escaneado y del catálogo, no de la request (el canal externo es el `External` de las filas 1-2).

## Impact

- **Dos cadenas completas de ejecución de procesos desde HTTP**: el alta/edición de MCP externos (filas 1-2) permite un `cmd /c <command>` con `args`/`cwd`/`env` del body, y el guardado crudo de config (fila 3) permite sembrar `[services]`/`[[external_mcp]]`/`[wake].command` que se ejecutan en el siguiente arranque (filas 4-5). Ambas están detrás de auth operator-class, pero son entrada externa → proceso sin allowlist ni confinamiento de `cwd`.
- **Rutas MCP/HTTP normales llevan paths fuera del workspace a las guilds**: el intent de `tylluan_do` (fila 6) y la llamada directa a guild (fila 7) permiten pasar `/etc/...`, `C:\...` o `../` a `filesystem`/`bash`; la fila 7 además tiene el ACL efectivamente abierto cuando `acl.roles` está vacío (default).
- **Hueco concreto de path traversal en Windows** (fila 8): el endpoint de ficheros de ingesta bloquea `..`, `/` y `\` pero no prefijos de unidad, donde `join` reemplaza la base.
- El resto de rutas con parámetros revisadas (`sandbox/files`, skills, ingesta de subida, docker, netsh) no construyen paths/argv desde la request o lo hacen con contención/validación.

## No verificado

- **Análisis estático**: no ejercité las rutas en vivo (ni con token de operador ni de agente); la columna «cómo se alcanza» se deduce de `routes.rs` y de `auth.rs` (operator-class vs resto).
- **ACL con roles vacío (fila 7)**: deduzco que `acl.roles` vacío salta la comprobación porque el bloque está condicionado a `!acl.roles.is_empty()`; no lo probé contra un kernel con config por defecto, ni confirmé si el rol se rellena siempre en producción.
- **Fila 1-2**: no verifiqué en vivo que `ensure_running` spawnee el `External` exactamente por `guild_process.rs` 648-656 (sí lo muestra el código de `GuildLauncher::External`); tampoco probé el comportamiento de `cmd /c` con `cwd` arbitrario.
- **Fila 8**: no probé la semántica de `Path::join` con `C:foo` en Windows en vivo; la conclusión es de lectura de la librería estándar.
- **No auditadas las guilds Python** (bash/filesystem son quienes finalmente abren ficheros/ejecutan): el análisis se queda en el borde Rust kernel → args de guild.
- **No verificado** si otros agentes/roles pueden llamar a `/api/v1/mcp/external` con un token de agente (el código dice operator-class; no probé la matriz de tokens).
- Sin cambios de código ni kernels; solo `grep`/lectura sobre el worktree en `97f89a4`.
