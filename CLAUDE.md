# Tylluan v0.16.0+ — Claude Code Instructions

> **Last sync: 2026-08-14.** This file is the human-readable context for Claude Code.
> For machine-readable contracts, see `.tylluan/agents.toml` (ADR-009).
> If this file is outdated, every new agent inherits wrong context from the first second.

## 🔴 REGLA FUNDACIONAL (LEER PRIMERO)

**Tylluan es un producto público MIT, autocontenido.** Este workspace (`E:\tylluan`)
no depende de ningún otro repositorio para funcionar, compilar, o pasar sus tests.

**Trabaja exclusivamente dentro de este workspace.**
**Si portas un patrón de otro proyecto, adáptalo e impleméntalo limpio — nunca copies
código ni referencies rutas de otros repositorios locales.**

---

## 🔴 Regla obligatoria — sincroniza ANTES de reportar nada

Antes de escribir un solo hallazgo, PR o mensaje de "esto está roto":

```bash
git fetch origin
git log -1 origin/main --oneline   # commit real de main
git log -1 HEAD --oneline          # tu commit local
```

Si difieren, `git pull` antes de seguir. Un hallazgo sobre código que ya cambió no es un hallazgo — es ruido.

---

## Environment

**Platform:** Windows 11 + Claude Code. Bash disponible solo para operaciones read-only.
Para arrancar procesos: proporcionar el comando al usuario, no ejecutarlo vía Bash.

**Kernel (instalado):** `tylluan-cli start`
**Kernel (desde source):** `cargo run -p tylluan-cli -- start`
**Health check:** `curl http://127.0.0.1:4000/health`
**Dashboard:** `cd dashboard && pnpm dev` → `http://localhost:5173`

---

## Arquitectura Invariante (CONTRACT-01)

1. **5 sovereign tools exactamente:** `tylluan_do`, `tylluan_remember`, `tylluan_recall`, `tylluan_think`, `tylluan_graph`. `all_tools()` en `server.rs` DEBE filtrar a estos 5 y nada más. NUNCA añadir herramientas nuevas aquí.
2. **BGE-M3 a 1024 dimensiones** — `vector_dimensions = 1024`. NUNCA reducir a 768.
3. **Headless-first:** kernel sin UI propia. Dashboard React en `/dashboard`.
4. **Puerto único:** `tylluan-nexus` escucha en `:4000` directamente. **SIN proxy** de zero-downtime — un solo proceso kernel.
5. **MIT soberanía:** sin dependencias cloud en el critical path.
6. **Degree penalty (no boost):** `local_query_graph` usa `pr_score / (1 + deg * 0.1)` — penaliza hubs genéricos. El boost (`*`) fue un bug corregido en v0.10.0.
7. **`[inference] device` gobierna TODO módulo de inferencia local, sin excepción** (José, 2026-08-28): ningún componente decide su propio execution provider por auto-detección — CPU es el modo por defecto seguro aunque se añadan 10.000 opciones más. Origen: `night_reasoner.py` auto-seleccionaba GPU vía `ort.get_available_providers()` ignorando `device="cpu"` (commit `0543d172`, "mejor rendimiento"), y probablemente mató un entrenamiento de Unsloth de 4 días al competir por VRAM en Night Consolidation. Fijado en `router/embeddings.rs::build_execution_providers()` (Rust, ya correcto) y `night_reasoner.py::_inference_device()` (Python, corregido 2026-08-28). Cualquier módulo nuevo que use ONNX/GPU debe leer este mismo campo antes de tocar un `InferenceSession`/`ExecutionProvider` — nunca `get_available_providers()` como decisión, solo como diagnóstico.

---

## Estado actual — v0.16.0+ (unreleased)

**Tests:** 685 kernel lib + 69 link lib + 12 fsrs = 766+ en verde — verificar con `cargo test -p tylluan-kernel --lib` antes de fiarte de cualquier cifra escrita aquí.
**Cargo.toml:** `version = "0.16.0"`, Rust edition 2024, toolchain 1.88.

### Cerrado desde v0.16.0 (2026-08-11 a 2026-08-14):
- **A2A F1-F4 completo** — cliente outbound real, exposición REST + intent con ACL, streaming SSE, hardening (Deep)
- **P2P peer authorization real** — Ed25519↔X25519 verificación (Deep)
- **ACL rediseñado fail-closed** — `AclContext` explícito por transporte (Codex)
- **ASI06 cerrado** — gate de coherencia para `tylluan_remember` (2 capas: síncrono + asíncrono)
- **FrictionStore** — path inyectable con migración columna-por-columna (Mimo)
- **split de `api_v1.rs`** — 3114 líneas → 3 archivos (Codex)
- **Dashboard identidad visual** — paleta propia, tipografía self-hosted, WCAG AA 10/10 (Antigravity)
- **3 bugs reales encontrados y cerrados end-to-end** incluyendo el cuelgue de Qwen Desktop en modo SSE

### Cerrado en v0.16.0 (2026-08-11):
- **M39** — MCP 2026-07-28 adoption: stateless core verificado, Tasks con guards reales, MCP Apps con manifiestos reales
- **M40** — Continuity/trust/action layer: 8 fases completadas, contratos autodocumentados, bootstrap unificado, plan→act→verify→undo completo
- **CoherenceGate→dataset** — fases 1+2: ejemplos estructurados A/B con ground truth real vía Signal Loop

### Cerrado en v0.15.0 (2026-07-30):
- Auditoría completa de conexión real — 5 guilds con IPC al puerto equivocado, escrituras a SilvaDB sin embedding, paneles con datos falsos
- Cifrado obligatorio Noise NK para gossip de producción
- CoherenceGate Layer 4 híbrido en modo observación
- 13 guilds v2 activados + test anti-drift

### Milestones completados (resumen)

| Milestone | Descripción | Estado |
|-----------|-------------|--------|
| **M1-M7** | Memoria, embeddings, retrievals, kernel, single binary | ✅ |
| **M10/M11** | Work Contracts + Federación completa | ✅ |
| **Encryption** | SQLCipher AES-256 en reposo | ✅ |
| **M12** | Ed25519 identity · STUN NAT · mDNS LAN | ✅ |
| **M13** | Binary releases (4 targets) · install scripts · `tylluan-cli` | ✅ |
| **M14-A** | DHT Kademlia · 256 K-buckets · mainline bootstrap | ✅ |
| **M14-B** | Gossip push-pull · LRU store · anti-entropy cursors | ✅ |
| **M14-C** | Noise Protocol XK/NK · Ed25519→X25519 · ChaCha20 AEAD | ✅ |
| **M14-D** | Guild Execution Channels — 4 fases completas | ✅ |
| **M14-E** | Mesh test harness — fault injection, partition, recovery | ✅ |
| **M14-F** | P2P TCP dispatch — Noise XK session pool, 3 fases | ✅ |
| **Security CI** | 30+ tests automatizados de seguridad | ✅ |
| **v0.8.0** | Core Memory · Coloquio flywheel · Hybrid Search v2 | ✅ |
| **v0.9.0** | HNSW · LightRAG graph · Batch embeddings | ✅ |
| **v0.10.0** | Retrieval benchmark · Degree bias fix | ✅ |
| **v0.11.0** | M14-D+E completos · Coordinator Synthesis | ✅ |
| **v0.12.0** | Single binary target · installers · Docker | ✅ |
| **v0.13.0** | Junior onboarding · First Minute · Security Hardening | ✅ |
| **v0.14.0** | A2A Interop · Signal Loop + CoherenceGate · Dashboard Substrate | ✅ |
| **v0.15.0** | Connection Audit · Mandatory Mesh Encryption | ✅ |
| **v0.16.0** | MCP 2026-07-28 · Continuity/Trust/Action · CoherenceGate Dataset | ✅ |

---

## Agentes del equipo

| Agente | Runtime | Rol |
|--------|---------|-----|
| **Claude Code (Sonnet 5)** | CLI / IDE | Tech lead — planes, briefings, síntesis, docs, memoria, verificación cruzada |
| **Deep** | OpenCode | Backend Rust + guilds Python — features complejas, cierre de bugs de fondo |
| **Mimo** | OpenCode | Auditorías/refactor de dashboard — tareas acotadas y verificadas |
| **Antigravity** | Gemini + MCP | UI/UX dashboard — tareas YA cerradas y acotadas por tech lead |
| **Qwen Desktop** | App escritorio | Research web + deep research — vía SSE MCP |

**Reglas de asignación:**
- Rust / crates/ → Deep (briefing previo con DoD y zonas excluidas)
- Dashboard / UI → Mimo o Antigravity, en pasos pequeños
- Research web → Qwen Desktop
- Orquestación / docs / arbitraje → Claude Code

---

## Archivos clave

| Archivo | Propósito |
|---------|-----------|
| `crates/tylluan-kernel/src/transport/server/` | Sovereign tools + `all_tools()` |
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

## North Star — Invariante Fundacional

**Invariante de portabilidad:** Un único binario arranca offline en hardware modesto (RPi4, CPU sin GPU) y también en un servidor. Sin dependencias de red en el path crítico. El conocimiento persiste en local, la sync con peers es oportunista — no requerida.

**Filtro de decisión para cualquier feature:** ¿Puede el mismo componente servir a un usuario modesto (5-10 peers, CPU sin internet) Y a uno con servidor (100+ peers, datacenter) **sin bifurcar el código** — solo diferente `tylluan.toml`?

**Invariantes derivados:**
- **Toaster-friendly:** Raspberry Pi 4 (4GB RAM) y hardware de 10 años
- **USB-portable:** bundle completo cabe y arranca desde un USB
- **Offline-first:** kernel arranca y opera sin internet
- **Sin bifurcación de código:** un solo binario, configuración distinta por entorno

---

## Reglas críticas

- NUNCA `vector_dimensions = 768` — rompe todos los embeddings
- NUNCA `host = "0.0.0.0"` + `dev_mode = true` juntos (LAN RCE)
- NUNCA tokens en archivos trackeados — solo en `.tylluan-token` (gitignored)
- NUNCA iniciar procesos vía Bash (AV bloquea spawning en Windows)
- NUNCA reducir timeouts para guilds de inferencia (BGE-M3 en CPU tarda 2-8s/embedding)
- NUNCA cambiar el degree bias de vuelta a multiplicación — el `/ (1 + deg * 0.1)` es correcto
- Dashboard: `pnpm`, nunca `npm` (dos lockfiles divergentes rompieron dependencias en producción)

---

## Validación estándar

**No copies comandos sueltos de memoria ni de este archivo — corre `scripts/verify.sh`.**

Añadido 2026-08-26 tras un día en el que tres agentes distintos (incluido Claude)
afirmaron "clippy limpio" o "tests en verde" y estaban equivocados, siempre por
la misma causa: usaron un comando más estrecho o un toolchain distinto al que
CI realmente ejecuta (`cargo clippy` sin `--all-targets`, toolchain local en vez
de `stable`, `pnpm lint` sin `--frozen-lockfile`). El bloque de comandos que
antes vivía aquí sufría exactamente esa deriva — decía "685+" cuando ya eran
709+, y no incluía `--all-targets` en absoluto.

```bash
bash scripts/verify.sh              # todo — kernel, tylluan-link, dashboard, docs
bash scripts/verify.sh --rust       # solo Rust (kernel + tylluan-link)
bash scripts/verify.sh --dashboard  # solo dashboard
bash scripts/verify.sh --docs       # solo drift de STATUS.md/README.md (segundos)
```

Instala también el hook de pre-push, una vez por checkout, para que esto se
ejecute solo antes de cada `git push` (usa `git push --no-verify` para saltarlo
deliberadamente, solo si sabes por qué):

```bash
bash scripts/install_hooks.sh
```

El script fija el toolchain a `stable` explícitamente (`rustup run stable`) —
igual que `.github/workflows/ci.yml` — porque el toolchain local por defecto de
esta máquina puede ir por delante o detrás del que usa CI, y esa diferencia ya
ha producido falsos positivos reales más de una vez.

---

## Crate structure

| Crate | Purpose |
|-------|---------|
| `tylluan-kernel` | Core binary (`tylluan-nexus`) + library: MCP, memory, federation, security |
| `tylluan-common` | Shared types, error types, constants |
| `tylluan-link` | Federation networking: mesh identity, DHT, NAT, mDNS, Noise Protocol |
| `tylluan-cli` | `start / stop / status / logs / connect / download-models / install` |
| `tylluan-evals` | Benchmark harness: Recall@N, Precision@N, latency percentiles |
| `tylluan-fsrs` | Spaced repetition scheduling for memory decay |
| `tylluan-gui` | Tauri-based desktop GUI (experimental) |

---

## CI Pipeline

| Job | Status |
|-----|--------|
| Rust — build + test | ✅ |
| Rust — cargo-deny (licenses + advisories + bans) | ✅ |
| Rust — clippy -D warnings | ✅ |
| Python — ruff lint + pytest | ✅ |
| Dashboard — pnpm lint + build | ✅ |
| Rust — security audit tests (9 test suites) | ✅ |
| Rust — ARM64 portability (aarch64-unknown-linux-gnu) | ✅ |
| Reproducible build check | ✅ |
| README test count validation | ✅ |

---

*This file reflects the project's actual state, not aspirational goals. Keep it synchronized on every milestone close.*
