# Tylluan — Roadmap Estratégico

> **Última actualización:** 2026-07-06 · v0.11.0 (HEAD `2dd2b3e`) — M18 P0-P2 done, P3 pendiente
> **Fuente de verdad:** STATUS.md · Decisiones en ADRs bajo `docs/architecture/`
> **Norte permanente:** Rufus test — funciona en frío, sin docs, sin Rust, en < 5 min.

---

## Estado actual — v0.11.0 ✅

M15-M17 cerrados. M18 P0-P2 entregados, P3 pendiente (re-benchmark ≥30%). M20 Complexity Cascade integrado. 349 tests. Puertos: :4000-4002.

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

### M18 — TRINITY Coordinator Guild (v0.14.0) — P3 PENDIENTE

**Norte:** Mejorar la calidad en tareas multi-paso. Un guild `coordinator` orquesta Thinker/Worker/Verifier. Basado en paper ICLR 2026: "TRINITY" (arXiv:2512.04695).

**Fases:**

| Fase | Descripción | Agente | Estado |
|------|-------------|--------|--------|
| P0 | Spec + ADR-008: Thinker/Worker/Verifier, routing via catalog.rs | Claude | ✅ |
| P1 | `guilds/core/coordinator.py` + catalog.rs + test_coordinator.py | Deep | ✅ |
| P2 | Benchmark 10 queries, delta=22.2% — RECHAZADA (< 30%) | Antigravity | ✅ |
| P3a | **ThreadPoolExecutor para sub-tasks independientes** — causa raíz del timeout en CPU: el loop es serial, tareas paralelas esperan 180s c/u | Deep | ⬜ |
| P3b | Re-benchmark post-paralelismo: estimado delta 40-60% | Antigravity | ⬜ |

**Causa raíz P3:** `coordinate()` en `coordinator.py` itera serial (`for i, task in enumerate(tasks)`). Sub-tasks independientes deberían ejecutarse en paralelo con `concurrent.futures.ThreadPoolExecutor`. Solo los que referencian `prev_result` deben ser secuenciales.

**Criterio de cierre:** Re-benchmark con delta ≥ 30% + `_is_synthesis_intent()` activo.

---

### M21 — Performance Foundation (v0.15.0) [NUEVO — antes de DX]

**Norte:** Eliminar los bottlenecks de rendimiento que afectan la experiencia real de usuario. El embedding LRU cubre el routing pero no el recall. El coordinator es serial. Las guilds tienen cold start de 1-2s.

**Por qué antes de DX:** Un wizard bonito no sirve si las queries tardan 300ms de más por re-embedding. Primero que vuele, luego que sea un placer.

**Fases:**

| Fase | Descripción | Agente | ROI | Estado |
|------|-------------|--------|-----|--------|
| P0 | **Recall embedding cache**: extender LRU al path `silva/search.rs` — actualmente `tylluan_recall` re-embeds en cada query aunque el texto sea idéntico. `DashMap<sha256(text), Vec<f32>>` con TTL 5min, max 1024 entries | Deep | CRÍTICO | ⬜ |
| P1 | **SQLite PRAGMA tuning**: `cache_size=-65536` (64MB), `mmap_size=268435456` (256MB), `synchronous=NORMAL`. 20-40% mejor en lecturas concurrentes. 1h de trabajo. | Deep | ALTO | ⬜ |
| P2 | **Coordinator ThreadPoolExecutor**: sub-tasks sin dependencia de `prev_result` se ejecutan en paralelo. Solo los que referencian contexto anterior son secuenciales. Necesario para M18-P3. | Deep | CRÍTICO | ⬜ |
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
| P1 | **`tylluan doctor`**: comprueba binario, config válida, Python version, guilds instaladas, modelos cacheados, MCP responding. Imprime diagnóstico + acción correctiva para cada fallo. | Deep | ⬜ |
| P2 | Profile wizard en first-run: `tylluan start --setup` → 3 preguntas (GPU?, perfil, nombre agente) → genera `tylluan.toml`. Sin editar TOML a mano. | Deep + Claude | ⬜ |
| P3 | `tylluan update` — comprueba release en GitHub, descarga binario si hay nueva versión, reinicia limpio. | Deep | ⬜ |
| P4 | AGENTS.md como contrato declarativo estándar: cada agente define su perfil y permisos. Kernel lo lee al arrancar. | Claude (spec) + Deep (kernel) | ⬜ |

**Criterio de cierre:** Instalar, configurar y hacer la primera consulta MCP en < 3 minutos en máquina virgen, sin leer ningún documento.

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

**Criterio de cierre:** Benchmarks publicados en README. Al menos 1 contributor externo ha abierto un PR.

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
M19 ─── DX 10/10 · Fugu Parity ─────────────────────────── v0.16.0
   │    `tylluan` cmd único · doctor · wizard · auto-update
   │
   ▼
M22 ─── Security Hardening ─────────────────────────────── v0.17.0
   │    SQLCipher default · input sanitization · capabilities
   │
   ▼
M23 ─── Credibilidad Pública ────────────────────────────── v0.18.0
   │    benchmarks comparativos · /health granular · community
   │
   ▼
M14-F/3 ─ P2P Kernel Wiring ───────────────────────────── v0.19.0
           RemoteTcp arm · p2p_pool · full mesh dispatch
```

**Principio de orden:** Mejorar lo que ya tenemos (M18 cierre → perf → DX) antes de credibilidad externa. Sin benchmarks comparativos antes de tener performance sólida.

---

## Reglas de Disciplina (permanentes)

1. **Rufus test primero.** Ningún feature nuevo hasta que M15 esté cerrado.
2. **Datos antes que intuición.** Cada milestone de calidad (M16, M18) requiere benchmark antes/después.
3. **No añadir al kernel sin necesidad.** CONTRACT-01 (5 sovereign tools) no se toca.
4. **Verificar antes de decidir.** OpenClaw, NemoClaw, cualquier integración — primero fuente primaria, luego spike, luego milestone.
5. **Un milestone = un criterio medible.** Si no puede formularse como "X funciona en Y condición", no es un criterio de cierre.

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
