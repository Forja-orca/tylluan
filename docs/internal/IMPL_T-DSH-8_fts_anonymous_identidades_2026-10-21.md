# T-DSH-8 — Desincronía FTS, atribución `anonymous` y caja partida de identidades

- **Agente:** dsh
- **Fecha:** 2026-10-21
- **Base:** `cd0cbb1` == origin/main. **Solo lectura**: copias frescas de `silva.db`, `audit.db` y `mailbox.db` a un directorio temporal propio, abiertas como copias; `data/` solo se leyó. Cero cambios de código: el único artefacto es este informe.
- **Copias:** silva 170,8 MB (10.931 nodos, 24.421 entradas de índice FTS), audit 6.159 filas, mailbox completa. `PRAGMA quick_check` = ok en las tres.

## (a) `nodes_fts_docsize` 24.421 vs 10.931 nodos: ¿bug o cuenta distinto?

**Las dos cosas.** La FTS5 cuenta de otra forma **y** hay un bug real de sincronización.

### Cuenta distinto (explica la mitad)

`nodes_fts` es una tabla FTS5 **external-content** sobre `nodes`:

```sql
CREATE VIRTUAL TABLE nodes_fts USING fts5(
    id UNINDEXED, content, metadata,
    content=nodes, content_rowid=rowid, tokenize='porter unicode61');
```

(`crates/tylluan-kernel/src/memory/silva/schema.rs` líneas 119-126.)

Consecuencias de esa forma:
- `SELECT COUNT(*) FROM nodes_fts` **lee la tabla de contenido** (`nodes`), no el índice: devuelve 10.931 siempre, aunque el índice tenga basura.
- `nodes_fts_docsize` es la tabla interna del índice (una fila por *docid indexado*): 24.421. La diferencia **no es solo semántica**, porque:

### Bug real: los borrados no limpian el índice

- **13.497 docids del índice no tienen nodo vivo** (55,3 % del índice): `SELECT COUNT(*) FROM nodes_fts_docsize d WHERE NOT EXISTS (SELECT 1 FROM nodes n WHERE n.rowid=d.id)`.
- **0 triggers** sobre `nodes`/`nodes_fts` (`SELECT name FROM sqlite_master WHERE type='trigger'` → vacío): la sincronización es 100 % manual en Rust.
- **7 nodos sin entrada en el índice** (dirección contraria): cuatro `session_digest:*` del 2026-07-25, dos `image` del 2026-07-29 y `file:nat.rs` del 2026-07-30.

**Causa (archivo:línea).** Varios caminos borran de `nodes` sin tocar la FTS:

| Camino | archivo:línea |
|---|---|
| limpieza automática por peso (rutina) | `memory/silva/maintenance.rs` líneas 419-425 |
| limpieza por antigüedad/purga | `memory/silva/maintenance.rs` líneas 463, 491, 711 |
| decay / purga de obsoletos | `memory/silva/decay.rs` líneas 205, 758, 801 |
| borrados por id/LIKE/metadata | `memory/silva/nodes.rs` líneas 900, 1075, 1376, 1469 |
| wipe administrativo | `transport/http/api_v1/api_admin.rs` línea 1610 |

Solo dos rutas sí limpian la FTS (borrando por rowid): `nodes.rs` líneas 1084 y 1130.

**Reproducción en copia** (todo verificado, no inferido):
1. `DELETE FROM nodes_fts WHERE rowid = ?` **sí funciona** (docsize 24421 → 24420). El problema no es la API, es que casi nadie la llama.
2. El comando canónico `INSERT INTO nodes_fts(nodes_fts, rowid, ...) VALUES('delete', ...)` también funciona.
3. El upsert de `insert_node` (`nodes.rs` líneas 249-255) **sí reindexa** en updates: tras simularlo, un token nuevo matchea y el `docsize` no crece. (Un `UPDATE nodes SET content` por SQL directo, sin pasar por la app, **no** reindexa: sin triggers, eso deja índice obsoleto — pero no es el camino normal.)
4. `nodes_fts_data` ocupa 23,6 MB; el índice crece con cada update hasta un `optimize`/`rebuild` (no hay mantenimiento periódico que lo compacte).

**Impacto.** La búsqueda BM25 **no** devuelve nodos borrados (el JOIN con `nodes` los filtra: `memory/silva/search.rs` líneas 918-919 y 928-929), así que no hay resultados fantasma. Lo que sí hay: 55 % del índice es basura (I/O, caché, memoria), las estadísticas de documento/IDF de bm25 están sesgadas por 13.497 docs inexistentes, y `/health` miente en `index_freshness.fts_rows` porque cuenta filas de la tabla de contenido.

**Recomendación.** Triggers FTS5 canónicos sobre `nodes` (AFTER INSERT → insert; AFTER DELETE → comando `'delete'` con OLD; AFTER UPDATE → `'delete'` OLD + insert NEW) **o**, si se prefiere mantener el sync en Rust, un `INSERT INTO nodes_fts(nodes_fts) VALUES('rebuild')` periódico en maintenance + un contador de desincronía (`nodes` vs `docsize`) en `/health` para que el drift sea visible en vez de silencioso.

## (b) De dónde sale `anonymous` en el handshake MCP

`agent_id` **no viene de la credencial**: viene del argumento del cliente, con default `"anonymous"`.

| Llamador | De dónde toma el `agent_id` | Si falta |
|---|---|---|
| `tools/call` (cualquier cliente MCP) | `arguments.agent_id` (`transport/server/handlers.rs` líneas 23-27; `handler_do/mod.rs` línea 175) | `"anonymous"` |
| Token con binding en ACL | `token_agent_bindings` (`transport/http/auth.rs` líneas 138-140; middleware líneas 585-601) | binding vacío ⇒ `current_bound_agent_id()` = None |
| Chequeo anti-suplantación | `handlers.rs` líneas 33-39 | solo bloquea si hay binding **y** el declarado no es `anonymous` (anónimo siempre permitido) |
| SSE | `transport/http/sse.rs` línea 387 | `"anonymous"` |
| Guild dispatch externo | `registry/guild_process.rs` líneas 513, 574, 664 | `"anonymous"` |
| Background jobs | `transport/server/background_jobs.rs` líneas 129, 147 | `"anonymous"` |
| Scheduler observe | `router/scheduler/observe.rs` línea 88 | `"anonymous"` |
| Coloquio REST | `author_id` del body (`api_v1/api_coloquio.rs` línea 258 → `memory/coloquio.rs` líneas 515-525) | obligatorio; solo protege `PROTECTED_AUTHORS` |

**Config de producción relevante** (`tylluan.toml`): `dev_mode = true` (línea 198), `default_role = "admin"` (línea 230), `[security.acl.token_agent_bindings]` **vacío** (línea 252), `[security.acl.agent_permissions]` **vacío**. Sin bindings, ningún token queda atado a un agente: el chequeo de suplantación nunca dispara y el `agent_id` es puramente declarativo.

**Qué se pierde cuando falta** (calidad, no solo estética): se salta el ACL por agente (`handlers.rs` línea 44), no se crea identidad (`handlers.rs` línea 55), el journal anota `anonymous` (línea 29), la memoria se escribe sin dueño (invisible a consultas por agente: `memory/agent_memory.rs` línea 40), el decay no la rastrea (`memory/silva/decay.rs` línea 711) y el rate-limit por agente comparte un mismo cubo.

**Cifras en la copia (6.159 filas de audit):**

| Atribución | Filas | % | Origen (guild/tool) |
|---|---|---|---|
| `anonymous` | 284 | 4,6 | tools kernel: `tylluan_recall` 129, `tylluan_remember` 56, graph/whoami/think… |
| `agent_id` vacío (`''`) | **490** | 8,0 | tool calls de guilds: `coloquio` read 156 / post 151, `bash` 97, `llama_backend` 11… |
| **sin atribuir** | **774** | **12,6** | rango 2026-07-19 → 2026-10-21 |

El segundo agujero (490 filas `''`) es distinto del primero: el audit de las llamadas a guilds no propaga el `agent_id` (queda vacío por defecto de columna). Además existe un nodo `agent:anonymous` pese a que el bootstrap de identidad se salta para anónimos (no verificado quién lo creó).

**Riesgo.** Toda métrica "por agente", el aislamiento de memoria, los permisos por agente y cualquier política de decay/gobernanza pierden entre el 12,6 % del tráfico (y el 29,5 % de las llamadas reales de T-DSH-7 ileíbles, según el criterio de exclusiones de benchmark). También con 490 filas el audit no permite responder "quién llamó a bash".

## (c) `deep` y `Deep` como identidades partidas

**No existe normalización en ningún punto.** El `agent_id`/`author_id` se guarda y compara tal cual llega:

- Almacenamiento crudo: `memory/coloquio.rs` líneas 515-525 (`author_id.to_string()`, solo prefija `agent-as-` si suplanta a `PROTECTED_AUTHORS`) y los inserts de audit (`transport/server/handler_do/audit.rs` líneas 71, 207, 213, 254, 298, 311).
- Comparaciones exactas: ACL por agente (`transport/http/auth.rs` líneas 142-152, `HashMap` clave exacta), bindings token→agente (`auth.rs` líneas 138-140), filtros de memoria y `GROUP BY` de métricas.
- Única comparación insensible a caja en todo el kernel: `security/agents_contract.rs` línea 89 (`trusts()` usa `eq_ignore_ascii_case`) — compara, pero no normaliza nada.

**Evidencia en las copias:**

| Fuente | `deep` | `Deep` | Otras variantes |
|---|---|---|---|
| audit `agent_id` | 221 | 15 | — |
| coloquio `author_id` | 273 | 1 | `antigravity` 346 / `Antigravity` 20; `buffy` 182 / `Buffy` 3 |
| silva (nodos identidad) | `agent:deep` | **`agent:Deep`** | ambas existen a la vez |

Nota: `agent_memory:deep:` son 8 nodos y no hay `agent_memory:Deep` — la variante mayúscula nunca llegó a escribir memoria con ese nombre, pero sí creó su nodo de identidad. `agents.toml` declara nombres canónicos en minúscula (`deep`, `antigravity`, `buffy`…).

**Consecuencia.** Dos nodos de identidad para el mismo agente, historial/actividad partido, permisos e `memory_isolation` por agente que no cubren la variante, y métricas por agente que subestiman. No es cosmético: rompe la trazabilidad y las políticas que se apoyan en el `agent_id` como clave.

**Recomendación.** Un helper único de canonicalización (trim + lowercase) aplicado en el punto de entrada (`handlers.rs` líneas 23-27, `coloquio.rs` líneas 515-525 y el dispatch de guilds), almacenando el canónico (y opcionalmente el crudo en metadata); migración de fusión de los nodos/variantes existentes (ojo: `LIKE` en SQLite es case-insensitive, la migración debe usar igualdad exacta o `COLLATE BINARY`); y un test de contrato "una variante de caja resuelve la misma identidad".

## Impact

- **Solo lectura**: copias a un directorio temporal propio; `data/` intacto (solo lectura); cero cambios de código. Único artefacto: este informe.
- Los tres hallazgos son accionables sin tocar producción: (a) triggers/rebuild FTS + reconciliación en health; (b) propagar `agent_id` al audit de guilds y decidir bindings en ACL; (c) canonicalización + migración de caja.
- Sin secretos: solo ids, conteos y líneas de código; no se extrajo contenido de mensajes ni tokens.

## No verificado

- **Qué config carga el kernel vivo**: `tylluan.toml` de producción tiene `dev_mode = true`, `default_role = admin` y bindings vacíos, pero no verifiqué que el proceso en ejecución use exactamente ese fichero (no toqué el kernel).
- **Causa de los 7 nodos sin indexar**: candidatos son un camino de inserción previo al sync de FTS o una migración hybrid→silva; no se trazó línea a línea.
- **Quién creó `agent:anonymous`** si el bootstrap de identidad se salta para anónimos.
- **Que las 490 filas `agent_id=''` vengan exactamente del audit de guild-dispatch**: se infiere por `guild`/`tool_name`, no por traza de código del punto de escritura.
- **Impacto cuantificado del bloat FTS en la calidad de bm25**: se midió tamaño y desincronía, no la degradación de ranking.
- **Otras variantes de caja** no detectadas: solo comparé `lower()` contra el valor crudo para los agentes ya presentes.
