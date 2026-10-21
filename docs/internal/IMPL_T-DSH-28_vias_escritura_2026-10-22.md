# T-DSH-28 — Inventario exhaustivo de vías de escritura (nodos, aristas, mensajes)

- **Agente:** dsh
- **Fecha:** 2026-10-22
- **Base:** worktree `E:\tylluan-wt-dsh` en `--detach origin/main` (`e8d7b9b`); limpio antes de empezar.
- **Encargo (turno 1535):** solo lectura. Inventario EXHAUSTIVO de toda ruta HTTP y herramienta MCP que escriba nodos, aristas o mensajes (routes.rs y handlers: silva_*, memory, ingest, coloquio, graph, a2a, federation, mailbox, skills, jobs de fondo): tabla ruta × quién puede llamarla (credencial) × defensa de escritura × archivo:línea; buscar una séptima vía que Buffy no haya visto.

## Modelo de credenciales (de dónde salen las columnas)

| hecho | evidencia |
|---|---|
| Toda ruta `/api/` exige bearer **salvo 4 excepciones** con verificación interna: `/federation/sync/receive`, `/federation/sync/export`, `/federation/ping`, `/mcp/probe` | `auth.rs` (línea 220-227) |
| Clase **Operador** (solo token maestro por cabecera o dev-bypass local; un JWT OAuth NO vale): admin/*, maintenance/*, config, mcp/external, autoresearch, a2a/agents, dispatches/*, bash, doctor/repair, guild-start, setup-hint/apply, guilds/register|request|dispatch/execute|dispatch/remote, POST guilds start/stop/reset-backoff/test, peers/sharing mutaciones, sync/pull|both, POST inference/providers|security/scopes|skills, DELETE silva/nodes/*, memory/quarantine*, **POST silva/edge**, **POST silva/consolidate** | `auth.rs` (línea 253-329) |
| **Todo lo demás es clase agente**: cualquier bearer válido (token maestro u OAuth JWT con scopes) — incluye `silva/node`, `memory/write`, `ingest`, `coloquio/*`, `tylluan_graph`, `mailbox/send`, `nodes/*`, `canvas/*`, `routing/anchors`, `session-digest`, `blackboard/*`, `work-contracts/*`, `journal/*`, `stigmergy/*` | `auth.rs` (línea 253-329) + `routes.rs` |
| `dev_mode=true` desactiva la autenticación (bypass local de desarrollo); aun así la clase Operador tiene su propia tabla | `auth.rs` (línea 27-36, 331+) |
| **Las guilds reciben `TYLLUAN_TOKEN` (token operador del kernel) inyectado en el spawn** → cualquier guild puede llamar como operador a TODAS las rutas anteriores (hallazgo T-DSH-26) | `guild_process.rs` (línea 478-491) |

Defensas hoy (3 piezas): **capa 1** `matches_injection_pattern` (10 substrings ingleses; `poison_patterns.rs`, línea 9-20), **juez capa 2** fire-and-forget con cuarentena (`write_gate.rs`), y **marca** `untrusted_content`/`untrusted_reason` (`trust.rs`). Solo tres escrituras usan capa 1; el juez solo corre en tres.

## A. Escrituras DEFENDIDAS (capa 1 + juez)

| ruta / tool | escribe | credencial | defensa | evidencia |
|---|---|---|---|---|
| `POST /api/v1/memory/write` | nodo `memory_write:*` | agente | capa 1 hard-reject 403 + juez post-escritura (cuarentena) | `api_memory.rs` (línea 20-24, 55-63) |
| MCP `tylluan_remember` | nodo `memory:*` / `agent_memory` | agente | capa 1 hard-reject + juez | `handler_remember.rs` (línea 63, 100+) |
| `POST /api/v1/ingest` | nodos de documento/chunk + aristas | agente | capa 1 **marca** `untrusted_content` (no rechaza: un documento legítimo puede hablar de inyección) + juez | `api_ingest.rs` (línea 396-405, 429) |
| `POST /api/v1/ingest/upload` | fichero en `data/ingest` → pipeline de ingesta (indirecta) | agente | la escritura de nodo ocurre en `/api/v1/ingest` (defendida); el fichero se sanea en el nombre | `api_ingest.rs` (línea 104-150) |

## B. Escrituras con SOLO marca (no capa 1 / no juez) — contenido externo marcado, no filtrado

| ruta / tool | escribe | credencial | defensa | evidencia |
|---|---|---|---|---|
| `POST /api/v1/federation/sync/receive` | nodos ingeridos de peers | **peer** (bypass bearer, token de peer propio) | marca `federation_peer:<name>`; sin capa 1 ni juez | `api_federation.rs` (línea 149) |
| `POST /a2a` (`message/send`) | nodo `a2a_result` | agente | marca fija `untrusted_content:true` + `a2a external agent:<id>` | `a2a.rs` (línea 544-545) |
| `POST /api/v1/mailbox/send` | mensaje del buzón | agente | marca **solo si el remitente no tiene identidad local**; cuerpo sin capa 1/juez | `mod.rs` (línea 152-185) |
| `POST /api/v1/session-digest` | nodo `agent_summary` (vía `AgentMemoryManager`) | agente | marca heredada si alguna memoria consolidada estaba marcada; si no, sin defensa | `mod.rs` (línea 1452-1467), `agent_memory.rs` (línea 380-395) |
| consolidación de `agent_memory` (fondo) | resumen de memorias | interno | marca heredada (`inherited: ...`) | `agent_memory.rs` (línea 380-395, 465-480) |

## C. Escrituras de NODOS sin defensa (candidatas a D-32)

| ruta / tool | escribe | credencial | defensa | evidencia |
|---|---|---|---|---|
| `POST /api/v1/silva/node` | nodo con `content` y `metadata` arbitrarios (+embedding) | agente | **ninguna** (ni capa 1, ni juez, ni marca) | `mod.rs` (línea 808); `routes.rs` (línea 38) |
| **`POST /api/v1/silva/save-cluster-summary`** | nodo `graphrag_summary:<cluster>` con `summary` arbitrario | **agente** (NO es clase operador) | **ninguna** | `mod.rs` (línea 953-966) → `graph_rag.rs` (línea 150) |
| `POST /api/v1/coloquio/channels/{id}/post` | mensaje del canal | agente | **ninguna** | `api_coloquio.rs` (línea 232-258) |
| `POST /api/v1/coloquio/documents` (+`/append`, `PUT`) | nodo `document` con texto del doc | agente | **ninguna** | `api_coloquio.rs` (línea 409) |
| `POST /api/v1/tylluan_graph` y MCP `tylluan_graph(add_triple)` | nodos `concept` (subject/object como contenido) + arista | agente | **ninguna** | `handler_graph.rs` (línea 30-32); ruta `routes.rs` (línea 168) |
| `POST /api/v1/silva/edge` | arista con `metadata` | **operador** | **ninguna** | `mod.rs` (línea 724-730) |
| `POST /api/v1/silva/consolidate` | consolida/reescribe nodos (fondo) | **operador** | **ninguna** | `mod.rs` (línea 926) |
| `POST /api/v1/canvas/{channel}/nodes` | nodo `canvas_state` con labels/estado serializado por el cliente | agente | **ninguna** | `api_canvas.rs` (línea 262-296, upsert 293) |
| `POST /api/v1/routing/anchors` | filas de anclas (contenido = `intent` del cliente + embedding) | agente | **ninguna** | `api_federation.rs` (línea 926-953) |
| `POST /api/v1/blackboard/tasks/{msg_id}/done` | nodo `episode` con el `result` del cliente | agente | **ninguna** | `mod.rs` (línea 307-330, upsert 320) |
| `POST /api/v1/work-contracts/{id}/close` (síntesis) | nodo `task_synthesis:<id>` con texto sintetizado del contrato | agente | **ninguna** | `api_contracts.rs` (línea 534) |
| `POST /api/v1/skills` (`@skill:save`) | **fichero** `./.agents/skills/{name}.md` con contenido arbitrario; `name` sin sanear (`../`) → posible escritura fuera del repo | **operador** | **ninguna** | `api_admin.rs` (línea 1494-1513) |
| MCP `tylluan_graph`/`@correct:<node>:<content>` | nodo corregido con contenido del intent | agente | **ninguna** | `handler_do/correct.rs` (línea 133) |
| MCP `tylluan_do` intent `record_experience` | memoria de experiencia (action/outcome/lesson) | agente | **ninguna** (agent_memory; sin capa1/juez) | `handler_do/mod.rs` (línea 463-471) |
| MCP `tylluan_do` intent `@skill:` | nodo `project_skill` con contenido | agente | **ninguna** | `handler_do/skill.rs` (línea 128) |
| MCP `tylluan_do` intent `register_identity` | nodo `agent_identity` (human_name/role/purpose) | agente | **ninguna** | `handler_do/routing.rs` (línea 264) |
| MCP **`tylluan_ingest`** (subtool documentado de `tylluan_do`) | chunks de `content` arbitrario **o de un fichero local arbitrario** (`ingest_type:"file"`, path del cliente) | agente | **ninguna** (no capa 1, no juez, no marca) | `handlers.rs` (línea 4-12, 95) → `handler_ingest.rs` (línea 20-99, upsert 79) |
| MCP `tylluan_do` intents `correct`/lessons | nodos `lesson`/episodio | agente | **ninguna** | `handler_do/mod.rs` (línea 1398, 1432, 1472) |
| grafo de aristas desde salida de guild | aristas extraídas de la respuesta de una guild | agente→guild | **ninguna** | `handler_do/routing.rs` (línea 355) |

## D. Escrituras de MENSAJES (buzón, nodos-agente, journal) sin defensa de contenido

| ruta / tool | escribe | credencial | defensa | evidencia |
|---|---|---|---|---|
| `POST /api/v1/blackboard/plan` | tareas en el buzón (`send_task`) con texto del plan | agente | **ninguna** | `mod.rs` (línea 274-292, send_task 280) |
| `POST /api/v1/nodes/{agent_id}/send` y `/nodes/broadcast` | mensajes al router de nodos-agente (`payload` arbitrario) | agente | **ninguna** | `mod.rs` (línea 1541-1575) |
| `POST /api/v1/journal/{agent_id}/checkin` | tarea en el journal (texto libre) — **se devuelve al modelo en `whoami.last_task` sin banner** | agente | **ninguna** (canal de inyección a contexto vía whoami) | `api_journal.rs` (línea 281), `handlers.rs` (línea 525, 538) |
| `POST /api/v1/stigmergy/traces` | traza de trabajo (texto/uri) en DB | agente | **ninguna** | `routes.rs` (línea 296) |
| `POST /api/v1/coloquio/channels` (+`/typing`, docs) | canal/documentos | agente | **ninguna** | `api_coloquio.rs` (línea 167-220) |
| `POST /api/v1/mailbox/send` | mensaje (ver B) | agente | marca condicional; contenido sin filtrar | `mod.rs` (línea 152-185) |

## E. Herramientas MCP (endpoint `/mcp`, clase agente) y subtools

Los 6 soberanos: `tylluan_do`, `tylluan_remember`, `tylluan_recall`, `tylluan_think`, `tylluan_graph`, `tylluan_decide` (`handlers.rs`, línea 11-13). **Escriben**: `tylluan_remember` (defendida), `tylluan_graph` (add_triple: sin defensa), `tylluan_do` (subtooks con escritura: `tylluan_ingest`, `register_identity`, `record_experience`, `@skill:`, `@correct:`, `@coloquio:` → post de canal, `agent_send_mail` → buzón; todas sin capa 1/juez, algunas con marca condicional). `tylluan_decide/think/recall` no escriben nodos (recall escribe feedback de ranking en DB, no contenido).

## F. Jobs de fondo e indirectas

| escritor | escribe | defensa | evidencia |
|---|---|---|---|
| job `bg_guild_call` (fondo) | nodo `job_result` con la salida de la guild (texto generado) | **ninguna** | `background_jobs.rs` (línea 266, 338) |
| `POST /api/v1/silva/graphrag-trigger` (operador) y NightConsolidation | resúmenes GraphRAG (`save_summary`) | **ninguna** | `api_monitor.rs` (línea 738-763), `graph_rag.rs` (línea 150) |
| consolidación de memoria de agente | `agent_summary` | marca heredada | `agent_memory.rs` (línea 380-395) |
| autosync federado | nodos de peers | marca (B) | `api_federation.rs` (línea 149) |
| mailbox `knowledge_share` (broadcast desde `remember`) | mensajes broadcast | sin capa 1 (el contenido ya pasó por capa 1 en remember) | `handler_remember.rs` (broadcast ~línea 280) |
| guilds (con `TYLLUAN_TOKEN`) | **cualquier ruta de A-D vía HTTP como operador** | la de la ruta llamada | `guild_process.rs` (línea 478-491) |

## LA SÉPTIMA VÍA (y las siguientes)

Buffy contó 6 vías: `silva/node`, Coloquio, `tylluan_graph(add_triple)`, `@skill:save` + las 2 defendidas (`memory/write`, `ingest`). **La séptima que no aparece en esa lista es `POST /api/v1/silva/save-cluster-summary`**:

- Escribe un nodo `graphrag_summary:<cluster>` cuyo contenido es `summary`, un string **arbitrario del llamante** (`mod.rs`, línea 953-966 → `graph_rag.rs`, línea 150).
- **No es clase operador** (no está en la tabla de `auth.rs`, línea 253-329): cualquier agente con bearer puede llamarla, y también cualquier guild (tiene el token maestro inyectado).
- **No tiene ninguna defensa**: ni `matches_injection_pattern`, ni juez, ni marca; y el nodo resultante entra en el contexto de `tylluan_think`/recall como conocimiento de alta confianza (es un "resumen" del grafo).
- Es distinta de Coloquio y de `silva/node` (flujo y tipo de nodo propios) y no estaba en la enumeración de Buffy.

Además encontré **otras vías fuera de su lista**, por si el D-32 quiere cubrirlas también (ordenadas por riesgo de canal hacia el modelo):
1. **MCP `tylluan_ingest`** (no documentada como tool soberana pero es subtool de `tylluan_do`): chunks arbitrarios a `ingested:*` sin defensa; con `ingest_type:"file"` lee **cualquier fichero local** que le pase el cliente y lo persiste como memoria — exfiltración y envenenamiento en una sola tool.
2. **`POST /api/v1/canvas/{channel}/nodes`**: nodo `canvas_state` con texto del cliente.
3. **`POST /api/v1/routing/anchors`**: escribe anclas de enrutado con `intent` del cliente + embedding (puede sesgar el router).
4. **`POST /api/v1/journal/{agent_id}/checkin`**: tarea libre que `whoami` devuelve al modelo como `last_task` **sin banner** (canal directo a contexto).
5. **`POST /api/v1/blackboard/tasks/{id}/done` → `episode`** y **`POST /api/v1/work-contracts/{id}/close` → `task_synthesis`**.
6. **Jobs de fondo `job_result`** con salida de guild sin defensa.
7. **`@skill:save`**: además de contenido sin filtrar, `name` se usa en `join` sin sanear (`../`) → posible escritura de fichero fuera del repo (requiere operador, pero las guilds lo son).

## Impacto para D-32 (defensa de escritura)

- Hoy **solo 3 de ~25 vías de escritura tienen capa 1**, y el juez solo corre en esas 3. La marca `untrusted` cubre federación/A2A y (condicional) correo, pero **no** las vías de mayor superficie (silva/node, Coloquio, graph, summary, canvas, anchors, ingest MCP).
- La séptima (`silva/save-cluster-summary`) es la más engañosa: clase agente + nombre "de sistema" + entra como resumen de grafo, que es exactamente el material que un lector confía.
- El patrón común de D-32 debería ser la semántica de `ingest` (marcar, no rechazar, porque el contenido legítimo puede hablar de inyección) aplicada a **todas** las escrituras de nodo con texto libre, más juez común y visibilidad de errores (D-33).

## Impact

- **Solo lectura**: este informe es el único artefacto (1 fichero nuevo en `docs/internal/`, sin commitear). Cero cambios en `src/`, `guilds/`, tests o configuración; no se ejecutó ninguna ruta ni tool (no se escribió ni un nodo en ningún kernel real); no se tocó `E:\tylluan` ni el puerto 47004.
- Es el inventario que D-32 (Deep) necesita para no olvidar vías: 4 defendidas, 5 con solo marca, ~20 sin defensa, 6 jobs/indirectas, y la séptima vía argumentada con archivo:línea.

## No verificado

- **Dinámico**: no se llamó ninguna ruta ni tool; el inventario sale de leer `routes.rs`, `auth.rs`, handlers y `grep` de primitivas de escritura. No se comprobó en vivo qué nodos aparecen y cuáles no (no se arrancó kernel).
- **Juez capa 2**: `write_gate.rs` lo describe como fire-and-forget con cuarentena; con backends externos de varios modelos el aviso del turno 1535 dice que queda mudo (petición sin campo `model`, 400 tragado) — eso es D-33, **no** lo he re-medido aquí.
- **`tylluan_ingest` con `ingest_type:"file"`**: leí el comentario de la ruta `url` ("requires network guild") y la rama `file` lee `source` como path; no verifiqué ejecutándolo si hay normalización de path o límites.
- **`@skill:save` traversal**: el `join` con `name` no saneado es lectura de código; no se ejecutó una escritura fuera del repo (sería alterar el sistema).
- **Rutas que no escriben** (reads, config, admin) no están listadas; el barrido cubrió `routes.rs` completo (unas 200 rutas) y me quedé con las que escriben nodo/arista/mensaje o fichero con contenido.
- **`agent_memory.rs` (marca heredada)**: leí el bloque de consolidación y el de digest; no revisé todas las ramas de `remember_agent`/`create_session_digest` en detalle (puede haber sub-rutas sin marca).
- **`routes.rs` con `?`/query y streaming (SSE/ws/canvas)**: `canvas/ws` no escribe nodo por sí mismo (los nodos entran por `/canvas/{channel}/nodes`, cubierto); no audituré el protocolo WS mensaje a mensaje.
