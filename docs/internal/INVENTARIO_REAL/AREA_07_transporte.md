# AREA 07 — Transporte (MCP, REST v1, A2A)

**Autora:** Buffy · **Base:** `a564880` · **Fecha:** 2026-10-28 · **Modo:** lectura pura (no se ejecutó el kernel)

Alcance: `crates/tylluan-kernel/src/transport/**` — 68 ficheros, 36 374 líneas. Lo que se inventaría
aquí es **la superficie** (qué se anuncia, qué responde, qué lo protege), no la lógica de cada handler.

---

## 1. Superficie MCP

| Pieza | Qué se anuncia | Evidencia |
|---|---|---|
| Tools soberanas | **6** exactas: `tylluan_do`, `tylluan_remember`, `tylluan_recall`, `tylluan_think`, `tylluan_graph`, `tylluan_decide` | `server/tools.rs:49` (`sovereign_tool_names`), filtro en `server/tools.rs:16-30` |
| Tools definidas | **35** (`kernel_tools`) | `server/tools.rs:153` |
| Tools expuestas por defecto | **6** — las otras 29 solo con `expose_guild_tools = true` (por defecto `false`) | `config.rs:663`, `main.rs:1371`, `server/tools.rs:18-21` |
| Modo expandido | 35 kernel + todas las de guild (`registry.all_tools()`), deduplicadas por nombre | `server/tools.rs:24-43` |
| Recursos | `tylluan://skills` (texto fijo) + el recurso de MCP Apps del grafo | `api_v1/mcp.rs:778-793` |
| Prompts | 2: `tylluan_capabilities`, `tylluan_engineering_constitution` | `api_v1/mcp.rs:690-712` |
| Versiones de protocolo | 4: `2026-07-28`, `2025-06-18`, `2025-03-26`, `2024-11-05` | `api_v1/mcp.rs:33` |

Métodos JSON-RPC implementados (`api_v1/mcp.rs`): `initialize` (415), `ping` (522), `notifications/initialized`,
`tools/list` (534), `tools/call` (573), `prompts/list` (690), `prompts/get` (713), `resources/list` (778),
`resources/read` (797), `server/discover` (858), `tasks/get` (881), `tasks/update` (914), `tasks/cancel` (950).

No implementados: `resources/templates/list`, `completion/complete`, `logging/setLevel`, `tasks/result`, `tasks/list`
(`grep 'tasks/result|tasks/list'` en `transport/` → sin resultados).

## 2. Superficie REST

| Métrica | Valor |
|---|---|
| Registros `.route()` vivos | **249** — `api_v1/routes.rs` 230 + `http/sse.rs` 3 + `http/mod.rs` 16 |
| Paths distintos en `routes.rs` | 248 |
| Registros muertos | **2** — `a2a_routes()` (`http/a2a.rs:773`) no se llama desde ningún sitio |
| Rutas mutantes (POST/PUT/DELETE/PATCH/any) | ≥110 por el parser del test guardián (`http/auth.rs:1230`) |

Superficie **pública** (sin bearer), `http/mod.rs:1364-1386`:

| Ruta | Nota |
|---|---|
| `/health`, `/discovery` | `/discovery` devuelve solo 6 tools + endpoints, sin datos de sesión (`http/mod.rs:1696`) |
| `/api/v1/mcp/probe` | eco de `User-Agent` + dialecto detectado (`api_v1/mod.rs:1560`) |
| `/.well-known/agent-card.json` | tarjeta A2A |
| `/.well-known/oauth-authorization-server`, `/.well-known/oauth-protected-resource`, `/oauth/authorize`, `/oauth/token`, `/oauth/revoke` | flujo OAuth F2 |
| `/`, `/index.html` | SPA (`http/mod.rs:1419-1420`, fuera del middleware) |
| `/metrics` | **solo** con la feature `observability`, que no está en la release |

Todo lo demás (API v1 + SSE + MCP + A2A) va tras dos capas: `operator_gate_middleware` dentro de
`bearer_auth_middleware` (`http/mod.rs:1397-1405`).

## 3. A2A

| Método | Evidencia |
|---|---|
| `message/send` | `http/a2a.rs:388` |
| `message/stream` | `http/a2a.rs:389` |
| `tasks/get` | `http/a2a.rs:392` |
| `tasks/cancel` | `http/a2a.rs:393` |
| Tarjeta en `/.well-known/agent-card.json` | `http/a2a.rs` + `http/mod.rs:1382` |

## 4. Grupos de alias (varias rutas, un solo handler)

| Handler | Rutas | Nº |
|---|---|---|
| `silva_graph_handler` | `/api/v1/silva/graph`, `/api/v1/graph/viz`, `/memory/graph` | 3 |
| `guild_tool_call_handler` | `/api/v1/guilds/{guild_name}/tools/{tool_name}`, `/api/v1/guilds/{guild_name}/call/{tool_name}` | 2 |
| `list_sessions_handler` | `/api/v1/sessions`, `/api/v1/system/sessions` | 2 |
| `docker_containers_handler` | `/api/v1/docker/ps`, `/api/v1/docker/containers` | 2 |
| `golden_signals_handler` | `/api/v1/system/signals`, `/api/v1/health/golden-signals` | 2 |
| `maintenance_status_handler` | `/api/v1/maintenance/status`, `/api/v1/system/maintenance/status` | 2 |

Total: **13 rutas para 6 handlers**.

Líneas exactas (`api_v1/routes.rs`): 32 ↔ 139 (guild call), 39 ↔ 57 ↔ 140 (grafo), 50 ↔ 54 (sesiones),
64 ↔ 65 (docker), 112 ↔ 135 (golden signals), 114 ↔ 115 (maintenance).

---

## 5. Lo que no cuadra (con evidencia)

### T-03 · El catálogo de capacidades anuncia un recurso y un prompt que no existen
`/api/v1/capabilities` publica `mcp.resources = [{ uri: "tylluan://metadata/guilds" }]`
(`api_v1/mod.rs:1137`) y `mcp.prompts = [tylluan_guilds_catalog]` (`api_v1/mod.rs:1130`). Pero:

- `resources/list` no incluye `tylluan://metadata/guilds` (`api_v1/mcp.rs:778-793`, solo skills + grafo).
- `resources/read` con esa URI cae en `else if uri != "tylluan://skills"` y responde
  `-32602 unknown resource uri` (`api_v1/mcp.rs:823-824`, error en la 824).
- `prompts/get` con `tylluan_guilds_catalog` cae en `else if prompt_name != "tylluan_capabilities"`
  y responde `unknown prompt` (`api_v1/mcp.rs:727`).

Un agente que lea `/api/v1/capabilities` y pida lo que ahí se ofrece recibe error. Misma forma que el
fallo de Project Skills (K-02): lo anunciado no se puede leer.

### T-06 · La capa de 100 MB cae sobre 41 rutas, no solo sobre las de ingest
En `api_v1/routes.rs:63` hay `.layer(DefaultBodyLimit::max(100 * 1024 * 1024))` con el comentario
«100MB body limit for multipart uploads». En axum, `Router::layer` se aplica a **las rutas ya
registradas**, no a las siguientes:

- Antes de la capa: **41 rutas**, entre ellas `/api/v1/do`, `/api/v1/decide`, `/api/v1/guilds/*`,
  `/api/v1/silva/*` y `/api/v1/sessions/*`.
- Después: **189 rutas** que conservan el límite por defecto de axum (2 MB).

O sea: el límite de 100 MB acaba donde no se quería (incluida la puerta principal `tylluan_do`) y falta
en `/api/v1/ingest` (registrada después, línea 171), que es justo upload de ingest documentado como R15.
`/api/v1/ingest/upload` (línea 60) sí lo tiene, porque se registró antes de la capa.

### T-04 · `a2a_routes()` es un registro duplicado y muerto
`http/a2a.rs:773` construye un `Router` con `/.well-known/agent-card.json` y `/a2a`. No lo llama nadie
(`grep a2a_routes` → solo la definición). `build_router` registra esas dos rutas a mano
(`http/mod.rs:1382` y línea 1394). Si alguien añade una ruta en `a2a_routes()` no surte efecto.

### T-07 · `tasks/update` sí y `tasks/result` no
Se anuncia `"tasks": {}` en `capabilities` (`api_v1/mcp.rs:869`) y hay `tasks/get`, `tasks/update`,
`tasks/cancel`, pero **no** `tasks/result` ni `tasks/list`. `tasks/update` además permite que el propio
cliente mueva el estado de la tarea; está protegido con `validate_task_status_transition`
(`api_v1/mcp.rs:914-948`). Si `tasks/update` es una extensión propia o un método del spec no lo he
verificado contra el spec publicado: queda en `SIN_VERIFICAR`.

### T-09 · Cuatro superficies de descubrimiento de herramientas
`/api/v1/tools` (`api_v1/mod.rs:1093`), `/api/v1/tools/explore` (`:482`), `/api/v1/capabilities` (línea 1098)
y `/api/v1/capability-registry` (`api_guilds.rs:200`). Cada una con una forma de respuesta distinta y
ninguna es la fuente de las demás; la única consistente con `all_tools()` es `capabilities`.

---

## 6. Lo que sí está bien y conviene no romper

- **T-01** Las 6 soberanas se filtran en un único sitio (`all_tools`) y tienen dos tests: conteo exacto
  (`tests/sovereign_tools_e2e.rs:431`) y nombres exactos (`:55`).
- **T-02** Hay un test guardián que **parsea `routes.rs`** y falla si una ruta mutante no está clasificada
  como operador o como agente/par (`http/auth.rs:1230`, con cordura: ≥110 mutantes y ≥50 de operador).
  Es el gate que impide añadir una ruta de escritura sin decidir quién la puede llamar.
- **T-08** El token por query-string sobrevive solo para SSE, queda marcado como `MasterTokenQuery` y
  **no** satisface la clase operador (`http/auth.rs:337-338`, `:547-549`); hay saneado de query en logs
  (`http/auth.rs:803-805`).
- **T-05** Los alias están todos documentados como compatibilidad y comparten handler, así que no
  divergen en comportamiento; el coste es superficie de auditoría, no duplicación de lógica.

## 7. Autoauditoría (5 filas al azar, comandos re-ejecutados)

| Fila | Comando | Resultado |
|---|---|---|
| T-01 |  `grep -c` de `TylluanTool::new(` en `server/tools.rs` | 35 definidas; 6 en `sovereign_tool_names` |
| T-03 | `grep -n 'metadata/guilds\|guilds_catalog' api_v1/mod.rs` | 1130 y 1137 (los dos anuncios) |
| T-06 | script sobre `routes.rs` contando `.route(` antes/después de la línea 63 | 41 / 189 |
| T-04 | `grep -rn a2a_routes .` | 1 sola aparición (la definición) |
| T-08 | `grep -n MasterTokenQuery http/auth.rs` | 62-66 (variante), 549 (asignación), 337-338 (no operador) |

## No verificado

- **Nada se ha ejecutado en vivo.** No he arrancado kernel ni he hecho una petición HTTP real: todos los
  códigos de estado citados salen de leer el handler, no de una respuesta. En particular no he comprobado
  que `/api/v1/capabilities` devuelva exactamente ese JSON ni que `resources/read` dé `-32602` en vivo.
- No he medido el efecto real del límite de 2 MB vs 100 MB: no he subido un cuerpo grande a `/api/v1/ingest`
  (la fila T-06 afirma el reparto de capas según la semántica de axum, no una subida real).
- No he verificado `tasks/update` contra el spec MCP publicado (necesita fuente externa).
- No he revisado la lógica interna de los 249 handlers, solo su registro, su clase de auth y su forma de
  anuncio. Un handler puede estar registrado y devolver 501 sin que este mapa lo vea
  (`grep todo!()/unimplemented!/NOT_IMPLEMENTED` en `transport/` → 0 resultados, que es la única señal).
- No he tocado el dashboard: qué rutas consume el panel es materia del área 13.
