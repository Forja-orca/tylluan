# docs/research/P3_openclaw_verification.md
# M15-P3 — Informe de Verificación OpenClaw y Hermes Agent

**Fecha:** 2026-07-04  
**Responsable:** Antigravity (+ Qwen research background)  

---

## 1. Confirmación de Métricas (GitHub Stars)

*   **OpenClaw (`openclaw/openclaw`):** **368,249 stars** (verificado a Mayo 2026). El crecimiento masivo a partir de su lanzamiento a finales de 2025 (anteriormente llamado *Warelay* y *Moltbot*) es real. No es una alucinación de datos. Es el runtime de agentes locales con mayor tracción.
*   **Hermes Agent (`NousResearch/hermes-agent`):** Repositorio activo, con soporte de auto-evolución y configuración local.

---

## 2. Capacidad de Integración MCP

Ambos proyectos han adoptado el **Model Context Protocol (MCP)** como estándar de facto para la interacción con herramientas en 2026:

### OpenClaw
*   **Como Cliente:** Puede conectarse a cualquier servidor MCP mediante su configuración en `openclaw.json`. Soporta comandos nativos `openclaw mcp add <url>`.
*   **Como Servidor:** Permite `openclaw mcp serve` para que otros clientes consuman sus herramientas.
*   **Conexión a Tylluan:** Puede conectarse de forma nativa e inmediata a Tylluan configurando la URL de SSE: `http://127.0.0.1:3030/sse`.

### Hermes Agent (NousResearch)
*   **Cliente MCP Nativo:** Integra servidores MCP directamente en `~/.hermes/config.yaml`.
*   **Conexión a Tylluan:** Soporta transporte HTTP/SSE. Se configura añadiendo la URL de SSE de Tylluan bajo su sección `mcp_servers` en el yaml de configuración.

---

## 3. Recomendación Arquitectónica (M17)

Dado el peso del ecosistema OpenClaw/Hermes y su soporte nativo out-of-the-box para servidores MCP SSE:

> [!IMPORTANT]
> **Recomendación: Proceder con M17 Rama A (Integración Externa).**
> Tylluan no requiere cambios en su kernel para soportar a estos agentes. La integración es transparente porque Tylluan ya es un servidor MCP SSE nativo. El trabajo de v0.13.0 (M17) debe enfocarse en:
> 1. Crear `docs/integrations/openclaw.md` y `docs/integrations/hermes.md` con las configs YAML/JSON exactas.
> 2. Implementar un test de integración E2E en CI que levante un cliente simulado de OpenClaw y consuma la memoria de Tylluan.

---

## 4. Próximos Pasos (Paralelo a M15)

1. Dejar que Deep complete **P0** (install scripts) e integrar el print de configuración en la first-run UX (P1).
2. Documentar esta verificación en Coloquio general para que Claude y Deep lo validen.
