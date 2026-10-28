# IMPL M-BUFFY-14 — Project Skills: reproducción del desajuste de almacenes (E-08)

Fecha: 2026-10-28 · Agente: Buffy · Base: `a564880` (HEAD == origin/main) · Worktree: `E:\tylluan-wt-buffy`

**Encargo:** dejar demostrado con tests el hallazgo E-08 de Deep (guardar/listar/abrir/borrar
mezclan fichero y SilvaDB) **sin arreglarlo**. Este informe no cambia código de producción:
solo añade `crates/tylluan-kernel/tests/project_skills_store_mismatch.rs`.

## 1. Tabla `operación → almacén → evidencia`

| Operación | Punto de entrada real | Almacén | Evidencia |
|---|---|---|---|
| Listar (panel) | `getProjectSkills` → `GET /api/v1/skills` → `project_skills_list_handler` | **FICHERO** — `read_dir` de `./.agents/skills`, `./.tylluan/skills`, `./skills` | `dashboard/src/lib/api/memory.ts:179-183`; `crates/tylluan-kernel/src/transport/http/api_v1/api_admin.rs:1459-1485` (dirs en la 1461, `read_dir` en la 1463) |
| Listar (fallback del panel) | `@skill:list` → `handle_skill_prefix` | **SILVADB** — nodos `project_skill` filtrados por `project_root` | `memory.ts:185-190`; `crates/tylluan-kernel/src/transport/server/handler_do/skill.rs:31-49` |
| Guardar (panel) | `saveProjectSkill` → intent `@skill:save` | **SILVADB** — nodo `skill:project:<hash>:<name>` | `memory.ts:209-212`; `skill.rs:104-150` (`upsert_node` en la 137, `spawn_write_gate_judge` en la 139) |
| Guardar (HTTP) | `POST /api/v1/skills` → `project_skills_save_handler` | **FICHERO** — `./.agents/skills/<name>.md` | `api_admin.rs:1542-1570` (dir fijado en la 1551, `fs::write` en la 1538) |
| Abrir | `getProjectSkill` → intent `@skill:get` | **SILVADB** — `get_node("skill:project:<hash>:<name>")` | `memory.ts:202-207`; `skill.rs:78-102` (`get_node` en la 89) |
| Borrar | `deleteProjectSkill` → `DELETE /api/v1/skills/<name>` **y, al fallar, `@skill:delete`** | **SILVADB** | `memory.ts:214-231`; `skill.rs:55-76` (`delete_node` en la 66); ruta del `DELETE` **inexistente**: `api_v1/routes.rs:94` registra solo `get(...).post(...)` |
| Catálogo MCP | recurso `tylluan://skills` | **NINGUNO** — texto de ejemplo hardcodeado | `transport/http/api_v1/mcp.rs:823-852`; `transport/server/mod.rs:355-387` |

Lectura: **el panel guarda en SilvaDB y lista del fichero**. Ninguna de las dos superficies
sabe de la otra, y el `DELETE` que el panel ya invoca ni siquiera está registrado.

### El detalle que agrava el síntoma

`getProjectSkills` corta el fallback con `if (Array.isArray(res)) return res;`
(`memory.ts:182`). Un array **vacío** también es un array, así que cuando el directorio de
ficheros no existe —el caso normal, porque el panel nunca escribe ahí— la función devuelve
`[]` y **jamás llega** a `@skill:list`. Consecuencia concreta: guardas una habilidad desde el
panel (va a SilvaDB), refrescas y la lista sale vacía. La lista de Silva es inalcanzable.

Además `POST /api/v1/skills` no lo llama nadie del dashboard (`grep 'api/v1/skills' dashboard/src`:
solo `memory.ts:181` GET y `memory.ts:216` DELETE): el handler de guardado por fichero es
código sin consumidor desde la UI, y aun así es el que decide qué ve la UI.

## 2. Tests que lo demuestran

Fichero nuevo: `crates/tylluan-kernel/tests/project_skills_store_mismatch.rs` (4 tests, todos
`#[ignore = "M-BUFFY-14: ..."]` para no teñir la suite normal de rojo).

Cada test afirma el comportamiento **correcto** (lo que el usuario espera de un único listado),
y por eso falla hoy:

| Test | Afirma | Por qué falla hoy |
|---|---|---|
| `skill_saved_through_mcp_shows_up_in_the_http_list` | lo guardado por `@skill:save` (Silva) aparece en `GET /api/v1/skills` | el GET solo lee ficheros |
| `skill_saved_through_http_can_be_opened_by_get_intent` | lo guardado por `POST /api/v1/skills` (fichero) se abre por `@skill:get` (Silva) | no existe nodo de Silva |
| `deleting_a_listed_skill_removes_it_from_the_list` | tras el borrado del panel, la habilidad sale de la lista | borra un nodo inexistente; el `.md` sigue y sigue listado |
| `delete_route_for_a_single_skill_is_registered` | `DELETE /api/v1/skills/<name>` responde 2xx | la ruta no está registrada → 404 |

Notas de método:
- Los handlers HTTP resuelven `./.agents/skills` **relativo al cwd**, no del `HttpState`. El
  binario de test mueve su propio cwd a un temporal una sola vez (`OnceLock`), así que no
  escribe nada en el árbol del repo. Se comprobó: la prueba no deja ficheros en `crates/`.
- El id de Silva (`acc * 31 + byte` del cwd) y los metadatos (`project_root`) se duplican desde
  `skill.rs` a propósito: con otro id el test no probaría nada.

### Salida real

```
cargo test -p tylluan-kernel --test project_skills_store_mismatch -- --ignored --nocapture
```

```
running 4 tests

thread 'skill_saved_through_http_can_be_opened_by_get_intent' panicked at ...:
M-BUFFY-14: la habilidad 'mmb14_http_only' se guardó como fichero (.agents/skills/mmb14_http_only.md)
y se lista correctamente, pero al abrirla el panel pregunta a SilvaDB por el nodo
'skill:project:7b73758055b46e4d:mmb14_http_only', que no existe → "Skill 'mmb14_http_only' not found in this project."

thread 'deleting_a_listed_skill_removes_it_from_the_list' panicked at ...:
M-BUFFY-14: se pidió borrar 'mmb14_delete_me'. SilvaDB dijo deleted=false (no había nodo que borrar)
y el fichero .agents/skills/mmb14_delete_me.md sigue en disco, así que la habilidad sigue en la lista
= ["mmb14_delete_me", "mmb14_http_only"]

thread 'skill_saved_through_mcp_shows_up_in_the_http_list' panicked at ...:
M-BUFFY-14: `@skill:save` escribe en SilvaDB pero GET /api/v1/skills (api_admin.rs,
project_skills_list_handler) solo lee ficheros de ./.agents/skills, ./.tylluan/skills y ./skills.
Una habilidad guardada por el propio panel queda invisible: nombres en la lista = ["mmb14_delete_me", "mmb14_http_only"]

thread 'delete_route_for_a_single_skill_is_registered' panicked at ...:
M-BUFFY-14: el dashboard llama DELETE /api/v1/skills/mmb14_route_probe
(dashboard/src/lib/api/memory.ts, deleteProjectSkill) pero esa ruta no existe en api_v1_routes
(routes.rs registra get+post en /api/v1/skills). Respuesta real: 404 Not Found

test result: FAILED. 0 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.57s
[exit 101]
```

Cada uno falla por la razón esperada y **ninguno** por un fallo de montaje: los cuatro llegan a
la aserción final después de superar sus precondiciones (el `GET` responde 200, el `POST`
responde 200, el fichero se lista).

## 3. Fix propuesto (5 líneas, NO aplicado)

La fuente de verdad única debe ser **SilvaDB**: es lo que ya declara el doc-comment del
intent (`skill.rs:5-7`, «project-scoped reusable skill context in SilvaDB») y lo que el panel
ya usa para guardar.

1. `project_skills_list_handler` pasa a recibir `State` y a listar
   `get_nodes_by_types(["project_skill"])` filtrado por `project_root`, igual que `skill.rs:31-49`
   (se va el `read_dir`).
2. `project_skills_save_handler` hace `upsert_node` + `spawn_write_gate_judge`, igual que
   `skill.rs:137-143` (se va el `fs::write`, y con él la doble marca de capa 1 en dos sitios).
3. Registrar `DELETE /api/v1/skills/:name` → `delete_node`, borrando además el `.md` huérfano si
   existe (transición), en `routes.rs:94`.
4. Quitar el cortocircuito `if (Array.isArray(res)) return res;` de `memory.ts:182` para que haya
   **un** camino de listado, no dos.
5. Migración idempotente al arrancar: cada `*.md` de `.agents/skills` que no tenga nodo se
   inserta en Silva con el banner de capa 1 ya aplicado (nadie pierde habilidades existentes).

## 4. Hallazgo colateral (mismo panel, fuera de E-08)

`tylluan://skills` **no devuelve las habilidades del proyecto**: devuelve un catálogo de
intents de ejemplo escrito a mano (`mcp.rs:823-852` y otra copia literal en
`server/mod.rs:355-387`, texto duplicado en dos sitios). Un cliente MCP que lea ese recurso
esperando «skills del proyecto» recibe contenido fijo. Clasificación: MAL_PLANTEADO, gravedad
MEDIA, esfuerzo S (una fuente, una función). Va al inventario, no a este informe.

## Impact

- Solo se añade un fichero de test (`crates/tylluan-kernel/tests/project_skills_store_mismatch.rs`).
  **Cero** cambios en código de producción: el desajuste queda reproducido, no corregido, como
  pedía la cola.
- Los 4 tests están `#[ignore]`: la suite normal (`cargo test -p tylluan-kernel --lib`) no cambia
  de cifra y ningún binario de integración ajeno se ve afectado.
- No toca `transport/`, `guilds/`, `integrations/` ni `tylluan*.toml`, así que no hay sección
  `## Impact` de ADR-013 más allá de esta. El **fix** propuesto sí tocaría `transport/` y
  `dashboard/`: cuando se aplique, esa sección será obligatoria y deberá revisarlo el TL.
- Sin commit y sin push (solo el TL integra).

## No verificado

- **No** he reproducido el síntoma en el panel real con un kernel vivo y el dashboard arrancado
  (solo contra el router en proceso de test). El `404` del `DELETE` es del router, no de un
  kernel en el puerto real.
- **No** he comprobado qué hace un cliente MCP real con `tylluan://skills`; leí las dos copias
  del texto, no las consumí.
- **No** he medido cuántas habilidades reales hay hoy en `.agents/skills` ni en SilvaDB de
  producción (no toco la base de producción). La migración del punto 5 podría mover 0 o N.
- **No** sé si `POST /api/v1/skills` lo usa algún cliente externo fuera del dashboard: `rg` sobre
  `dashboard/` no lo encuentra, pero no he mirado clientes de terceros.
- El `hash` del cwd depende de la ruta absoluta: dos despliegues del mismo proyecto en rutas
  distintas ven bases de skills distintas. No lo he verificado en producción, solo deducido del
  código (`skill.rs:24-27`).
