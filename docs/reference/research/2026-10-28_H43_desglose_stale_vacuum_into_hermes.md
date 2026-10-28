# H-43 — Desglose exacto de los ~6400 stale + VACUUM INTO en seco (hermes)

**Asignación:** T1985 (TL claude-code). **Base:** `2622acb` (PUSH183). **Tipo:** solo lectura; cero escrituras en producción — todo corrió sobre una **copia en mi worktree**. Fecha: 2026-10-28 ~13:5x local. Autor: hermes.

## Método y materiales

Copia consistente de la BD viva con la backup API de sqlite (fuente abierta readonly; solo escribe el destino):

```python
src = sqlite3.connect("file:E:/tylluan/data/silva.db?mode=ro", uri=True)
dst = sqlite3.connect("silva.h43-copy.db")   # en D:/hermes-projects/tylluan-wt-hermes
src.backup(dst)
```
Salida literal: `backup() OK en 2.0s` (203.730.944 B). La copia respeta la convención: la BD vivo no se abre en modo escritura.

Hash vigente del engine, obtenido por proxy sobre la copia (hash dominante en las últimas 200 filas de `node_embeddings`): **`sha256:310861041a33d9f4a8cb02bb94333ec530d4364159cf4a039c4a8459bb665754`** (199/200). `engine_hash()` es un fingerprint SHA-256 de ficheros en disco (embeddings.rs:1252-1260); las filas sin fingerprint se marcan `unknown-pre-hash` (nodes.rs:124) y el kernel **no** las cuenta stale por hash — deliberado, nodes.rs:121.

## (a) Desglose exacto — repliqué el WHERE de `get_stale_embeddings` (nodes.rs:644-666) tal cual, sparse incluido (`hybrid_sparse_enabled = true`, toml:287)

Salidas literales de la consulta sobre la copia:

```
TOTAL stale (DISTINCT, misma fórmula del kernel): 6371
  A) sin embedding:            4493
  B) model_name != engine_id:  1870   (bge-m3-v2-onnx 1083 / distill-v1 781 / bge-m3 6)
  C) hash real != vigente:        0
  D) sin firma sparse:         4501   (de ellos 4493 son tambien A; solo-sparse = 8)
suma condiciones 10864 > total => solapamiento A∩D=4493, B∩D=0, C=0
universo: nodes 10468 | node_embeddings 5975 | sparse 5967
```

Referencia del kernel vivo a las 13:35:05: `Found 6571 stale` — mi copia a ~13:5x da 6371; el delta (~200) es el reindexer comiendo ~100/tick y writes nuevos entre la línea del log y mi snapshot. Los nombres legacy ya bajaron solos desde la foto de H-41 (1087→1083, 785→781): el reindexer SI progresa en B.

**Corrección a mi estimación de H-42** (allí dije «~4.184 filas mxbai con hash viejo»): **C=0**. Esas 4.184 filas mxbai tienen hash vigente (3.975) o NULL/`unknown-pre-hash` (2.000, toleradas por diseño). La composición real es otra:

**El 71 % del stale son nodos que NUNCA tuvieron embedding** (4.493 de 10.468 nodos = 43 % del grafo sin vector). Por tipo:

```
SIN embedding:  code_entity 1851 | coloquio_memory 962 | session_digest 792 |
                episode 511 | routing_anchor 152 | summary 108 | agent_summary 50 | synthesis 20
CON embedding:  episode 2120 | agent_summary 1557 | coloquio_memory 690 | lesson 497 | routing_anchor 347
```

Consecuencia operativa para la decisión B3: la cola del reindexer está dominada por tipos de escritura continua (`code_entity`, `coloquio_memory`, `session_digest`); a 100/tick con este flujo, nunca converge y además los `episode` sin vector llevan meses esperando detrás de la cola. Hacer configurable el tope (opción B de H-42) es necesario pero quizá no suficiente: **decisión de diseño para TL/Jose** (no la tomo yo) es si todos los tipos deben tener embedding — un filtro `WHERE n.type NOT IN (...)` en `get_stale_embeddings` es un cambio de una línea con semántica opuesta al raise del tope. Los 2.000 mxbai `unknown-pre-hash` seguirán tolerados por diseño (no cuentan).

## (b) VACUUM INTO en seco sobre la copia

```python
c = sqlite3.connect("silva.h43-copy.db")
c.execute("VACUUM INTO 'silva.h43-vacuum-test.db'")
```
Salida literal: `VACUUM INTO OK en 2.3s` → `integrity_check: ('ok',)` → conteos idénticos tras el vacuum `(node_embeddings 5975, nodes 10468, sparse 5967)` → tamaño `203730944 -> 166338560` B.

Veredicto: el esquema de silva.db **soporta VACUUM INTO con python-sqlite standard** (no hay obstáculo SQLCipher en esta BD — se abre sin key; el feature encryption no está activo en esta instalación, coherente con que H-41 la abriera readonly). Bonus medido: la BD tiene ~37 MB (18 %) de paginas libres. Para el backup pre-B3 real sobre la BD VIVA hace falta aún la prueba contra el proceso del kernel en marcha (la copia no replica la concurrencia WAL): [no verificado] — siguiente paso natural si Jose aprueba B, ejecutar `VACUUM INTO` con el kernel corriendo (escritura solo del fichero nuevo; no toqué producción).

## Impact

Producción: cero escrituras, cero procesos; solo lectura readonly de la BD y del log. Worktree: dos ficheros `.db` temporales (gitignored, `*.db` en .gitignore:13) — **borrados tras redactar este informe**; todo lo medido queda con salida literal aquí. Corrige una afirmación de H-42 publicada en main (componente «hash viejo» = 0; composición real = nunca-embedeados 71 %).

## No verificado / límites

- [no verificado] Snapshot de la copia (13:5x) vs contaje del kernel (13:35): los números exactos de A/B/D se mueven tick a tick; la ESTRUCTURA (71 % nunca-embedeados, C=0, legacy ≈ 1870) es estable.
- [no verificado] Por qué los `code_entity`/`session_digest`/`coloquio_memory` no se embedean al escribir (falta de llamada a save_embedding en esas rutas, o fallo ONNX silencioso bajo carga): no leé las rutas de creación de esos tipos — es lead para la próxima tarea, no afirmación.
- [no verificado] VACUUM INTO contra la BD viva con WAL activo (la prueba en seco fue sobre copia).
- Hash vigente obtenido por proxy estadístico (199/200), no por lectura del engine en marcha; el engine podria reportar fingerprint distinto si los ficheros en disco cambiaran desde las últimas escrituras.
