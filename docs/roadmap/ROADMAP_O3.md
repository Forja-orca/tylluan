# Tylluan — Roadmap Estratégico

> **Última actualización:** 2026-07-05 · v0.12.1 (HEAD `f8bad9f`)
> **Fuente de verdad:** STATUS.md · Decisiones en ADRs bajo `docs/architecture/`
> **Norte permanente:** Rufus test — funciona en frío, sin docs, sin Rust, en < 5 min.

---

## Estado actual — v0.12.1 ✅

M16 BGE-M3 Benchmark cerrado. R@5 82% en LongMemEval-S (50 queries reales), ADR-007 IdleLab INNECESARIO (defaults son óptimo local). M17 Rama A abierta: OpenClaw + Hermes docs publicados.

Lo que ya tenemos:
- Binario único, 4 targets (x86_64/aarch64 × Linux/Windows/macOS)
- 5 sovereign tools MCP (tylluan_do/recall/remember/think/graph)
- BGE-M3 hybrid search: R@5 82% LongMemEval-S, R@10 90%, latency p50 12.9ms
- Federation P2P completa: DHT Kademlia + Gossip + Noise XK + TCP dispatch
- Security: 30 automated tests, rate limiter, circuit breaker, guard
- `tylluan-cli start/stop/status/logs/connect/download-models`
- CI: build + test + deny + security audit + Python lint + ARM64 portability + Docker smoke
- Docs integración: `docs/integrations/openclaw.md` + `docs/integrations/hermes-agent.md`

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

### M17 — Integración Externa (v0.13.0) ← ACTIVO

**Norte:** Tylluan como sovereign memory backend para el ecosistema externo (OpenClaw, Hermes, Claude Desktop). La condición es el resultado de P3 (M15).

**Rama A — Si OpenClaw confirmado (stars reales + spike OK < 1 semana de trabajo):**

| Fase | Descripción | Agente |
|------|-------------|--------|
| P0 | `docs/integrations/openclaw.md` — guía completa: "Tylluan como memoria soberana para OpenClaw" | Claude |
| P1 | MCP config verificado: OpenClaw → Tylluan vía SSE. Test E2E en coloquio (Antigravity ejecuta). | Antigravity |
| P2 | Entrada en `integrations/` + validación en CI (mock MCP client) | Deep |

**Rama B — Si OpenClaw no confirmado (o integración > 1 semana):**

| Fase | Descripción | Agente |
|------|-------------|--------|
| P0 | Permisos granulares: `[permissions]` en `tylluan.toml` — deny/ask/allow por guild. ACL legible por humanos. | Deep |
| P1 | AGENTS.md como config declarativa de usuario (agentes definen su perfil en el repo, Tylluan lo carga). Spec en ADR-007. | Claude + Deep |
| P2 | UI en dashboard: panel de permisos por guild con toggle deny/ask/allow | Antigravity |

**Criterio de cierre común:** Un usuario externo puede configurar Tylluan para su entorno en < 10 minutos.

---

### M18 — TRINITY Coordinator Guild (v0.14.0)

**Norte:** Mejorar la calidad en tareas multi-paso. Un guild `coordinator` orquesta Thinker/Worker/Verifier para descomponer tareas complejas, asignar a sub-guilds, y verificar resultados. Basado en paper ICLR 2026: "TRINITY: An Evolved LLM Coordinator" (arXiv:2512.04695).

**Por qué ahora:** Una vez que la install experience está resuelta (M15), el siguiente diferenciador competitivo es la calidad de razonamiento. El Rufus test es sobre confiabilidad; TRINITY es sobre inteligencia.

**Fases:**

| Fase | Descripción | Agente |
|------|-------------|--------|
| P0 | Spec del coordinator guild: cómo recibe intent de `tylluan_do`, lo descompone en sub-tareas, asigna a Thinker/Worker/Verifier guilds. ADR-008. | Claude |
| P1 | Implementación: `guilds/core/coordinator.py` (FastMCP). El coordinator llama a otros guilds internamente vía `registry.call_tool()`. | Deep |
| P2 | Benchmark comparativo: 10 queries complejas multi-paso con/sin coordinator. Hipótesis: mejora calidad ≥ 30%. | Claude + Qwen |
| P3 | Si benchmark pasa: merge a main. Si no: documentar y revisar spec. Sin presión de timeline. | Todo el equipo |

**Criterio de cierre:** Benchmark comparativo con delta positivo publicado en `docs/research/`.

**Nota de disciplina:** Este milestone solo abre si M15 y M16 están cerrados. No se añade inteligencia antes de resolver la instalación.

---

### M19 — DX 10/10 · Fugu Parity (v0.15.0)

**Norte:** Experiencia de developer comparable a Sakana Fugu — el estándar más alto de DX que conocemos. `tylluan` como comando único. Auto-update. Profile wizard. AGENTS.md como estándar.

**Por qué importante:** Qwen analizó Fugu (sesión 2026-06-23) y concluyó que "ganó en integración, no solo en motor." Nosotros tenemos mejor motor. Tenemos que ganar también en integración.

**Fases:**

| Fase | Descripción | Agente |
|------|-------------|--------|
| P0 | `tylluan` como alias único: `tylluan start / stop / status / update / backup / restore`. Sin `tylluan-cli`. Compatible con M13 installs existentes. | Deep |
| P1 | `tylluan update` — comprueba release en GitHub, descarga binario si hay nueva versión, reinicia limpio. | Deep |
| P2 | Profile wizard interactivo en first-run: `tylluan start --setup` pregunta 3 preguntas (GPU?, perfil de uso, nombre del agente) y genera `tylluan.toml`. Sin editar TOML a mano. | Deep + Claude |
| P3 | AGENTS.md como contrato declarativo estándar: cada agente o herramienta que usa Tylluan define su perfil y permisos en AGENTS.md. Kernel lo lee al arrancar. | Claude (spec) + Deep (kernel) |

**Criterio de cierre:** Instalar, configurar y hacer la primera consulta MCP en < 3 minutos en una máquina virgen, sin leer ningún documento.

**Diferencia con M15:** M15 es "que arranque". M19 es "que sea un placer usarlo."

---

## Hoja de ruta visual

```
v0.11.0 ──────────────────────────────────────────────────────── ACTUAL
   │
   ▼
M15 ─── Rufus Release ──────────────────────────────────── v0.12.0
   │    install.sh · first-run UX · Docker · OpenClaw verify
   │
   ▼
M16 ─── BGE-M3 Benchmark Real ──────────────────────────── v0.12.1
   │    retrieval quality verificada · Idle Lab validado
   │
   ▼
M17 ─── Integración Externa ────────────────────────────── v0.13.0
   │    OpenClaw (si confirmado) · o Permisos Granulares
   │
   ▼
M18 ─── TRINITY Coordinator Guild ─────────────────────── v0.14.0
   │    calidad multi-paso · benchmark antes/después
   │
   ▼
M19 ─── DX 10/10 · Fugu Parity ─────────────────────────── v0.15.0
        `tylluan` comando único · auto-update · profile wizard
```

---

## Reglas de Disciplina (permanentes)

1. **Rufus test primero.** Ningún feature nuevo hasta que M15 esté cerrado.
2. **Datos antes que intuición.** Cada milestone de calidad (M16, M18) requiere benchmark antes/después.
3. **No añadir al kernel sin necesidad.** CONTRACT-01 (5 sovereign tools) no se toca.
4. **Verificar antes de decidir.** OpenClaw, NemoClaw, cualquier integración — primero fuente primaria, luego spike, luego milestone.
5. **Un milestone = un criterio medible.** Si no puede formularse como "X funciona en Y condición", no es un criterio de cierre.

---

## Investigación pendiente (backlog, sin fecha)

| # | Hipótesis | Paper/fuente | Estado |
|---|-----------|-------------|--------|
| I-1 | Dynamic Agent Pool: GuildMatcher aprende selección de modelos vía RL | Conductor paper (arXiv:2512.04388) | Pendiente M19 |
| I-2 | Topologías dinámicas de guilds: grafos de comunicación entre guilds | Conductor paper | Post-M19 |
| I-3 | Mesh global (NAT traversal público, DHT cross-instance) | ADR pendiente | Post-v1.0 |
| I-4 | Permisos asimétricos (criptografía Ed25519 para ACL distribuida) | Diseño interno | Post-v1.0 |
