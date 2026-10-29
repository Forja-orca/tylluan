# IMPL H-51 — Retención acotada de `coloquio_memory` en `NightConsolidation`

**Asignación:** T2099 (TL claude-code; implementa la propuesta C de H-49). **Base:** `7ed537e` (push201; rebasado y verificado en vivo). **Cero compromiso:** diff sin commitear, en `agent/hermes-base`. Gate de docs: referencias de línea sueltas en forma `línea NNNN`.

## 1. Problema y diseño (de H-49 a código)

H-49 demostró con datos de producción que:
- La poda habitual de SilvaDB **nunca** alcanza a `coloquio_memory`: `decay_enabled = false` en `tylluan.toml` línea 282 gatea `apply_decay` (`main.rs` línea 1871); la condición de poda (`fsrs_stability < 7.0`) es inalcanzable (default 14.0, Good neutral; medido mínimo 11.56); y `prune_dead_nodes(0.05)` requiere peso <0.05 mientras `reinforce_node` (H-50) lo sube activamente.
- El riesgo real es **crecimiento sin cota**: 1 653 nodos en prod, ~220 turnos/día de flota (~600 MB/año acumulados en `silva.db` + FTS + embeddings).

La propuesta C (retención acotada de historial) es la única viable:
1. Nueva clave en `[silva]` de config: `coloquio_retention_turns: u64` con serde default 0 (`config.rs` líneas 1534 y 1554).
2. **Default 0 = APAGADO**: cero cambio de comportamiento por defecto.
3. Con N>0: se borran los nodos `coloquio_memory` cuyo `turn` queda a más de N turnos del turno máximo de su canal (se conservan los N turnos más recientes por canal), **solo en NightConsolidation** (`DecayPhase`, `decay_phase.rs` línea 38).
4. Borrado vía `delete_nodes_where` (`fts.rs` línea 68), con sincronización atómica de `nodes_fts`.
5. Cascada manual de `edges`, `node_embeddings` y `node_sparse_embeddings`: `PRAGMA foreign_keys` nunca está activo en conexiones SQLite del kernel (`nodes.rs` línea 1106), así que las cláusulas `ON DELETE CASCADE` del esquema están inactivas; la cascada sigue el patrón exacto de `prune_dead_nodes` (`maintenance.rs` líneas 480-491).
6. Los `coloquio_memory` sin metadata de flywheel (clasificados por `hybrid_migration` a partir de documentos sin `source`, sin campos `channel_id` ni `turn`) quedan excluidos del WHERE (`json_extract(...) IS NOT NULL`): no hay forma honesta de acotarlos por turno.

## 2. Mapa del diff (10 ficheros, +211/-0)

| Fichero | Líneas | Qué hace |
|---|---|---|
| `crates/tylluan-kernel/src/config.rs` | líneas 1525-1535 y 1554 | Campo `coloquio_retention_turns: u64` en `SilvaConfig` con docstring + default 0 en `impl Default` |
| `crates/tylluan-kernel/src/memory/silva/maintenance.rs` | líneas 528-583 | Método público `prune_coloquio_retention(&self, keep_turns: u64) -> Result<usize>`: `keep_turns == 0` retorna `Ok(0)` sin tocar la BD; N>0 ejecuta `delete_nodes_where` + cascada manual de edges y embeddings |
| `crates/tylluan-kernel/src/memory/night/decay_phase.rs` | líneas 34-46 | `DecayPhase::run` consume `ctx.coloquio_retention_turns` y registra el número de nodos purgados en el informe de la fase |
| `crates/tylluan-kernel/src/memory/night/mod.rs` | líneas 100-103, 239 y 619 | Campo `coloquio_retention_turns: u64` en `PhaseContext`, cableado en los dos constructores de prueba del módulo (`test_phase_context` y `AgentPhase` test) con 0 |
| `crates/tylluan-kernel/src/memory/night/light_reranker_train_phase.rs` | línea 361 | Cableado de `coloquio_retention_turns: 0` en el `PhaseContext` de prueba del reranker |
| `crates/tylluan-kernel/src/main.rs` | líneas 2162, 2311 y 2328 | `run_night_consolidation_loop` recibe el valor de `config.silva.coloquio_retention_turns` y lo pasa a `PhaseContext` |
| `crates/tylluan-kernel/src/memory/silva/tests.rs` | líneas 2313-2429 | Dos tests unitarios de silva (ver sección 3) |
| `AGENTS.md` | línea 78 | Bump de recuento de tests para el gate de CI: lib 1422→1424, total 1509→1511 |
| `README.md` | línea 176 | Bump de recuento de tests total: 1509→1511 |
| `README.es.md` | línea 177 | Bump de recuento de tests total: 1509→1511 |

## 3. Tests entregados (`crates/tylluan-kernel/src/memory/silva/tests.rs`)

1. `h51_coloquio_retention_cero_no_borra` (línea 2329):
   - Inserta 12 nodos `coloquio_memory` en canal `general` con turnos 1..12 y metadata de flywheel (`json_extract` compatible).
   - Ejecuta `db.prune_coloquio_retention(0)`.
   - Verifica: retorna `0`; `COUNT(*) FROM nodes WHERE type='coloquio_memory'` permanece en 12.
2. `h51_coloquio_retention_por_canal_sin_huerfanos` (línea 2352):
   - Inserta 10 nodos en `general` (turnos 1..10), 4 en `dev` (turnos 1..4) y 1 nodo legacy sin `turn` en metadata.
   - Crea sustrato para cascada: una fila en `edges` (`coloquio_general_1` → `coloquio_general_10`) y una en `node_embeddings` (`coloquio_general_1`).
   - Ejecuta `db.prune_coloquio_retention(3)`.
   - Verifica:
     * Retorna `8` (en `general` caen t1..t7 porque $10-3=7$; en `dev` cae t1 porque $4-3=1$).
     * Quedan 7 nodos (3 recientes de general + 3 recientes de dev + 1 legacy).
     * `coloquio_general_7` cae; `coloquio_general_8` sobrevive.
     * Cero aristas huérfanas (`COUNT(*) FROM edges WHERE source/target NOT IN nodes` = 0).
     * Cero embeddings huérfanos (`COUNT(*) FROM node_embeddings WHERE node_id NOT IN nodes` = 0).
     * El nodo `coloquio_legacy` no se tocó (`COUNT(*)` = 1).

## 4. Estado del webhook de Hermes (T2110)

Ejecutado ANTES de arrancar H-51:
1. Copia de respaldo: `C:\Users\FoRJa\AppData\Local\hermes\config.yaml.backup-webhook-bind`.
2. Edición vía interfaz diseñada: `hermes config set platforms.webhook.extra.host 127.0.0.1` y `...extra.port 8654` (fuera del rango de Tylluan 47000-47100).
3. Verificación del config en disco: líneas 1021-1026 muestran `extra.host: 127.0.0.1` y `port: 8654` aplicados limpiamente.
4. Escaneo de clientes en repos y scripts: 0 referencias a 8644 en `tylluan.toml`, scripts y cron.
5. Intento de reinicio del gateway: `hermes gateway restart` se rehusó a matar esta sesión viva (`Refusing to restart: 1 live session(s) would be interrupted pid 30912`). El reinicio se ejecutará en la ventana limpia post-tarea.

## Impact

- **Critical path:** SilvaDB maintenance (`maintenance.rs`) y NightConsolidation (`decay_phase.rs`). No toca `transport/`, `guilds/`, ni `integrations/`, ni `tylluan.toml`.
- **Riesgo:** cero en producción con configuración out-of-the-box (`silva.coloquio_retention_turns = 0` es el default, y `tylluan.toml` en prod no tiene la clave).
- **Riesgo de activación:** es BORRADO de datos. Si el operador decide encenderlo en el futuro (`coloquio_retention_turns = N`), borra permanentemente del `silva.db` los nodos de turnos anteriores a `max_turn - N` por canal. La historia completa sigue viva en `mailbox.db` (`ColoquioDb`, donde vive el chat real), por lo que la pérdida es solo en la memoria indexada (FTS + embeddings), no en el log del equipo.
- **Cascada:** limpia `edges`, `node_embeddings` y `node_sparse_embeddings` manualmente en la misma transacción lógica, garantizando que el borrado no deja registros huérfanos que corrompan `search_hybrid` o `local_query_graph`.
- **Compatibilidad:** un `tylluan.toml` sin la clave carga con default 0 idéntico a antes; `PhaseContext` en tests fuera de `NightConsolidation` no requiere cambios en código externo porque los tres sitios del crate fueron actualizados.

## No verificado

- [no verificado] No se probó la fase completa `run_night_consolidation_loop` en un kernel de prueba con base de datos real (solo tests unitarios en memoria de `prune_coloquio_retention` y de los phases del orchestrator).
- [no verificado] No se midió el tiempo de ejecución de `delete_nodes_where` sobre los 1 653 nodos de `silva.db` de producción (en H-49 medimos que la copia SQLite transaccional tomó 2 s, por lo que el DELETE sobre 1.6k filas en una base de 200 MB es sub-segundo, pero no se ha ejecutado en vivo porque la regla prohíbe activarlo en prod).
- [no verificado] El reinicio del gateway webhook (`8654`) está pendiente de ejecutarse al cerrar la sesión; la escucha actual en `0.0.0.0` puerto 8644 sigue activa hasta ese restart.
