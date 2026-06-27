# Tylluan v0.2.0 — Claude Code Instructions

## 🔴 REGLA FUNDACIONAL (LEER PRIMERO)

**Tylluan es el PRODUCTO PÚBLICO construido sobre ForjaMCPo3.**

```
ForjaMCPo3 (E:\ForjaMCPo3)  ←  framework interno privado del equipo
        ↓ sustrato cognitivo
Tylluan v0.2.0 (E:\tylluan)  ←  producto público, este repo
```

**NUNCA tocar `E:\ForjaMCPo3` desde este workspace.**
**NUNCA copiar código de Forja directamente — adaptar e implementar limpio.**

---

## Environment

**Platform:** Windows 11 + Claude Code. Bash tool disponible solo para operaciones read-only.
Para arrancar procesos: proporcionar el comando al usuario, no ejecutarlo vía Bash.

**Kernel:** `tylluan-nexus.exe` + `tylluan-proxy.exe` vía `.\tylluan-mcp.bat`
**Dashboard:** `cd dashboard && npm run dev` → `http://localhost:5173`
**Health check:** `curl http://127.0.0.1:3030/health`

---

## Arquitectura Invariante

1. **5 sovereign tools:** `tylluan_do`, `tylluan_remember`, `tylluan_recall`, `tylluan_think`, `tylluan_graph`
2. **BGE-M3 a 1024 dimensiones** — `vector_dimensions = 1024` en `tylluan.toml`. NUNCA reducir a 768.
3. **Headless-first:** kernel sin UI propia. Dashboard React en `/dashboard`.
4. **Proxy + Kernel:** `tylluan-proxy` en `:3030` (fijo) + `tylluan-nexus` en puerto dinámico.
5. **AGPL soberanía:** sin dependencias cloud en el critical path.
6. **n8n es OPCIONAL** — `n8n_bridge` en lazy, nunca en always_on.

---

## Milestones Tylluan v0.2.0

| Milestone | Descripción | Estado |
|-----------|-------------|--------|
| **M2** | Hybrid Search v2 — BGE-M3 1024-dim nativo | ✅ Listo |
| **M1** | Memory Decay & Salience — half-life en SilvaDB | ✅ Listo |
| **M4** | rmcp Migration — sse.rs custom + stdio/server = rmcp | ✅ Listo (M4-B) |
| **M6** | Dual-Level Retrieval — LightRAG pattern | Siguiente |
| **M7** | Single-Binary Packaging — polish final + release | Último |

**M2 es bloqueante para todo lo demás.** Si `vector_dimensions = 768` en producción, todos los embeddings son incompatibles desde el primer nodo.

---

## P0 — BGE-M3 Dimension Fix (M2)

El bug heredado de Forja: `embeddings.rs` truncaba BGE-M3 de 1024 → 768 por compatibilidad con SilvaDB existente. Tylluan empieza limpio — **1024 desde el primer nodo**.

Archivos clave:
- `tylluan.toml`: `vector_dimensions = 1024` ✅ (ya corregido)
- `crates/tylluan-kernel/src/router/embeddings.rs`: verificar que NO hay truncación a 768
- `crates/tylluan-kernel/src/db/schema.rs`: verificar que `VECTOR(1024)` en CREATE TABLE

---

## Agentes del equipo en este workspace

| Agente | Runtime | Rol en Tylluan |
|--------|---------|----------------|
| **Claude Code Haiku** | Este workspace | Tech lead Tylluan — contexto, planes, prompts profesionales |
| **OpenCode (DeepSeek)** | VS Code extension | Implementación Rust — crates/tylluan-kernel/ |
| **Antigravity** | Browser + MCP | Research web, auditoría MCP desde cliente |
| **Hermes** | WSL/terminal | Testing, validación, scripts |
| **Qwen Desktop** | App escritorio | Research masivo, auditoría vía SSE MCP |

**Claude Code Haiku** en este workspace mantiene el contexto de Tylluan y orquesta la flota.
**Claude Code Sonnet** en `E:\ForjaMCPo3` mantiene el contexto global del stack — Tech Lead general.

---

## MCP — Conexión a Forja

Este workspace Claude Code tiene acceso a Forja vía MCP (`.claude/settings.json`):
- Coloquio para comunicación entre agentes
- Memoria compartida del equipo
- Herramientas de investigación y búsqueda

Endpoint: `http://127.0.0.1:3030/sse` (requiere Forja corriendo)

---

## Key File Locations

| Archivo | Propósito |
|---------|-----------|
| `crates/tylluan-kernel/src/transport/server.rs` | Sovereign tools + `all_tools()` |
| `crates/tylluan-kernel/src/router/embeddings.rs` | BGE-M3 — verificar sin truncación a 768 |
| `crates/tylluan-kernel/src/db/schema.rs` | Schema SilvaDB — debe ser VECTOR(1024) |
| `guilds/core/` | Python guilds (fastmcp) |
| `tylluan.toml` | Config runtime |
| `dashboard/` | React dashboard |

---

## Arranque del Kernel

```bat
.\tylluan-mcp.bat
```

Verificar: `curl http://127.0.0.1:3030/health`

---

## Reglas críticas

- NUNCA `vector_dimensions = 768` — rompe todos los embeddings
- NUNCA `host = "0.0.0.0"` + `dev_mode = true` juntos (LAN RCE)
- NUNCA tokens en archivos trackeados — solo en `.tylluan-token` (untracked)
- NUNCA iniciar procesos vía Bash (AV bloquea spawning en Windows)
- NUNCA tocar `E:\ForjaMCPo3` desde aquí — son workspaces separados
