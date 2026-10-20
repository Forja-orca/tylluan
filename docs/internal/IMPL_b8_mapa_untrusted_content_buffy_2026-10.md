# B-8 — Mapa de `untrusted_content`: dónde se pone → dónde se lee → qué decisión cambia

**Fecha:** 2026-10-21 · **Agente:** Buffy · **Worktree:** `E:\tylluan-wt-buffy` (HEAD `8001d23`, diff sin commitear, regla 9) · **Código del kernel: NO MODIFICADO** (encargo: "sin cambios de código"; mapeo estático con archivo:línea verificado contra el árbol actual).

## 1. Tabla principal

| Dónde se pone | Dónde se lee | Qué decisión cambia |
|---|---|---|
| **Externo**: `metadata` JSON de un nodo Silva con `"untrusted_content": true` (contrato de lectura; en el repo **no hay ningún escritor Rust** del boolean — ver §2). Productores previstos: MCP tipo REA, A2A, ingestion web (`coherence_gate.rs:86-93`). | `coherence_gate.rs:94-101` (`metadata_untrusted`: parsea `node.metadata`, mejor esfuerzo, nunca panic) | — |
| ↳ | `coherence_gate.rs:323-333` (capa 2 del gate; condición OR en :329: `provenance == "federation_peer" && weight <= 1.0` **o** `metadata_untrusted`) | `score *= PROVENANCE_PENALTY` (0.1, `coherence_gate.rs:35`) — **solo ranking**: el nodo sigue en resultados, degradado. Cuenta en `GateStats.penalized` (:349-351). |
| ↳ (vía penalized) | `handler_recall.rs` (líneas 676 y 1008) | Aviso «⚠️ N resultados filtrados por control de coherencia» en la respuesta de `tylluan_recall` (N = eliminated + penalized). |
| ↳ (vía `GateStats.penalized_nodes` → `penalized_node_ids`) | `handler_recall.rs:618` y `:850` → `coherence_gate.rs:383+` (`hybrid_classify`) | Etiquetado de los ejemplos A/B (`gate_label = REJECT` para nodos penalizados) que alimentan el coleccionador de datos de entrenamiento (`llm_examples`). **Sin decisión en runtime** sobre el turno actual. |
| ↳ (stats agregadas) | `api_security.rs:10` (`cumulative_stats`) | Exposición en el endpoint de stats del gate (observabilidad, no decisión). |

**Resumen de la decisión real:** el flag `untrusted_content` hoy **penaliza ×0.1 el score en el recall** (y solo ahí). No cuarentena, no elimina, no bloquea escrituras, no marca el contenido devuelto, y **no se aplica en ninguna otra ruta de lectura** (§3).

## 2. Escritores del flag (hueco del contrato)

| Sitio | Qué hace hoy |
|---|---|
| Kernel Rust (`src/`) | **Cero escritores.** `grep -rn "untrusted_content" src/` solo devuelve líneas de `coherence_gate.rs` (lectura + tests T1277-A). Ni `memory/write`, ni ingest, ni A2A, ni federation escriben el boolean en el metadata. |
| `guilds/core/utils.py:204` (`flag_untrusted_content`) | Solo añade un **banner de texto** al contenido si coincide un patrón de inyección; no toca metadata ni booleano. Consumido por `websearch.py:67` y `deep_web_research.py:127,219`. |
| Externo | Cualquier cliente que haga `upsert_node`/`memory/write` con `metadata` JSON conteniendo `"untrusted_content": true`. El contrato es de **lectura honoraria**: el gate lo respeta si existe, nadie lo exige al escribir. |

Consecuencia operativa: en una flota donde los productores de contenido externo son clientes MCP (Python/JS ajenos), la protección depende de que **cada** productor acuerde el booleano; en el repo, el único mecanismo real de "marcado" es el banner de texto de Python, que el gate **no** lee (no es metadata).

## 3. Rutas por las que memoria no confiable llega al contexto de un turno

Solo la **primera** pasa por el gate:

| # | Ruta | Archivo:línea | Gate (`CoherenceGate::filter`) |
|---|---|---|---|
| 1 | `tylluan_recall` (búsqueda híbrida) | `handler_recall.rs:597` (cache-hit) y `:834` (cache-miss) | **SÍ** — las únicas dos invocaciones del gate en todo `src/` |
| 2 | `tylluan_recall` modo `@inbox` (buzón del agente) | `handler_recall.rs:206-227` — `get_thread`/`get_messages_for` y devolución verbatim **antes** del gate (early return) | NO |
| 3 | Contexto de sesión que el recall prepende (resumen del agente) | `handler_recall.rs:191-204` — `get_summary` prependido con cabecera «Contexto de sesiones anteriores»; la cabecera de desconfianza solo se activa con `provenance == "federation_peer"` (:195-199), **no** con el flag `untrusted_content` | NO (es contexto, no candidato) |
| 4 | `agent_bootstrap` / `resume_context` (resumen + memorias recientes) | `bootstrap.rs:25-99` (`get_summary` :33, `get_memories` :34); invocado además automáticamente en **cada tool call** con `agent_id` (`handlers.rs:55-63`) | NO |
| 5 | `read_channel`/`search_channel` del guild Coloquio (mensajes de canales) | `guilds/core/coloquio.py:401+` → REST `api_coloquio` (`api_v1/mod.rs:206-215`: thread/new/search/turn) | NO |
| 6 | `agent_mail` (buzón/blackboard) por REST | `api_v1/mod.rs:287` (`get_tasks_for_agent`) y rutas mailbox asociadas | NO |
| 7 | `tylluan_think` (broadcasts recientes de actividad) | `handler_think.rs:70` (`get_recent_broadcasts("broadcast", "agent_activity", 24)`) | NO |
| 8 | Creación de `agent_summary` (consolidación) | `agent_memory.rs:319` (`consolidate_if_needed`) — previews de 80 chars **verbatim** de las memorias, sin juzgar (hallazgo 5 del B-4); el resumen resultante sirve por las rutas 3 y 4 | NO (creación) / solo ranking si luego se recuerda (ruta 1) |

### Lectura cruzada (por qué esto importa)

- El **camino de mayor volumen** hacia el contexto de un turno no es el recall: es el bootstrap automático (ruta 4) y el resumen de sesión (ruta 3), ambos sin gate y sin marca de desconfianza salvo federation.
- El modo `@inbox` (ruta 2) hace que `tylluan_recall` sea **también** una ruta de buzón sin gate — el mismo nombre de herramienta tiene dos posturas de confianza distintas.
- Un nodo marcado `untrusted_content: true` que termine consolidado en un `agent_summary` (ruta 8) pierde su metadata al resumirse: el resumen es un nodo nuevo **sin el flag**, y por las rutas 3-4 viaja sin degradación alguna.
- La única señal de desconfianza hoy "visible" para el LLM es el **banner de texto** de `flag_untrusted_content` (Python), que es exactamente lo que un inyector puede escribir/contrarrestar, y ni siquiera llega como metadata al gate.

## 4. No verificado / limitaciones

- Mapeo **estático** contra `8001d23`; no ejecuté un drill vivo (el encargo era la tabla, sin cambios de código).
- No verificado si algún cliente externo de la flota (REA, A2A) escribe hoy el booleano en la práctica — el contrato de lectura existe desde T1277-A pero el repo no lo produce.
- La ruta WS del Coloquio (dashboard) comparte el mismo almacén que la ruta 5 pero no la audité línea a línea.

## Impact (ADR-013)

Sin cambios en `src/`, sin tocar el kernel de producción ni sus datos. Solo se añade este informe en `docs/internal/`. Los hallazgos (rutas 2-8 sin gate, hueco de escritores del flag) son recomendaciones para el TL: no se aplicó nada.

## Checks ejecutados

- Verificación de cada `archivo:línea` citado contra el árbol de `8001d23` (lecturas directas + greps de llamadores: `CoherenceGate::filter` tiene exactamente 2 call sites, ambos en `handler_recall.rs`).
- `scripts/check_docs_reality.sh` y `check_no_predation.sh`: al cierre de la cola 2 sobre el estado final del worktree.
- No se ejecutó `cargo test/clippy`: el ítem no tocó código Rust.
