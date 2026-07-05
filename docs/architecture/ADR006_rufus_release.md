# ADR-006 — M15: Rufus Release (v0.12.0 Install Experience)

**Status:** Accepted  
**Date:** 2026-07-04  
**Authors:** Tech Lead (Claude), equipo (coloquio #general T162–T176)  
**Depends on:** M13 binary releases (tylluan-cli, install scripts base), M7 bundled-dashboard

---

## Context

Tylluan v0.11.0 tiene el motor completo: BGE-M3 hybrid search, mesh P2P, federation, 363 tests, CI verde. Tiene binarios para 4 targets y `tylluan-cli start`.

**El problema:** Nadie externo puede instalarlo sin saber Rust. El flujo actual requiere:
1. Instalar Rust toolchain (`rustup`)
2. Clonar el repo
3. `cargo build -p tylluan-cli`
4. Leer `README.md` para entender qué configurar
5. Descargar modelos ONNX manualmente

Esto viola el Rufus test: **Rufus funciona en frío, en cualquier máquina, sin que nadie tenga que explicar nada.** Lleva 15 años siendo así.

**Consecuencia actual:** Tylluan es un laboratorio privado del equipo. No puede ser adoptado por desarrolladores externos, ni por agentes externos que quieran usarlo como backend de memoria. Todo el trabajo de M14 (mesh P2P) no tiene usuarios reales porque nadie puede instalarlo.

**Norte permanente (enunciado del equipo, #general T164-T172):**  
> "tylluan-cli start funciona en frío en una máquina que nunca ha visto Rust, en menos de 5 minutos, sin leer ningún documento."

---

## Constraints

- Sin Rust, sin cargo, sin Python requeridos para el usuario final
- `embedding_model = "none"` (BM25-only) es el default — funciona sin descargar modelos
- El container Docker (P2) es **del usuario**, no del vendor — soberanía intacta
- CONTRACT-01 inviolable (5 sovereign tools, no se toca)
- No añadir dependencias cloud en el critical path
- Los install scripts existentes (`install.sh` / `install.ps1`) se reescriben desde cero — no se parchean

---

## Alternatives Considered

### Option A — Mejorar los install scripts existentes (rejected)
Los scripts en `M13` descargan el binario pero no hacen el health check ni imprimen la configuración MCP. Parchearlos introduce deuda técnica — ya tienen lógica condicional frágil.  
**Rechazado:** mejor reescritura limpia con test de smoke integrado.

### Option B — GUI installer (.msi / .dmg) (rejected)
Un instalador gráfico es el máximo de UX, pero requiere CI de firma de código, notarización macOS, y mantenimiento de plataforma específico. Coste > beneficio en esta fase.  
**Rechazado:** demasiado overhead para el tamaño del equipo.

### Option C — Solo Docker, sin binario nativo (rejected)
Docker es opcional para quienes quieren aislamiento — no puede ser el único camino. Un RPi4 sin Docker Desktop tiene que poder instalar Tylluan igual.  
**Rechazado:** viola el principio "toaster friendly".

### Option D — Binario + scripts mejorados + Docker como primera opción (CHOSEN)
El binario nativo sigue siendo el camino principal. Los install scripts se reescriben con smoke test integrado. Docker se añade como opción equivalente para quienes lo prefieran. La first-run experience se mejora en el kernel mismo.

---

## Decision: Rufus Release — 4 fases

### Fase P0 — Install scripts que realmente funcionan

**Archivos:** `install.sh` (Linux/macOS), `install.ps1` (Windows)

**Flujo completo que debe implementar cada script:**

```
1. Detectar OS y arquitectura (x86_64 / aarch64)
2. Detectar la última release de GitHub (sin token, API pública)
3. Descargar el binario correcto desde GitHub Releases
4. Colocar en ~/.tylluan/bin/ + añadir al PATH (con instrucción clara si no puede)
5. Ejecutar: tylluan-cli start --profile portable
   → portable = embedding_model = "none" (BM25-only, sin descargar nada)
6. Esperar hasta que /health responde OK (max 30s, con spinner)
7. Imprimir: "✓ Tylluan está corriendo en http://127.0.0.1:3030"
8. Llamar al módulo de first-run (P1) para imprimir config MCP
```

**Smoke test automatizado (CI):**
- Job `install-smoke-linux`: Ubuntu 22.04 limpio → `curl -fsSL .../install.sh | bash` → `curl 127.0.0.1:3030/health` → assert `{"status":"ok"}`
- Job `install-smoke-windows`: Windows Server 2022 limpio → `irm .../install.ps1 | iex` → mismo assert

**Invariantes:**
- Sin Rust. Sin cargo. Sin pip. Sin node.
- Si falla algo, el mensaje de error dice exactamente qué hacer (no "error: unknown").
- El script es idempotente — ejecutarlo dos veces no rompe nada.

---

### Fase P1 — First-run experience

**Dónde vive:** en `tylluan-cli` (comando `tylluan-cli start`) y opcionalmente en el kernel mismo (endpoint `GET /api/v1/setup-hint`).

**Comportamiento en primera ejecución (cuando no existe `~/.tylluan/config.toml`):**

```
╔════════════════════════════════════════════════════════╗
║  Tylluan v0.12.0 arrancado en http://127.0.0.1:3030   ║
║  Modo: BM25 (sin modelo — ideal para empezar)          ║
╠════════════════════════════════════════════════════════╣
║  Conecta tu cliente MCP:                               ║
║                                                        ║
║  Claude Desktop (~/.claude/claude_desktop_config.json):║
║  {                                                     ║
║    "mcpServers": {                                     ║
║      "tylluan": { "type": "sse",                      ║
║        "url": "http://127.0.0.1:3030/sse" }           ║
║    }                                                   ║
║  }                                                     ║
║                                                        ║
║  Claude Code: /mcp add tylluan sse                    ║
║    http://127.0.0.1:3030/sse                          ║
║                                                        ║
║  Para BGE-M3 (mejor recall):                          ║
║    tylluan-cli download-models                        ║
╚════════════════════════════════════════════════════════╝
```

**Cambios en el kernel:**
- `embedding_model = "none"` como valor por defecto en `tylluan.toml` generado en primera ejecución
- En modo `none`: BM25 funciona, HNSW y embeddings desactivados con mensaje claro (no error silencioso)
- `GET /api/v1/setup-hint` devuelve JSON con configs MCP para los 3 clientes principales

**Criterio:** Un usuario que acaba de instalar Tylluan sabe cómo conectar su cliente MCP sin leer ningún documento externo.

---

### Fase P2 — Docker imagen oficial

**Imagen:** `ghcr.io/forja-orca/tylluan:latest` y `ghcr.io/forja-orca/tylluan:v0.12.0`

**Flujo de uso:**

```bash
# Linux / macOS
docker run -d \
  --name tylluan \
  -p 3030:3030 \
  -v ~/.tylluan:/data \
  ghcr.io/forja-orca/tylluan:latest

# Windows PowerShell
docker run -d `
  --name tylluan `
  -p 3030:3030 `
  -v "$env:USERPROFILE\.tylluan:/data" `
  ghcr.io/forja-orca/tylluan:latest
```

**Spec de la imagen:**
- Base: `debian:bookworm-slim` (no alpine — ONNX Runtime necesita glibc)
- Solo el binario `tylluan-nexus` + `tylluan-cli` + runtime ONNX
- Datos en `/data` (volumen del usuario) — los datos son del usuario, no del vendor
- Health check: `HEALTHCHECK CMD curl -f http://localhost:3030/health || exit 1`
- Sin secrets en la imagen — token en `/data/.tylluan-token` (montado por el usuario)

**Invariante de soberanía:** El container no hace llamadas a ningún servicio externo por defecto. Ni telemetría, ni ping home, ni actualizaciones automáticas. El usuario controla todo lo que sale de su máquina.

**CI job:** `docker-smoke` — build + `docker run` + health check en GitHub Actions.

---

### Fase P3 — Verificación OpenClaw (paralela, informa M17)

No es trabajo de kernel — es investigación. Corre en paralelo a P0/P1/P2.

**Tarea para Antigravity + Qwen:**

1. **Stars reales:** abrir `https://github.com/OpenClaw-AI/openclaw` (o el repo correcto) y contar stars. Si no existe o son < 50k, la señal no es real.
2. **Spike de integración (2h máximo):**
   - ¿OpenClaw tiene soporte nativo para MCP servers?
   - ¿Puede conectarse a `http://127.0.0.1:3030/sse` como memory backend sin cambios en Tylluan?
   - Si sí: ¿cuántas horas de trabajo es la integración completa?
3. **Informe en #general** con recomendación binaria: M17 Rama A (integración) o Rama B (permisos granulares).

**Deadline:** antes de que Deep cierre P1. La rama de M17 se decide entonces.

---

## Implementation Order

```
P3 (Antigravity + Qwen) ──── paralelo ────────────────────────────────▶ informe M17
P0 (Deep)               ──── install.sh/ps1 ───────────────────────────▶
P1 (Deep + Claude)      ────────────────────── first-run UX ───────────▶
P2 (Deep impl)          ──────────────────────────── Docker ───────────▶
                                                            Antigravity valida Docker
```

P0 es el bloquero de todo. P1 puede arrancar en paralelo con P0 si Deep tiene dos frentes. P2 es el último.

---

## Acceptance Criteria (DoD de M15)

El milestone cierra cuando se cumplen **todos** simultáneamente:

- [ ] `curl -fsSL .../install.sh | bash` en Ubuntu 22.04 limpio → kernel UP en < 5 min
- [ ] `irm .../install.ps1 | iex` en Windows 11 limpio → kernel UP en < 5 min
- [ ] Primera ejecución imprime config MCP para Claude Desktop, Claude Code, Cursor
- [ ] `docker run ghcr.io/forja-orca/tylluan:latest` arranca y responde `/health`
- [ ] CI verde: jobs `install-smoke-linux`, `install-smoke-windows`, `docker-smoke`
- [ ] P3 entrega informe (no bloquea el cierre de M15, pero debe llegar antes de abrir M17)

---

## Non-Goals (M15 no incluye)

- GUI installer (.msi / .dmg) — post-M19
- Auto-update en el binario — M19
- Profile wizard interactivo — M19
- Soporte para más modelos de embedding — no hay demanda probada
- Cambios en el kernel (CONTRACT-01, sovereign tools, memory layer) — M15 es solo install/UX

---

## Consequences

**Positivo:**
- Tylluan pasa de laboratorio privado a producto instalable por cualquiera
- Los usuarios externos pueden conectar cualquier cliente MCP en < 5 minutos
- Docker imagen oficial desbloquea despliegues en servidores sin necesidad de compilar
- OpenClaw verification determina la dirección de M17 con datos reales, no suposiciones

**Negativo / Riesgos:**
- Los CI jobs de smoke test requieren GitHub Actions runners con red real (no puede ser solo `cargo test`)
- La imagen Docker en `ghcr.io` requiere que José configure GitHub Container Registry permissions (no es automático)
- P3 puede devolver "OpenClaw no es lo que pensábamos" — en ese caso M17 cambia de plan, pero eso es preferible a construir una integración sobre datos falsos

---

## Related ADRs

- [ADR-004 — M14-D Guild Execution Channels](M14D_dispatch_spec.md)
- [ADR-005 — M14-F P2P TCP Dispatch](M14F_p2p_dispatch_spec.md)
- ADR-007 (pendiente) — M16 BGE-M3 Benchmark Real
