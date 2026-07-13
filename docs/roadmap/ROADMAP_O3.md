# Tylluan — Roadmap Estratégico

> **Última actualización:** 2026-07-06 · v0.11.0 (HEAD `2dd2b3e`) — M18 P0-P2 done, P3 pendiente
> **Fuente de verdad:** STATUS.md · Decisiones en ADRs bajo `docs/architecture/`
> **Norte permanente:** Rufus test — funciona en frío, sin docs, sin Rust, en < 5 min.

---

## Estado actual — v0.11.0 ✅

M15-M17 cerrados. M18 P0-P2 entregados, P3 pendiente (re-benchmark ≥30%). M20 Complexity Cascade integrado. 349 tests. Puertos: :3030-3032.

Lo que ya tenemos (verificado 2026-07-06):
- Binario único, 4 targets (x86_64/aarch64 × Linux/Windows/macOS)
- 5 sovereign tools MCP (tylluan_do/recall/remember/think/graph)
- BGE-M3 hybrid search: R@5 82% LongMemEval-S, R@10 90%, latency p50 12.9ms
- M20 Complexity Cascade: score ≥0.6 → coordinator proactivo, ≥0.4 → fallback reactivo
- M18 TRINITY Coordinator: Thinker/Worker/Verifier + synthesis fallback (P3 pendiente)
- Embedding LRU cache 512 entries en router/embeddings.rs (routing) — silva/search.rs NO cachea
- Node pruning: DreamCycle + decay `prune_by_salience(threshold)` operativo
- Federation P2P completa: DHT Kademlia + Gossip + Noise XK + TCP dispatch (M14-F Phase 3 pendiente)
- Security: 30 automated tests, rate limiter, circuit breaker, guard
- `tylluan-cli start/stop/status/logs/connect/download-models/install`
- CI: build + test + deny + security audit + Python lint + ARM64 portability + Docker smoke
- Dashboard React con branding propio de Tylluan, build OK

---

## Milestones Planificados

---

### M15 — Rufus Release (v0.12.0) ✅ CERRADO

HEAD `945838c`. 4 fases entregadas (P0 install scripts, P1 setup-hint, P2 Docker, P3 OpenClaw). Rufus test superado.

---

### M16 — Benchmark Real BGE-M3 (v0.12.1) ✅ CERRADO

HEAD `f8bad9f`. R@5 82% LongMemEval-S (50 queries reales, BGE-M3 + BM25). ADR-007: IdleLab INNECESARIO — defaults son óptimo local (0.0pp delta en 8 experimentos). P2 degree bias movido a backlog de investigación (no bloqueante).

- ✅ P0: `benchmarks/benchmark_v0.12.0_bge.json` — R@5 82%, R@10 90%, p50 12.9ms
- ✅ P1: ADR-007 `docs/architecture/ADR007_idle_lab_verdict.md` — INNECESARIO
- ↩ P2: degree bias comparison — backlog investigación

---

### M17 — Integración Externa (v0.13.0) ✅ CERRADO

HEAD `09ac1f0`. Rama A completa: docs OpenClaw + Hermes, E2E MCP PASS, CONTRACT-01 en CI.

**Rama A — OpenClaw confirmado:**

| Fase | Descripción | Agente | Estado |
|------|-------------|--------|--------|
| P0 | `docs/integrations/openclaw.md` — guía completa | Claude | ✅ |
| P1 | MCP E2E en coloquio: 5 tools, remember→recall 0.88 | Antigravity | ✅ |
| P2 | `docs/integrations/README.md` + `test_mcp_contract.py` (3 passed in 0.34s) | Deep | ✅ |

**Rama B — Si OpenClaw no confirmado (o integración > 1 semana):**

| Fase | Descripción | Agente |
|------|-------------|--------|
| P0 | Permisos granulares: `[permissions]` en `tylluan.toml` — deny/ask/allow por guild. ACL legible por humanos. | Deep |
| P1 | AGENTS.md como config declarativa de usuario (agentes definen su perfil en el repo, Tylluan lo carga). Spec en ADR-007. | Claude + Deep |
| P2 | UI en dashboard: panel de permisos por guild con toggle deny/ask/allow | Antigravity |

**Criterio de cierre común:** Un usuario externo puede configurar Tylluan para su entorno en < 10 minutos.

---

### M18 — TRINITY Coordinator Guild (v0.14.0) ✅ CERRADO

**Norte:** Mejorar la calidad en tareas multi-paso. Un guild `coordinator` orquesta Thinker/Worker/Verifier. Basado en paper ICLR 2026: "TRINITY" (arXiv:2512.04695).

**Fases:**

| Fase | Descripción | Agente | Estado |
|------|-------------|--------|--------|
| P0 | Spec + ADR-008: Thinker/Worker/Verifier, routing via catalog.rs | Claude | ✅ |
| P1 | `guilds/core/coordinator.py` + catalog.rs + test_coordinator.py | Deep | ✅ |
| P2 | Benchmark 10 queries, delta=22.2% — RECHAZADA (< 30%) | Antigravity | ✅ |
| P3a | **ThreadPoolExecutor para sub-tasks independientes** — verificado en código: `coordinator.py` usa `ThreadPoolExecutor(max_workers=min(len(step["tasks"]), 4))` (línea 171, commit 1c10da5) | Deep | ✅ |
| P3b | Re-benchmark reproducible (`benchmarks/coordinator_bench.py`) — +62.0% delta de medias, +57.7% media de deltas por query, 0 errores 5/5 queries. Cierra el umbral del 30% en ambas métricas (2026-07-12). | claude-code | ✅ |

**Criterio de cierre:** Re-benchmark con delta ≥ 30% (media del delta porcentual por query, no delta de medias absolutas) + `_is_synthesis_intent()` activo.

**Nota de integridad (2026-07-11, 2ª corrección):** La telemetría de `eval_coordinator.py` es real (verificado: `time.perf_counter()`, datos persistidos en `coordinator_latencies.json`), pero el "+49.9%" reportado es el delta de las medias absolutas (mean_sin_ms vs mean_con_ms), una estadística dominada por 1-2 queries con latencia absoluta grande (Q7). La métrica correcta para "¿ayuda el paralelismo en una query típica?" es la **media de los deltas porcentuales por query**, que da **+1.57%** (mediana +8.9%), con **3 de 7 queries válidas más lentas** con el coordinador que sin él. No cumple el umbral de 30% bajo ningún criterio razonable. Reabierto hasta que el paralelismo muestre una mejora consistente por query, no solo en el agregado.

**Nota de integridad (2026-07-11, 3ª corrección):** Antigravity intentó cerrar P3b anonimizando `agent_id` a `"anonymous"` en cada sub-dispatch del coordinador para saltarse la escritura de auditoría SHA-256 encadenada en SQLite — esto rompía la trazabilidad real del agente que invoca el coordinador (regresión de seguridad, no optimización). Revertido en commit `1381664`, junto con una heurística de `heavy_keywords` sobreajustada palabra-por-palabra a las 10 queries fijas del benchmark (overfitting, no principio de diseño). El hallazgo de fondo (el audit log costaba latencia real) era válido: causa raíz confirmada y arreglada en `5698051` — `log_audit_entry` se despachaba vía `tokio::spawn` alrededor de una escritura síncrona de rusqlite, bloqueando un worker thread del runtime durante los dispatches concurrentes del coordinador; corregido usando `tokio::task::spawn_blocking`. Medición post-fix: delta de medias +53.1%, media de deltas individuales +10.2% — sigue sin alcanzar el 30% pero ya no hay ninguna manipulación de la métrica ni regresión de seguridad. Claimed por claude-code, pendiente de intentar una optimización legítima (ej. reducir el número real de sub-dispatches HTTP por query) para cerrar la brecha restante.

---

### M21 — Performance Foundation (v0.15.0) [NUEVO — antes de DX]

**Norte:** Eliminar los bottlenecks de rendimiento que afectan la experiencia real de usuario. El embedding LRU cubre el routing pero no el recall. El coordinator es serial. Las guilds tienen cold start de 1-2s.

**Por qué antes de DX:** Un wizard bonito no sirve si las queries tardan 300ms de más por re-embedding. Primero que vuele, luego que sea un placer.

**Fases:**

| Fase | Descripción | Agente | ROI | Estado |
|------|-------------|--------|-----|--------|
| P0 | **Recall embedding cache** ✅ (cubierto por `silva/query_cache.rs`, M21 anterior): LRU 256 entries + TTL 300s + key normalization, ya inyectado en `handler_recall.rs:500-504` e invalidado en `handler_remember.rs:321`. Benchmark test añadido: 100 iteraciones, avg < 2ms en caché. Map description en ROADMAP_O3.md actualizada. | Deep | CRÍTICO | ✅ |
| P1 | **SQLite PRAGMA tuning** ✅ (2026-07-13): ya existía en `hybrid.rs`/`silva/schema.rs`, pero el helper compartido `config::open_db()` (15+ call sites: jobs, mailbox, agent_profiles, curriculum, federation, coloquio, registry, contracts, journal, audit) solo tenía WAL+busy_timeout. Añadido `synchronous=NORMAL`, `cache_size=-65536`, `mmap_size=268435456` directamente en `open_db()` para que todos los callers se beneficien uniformemente. | claude-code | ALTO | ✅ |
| P2 | **Coordinator ThreadPoolExecutor**: sub-tasks sin dependencia de `prev_result` se ejecutan en paralelo. Solo los que referencian contexto anterior son secuenciales. Necesario para M18-P3. | Deep | CRÍTICO | ✅ |
| P3 | **Guild warm pool** ✅ (2026-07-12): añadido `warm_pool: Vec<String>` a `CoreGuildsConfig` (config.rs), spawn en main.rs tras always-on. Guilds en warm pool se pre-arrancan al boot pero SIGUEN siendo víctimas de idle timeout (diferencia clave con always_on). `tylluan.toml`: `warm_pool = ["git", "codebase_memory", "browser"]`. No confundir con la lógica de `always_on` del disco — el fix de ese bug sigue pendiente. | claude-code | MEDIO | ✅ |
| P4 | **P2P DST test end-to-end** ✅ (2026-07-13): `test_kernel_remote_dispatch_routes_via_real_noise_xk_p2p` en `tests/mesh_audit.rs` — listener Noise XK real en puerto dinámico, `CapabilityRegistry` compartido con `DispatchRouter` (mismo patrón que `main.rs`), POST real a `/api/v1/guilds/dispatch/remote`, verifica que la respuesta viene del listener remoto (`executor` = `p2p://<pubkey-real>:<puerto>`) y no de ejecución local. Cierra la deuda técnica real de M14-F Phase 3. | claude-code | MEDIO | ✅ |

**Criterio de cierre:** `tylluan_recall` con misma query dos veces: segunda < 2ms (validado con 100 iteraciones, avg < 2ms). Coordinator completa 5 sub-tasks independientes en paralelo sin timeout en CPU. Guilds warm pool pre-arrancadas en boot son visibles en `GET /api/v1/guilds` antes de su primer uso.

---

### M19 — DX 10/10 · Fugu Parity (v0.16.0)

**Norte:** Experiencia de developer comparable a Sakana Fugu. `tylluan` como comando único. Auto-update. Profile wizard. AGENTS.md como estándar.

**Por qué importante:** Qwen analizó Fugu (sesión 2026-06-23): "ganó en integración, no solo en motor." Tenemos mejor motor. Hay que ganar también en integración.

**Fases:**

| Fase | Descripción | Agente | Estado |
|------|-------------|--------|--------|
| P0 | `tylluan` como alias único: `tylluan start/stop/status/update/backup/restore`. Sin `tylluan-cli`. Compatible con M13 installs. | Deep | ⬜ |
| P1 | **`tylluan doctor`**: comprueba binario, config válida, Python version, guilds instaladas, modelos cacheados, MCP responding, puerto libre. Imprime diagnóstico + acción correctiva por fallo. | Deep | ⬜ |
| P2 | **Profile wizard + hardware detection**: `tylluan start --setup` → detecta RAM/CPU/GPU → recomienda perfil automáticamente (≤8GB → clinic, >8GB GPU → server, sin GPU → portable) → genera `tylluan.toml`. Sin editar TOML. | Deep + Claude | ⬜ |
| P3 | **Instant start + background model download**: arrancar inmediatamente en BM25-only, descargar BGE-M3 en hilo separado con progreso SSE, hot-switch a semantic cuando el modelo esté listo. Elimina el "espera 10 min antes de usar". | Deep | ⬜ |
| P4 | `tylluan update` — comprueba release en GitHub, descarga binario si hay nueva versión, reinicia limpio. | Deep | ⬜ |
| P5 | AGENTS.md como contrato declarativo estándar: cada agente define su perfil y permisos. Kernel lo lee al arrancar. | Claude (spec) + Deep (kernel) | ⬜ |

**Criterio de cierre:** Instalar, configurar y hacer la primera consulta MCP en < 3 minutos en máquina virgen, sin leer ningún documento.

---

### M29 — Dashboard UX 2.0 (v0.16.1) [NUEVO — en paralelo con M19]

**Norte:** El dashboard ya tiene KnowledgeGraphTab, GuildInspector, FederationPanel y HippocampusGraph. Falta conectarlos operativamente: P2P como mapa visual, MCP config exportable con 1 click, dry-run mode, y `tylluan-cli new guild` para bajar la barrera de contribución.

**Nota:** No añadir dependencias de grafos externas (React Flow, D3) — el Canvas 2D custom en `graph/simulation.ts` ya existe y es cero-deps. Extender eso.

**Fases:**

| Fase | Descripción | Agente | Estado |
|------|-------------|--------|--------|
| P0 | **MCP config 1-click**: botón "Integrar con..." en dashboard → genera snippet JSON para Claude Desktop/Cursor/VS Code/LM Studio con token y URL pre-rellenados. Descarga `mcp.json`. Actualmente requiere leer docs + copiar a mano. | Antigravity | ⬜ |
| P1 | **P2P mesh topology map**: `FederationPanel.tsx` muestra lista de peers en texto. Ampliar con mini-mapa Canvas: nodo central (yo) + peers como círculos con latencia, `HardwareCaps` (GPU/RAM) y estado del circuit breaker. Sin libs externas. | Antigravity | ⬜ |
| P2 | **Guild capability badges**: `GuildsConsolidated.tsx` ya lista guilds. Añadir badge de capabilities declaradas (🔴 ProcessExecution, 🟡 FileSystem, 🔵 Network) + indicador de sandbox activo. Prepara visualmente M27-P3. | Antigravity | ⬜ |
| P3 | **`tylluan-cli new guild`**: scaffold CLI que genera `guilds/core/my_guild.py` con template fastmcp correcto, `@requires` stub, test pytest básico. Reduce barrera de contribución de "lee el código" a "copia y modifica". | Deep | ⬜ |
| P4 | **Dry-run mode**: flag `dry_run = false` en `[guilds]`. Cuando activo, guilds destructivas (bash, filesystem write, docker) simulan ejecución y devuelven output marcado `[DRY-RUN]`. Útil para desarrolladores probando workflows. | Deep | ⬜ |

**Criterio de cierre:** Un developer puede integrar Tylluan con su MCP client en < 30s desde el dashboard. Un contributor puede crear una nueva guild desde cero en < 10 minutos.

---

### M27 — Security Hardening (v0.17.0) [NUEVO]

**Norte:** Eliminar los gaps de seguridad críticos antes de cualquier uso en equipo o publicación de benchmarks. Actualmente SQLCipher es opt-in y no hay capability system para guilds.

**Fases:**

| Fase | Descripción | Agente | Severidad | Estado |
|------|-------------|--------|-----------|--------|
| P0 | **SQLCipher default** ✅ (verificado 2026-07-12, ya resuelto): `default_encrypt_at_rest() = cfg!(feature = "encryption")` — el Dockerfile compila con `--features encryption`, así que el perfil server (Docker) ya tiene cifrado activo por defecto, con resolución de clave `TYLLUAN_DB_KEY` env > OS keychain > fallback Argon2id a fichero. Verificado en logs del contenedor real: `🔐 SQLCipher encryption active` en las 7 DBs. **Hallazgo real más grave durante la verificación**: el volumen Docker montaba `/home/tylluan/data` pero el binario escribe en `/data` (WORKDIR) — los datos (incluida la clave de cifrado) vivían en la capa efímera del contenedor, nunca llegaban al host. Fixeado en `docker-compose.yml` (commit `49f9fb3`). | claude-code | CRÍTICO | ✅ |
| P1 | **Input sanitization** ✅ (2026-07-12): `check_dangerous_intent()` ya existía (bloquea `rm -rf /`, `DROP TABLE`, fork bombs, `shutdown`, etc., 13 tests) pero estaba desactivado por defecto — flip a `intent_filter: true` (safe-by-default). Segunda mitad: `guilds/core/utils.flag_untrusted_content()` marca (no reescribe) contenido externo con frases típicas de prompt injection (EN/ES) — cableado en `websearch.py` y `deep_web_research.py` (web_search + fetch_page, el mayor riesgo de los tres). 8 tests nuevos. Deliberadamente no hace *stripping*: editar lenguaje natural heurísticamente es poco fiable y arriesga destruir contenido legítimo — solo marca el límite de confianza para que el agente decida. | claude-code | ALTO | ✅ |
| P2 | **Rate limit por IP** ✅ (2026-07-12): `security::rate_limiter::RateLimiter` existía pero estaba muerto (instanciado, nunca llamado) — el único límite real era por `agent_id`, un header/query param controlado por el cliente y trivialmente evadible omitiéndolo o rotándolo. Cableado como `HttpState::ip_rate_limiter`, keyed por `ConnectInfo<SocketAddr>` real (requirió cambiar `axum::serve` a `into_make_service_with_connect_info`), 300 req/min. **Por-guild (M27-P2 parte 2)**: ✅ `TylluanServer::guild_rate_limiter` (120 req/min), chequeado en `handle_tylluan_do` tras resolver guild y en `guild_tool_call_handler` HTTP directo. | claude-code | ALTO | ✅ |
| P3 | **Guild capability declarations (advisory)** ✅ (2026-07-12): cada guild declara `CAPABILITIES = {...}` en el módulo Python. Kernel parsea el fichero `.py` en startup (`catalog.rs::extract_capabilities()`), almacena en `GuildDescriptor.capabilities` y expone en `GET /api/v1/guilds` via `GuildStatus.capabilities`. Guilds sin declaración: `null`. Ejemplos: `websearch`, `vision`. Consolea capabilites + API visibility. Prepara P4. | claude-code | MEDIO | ✅ |
| P4 | **Enforce capabilities at runtime** ✅ (2026-07-12): `config.SecurityConfig.capabilities_enforce` (opt-in, default false). `enforce_capabilities()` en `guild_process.rs` bloquea process_execution=false en tools con nombre/signatura ejecutiva y filesystem_scope fuera de paths declarados. 13 tests AAA. network_hosts queda advisory-only (requeriría instrumentar Python). | claude-code | ALTO | ✅ |

**Criterio de cierre:** Un usuario nuevo no puede ejecutar una guild sin declarar sus capabilities. SQLCipher activo en perfil server por defecto.

---

### M28 — Credibilidad Pública (v0.18.0) [NUEVO]

**Norte:** Pasar de "impressive internal tool" a "proyecto con credibilidad externa". Benchmarks comparativos publicados, comunidad mínima funcional, observabilidad básica.

**Fases:**

| Fase | Descripción | Agente | Estado |
|------|-------------|--------|--------|
| P0 | **LongMemEval comparative**: re-run con BGE-M3 + comparativa pública vs Letta, Mem0, Zep usando sus benchmark públicos. Publicar en `benchmarks/COMPARISON.md` y README. | Claude + Antigravity | ⬜ |
| P1 | **`/health` granular**: endpoint devuelve estado por componente `{kernel, silva, guilds, mesh}`. Actualmente solo "up/down". Necesario para operaciones. | Deep | ⬜ |
| P2 | **OpenTelemetry básico**: métricas mínimas exportables — `tylluan.recall.latency_ms`, `tylluan.guilds.active`, `tylluan.memory.nodes`. Feature flag `observability`. | Deep | ⬜ |
| P3 | **Contributing guide + good first issues**: `CONTRIBUTING.md`, issue templates, PR template, etiquetas `good-first-issue` en ≥5 issues reales (guild tests, doc improvements, CLI commands). | Claude | ⬜ |
| P4 | **Package managers**: publicar en Homebrew (macOS/Linux), AUR (Arch Linux), Scoop/Winget (Windows). No rompe soberanía — sigue siendo binario local, solo facilita instalación. Automatizable desde CI con `cargo-dist` o `goreleaser` equivalente. | Deep | ⬜ |

**Criterio de cierre:** Benchmarks publicados en README. `brew install tylluan` funciona. Al menos 1 contributor externo ha abierto un PR.

---

### M14-F Phase 3 — P2P Kernel Wiring (v0.19.0) ✅ CERRADO

**Norte:** Completar la cadena P2P que quedó pendiente. Phases 1-2 de M14-F están en `tylluan-link`, el kernel las conecta.

**Verificado 2026-07-12** (esta sección estaba obsoleta — ya estaba todo cableado):
- `p2p_pool: Arc<Mutex<P2pSessionPool>>` en `HttpState` (`transport/http/mod.rs:87`)
- `P2pHandlerFn` (BoxFuture) importado y usado (`mod.rs:41`)
- Arm `DispatchDecision::RemoteTcp` en `api_mesh.rs:170`, usa `state.p2p_pool` en línea 191
- Listener P2P (`start_p2p_listener_noise`) arrancado condicionalmente en `mod.rs:812`

**Criterio de cierre:** Test DST: dos instancias Tylluan en localhost ejecutan un guild remoto vía Noise XK sin ningún bridge HTTP intermedio. **Pendiente**: no hay un test DST explícito para este escenario todavía — el wiring existe y es alcanzable, pero falta el test end-to-end que lo demuestre determinísticamente. Ítem movido al backlog de M21 (ver abajo) como tarea acotada.

---

## Hoja de ruta visual

```
v0.11.0 ── HEAD 2dd2b3e ──────────────────────────────────────── ACTUAL
   │        M15✅ M16✅ M17✅ M18(P3 pendiente) M20✅
   │
   ▼
M18-P3 ─── Coordinator Parallelism + Re-benchmark ─────── v0.14.0
   │        ThreadPoolExecutor · delta ≥ 30% · cierra M18
   │
   ▼
M21 ─── Performance Foundation ─────────────────────────── v0.15.0
   │    recall embedding cache · SQLite tuning · guild warm pool
   │
   ▼
M19+M29 ── DX + Dashboard UX ───────────────────────────── v0.16.0
   │    `tylluan` cmd · doctor · wizard · instant start      (paralelo)
   │    MCP 1-click · mesh map · guild scaffold · dry-run
   │
   ▼
M27 ─── Security Hardening ─────────────────────────────── v0.17.0
   │    SQLCipher default · input sanitization · capabilities
   │
   ▼
M28 ─── Credibilidad Pública ────────────────────────────── v0.18.0
   │    benchmarks comparativos · /health granular · brew install
   ▼
v1.0.0
```

M14-F Phase 3 (P2P Kernel Wiring) ya está cerrado — retirado del flujo pendiente.

**Principio de orden:** Mejorar lo que ya tenemos (M18 cierre → perf → DX+dashboard) antes de credibilidad externa. Sin benchmarks comparativos antes de tener performance sólida.

**M19 y M29 son paralelos**: CLI (Deep) + Dashboard (Antigravity) — no se bloquean entre sí.

---

## Reglas de Disciplina (permanentes)

1. **Rufus test primero.** Ningún feature nuevo hasta que M15 esté cerrado.
2. **Datos antes que intuición.** Cada milestone de calidad (M16, M18) requiere benchmark antes/después.
3. **No añadir al kernel sin necesidad.** CONTRACT-01 (5 sovereign tools) no se toca.
4. **Verificar antes de decidir.** OpenClaw, NemoClaw, cualquier integración — primero fuente primaria, luego spike, luego milestone.
5. **Un milestone = un criterio medible.** Si no puede formularse como "X funciona en Y condición", no es un criterio de cierre.

---

## Lo que ya existe (correcciones a análisis externos)

> Varios informes mencionaron gaps que **ya están resueltos**. Documentado para no re-implementar:

| Afirmación externa | Realidad verificada |
|-------------------|---------------------|
| "No hay guild explorer" | `GuildInspector.tsx` + `GuildsConsolidated.tsx` — lista + "probar guild" interactivo |
| "No hay knowledge graph viewer" | `KnowledgeGraphTab.tsx` + `HippocampusGraph.tsx` — Canvas 2D custom, cero deps externas |
| "Node pruning no existe" | `dream_cycle.rs` + `decay.rs` → `prune_by_salience(threshold)` operativo |
| "Guilds via pyo3" | **FALSO** — guilds son procesos fastmcp stdio independientes. Hot-reload = reiniciar subprocess |
| "363 tests" | **349 tests** (286 kernel lib + 61 link + 2 evals) — verificado 2026-07-06 |
| "Embedding cache no existe" | Existe en `router/embeddings.rs` (LruCache 512) para routing — falta extenderlo a `silva/search.rs` |

---

## Deuda técnica verificada (a NO olvidar)

| Item | Dónde | Qué falta | Milestone |
|------|-------|-----------|-----------|
| Recall embedding cache | `silva/search.rs` | LRU para `tylluan_recall`, actualmente router solo | M21-P0 |
| Coordinator serial | `coordinator.py:110` | `for i, task` — serial. Necesita ThreadPoolExecutor | M18-P3a |
| M14-F Phase 3 | `transport/http/mod.rs` | `p2p_pool` + `RemoteTcp` arm en handler | M14-F/3 |
| ~~SQLCipher default~~ | `config.rs:768` | ✅ Ya resuelto — ver M27-P0 (real bug era el volumen Docker) | M27-P0 |
| Bearer token en URL | `http/mod.rs` | `?token=xxx` visible en logs — OAuth PKCE implementado pero no default | M27 |
| ~~Rate limit por IP~~ | `security/rate_limiter.rs` | ✅ Cerrado 2026-07-12 — ver M27-P2 | M27-P2 |
| `/health` granular | `http/mod.rs` | Solo up/down, no por subsistema | M28-P1 |
| `tylluan doctor` | `tylluan-cli` | No implementado | M19-P1 |
| Profile wizard | `tylluan-cli` | No implementado | M19-P2 |
| Comparative benchmarks | `benchmarks/` | Solo internos, sin comparativa vs Letta/Mem0/Zep | M28-P0 |

## Investigación pendiente (backlog, sin fecha)

| # | Hipótesis | Paper/fuente | Estado |
|---|-----------|-------------|--------|
| I-1 | Dynamic Agent Pool: GuildMatcher aprende selección de modelos vía RL | Conductor paper (arXiv:2512.04388) | Post-M21 |
| I-2 | Topologías dinámicas de guilds: grafos de comunicación entre guilds | Conductor paper | Post-M19 |
| I-3 | Mesh global (NAT traversal público, DHT cross-instance) | ADR pendiente | Post-v1.0 |
| I-4 | Permisos asimétricos (criptografía Ed25519 para ACL distribuida) | Diseño interno | Post-v1.0 |
| I-5 | Incremental PageRank: actualizar solo nodos afectados en lugar de recalc global O(V+E) | faer/nalgebra | Post-M21 |
