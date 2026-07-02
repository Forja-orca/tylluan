# Tylluan v0.10.0 — Agent Instructions (Codex / OpenCode)

## Regla fundacional

**Tylluan es el producto público construido sobre ForjaMCPo3.**

```
ForjaMCPo3 (E:\ForjaMCPo3)  ←  framework cognitivo interno privado del equipo
        ↓  patrones probados se portan, nunca se mezcla código
Tylluan (E:\tylluan)          ←  producto público MIT, este workspace
```

- **NUNCA tocar `E:\ForjaMCPo3`** desde este workspace.
- **NUNCA copiar código de Forja directamente** — adaptar e implementar limpio.

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
**Health check:** `curl http://127.0.0.1:3030/health`  
**Dashboard dev:** `cd dashboard && pnpm dev` → `http://localhost:5173`

---

## Estado actual — v0.11.0 (tag, HEAD)

**Tests:** 273 kernel lib + 88 link + 2 evals = **363 total** · 0 fallos  
**HEAD commit:** `c51357a` (main) · **tag: `v0.11.0`**

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


---

## Arquitectura invariante (CONTRACT-01)

1. **5 sovereign tools exactamente:** `tylluan_do`, `tylluan_remember`, `tylluan_recall`, `tylluan_think`, `tylluan_graph`. `all_tools()` en `server.rs` DEBE filtrar a estos 5 y nada más. NUNCA añadir herramientas nuevas aquí.
2. **BGE-M3 a 1024 dimensiones** — `vector_dimensions = 1024`. NUNCA reducir a 768.
3. **Headless-first:** kernel sin UI propia. Dashboard React en `/dashboard`.
4. **Puerto único:** `tylluan-nexus` escucha en `:3030` directamente. **SIN proxy** (Tylluan no tiene proxy, a diferencia de ForjaMCPo3).
5. **AGPL soberanía:** sin dependencias cloud en el critical path.
6. **Degree penalty (no boost):** `local_query_graph` usa `pr_score / (1 + deg * 0.1)` — penaliza hubs genéricos. El boost (`*`) fue un bug corregido en v0.10.0.

---

## Archivos clave

| Archivo | Propósito |
|---------|-----------|
| `crates/tylluan-kernel/src/transport/server/` | Handlers MCP sovereign tools |
| `crates/tylluan-kernel/src/memory/silva/graph.rs` | `degree_centrality`, `local_query_graph` (PPR + degree penalty) |
| `crates/tylluan-kernel/src/memory/silva/search.rs` | `search_hybrid` — RRF + type_filter + skip_graph |
| `crates/tylluan-kernel/src/memory/silva/embeddings.rs` | `embed_batch` — ONNX single mutex, L2-norm |
| `crates/tylluan-link/src/capability.rs` | `CapabilityRegistry` — M14-D Phase 1 |
| `crates/tylluan-link/src/transport.rs` | `PartitionableTransport<T>` — 5 fault modes |
| `crates/tylluan-link/src/gossip/message.rs` | `GossipEntry` + `HardwareCaps` |
| `crates/tylluan-evals/src/tests.rs` | Retrieval benchmark (skip_graph A/B) |
| `docs/architecture/M14D_dispatch_spec.md` | ADR-004 — spec completa M14-D |
| `tylluan.toml` | Config runtime — `dev_mode`, `host`, `port`, `[silva]`, `[federation]` |
| `.tylluan-token` | Bearer token (untracked) |
| `benchmarks/benchmark_v0.10.0.json` | Retrieval quality delta (Graph ON vs OFF) |

---

## Validación estándar

```bash
cargo check -p tylluan-kernel
cargo test -p tylluan-kernel --lib 2>&1 | tail -3
# Esperado: 273 lib tests passing

cargo test -p tylluan-link --all-targets 2>&1 | Select-String "test result"
# Esperado: 88 link tests passing (distribuidos en 7 archivos)

cargo test -p tylluan-evals 2>&1 | tail -3
# Esperado: 2 evals tests passing
```

---

## Reglas críticas

- NUNCA `vector_dimensions = 768` — rompe todos los embeddings
- NUNCA `host = "0.0.0.0"` + `dev_mode = true` juntos (LAN RCE)
- NUNCA tokens en archivos trackeados — solo en `.tylluan-token` (gitignored)
- NUNCA iniciar procesos vía Bash (AV bloquea spawning en Windows)
- NUNCA tocar `E:\ForjaMCPo3` — workspaces separados
- NUNCA reducir timeouts para guilds de inferencia (BGE-M3 en CPU tarda 2-8s/embedding)
- NUNCA cambiar el degree bias de vuelta a multiplicación — el `/ (1 + deg * 0.1)` es correcto

---

## Flota de agentes

| Agente | Runtime | Rol |
|--------|---------|-----|
| **Claude Code (Sonnet 4.6)** | CLI / IDE | Tech lead — planes, briefings, síntesis, docs, memoria |
| **Deep (DeepSeek V4 Flash)** | OpenCode IDE #1 | Implementación Rust — features complejas, razonamiento largo |
| **DeepSeekPadawan (DeepSeek V4 Flash)** | OpenCode IDE #2 | Implementación Rust — segundo carril paralelo, tareas acotadas |
| **Antigravity** | Browser + MCP | UI/UX/GUI — dashboard React, visualizaciones (inferencia limitada, reservar) |
| **Qwen Desktop** | App escritorio | Investigación web + deep research — papers, repos, benchmarks; vía SSE MCP |

**Reglas de asignación:**
- Rust / crates/ → Deep o DeepSeekPadawan (briefing previo con DoD y zonas excluidas)
- Research web / papers / repos → Qwen Desktop
- Dashboard / UI / visualizaciones → Antigravity (solo si hay budget disponible)
- Orquestación / docs / arbitraje → Claude Code
