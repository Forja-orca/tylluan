# H-41 — Plan de verificacion en vivo de H-40b tras reinicio de produccion (hermes)

**Asignacion:** T1972 (TL claude-code). **Base:** `b668497` (B1 ya integrada: commit `e123c37`, PUSH181). **Tipo:** SOLO LECTURA — ni escrituras a silva.db, ni senales al kernel, ni recompilacion contra E:\tylluan. Fecha: 2026-10-28.

Riesgo que cierra (TL, T1969): "tras reiniciar produccion hay que comprobar en vivo que `engine_id()` coincide con el `model_name` guardado (si no, el gate semantico quedaria sin efecto)". B1 filtra el batch del gate por `model_name = engine_id()`; si el id del motor en marcha no coincide con el de las filas, la comparacion no encuentra filas y el gate semantico se vuelve inocuo en silencio.

## Paso 0 — El binario en marcha lleva B1 ( antes de nada )

```bash
curl -s http://127.0.0.1:47004/health    # el health reporta el commit cargado
# sea SHA ese commit:
git -C E:/tylluan show <SHA>:crates/tylluan-kernel/src/memory/silva/nodes.rs | grep -c "model_name: Option<&str>"   # esperado: 1
```
Si da 0, el binario es anterior a PUSH181: parar el plan y avisar (nada que verificar). **Demostracion en vivo 28-oct 11:5x:** el kernel en marcha reporta `commit: a564880` y `git show a564880:...nodes.rs | grep -c` = **0** → el gate del paso 0 corta aqui hasta que produccion se reinicie con un binario post-`e123c37` (el TL anuncio ese reinicio; todavia no ha ocurrido).

## Paso 1 — engine_id esperado del motor en marcha

`engine_id()` = `format!("{}-v2-onnx", self.model_type)` (`crates/tylluan-kernel/src/router/embeddings.rs, líneas 1248 a 1250`, leido en `b668497`). El `model_type` sale de config:

```bash
grep -n "embedding_model" E:/tylluan/tylluan.toml   # hoy: :173 embedding_model = "mxbai-embed-large"
```
Engine_id esperado hoy: **`mxbai-embed-large-v2-onnx`**. Si alguien cambia la config, el esperado es `«modelo»-v2-onnx` literal.

## Paso 2 — Distribucion real de model_name en silva.db (solo lectura, sin tocar el fichero vivo)

Este host NO tiene CLI `sqlite3` (verificado 28-oct); usar python (modulo stdlib), URI readonly:

```bash
python -c "import sqlite3; c=sqlite3.connect('file:E:/tylluan/data/silva.db?mode=ro&immutable=1', uri=True); \
[print(r) for r in c.execute('SELECT model_name, COUNT(*) FROM node_embeddings GROUP BY model_name ORDER BY 2 DESC')]"
```
- OK: la fila DOMINANTE == engine_id del Paso 1. Foto REAL medida hoy 28-oct 11:5x con ese comando: `mxbai-embed-large-v2-onnx` **4184** / `bge-m3-v2-onnx` 1087 / `distill-v1` 785 / `bge-m3` 6. Los 1.878 legacy quedan inertes para el gate por B1 — no son un error; se re-embeddean si el TL aprueba B3.
- NO-OK (riesgo del TL): dominante != engine_id → el gate semantico no esta comparando nada. Remediacion: revisar `embedding_model` en tylluan.toml (o plan B3). NO tocar nada mas.

## Paso 3 — Nueva escritura usa el id correcto (opcional, tras el reinicio)

Tablas reales (listadas hoy via `sqlite_master`): `nodes` y `node_embeddings(node_id, embedding, model_name, model_hash, dimensions)`.

```bash
python -c "import sqlite3; c=sqlite3.connect('file:E:/tylluan/data/silva.db?mode=ro&immutable=1', uri=True); \
print([r[1] for r in c.execute('PRAGMA table_info(nodes)')]); \
[print(r) for r in c.execute('SELECT model_name, COUNT(*) FROM node_embeddings WHERE rowid > (SELECT MAX(rowid)-50 FROM node_embeddings) GROUP BY 1')]"
```
Las filas nuevas post-reinicio deben traer el engine_id del Paso 1. [no verificado: columnas de `nodes` (el print las muestra en seco; el join por fecha queda sin anclar) y `rowid` como proxy de novedad — puede mezclar re-writes.]

## Paso 4 — Tasa de penalizacion del gate antes/despues (kernel.log)

Linea de referencia (medida en H-40, gawk sobre los 3 rollados): **62.955 penalizados / 63.711 candidatos = 98,8 %**, formato literal `Coherence Gate: X eliminated, Y penalized of Z candidates`.

```bash
# ANTES: rollados previos al reinicio. DESPUES: kernel.log post-reinicio.
for F in E:/tylluan/logs/kernel.log; do
  gawk 'match($0, /([0-9]+) eliminated, ([0-9]+) penalized of ([0-9]+) candidates/, m) {e+=m[1]; p+=m[2]; c+=m[3]}
        END {printf "%s: eliminados %d  penalizados %d / %d = %.1f%%\n", FILENAME, e, p, c, (c?100*p/c:0)}' "$F"
done
ls -la E:/tylluan/logs/kernel.log*   # identificar el corte por mtime del reinicio
```

**Medicion ANTES en vivo (comando gawk ejecutado hoy 28-oct 11:59, kernel.log actual, binario pre-B1):** `51 eliminados, 5421 penalizados / 5580 = 97,2 %` (el log vivo del dia; el 98,8 % de H-40 era sobre los 3 rollados completos — ambas cotas valen como referencia segun el ambito).

**Lectura esperada para no dar falsa alarma:** la tasa global NO deberia caer a ~0: B1 retira solo las penalizaciones cross-model (el subconjunto de los 1.878 legacy y de comparaciones con modelos retirados); el umbral 0.85 + scope `all` sigue aplastando pares mxbai-vs-mxbai legitimos con coseno 0.68-0.84 (eso es B4/B2, pendientes). Senal de alarma real: tasa DESPUES igual o mayor que la de ANTES con paso 0 y 2 en verde → revisar engine_id de nuevo. [no verificado: cuanto bajas exactamente la tasa global — requiere la medicion post-reinicio real.]

## Que NO hace este plan

No toca el kernel vivo (sin senales, sin restart, sin recall activo: una llamada `tylluan_recall` seria lectura funcional pero el TL dijo "sin tocar el kernel vivo" y no la incluyo). No escribe en la DB (URI `mode=ro&immutable=1`). No recompila.

## Impact

Ningun cambio de codigo; el plan solo lee: HTTP health, config, DB via URI readonly y logs. Riesgo de ejecucion: cero escritura; `immutable=1` evita hasta el WAL/shm junto al fichero. Si gawk no existe en el host, fallback: `grep -o "… "` + awk clasico sin match() en 3 pasadas.

## No verificado / limites

- [no verificado] Formato exacto de linea de arranque para el corte antes/despues: uso mtime de rollados como proxy; el nombre exacto de roll (kernel.log.1 vs fecha) no lo confirme en `b668497`.
- [no verificado] Schema completo de la tabla de nodos para el paso 3 (join por novedad); el proxy rowid puede mezclar re-writes.
- [no verificado] Distribucion post-reinicio: la foto es pre-B1-medicion (28-oct); si el TL ejecuta B3 cambia y este plan sigue valiendo (la dominante debe seguir siendo el engine_id).
- Distribuciones y conteos citados (3.943/1.087/785/6, 98,8 %) son las mediciones H-40 de esta misma semana sobre copia readonly — verificables con los mismos comandos.
