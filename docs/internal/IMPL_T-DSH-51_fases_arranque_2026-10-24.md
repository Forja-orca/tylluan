# T-DSH-51 — Fases del arranque del kernel: qué queda tras D-39a

- **Agente:** dsh
- **Fecha:** 2026-10-24
- **Base:** `origin/main` `06bfcc1`. D-39a (repo map acotado) va en el push 125 del TL; aquí se mide el arranque **ANTES** de D-39a y se proyecta el después.
- **Encargo (T1604):** reconstruir con `E:\tylluan\logs\kernel.log` una tabla «fase de arranque → segundos» para los 3 arranques del 22-23 oct (config, integridad de BD, sondeo ONNX, migraciones, modelos, spawn de guilds, repo map, bind HTTP, auto-link, primer ciclo de sync), citando la línea de inicio y fin de cada fase; indicar qué fases siguen siendo > 5 s sin el repo map y cuáles se podrían paralelizar o diferir.
- **Entregable:** `scripts/measure_startup_phases.py` (nuevo, solo lectura) + este informe. Sin tocar `src/`, guilds ni config; sin lanzar nada contra el kernel ni motores.

## 1. Método

`scripts/measure_startup_phases.py --log logs/kernel.log --since 2026-10-22 --until 2026-10-23`:

- Delimita cada arranque entre `TylluanNexus kernel started (headless mode)` y la línea `Windows job object set` del siguiente.
- Localiza anclas por mensaje literal del log (no por tiempo) y calcula `fin - inicio`; la fase `repo_map` usa la **duración que el propio kernel reporta** (`Repo map built: … (Nms)`), así que su inicio se deriva (fin − N ms).
- Fases multi-evento: `guild_embeddings` y `spawn_always_on` van de su primera a su última aparición antes del repo map; `auto_link` de «Starting CERO-LLM» a «CERO-LLM complete».

Los 3 arranques del encargo (el log tiene uno más anterior, 18:10, que descarto) son los que preceden a los tres `Repo map built` de 23:04:43, 23:53:47 y 01:13:40.

## 2. Tabla por arranque (segundos)

### Arranque A — inicio 22:48:07.523, bind HTTP 23:04:43.320 (16 m 35,8 s)

| fase | inicio (línea del log) | fin (línea) | segundos |
|---|---|---|---:|
| config | 22:48:07.577 (L200223) | 22:48:07.601 (L200225) | 0,02 |
| integridad BD | 22:48:08.112 (L200231) | 22:48:10.870 (L200232) | 2,76 |
| init memoria (schema+migrations+mailbox) | 22:48:11.073 (L200239) | 22:48:11.101 (L200246) | 0,03 |
| carga modelo embeddings | 22:48:11.862 (L200254) | 22:48:18.050 (L200258) | 6,19 |
| embeddings de guilds (45) | 22:48:18.050 (L200259) | 22:48:22.942 (L200303) | 4,89 |
| reranker | 22:48:23.044 (L200304) | 22:48:23.726 (L200306) | 0,68 |
| registry + transports | 22:48:23.747 (L200310) | 22:48:23.838 (L200337) | 0,09 |
| **spawn always-on (13 guilds, background)** | 22:48:23.836 (L200328) | 22:53:43.864 (L201017) | **320,03** |
| **repo map** | inicio derivado 22:48:24.124 | 23:04:43.183 (L202026) | **979,06** |
| **bind HTTP** | 23:04:43.320 (L202030) | — | — |
| auto-link (background, concurrente) | 22:48:58.443 (L200385) | 22:49:53.425 (L200820) | 54,98 |
| federation task + primer ciclo | 23:04:43.204 (L202027) | 23:04:48.224 (L202033) | +5,02 |

Este arranque no registró las líneas de `ONNX Runtime compat probe` ni `fingerprint sha256` (posible caché caliente / ruta distinta): la fase ONNX no es medible aquí.

### Arranque B — inicio 23:37:02.841, bind HTTP 23:53:47.984 (16 m 45,1 s)

| fase | inicio (línea del log) | fin (línea) | segundos |
|---|---|---|---:|
| config | 23:37:02.841 (L203612) | 23:37:02.844 (L203614) | 0,00 |
| integridad BD | 23:37:03.242 (L203620) | 23:37:05.696 (L203621) | 2,45 |
| init memoria | 23:37:05.786 (L203628) | 23:37:06.267 (L203634) | 0,48 |
| **sondeo ONNX → motor sparse BGE-M3 listo** | 23:37:06.277 (L203635) | 23:37:14.496 (L203638) | **8,22** |
| **carga modelo mxbai** | 23:37:14.547 (L203647) | 23:37:20.255 (L203652) | **5,71** |
| **fingerprint del modelo (sha256)** | 23:37:20.255 | 23:37:35.721 (L203653) | **15,47** |
| **embeddings de guilds (45)** | 23:37:35.721 (L203654) | 23:37:59.957 (L203698) | **24,24** |
| reranker | 23:38:00.477 (L203701) | 23:38:01.699 (L203703) | 1,22 |
| registry + banner «operational» | 23:38:01.707 (L203707) | 23:38:01.893 (L203739) | 0,19 |
| **spawn always-on (background)** | 23:38:01.877 (L203729) | 23:44:42.475 (L204427) | **400,60** |
| **repo map** | inicio derivado 23:38:01.909 | 23:53:47.899 (L204490) | **945,99** |
| **bind HTTP** | 23:53:47.984 (L204498) | — | — |
| auto-link (background) | 23:38:11.044 (L204071) | 23:38:26.088 (L204155) | 15,04 |
| federation task + primer ciclo | 23:53:47.906 (L204491) | 23:53:52.911 (L204502) | +5,01 |

### Arranque C — inicio 00:47:28.727, bind HTTP 01:13:40.116 (26 m 11,4 s)

| fase | inicio (línea del log) | fin (línea) | segundos |
|---|---|---|---:|
| config | 00:47:28.728 (L205836) | 00:47:28.731 (L205838) | 0,00 |
| integridad BD | 00:47:29.084 (L205844) | 00:47:31.482 (L205845) | 2,40 |
| init memoria | 00:47:31.593 (L205852) | 00:47:31.635 (L205857) | 0,04 |
| **sondeo ONNX → sparse listo** | 00:47:31.646 (L205858) | 00:47:38.685 (L205861) | **7,04** |
| carga modelo mxbai | 00:47:39.981 (L205870) | 00:47:44.767 (L205875) | 4,79 |
| **fingerprint del modelo** | 00:47:44.767 | 00:47:57.098 (L205876) | **12,33** |
| **embeddings de guilds (45)** | 00:47:57.099 (L205877) | 00:48:07.545 (L205921) | **10,45** |
| reranker | 00:48:07.764 (L205924) | 00:48:08.670 (L205926) | 0,91 |
| registry + banner | 00:48:08.695 (L205930) | 00:48:08.871 (L205962) | 0,18 |
| **spawn always-on (background)** | 00:48:08.867 (L205951) | 00:51:56.140 (L206633) | **227,27** |
| **repo map** | inicio derivado 00:48:08.915 | 01:13:40.059 (L206750) | **1531,14** |
| **bind HTTP** | 01:13:40.116 (L206755) | — | — |
| auto-link (background) | 00:49:07.066 (L206110) | 00:49:26.768 (L206204) | 19,70 |
| federation task + primer ciclo | 01:13:40.084 (L206751) | 01:13:45.101 (L206759) | +5,02 |

**El repo map es el 94-98% del tiempo hasta el bind** (979,1/995,8 s; 946,0/1005,1 s; 1531,1/1571,4 s). Todo lo demás, sumado en serie, son 16,3 s (A), 59,1 s (B) y 40,2 s (C) hasta el banner «operational».

## 3. Qué queda tras D-39a (repo map ≈ 2 s)

Con el repo map en ~2 s, el bind ocurriría justo después del banner + ~2 s:

| arranque | hasta banner | + repo map 2 s ≈ bind | antes (real) |
|---|---:|---:|---:|
| A (caché caliente) | 16,3 s | **≈ 19 s** | 995,8 s |
| B | 59,1 s | **≈ 62 s** | 1005,1 s |
| C | 40,2 s | **≈ 43 s** | 1571,4 s |

### Fases que SIGUEN siendo > 5 s sin el repo map

| fase | medido (A/B/C) | ¿bloquea el bind? | propuesta |
|---|---:|---|---|
| **fingerprint sha256 del modelo** | — / 15,5 / 12,3 s | sí | **cachearlo** por (tamaño, mtime) en `data/` o junto al modelo; solo recalcular si cambia. Es procedencia, no corrección: puede moverse a background tras el bind |
| **embeddings de guilds (45)** | 4,9 / 24,2 / 10,5 s | sí | `embed_batch` en una pasada (hoy 1 por guild, ~0,5 s cada una) y/o **caché por hash de la descripción** (solo re-embebe las cambiadas); en régimen estable ≈ 0 s |
| **motor sparse BGE-M3 (ONNX)** | — / 8,2 / 7,0 s | sí | crear sesiones ONNX **en paralelo** (spawn_blocking + join) con la del modelo principal; o lazy en el primer recall |
| **carga modelo mxbai (ONNX)** | 6,2 / 5,7 / 4,8 s | sí | ídem: paralelizar con sparse/reranker; el bind no necesita el modelo cargado si `/health` no lo exige |
| reranker Jina | 0,7 / 1,2 / 0,9 s | sí | diferir a la primera llamada que rerankee (config `reranker_enabled`), no precargar en boot |
| spawn always-on | **320 / 401 / 227 s** | no (background tras el banner) | lanzar las 13 guilds **en paralelo con concurrencia acotada** (p.ej. 4): cada una tarda 15-30 s en handshake MCP (coloquio 20 s, scrapling 26 s, scheduler 16 s) y hoy van en serie |
| auto-link CERO-LLM | 55 / 15 / 20 s | no (arranca a los 8-90 s del banner) | **diferirlo** tras el bind/«ready» (o a idle) para no competir por CPU con el resto del arranque |
| NightConsolidation (12 fases) | arranca en el banner | no | diferir N segundos tras «ready» (hoy ya limita a 2 en paralelo, pero compite) |
| integridad BD | 2,8 / 2,5 / 2,4 s | sí | **no tocar**: es el gate fail-closed de datos; < 5 s y valioso |
| federation task/primer ciclo | +5 s tras bind | no | correcto; intervalo 5 s |

**Resumen:** sin repo map, el arranque queda dominado por **ONNX + embeddings** (fingerprint 12-15 s, guild embeddings 10-24 s, sparse 7-8 s, mxbai 5-6 s); paralelizando la creación de sesiones y cacheando fingerprint/embeddings se puede bajar a ~15-20 s. El spawn de guilds y el auto-link no retrasan el puerto pero sí la disponibilidad plena y la CPU durante el arranque: paralelizar el primero y diferir el segundo mejora la experiencia sin tocar el camino crítico.

## 4. Verificación

| comprobación | resultado |
|---|---|
| script reproducible | `scripts/measure_startup_phases.py --log logs/kernel.log --since 2026-10-22 --until 2026-10-23` → 4 arranques; tablas de arriba |
| cifras del log | `Repo map built` 24 878/17 900 941/945 990 ms; 24 880/17 898 448/979 059 ms; 24 884/17 904 534/1 531 144 ms — coinciden con el informe T-DSH-50 |
| líneas citadas | todas verificadas sobre `E:\tylluan\logs\kernel.log` (L… = número de línea 1-based) |
| gates | `check_doc_links.py` exit 0; `check_docs_reality.sh` solo el artefacto preexistente `ROADMAP_O3.md` (línea 42, WSL); `check_test_count.sh` ✅ (sin tests nuevos: el entregable es script) |
| sin `cfg(unix)`/`cfg(windows)` | n/a (Python) |
| diff | 2 ficheros nuevos sin commitear: `scripts/measure_startup_phases.py` + este informe. Sin tocar `src/`, guilds, `repo_map.rs` ni config |

## Impact

- **Solo lectura**: no se modifica el kernel ni config; el script es portable (`--log`, sin rutas de máquina hardcodeadas). El TL ya implementó D-39a con la tabla de T-DSH-50; esto cierra el «qué queda» con evidencia de los 3 arranques reales.
- **Camino crítico identificado tras D-39a**: ONNX (sparse 7-8 s + mxbai 5-6 s + reranker ~1 s) + fingerprint 12-15 s + guild embeddings 10-24 s ⇒ ~62 s en frío, ~19-43 s con caché caliente. Paralelizar sesiones ONNX y cachear fingerprint/embeddings son los cambios de mayor retorno; el spawn de guilds (227-401 s en background) y el auto-link (15-55 s de CPU) no retrasan el puerto pero sí la disponibilidad y la contención.
- **No propone tocar la integridad de BD** (2,4-2,8 s, fail-closed) ni el orden repo-map→bind (correcto).

## No verificado

- **Proyección post-D-39a** («≈19/62/43 s»): es aritmética (banner + repo map 2 s), no una medición con D-39a aplicado; D-39a va en el push 125 y no lo ejecuté.
- **Atribución de fases por anclas de log**: heurística sobre mensajes; si dos fases se solapan (p.ej. fingerprint con otras tareas) el tiempo mostrado es el hueco entre líneas, no CPU pura.
- **Arranque A** no registra `ONNX probe` ni `fingerprint`: no se puede comparar esa fase; puede ser caché caliente o una ruta de código distinta en esa build.
- **Varianza de disco/caché**: los mismos 45 embeddings tardan 4,9 s (A), 24,2 s (B) y 10,5 s (C); no he aislado si es caché de ONNX, I/O o contención con el auto-link/night consolidation que corre en paralelo.
- **Coste de paralelizar el spawn de guilds**: no medido; 13 guilds con concurrencia 4 podría saturar CPU/RAM en hardware modesto (el «toaster-friendly» es invariante). Requiere prueba aparte.
- **Duración de NightConsolidation**: no la extraje (fase multi-evento larga); solo consta que arranca en el banner.
- **Log de 36 MB con 4 arranques**: los tiempos son de esta máquina y este disco; no son extrapolables a otros equipos.

## Nota del TL (2026-10-24)
Verificado por el TL: ejecutado `scripts/measure_startup_phases.py --log logs/kernel.log --since 2026-10-22 --until 2026-10-23` y reproduce los cuatro arranques del log con las mismas horas de apertura del puerto (23:04:43.320, 23:53:47.984, 01:13:40.116). La proyeccion «19 / 62 / 43 s» tras D-39a es aritmetica de DSH, no medicion; la medicion real queda para T-DSH-52. El script solo lee el log (sin red ni procesos).
