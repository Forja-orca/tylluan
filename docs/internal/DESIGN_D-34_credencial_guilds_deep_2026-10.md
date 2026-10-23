# D-34 — Las guilds dejan de recibir el token de operador (DISEÑO)

- **Agente:** Deep (OpenCode)
- **Fecha:** 2026-10-23
- **Base:** `origin/main` = `a85f4d7`
- **Estado:** **solo diseño.** No he tocado código de autenticación todavía: la cola pide
  diseño primero y hay **una decisión** (§5) que no me corresponde tomar.

## Impact (previsto)

Hoy `registry/guild_process.rs` inyecta `TYLLUAN_TOKEN` (el token maestro) en el entorno
de **cada** guilda stdio. Cualquier guilda comprometida —una dependencia Python maliciosa,
un bypass de la allowlist de `bash`— puede llamar como **operador** a las rutas de
administración, mantenimiento, configuración, dispatch y aprobación. Un sandbox no
arregla eso: el token viaja dentro.

El diseño sustituye esa inyección por una **credencial de clase guild**, distinta del
token maestro, efímera (por arranque) y sin acceso a la clase operador. El Bearer maestro
sigue funcionando exactamente igual para el operador humano.

---

## 1. Lo que ya existe y se puede reutilizar (verificado en el código)

| Pieza | Dónde | Por qué sirve |
|---|---|---|
| `AclAuthKind` | `transport/http/auth.rs:57` | Ya distingue `MasterToken` / `MasterTokenQuery` / `OAuthJwt` / `DevBypass` / `PublicBypass`. Añadir `GuildToken` es una variante más, no un sistema paralelo. |
| `operator_class_required(method, path)` | `auth.rs:253` | Tabla **pura y testeada** de rutas de operador (`admin/*`, `maintenance/*`, `config`, `dispatches/*`, aprobaciones, `bash`, `silva/edge`, `silva/consolidate`, `memory/quarantine`, lifecycle de guilds…). |
| `operator_gate_allows(kind)` | `auth.rs:336` | Ya es fail-closed: **solo** `MasterToken` y `DevBypass`. Un `GuildToken` cae fuera sin tocar la tabla. |
| `guilds/core/kernel_auth.py` | guild-side | Punto único de resolución del Bearer para todas las guilds. Cambiar aquí cubre a todas. |
| `resolve_self_auth_token()` | `security/coherence_gate.rs` | Lo que hoy se inyecta como `TYLLUAN_TOKEN`. |

Consecuencia: la parte "403 en operador" **no necesita lista nueva**: el gate ya existe y
solo hay que no incluirlo en `operator_gate_allows`.

## 2. Diseño propuesto

### 2.1 Credencial

- Módulo nuevo `security/guild_credential.rs` (mismo patrón que `judge_telemetry`:
  `OnceLock` de proceso, sin persistencia):
  - `issue(guild_name) -> String` genera 32 bytes aleatorios (base64url), registra
    `token -> guild_name` y devuelve el token.
  - `authenticate(token) -> Option<String>` (nombre de la guilda).
  - `revoke(token)` para el apagado de una guilda.
- **Efímera:** se regenera en cada arranque del kernel. Reiniciar revoca todo. No se
  escribe en disco, no se loguea, no se serializa.
- Una credencial **por guilda** (no una compartida): permite atribuir el tráfico en logs
  (`agent_id = "guild:<nombre>"`) y revocar solo la que se apaga.

### 2.2 Inyección

`guild_process.rs`: en lugar de `command.env("TYLLUAN_TOKEN", master)`,
`command.env("TYLLUAN_GUILD_TOKEN", issue(&self.name))`. `TYLLUAN_TOKEN` deja de
inyectarse.

### 2.3 Autenticación y rol

- `bearer_auth_middleware`: tras comprobar maestro/query/OAuth, prueba
  `guild_credential::authenticate(bearer)`; si casa → `(true, AclAuthKind::GuildToken)`.
- Rol ACL: **`acl.default_role`** (hoy `"admin"`), con override opcional
  `[security.acl] guild_role` (`Option<String>`) para que el operador endurezca sin que
  yo adivine listas. `agent_id` = `guild:<nombre>` (trazabilidad y rate-limit por guilda).
- `operator_gate_allows` **no se toca**: un `GuildToken` recibe 403 en toda la clase
  operador. Ese es el criterio de aceptación (403 en operador, 2xx en `silva/node`).

### 2.4 Guild-side

`kernel_auth.resolve_kernel_token()`:

1. `TYLLUAN_GUILD_TOKEN` (kernel-spawned) → úsalo y **no caigas al maestro ni al fichero**.
2. si no existe: `TYLLUAN_TOKEN` (operador que arranca una guilda a mano) →
3. `.tylluan-token` (comportamiento actual, solo para arranque manual/dev).

Así una guilda lanzada por el kernel nunca resuelve el maestro.

### 2.5 Tests (los que pide la cola)

- Integración (`tests/d34_guild_credential.rs`): `test_state` + `issue("test-guild")`:
  - `POST /api/v1/silva/node` con la credencial → 2xx.
  - `POST /api/v1/admin/...` (o `maintenance/purge`, `GET /api/v1/config`) → **403**.
  - `POST /api/v1/silva/edge` → ver §5.
  - Bearer maestro → 2xx en la ruta de operador (no se rompe al operador).
  - Token inventado → 401.
- Unit: `operator_gate_allows(Some(GuildToken)) == false`; `authenticate` de un token
  revocado devuelve `None`; dos guilds no comparten token.
- Python (`tests/python/test_kernel_auth_guild_token.py`): con
  `TYLLUAN_GUILD_TOKEN` puesto, no se lee `.tylluan-token` aunque exista.
- Verificación completa: clippy `-D warnings`, lib ×2, `git apply` en worktree limpio +
  `cargo check --tests` aislado.

## 3. Qué guilds dejan de poder hacer (declaración explícita)

Inventario real de llamadas guilda→kernel (`rg '/api/v1/' guilds`):

```
agents/heartbeat, agents/session, agents/session/start,
coloquio/*, do, embed, events, graph/add,
guilds/llama_backend/tools/query_model,
memory/search, memory/write,
silva/context, silva/edge, silva/node, silva/recent,
work-contracts/*
```

De todas ellas, la clase operador solo alcanza **`POST /api/v1/silva/edge`**
(`silva_utils.write_edge`, que es la escritura de tripletas del grafo). Todo lo demás es
clase agente y sigue funcionando. No hay ninguna guilda llamando a `admin/*`,
`maintenance/*`, `config`, `dispatches/*`, aprobaciones ni `bash`.

## 4. Límite que este diseño NO cierra (importante)

`kernel_auth.py` cae al fichero `.tylluan-token` cuando no hay variable de entorno. Un
proceso Python comprometido **con los mismos permisos de usuario** puede leer ese fichero
igual que puede leer cualquier otro del workspace. D-34 elimina la **inyección** del
maestro (que es lo que pedía la cola) y hace que la credencial de guilda no sirva para
operar; **no** convierte al usuario en un sandbox. Cerrar eso exige aislamiento de SO
(usuario distinto, contenedor con el token fuera), fuera de alcance aquí.

## 5. Decisión que necesito (TL/José) — bloquea la implementación

`guilds/core/silva_utils.py:218` escribe tripletas con `POST /api/v1/silva/edge`, que la
tabla `operator_class_required` marca como **operador** ("un POST hostil deshace una
decisión de seguridad"). Un `GuildToken` recibirá 403 y `write_edge` dejará de funcionar
para todas las guilds de memoria. No existe ruta equivalente no-operador
(`/api/v1/graph/add` no está en el kernel: es superficie de la guilda de grafo).

Opciones, en mi orden de preferencia:

- **A (recomendada): credencial de guilda con UNA excepción auditada** —
  `POST /api/v1/silva/edge` permitida solo a `GuildToken`, declarada en la tabla y en el
  informe. Mantiene el gate intacto para el operador y para todo lo demás; el precio es
  que una guilda comprometida puede escribir aristas (que ya podía por `silva/node`).
- **B: cero excepciones** — la credencial de guilda no puede escribir aristas;
  `write_edge` deja de funcionar hasta migrarlo (trabajo aparte: ruta nueva o tool MCP).
  Más limpio de seguridad, regresión funcional real y silenciosa hoy.
- **C: no emitir credencial todavía** y limitarse a dejar de inyectar el maestro (las
  guilds quedarían 401 con `dev_mode=false`): no lo recomiendo, rompe la flota.

Mi preferencia es **A**, y no la implemento sin que lo confirméis porque debilita una
decisión de seguridad que alguien tomó a propósito.

## No verificado

- Todo: no hay código todavía. Las rutas citadas salen de `rg` sobre `guilds/**/*.py` y
  de la tabla `operator_class_required`, no de tráfico real capturado.
- No he comprobado qué guilds se lanzan con `OpenAI`-style HTTP externo ni si alguna
  arranca con un entorno custom que ya traiga `TYLLUAN_TOKEN` del operador.

---

## Actualización 2026-10-23 22:00 — DECIDIDO Y EJECUTADO: opción A

El TL confirmó la opción A (turnos 1588/1591, cola `85861c6`), con seis
condiciones que este diseño ya cumple o amplía:

1. La excepción es solo `POST /api/v1/silva/edge` y solo para
   `AclAuthKind::GuildToken` (`guild_operator_exception`). El resto de la clase
   operador sigue 403 para la guild (barrido vivo en
   `crates/tylluan-kernel/tests/cola_d34_acceptance.rs`).
2. Cada uso deja fila en `guild_audit_log` con la guild que emitió el token y el
   par subject/object truncado a 64 caracteres
   (`audit_guild_edge_exception` en `api_v1/mod.rs`).
3. Test vivo que barre 22 superficies de operador
   (`d34_guild_credential_is_403_on_every_operator_route`).
4. Credencial por proceso, en memoria, sin disco ni logs; se revoca al apagar la
   guild (`revoke_guild`).
5. `kernel_auth.py` cae al maestro/fichero **solo** si no existe
   `TYLLUAN_GUILD_TOKEN`; con la variable presente no hay fallback.
6. Se mantiene el `env_clear()` + lista blanca de la enmienda de T-DSH-38.

### Añadido respecto al diseño inicial: `env_clear`

Una guild stdio ya no hereda el entorno del kernel. La lista blanca es
fail-closed por nombre (sistema, locale, Python, Docker, proxies y la
configuración benigna que las guilds leen: `KERNEL_BASE`, `SCHEDULER_DB`,
`OPENAI_BASE_URL`, `OLLAMA_HOST`...). Las claves de servicio se entregan **por
guild** y solo a la guild que las necesita (`guild_may_inherit`):

| guild | credencial de servicio |
|---|---|
| `llama_backend` | cualquier `*_API_KEY` / `*_API_TOKEN` (su `api_key_env` lo define el operador) |
| `n8n_bridge` | `N8N_API_KEY` |
| `claude_watcher` | `ANTHROPIC_API_KEY` |
| `antigravity_watcher` | `GEMINI_API_KEY` |

Sin esto, cuatro guilds dejaban de funcionar en silencio (el juez externo, entre
ellas). No es autoridad sobre el kernel: es la credencial del servicio de
terceros que la propia guild usa.

### Ruta no-operador futura para escribir tripletas (retirada de la excepción)

La excepción existe solo porque `guilds/core/silva_utils.py::write_edge` usa
`POST /api/v1/silva/edge`, marcada como operador. Retirada, en este orden:

1. Exponer una ruta de **clase agente** para aristas, con la misma validación
   que `silva/node` (capa 1 + juez + procedencia) y sin borrar aristas: p. ej.
   `POST /api/v1/silva/edge` con `AclAuthKind::GuildToken` pasaría a una ruta
   nueva `/api/v1/graph/edge` clasificada como agente en
   `operator_class_required` (y en el test de inventario de rutas).
2. Migrar `write_edge` a esa ruta y verificar con una guild real.
3. Borrar `guild_operator_exception`, su uso en `operator_gate_middleware`, su
   test unitario y la entrada de `OPERATOR_SWEEP`; la excepción desaparece del
   audit como caso especial.

Mientras tanto, el coste declarado es: una guild comprometida puede escribir
aristas (ya podía escribir nodos por `silva/node`), y cada escritura queda
atribuida en el audit con nombre de guild.
