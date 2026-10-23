# D-34 — Las guilds dejan de recibir el token de operador (implementación)

- **Agente:** Deep (OpenCode)
- **Fecha:** 2026-10-23
- **Base:** `origin/main` = `85861c6` (`git fetch` + `checkout -f --detach`)
- **Diseño previo:** `docs/internal/DESIGN_D-34_credencial_guilds_deep_2026-10.md`
  (aprobado por el TL en los turnos 1588/1591, opción A)
- **Estado:** implementado y verificado localmente; diff **sin commitear**.
  Parche: `E:\tylluan-patches\deep\D-34.patch`.

## Impact

**Ruta crítica de autenticación.** Antes de este cambio, `GuildProcess::spawn`
inyectaba `TYLLUAN_TOKEN` —el token maestro, el mismo que acepta el kernel— en
el entorno de **cada** guild stdio, y además el hijo heredaba **todo** el
entorno del kernel (medido en `IMPL_T-DSH-38`). Una guild comprometida (una
dependencia Python maliciosa, un bypass de la allowlist de `bash`) llamaba como
**operador** a administración, mantenimiento, configuración, dispatch y
aprobaciones, y podía leer claves de API y tokens de servicios del operador.

Ahora:

1. cada guild recibe `TYLLUAN_GUILD_TOKEN`, una credencial **efímera, por
   proceso, en memoria** (nunca a disco, nunca a logs);
2. esa credencial autentica como `AclAuthKind::GuildToken`, que el gate de
   operador ya rechazaba (solo `MasterToken`/`DevBypass` pasan) — salvo **una**
   excepción auditada: `POST /api/v1/silva/edge`;
3. el hijo nace con `env_clear()` + lista blanca fail-closed por nombre, y solo
   recibe las credenciales del **servicio de terceros** que su propia guild usa;
4. el token maestro y `.tylluan-token` **no se generan, no se rotan y no se
   tocan** (regla de José); un cliente con el Bearer maestro funciona igual
   (verificado).

## Qué cambia, fichero a fichero

### `crates/tylluan-kernel/src/security/guild_credential.rs` (nuevo)

Almacén de proceso (`OnceLock<RwLock<HashMap<token, guild>>>`):

- `issue(guild)` — 32 bytes aleatorios (`rand`), prefijo `tyg_`, base64url; una
  credencial por spawn y por guild;
- `authenticate(token)` — comparación en tiempo constante
  (`ExecutionGuard::secure_compare`); cadena vacía nunca autentica;
- `revoke(token)` / `revoke_guild(guild)` — el `kill()` de la guild revoca su
  credencial; `active_count()` para diagnóstico;
- `is_guild_token()` — solo para dar un mensaje claro, **nunca** para autorizar.

### `crates/tylluan-kernel/src/transport/http/auth.rs`

- `AclAuthKind::GuildToken` (variante nueva).
- `bearer_auth_middleware`: la credencial se resuelve **solo** de la cabecera
  `Authorization` (nunca del query-string), antes de OAuth, y se comprueba
  contra el almacén cuando el token no es el maestro.
- Rol/`agent_id`: `acl.default_role` (idéntico a lo que la guild tenía con el
  maestro, para no romper el ACL por guild) y `agent_id = "guild:<nombre>"`, que
  da trazabilidad y rate-limit por guild.
- `operator_gate_allows` **sin cambios** → la clase operador sigue cerrada.
- `guild_operator_exception(method, path)` — la única excepción, pura y testeada:
  solo `POST` + `/api/v1/silva/edge` exacta (`silva/edge/search` y
  `silva/consolidate` siguen cerradas). Lleva documentada su retirada.
- `operator_gate_middleware`: aplica la excepción solo a `GuildToken`.

### `crates/tylluan-kernel/src/transport/http/api_v1/mod.rs`

`silva_add_edge_handler`: si el llamante es `GuildToken`, escribe una fila en
`guild_audit_log` (`audit_guild_edge_exception`) con **quién** (guild que emitió
la credencial), **qué** (`source -[predicate]-> target`, cada parte truncada a 64
caracteres) y si aterrizó. Es `spawn_blocking` y no propaga errores: un fallo del
audit no convierte una escritura correcta en 500.

### `crates/tylluan-kernel/src/registry/guild_process.rs`

- Fuera la inyección de `TYLLUAN_TOKEN`.
- `command.env_clear()` + lista blanca (`guild_inherited_env`): sistema
  (`PATH`, `PATHEXT`, `SYSTEMROOT`, `COMSPEC`, `TEMP`…), locale, `PYTHON*`,
  `SSL_*`, Docker, proxies y la configuración benigna que las guilds leen
  (`KERNEL_BASE`, `SCHEDULER_DB`, `COMFY_BASE_URL`, `N8N_BASE_URL`,
  `OPENAI_BASE_URL`, `OLLAMA_HOST`…). **Fail-closed**: lo que no está en la
  lista no llega, así un secreto nuevo queda excluido por defecto.
- Lo que necesita el launcher (`PYTHONPATH`, `PYTHONUNBUFFERED`, `TYLLUAN_ROOT`)
  se acumula en `launcher_env` y se aplica **después** del `env_clear`; la rama
  Docker recibe además `-e TYLLUAN_ROOT=/workspace`.
- Credenciales de servicio **por guild** (`guild_may_inherit`): `llama_backend`
  recibe `*_API_KEY`/`*_API_TOKEN` (su `api_key_env` lo define el operador);
  `n8n_bridge` → `N8N_API_KEY`; `claude_watcher` → `ANTHROPIC_API_KEY`;
  `antigravity_watcher` → `GEMINI_API_KEY`. Sin esto, esas cuatro guilds
  (incluido el juez externo) dejaban de funcionar en silencio.
- `kill()` revoca la credencial de la guild.

### `guilds/core/kernel_auth.py`

Prioridad: `TYLLUAN_GUILD_TOKEN` → `TYLLUAN_TOKEN` → `.tylluan-token`. Con la
variable de guild presente **no hay fallback** (si cayera al maestro, el split
no serviría de nada). Documentado en el docstring.

### Tests

- `crates/tylluan-kernel/tests/guild_env_audit.rs` (T-DSH-38): se quita el
  `#[ignore = "D-34"]` y el test que fijaba el comportamiento actual se
  **invierte** (ahora afirma: sin token maestro, sin secretos del padre, con
  `TYLLUAN_GUILD_TOKEN` de prefijo `tyg_`).
  Nota: **el TL esperaba 5 `#[ignore]` de aceptación y solo existía 1** (el
  `desired_*` de este fichero); los otros 5 criterios se cubren con tests
  nuevos vivos, no des-ignorados.
- `crates/tylluan-kernel/tests/cola_d34_acceptance.rs` (nuevo, 5 tests vivos):
  - `d34_guild_credential_reaches_silva_node` — 2xx en clase agente;
  - `d34_guild_credential_is_403_on_every_operator_route` — barrido de **22**
    superficies de operador (config, admin, maintenance, bash, lifecycle de
    guilds, dispatch, aprobaciones, quarantine, borrado/consolidación Silva,
    `decide/shadow`, `silva/conflicted`, mcp/external, federación, doctor,
    register);
  - `d34_master_token_still_passes_operator_gate` — el Bearer maestro sigue
    200 en `config` y `silva/conflicted`;
  - `d34_unknown_guild_token_is_401`;
  - `d34_guild_edge_exception_is_audited` — la excepción pasa **y** deja fila
    con guild, `agent_id`, intención y preview truncado.
- `tests/python/test_kernel_auth_guild_token.py` (nuevo, 6 tests): la credencial
  de guild gana y **no** cae al maestro ni al fichero; el maestro sigue valiendo
  para arranques manuales; el fichero es el último recurso; valores en blanco se
  ignoran.

## Verificación

| criterio | resultado |
|---|---|
| `cargo clippy -p tylluan-kernel --lib -j 3 -- -D warnings` | exit 0 (1m23s) |
| `cargo clippy -p tylluan-kernel --all-targets -j 3 -- -D warnings` | exit 0 (1m43s) |
| `cargo test -p tylluan-kernel --lib` ×2 (secuencial) | **1195 passed** (49,70 s) y **1195 passed** (39,99 s); 4 ignorados |
| `--test cola_d34_acceptance` | **5 passed** |
| `--test guild_env_audit` | **2 passed** (el deseado ya no está ignorado) |
| `--test oauth_operator_gate` | **6 passed** (el gate no se rompió) |
| `--test guilds_v2_integration` | **3 passed** |
| `--test capabilities_enforce_matrix` | **2 passed** |
| `--test exec_like_tool_names_all_guilds` (DSH) | **30 passed** |
| `python -m pytest tests/python/test_kernel_auth_guild_token.py` | **6 passed** |
| `git worktree add --detach E:\tylluan-patch-wt34 origin/main` + `git apply --check` / `git apply` | limpio, aplica sin conflictos (11 ficheros) |
| `CARGO_TARGET_DIR=... cargo check -p tylluan-kernel --tests` en ese worktree | **Finished** `dev` (7m48s) |
| `git diff --stat` | 10 ficheros + este informe, **+966/−67** |

Los 4 lib tests nuevos respecto a `a85f4d7` son el unitario del almacén, el de la
excepción única, y los dos que el barrido/rol añaden en `auth.rs`.

## Pending / decisiones que NO he tomado

- **DECISIÓN PENDIENTE (producto):** el rol ACL de una credencial de guild es
  `acl.default_role` (hoy `"admin"`), igual que tenía con el maestro. La
  reducción de privilegio está en el **gate de operador**, no en el ACL por
  guild. Endurecerlo (`guild_role`, p. ej. `"agent"`) requiere decidir qué pasa
  con `acl_can_access` para roles sin entrada en `acl.roles`; no lo cambio sin
  criterio porque puede romper llamadas legítimas de guilds.
- **DECISIÓN PENDIENTE:** `mcp_bridge.py:104` sigue pasando `{**os.environ}` a
  los subprocesos stdio que lanza (`enforce_allowlist` decide entre entorno
  completo y mínimo). Es la misma clase de hallazgo que D-34 pero **dentro** de
  una guild; fuera de alcance aquí.
- **DECISIÓN PENDIENTE:** `llama_backend` recibe cualquier `*_API_KEY` porque su
  `api_key_env` es configurable por el operador. La solución limpia es que las
  claves de proveedor vengan de config por proveedor, no del entorno.

## No verificado

- **Guilds reales en producción**: el `env_clear` se ha medido con una guild de
  prueba mínima (`guild_env_audit`) y con los tests de integración, no
  arrancando la flota real desde el kernel de `:47004` (no ejecuto procesos).
  En particular, no he visto funcionar en vivo `n8n_bridge`, `claude_watcher`,
  `antigravity_watcher` ni el juez contra un proveedor externo con API key.
- **Rama Docker** (`docker run`): no ejecutada (motor apagado); se deduce de los
  `-e` y de la lista blanca, no medida.
- **Producción del audit**: la fila se comprueba en un `TYLLUAN_AUDIT_DB`
  temporal; no he mirado `data/audit.db` real.
- **Patrón 404-antes-del-gate**: una ruta que no existe bajo un prefijo de
  operador (p. ej. `/api/v1/admin/loquesea`) devuelve **404 sin auth** porque el
  middleware cuelga de las rutas registradas y el fallback responde antes. No es
  una fuga (no hay handler) pero conviene saberlo al leer el barrido: los 22
  casos del test son rutas **reales**.
- **Hosts/multi-tenant**: el almacén es de proceso; con varios kernels en el
  mismo host cada uno emite sus propias credenciales (correcto por diseño, no
  probado).
