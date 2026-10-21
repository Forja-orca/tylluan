# T-DSH-19 — Auditoría del QUICKSTART (docs/getting-started/QUICKSTART.md)

- **Agente:** dsh
- **Fecha:** 2026-10-21
- **Base:** `db70fb5` == origin/main (push90: quickstart inglés reasignado de Hermes, ya integrado).
- **Encargo (turno 1489):** solo lectura, sin arrancar kernels: tabla afirmación → cómo se verificó → fuente (archivo:línea o comando) → OK/FALSO/no verificable, para CADA comando, ruta, endpoint, subcomando de `tylluan-cli`, nombre de perfil, cifra (45 guilds, 6 tools, 34 MB, 67 MB, 200k docs/8 GB, puerto 47004) y afirmación de seguridad. Un FALSO lleva archivo:línea y corrección propuesta. Diff sin commitear solo si corrijo el quickstart (cambios mínimos), informe con `## Impact` y «no verificado».

## Entrega

- **Diff sin commitear: `docs/getting-started/QUICKSTART.md` (4 correcciones mínimas, 4 líneas)** + este informe. Cero cambios en código.
- Verificación: lectura del código real (no del doc), `rust-toolchain.toml`, `Cargo.toml`, scripts de instalación, `--help` del CLI en fuente (definiciones clap), rutas del router, recuento canónico de guilds del propio gate, y las mediciones publicadas del README.

## Tabla afirmación × verificación × fuente × veredicto

### Prerrequisitos (§1)

| # | afirmación | cómo se verificó | fuente | veredicto |
|---|---|---|---|---|
| 1 | Rust **1.88+** «pinned in rust-toolchain.toml» | leer el pin real | `rust-toolchain.toml:2` = `channel = "1.99.1"` | **FALSO** → corregido a «1.99.1 (pinned)». (Nota: el badge de `README.md:19` repite «1.88+»; fuera de alcance, conviene corregirlo también) |
| 2 | Git | trivial | — | OK |
| 3 | Python **3.12+** (opcional; solo guilds) | contraste con docs y requisitos | `README.md:217` (misma afirmación: «requires Python 3.12 + FastMCP»), `guilds/requirements.txt` existe | OK (documentado; no ejecutado) |
| 4 | Node.js + pnpm opcionales (dashboard) | existencia del proyecto | `dashboard/package.json`, `dashboard/pnpm-lock.yaml` | OK |

### Instalación (§2)

| # | afirmación | cómo se verificó | fuente | veredicto |
|---|---|---|---|---|
| 5 | URLs `raw.githubusercontent.com/Forja-orca/tylluan/main/install.sh` y `install.ps1` | ficheros en la raíz + remote real | `install.sh`, `install.ps1` en raíz; `git remote -v` = `https://github.com/Forja-orca/tylluan.git` | OK |
| 6 | El instalador descarga binarios pre-compilados (`tylluan-nexus` y `tylluan-cli`) en `~/.tylluan/bin/` | leer script: `ARCHIVE=tylluan-${TARGET}.tar.gz`, extrae con `--strip-components=1`, chmod a ambos | `install.sh:40,108,112`; `install.ps1:19,116-120,202-203` | OK |
| 7 | Añade al PATH | bash/escribe en `.bashrc`/`.zshrc`; PowerShell `SetEnvironmentVariable("PATH","User")` | `install.sh:118-126`; `install.ps1:145-148` | OK |
| 8 | Escribe `tylluan.toml` con perfil `portable` y **arranca** el kernel | `install --profile portable` documenta que escribe TOML y arranca; script lo invoca y espera health | `install.sh:135-154`; `install.ps1:155-169` | OK |
| 9 | `git clone …` + `cargo build --release -p tylluan-kernel` | paquete y binario reales | `crates/tylluan-kernel/Cargo.toml:2` (`tylluan-kernel`) y `:8-9` (`[[bin]] name = "tylluan-nexus"`) | OK |
| 10 | `python -m venv .venv` + `pip install -r guilds/requirements.txt` | fichero existe | `guilds/requirements.txt` (Test-Path: true) | OK |
| 11 | Binario en `target/release/tylluan-nexus(.exe)` | nombre de bin | `Cargo.toml:9` | OK |

### Arranque y aislamiento (§3)

| # | afirmación | cómo se verificó | fuente | veredicto |
|---|---|---|---|---|
| 12 | `cargo run -p tylluan-cli -- start` | paquete + subcomando `start` clap | `crates/tylluan-cli/Cargo.toml`; `tylluan-cli/src/main.rs:45-60` | OK |
| 13 | `./target/release/tylluan-nexus` directo | bin name | `Cargo.toml:9` | OK |
| 14 | Puerto por defecto **47004** | defaults del kernel + CLI | `config.rs (línea 1921)` `default_port() = 47004`; `tylluan-cli/src/main.rs:12` `DEFAULT_PORT: 47004` | OK |
| 15 | Si 47004 está ocupado, detecta `data/active_port.json` y gestiona el binding | leer `start_http_server` | `transport/http/mod.rs:309-354` (lee/escribe `data/active_port.json`, sustituye al kernel previo) | OK |
| 16 | Instancias aisladas: directorio/config propios, `TYLLUAN_DATA_DIR`, `--port` | env var y flags reales | `main.rs:181,494,630`; `main.rs:364,521` (`--port`); `tylluan-cli/src/main.rs:69` (`--data-dir`) | OK |

### Perfiles (§4)

| # | afirmación | cómo se verificó | fuente | veredicto |
|---|---|---|---|---|
| 17 | Perfiles `portable` / `clinic` / `server` | enum clap + `--help` en fuente | `tylluan-cli/src/main.rs:18-29,114` | OK |
| 18 | `portable` = sin embeddings (BM25-only), ~34 MB residente idle | medición publicada + mapeo de perfil | `tylluan-cli/src/main.rs (línea 1085)` (`("none", 0)`); `README.md:36` («~34 MB resident memory at idle», 0.9–3.7 s, WSL2 6 CPU/8 GB, binario v0.17.1) | OK |
| 19 | `clinic` = BGE-Small **384-dim, 67 MB**, ~200k docs en 8 GB | enum + mapeo + comentario + README | `tylluan-cli/src/main.rs:19` («BGE-Small embedding (67MB, 384-dim). Good for ~200K docs on 8GB RAM»), ` (línea 1086)` (`("bge-small", 384)`); `README.md:256` (~67 MB) | OK |
| 20 | `server` = `mxbai-embed-large` **1024-dim** | mapeo de perfil | `tylluan-cli/src/main.rs (línea 1087)` (`("mxbai-embed-large", 1024)`) | OK |
| 21 | `tylluan-cli install --profile portable/server` | subcomando `install` + `--profile` | `tylluan-cli/src/main.rs:124-136` | OK |

### MCP y auth (§5)

| # | afirmación | cómo se verificó | fuente | veredicto |
|---|---|---|---|---|
| 22 | MCP sobre SSE (`/sse`) y Streamable HTTP (`/messages`) | rutas reales del router | `sse.rs:25-29` (`/sse`); `transport/http/mod.rs (línea 1213)` (`/messages`), ` (líneas 1214-1216)` (`/mcp`, `/api/v1/mcp`) | OK |
| 23 | `dev_mode = false` por defecto | `Default` de la config | `config.rs:633` (`dev_mode: false`) | OK |
| 24 | Genera bearer token aleatorio en el primer arranque y lo escribe en `.tylluan-token` | leer `load_or_generate_token` | `config.rs (línea 2242)-2283` (genera y guarda; falla con error si no puede) | OK |
| 25 | `cat .tylluan-token` | fichero en el directorio del kernel | `config.rs (línea 2261)` `Path::new(".tylluan-token")` | OK |
| 26 | Bloque JSON `claude_desktop_config.json` con `type: sse` + `url` con `?token=` | /sse acepta `?token=`; campo `type` es formato de cliente | `sse.rs` handler + auth; formato del cliente = externo | OK (formato del cliente no verificable aquí) |

### Herramientas soberanas (§6)

| # | afirmación | cómo se verificó | fuente | veredicto |
|---|---|---|---|---|
| 27 | Exactamente 6 tools soberanas | lista + filtro de `all_tools`/`kernel_tools` | `transport/server/tools.rs:51-56` (6) y `:155-320` (definiciones); `handlers.rs:12` | OK |
| 28 | `tylluan_remember`: «automatic decay and deduplication» | descripción real de la tool + dónde vive la deduplicación | `tools.rs:205-206` (desc: «Store information in long-term memory…»); dedup = `memory/dream_cycle.rs:73-150` (consolidación nocturna), no en la escritura | **FALSO (matiz)** → corregido: «automatic decay (duplicate merging runs later in the DreamCycle consolidation, not on the write path)» |
| 29 | `tylluan_recall`: BM25 + vectores + Personalized PageRank | `search_hybrid` + graph PPR | `memory/silva/search.rs` (RRF BM25+vector+graph), `memory/silva/graph.rs` (`local_query_graph` PPR + degree penalty) | OK |
| 30 | `tylluan_think`: razonamiento estructurado y multi-hop | descripción + parámetro `chain` | `tools.rs:246-263` («multi-hop reasoning chain expansion») | OK |
| 31 | `tylluan_graph`: triples, paths, vecinos | descripción real | `tools.rs:265-290` | OK |
| 32 | `tylluan_do`: despacho a guilds python | descripción real | `tools.rs:156-157` | OK |
| 33 | `tylluan_decide`: «(Choice, Score, **Noul**)» | descripción real de la tool | `tools.rs:292-293` = «(Choice, Score, **Boolean**)» | **FALSO (typo)** → corregido a «Boolean» |

### Guilds (§7)

| # | afirmación | cómo se verificó | fuente | veredicto |
|---|---|---|---|---|
| 34 | **45** guilds enrutables por defecto | recuento canónico del propio gate + descubrimiento | `scripts/check_docs_reality.sh:419-440`: `FastMCP(` en `guilds/*.py` (46, dedup) − `EXCLUDED_GUILDS` (`vision_moondream`) = **45**; descubrimiento en `router/catalog.rs:587-610` | OK (nota: QUICKSTART no está en la lista del gate, solo README/README.es/STATUS/svg) |
| 35 | Guilds auto-descubiertas en `guilds/` | `builtin_catalog` escanea en boot | `router/catalog.rs:587-610` + `scan_guilds_directory` | OK |
| 36 | `tylluan-cli doctor` | subcomando existe | `tylluan-cli/src/main.rs:66-68` | OK |

### Verificación esencial (§8)

| # | afirmación | cómo se verificó | fuente | veredicto |
|---|---|---|---|---|
| 37 | `curl /health` devuelve status, version y commit | struct `HealthResponse` | `transport/http/mod.rs (línea 1201)` (ruta), ` (línea 1376)-1380` (`status`, `version`, `commit`) | OK |
| 38 | `curl -H "Authorization: Bearer …" /api/v1/silva/stats` | ruta + capa de auth | `api_v1/routes.rs:35`; rutas api_v1 dentro de `protected_routes` con `bearer_auth_middleware` (`mod.rs (línea 1211)-1228`) | OK |
| 39 | `tylluan-cli audit` | subcomando + descripción | `tylluan-cli/src/main.rs:96-100` | OK |

### Seguridad (§9)

| # | afirmación | cómo se verificó | fuente | veredicto |
|---|---|---|---|---|
| 40 | Bind a `127.0.0.1` por defecto | default real | `config.rs (línea 1918)` `default_host() = "127.0.0.1"` | OK |
| 41 | Nunca `0.0.0.0` + `dev_mode=true` | hay guard que fuerza loopback | `config.rs (línea 2185)-2196` (fuerza host a 127.0.0.1 con aviso CRITICAL) | OK |
| 42 | «Fail-closed: con dev_mode=false toda operación mutante exige bearer» | rutas protegidas + middleware | `mod.rs (línea 1205)-1228`: `api_v1 + sse + /messages + /mcp + /a2a` tras `bearer_auth_middleware`; públicas solo `/health`, `/discovery`, `/api/v1/mcp/probe`, agent-card y `/oauth/*` (superficie de auth, por diseño) | OK (matiz: `/oauth/*` es público por diseño) |
| 43 | `.tylluan-token` git-ignored | `.gitignore` | `.gitignore:26,183-184` | OK |
| 44 | Peers de federación: bearer `auth_token` + Noise NK (ChaCha20-Poly1305) | leer crate link | `crates/tylluan-link/src/noise.rs:4-7,18,60` (XK/NK + ChaChaPoly por dentro), `dispatch.rs:9,49-71` (payloads con Noise NK) | OK |

### Cierre (§10)

| # | afirmación | cómo se verificó | fuente | veredicto |
|---|---|---|---|---|
| 45 | Enlaces a README, SECURITY.md, concepts/SECURITY.md | ficheros existen | `README.md`, `SECURITY.md`, `docs/concepts/SECURITY.md` (Test-Path todos true) | OK |
| 46 | «the three-step binary install» del README | leer README: Quick Start tiene **1—Install, 2—Start, 3—Connect, 4—Try it** (4 pasos) | `README.md:202-303` | **FALSO (menor)** → corregido a «the binary Quick Start (4 steps)» |

## Correcciones aplicadas (diff sin commitear, 4 líneas)

1. `QUICKSTART.md:9` — «Rust 1.88+» → **1.99.1** (el pin real).
2. `QUICKSTART.md:121` — `tylluan_remember`: «automatic decay and deduplication» → decay + nota de que la deduplicación es del DreamCycle.
3. `QUICKSTART.md:126` — `tylluan_decide`: «(Choice, Score, **Noul**)» → **Boolean**.
4. `QUICKSTART.md:172` — «three-step binary install» → «binary Quick Start (4 steps)».

## Impact

- **Diff sin commitear: solo `docs/getting-started/QUICKSTART.md` (4 líneas) + este informe.** Cero cambios en código, cero arranques de kernel, cero escrituras en producción.
- Los 4 falsos eran de documentación (versión de Rust obsoleta, un typo `Noul`, una capacidad exagerada y un recuento de pasos); ninguno afecta al comportamiento del kernel. Las 42 restantes se sostienen contra el código real.
- **Nota para TL:** `README.md:19` repite «Rust 1.88+» (mismo falso); el QUICKSTART no está en la lista de documentos que vigila el gate de guilds (`check_docs_reality.sh`), por lo que la cifra 45 puede volver a divergir sin que CI lo note.

## No verificado

- **Nada ejecutado en vivo**: no se arrancó el kernel ni el instalador (regla de flota); todo es lectura de código/scripts + recuento estático. La instalación real (`curl | sh`) no se probó.
- **Formato `claude_desktop_config.json` con `"type": "sse"`**: formato del cliente (Claude Desktop/OpenCode/Cursor), no del kernel; no ejecutado.
- **Python 3.12+ y `pip install -r guilds/requirements.txt`**: documentado en README y el fichero existe, pero no se instaló ni se ejecutaron guilds.
- **Mediciones de 34 MB / 0.9–3.7 s / 67 MB / 200k docs**: se citan del README (medición publicada el 2026-10-14) y del comentario del CLI; no se repitieron en esta máquina.
- **Puerto 47004 con un kernel realmente ocupándolo**: la gestión de `active_port.json` se leyó en código, no se reprodujo arrancando dos kernels.
