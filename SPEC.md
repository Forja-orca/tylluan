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
- Leer `docs/concepts/ARCHITECTURE.md` / `docs/concepts/PROJECT_STRUCTURE.md` para navegar la arquitectura sin releer todo el repo
- Respetar `AI_POLICY.md` si el contributor es un agente de IA

### 2. Agentes usuarios (clientes MCP de terceros que conectan a un Tylluan instalado)
Casos de uso:
- Instalar con `tylluan install --profile portable|clinic|server` y conectar vía `:4000/sse`
- Usar las 5 sovereign tools (`tylluan_do`, `tylluan_remember`, `tylluan_recall`, `tylluan_think`, `tylluan_graph`) para dar memoria persistente a cualquier agente sin tooling nativo
- Casos reales: un médico en zona sin internet que necesita memoria clínica offline-first (perfil `clinic`); un desarrollador que quiere que su agente de código recuerde contexto entre sesiones (perfil `portable`); un equipo que necesita mesh compartido entre varias instancias (perfil `server`)

### 3. Humanos (usuarios finales que instalan Tylluan para sí mismos)
Casos de uso:
- Instalación en <5 min siguiendo `docs/getting-started/QUICKSTART.md`, sin necesidad de leer código
- Dashboard web (`:4000/` o `:5173` en dev) como punto de entrada visual, con wizard de primera vez (M23-P1)
- Confiar en que sus datos nunca salen de su máquina (soberanía, licencia MIT sin telemetría oculta)

## Documentación que falta (pendiente, priorizado)

| Falta | Para quién | Prioridad |
|-------|-----------|-----------|
| Casos de uso reales documentados con ejemplos (el "médico offline", "dev con memoria persistente", "equipo con mesh") — hoy dispersos en Coloquio, no en un doc público | Humanos, agentes usuarios | Alta |
| Guía "Tylluan vs Letta/Mem0/Zep" — comparativa honesta para credibilidad externa (M23 roadmap viejo lo pedía, nunca se escribió) | Humanos evaluando adopción | Alta |
| `CONTRIBUTING.md` con good-first-issues reales etiquetados (mencionado en roadmap, no verificado que exista en disco) | Agentes constructores nuevos/externos | Media |
| Documentación de perfiles de instalación (`portable`/`clinic`/`server`) con criterio claro de cuál elegir | Humanos, agentes usuarios | Media |

---

## Propiedades de Soberanía

Tylluan sigue estas 7 propiedades como principios de diseño explícitos:

1. **Localidad de Datos Física:** Toda la base de conocimiento (SilvaDB) reside localmente en archivos SQLite bajo el directorio del usuario. Cero dependencias de APIs en la nube en la ruta crítica.
2. **Ejecución Hardware-Bound:** Optimizado específicamente para hardware con recursos limitados (Raspberry Pi 4 / ARM64). El motor híbrido (BM25 + fastembed ONNX) corre local sin requerir GPUs comerciales pesadas.
3. **Ausencia de Telemetría Externa:** Sin llamadas ocultas de diagnóstico ni recolección de estadísticas fuera del host.
4. **Criptografía Soberana:** Identidad del nodo gestionada localmente mediante firmas criptográficas Ed25519 y transporte cifrado a través de Noise Protocol XK.
5. **Decaimiento Adaptativo (FSRS-5):** La memoria humana olvida de forma selectiva. FSRS-5 permite que cada nodo mantenga su propia estabilidad y retrievabilidad a nivel de base de datos, optimizando el contexto de forma biológica sin depender de LLMs para ponderar frescura.
6. **Federación en Red P2P:** Redundancia distribuida sin servidores centrales. Sincronización push/pull directa y anti-bucles mediante Kademlia DHT.
7. **Código Soberano (Licencia MIT):** Libre de licencias corporativas restrictivas o gatekeeping comercial.

---

## Tabla Comparativa: Tylluan vs Estado de la Arte (Mem0 / Letta / Zep)

| Dimensión | Tylluan | Mem0 | Letta (formerly MemGPT) |
|-----------|---------|------|-------------------------|
| **Soberanía** | 🟢 Local-first (ONNX/SQLite) | 🟡 Cloud-first / API key | 🟡 Local / Configuración compleja |
| **Optimización Edge (Pi 4)** | 🟢 Sí (fórmula exponencial FSRS) | 🔴 No (depende de llamadas OpenAI) | 🔴 No (alto consumo en base de datos) |
| **Algoritmo de Olvido** | 🟢 FSRS-5 por nodo + Retrievability | 🔴 Peso estático (LIFO) | 🟡 Memoria jerárquica (L1/L2) con LLM |
| **Federación P2P** | 🟢 Nativo (Kademlia + Noise XK) | 🔴 No | 🔴 Centralizado (Server-Client) |
| **Consolidación** | 🟢 DreamCycle (dedup/decay automático) + memoria episódica | 🔴 No | 🟡 Buffer de mensajes manual |
| **Tooling** | 🟢 5 Sovereign Tools MCP | 🟡 Integración custom | 🟡 Agent-specific APIs |

