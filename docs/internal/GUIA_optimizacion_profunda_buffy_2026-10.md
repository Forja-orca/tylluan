# Guía de optimización profunda — Tylluan

**Fecha:** 2026-10-22 · **Agente:** Buffy · **HEAD analizado:** `5e49bf8` (= `origin/main`)

> **Ninguna línea de código ha sido modificada.** Todo lo que sigue es lectura,
> verificación y propuesta. Cada afirmación lleva `fichero:línea` para que
> cualquiera del equipo la compruebe sin confiar en mi palabra.

---

## Cómo leer esta guía

Cada hallazgo lleva una etiqueta de **estado de verificación**:

| Etiqueta | Significado |
|---|---|
| **[VERIFICADO]** | Lo leí en el código y lo reproduje/contrasté. La línea existe y hace lo que digo. |
| **[HIPÓTESIS]** | Deducción razonable a partir del código, **sin medición**. No citar como hecho. |
| **[LITERATURA]** | Contraste con fuentes externas, con enlace. |

Y una de **prioridad**, según el criterio del encargo (un 5% en 10 cosas suma mucho
más que un rediseño):

| Prioridad | Significado |
|---|---|
| **P0** | Barato, riesgo casi nulo, medible. Hacer primero. |
| **P1** | Requiere medición antes/después, riesgo bajo-medio. |
| **P2** | Requiere decisión de arquitectura. No hacer sin ADR. |

**Regla de oro para todo el equipo:** ninguna de estas mejoras se acepta sin una
medida antes/después sobre el camino real. El bloque 8 define cómo medir. Si no
se mide, no se fusiona.

---

## Bloque 1 — Mapa del sistema

### Tamaño por crate (solo `src/`, tests excluidos)

| Crate | LOC | Ficheros | Peso |
|---|---:|---:|---|
| `tylluan-kernel` | **89.431** | 193 | 89% del código |
| `tylluan-evals` | 3.978 | 9 | harness de benchmark |
| `tylluan-link` | 4.828 | 16 | P2P/federación |
| `tylluan-cli` | 2.593 | 4 | arranque/gestión |
| `tylluan-fsrs` | 327 | 1 | spaced repetition |
| `tylluan-common` | 184 | 4 | tipos compartidos |

**Lectura:** el peso está todo en el kernel, y dentro del kernel en un puñado de
ficheros grandes. Los 12 más grandes suman ~22.000 LOC (25% del kernel en 12 de
193 ficheros). Eso es buena noticia: **el perímetro de mejora es pequeño.**

Los que importan para velocidad:

| Ficheros | LOC | Por qué importa |
|---|---:|---|
| `memory/silva/search.rs` | 1.858 | **El camino caliente.** RRF, vector, FTS5 |
| `router/embeddings.rs` | 1.696 | ONNX, el cuello de botella de CPU |
| `transport/server/handler_recall.rs` | 1.545 | el endpoint de recall |
| `memory/silva/nodes.rs` | 2.059 | acceso a nodos (N+1 vive aquí) |
| `memory/silva/graph.rs` | 1.123 | PPR + degree centrality |
| `transport/http/auth.rs` | 1.193 | middleware por request |

### El camino caliente del recall (el que decide la latencia percibida)

```
tylluan_recall
  └─ handler_recall.rs
       └─ search_hybrid_for_recall_detailed      search.rs:546
            ├─ source_limit = limit × 3          search.rs:555-559   ← sobre-fetch ×3
            └─ search_hybrid_detailed            search.rs:404
                 ├─ ① EMBED query               embeddings.rs:795   ← mutex ONNX, lote=1
                 ├─ ② VECTOR (IVF/linear)       search.rs:165/190   ← scan 5000 + N+1
                 ├─ ③ GRAFO PPR (si semillas)   graph.rs:769        ← 2-3 queries chunked
                 ├─ ④ FTS5 BM25                 search.rs:920       ← bien, 1 query
                 ├─ ⑤ FUSIÓN RRF  K=60          search.rs:412,427   ← node.clone()
                 └─ ⑥ FILTRO archived           search.rs:563-571   ← otra ida a DB
            └─ ⑦ RERANK (2º modelo ONNX)        search.rs:793-811   ← si reranker_enabled
```

**Siete etapas, y seis de ellas pasan por un candado global.** Esa es la frase
que resume todo el documento.

---

## Bloque 2 — El hallazgo estructural (el que domina todo lo demás)

### [VERIFICADO] Todo el acceso a SilvaDB serializa por un único candado

`memory/silva/mod.rs:268-277`:

```rust
pub(crate) fn conn_timed(&self) -> tokio::sync::MutexGuard<'_, Connection> {
    let t0 = std::time::Instant::now();
    let guard = self.conn.blocking_lock();   // ← un solo candado, una sola conexión
    ...
}
```

`conn` es `tokio::sync::Mutex<Connection>`: **una única conexión SQLite** detrás de
**un único candado exclusivo**. Todas las lecturas de recall, todas las escrituras,
el PPR, el checkpoint y el fingerprint gate pasan por ahí.

**Por qué importa, con precisión técnica.** WAL permite que *conexiones separadas*
lean mientras otra escribe ([SQLite WAL](https://www.sqlite.org/wal.html)). **No**
convierte una conexión protegida por un candado exclusivo en lectores concurrentes:
con un solo candado, hay exactamente un query en vuelo en todo el proceso, aunque
sea de solo lectura.

> **[LITERATURA]** "WAL solves a different problem: it permits readers on separate
> connections to proceed alongside a writer... Tokio's mutex can make waiting for
> access asynchronous, but it does not make the protected SQLite operations
> concurrent."
> — [SQLite threading modes](https://www.sqlite.org/threadsafe.html),
> [WAL concurrency](https://www.sqlite.org/wal.html)

Complicación de Rust: **`rusqlite::Connection` es `!Sync`** ([rusqlite#188](https://github.com/rusqlite/rusqlite/issues/188)),
así que la opción "un candado" no es pereza, es la más simple que compila. Pero
también es el techo.

**Lo bueno:** el equipo ya lo sabe. Hay telemetría del candado —
`sqlite_lock_wait_nanos` y `sqlite_lock_acqs` (`mod.rs:272-275`). **Eso significa
que la mejora P0-2 del bloque 4 se puede medir sin escribir instrumentación nueva.**

**[HIPÓTESIS]** Bajo carga concurrente de recall (dashboard + varios clientes MCP),
la espera del candado domina la latencia p95. **Se puede confirmar hoy mismo**
consultando los contadores existentes. Si el ratio `wait/acq` es bajo, este bloque
 baja a P2 y el foco se va al bloque 3. **Medir antes de creerlo.**

---

## Bloque 3 — El camino del recall, etapa por etapa

### ① Embed de la consulta — lote de 1 bajo candado

`embeddings.rs:795-810`:

```rust
pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
    ...
    let mut batch = self.embed_batch(&[text])?;   // ← lote EXACTAMENTE de 1
```

`embed_batch` toma `self.model.lock()` — `std::sync::Mutex` (`embeddings.rs:763`).
Cada consulta concurrente compra el candado entero para inferir **un** texto.

**Pero la solución ya existe y está implementada.** `embeddings.rs:820-825`:

```rust
pub fn embed_batch_coalesced(self: &Arc<Self>, text: &str) -> Result<Vec<f32>> {
    if !Self::batching_enabled() {
        return self.embed(text);                    // ← camino actual de producción
    }
    self.shared_batcher()?.embed_one(text.to_string())
}
```

El *coalescing batcher* (T582) mergea N peticiones concurrentes en **una** inferencia.
N llamadores concurrentes pagan **un** candado en vez de N.

**[VERIFICADO] Está apagado.** `embeddings.rs:848-853` lee
`cfg.silva.embed_batching_enabled`, cuyo default es `false` (`config.rs:1224`,
`config.rs:1261`), y hay un **test que lo fuerza apagado** (`config.rs:2469-2471`,
"el master de recall sigue OFF por defecto (F2 pendiente del gate de F1)").
No aparece en `tylluan.toml`, así que producción corre el camino serializado.

Ver detalle en la mejora **P0-1**.

### ② Búsqueda vectorial — scan de 5000 blobs y N+1

`search.rs:165-215` (`search_vector_linear`), el fallback sin IVF:

```sql
SELECT node_id, embedding, model_name, model_hash
FROM node_embeddings ORDER BY rowid DESC LIMIT 5000
```

Luego, en un único hilo dentro de `block_in_place`:
- recorre los 5.000 blobs con `blob.chunks_exact(4)`;
- calcula **el producto punto Y la norma almacenada por fila**:
  ```rust
  let mut stored_norm_sq: f32 = 0.0;
  for (chunk, &q) in blob.chunks_exact(4).zip(query_embedding.iter()) {
      let s = f32::from_le_bytes([...]);
      dot += s * q;
      stored_norm_sq += s * s;      // ← trabajo redundante, ver P1-1
  }
  ```
- y después, **un `SELECT` por candidato superviviente**:
  ```rust
  for (id, score) in scored {
      if let Ok(Some(node)) = self.get_node_sync(&id, &conn) {   // ← N+1
  ```

`get_node_sync` (`nodes.rs:356-383`) **prepara la SQL en cada llamada** y hace un
`Utc::now()` por fila (`nodes.rs:378`).

**El N+1 está en cuatro sitios, no en uno:**

| Sitio | Línea exacta del `get_node_sync` |
|---|---|
| `search_vector_hnsw` | `search.rs:177` (bucle en `173-181`) |
| `search_vector_linear` | `search.rs:277` |
| path **IVF** | `search.rs:367` |
| `local_query_graph` | `graph.rs:796` |

**Matiz importante para no exagerar:** `nodes.id` es `TEXT PRIMARY KEY`, así que
SQLite crea el índice automático y **cada lookup es indexado, no un scan**. El
coste no es de escaneo: es el *overhead* de preparar SQL + round-trip + mapeo de
14 columnas, repetido `limit` veces. Con `limit=20` son 20 queries donde cabría 1.

> **[LITERATURA]** El propio código ya lo sabe en otro path.
> `search_sparse_candidates` documenta: *"Single JOINed query (no N+1)"*
> (`search.rs:814`). **El patrón está resuelto en el path denso-sparse y sin
> resolver en el denso.** Eso es el argumento más fuerte de toda la guía: no es
> una innovación, es aplicar a un path un patrón que ya existe en el repo.

#### [MEDIDO] El repo ya tiene el benchmark de esto, y confirma la ganancia

`search.rs:1578-1631` (`fingerprint_gate_cost`) **compara las dos estrategias
sobre los mismos 300 candidatos**: el enfoque por chunks del fingerprint gate
contra el bucle N+1 de referencia. Lo ejecuté:

```
cargo test -p tylluan-kernel --lib fingerprint_gate_cost -- --nocapture

T1277-E fingerprint_gate_cost: rows=1000 candidates=300 iters=100
  gate_p50=2013us gate_p95=2118us (min=1834us max=2841us)
  — reference get_node_sync loop p50=12889us
```

| Estrategia | p50, 300 candidatos |
|---|---:|
| `IN (...)` por chunks (gate) | **2.013 µs** |
| Bucle N+1 `get_node_sync` (referencia) | **12.889 µs** |

**≈6,4× más rápido** el materializado por chunks. A `limit=20`, el bucle N+1
representa ~860 µs de overhead por recall que pasarían a ser ~135 µs.

**Tres advertencias que hay que leer junto a la cifra** — no sirve de nada citar
el número sin ellas:
1. **Es un build de test (`debug`, sin optimizar).** Los absolutos mejorarían en
   release; el **ratio** es lo portable, y no está garantizado que sea idéntico.
2. **DB en memoria, sin I/O, caché caliente.** Producción tiene WAL + disco +
   contención por candado.
3. **Mide solo el paso de materializado**, no la query de ranking.

Sirve para decidir que **P1-2 va en la dirección correcta**. No sirve para prometer
un 6,4× en la latencia total del recall — esa medida la tiene que dar el bloque 8.

### ③ Grafo PPR — bien hecho, no tocar la estructura

`graph.rs:207-277` (`personalized_pagerank_local`): carga el subgrafo a 2 saltos
con **queries por chunks de 50** (`graph.rs:233-241`), limita a 200 nodos, y hace
PageRank **en memoria**. Es el diseño correcto: no hay N+1 en el bucle de iteración.

Dos detalles menores:
- `graph.rs:238`: `WHERE source IN (...) OR target IN (...)` — un `OR` entre dos
  columnas indexadas (`idx_edges_source`, `idx_edges_target`, `schema.rs:417-418`).
  SQLite puede hacer *index-union*, pero con 100 parámetros bound es la query más
  cara de la ruta. Un `UNION ALL` de dos selects explícitos es más predecible.
- `graph.rs:796`: vuelve el **N+1** en la fase de materialización.

### ④ FTS5 BM25 — el mejor tramo del camino

`search.rs:920-933`:

```sql
SELECT ... FROM nodes_fts f JOIN nodes n ON n.rowid = f.rowid
WHERE nodes_fts MATCH ?1
ORDER BY bm25(nodes_fts, 10.0, 5.0, 5.0)
LIMIT {max_results}
```

Una sola query, con pesos por columna explícitos (contenido 10, otros campos 5).
**Bien hecho.** El único riesgo es el fallback `LIKE '%term%'` (`search.rs:960,
969`) — `LOWER(content) LIKE ?1` no puede usar índice y escanea entero. Es
correcto como fallback, pero conviene saber que **un FTS5 mal construido cae a un
scan completo en silencio**.

### ⑤ Fusión RRF — clones completos y una asunción sin validar

`search.rs:412` fija `const K: f32 = 60.0`, y `search.rs:427-432`:

```rust
for (rank, (node, _score)) in vector_results.iter().enumerate() {
    let rrf = 1.0 / (K + rank as f32 + 1.0);
    rrf_scores.entry(node.id.clone())
        .and_modify(|e| e.1 += rrf)
        .or_insert((node.clone(), rrf));   // ← clona contenido + metadata completos
}
```

Dos observaciones:

1. **`node.clone()`** mete en el `HashMap` el nodo completo (content + metadata,
   ambos `String`). Con `source_limit = limit × 3` son hasta 60 nodos clonados.
   Se puede fusionar sobre IDs y materializar al final.
2. **[LITERATURA] La asunción de "tres votos iguales" no está validada.**
   El grafo entra con el mismo peso que BM25 y los vectores, sin que haya evidencia
   de que aporte igual. Ver bloque 6.

### ⑥ Filtro de archivados — otra ida a DB y sobre-fetch ×3

`search.rs:555-572`:

```rust
let source_limit = if include_archived { limit } else { limit.saturating_mul(3).max(limit) };
let (mut results, meta) = self.search_hybrid_detailed(..., source_limit, ...).await?;
if !include_archived {
    let archived_ids = self.archived_lifecycle_ids_among(&results...ids).await...;
    results.retain(|(node, _)| !archived_ids.contains(&node.id));
}
```

**Todo el pipeline corre a ×3 de trabajo** — 3× embeddings-candidatos, 3× N+1,
3× PPR, 3× clonado — **solo para descartar archivados después.**
`archived_ids` se recorre con `.contains()` sobre un `Vec` (o `HashSet` según
implementación); conviene verificar cuál es, porque si es `Vec` es O(n²).

### ⑦ Reranker — un segundo modelo ONNX en el camino caliente

`search.rs:793-811` + `config.rs:1264`: **`reranker_enabled: true` por defecto.**
Cada recall puede disparar un segundo modelo (`RerankEngine::load_with_device`,
cargado en `main.rs`) dentro de `block_in_place`. Esto es **CPU adicional por
consulta** y no aparece reflejado en el diagrama mental que suele tener el equipo.

---

## Bloque 4 — Las mejoras priorizadas

### P0-1 · Encender el coalescing batcher de embeddings

**Qué:** poner `[silva] embed_batching_enabled = true` en `tylluan.toml`.

**Por qué es P0:** la optimización **ya está escrita, probada y documentada**
(`embeddings.rs:47-131`, T582). No hay código nuevo que escribir. Es un flag.

**Evidencia:** `config.rs:1261` default `false`; `config.rs:2469-2471` test que la
fuerza apagada con nota *"F2 pendiente del gate de F1"*.

**Riesgo real y por qué el equipo la apagó a propósito.** El comentario de ADR-017
(`embeddings.rs:828-834`) menciona *"the contamination measured in the A/B baseline"*:
activar el batcher global mezcla el tráfico de background (reindexado) con el de
recall dentro de la misma ventana, y eso **midieron que contaminaba el benchmark
de latencia**. Por eso existe también `embed_batching_routing_enabled`
(`config.rs:1232`) para aislar la clase ROUTING.

> **Conclusión: no es un interruptor que se encienda y ya.** Es P0 *de medición*:
> encenderlo en el kernel de prueba, medir recall p50/p95 con y sin él, y
> comprobar si la contaminación del A/B sigue siendo real hoy. Si el reindexado
> estático (background) es el problema, la solución es **una ventana propia para
> reindexado**, no dejar el batcher apagado.

**Cómo verificar:** comparar p50/p95 de `tylluan_recall` bajo 4-8 clientes
concurrentes, con y sin el flag. Los contadores `sqlite_lock_acqs` y el tiempo de
inferencia ONNX deben bajar.

**No hacer:** activarlo en producción sin la medencia en kernel de prueba.

---

### P0-2 · Medir la espera del candado de SQLite antes de tocar nada más

**Qué:** leer `sqlite_lock_wait_snapshot()` (`mod.rs:281-287`) bajo carga real y
calcular `wait_ns / acqs`.

**Por qué es P0:** es **gratis** (ya está instrumentado) y decide si el bloque 2
es P0/P1 o P2. Es el experimento que hace barato todo lo demás.

**Cómo:** disparar N recalls concurrentes y leer el delta. Si la espera media por
adquisición es despreciable (<50µs), el candado no es el problema y el foco se va
a ① y ②. Si es de milisegundos, es el problema #1.

---

### P1-1 · Normalizar al escribir y hacer *dot product* puro al buscar

**[VERIFICADO]** `embed_batch` **ya normaliza L2** cada vector
(`embeddings.rs:947`, `978-981`), y la normalización **forma parte de la huella
del motor** (`embeddings.rs:462`, `("norm", "l2")`).

Pero `search_vector_linear` recalcula la norma almacenada **de cada fila, en cada
consulta** (`search.rs:191-198`). Para las filas del motor actual, esa norma es
**1.0 por construcción**. Es trabajo redundante en el loop más caliente del sistema.

**Qué:** garantizar normalización en escritura y, en el escaneo, saltar
`stored_norm_sq` (tratarla como 1.0) manteniendo el chequeo de dimensión. Quedaría
solo `dot`.

> **[LITERATURA]** Es la optimización estándar: "if vectors are unit-normalized
> when written, search reduces to a dot product. That avoids repeating vector
> norms at query time" — [sqlite-vector](https://github.com/sqliteai/sqlite-vector).

**Riesgo:** filas **legacy** (anteriores a la normalización) o de otra dimensión.
El código actual las defiende. Cualquier cambio debe **mantener la defensa** y
debe medirse contra `embedding_fingerprint_filter` (`config.rs:1209`).
**Requiere ADR porque toca un invariante de datos.**

---

### P1-2 · Matar los cuatro N+1 con `WHERE id IN (...)`

**Qué:** sustituir los tres bucles `get_node_sync(&id, &conn)` por **una** query
`WHERE id IN (?, ..., ?)` con el patrón de chunks que **ya existe** en el repo
(mismo truco que `fingerprint_dropped_ids`, `search.rs:101-103`, y que
`search_sparse_candidates` ya usa).

**Ganancia medida:** el propio benchmark del repo (`fingerprint_gate_cost`) da
**12.889 µs → 2.013 µs** para 300 candidatos (**≈6,4×**). De `limit` round-trips
(y `limit` `prepare()`) a 1 o 2. Con `limit=20`: **~20 queries → 1**.
Ver la tabla y las tres advertencias en el bloque 3-②.

**Riesgo bajo.** Es el cambio más limpio de la guía porque **el patrón ya está
aprobado en el código**. Además conviene cachear el `Statement` preparado en
`get_node_sync` o eliminarla por completo al pasar a `IN (...)`.

**Cómo verificar:** contar queries por recall con el log de SQLite, o medir
latencia p95. Los `sqlite_lock_acqs` deben caer de forma visible.

---

### P1-3 · Quitar el sobre-fetch ×3 del filtro de archivados

**Qué:** filtrar archivados **antes** de la fusión, o empujar el filtro al SQL,
en vez de pedir `limit × 3` y descartar después (`search.rs:555-572`).

**Ganancia:** reduce a **×3** todo el trabajo de la ruta (vector, PPR, clonado,
N+1). Es la palanca de multiplicación más grande de la guía: un 30% de mejora en
cada etapa aquí vale ×3 sobre el total.

**Riesgo:** cambio de comportamiento en el recuento final. Requiere test de
regresión con el corpus de `benchmarks/`.

---

### P1-4 · Separar escrito y lector en SQLite (arquitectura)

**Qué:** un **único escritor** + un **pool pequeño de lectores** (2-4 conexiones).

**Por qué:** es el único camino para que WAL rinda como promete. Hoy la lectura
serializa igual que la escritura.

> **[LITERATURA]** Remedios estándar: `deadpool-sqlite` (con `interact()`),
> `r2d2-sqlite` + `spawn_blocking`, o `sqlx-sqlite`. Medidas publicadas, con
> matices: un benchmark SQLx reporta **60.061 vs 2.586 rows/sec (~23×)** para
> *escrituras* con un solo escritor frente a un pool de 50 — es contención de
> escritura, **no** una promesa de lectura
> ([EMS](https://emschwartz.me/psa-your-sqlite-connection-pool-might-be-ruining-your-write-performance/)).
> Una app Go de 200 usuarios reportó **+65% de throughput solo con WAL** y
> **+14-121%** separando escritor de pool de lectores, con óptimo en 4 lectores
> ([dev.to](https://dev.to/eugenioenko/what-200-concurrent-users-taught-me-about-sqlite-performance-442j)).

**⚠️ Trampa crítica que el equipo debe conocer:** `PRAGMA cache_size` es
**por conexión** ([docs](https://www.sqlite.org/pragma.html#pragma_cache_size)).
Hoy: `-64000` ≈ **64 MB** (`schema.rs:12`). Con un pool de 8 conexiones →
**512 MB** de caché SQLite. **Hay que bajar `cache_size` al escalar el pool o se
pierde RAM.** Esto es el tipo de detalle que rompe producción si se ignora.

**Riesgo medio-alto.** `rusqlite::Connection` es `!Sync`, así que toca la superficie
de API de todo el layer de datos. **Requiere ADR y una fase larga.** No es un "5%",
es un "30% en concurrencia, con 3 semanas de riesgo".

---

### P2-1 · Revisar `page_size`, `busy_timeout` y `foreign_keys` (deuda de configuración)

Tres cosas que encontré de paso y que conviene que alguien decida:

1. **`PRAGMA page_size = 4096` es un no-op.** Se fija en `schema.rs:15` y
   `hybrid.rs:163` **después** de `journal_mode=WAL`.
   > **[LITERATURA]** "**page size cannot be changed while in WAL mode**"
   > ([WAL limitations](https://www.sqlite.org/wal.html),
   > [file format](https://www.sqlite.org/fileformat.html)). Además, 4096 es el
   > valor por defecto, así que no hay pérdida funcional — pero el código sugiere
   > un control que no existe. **Riesgo: que alguien lo "ajuste" en producción
   > creyendo que hace algo.**

2. **`busy_timeout` no está en SilvaDB ni en hybrid.** Está en el `open_db`
   compartido (`config.rs:1484`, 5000 ms) pero `silva/schema.rs:6-16` y
   `hybrid.rs:153-164` no lo fijan. Hoy da igual *dentro* del proceso (una sola
   conexión), pero **importa en cuanto haya pool** o si otra herramienta abre el
   fichero. Rusqlite documenta 5000 ms por defecto, pero debe ser explícito.

3. **`PRAGMA foreign_keys` nunca se habilita** — lo documentan los propios autores
   como problema abierto: `nodes.rs:1017` *"`PRAGMA foreign_keys` is never enabled
   on this connection"* y `maintenance.rs:440` *"(none exists; several
   doc-comments here even claim the opposite)"*. Los `ON DELETE CASCADE` dependen
   de flags del build vendido, no de una instrucción explícita. **Esto es deuda de
   integridad de datos, no de rendimiento**, y está reconocida sin cerrar.

---

## Bloque 5 — Qué NO tocar (y por qué gastar tiempo aquí es pérdida)

Por el criterio del encargo — *mejorar, no añadir* — estas tres cosas parecen
mejoras y **no lo son**:

1. **No migrar el escaneo vectorial a ANN.** Con 5.000 vectores de 1024-d float32
   son **20,5 MB** de payload. Un escaneo exacto SIMD AVX2 sobre 1M de vectores
   1024-d se mide en **0,325 s** → **~1,6 ms a 5k**
   ([LanceDB](https://www.lancedb.com/blog/my-simd-is-faster-than-yours-fb2989bf25e7)).
   > **[LITERATURA]** "At 5,000 vectors, I would keep exact search for now—but
   > stop fetching and decoding thousands of SQLite blobs on every query if that
   > cost is material." El punto de inflexión razonable para empezar a medir ANN
   > es **~10.000** vectores ([Qdrant](https://qdrant.tech/course/essentials/day-2/what-is-hnsw/)).
   > **El problema hoy no es el algoritmo, es el round-trip a SQLite por fila (P1-2).**

2. **No reescribir el PPR.** `graph.rs:207-277` ya está bien: chunks, límite de
   200 nodos, PageRank en memoria. Solo sufre del N+1 al materializar (P1-2).

3. **No tocar el FTS5.** Es el mejor tramo del camino (`search.rs:920-933`).

---

## Bloque 6 — Contraste con literatura

### RRF con K=60

**[LITERATURA]** Cormack et al. (SIGIR 2009) eligieron 60 en un *pilot* y lo
describieron como "near-optimal" pero **"not critical"**. No es una constante
teórica.
([paper](https://cormack.uwaterloo.ca/cormacksigir09-rrf.pdf),
[registro Google Research](https://research.google/pubs/reciprocal-rank-fusion-outperforms-condorcet-and-individual-rank-learning-methods/))

Elastic, barriendo hiperparámetros, encontró **K=20, depth N=1000** óptimos, pero
**la diferencia entre la mejor y la peor combinación probada fue de solo ~5%** en
NDCG@10. Es decir: **K=60 es defendible, y afinarlo no es donde está el dinero.**
([Elastic](https://www.elastic.co/search-labs/blog/improving-information-retrieval-elastic-stack-hybrid))

**Lo que sí importa para Tylluan:** el grafo-PPR entra como **un tercer voto de
igual peso** que BM25 y vectores (`search.rs:412,427-432`) **sin ninguna evidencia
de que aporte igual de bien**. Bruch, Gai & Ingber (2023) reportan que una
combinación convexa afinada **supera a RRF** tanto en dominio propio como ajeno,
y que el RRF afinado generaliza mal fuera de dominio
([arXiv:2210.11934](https://arxiv.org/abs/2210.11934)).

**Recomendación concreta y barata:** antes de cualquier cambio de algoritmo,
**separar la contribución de cada fuente en el telemetría que ya existe**
(`SearchMeta.graph_contrib`, `search.rs:444-448`). Si resulta que el grafo aporta
poco o mucho, eso es un dato, no una opinión. **Ya está cableado, solo falta leerlo.**

### Sobre el reranker

**[HIPÓTESIS]** Con `reranker_enabled=true` por defecto y un segundo modelo ONNX
en el camino, **no tengo medida de cuánto aporta a la calidad ni cuánto cuesta en
latencia**. Es el candidato más probable a que "desactivarlo mejore velocidad sin
perder calidad" — o exactamente al revés. **Métrica pendiente, no asumir.**

---

## Bloque 7 — Deuda detectada de paso (no es rendimiento, pero es riesgo)

| Hallazgo | Evidencia | Severidad |
|---|---|---|
| `--config` con ruta relativa se descarta en silencio | `main.rs:469-473` (chdir) + `main.rs:527` (read relativo) → `warn!` + defaults | **Alta.** Con `--config relativo` y sin `--port` el kernel cae al puerto 47004 (producción). Misma causa que la incidencia 2026-10-11, aún abierta en origen. Detalle en `IMPL_b11_supervivencia_jwt_buffy_2026-10.md` |
| `PRAGMA foreign_keys` nunca habilitado | `nodes.rs:1017`, `maintenance.rs:440` | **Alta** (integridad). Reconocida, no cerrada |
| Banner de arranque miente el config cargado | `main.rs` banner: `Config  : tylluan.toml` escrito a mano | Media. Complica toda diagnóstico |
| `PRAGMA page_size` inefectivo | `schema.rs:15`, `hybrid.rs:163` tras `journal_mode=WAL` | Baja funcional, **media** por confusión |

---

## Bloque 8 — Cómo medir (obligatorio antes de aceptar cualquier mejora)

Sin esto, "5% de mejora" es una opinión. El repo **ya tiene** el harness:
`tylluan-evals` (3.978 LOC) con `paired_eval.rs` y `benchmarks/`.

**Métrica primaria:** p50 y p95 de `tylluan_recall` con latencia extremo a extremo.

**Protocolo mínimo:**
1. Corpus de queries representativas (ya existen `benchmarks/datasets/`).
2. 3 corridas por configuración, descartar la primera (calentar ONNX y page cache).
3. Concurrency 1, 4 y 8 — **la serialización solo se ve con concurrencia**.
4. Registrar además: `sqlite_lock_acqs` / `sqlite_lock_wait_nanos` (ya existen),
   y el tiempo de inferencia ONNX.
5. Reportar **mediana y p95**, nunca media (la distribución es larga).

**Guardrail de calidad:** toda mejora de velocidad debe acompañarse de
**Recall@5 sin regresión** sobre el corpus. Si la latencia baja y el recall baja,
no es una mejora.

---

## Resumen ejecutivo — orden de ataque

| # | Mejora | Prioridad | Coste | Riesgo | Requiere |
|---|---|---|---|---|---|
| 1 | Medir espera del candado SQLite | **P0** | Nulo | Nulo | Nada |
| 2 | Medir batcher ONNX encendido | **P0** | Bajo | Bajo | Kernel de prueba |
| 3 | N+1 → `IN (...)` en **4** sitios | **P1** | Bajo | Bajo | **Ya medido** 6,4× en el paso |
| 4 | Quitar sobre-fetch ×3 | **P1** | Medio | Medio | Test de regresión |
| 5 | Dot product puro (norma precalculada) | **P1** | Medio | Medio | **ADR** (invariante) |
| 6 | Leer `graph_contrib` del telemetría | **P1** | Bajo | Nulo | Nada |
| 7 | Pool lector + único escritor | **P2** | Alto | Alto | **ADR** |
| 8 | Decidir `page_size`/`busy_timeout`/`FK` | **P2** | Bajo | Bajo | Decisión |

**Los ítems 1, 2 y 6 cuestan casi nada y deciden hacia dónde van el resto.**
Si tuviera que defender una sola recomendación: **empieza por medir, no por
reescribir** — el repo ya tiene la telemetría y el harness; lo que falta es
leerlos de forma sistemática.

---

## Limitaciones de este documento

- **Es mayoritariamente análisis de código, no medición.** Una sola cifra está
  medida (P1-2, el N+1, vía el benchmark que ya existe en el repo). El resto son
  hipótesis y números de literatura de **otros sistemas y otros hardware**. Por eso
  el bloque 8 es obligatorio, no opcional.
- **Nota sobre el build de la medida:** se corrió en perfil `debug` sin optimizar.
  Es suficiente para decidir dirección, no para prometer tiempos absolutos.
- **No cubre "línea a línea" el 89% del kernel.** Cubre los caminos que deciden la
  velocidad, que es lo que sirve al objetivo. El resto está inventariado en el
  bloque 1 para futuras tandas.
- **No analicé a fondo** el dispatch de guilds (`guild_process.rs`, 2.050 LOC) ni
  el dashboard. Quedan para otra tanda.
- **El `reranker` está detectado pero no medido.** No afirmo que sea lento.

**No se ha modificado ningún fichero de código.** Este documento es el único artefacto nuevo.
