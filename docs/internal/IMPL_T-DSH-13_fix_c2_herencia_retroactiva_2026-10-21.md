# T-DSH-13 — C2: herencia retroactiva de `untrusted_content` a los resúmenes

- **Agente:** dsh
- **Fecha:** 2026-10-21
- **Base:** `d8bec3b` == origin/main (push 84, T-DSH-12 integrada; lib 1142).
- **Encargo (turno 1475):** fix de **C2** (herencia retroactiva): cuando un nodo pasa a `untrusted_content`, re-marcar los `agent_summary`/`session_digest` que lo citan (o pasada de maintenance que propague la marca), mínimo y **en el sitio donde se marca**. Diff SIN commitear con test que pase C2 y no rompa C1; **sin tocar B2** (decisión de José). Informe con `## Impact` y «no verificado».

## Diseño (2 ficheros de código + 1 test)

El flag `untrusted_content` se escribe hoy en tres productores —federación (`api_federation.rs`), ingesta (`api_ingest.rs`) y A2A (`a2a.rs`)— y **todos pasan por `upsert_node_with_validity`**. Ese es el choke point del marcado, así que la propagación va ahí, no en cada productor:

1. **`security/trust.rs` — nueva función `propagate_untrusted_to_summaries(silva, node_id)`** (async, `pub`):
   - carga el nodo; si no está marcado, no hace nada;
   - toma el **mismo recorte de 80 chars que copia la consolidación** (`agent_memory.rs`), así que "cita" = el resumen contiene ese preview literal;
   - `SELECT id, metadata FROM nodes WHERE type IN ('agent_summary','session_digest') AND instr(content, ?1) > 0`;
   - a cada resumen sin marca le inserta `untrusted_content: true` + `untrusted_reason: "inherited(retro): <origen>"` vía `UPDATE nodes SET metadata`;
   - idempotente (salta los ya marcados) y devuelve el número de re-marcados.
2. **`memory/silva/nodes.rs` — hook al final de `upsert_node_with_validity`**: tras el `commit`, si el metadata de ESTA escritura lleva `untrusted_content: true`, llama a la propagación y loggea cuántos resúmenes re-marcó. Solo se ejecuta en escrituras marcadas (raras: federación/A2A/ingesta), no en el camino caliente.
3. **`tests/trust_summary_retro.rs`** (nuevo, **no ignorado**): C1 control (herencia en creación intacta), C2 (resumen limpio → marcar la fuente después → resumen re-marcado → el banner llega al bootstrap), control negativo (un resumen que no cita la fuente no se toca) e idempotencia (segunda propagación devuelve 0).

No se crea ningún nodo nuevo, no se cambia contenido, no se toca el gate ni el ranking, y **B2 no se ha tocado** (sigue abierto a decisión de José).

## Verificación

| check | resultado |
|---|---|
| `cargo test --test trust_summary_retro` | **1 passed** |
| Matriz `trust_banners_adversarial -- --ignored` | **C1 → OK**, **C2 → OK** (re-marcado verificado); quedan **B2** y **B3b** (carrera): `2 quebrados de 12` |
| `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` | **exit 0** |
| `cargo test -p tylluan-kernel --lib` | **1142 passed / 0 failed / 4 ignored** (recuento intacto) |

## Impact

- **2 ficheros de `src`** (`trust.rs` +27 líneas de helper, `nodes.rs` +14 de hook) y **1 test nuevo**; sin dependencias nuevas. ADR-013: el único cambio de comportamiento es que un resumen que ya copió el texto de un nodo marcado a posteriori hereda la marca (banner en bootstrap/contexto). Ningún resumen limpio sin cita cambia.
- **Coste**: una lectura + un SELECT + N UPDATEs solo en escrituras con flag; no afecta al camino caliente del recall.
- Cero escrituras en producción (estado de test en memoria); sin secretos.
- **B2 intacto** por indicación explícita (su propuesta sigue anotada para José: roster + binding, marcar por defecto, no rechazar).

## No verificado

- **Índice FTS del metadata**: la propagación actualiza `nodes.metadata` con SQL directo y no re-indexa la FTS5 (el metadata nuevo no entra en el índice hasta un rebuild). El contenido y la consulta por `agent_id` no cambian; el efecto es que buscar por el token literal `untrusted_content` en FTS5 no encontrará esos resúmenes hasta el rebuild (mismo tipo de deriva que documentó T-DSH-8). No se verificó impacto real porque no hay consultas que busquen esa clave.
- **Quarantine del juez (ASI06)**: la cuarentena marca la columna `quarantined`, no el flag de metadata; si el juez debe además propagar desconfianza a resúmenes ya creados, es una decisión aparte (no implementada, mencionada como vecina).
- **Resúmenes con el texto truncado o reformateado**: la cita se detecta por el preview literal de 80 chars; si una consolidación futura cambia el formato del preview, la propagación no encontrará el resumen (fallará en silencio, sin marcar).
- **Productores externos que escriban el flag por SQL directo** (fuera de `upsert_node_with_validity`): no se propagaría; en el repo no existe ese camino.
