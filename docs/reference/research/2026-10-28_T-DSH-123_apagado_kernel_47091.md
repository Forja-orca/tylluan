# T-DSH-123 — Apagado del kernel de PRUEBA en 47091: tiempo entre POST /admin/shutdown y salida real

- Agente: dsh
- Fecha: 2026-10-28
- Encargo (T2022): kernel de PRUEBA (nunca prod), puerto 47091, DATA_DIR propio vacio, medir 5 veces el tiempo entre POST /api/v1/admin/shutdown y la salida real, en reposo y con embeddings en vuelo; tabla de segundos y veredicto sobre la hipotesis del TL (el runtime de tokio espera a los spawn_blocking ONNX del reindexer). Solo informe, sin tocar codigo.

## Montaje (verificado)

- Binario: copia de `E:\cargo-target\prod-build\release\tylluan-nexus.exe` (34.006.528 B, 28/10 18:55) en `E:\tylluan-targets\dsh-t7-data\t123\tylluan-nexus.exe`. **Nunca** el kernel de produccion (:47004, PID 15960 intacto).
- Config propia (`t123\tylluan.toml`): `[nexus] dev_mode=true, host=127.0.0.1, port=47091, port_retry_secs=0`; `[memory] embedding_model="bge-m3"` (1024d, cache fastembed del home); `[silva] db_path=./data/silva.db`; mesh/p2p/gossip/federation desactivados; sentinel off; guilds always_on=[].
- DATA_DIR propio (`t123\data`) borrado antes de cada corrida (con reintento por locks de SQLite).
- Metodo: `Start-Process` (con `-WindowStyle Hidden`) → esperar `/health` → [caso busy] generar carga → `POST /api/v1/admin/shutdown` → sondear `HasExited` cada 20 ms hasta salida real (tope 300 s). Fases medidas tambien por timestamps del log: peticion shutdown -> "Shutting down gracefully" -> "TylluanNexus stopped" -> salida.
- Scripts: `t123\measure.ps1`, `measure_busy2.ps1`, `measure_busy3.ps1`; resultados crudos en `t123\results.jsonl`, `results2.jsonl`, `results3.jsonl`; logs `stdout-<tag>.log`.

## Hecho de fase (clave, en todas las corridas)

`POST /admin/shutdown` -> "TylluanNexus stopped" tarda **0,01 s**; **todo el retardo ocurre DESPUES del log "stopped"**, en el desmontaje del runtime. Confirma el hecho observable del TL (el log decia "stopped" y el proceso seguia vivo), pero localiza la espera despues de la parada ordenada, no en el trabajo de apagado.

## Tabla — reposo (5 + 3 de confirmacion, n=8)

| corrida | segundos (shutdown -> salida) |
|---|---|
| idle-1 | 24,60 |
| idle-2 | 12,31 |
| idle-3 | 14,70 |
| idle-4 | 14,41 |
| idle-5 | 11,68 |
| idle-6 | 13,33 |
| idle-7 | 11,29 |
| idle-8 | 10,56 |

media 14,11 s; mediana 12,82 s; rango 10,56-24,60 s. (Una corrida previa de idle-2 con datos sin limpiar dio 18,68 s; se descarta y se repitio limpia.)

## Tabla — con embeddings en vuelo

### (a) 30 nodos secuenciales + POST reindex, y shutdown (n=5)
En este montaje el handler `POST /api/v1/silva/node` **ya embebe** cada nodo (linea 900-910 de `api_v1/mod.rs`), asi que al llegar al reindex solo quedaban 2 stale: no habia lote en vuelo al apagar.

| corrida | stale | segundos |
|---|---|---|
| busy-1 | 2 | 11,91 |
| busy-2 | 2 | 8,59 |
| busy-3 | 2 | 7,73 |
| busy-4 | 2 | 9,37 |
| busy-5 | 2 | 7,80 |

media 9,08 s.

### (b) 8 creaciones de nodo CONCURRENTES (ONNX dentro del handler) y shutdown 250 ms despues (n=6 validas)
| corrida | exit s | completadas/fallidas |
|---|---|---|
| busy2-2 | 14,07 | 2/0 |
| busy2-3 | 27,30 | 2/6 |
| busy2-4 | 30,46 | 5/3 |
| busy2-5 | 26,53 | 4/4 |
| busy2-6 | 14,03 | 2/1 |
| busy2-7 | 25,82 | 2/6 |

media 23,04 s; mediana 26,18 s. (busy2-1 se descarta: arranco con el puerto aun ocupado, /health respondio de un kernel anterior y se mato a los 300 s.)

### (c) Reindex con lote real: 37-42 nodos SIN embedding en la DB (borrados con sqlite3 entre corridas)
| corrida | stale | retardo antes de shutdown | exit s |
|---|---|---|---|
| busy3-1 | 37 | 0 ms | 11,01 |
| busy3-2 | 40 | 0 ms | 10,16 |
| busy3-3 | 42 | 1500 ms | 33,22 |
| busy3-4 | 40 | 3000 ms | 17,39 |

## Veredicto sobre la hipotesis

- **Confirmado**: el proceso sobrevive **10-33 s** despues del log "TylluanNexus stopped"; el retardo esta en el desmontaje final del runtime/proceso, no en la parada ordenada (0,01 s hasta "stopped"). No se reprodujeron los >85 s del TL (maximo medido 33,2 s).
- **Refutada tal como esta formulada** (el runtime espera a los `spawn_blocking` ONNX del reindexer):
  1. En **reposo**, sin ningun reindex, el retardo ya es 10,56-24,60 s (media 14,1 s).
  2. Con reindex disparado y shutdown inmediato (37-40 stale), la salida es **10-11 s**: el lote del reindexer no anade una espera proporcional.
  3. Con 1,5 s de retardo (lote claramente en vuelo) sale en 33,2 s, muy por debajo de lo que quedaria de lote (~40 nodos x ~1,3 s ≈ 50 s); con 3 s de retardo sale en 17,4 s. No hay correlacion proporcional.
- **Explicacion mas probable (NO verificada)**: el retardo es el desmontaje de la sesion ONNX/ORT y/o el join de hilos del blocking pool/workers al caer el runtime; el caso (b) (ONNX sincrono dentro de workers HTTP, 14-30 s) apunta a hilos de trabajo atascados en inferencia, no al reindexer.

## Impact

- El "proceso vivo tras el log stopped" queda **medido y localizado**: es el desmontaje final (10-33 s), no el apagado ordenado. Eso cambia el sitio donde hay que actuar (runtime/ORT/hilos), no la parada de guilds ni el token de cancelacion.
- La hipotesis del reindexer como causa principal **no se sostiene** con estos datos; si se quiere atribuir con certeza hace falta instrumentacion (volcado de hilos durante la espera, tiempos de drop de la sesion ONNX).

## No verificado

- Un solo binario/config (prod-build 18:55, bge-m3 porque mxbai no estaba en cache; en prod es mxbai) y 8+6+5+4 muestras; no se probo el binario de produccion ni carga real.
- No se hizo volcado de hilos/stack durante el retardo: la causa ORT/blocking-pool es **hipotesis**, no medida.
- Los >85 s del TL no se reprodujeron; puede depender de estado real (DB grande, reindexer en curso de verdad, guilds), no cubierto aqui.
- Timings con alta varianza (10-33 s) incluso en reposo; las medias son orientativas.
- No se toco codigo, ni `E:\tylluan`, ni el kernel de produccion (:47004).
