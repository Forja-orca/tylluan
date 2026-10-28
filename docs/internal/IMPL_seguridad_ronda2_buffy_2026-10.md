# IMPL Seguridad ronda 2 (B-1 a B-4) — Buffy

Fecha: 2026-10-28 · Agente: Buffy · Base: `d4f2537` (HEAD == `origin/main`) · Worktree: `E:\tylluan-wt-buffy`

Fuente de la tarea: `docs/internal/COLA_buffy_2026-10-28_ronda2.md` (CLAUDE, turno 2033 de Coloquio).
Alcance: **solo informe**. No hay cambios en código de producción; sí tres sondas reproducibles y su evidencia literal.

## 0. Método (idéntico para B-1, B-2 y B-3)

| Elemento | Valor |
|---|---|
| Binario | `E:\cargo-target\prod-build\release\tylluan-nexus.exe` (33.796.608 B) |
| Versión / commit servido | `0.17.2` / **`d4f2537`** (confirmado en `/health`) |
| Puerto | **47093** (el de producción, 47004, intacto todo el tiempo) |
| `dev_mode` | **`false`** |
| `DATA_DIR` | `test-run-buffy-r2/data` (propio, vacío al empezar) |
| cwd | `test-run-buffy-r2` (nunca `E:\tylluan`) |
| Credenciales | token maestro (`TYLLUAN_TOKEN`, 41 chars) + un segundo token listado en `security.acl.tokens` (31 chars) |
| Estado final | kernel **detenido**, puerto 47093 libre, `netstat` sin LISTENING |

Sondas dejadas en `test-run-buffy-r2/` (sin secretos: el maestro se lee de `.tylluan-token`, que está en `.gitignore`):
`probe_b1.py`, `probe_b2.py`, `probe_b3.py`, `creds.py`, `b3_matrix.json`, `kernel.log`.

---

## B-1 — LEAD-01: el BWC no liga el `agent_id` del payload al bearer

### Veredicto: **CONFIRMADO**

Config de la tanda: `token_agent_bindings = { "<token maestro>" = "agentA" }`, es decir el único bearer que autentica está ligado a `agentA`.

| Petición | Estado | Efecto en el contrato |
|---|---|---|
| `POST /api/v1/work-contracts` (maestro) | **201** | creado `bwc-b22db57a…`, equipo `[agentA, agentB]`, consolidador `agentA` |
| `POST /api/v1/work-contracts/{id}/deliver` · payload `agent_id="agentB"` · bearer ligado a `agentA` | **200** | `{"status":"delivered","deliveries":1,"team_size":2}` — **aceptado** |
| `POST /api/v1/work-contracts/{id}/vote` · payload `agent_id="agentB"` · mismo bearer | **200** | voto registrado como `agentB` |
| `POST …/deliver` · payload `agentB` + cabecera `X-Agent-Id: agentA` | **200** | `deliveries: 2` |
| `GET /api/v1/work-contracts/{id}` | **200** | `deliveries[].agent_id = "agentB"` (×2), `votes[].agent_id = "agentB"` |
| `POST …/deliver` **sin bearer** (control) | **401** | `unauthorized` |

Lectura: el contrato queda **a nombre de quien el cliente diga en el JSON**, no de la identidad del token. La ligadura configurada (`agentA`) no se aplica en esta ruta.

### Por qué ocurre (lectura de código, coincide con la sonda)

`api_contracts.rs` toma `req.agent_id` y lo escribe tal cual en `ContractDelivery` / `ContractVote`; **ningún handler de ese fichero llama a `current_bound_agent_id()`**. La ligadura sí se aplica en otros sitios y eso acota el hallazgo:

| Fichero | ¿Consulta la ligadura? |
|---|---|
| `transport/http/a2a.rs` | sí |
| `transport/http/api_v1/api_memory.rs` | sí |
| `transport/http/api_v1/mcp.rs` | sí |
| `transport/http/api_v1/mod.rs` | sí |
| `transport/server/handlers.rs` | sí |
| **`transport/http/api_v1/api_contracts.rs`** | **no** |

El único control de identidad del contrato es `contract_close_handler` (`req.agent_id != entry.consolidator` → 403), que se satisface **enviando el nombre del consolidador en el JSON**. `tick`, `context/decision` y `context/pointer` tienen el mismo patrón.

### B-1b (hallazgo colateral, importante para la COLA): `security.acl.tokens` no autentica

La COLA pide «dos tokens de agente distintos». Probado: un token que **sí** está en `security.acl.tokens` (mapeado a `reader`) como bearer recibe **401** en cualquier ruta protegida:

```
GET /api/v1/work-contracts/active  con token-B (en acl.tokens) -> 401
GET /api/v1/work-contracts/active  con token maestro           -> 400 (falta channel_id: pasó auth)
```

`acl.tokens` es **solo un mapa token→rol**; no es una credencial. Las únicas credenciales que autentican son: el token maestro, un JWT OAuth (`scope=mcp`) y la credencial de guild. Consecuencia práctica: no existe hoy forma de dar a dos agentes identidades separadas por la vía de `acl.tokens`, y eso convierte a OAuth en la única segunda identidad real — que es justo el sujeto de B-2.

---

## B-2 — LEAD-02: bearer OAuth sin mapeo hereda `default_role`

### Veredicto: **CONFIRMADO**

Flujo real ejecutado, no simulado: PKCE S256 → `GET /oauth/authorize` (P1, 200 HTML con `consent_id`/`csrf`) → `POST /oauth/authorize` con `decision=approve` y la credencial de operador (el token maestro) → **307** con `?code=…` → `POST /oauth/token` → JWT con `{"scope":"mcp","sub":"mcp"}`.
Dato operativo: el consentimiento **se recuerda** en `DATA_DIR`; la segunda tanda fue `GET /oauth/authorize → 307` con code directo, cero acciones humanas (comportamiento documentado del automáta).

Guild objetivo: `filesystem`, que **no** está en la lista del rol `reader` del config de prueba.

| `default_role` | credencial | `POST /api/v1/guilds/filesystem/call/list` | ¿ejecutó el handler? |
|---|---|---|---|
| `admin` | token maestro | **200** `Unknown tool: list` | sí (el ACL dejó pasar; falló el tool, no el auth) |
| `admin` | **JWT OAuth sin mapeo** | **200** `Unknown tool: list` | **sí → hereda admin** |
| `admin` | JWT + `X-Agent-Id: agente-inexistente` | 200 | sí |
| `reader` | token maestro | **403** `Role 'reader' does not have access to guild 'filesystem'` | no |
| `reader` | **JWT OAuth sin mapeo** | **403** (idéntico) | no |
| `reader` | JWT → guild `core` | 403 | no |

Lectura: el rol efectivo de un JWT OAuth **no depende del token**, depende exclusivamente de `default_role`. En producción `default_role = "admin"` y `roles.admin = ["*"]`, así que cualquier JWT emitido por el propio `/oauth/authorize` (scope `mcp`, sin mapeo en `acl.tokens`) tiene acceso a **todos** los guilds.

### B-2b: con `roles` vacío el gate de guild no se evalúa

`guild_tool_call_handler` solo aplica el ACL `if !acl.roles.is_empty()`. Un `tylluan.toml` sin `[security.acl.roles]` (que es el **default** del struct) deja el gate de guilds desactivado por completo, sea cual sea `default_role`. Producción sí tiene `roles`, así que hoy no aplica — pero es el estado al que se vuelve si alguien limpia esa tabla.

### Propuesta de valor mínimo seguro para `default_role`

`default_role = "guest"` con `roles.guest = []` (deny-all por defecto: `acl_can_access` devuelve `false` para roles no listados).

Coste que hay que aceptar explícitamente, medido en esta sonda: **el token maestro también hereda `default_role`** — con `default_role = "reader"` el propio maestro recibió 403 en `filesystem`. Para no dejar al operador sin acceso hay que mapear explícitamente las credenciales que deban ser `admin` en `security.acl.tokens` (incluido el maestro) o introducir una regla «clase operador ⇒ admin» en el resolvedor. Esa segunda opción es la que recomendaría discutir: hoy `AclAuthKind::MasterToken` no aporta ningún privilegio de rol.

---

## B-3 — Qué se rompe con `dev_mode = false` desde el dashboard

Método: extraídas las rutas que el dashboard pide de verdad (`client.fetch`/`bridge.fetch`/`fetch` en `dashboard/src` y `packages/tylluan-ui-core/src`), **87 rutas distintas**, sondeadas con y sin token contra el kernel de prueba.

| Resultado | Rutas |
|---|---|
| **401 sin token, 200 con token** | **66** (todas las de datos: `/api/v1/guilds*`, `/api/v1/silva*`, `/api/v1/memory*`, `/api/v1/coloquio*`, `/api/v1/repo-map`, `/api/v1/skills`, `/api/v1/security/*`, `/api/v1/jobs`, `/memory/graph`…) |
| 200 con y sin token (bypass público) | **2**: `/health`, `/api/v1/mcp/probe` |
| **Rechazadas aun con token válido** | **0** |
| 405 en GET (rutas de escritura) | 15 |
| 404 (ruta no registrada) | `/api/v1/sessions/{id}`, **`/api/v1/skills/{name}`** |

Columna «usa el adaptador autenticado»: **sí** en 84 de 87. El adaptador existe y funciona: `NexusBridge.fetch` (`packages/tylluan-ui-core/src/lib/api-client.ts`) pone `Authorization: Bearer` con el token de `localStorage['tylluan_token']`, y `dashboard/src/App.tsx` monta `AuthModal` para introducirlo.

**Las 3 llamadas crudas** (`fetch` sin el cliente), que son las únicas candidatas a romperse:

| ruta | fichero | con token | sin token | veredicto |
|---|---|---|---|---|
| `/api/v1/ingest/upload` | `VisionTab.tsx` | 405 (GET) / requiere POST | **401** | **roto: sube ficheros sin bearer** — es AG-13, ya asignado |
| `/api/v1/config/device` | `ModelsEmbeddingsDisk.tsx` (`bridge.fetch`) | 200 | 401 | falso positivo: sí usa el adaptador |
| `/api/v1/config/inference-llama` | `ModelsRoleAssignment.tsx` (`bridge.fetch`) | 200 | 401 | falso positivo: sí usa el adaptador |

Conclusión operativa: cerrar `dev_mode` **no exige tocar el dashboard** salvo `VisionTab`; exige que quien abra el panel introduzca el token una vez (`AuthModal`). El riesgo no es de código, es de despliegue: sin ese paso, las 66 rutas de datos devuelven 401 y el panel se ve vacío sin decir por qué.

Nota metodológica: al sondear 87 rutas seguidas el limitador por IP (300/min) devolvió **429** en las tres últimas; repetí esas tres tras 45 s y los estados de la tabla son los de la repetición, no los 429.

Nota lateral: `DELETE /api/v1/skills/{name}` sigue sin registrarse (404), coherente con M-BUFFY-14.

---

## B-4 — Ataques a `tylluan update` (auditoría, **no ejecutada**)

No pude ejecutarla y lo digo antes del resultado: el comando exige `api.github.com` (URL fija en el código, sin override), el `prod-build` no tiene `tylluan-cli.exe`, y `crates/tylluan-cli` no tiene target `lib`, así que `extract_update_binaries` no es alcanzable desde un test de integración sin añadir un `[lib]` al `Cargo.toml`. Lo de abajo es lectura del código de `d4f2537`, con la columna «obtenido» marcada como tal.

| Caso | Esperado | Obtenido (lectura) |
|---|---|---|
| Tar con miembro `../../evil.exe` | no escribir fuera del staging | **protegido**: el destino es `stage.join(file_name)` con lista blanca de dos nombres, la ruta del archivo nunca se usa como destino |
| `tylluan-nexus.exe` duplicado en el tar | rechazar | **protegido**: `bail!` ante nombre repetido |
| Falta uno de los dos binarios | rechazar | **protegido**: «Release archive is missing {name}» |
| Checksum alterado | no instalar | **protegido**: `verify_sha256` antes de extraer; el `checksums.txt` se descarga como objeto aparte del archivo (E-03) |
| Versión menor que la instalada | no degradar | **protegido**: `compare_versions` compara `latest` contra `current` y el brazo `Less` no instala |
| Sin `checksums.txt` en la release | no instalar | **protegido**: 4xx → `bail!` |
| **Miembro que es enlace simbólico llamado `tylluan-nexus.exe`** | rechazar | **hueco**: no se comprueba `entry.header().entry_type()`; `unpack` crearía el symlink y `install_staged_binaries` lo renombraría a destino (binario instalado = enlace a ruta arbitraria) |
| **Miembro que es directorio con ese nombre** | rechazar | **hueco** (misma raíz: no se valida el tipo) |
| **Gzip bomb** en el archivo | no agotar disco | **hueco**: sin cota de tamaño descomprimido |
| Binario cambiado de nombre (contenido de `tylluan-cli` en el miembro `tylluan-nexus`) | detectar | **no detectable** por diseño: el checksum es del publicador, y ambos artefactos vienen del mismo origen. Solo lo cerraría una firma (minisign/cosign), no un hash |

Gravedad realista de los tres huecos: el checksum se descarga por TLS del mismo `latest_tag` que el archivo, así que un atacante sin control de la release no puede llegar a ellos. Son defensa en profundidad, no una vulnerabilidad explotable hoy. El arreglo del tipo de entrada es una línea (`if !entry.header().entry_type().is_file() { continue; }`) y me parece barato para lo que compra.

Detalle de legibilidad (no es un defecto): `compare_versions(current, latest)` devuelve `latest.cmp(current)`, así que `Greater` significa «hay actualización». Funciona, pero el nombre del parámetro invita a leerlo al revés.

---

## Impact

- **Cero cambios en código de producción.** Este informe añade un fichero (`docs/internal/IMPL_seguridad_ronda2_buffy_2026-10.md`) y deja tres sondas reproducibles en `test-run-buffy-r2/`, que está **sin trackear** y cuyo `tylluan.toml` cae en la regla `.gitignore` existente.
- Sin secretos en lo añadido: el token maestro de prueba se lee de `.tylluan-token` (ignorado); borré el `.creds.json` que había usado al principio para no dejar un secreto en un fichero que git **no** ignora.
- No toca `transport/`, `guilds/`, `integrations/` ni ningún `tylluan*.toml` versionado.
- El kernel de producción (47004, commit `d4f2537`) no se ha tocado: todas las peticiones fueron a 47093 y ese proceso está detenido.
- Si se acepta B-2, el cambio en `tylluan.toml` **sí** exigiría sección `## Impact` propia y revisión del TL, porque deja al maestro sin rol admin por defecto.

## No verificado

- **No** he escrito el test de regresión de B-1. Propuesta concreta: un test de integración con el harness de `tests/support` que cree un contrato y afirme `403` al entregar con un `agent_id` distinto del ligado; hoy fallaría, así que iría `#[ignore]` como los de M-BUFFY-14. No lo he escrito por tiempo, no por falta de medio.
- **No** he probado B-4 en vivo (motivos arriba). Los «obtenido» de su tabla son lectura de código, no ejecución.
- **No** he probado el flujo OAuth con un cliente MCP real (Claude Desktop); usé PKCE a mano contra el kernel de prueba.
- **No** he medido cuántas claves hay en `token_agent_bindings` de producción (está vacío en lo que leí) ni si algún consumidor real usa BWC con varios agentes: la severidad práctica de B-1 depende de eso y no la he medido.
- **No** he comprobado el comportamiento de `default_role` en despliegues que ya tengan JWT emitidos: los tokens son stateless y su rol se resuelve por petición, así que un cambio de `default_role` los afecta en el siguiente request (deducido del código, no medido).
