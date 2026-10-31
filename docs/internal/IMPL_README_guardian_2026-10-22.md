# IMPL — Guardián del README público (2026-10-22)

**Hash base:** `97ba01e3a1326e5715b785788f60f3fb60497221` (origin/main, worktree `E:\tylluan-wt-readme`, detached)
**Estado:** cambios SIN commitear (README.md y docs/assets/architecture.svg). Este informe es el único fichero nuevo.

## Resumen

- Afirmaciones comprobadas: 58 filas en la tabla. OK sin cambios: 38. Corregidas: 13 (tabla, filas marcadas CORREGIDO). Quitadas por falta de fuente: 3 cifras (filas 27, 28, 30) más las frases de arranque "instantáneo" y "menos de un minuto". Sin verificar, dejadas tal cual y listadas abajo: 6.
- Scripts: `scripts/check_docs_reality.sh` exit 0, sin ❌ (última línea: `✅ No doc-path, doc-port or version drift found (174 low-confidence findings suppressed).`). `python scripts/check_doc_links.py` exit 0 (última línea: `✅ 222 relative doc links across 312 Markdown files all resolve to tracked paths.`).
- Recuento de tests: `scripts/check_test_count.sh` da OK: README y AGENTS.md = 1272 (kernel 1185 + link 75 + fsrs 12). No modificado.

## Tabla afirmación -> fuente/comando -> resultado

| # | Afirmación (README) | Fuente / comando | Resultado |
|---|---|---|---|
| 1 | Versión 0.17.1 (badge y texto) | `Cargo.toml` `[workspace.package] version`, STATUS.md | OK |
| 2 | Tags de versión | `git tag` (últimos: v0.9.0 en la lista corta; STATUS cita v0.17.1 sin tag comprobado aquí) | OK (versión); tag v0.17.1 no comprobado localmente |
| 3 | Rust "1.88+ minimum" | `rust-toolchain.toml` = `1.99.1`; no hay `rust-version` en Cargo.toml | CORREGIDO: badge a `rust-1.99.1` (1.88+ sin fuente) |
| 4 | Python 3.12+ | `pyproject.toml` `requires-python = ">=3.12"` | OK |
| 5 | Licencia MIT | `LICENSE` (MIT License) + badge | OK |
| 6 | Recuento de tests 1272 | `scripts/check_test_count.sh` → OK, 1272 | OK |
| 7 | 6 herramientas MCP | `crates/tylluan-kernel/src/transport/server/tools.rs` `sovereign_tool_names()` (6) y `all_tools()`; default `expose_guild_tools: false` (config.rs:634) | OK, pero la frase "every client sees exactly these six" sólo vale en modo por defecto. CORREGIDO (matiz) |
| 8 | 45 guilds | STATUS.md:11 ("45 enrutables"), guilds/README.md | OK |
| 9 | "Python guilds (45 tools)" | idem | CORREGIDO: "45 guilds" |
| 10 | Puerto 47004 | `default_port()` config.rs:1921; `tylluan.example.toml:6`; QUICKSTART | OK |
| 11 | Comando `tylluan-cli start` y salida de ejemplo | `crates/tylluan-cli/src/main.rs:214-241` (mensajes idénticos) | OK |
| 12 | `tylluan start --setup` / `tylluan-cli install` | clap `name = "tylluan"` en CLI; el binario instalado es `tylluan-cli` (install.sh) | CORREGIDO: `tylluan-cli start --setup` |
| 13 | Endpoint `/health` | rutas en transport/ | OK |
| 14 | Endpoints memory/write, memory/search, decide, do, federation peers/sync/permissions (PUT), a2a agents | `api_v1/routes.rs`, `api_federation.rs:333` | OK |
| 15 | Auth: token en `.tylluan-token` | config.rs:2242-2243 | OK (sólo nombre de fichero; ningún valor) |
| 16 | Dev mode "(`--dev`)" | no existe flag `--dev` en kernel ni CLI; la opción es `dev_mode` (config.rs:599) | CORREGIDO: "`dev_mode = true` en tylluan.toml" |
| 17 | `dev_mode=false`, `host=127.0.0.1` por defecto | config.rs:633, 1918 | OK |
| 18 | STUN opt-in, `[nat] enabled = false` | config.rs:382-390 (`enabled: false`) | OK (quitada la fecha "fixed 2026-08-23") |
| 19 | Malla: sin bootstrap WAN en ninguna release | config.rs:430-436: `mainline_dht_enabled` = false; feature `mainline-dht` no usada en release.yml | OK. Nota: `bootstrap_enabled` = true por defecto (sin seed_nodes); no revisado en profundidad |
| 20 | Embeddings por defecto mxbai-embed-large 1024d | config.rs:1928 | OK |
| 21 | Lista de embeddings configurables "mxbai-q, bge-m3, bge-small, minilm, nomic, arctic, e5, none" | `router/embeddings.rs` (nomic, arctic, minilm, bge-m3 visibles); mxbai-q y e5 no comprobados | Parcial, sin cambios |
| 22 | Reranker Jina v1 Turbo | `jina-reranker-v1-turbo-en` en kernel | OK (en SVG: "v2" eliminado, CORREGIDO) |
| 23 | Sparse léxico | config.rs:1949 `default_hybrid_sparse_enabled` = false | OK en SVG ("config-gated"); en Mermaid "SPLADE" CORREGIDO a "Learned-sparse lexical (BGE-M3, opt-in)" |
| 24 | Plan mode `tylluan_do(plan=true)` | tools.rs:166 (`plan`, plan_id, approve_action) | OK |
| 25 | `include_archived` en tylluan_recall | handler_recall.rs, search.rs | OK |
| 26 | Cifras LongMemEval R@1/5/10, MRR 0.46, p50 12.9 ms | `benchmarks/longmemeval_v0.12.0.json` | OK (ya marcadas como BGE-M3 v0.12.0 en README) |
| 27 | Tabla de perfiles: portable R@5 38%, R@10 42%, 0.4 ms; RAM ~30 MB | ninguna fuente en `benchmarks/` ni `docs/` | QUITADO (sin fuente): columnas reemplazadas por "not measured" |
| 28 | Tabla de perfiles: clinic R@5 61%, R@10 68%, 3.2 ms, ~300 MB RAM | ninguna fuente | QUITADO (sin fuente) |
| 29 | clinic descarga ~100 MB | `tylluan-cli/src/main.rs:19, 627, 1149`: BGE-Small 67 MB | CORREGIDO a ~67 MB |
| 30 | Medición de arranque 2026-10-14 (16.7 MB, 0.9–3.7 s, 34 MB RSS, sin TCP saliente en 10 s) | no hay fichero de medición en el repo; sólo aparece en el propio README y en QUICKSTART:75 (`~34 MB resident idle`) | QUITADO (sin fuente primaria). Ver "pendientes" sobre QUICKSTART |
| 31 | `portable` = perfil por defecto de install.sh/ps1 | install.sh / install.ps1 ejecutan `install --profile portable` | OK |
| 32 | "tylluan-cli install --profile server" ~670 MB | no hay tamaño en la CLI | CORREGIDO: "size not verified" |
| 33 | ONNX probe antes de llamar; smoke test sin ONNX en CI | ci.yml:321 `boot-smoke-no-onnx` | OK (quitada la fecha) |
| 34 | CI: build+test, clippy, cargo-deny, Python lint+test, dashboard, security audit | ci.yml: rust-build-test, clippy (l.29), rust-security-audit (cargo-deny), python-lint-test, dashboard-lint, security-claims-gate | OK salvo "dashboard build" (el job es dashboard-lint; no comprobado más) |
| 35 | ARM64: "CI compiles the ARM64 build" | ci.yml:158-162 `cargo check --target aarch64` | CORREGIDO a "type-checks (cargo check)" |
| 36 | Dashboard "served automatically at :47004 (single binary)" | transport/http/mod.rs:82-84: rust-embed sólo con `feature = "bundled-dashboard"`; release.yml no lo activa | CORREGIDO: sólo con esa feature o si existe `pnpm build` en el árbol |
| 37 | Dashboard dev `pnpm dev` en :5173 | dashboard/package.json `"dev": "vite"` | OK |
| 38 | Features: encryption (SQLCipher), bundled-dashboard, DirectML/CUDA | Cargo.toml (`encryption` l.145, `bundled-dashboard` l.163), ort `directml`, feature `cuda` | OK |
| 39 | Cifrado por defecto sólo con la feature | config.rs:1409 `cfg!(feature = "encryption")` | OK |
| 40 | A2A: Agent Card + JSON-RPC 2.0; interop con LangGraph, CrewAI | `/.well-known/agent-card.json`, `/api/v1/a2a/agents` existen; interop con frameworks concretos NO comprobada | CORREGIDO (frameworks quitados, "not yet tested") |
| 41 | MCP: "works with ... any MCP client" | integrations/ tiene configs de Claude, Cursor, VS Code, LM Studio | CORREGIDO ("such as ...") |
| 42 | Federación: `[federation] auto_sync_mode`, `sync_interval_ms` default 30000 | config.rs:353-369, 1942 | OK |
| 43 | Endpoints de federación de los ejemplos (add peer, sync, sync/pull, nodes) | api_federation routes | OK |
| 44 | Permisos por peer/espacio, `dispatch_allow` exacto | routes.rs:184, api_federation.rs:333-364 | OK |
| 45 | Ejemplos `examples/*.py` | ficheros existen | OK |
| 46 | Enlaces relativos del README | `check_doc_links.py` | OK |
| 47 | Rutas citadas (assets, benchmarks, ADRs, screenshots, docs) | `ls` / check_docs_reality | OK (`dashboard/dist` tenía una cita nueva que el script marcó; corregida al rehacer la frase) |
| 48 | Commit 61bc97e (OAuth fase 1) | `git cat-file` | OK |
| 49 | ADR-019 / 018 / 020 / 021 existen; ADR-015 (stigmergia) existe | `docs/reference/adr/` | OK |
| 50 | Estado de tabla "Architecture decisions in flight" (Decision service 6ª herramienta, sin proveedor de modelo) | config.rs:2002-2026 (2 contratos en shadow); `api_types.rs` | OK |
| 51 | "Decima-small +5 net correct de 245" | docs/reference/research/2026-10-15_decision_models… contiene "245 casos HITL" (base de datos), delta +5 no comprobado | sin verificar, sin cambio |
| 52 | Trust Console, screenshots | assets/screenshots/*.png existen | OK (imagen no inspeccionada visualmente) |
| 53 | "Coloquio conversations stored as episodic nodes" | STATUS.md (P0-B) | OK según STATUS |
| 54 | Memory lifecycle active→quiet→consolidated→archived; "summaries immune to pruning" | STATUS.md (ADR-012) lifecycle OK; inmunidad de summaries no comprobada | Parcial, sin cambio |
| 55 | Mesh Noise NK/XK, ChaCha20, Ed25519, mDNS opt-in | tylluan-link/src/noise.rs; STATUS | OK |
| 56 | "legacy plaintext wire path only for backward compat" | no comprobado | sin verificar, sin cambio |
| 57 | "Docker smoke CI" listado como pendiente de v1.0 | existe `.github/workflows/docker-smoke.yml` | sin verificar (puede estar desfasado); sin cambio |
| 58 | Enlaces `github.com/Forja-orca/tylluan` (badge CI, install.sh/ps1, Discussions) | URL pública del repo | Sin cambio (ver privacidad) |

## Cambios hechos (fichero:línea de la versión original de 97ba01e)

README.md:
- L19: badge Rust `1.88+` → `1.99.1` (toolchain pinned; no hay rust-version).
- L36: párrafo "works without internet": quitadas las cifras de arranque (16.7 MB, 0.9–3.7 s, 34 MB, sin TCP en 10 s) sin fuente en el repo; queda la afirmación verificable y el límite (no probado en Raspberry Pi; CI sólo hace `cargo check` ARM64).
- L41: quitada la fecha "fixed 2026-08-23" del párrafo de STUN (narrativa interna).
- L63 (A2A) y L64 (MCP): quitados los frameworks (LangGraph, CrewAI) cuya interop no está probada; "any MCP client" → "such as ...".
- L98-100 (Dashboard): la URL de producción sólo vale con `--features bundled-dashboard`.
- L116-118 (Six tools): añadido matiz de modo por defecto (`expose_guild_tools`).
- L156-162 (tabla de perfiles): cifras de recall/latencia/RAM de portable y clinic sustituidas por "not measured" (sin fuente); clinic ~100 MB → ~67 MB; nota al pie reescrita.
- L204: quitada la fecha y la jerga "verified live"; el smoke test no-ONNX se cita por su job de CI.
- L237: "It boots instantly" (no medido) → "starts without fetching models".
- L254-258 (TIP y Auth): `tylluan start --setup` → `tylluan-cli start --setup`; "~670 MB" → "size not verified"; `--dev` (inexistente) → `dev_mode = true` en tylluan.toml.
- L308: "Python guilds (45 tools)" → "(45 guilds)".
- L404 (Mermaid): nodo SPARSE "Research: Sparse Lexical (SPLADE)" → "Learned-sparse lexical (BGE-M3, opt-in, off by default)" (el código es BGE-M3 learned-sparse, opt-in).

docs/assets/architecture.svg:
- L79: cliente MCP: quitados "Qwen · Antigravity" (clientes ligados a la flota interna; en duda, se quita) → "LM Studio".
- L84: cliente A2A: "LangGraph · CrewAI · AutoGen · any A2A SDK" → "Agent2Agent-compliant clients".
- L106: "1,154 TESTS GREEN" → "1,272 TESTS GREEN" (script OK).
- L124: "Agent Card (/api/v1/a2a/agents)" → "Agent Card (/.well-known/agent-card.json)".
- L129: REST: rutas inexistentes (/recall, /remember, /think, /graph) → `/api/v1/memory/search · /api/v1/memory/write · /api/v1/decide · /health`.
- L152-153: "4 typed contracts" → "2 named contracts · deterministic rules"; "Optional loopback model in shadow mode (v29)" → "Shadow mode · no model provider wired yet (ADR-021)" (contradice el README: no hay proveedor de modelo).
- L186: "Jina Reranker v1 Turbo / v2" → "v1 Turbo" (sólo existe la v1 turbo en el código).

## Diagramas revisados

- `docs/assets/architecture.svg` (fuente editable): **corregido** en sus etiquetas (8 líneas, ver arriba). La estructura (cliente → gateways → núcleo de 6 herramientas → SilvaDB → inferencia local → 45 guilds → mesh) sigue fiel a STATUS.md. No se rediseñó nada.
- **`docs/assets/architecture.png` NO regenerado**: sigue mostrando las etiquetas antiguas (1,154 tests, "4 contracts", etc.). Hace falta regenerarlo desde el SVG corregido (no hay herramienta de rasterizado en este entorno). **Pendiente para el TL.**
- `docs/assets/architecture_interactive.html`: no revisado en detalle (no contiene "1,154"). Puede tener otras etiquetas antiguas; no comprobado.
- Diagrama Mermaid del README: estructura consistente con STATUS.md; sólo cambiado el nodo SPARSE. No se añadió un Mermaid nuevo de memoria/defensas: el README ya incluye CoherenceGate L1-L4 y SilvaDB en el Mermaid.
- Nota: el Mermaid dice "Tasks · Streaming SSE" para A2A; no comprobado.

## Lo que NO pude verificar

- Métricas de arranque en ARM/Raspberry Pi (ya quitadas del README; no hay medición).
- Interop real con LangGraph/CrewAI/AutoGen (quitados).
- Inmunidad de `agent_summary`/`session_digest` a la poda automática.
- Campo JSON exacto del ejemplo `POST /api/v1/memory/write` (`content`).
- Lista completa de modelos de embedding (`mxbai-q`, `e5`).
- Tamaño de descarga de `server` (~670 MB) y RAM de `server`.
- Delta "+5 net de 245" de Decima-small.
- Tag v0.17.1 en git (sólo comprobé la versión en Cargo.toml).
- Captura de pantallas (existen; no inspeccionadas visualmente).
- Que los `docs/` enlazados por el README sigan vigentes en contenido (sólo existencia y enlaces).

## Pendientes / hallazgos para el TL (fuera del alcance de README.md)

1. **Privacidad (no grave, a confirmar):** la URL `github.com/forja-orca/tylluan` (badge CI, install.sh/ps1 e Discussions) contiene "forja-orca". Lo he dejado porque es el repo público y romper los enlaces de instalación es peor. Confirmar que el nombre del repo público es ese. No hay nombres de proyectos privados, rutas de máquina, tokens ni nombres de agentes de la flota en README.md ni en el SVG tras los cambios.
2. **README.es.md y README.zh.md** replican las cifras antiguas (p. ej. tabla de perfiles, "1272" bien, pero ahí están también las cifras de arranque y "45 tools"). No los he tocado (fuera de mi permiso). Deben sincronizarse.
3. **docs/getting-started/QUICKSTART.md:75** repite "~34 MB resident idle" sin fuente primaria: el README ya no lo publica; QUICKSTART quedaría sin respaldo.
4. **STATUS.md:13 (pendientes abiertos):** el listener P2P (`p2p.enabled`, apagado por defecto) ejecuta cualquier guild para un par aprobado sin lista de permitidos. Contradice en parte el "deny-by-default" de ADR-019 paso 2 si el listener está activo. No lo cito en el README; conviene que el TL lo revise antes de publicar la sección de federación.
5. **Regenerar `docs/assets/architecture.png`** desde el SVG corregido.
6. **Commits:** no he hecho add/commit/push/stash. Cambios sólo en el worktree.

## Impact

Para el lector público de GitHub:
- Las cifras que se mantienen son las que tienen fuente en el repo (versión 0.17.1, 1272 tests, 45 guilds, 6 herramientas, puerto 47004, benchmark LongMemEval v0.12.0 con su fecha y modelo).
- Desaparecen las cifras de rendimiento de arranque y de recall de los perfiles `portable` y `clinic`, que no se podían reproducir desde el repo. Si se quieren de vuelta, debe publicarse primero el fichero de medición.
- El requisito de Rust ahora es el toolchain fijado (1.99.1); antes decía "1.88+" sin fuente.
- `tylluan-cli start --setup` y `dev_mode = true` son los comandos correctos (antes `tylluan start` y `--dev`, que no existen tal cual).
- El dashboard de una descarga normal no aparece en :47004 salvo que el binario se compile con `bundled-dashboard`.
- La arquitectura visual corrige el número de tests, las rutas REST, los contratos de decisión y el proveedor de modelo (hoy no hay).
- La imagen PNG del diagrama todavía muestra las etiquetas antiguas hasta que se regenere.
