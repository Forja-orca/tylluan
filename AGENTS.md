# Tylluan v0.16.0+ (unreleased) — Agent Instructions (Codex / OpenCode)

> Este archivo es leído automáticamente por agentes OpenCode/Codex al conectar (Deep,
> Mimo, cualquier futuro agente en ese runtime). Si está desactualizado, TODO agente
> de esa clase arranca con información falsa desde el primer segundo — no es un
> detalle cosmético. Confirmado 2026-07-28: este archivo llevaba meses con versión,
> milestones y flota incorrectos mientras el equipo lo usaba a diario.

## Regla fundacional

**Tylluan es un producto público MIT.** Este workspace (`E:\tylluan`) es autocontenido:
no depende de ningún otro repositorio ni framework interno para funcionar, compilar,
o pasar sus tests.

- Trabaja exclusivamente dentro de este workspace.
- Si necesitas portar un patrón de otro proyecto, adáptalo e impleméntalo limpio aquí
  — nunca copies código ni referencies rutas de otros repositorios locales.

---

## Regla obligatoria — sincroniza ANTES de reportar nada (2026-07-30)

**Causa raíz de un incidente real:** un agente nuevo auditó la conexión del proyecto y reportó
4 hallazgos como si estuvieran abiertos — los 4 ya estaban cerrados en `main`, en commits de
minutos/horas antes. Su checkout local nunca se sincronizó contra `origin/main` al empezar la
sesión, así que trabajó (y reportó) sobre un estado del repo que ya no existía. No es un caso
aislado: este mismo `AGENTS.md` ya había estado meses desactualizado por la misma razón de fondo
(nadie fuerza una sincronización al arrancar).

**Regla, sin excepción, antes de escribir un solo hallazgo, PR o mensaje de "esto está roto":**

```bash
git fetch origin
git log -1 origin/main --oneline   # commit real de main
git log -1 HEAD --oneline          # tu commit local
```

Si difieren, `git pull` (o el equivalente de tu runtime) **antes** de seguir. Un hallazgo sobre
código que ya cambió no es un hallazgo — es ruido que hace perder tiempo real al resto del equipo
verificándolo. Si tu runtime no te deja sincronizar automáticamente, dilo explícitamente en tu
primer mensaje ("mi checkout puede estar desactualizado, no lo he podido sincronizar") en vez de
reportar con la confianza de quien sí lo hizo.

---

## Environment

**Platform:** Windows 11. Bash disponible solo para operaciones read-only (git, cargo check/test).  
Para arrancar procesos: proporcionar el comando al usuario, no ejecutarlo vía Bash.

**Arrancar kernel:**
```bash
tylluan-cli start
# o desde source:
cargo run -p tylluan-cli -- start
```
**Health check:** `curl http://127.0.0.1:4000/health`  
**Dashboard dev:** `cd dashboard && pnpm dev` → `http://localhost:5173`

---

## Estado actual — v0.16.0+ (unreleased, 34 commits desde el release)

**Tests:** 685 lib tests (kernel) + 69 (tylluan-link) + 12 (tylluan-fsrs) = 766 en verde — verificar con `cargo test -p tylluan-kernel --lib` antes de fiarte de cualquier cifra escrita aquí, el número real cambia cada ciclo.
**HEAD commit real:** consultar `git log --oneline -1`, o `curl http://127.0.0.1:4000/health` para el commit que el kernel EN EJECUCIÓN tiene cargado (puede ir por detrás de main si nadie ha reconstruido tras el último cambio en `.rs`).

### Cerrado desde v0.16.0 (2026-08-11 a 2026-08-14, sin tag de versión todavía): dos rondas de auditoría externa cerradas el mismo día cada una — RCE crítico de P2P (`4674f84`) con verificación real de peer añadida después (`ebbc998`, Ed25519↔X25519), ACL rediseñado fail-closed (`09b9668`), ASI06 cerrado con gate de escritura de 2 capas para `tylluan_remember` (`2bd0416`). `FrictionStore` con path inyectable, split de `api_v1.rs` (3114→3 archivos). A2A F1-F4 completo: cliente outbound real verificado contra el SDK oficial, exposición REST/intent con ACL, streaming SSE, hardening (`3f4ce1e`). Dashboard: identidad visual soberana en 4 fases verificadas (paleta con nombre propio, tipografía self-hosted, WCAG AA real, piloto de foco de teclado). Ver `CHANGELOG.md` sección `[Unreleased]` para el detalle completo.

### Cerrado en v0.16.0 (2026-08-11): M39 (adopción MCP 2026-07-28: stateless core verificado en vivo, Tasks con guards reales, MCP Apps con manifiestos reales) y M40 (Tylluan como capa de continuidad/confianza/acción, 8 fases) ambos completos — condición explícita de José para el release. 3 bugs reales encontrados en vivo y cerrados end-to-end, incluido el cuelgue histórico de Qwen Desktop en modo SSE (causa raíz: `sse_handler` descartaba los headers reales del cliente). Circuito CoherenceGate→dataset fase 1+2: ejemplos estructurados A/B con ground truth real vía el Signal Loop de ADR-011, nada entrenado todavía.

### Cerrado en v0.15.0 (2026-07-30): auditoría completa de conexión real (Deep/Mimo/Claude) — 5 guilds con IPC al puerto equivocado, escrituras a SilvaDB sin embedding, paneles de dashboard con datos falsos; cifrado obligatorio Noise NK para gossip de producción (antes en texto plano); CoherenceGate Layer 4 híbrido wireado en vivo, modo observación; `[guilds.v2]` activó 13 guilds más + test estructural anti-drift; crash de visión por TDR de GPU en Windows, causa raíz confirmada y arreglada.

### Milestones completados

| Milestone | Descripción | Estado |
|-----------|-------------|--------|
| **M1** | Memory decay — half-life T½=14d, type-specific rates | ✅ |
| **M2** | Hybrid Search v2 — BM25 + FTS5 + BGE-M3 + RRF | ✅ |
| **M3** | Guild auto-discovery — scan `guilds/` at startup | ✅ |
| **M7** | Single binary — `--features bundled-dashboard` | ✅ |
| **M10** | Bounded Work Contracts — finite multi-agent protocol | ✅ |
| **M11** | Federation — SQLite peers · push/pull/auto-sync · ChaCha20 | ✅ |
| **M12** | Mesh identity — Ed25519 · STUN NAT · mDNS LAN | ✅ |
| **M13** | Binary releases (4 targets) · install scripts · `tylluan-cli` | ✅ |
| **Security CI** | 30 automated security tests | ✅ |
| **Encryption** | SQLCipher AES-256 at rest (`--features encryption`) | ✅ |
| **v0.6–v0.9** | Core Memory · HNSW · LinearRAG · Episodic search · Batch embeddings | ✅ |
| **M14-A** | DHT Kademlia · 256 K-buckets · mainline BitTorrent bootstrap | ✅ |
| **M14-B** | Gossip push-pull · LRU store · anti-entropy cursors · HardwareCaps | ✅ |
| **M14-C** | Noise XK/NK · Ed25519→X25519 · wired to federation sync | ✅ |
| **M6-full** | `PartitionableTransport<T>` (5 modes) + `fault_dst.rs` (4 DST scenarios) | ✅ |
| **v0.10.0** | Retrieval benchmark · degree-bias fix (penalty not boost) · ADR-004 M14-D | ✅ |

### Completado en v0.11.0

| Milestone | Descripción | Estado |
|-----------|-------------|--------|
| **M14-D Phase 1** | `CapabilityRegistry` + `HardwareCaps` in `GossipEntry` | ✅ |
| **M14-D Phase 2** | `DispatchRouter` — scoring, circuit breaker, `prune_expired` kernel wiring | ✅ |
| **M14-D Phase 3** | `GuildDispatchRequest/Response` + Noise NK + `/api/v1/guilds/dispatch/execute` | ✅ |
| **M14-D Phase 4** | `DispatchQueue` + `/guilds/dispatch/remote` + `/guilds/peers` + circuit breaker | ✅ |
| **M14-E Phase 1** | `mesh_simulation.rs` — full-mesh, star topology, split-brain + heal | ✅ |
| **M14-E Phase 2+3** | `dispatch_dst.rs` — multi-peer routing + `DispatchQueue` moved to link | ✅ |
| **CI/deps cleanup** | `deny.toml` green · `Cargo.toml` 0.11.0 · README/docs consistency | ✅ |
| **ADR-005 M14-F** | P2P TCP dispatch spec — Noise XK session pool, Option A transparent routing, 6-phase plan | ✅ |
| **Moondream guild** | `guilds/core/vision_moondream.py` — `analyze_image` + `caption_image` via moondream pip | ✅ |
| **M14-F Phase 1** | `p2p.rs` — `P2pSessionPool` + `execute_remote_tcp()` + `HardwareCaps.supports_p2p/tcp_port` | ✅ |
| **M14-F Phase 2** | `start_p2p_listener_noise()` + `DispatchDecision::RemoteTcp` + score-first routing + `p2p_dst.rs` (3 tests) | ✅ |
| **M14-F Phase 3** | `P2pHandlerFn` (BoxFuture) · `P2pConfig` `[p2p]` in `config.rs` · `p2p_pool` in `HttpState` · P2P listener spawn (conditional) · `api_mesh.rs` native `RemoteTcp` via `execute_remote_tcp` · `guild_peers_handler` exposes `supports_p2p/tcp_port` | ✅ |
| **Portability CI** | `portability-check` job — `cargo check` para `aarch64-unknown-linux-gnu` en cada push · ARM64 (RPi4) portability garantizada | ✅ |

### Completado en v0.12.0

| Milestone | Descripción | Estado |
|-----------|-------------|--------|
| **M15-P0** | `install.sh` / `install.ps1` — descarga binario, arranca `--profile portable`, health check 30s, imprime config MCP | ✅ `2df8f73` |
| **M15-P1** | `GET /api/v1/setup-hint` — JSON con configs Claude Desktop / Code / Cursor. BM25 como default | ✅ `2df8f73` |
| **M15-P2** | Docker imagen `ghcr.io/forja-orca/tylluan:latest` — `debian:bookworm-slim` + ONNX 1.22.0 + bundled-dashboard + `always_on=[]` + docker-smoke CI auth | ✅ `a2642da`→`945838c` |
| **M15-P3** | OpenClaw 368k stars verificados · Hermes Agent compatible · M17 Rama A decidida | ✅ `5c9b32d` |
| **ADR-006** | Spec Rufus Release — `docs/reference/adr/ADR006_rufus_release.md` | ✅ |
| **Roadmap** | M15-M19 planificados — `docs/roadmap/ROADMAP_O3.md` | ✅ |


---

## Arquitectura invariante (CONTRACT-01)

1. **5 sovereign tools exactamente:** `tylluan_do`, `tylluan_remember`, `tylluan_recall`, `tylluan_think`, `tylluan_graph`. `all_tools()` en `server.rs` DEBE filtrar a estos 5 y nada más. NUNCA añadir herramientas nuevas aquí.
2. **BGE-M3 a 1024 dimensiones** — `vector_dimensions = 1024`. NUNCA reducir a 768.
3. **Headless-first:** kernel sin UI propia. Dashboard React en `/dashboard`.
4. **Puerto único:** `tylluan-nexus` escucha en `:4000` directamente. **SIN proxy** de zero-downtime — un solo proceso kernel.
5. **MIT soberanía:** sin dependencias cloud en el critical path.
6. **Degree penalty (no boost):** `local_query_graph` usa `pr_score / (1 + deg * 0.1)` — penaliza hubs genéricos. El boost (`*`) fue un bug corregido en v0.10.0.

---

## Archivos clave

| Archivo | Propósito |
|---------|-----------|
| `crates/tylluan-kernel/src/transport/server/` | Handlers MCP sovereign tools |
| `crates/tylluan-kernel/src/memory/silva/graph.rs` | `degree_centrality`, `local_query_graph` (PPR + degree penalty) |
| `crates/tylluan-kernel/src/memory/silva/search.rs` | `search_hybrid` — RRF + type_filter + skip_graph |
| `crates/tylluan-kernel/src/router/embeddings.rs` | `embed_batch` — ONNX single mutex, L2-norm |
| `crates/tylluan-link/src/capability.rs` | `CapabilityRegistry` — M14-D Phase 1 |
| `crates/tylluan-link/src/transport.rs` | `PartitionableTransport<T>` — 5 fault modes |
| `crates/tylluan-link/src/gossip/message.rs` | `GossipEntry` + `HardwareCaps` |
| `crates/tylluan-evals/src/tests.rs` | Retrieval benchmark (skip_graph A/B) |
| `docs/reference/adr/M14D_dispatch_spec.md` | ADR-004 — spec completa M14-D |
| `tylluan.toml` | Config runtime — `dev_mode`, `host`, `port`, `[silva]`, `[federation]` |
| `.tylluan-token` | Bearer token (untracked) |
| `benchmarks/benchmark_v0.10.0.json` | Retrieval quality delta (Graph ON vs OFF) |

---

## Validación estándar

```bash
cargo check -p tylluan-kernel
cargo test -p tylluan-kernel --lib 2>&1 | tail -3
# Esperado: 685+ lib tests passing

cargo test -p tylluan-link --all-targets 2>&1 | Select-String "test result"
# Esperado: 69+ link tests passing

cargo test -p tylluan-evals 2>&1 | tail -3
# Esperado: 2 evals tests passing
```

---

## Reglas críticas

- NUNCA `vector_dimensions = 768` — rompe todos los embeddings
- NUNCA `host = "0.0.0.0"` + `dev_mode = true` juntos (LAN RCE)
- NUNCA tokens en archivos trackeados — solo en `.tylluan-token` (gitignored)
- NUNCA iniciar procesos vía Bash (AV bloquea spawning en Windows)
- NUNCA reducir timeouts para guilds de inferencia (BGE-M3 en CPU tarda 2-8s/embedding)
- NUNCA cambiar el degree bias de vuelta a multiplicación — el `/ (1 + deg * 0.1)` es correcto

---

## Flota de agentes

| Agente | Runtime | Rol |
|--------|---------|-----|
| **Claude Code (Sonnet 5)** | CLI / IDE | Tech lead — planes, briefings, síntesis, docs, memoria, verificación cruzada |
| **Deep** | OpenCode | Backend Rust + guilds Python — features complejas, cierre de bugs de fondo |
| **Mimo** | OpenCode | Auditorías/refactor de dashboard — tareas acotadas y verificadas, no diseño abierto de una vez |
| **Antigravity** | Gemini + MCP | UI/UX dashboard — tareas YA cerradas y acotadas por el tech lead (historial de datos simulados presentados como reales, verificar siempre antes de aceptar) |
| **Qwen Desktop** | App escritorio | Investigación web + deep research — vía SSE MCP, sin acceso a disco |

**Reglas de asignación:**
- Rust / crates/ → Deep (briefing previo con DoD y zonas excluidas)
- Dashboard / UI → Mimo o Antigravity, en pasos pequeños verificados uno a uno, nunca un rediseño completo de golpe
- Research web / papers / repos → Qwen Desktop
- Orquestación / docs / arbitraje → Claude Code

**Disciplina no negociable para cualquier agente de esta flota (aprendida a base de incidentes reales, 2026-07-28):**
1. Nunca afirmar "cerrado"/"funciona" sin verificar tú mismo (leer el código, correr el build/test real, probar en vivo). `cargo check`/`cargo test` NO corren `clippy -D warnings` — CI sí; corre `cargo clippy -p tylluan-kernel -- -D warnings` tú mismo antes de dar un cambio Rust por listo.
2. Un guild Python nuevo necesita registrarse en 3 sitios (`main.rs` lista `lazy_guilds`, `router/catalog.rs` weight+descripción+lista de nombres) o falla con "Unknown guild" en runtime — `discover_guilds()` no escanea subcarpetas, no basta con crear el `.py`.
3. Dashboard: `pnpm`, nunca `npm` (dos lockfiles divergentes rompieron dependencias en producción, 2026-07-28).
4. Si dos agentes tocan piezas complementarias (un endpoint + quien lo consume), acordar el esquema EXACTO (nombres de tabla/campo, puertos) en Coloquio antes de escribir código — pasó 2 veces el mismo día no hacerlo y hubo que arreglar el desajuste después.
5. Nunca dejar trabajo sin commitear acumulándose — commits pequeños y verificados, no todo junto al final.
6. Nunca atribuir una decisión a José que no diera — si actúas por iniciativa propia, dilo así.

**Perfiles declarativos (M19-P5):** ver [ADR-009](docs/reference/adr/ADR009_agents_declarative_contract.md) — `.tylluan/agents.toml` es el contrato máquina-legible que el kernel carga al arrancar (agent_id → rol ACL). Este archivo (AGENTS.md) sigue siendo la documentación humana; no se parsea.

---

## Protocolo de actualización de documentación post-milestone

**Regla:** ningún milestone se da por cerrado hasta que estos archivos reflejen el estado real. La doc que miente genera trabajo fantasma para el siguiente agente.

Al cerrar cualquier milestone, el agente que lo cierra ejecuta este checklist antes de anunciar "cerrado" en Coloquio:

1. **`STATUS.md`** — actualizar versión, tabla de tests, "What works" con lo que el milestone entregó. Fuente de verdad técnica: lo obsoleto se borra, no se acumula.
2. **`ROADMAP.md`** — mover el milestone a cerrado con fecha y commit hash.
3. **`docs/internal/PROJECT.map` / `OPERATIONS.map`** — solo si el milestone cambió arquitectura o invariantes.
4. **`SPEC.md`** — solo si cambió alcance, audiencias, o cerró un ítem de la tabla "documentación que falta".
5. **`CHANGELOG.md`** — entrada nueva por milestone/release, no acumular cambios sin registrar.
6. **`AGENTS.md`** (este archivo) — si cambió la flota de agentes o sus reglas de asignación.

**Quién verifica:** el Tech Lead (Claude Code) confirma que estos puntos están al día antes del visto bueno final — contrastado contra el archivo real, no solo el reporte del agente que cerró el milestone.

**Por qué existe esta regla:** en la sesión del 2026-07-10 se encontró `STATUS.md` con 2 milestones de retraso (M22 y M23-P1 cerrados sin documentar). Este protocolo existe para que no se repita.

**Escalada (2026-07-28):** este mismo archivo (`AGENTS.md`) llevaba meses reportando v0.10.0 mientras el repo estaba en v0.13-0.14, con la flota de agentes desactualizada. Como es el archivo que OpenCode/Codex carga automáticamente al conectar cualquier agente, la desactualización no es solo ruido documental — es contexto falso heredado por cada agente nuevo desde el primer segundo, coincidiendo con el hallazgo de investigación del mismo día sobre degradación de seguridad en el arranque en frío de un agente (arXiv:2606.07867). Revisar este archivo específicamente en cada cierre de milestone, no solo `STATUS.md`.
