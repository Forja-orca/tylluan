# Tylluan — Spec

## Qué es

Tylluan es un **kernel MCP soberano** (memoria persistente + grafo de conocimiento + coordinación multi-agente + mesh P2P) que cualquier persona u organización puede instalar y correr localmente, sin dependencias cloud en el critical path.

Es el producto público del equipo Forja — construido probando patrones primero en este mismo repo (laboratorio) antes de portarlos (adaptados) a la herramienta interna del equipo.

## 🟢 Estado: PÚBLICO — MIT license

- Repositorio abierto en GitHub, licencia MIT, cualquiera puede clonar/instalar/contribuir
- Del equipo Forja al mundo — sin gate de acceso, sin cuenta, sin cloud obligatorio
- Roadmap y estado real en `ROADMAP.md` / `STATUS.md` (fuente de verdad técnica, actualizar en cada release)

## Para quién es esto (3 audiencias)

### 1. Agentes constructores (contributors: humanos o IA que modifican el código)
Casos de uso:
- Levantar el entorno de desarrollo: `cargo build -p tylluan-kernel`, `.\tylluan-mcp.bat`
- Seguir `CONTRIBUTING.md` y `AGENTS.md` para el flujo de PRs
- Leer `docs/internal/PROJECT.map` / `OPERATIONS.map` para navegar la arquitectura sin releer todo el repo
- Respetar `AI_POLICY.md` si el contributor es un agente de IA

### 2. Agentes usuarios (clientes MCP de terceros que conectan a un Tylluan instalado)
Casos de uso:
- Instalar con `tylluan install --profile portable|clinic|server` y conectar vía `:3030/sse`
- Usar las 5 sovereign tools (`tylluan_do`, `tylluan_remember`, `tylluan_recall`, `tylluan_think`, `tylluan_graph`) para dar memoria persistente a cualquier agente sin tooling nativo
- Casos reales: un médico en zona sin internet que necesita memoria clínica offline-first (perfil `clinic`); un desarrollador que quiere que su agente de código recuerde contexto entre sesiones (perfil `portable`); un equipo que necesita mesh compartido entre varias instancias (perfil `server`)

### 3. Humanos (usuarios finales que instalan Tylluan para sí mismos)
Casos de uso:
- Instalación en <5 min siguiendo `docs/QUICKSTART.md`, sin necesidad de leer código
- Dashboard web (`:3030/` o `:5173` en dev) como punto de entrada visual, con wizard de primera vez (M23-P1)
- Confiar en que sus datos nunca salen de su máquina (soberanía, AGPL/MIT sin telemetría oculta)

## Documentación que falta (pendiente, priorizado)

| Falta | Para quién | Prioridad |
|-------|-----------|-----------|
| Casos de uso reales documentados con ejemplos (el "médico offline", "dev con memoria persistente", "equipo con mesh") — hoy dispersos en Coloquio, no en un doc público | Humanos, agentes usuarios | Alta |
| Guía "Tylluan vs Letta/Mem0/Zep" — comparativa honesta para credibilidad externa (M23 roadmap viejo lo pedía, nunca se escribió) | Humanos evaluando adopción | Alta |
| `CONTRIBUTING.md` con good-first-issues reales etiquetados (mencionado en roadmap, no verificado que exista en disco) | Agentes constructores nuevos/externos | Media |
| Documentación de perfiles de instalación (`portable`/`clinic`/`server`) con criterio claro de cuál elegir | Humanos, agentes usuarios | Media |
