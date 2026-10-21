# T-DSH-25 — Test de Continuidad (Q1): sondas sin LLM con reinicio simulado

- **Agente:** dsh
- **Fecha:** 2026-10-21
- **Base:** worktree `E:\tylluan-wt-dsh` en `--detach origin/main` (`514058e`); limpio antes de empezar.
- **Encargo (turno 1507):** Q1 del documento de preguntas pide un Test de Continuidad. Implementar las 3 sondas propias que NO necesitan LLM, contra el router real con `DATA_DIR` en carpeta temporal y un **reinicio simulado** (construir el estado, soltarlo y reconstruirlo desde el mismo directorio): (a) identidad; (b) tarea abierta (plan pendiente o mensaje de buzón sin leer); (c) memoria normal y marcada `untrusted`; (d) fecha-hora real y viva. Fichero nuevo `tests/continuity_probes.rs`, **ningún test con `cfg(unix)/cfg(windows)` completo**, tabla sonda × esperado × real, `#[ignore]` con motivo y archivo:línea si falla, informe con `## Impact` y no verificado.

## Entrega

`crates/tylluan-kernel/tests/continuity_probes.rs` — 5 sondas: **4 passed / 0 failed / 1 ignored** (el ignorado documenta un hallazgo real, no un fallo de la sonda). El estado del kernel se construye con almacenes **en fichero** dentro de un directorio temporal (silva, buzón, coloquio, journal, peers, contratos, jobs, identidad de nodo), se lanza el router real (`build_router`), se sueltan router y estado y se reconstruye desde el mismo directorio. Sin arrancar procesos (regla de flota).

```text
rustup run 1.99.1 cargo test -p tylluan-kernel --test continuity_probes -- --nocapture
# 4 passed; 0 failed; 1 ignored (19,7 s)
```

## Tabla sonda × esperado × real

| sonda | esperado | real | veredicto |
|---|---|---|---|
| (a) identidad | mismo `agent_id`, rol y fecha de creación antes/después del reinicio | `agent_id=probe-t25`, rol `Probe Continuity`, `born_at` idéntico; nodo `agent:probe-t25` presente tras el reinicio | OK |
| (b) tarea abierta: buzón | mensaje sin leer sobrevive y sigue legible, sin banner (remitente local registrado) | `@inbox` tras reinicio devuelve `T25-MAILBOX-sobrevive-al-reinicio`, sin banner | OK |
| (b2) tarea abierta: plan pendiente | un plan pendiente sobrevive a un reinicio real | **NO**: `PLAN_STORE` es `static OnceLock<RwLock<HashMap>>` en el proceso (`security/grants.rs`, línea 46-58) → un proceso nuevo lo pierde; in-process no es medible | `#[ignore]` — hallazgo O3 |
| (c) memoria | recuerdo normal aparece en `tylluan_recall` tras reinicio; el marcado conserva la marca | ambos sobreviven; metadata `untrusted_content=true` confirmada tras reinicio; `tylluan_think` lo muestra **con banner**; `tylluan_recall` lo devuelve **sin** banner | OK con hallazgo O2 |
| (d) tiempo | `whoami` da la fecha-hora real (±5 s) y avanza tras el reinicio | ±0 s en ambas medidas; `18:44:19.22` → `18:44:28.67` (avanza) | OK |

## Hallazgos

- **O1 — Carrera del auto-bootstrap de identidad (observada en la corrida 1).** Si `register_identity` es el **primer** contacto, el auto-bootstrap fire-and-forget que se lanza en `handlers.rs` (línea 51-66, `tokio::spawn` con check `!has_identity` + `upsert_node` sobre `agent:<id>`) puede escribir su placeholder **después** del registro y dejar `role=unregistered`, `born_at` del momento de la carrera. Evidencia: en la corrida 1 (registro como primera llamada) el `whoami` previo al reinicio ya devolvía el placeholder y el test falló en «rol perdido»; con la secuencia realista (primer contacto `whoami` → esperar el bootstrap → `register_identity` → esperar) el test pasa de forma estable. No es determinista (depende del orden de dos tareas async contra el mismo nodo, last-write-wins) y por eso **no** hay sonda que lo afirme; queda como observación con el log de la corrida 1. Mitigación sugerida para un D-item: que el bootstrap re-chequee `has_identity`/protección justo antes de escribir o que no se dispare cuando la herramienta es `tylluan_do` con intención de registro (hoy solo se excluyen los nombres `whoami`/`register_identity`, no el `tylluan_do` que los enruta).
- **O2 — `tylluan_recall` no surfacea el banner de un nodo marcado.** El flag persiste en metadata y `tylluan_think` lo muestra con banner (`handler_think.rs`, línea 388), pero los resultados normales de `tylluan_recall` no aplican `mark_text`: en `handler_recall.rs` el banner solo cubre el resumen de sesión del agente (línea 195) y los mensajes externos del buzón (línea 260), no la lista de resultados de búsqueda. Consecuencia: una memoria marcada conservada tras el reinicio es visible por recall **sin** la señal de desconfianza (sí con ella por think). Mismo tipo de hueco que los que D-18 estaba cerrando; candidato a D-item.
- **O3 — Los planes pendientes no sobreviven a un reinicio real.** `PLAN_STORE` (`security/grants.rs`, línea 46-58) es un estático en memoria del proceso: el flujo plan→aprobar de M31-P2 pierde los planes pendientes si el kernel reinicia entre `prepare` y `approve`. El mensaje de buzón sí sobrevive (fichero). La sonda quedó `#[ignore]` porque el reinicio simulado es in-process y el estático sigue vivo: el assert de la expectativa honesta (plan ausente) no puede pasar en este arnés; se documenta con la cita de código.
- **O4 — `tylluan_remember` con `agent_id` descarta el metadata del llamante.** En la variante con `agent_id` el recuerdo se guarda como `agent_memory` (ruta `AgentMemoryManager`) y el `metadata` (incluida la marca `untrusted_content`) no llega al nodo; sin `agent_id` se guarda como nodo `memory` de Silva con el metadata tal cual. Es la razón por la que la sonda (c) usa el camino sin `agent_id`; si se quiere poder marcar un recuerdo propio como no confiable, hoy el camino con `agent_id` no lo permite. Se declara como observación (puede ser diseño: `agent_memory` tiene su propio esquema).

## Verificación

| criterio | resultado |
|---|---|
| `cargo test --test continuity_probes` | **4 passed / 0 failed / 1 ignored** (19,7 s) |
| `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` | **exit 0** |
| `cargo test -p tylluan-kernel --lib` | **1174 passed / 0 failed / 4 ignored** (coincide con AGENTS.md) |
| `cfg(unix)`/`cfg(windows)` en el fichero nuevo | **ninguno** (requisito del encargo tras el CI rojo de `a14dcdd`) |

## Impact

- **Diff sin commitear: solo tests** — un fichero nuevo (`tests/continuity_probes.rs`) y este informe. **Cero cambios en `src/`**; no se tocó el kernel `:47004` ni `E:\tylluan`; sin secretos reales (token de test, directorios temporales `%TEMP%\tylluan_t25_*`, identidad de nodo en el propio temporal).
- Coste CI: +19,7 s (4 reinicios simulados con SQLite en fichero; las esperas son a propósito para dejar correr el auto-bootstrap y detectar el cambio de `mtime`).
- **Resultado de fondo para Q1:** identidad, buzón sin leer, memoria (normal y marcada) y fecha-hora **sí** sobreviven al reinicio simulado; los **planes pendientes no** (estático en proceso). Dos huecos de superficie de desconfianza (O2) y una carrera de identidad (O1) quedan como observaciones con cita de código, sin decisión de producto.

## No verificado

- **Reinicio real de proceso:** el arnés simula el reinicio soltando el estado y reconstruyéndolo in-process. No se arrancó ni reinició ningún proceso (regla de flota), así que: (i) el estado global en memoria que NO se recrea (p. ej. `PLAN_STORE`, caches estáticas) no se ve afectado por el «drop»; de ahí que (b2) no sea ejecutable; (ii) la liberación de ficheros SQLite en Windows se probó con reintentos (20 × 200 ms), sin fallo observado, pero no es un reinicio de SO real.
- **Carrera O1:** observada una vez (corrida 1) y no reproducida de forma determinista; el informe la declara como observación, no como sonda con veredicto.
- **Embeddings:** en el entorno de test no se cargan embeddings (`Búsqueda: solo texto`), así que la búsqueda de recall/think es BM25/FTS; con embeddings cargados el ranking podría cambiar, aunque la persistencia y la marca no dependen del ranking.
- **Ventanas de TTL y expiración** del buzón/consentimientos no se esperaron (mensaje con `ttlSecs` de un día; no se probó el vencimiento real).
- **Bootstrap MCP completo por stdio** (`build_agent_bootstrap`) no se ejerció como sesión de agente real: el estado se sondea con `tools/call` sobre el router real, que es la misma superficie que usa `build_agent_bootstrap` para leer Silva.
- **No se ejecutó `verify.sh` completo** (Rust: test nuevo + clippy + lib).
