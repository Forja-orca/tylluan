# Tylluan v0.5.0 — Claude Code Instructions

## 🔴 REGLA FUNDACIONAL (LEER PRIMERO)

**Tylluan es el PRODUCTO PÚBLICO construido sobre ForjaMCPo3.**

```
ForjaMCPo3 (E:\ForjaMCPo3)  ←  framework interno privado del equipo
        ↓ sustrato cognitivo
Tylluan v0.5.0 (E:\tylluan)  ←  producto público, este repo
```

**NUNCA tocar `E:\ForjaMCPo3` desde este workspace.**
**NUNCA copiar código de Forja directamente — adaptar e implementar limpio.**

---

## Environment

**Platform:** Windows 11 + Claude Code. Bash tool disponible solo para operaciones read-only.
Para arrancar procesos: proporcionar el comando al usuario, no ejecutarlo vía Bash.

**Kernel (instalado):** `tylluan-cli start`
**Kernel (desde source):** `cargo run -p tylluan-cli -- start`
**Health check:** `curl http://127.0.0.1:3030/health`
**Dashboard:** `cd dashboard && pnpm dev` → `http://localhost:5173`

---

## Arquitectura Invariante

1. **5 sovereign tools:** `tylluan_do`, `tylluan_remember`, `tylluan_recall`, `tylluan_think`, `tylluan_graph`
2. **BGE-M3 a 1024 dimensiones** — `vector_dimensions = 1024` en `tylluan.toml`. NUNCA reducir a 768.
3. **Headless-first:** kernel sin UI propia. Dashboard React en `/dashboard`.
4. **Puerto único:** `tylluan-nexus` escucha en `:3030` directamente (sin proxy en Tylluan — a diferencia de Forja).
5. **AGPL soberanía:** sin dependencias cloud en el critical path.
6. **n8n es OPCIONAL** — `n8n_bridge` en lazy, nunca en always_on.

---

## Milestones entregados

| Milestone | Descripción | Estado |
|-----------|-------------|--------|
| **M1-M7** | Memoria, embeddings, retrievals, kernel | ✅ v0.1.0 |
| **M10/M11** | Work Contracts + Federación completa | ✅ v0.3.0 |
| **Encryption** | SQLCipher AES-256 cifrado en reposo | ✅ v0.3.0 |
| **Docker** | Imagen de producción en `:3030`, caché BGE-M3 persistente | ✅ v0.4.0 |
| **Security CI** | 30 tests automatizados — intent filter, ACL, rate limiter | ✅ v0.4.0 |
| **CI** | Pipeline 5 jobs | ✅ 352 tests |
| **M12** | Ed25519 identity · STUN NAT · mDNS LAN · node signing | ✅ v0.4.0 |
| **M13** | Binary releases (3 targets) · install scripts · `tylluan-cli` | ✅ v0.4.0 |
| **M14-A** | DHT Kademlia routing table · Ed25519 XOR metric · mainline bootstrap · 23 tests | ✅ v0.5.0 |
| **M14-B** | Gossip protocol · symmetric push-pull · LRU store · anti-entropy cursors | ✅ v0.5.0 |

## v0.5.0 — Mesh Fabric (en curso)

| Milestone | Descripción | Estado |
|-----------|-------------|--------|
| **M14-A** | DHT Kademlia | ✅ |
| **M14-B** | Gossip protocol · symmetric push-pull · LRU store | ✅ |
| **M14-C** | Noise XK (TCP) + NK (HTTP) · Ed25519→X25519 · wired to federation sync endpoints | ✅ |
| **M14-D** | Cross-datacenter federation — latency-aware routing | 🔜 |
| **M14-E** | Mesh test harness — fault injection, partition, recovery | 🔜 |

---

## Agentes del equipo en este workspace

| Agente | Runtime | Rol en Tylluan |
|--------|---------|----------------|
| **Claude Code Sonnet** | `E:\ForjaMCPo3` | Tech lead global — contexto, planes, prompts |
| **OpenCode (DeepSeek V3 Flash)** | VS Code extension | Implementación Rust — crates/ |
| **Antigravity** | Browser + MCP | Research web, auditoría MCP desde cliente |
| **Qwen Desktop** | App escritorio | Research masivo, auditoría vía SSE MCP |

---

## MCP — Conexión a Forja

Este workspace tiene acceso a Forja vía MCP (`.claude/settings.json`):
- Coloquio para comunicación entre agentes
- Memoria compartida del equipo

Endpoint: `http://127.0.0.1:3030/sse` (requiere Forja corriendo)

---

## Key File Locations

| Archivo | Propósito |
|---------|-----------|
| `crates/tylluan-kernel/src/transport/server.rs` | Sovereign tools + `all_tools()` |
| `crates/tylluan-kernel/src/router/embeddings.rs` | BGE-M3 — verificar sin truncación a 768 |
| `crates/tylluan-kernel/src/db/schema.rs` | Schema SilvaDB — debe ser VECTOR(1024) |
| `crates/tylluan-link/src/dht/` | DHT Kademlia — M14-A |
| `guilds/core/` | Python guilds (fastmcp) |
| `tylluan.toml` | Config runtime |
| `dashboard/` | React dashboard |

---

## 🧭 North Star — Invariante Fundacional (leer antes de proponer cualquier feature)

**Invariante de portabilidad:** Un único binario arranca offline en hardware modesto (RPi4, CPU sin GPU) y también en un servidor. Sin dependencias de red en el path crítico. El conocimiento persiste en local, la sync con peers es oportunista — no requerida.

**Filtro de decisión para cualquier nueva feature:** ¿Puede el mismo componente servir a un usuario con hardware modesto (5-10 peers, CPU sin internet) Y a uno con servidor (100+ peers, datacenter) **sin bifurcar el código** — solo diferente `tylluan.toml`? Si no, el diseño está mal, no el scope.

**Invariantes derivados:**
- **Toaster-friendly:** debe funcionar en Raspberry Pi 4 (4GB RAM) y hardware de 10 años
- **USB-portable:** el bundle completo (binario + DB + modelo opcional) cabe y arranca desde un USB
- **Offline-first:** el kernel arranca y opera sin internet. La sync es oportunista, no requerida
- **Sin bifurcación de código:** un solo binario, configuración distinta por entorno

**Historial de deriva detectada (2026-06-30, coloquio T58-T67):**
- M14-D "cross-datacenter federation" → diferido (fuera del north star). Ver sección M14-D en ROADMAP.md.
- Un nodo en entorno modesto con 5-10 peers no necesita DHT a escala BitTorrent — pero DHT arranca en vacío sin coste, por lo que no se elimina.

---

## Reglas críticas

- NUNCA `vector_dimensions = 768` — rompe todos los embeddings
- NUNCA `host = "0.0.0.0"` + `dev_mode = true` juntos (LAN RCE)
- NUNCA tokens en archivos trackeados — solo en `.tylluan-token` (untracked)
- NUNCA iniciar procesos vía Bash (AV bloquea spawning en Windows)
- NUNCA tocar `E:\ForjaMCPo3` desde aquí — son workspaces separados
- NUNCA reducir timeouts en tests de DHT — BGE-M3 en CPU tarda 2-8s por embedding
