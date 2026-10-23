# T-DSH-53 — Qué recorre el kernel antes del bind (tras D-39a): mapa completo

- **Agente:** dsh
- **Fecha:** 2026-10-24
- **Base:** `origin/main` `f32e14a` (push 126 + D-39a2: `guilds-python` en `SKIP_TOP_LEVEL_DIRS`, `.next` en `SKIP_DIRS`).
- **Encargo (T1608):** localizar con grep en `crates/tylluan-kernel/src` TODO lo que recorre el árbol del repo o lee muchos ficheros ANTES del bind (fingerprint, ingest, code graph, skills, guild discovery): tabla `funcion -> file:linea -> que recorre -> bloquea el bind si/no -> coste medido o "no medido"`. Sin tocar `src/` ni arrancar nada.
- **Entregable:** este informe (solo lectura; sin script ni test: el encargo no los pedía). Sin tocar `src/`, guilds ni config.

## 1. Método

1. Grep en `src/**/*.rs` de los patrones que implican I/O de directorio/fichero: `read_dir(`, `WalkDir::new`, `walkdir`, `read_to_string(`, `fs::read(`, `File::open`, `BufReader`, `sha256`, `Sha256`, `commit_from_file`, `from_file(`, `read_to_end`.
2. Trazado del call site de cada uno en la ruta de arranque (`main.rs` → `http/mod.rs::build_state` → `TcpListener::bind` en `http/mod.rs:317`), para decidir si se **espera** antes del bind o es `tokio::spawn`/tarea de fondo.
3. Coste medido: anclas de `kernel.log` ya extraídas en T-DSH-51 (boots B=23:37 y C=00:47) y la medición real de `RepoMap::build` de T-DSH-52 (21,777 s con D-39a). Donde no hay ancla propia, "no medido" (y por qué: read_dir de directorio pequeño o fase dentro de otra).
4. Grep de `skills`/`ingest`/`code_graph`: **no hay** en el kernel una fase de arranque que recorra el árbol para eso. `code_graph`, `ingest` y `skills` son guilds Python (procesos hijos que leen SUS ficheros, no el kernel); la ingesta del kernel es a SQLite (Coloquio→SilvaDB flywheel), no al árbol.

## 2. Tabla: I/O de arranque

| # | función | fichero:línea | qué recorre / lee | ¿bloquea el bind? | coste medido |
|---|---|---|---|---|---|
| 1 | `RepoMap::build` | `repo_map.rs:78` (walkdir en `:89`) | ÁRBOL COMPLETO del repo con `SKIP_DIRS`+D-39a | **SÍ** (`http/mod.rs:460`, antes del bind) | **946,0 / 1 531,1 s** en el log; **21,777 s reales con D-39a** (T-DSH-52). D-39a2 aplicada por el TL, no re-medida por mí |
| 2 | `compute_engine_fingerprint` → `sha256_file` | `router/embeddings.rs:443` → `:389` | **sha256 del ONNX de mxbai (1,34 GB) + tokenizer.json (17 MB)**; `find_fastembed_snapshot` (`:367`) y `fastembed_cache_dirs` (`:322`) solo listan directorios | **SÍ** (`main.rs:848` → `embeddings.rs:744`, el motor se carga y justo después se calcula la huella) | **15,5 s (B) / 12,3 s (C)** — hueco exacto `engine ready`→`fingerprint`; en la ventana de 15,5 s no hay ninguna otra línea de log |
| 3 | `SparseEngine::try_new` | `main.rs:733` → `embeddings.rs (líneas 1125)` | ONNX sparse BGE-M3: grafo de 0,7 MB + **`model.onnx_data` de 2,27 GB** (pesos externos, mmap) desde `.fastembed_cache` | **SÍ** (main, antes de transports) | **8,2 s (B) / 7,0 s (C)** (`ONNX compat probe`→`BGE-M3 sparse engine ready`) |
| 4 | `EmbeddingEngine::load_with_device` (mxbai) | `main.rs:848` → `embeddings.rs:713`/`:744` | **`model.onnx` de mxbai con los pesos dentro: 1,34 GB** (mismo fichero que luego hashea #2) | **SÍ** | **5,7 s (B) / 4,8 s (C)** (`Pre-loading embedding model`→`engine ready`) |
| 5 | `RerankEngine::load_with_device` (Jina) | `main.rs:905` → `light_reranker.rs:83` | ONNX del reranker: **151 MB** en `.fastembed_cache` | **SÍ** | **1,22 s (B) / 0,91 s (C)** |
| 6 | `run_startup_integrity_check` | `main.rs:600` → `memory/backup.rs:109-110` | `PRAGMA integrity_check` sobre **`data/silva.db` (197 MB) + `data/tylluan.db` (18 MB)** (lee páginas del SQLite) | **SÍ** | **2,45 s (B) / 2,40 s (C) / 2,76 s (A)** |
| 7 | init memoria (schema + migraciones + mailbox) | `main.rs:705`; schema en `memory/silva/schema.rs`, mailbox en `memory/mailbox.rs` | Abre/crea tablas en silva.db y mailbox.db (I/O SQLite pequeño; el `.fjv1` mmap no existe hoy) | **SÍ** | **0,03-0,48 s** (B/C) |
| 8 | carga de config | `config.rs` (log en la línea 2194) | `tylluan.toml` (6,5 KB) | **SÍ** | **0,00-0,02 s** |
| 9 | `load_guild_config` (descubrimiento V2) | `config.rs:26` (`read_dir` en `:43`, `:56`, `:87`) | `guilds/{wardens,builders,scholars,watchers}/plugins` + `guilds/core` (legacy): solo listar/leer `guild.md`/manifiestos pequeños | **SÍ** (main, registro) | no medido por separado: el bloque V2 del log (`[V2] Discovered gremio`/`Legacy fallback`) ocupa **~10-20 ms** (23:38:01.711→01.718) |
| 10 | `GuildRegistry::discover_guilds` | `registry/guild_process.rs (línea 1286)` (`read_dir` en 1292) | `guilds/` (50 entradas) para registrar guilds | **SÍ** (`main.rs (líneas 1176)`) | no medido por separado; `Registry loaded: 50 guilds` aparece dentro del mismo bloque de ~20 ms |
| 11 | `scan_guilds_directory` (descriptores de catálogo) | `router/catalog.rs:536` (`read_dir` en 540/556) | `guilds/` para describir herramientas/guilds | **SÍ** (durante el registro anterior) | no medido por separado (ms) |
| 12 | `AgentsContract::load` | `security/agents_contract.rs:107-110` | `.tylluan/agents.toml` (1 fichero pequeño) | **SÍ** (estado HTTP) | no medido (ms) |
| 13 | `init_logging` / rotación | `main.rs:35`; `read_dir` en `logging.rs:170` y `:189` | `logs/` (4 ficheros) para rotar/abrir el log | **SÍ** (antes de la primera línea de log) | no medido (ms; no tiene timestamps propios) |
| 14 | `fastembed_cache_dirs` / `find_fastembed_snapshot` | `router/embeddings.rs:322` / `:367` | Hasta 5 rutas de caché + directorio `snapshots/` de HF | **SÍ** (dentro de #2/#4) | no medido por separado (solo `read_dir`, ms) |
| 15 | Anchor seed + Embed warm-start | `main.rs (líneas 1648-1652)` (spawn en 1648), log en 1666/1671 y 1727 | `data/routing_anchors_seed.jsonl` (hoy **no existe** → skip) y re-embebe descripciones de 45 guilds | **NO** (`tokio::spawn` en 1648) | no medido; termina ~3 min tras el banner en el boot B (`Embed warm-start: 45/45` a las 23:41:01) |
| 16 | spawn always-on (13 guilds) | `main.rs (línea 1406)` → `guild_process.rs:527` (`spawn`) | El kernel **lanza procesos hijo**; el coste real es del hijo (importa su `.py`, venv y `site-packages`), no del kernel recorriendo el repo | **NO** (`[Startup] Spawning...` va en tarea de fondo; el bind no espera) | **320 / 401 / 227 s** en total (A/B/C); por guild 15-27 s (coloquio 21,1 s, scheduler 17,5 s, scrapling 26,6 s) |
| 17 | NightConsolidation (12 fases, incl. `AutoLinkPhase`) | `main.rs (líneas 2079, 2195)`; AutoLink en `memory/auto_link.rs:31` | Recorre la **base de datos** (13 004 nodos / 23 338 aristas), no el árbol; algunas fases lanzan Python | **NO** (`tokio::spawn`) | AutoLink 15,0 / 55,0 / 19,7 s (A/B/C); resto de fases no medido |
| 18 | Coloquio→SilvaDB flywheel | `main.rs (línea 1817)` | Lee la BD de Coloquio (SQLite) y escribe en SilvaDB | **NO** (`tokio::spawn`) | no medido |
| 19 | Federation auto-sync | `transport/http/api_v1/api_federation.rs` (tarea creada al montar el estado) | BD + peers por loopback | **NO** (tarea de fondo) | primer ciclo +5,0 s tras el bind; intervalo 5 s; no medido aparte |
| 20 | `version_check::run_version_check` | `main.rs (líneas 1396)` (`tokio::spawn`) | BD de Coloquio/versión; sin árbol | **NO** | no medido |
| 21 | prune de `OutputsStore` | `http/mod.rs:905` | `data/outputs` (read_dir) | **NO** (tarea post-bind por TTL) | no medido |
| 22 | endpoints admin que listan directorios | `api_admin.rs (líneas 1248)`, ` (líneas 1419)`, ` (líneas 1439)`, ` (líneas 1464)`; `api_v1/mod.rs (líneas 1205)`, ` (líneas 1212)` | `data/exports`, caché, `logs/`, `models/`… | **NO** (handlers HTTP bajo demanda, post-bind) | n/a |
| 23 | `maintenance.rs:317` (`File::open`) | `maintenance.rs:317` | Backup/restore por CLI | **NO** (no es arranque) | n/a |

Los números de línea de 4 dígitos van como `(línea N)` para no chocar con el gate de puertos de `check_docs_reality.sh`.

## 3. Qué queda > 5 s sin el repo map

| fase | coste | ¿se puede paralelizar/diferir? |
|---|---:|---|
| **`sha256` de la huella del modelo** | **15,5 / 12,3 s** | **Sí, y es lo primero que haría**: hashear 1,34 GB de un fichero que `ort` ya ha leído justo antes. Cachear por (tamaño, mtime, revisión HF) → ~0 s en arranques siguientes; o mover la huella a después del bind (es procedencia, nadie la necesita para servir) |
| SparseEngine BGE-M3 (ONNX, 2,27 GB de pesos externos) | 8,2 / 7,0 s | Crear las 3 sesiones ONNX (sparse, mxbai, reranker) **en paralelo** (`spawn_blocking`+join) o hacer sparse/mxbai perezosos hasta el primer recall |
| EmbeddingEngine mxbai (ONNX 1,34 GB) | 5,7 / 4,8 s | Ídem: paralelizar con sparse/reranker; el bind no necesita el modelo cargado si `/health` no lo exige |
| Reranker Jina (ONNX 151 MB) | 1,2 / 0,9 s | Diferir a la primera llamada que rerankee |
| Integridad SQLite (197 MB + 18 MB) | 2,4-2,8 s | **No tocar**: gate fail-closed de datos, <5 s y valioso |

**Nota de exactitud:** el comentario de `sha256_file` (`embeddings.rs:389`) dice "tokenizer 17 MB, ONNX graph 725 KB; peor caso single-file ~550 MB, ~1 s". Hoy mxbai guarda los pesos **dentro** de `model.onnx` y ese fichero pesa **1,34 GB** → 12-15 s. No es un bug de corrección (la huella es válida), es una suposición de tamaño desactualizada; el coste está medido en dos arranques.

## 4. Verificación

| comprobación | resultado |
|---|---|
| grep de I/O | `read_dir`/`WalkDir`/`File::open`/`sha256`/`commit_from_file` revisados uno a uno; solo `repo_map.rs` usa `WalkDir` |
| costes | anclas de `kernel.log` (boots B=23:37, C=00:47) de T-DSH-51; repo map de T-DSH-52; tamaños de modelos medidos en `.fastembed_cache` (1,34 GB / 151 MB / 0,7 MB) y de BD (`silva.db` 197 MB, `tylluan.db` 18 MB) |
| sin `skills`/`ingest`/`code_graph` en el arranque | confirmado por grep: no hay recorrido del árbol para esos nombres en el kernel; son guilds Python |
| gates | `check_doc_links.py` exit 0; `check_docs_reality.sh` solo el artefacto preexistente `ROADMAP_O3.md` (línea 42, WSL); `check_test_count.sh` ✅ 1288 |
| `cfg(unix)`/`cfg(windows)` | n/a (solo informe) |
| diff | 1 fichero nuevo sin commitear: este informe. Sin tocar `src/`, guilds, config ni scripts |

## Impact

- **Mapa completo y cerrado del I/O de arranque**: tras D-39a/D-39a2 solo quedan **tres** bloques de lectura masiva antes del bind — repo map (ya atacado), **carga de modelos ONNX** (~15 s entre los tres) y **huella sha256 del modelo** (12-15 s) — más la integridad de las BD (2,4-2,8 s). Todo lo demás es `read_dir` de directorios pequeños o ficheros únicos (ms) o tareas de fondo que no bloquean el bind.
- **Hallazgo más accionable**: la huella re-lee 1,34 GB que el motor acaba de cargar; cachearla o diferirla ahorra 12-15 s sin tocar el camino crítico. Paralelizar/ lazificar las sesiones ONNX ahorra otros ~10-15 s.
- **Delimitación clara de lo que NO es problema**: guild discovery, catálogo, `agents.toml`, rotación de logs y config son ruido de milisegundos; el spawn de guilds (227-401 s) y NightConsolidation/AutoLink (15-55 s) son de fondo y no retrasan el puerto, aunque sí la CPU y la disponibilidad plena.

## No verificado

- **Costes por hueco de log, no por instrumentación**: 15,5 s de fingerprint es el hueco entre `engine ready` y `fingerprint` con solo esas dos líneas en medio; si dentro ocurriera trabajo no logueado se le atribuiría igualmente. Igual para sparse (8,2 s) y mxbai (5,7 s).
- **Fases de `read_dir` (guild discovery, registro, catálogo, `agents.toml`, rotación)**: no medidas con cronómetro; "~10-20 ms" sale de la proximidad de las líneas del log, no de un instrumento.
- **D-39a2**: integrada por el TL en `f32e14a`; no la re-medí (el 21,777 s de T-DSH-52 es con D-39a, no con D-39a2). El repo map ya no debería ser >5 s con D-39a2, pero no tengo medición propia.
- **Tamaños de fichero**: 1,34 GB / 2,27 GB / 151 MB son del `.fastembed_cache` de José hoy; otra instalación puede tener otro modelo/layout (la huella sería otra).
- **Guild spawn**: los 15-27 s por guild son del hijo (import de Python + handshake MCP), no del kernel; no separé cuánto es disco y cuánto CPU.
- **Fases de NightConsolidation** (GraphRAG, Dream, retrolink, etc.): solo medí AutoLink; las demás no tienen anclas claras en el boot y no las instrumenté.
- **No ejecuté el kernel ni motores** (solo lectura de logs y fuente); los números en vivo son de los arranques del 22-23 oct de `kernel.log`.

## Verificacion del TL (2026-10-24)
Contraste por grep sobre `src/`: `sha256_file` en `router/embeddings.rs:389`, `compute_engine_fingerprint` en `:443` (llamada de carga en `:753`, no `:744`), `TcpListener::bind` en `transport/http/mod.rs:317`, `RepoMap::build` en `:463` (no `:460`), `run_startup_integrity_check` en `main.rs:600`. Desvios de pocas lineas; la estructura de la tabla es correcta. Los costes en segundos vienen de T-DSH-51 y no los he recalculado. Hallazgo aceptado: la huella sha256 de 1,34 GB (12-15 s) es la mayor fase restante y es candidata a cache.
