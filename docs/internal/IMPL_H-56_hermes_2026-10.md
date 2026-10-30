# IMPL H-56 — Qué captura HOY Tylluan por agente (solo lectura, sin código)

**Asignación:** T2133 (TL claude-code, idea de Jose). **Base:** `7ed537e` (HEAD worktree verificado con `git rev-parse --short HEAD`). **Cero código, cero cargo:** informe documental. Mediciones sobre **copias ro** de las bases de producción (sqlite backup API sobre `E:/tylluan/data/*.db`; ficheros originales intactos, verificado por `lsof` implícito: solo aperturas `mode=ro`).

## 1. Tabla agente → qué se guarda → base/tabla

| Captura | Dónde vive (base → tabla) | Atribución por agente | Qué es exactamente |
|---|---|---|---|
| Posts de Coloquio (canal) | `mailbox.db` → `coloquio_messages` (6.3 MB) | columna `author_id` | texto íntegro del post + `turn`, `channel_id`, `created_at`, `metadata`, `reply_to_turn` |
| Espejo de posts en memoria | `silva.db` → `nodes` type=`coloquio_memory` (1 653 nodos, 2.31 MB contenido) | `metadata.channel_id` + `turn` | copia vectorizada/FTS del post, no el original |
| Llamadas a tools soberanas (intent) | `audit.db` → `guild_audit_log` (8 400 filas, 8.6 MB) | columna `agent_id` | **intent íntegro** (media 232 B, máx 7 794 B) + `status` + **`result_preview` truncado a 200 chars** + hash-chain |
| Resultados de `tylluan_do` | `silva.db` → `nodes` type=`episode`, id `memory:{millis}` (2 778 nodos, 1.77 MB) | `metadata.agent_id` | episodio completo del resultado (handler_do/outcome.rs línea 350) |
| `tylluan_remember` | `silva.db` → `nodes` id `memory:{millis}` (handler_remember.rs línea 444) | `metadata.agent_id` | nodo de memoria con embedding |
| `memory_write` REST | `silva.db` → `nodes` id `memory_write:{uuid}` | `owner_scope`/`agent_id` opcionales en request | **solo 13 nodos / 457 B en 3 meses** — la vía REST casi no se usa |
| Sesiones `/agents/session` | **NO se persisten**: `DashMap<String, AgentSession>` en RAM (`api_agents.rs` línea 28) | clave = `agent_id` | se pierden completas en cada restart |
| Síntesis de sesión (derivado) | `silva.db` → type=`session_digest` (1 011) y `agent_summary` (1 771, 2.51 MB) | id `session_digest:{agente}:{ts}` / metadata `agent_id` | resumen consolidado LOSSY (NightConsolidation); los `agent_summary` marcan `superseded_by` |
| `task_context` | `task_context.db` → `task_context` (12 KB; `updated_by`) | columna `updated_by` | contratos acotados: decisiones+punteros, no el chat |
| Docs ingeridos | `tylluan.db` → `documents` (2 900 filas, 3.25 MB) + FTS | `source` | ingestión documental, no conversación |
| Perfiles/rating | `agent_profiles.db` → `agent_profiles`, `agent_domain_scores` | `agent_id` | perfil declarativo y scores |

## 2. Bytes por día medidos (últimos 30 días, copias ro)

Posts Coloquio (`coloquio_messages`, bytes de `content`/día, ventana 30 d hasta el pico T2155):

| Agente | posts/30d | KB/30d | **KB/día** |
|---|---|---|---|
| claude-code | 563 | 680 | 22.7 |
| dsh | 169 | 566 | 18.9 |
| jose | 29 | 389 | 13.0 |
| deep | 175 | 387 | 12.9 |
| buffy | 108 | 275 | 9.2 |
| antigravity | 148 | 228 | 7.6 |
| hermes | 85 | 75 | 2.5 |

(El resto de autores: <1 KB/día. Históricos raros: `agent-as-system`/`kernel` 0.1 KB/d.)

Audit (`guild_audit_log`, intent+preview por fila): claude-code 1 385 filas (1.30 MB acumulados desde 2026-07-19); fila media = 232 B intent + ≤200 B preview ≈ 0.4 KB. Estima diaria flota: ~2 900 filas/mes ≈ 1.2 MB/mes de audit.

Overhead de silva: 6 319 embeddings densos (1024 d × f32 ≈ 4 KB c/u ≈ **26 MB**) + 6 311 sparse + 27 390 edges + FTS → explica los 203 MB de `silva.db` frente a ~10 MB de contenido crudo. Crecimiento total medido en H-49: ~600 MB/año en la flota actual; los embeddings dominan el ratio overhead/contenido (~2.6× el contenido).

## 3. Huecos frente a "todo el chat"

1. **El chat runtime de cada agente es invisible**: Tylluan solo registra lo que el agente decide enviarle (posts a Coloquio, `do`, `remember`). El prompt de sistema del agente, su conversación local con Jose (p.ej. este chat de Hermes, sesiones de Claude Code en terminal) y sus tool calls NO pasan por Tylluan y no se guardan en ningún sitio de Tylluan.
2. **Resultados truncados en audit**: `result_preview` tiene tope duro de 200 chars; la salida completa solo sobrevive si la llamada fue `tylluan_do` (episode) — las llamadas REST/guild no-MCP pierden el resultado.
3. **Sesiones volátiles**: `AgentSession` vive solo en RAM; tras restart no queda nada crudo de la sesión, únicamente la síntesis (`session_digest`/`agent_summary`) que es lossy y no conserva el registro fiel.
4. **Atribución imperfecta**: 1 280 filas de audit con `agent_id` vacío + 663 con `anonymous` (15%+30% de audit no atribuible a un agente real); los posts con alias duplicados (`Antigravity`/`antigravity`, `Buffy`/`buffy`, `hermes`/`hermes-agent`) fragmentan la cuenta por agente.
5. **Usuario fuera de Coloquio**: los mensajes de Jose solo se capturan si los escribe en Coloquio (35 posts históricos, 430 KB — sus prompts son largos); la interacción por chats privados de cada agente no deja rastro central.
6. **`memory_write` REST infrautilizado** (13 nodos/457 B) y con atribución opcional: si el caller no manda `agent_id`, el nodo queda huérfano de dueño.

## Impact

- Solo-lectura: cero cambios de código; `git diff` de este trabajo = solo este doc (`docs/internal/IMPL_H-56_hermes_2026-10.md`, untracked). No toca `transport/`, `guilds/`, `integrations/` ni `tylluan*.toml` → sin gate anti-predación.
- Las mediciones son de producción (copias ro); los números alimentan la idea de Jose de "sesión por agente con registro fiel": hoy faltan los huecos 1-3 de la sección 3 para poder afirmar "registro fiel".

## No verificado

- [no verificado] El ratio KB/día de audit para la flota es estimado a partir de conteos mensuales, no por día exacto (los `timestamp` están en 3 formatos distintos — ISO con nanos, ISO sin nanos, y texto sin zona — y no se parsearon todos).
- [no verificado] `result_preview` truncado a 200: confirmado por `MAX(LENGTH(result_preview))=200` en los datos; el literal del truncado en código no se re-ubicó con línea exacta en este tick (buscar en `agent_memory.rs`).
- [no verificado] No se midió el coste real de las columnas FTS5 (`nodes_fts*`) por separado; el 26 MB de embeddings es cálculo (6 319 × 1024 × 4 B), no lectura de página.
- [no verificado] El estado de `session_digest`/`agent_summary` tras el próximo ciclo de NightConsolidation puede cambiar los conteos (se midió a las ~02:30 del 2026-10-30).
