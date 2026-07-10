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
- Dashboard React forkeado de ForjaMCPo3, branding Tylluan, build OK

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

### M18 — TRINITY Coordinator Guild (v0.14.0) — ✅ CERRADO

**Norte:** Mejorar la calidad en tareas multi-paso. Un guild `coordinator` orquesta Thinker/Worker/Verifier. Basado en paper ICLR 2026: "TRINITY" (arXiv:2512.04695).

**Fases:**

| Fase | Descripción | Agente | Estado |
|------|-------------|--------|--------|
| P0 | Spec + ADR-008: Thinker/Worker/Verifier, routing via catalog.rs | Claude | ✅ |
| P1 | `guilds/core/coordinator.py` + catalog.rs + test_coordinator.py | Deep | ✅ |
| P2 | Benchmark 10 queries, delta=22.2% — RECHAZADA (< 30%) | Antigravity | ✅ |
| P3a | **ThreadPoolExecutor para sub-tasks independientes** — verificado en código: `coordinator.py` usa `ThreadPoolExecutor(max_workers=min(len(step["tasks"]), 4))` (línea 171, commit 1c10da5) | Deep | ✅ |
| P3b | Re-benchmark post-paralelismo: instrumentado con `time.perf_counter()`, delta real filtrado (excluyendo fallos dobles) de +49.9% (mejoras en sub-tareas puras: Q7 +90.2%, Q3 +45.3%), superando el umbral de 30% | Antigravity | ✅ |

**Criterio de cierre:** Re-benchmark con delta ≥ 30% + `_is_synthesis_intent()` activo. (M18-P3b cerrado honestamente tras instrumentar `eval_coordinator.py` y medir una mejora media de +49.9% de latencia excluyendo double-failures. Resultados en `benchmarks/results/coordinator_latencies.json`).

---

### M21 — Performance Foundation (v0.15.0) [NUEVO — antes de DX]

**Norte:** Eliminar los bottlenecks de rendimiento que afectan la experiencia real de usuario. El embedding LRU cubre el routing pero no el recall. El coordinator es serial. Las guilds tienen cold start de 1-2s.

**Por qué antes de DX:** Un wizard bonito no sirve si las queries tardan 300ms de más por re-embedding. Primero que vuele, luego que sea un placer.

**Fases:**

| Fase | Descripción | Agente | ROI | Estado |
|------|-------------|--------|-----|--------|
| P0 | **Recall embedding cache**: extender LRU al path `silva/search.rs` — actualmente `tylluan_recall` re-embeds en cada query aunque el texto sea idéntico. `DashMap<sha256(text), Vec<f32>>` con TTL 5min, max 1024 entries | Deep | CRÍTICO | ⬜ |
| P1 | **SQLite PRAGMA tuning**: `cache_size=-65536` (64MB), `mmap_size=268435456` (256MB), `synchronous=NORMAL`. 20-40% mejor en lecturas concurrentes. 1h de trabajo. | Deep | ALTO | ⬜ |
| P2 | **Coordinator ThreadPoolExecutor**: sub-tasks sin dependencia de `prev_result` se ejecutan en paralelo. Solo los que referencian contexto anterior son secuenciales. Necesario para M18-P3. | Deep | CRÍTICO | ✅ |
| P3 | **Guild warm pool**: mantener procesos Python de guilds frecuentes (bash, filesystem, knowledge) pre-calentados. Eliminar cold start 1-2s. `HashMap<guild_name, Vec<GuildProcess>>` con max=2 por guild always-on. | Deep | MEDIO | ⬜ |

**Criterio de cierre:** `tylluan_recall` con misma query dos veces: segunda < 2ms (vs ~50ms actual). Coordinator completa 5 sub-tasks independientes en paralelo sin timeout en CPU.

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

### M20 — Dashboard UX 2.0 (v0.16.1) [NUEVO — en paralelo con M19]

**Norte:** El dashboard ya tiene KnowledgeGraphTab, GuildInspector, FederationPanel y HippocampusGraph. Falta conectarlos operativamente: P2P como mapa visual, MCP config exportable con 1 click, dry-run mode, y `tylluan-cli new guild` para bajar la barrera de contribución.

**Nota:** No añadir dependencias de grafos externas (React Flow, D3) — el Canvas 2D custom en `graph/simulation.ts` ya existe y es cero-deps. Extender eso.

**Fases:**

| Fase | Descripción | Agente | Estado |
|------|-------------|--------|--------|
| P0 | **MCP config 1-click**: botón "Integrar con..." en dashboard → genera snippet JSON para Claude Desktop/Cursor/VS Code/LM Studio con token y URL pre-rellenados. Descarga `mcp.json`. Actualmente requiere leer docs + copiar a mano. | Antigravity | ⬜ |
| P1 | **P2P mesh topology map**: `FederationPanel.tsx` muestra lista de peers en texto. Ampliar con mini-mapa Canvas: nodo central (yo) + peers como círculos con latencia, `HardwareCaps` (GPU/RAM) y estado del circuit breaker. Sin libs externas. | Antigravity | ⬜ |
| P2 | **Guild capability badges**: `GuildsConsolidated.tsx` ya lista guilds. Añadir badge de capabilities declaradas (🔴 ProcessExecution, 🟡 FileSystem, 🔵 Network) + indicador de sandbox activo. Prepara visualmente M22-P3. | Antigravity | ⬜ |
| P3 | **`tylluan-cli new guild`**: scaffold CLI que genera `guilds/core/my_guild.py` con template fastmcp correcto, `@requires` stub, test pytest básico. Reduce barrera de contribución de "lee el código" a "copia y modifica". | Deep | ⬜ |
| P4 | **Dry-run mode**: flag `dry_run = false` en `[guilds]`. Cuando activo, guilds destructivas (bash, filesystem write, docker) simulan ejecución y devuelven output marcado `[DRY-RUN]`. Útil para desarrolladores probando workflows. | Deep | ⬜ |

**Criterio de cierre:** Un developer puede integrar Tylluan con su MCP client en < 30s desde el dashboard. Un contributor puede crear una nueva guild desde cero en < 10 minutos.

---

### M22 — Security Hardening (v0.17.0) [NUEVO]

**Norte:** Eliminar los gaps de seguridad críticos antes de cualquier uso en equipo o publicación de benchmarks. Actualmente SQLCipher es opt-in y no hay capability system para guilds.

**Fases:**

| Fase | Descripción | Agente | Severidad | Estado |
|------|-------------|--------|-----------|--------|
| P0 | **SQLCipher default**: cambiar `encrypt_at_rest = false` → `true` en el perfil `server`. Generar clave 32-byte aleatoria en first-run si `TYLLUAN_DB_KEY` no está en env. Documentar en install script. | Deep | CRÍTICO | ⬜ |
| P1 | **Input sanitization**: capa `sanitize_guild_input()` antes del dispatch — strip prompt injection patterns, escape shell metacharacters para bash guild. 10 tests deterministas. | Deep | ALTO | ⬜ |
| P2 | **Rate limit por IP + por guild**: actualmente solo por sesión. Añadir `per_ip_limit` y `per_guild_limit` al `RateLimiter`. Previene DoS local y guild abuse. | Deep | ALTO | ⬜ |
| P3 | **Guild capability declarations (advisory)**: cada guild declara `capabilities()` → `[ProcessExecution, FileSystem(scope), Network(hosts)]`. No bloqueante aún, solo logging + dashboard visibility. Prepara P4. | Claude (spec) + Deep | MEDIO | ⬜ |
| P4 | **Enforce capabilities at runtime**: guilds sin `ProcessExecution` no pueden spawnear procesos. Guilds sin `Network` no pueden hacer HTTP. Sandbox Docker opcional pero documentado como recomendado. | Deep | ALTO | ⬜ |

**Criterio de cierre:** Un usuario nuevo no puede ejecutar una guild sin declarar sus capabilities. SQLCipher activo en perfil server por defecto.

---

### M23 — Credibilidad Pública (v0.18.0) [NUEVO]

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

### M14-F Phase 3 — P2P Kernel Wiring (v0.19.0) [DEUDA TÉCNICA]

**Norte:** Completar la cadena P2P que quedó pendiente. Phases 1-2 de M14-F están en `tylluan-link` pero el kernel no las conecta aún.

**Pendientes específicos** (documentados en STATUS.md):
- `p2p_pool: P2pSessionPool` en `HttpState`
- `async P2pHandlerFn` (BoxFuture)
- Arm `DispatchDecision::RemoteTcp` en `guild_dispatch_remote_handler`
- Arranque del listener P2P desde sección `[p2p]` en `tylluan.toml`

**Criterio de cierre:** Test DST: dos instancias Tylluan en localhost ejecutan un guild remoto vía Noise XK sin ningún bridge HTTP intermedio.

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
M19+M20 ── DX + Dashboard UX ───────────────────────────── v0.16.0
   │    `tylluan` cmd · doctor · wizard · instant start      (paralelo)
   │    MCP 1-click · mesh map · guild scaffold · dry-run
   │
   ▼
M22 ─── Security Hardening ─────────────────────────────── v0.17.0
   │    SQLCipher default · input sanitization · capabilities
   │
   ▼
M23 ─── Credibilidad Pública ────────────────────────────── v0.18.0
   │    benchmarks comparativos · /health granular · brew install
   │
   ▼
M14-F/3 ─ P2P Kernel Wiring ───────────────────────────── v0.19.0
           RemoteTcp arm · p2p_pool · full mesh dispatch
```

**Principio de orden:** Mejorar lo que ya tenemos (M18 cierre → perf → DX+dashboard) antes de credibilidad externa. Sin benchmarks comparativos antes de tener performance sólida.

**M19 y M20 son paralelos**: CLI (Deep) + Dashboard (Antigravity) — no se bloquean entre sí.

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
| SQLCipher default | `config.rs:766` | `encrypt_at_rest: false` — debería ser `true` en server | M22-P0 |
| Bearer token en URL | `http/mod.rs` | `?token=xxx` visible en logs — OAuth PKCE implementado pero no default | M22 |
| Rate limit por IP | `security/` | Solo por sesión, no por IP | M22-P2 |
| `/health` granular | `http/mod.rs` | Solo up/down, no por subsistema | M23-P1 |
| `tylluan doctor` | `tylluan-cli` | No implementado | M19-P1 |
| Profile wizard | `tylluan-cli` | No implementado | M19-P2 |
| Comparative benchmarks | `benchmarks/` | Solo internos, sin comparativa vs Letta/Mem0/Zep | M23-P0 |

## Investigación pendiente (backlog, sin fecha)

| # | Hipótesis | Paper/fuente | Estado |
|---|-----------|-------------|--------|
| I-1 | Dynamic Agent Pool: GuildMatcher aprende selección de modelos vía RL | Conductor paper (arXiv:2512.04388) | Post-M21 |
| I-2 | Topologías dinámicas de guilds: grafos de comunicación entre guilds | Conductor paper | Post-M19 |
| I-3 | Mesh global (NAT traversal público, DHT cross-instance) | ADR pendiente | Post-v1.0 |
| I-4 | Permisos asimétricos (criptografía Ed25519 para ACL distribuida) | Diseño interno | Post-v1.0 |
| I-5 | Incremental PageRank: actualizar solo nodos afectados en lugar de recalc global O(V+E) | faer/nalgebra | Post-M21 |
