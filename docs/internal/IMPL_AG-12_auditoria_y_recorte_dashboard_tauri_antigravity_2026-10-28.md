# Informe AG-12 — Auditoría de Uso Real del Dashboard y Propuesta de Recorte (Tauri UI)

**Fecha:** 2026-10-28  
**Autor:** Antigravity (Agente UI/UX)  
**Destinatario:** Tech Lead (@claude-code) / @jose  
**Estado:** No verificado por el Tech Lead — Pendiente de revisión y aprobación formal  
**Base commit:** `399db1f`

---

## 1. Contexto y Objetivos

Conforme a la decisión de arquitectura comunicada por el Tech Lead (@claude-code) y @jose (Turno 1878):
- El dashboard actual cuenta con **6 pestañas consolidadas, 64 componentes principales en `src/components/`, 86 componentes `.tsx` en total, ~29.400 líneas de código y un `node_modules` de 564 MB**.
- Aproximadamente el **90%** de los paneles actuales muestran diagnósticos profundos de laboratorio, matrices internas de ruteo y telemetría de agentes que un operador humano no utiliza en su día a día.
- El objetivo del lote **AG-12** es:
  1. **FASE 1 (Auditoría de Uso Real):** Mapear panel por panel, determinar los endpoints a los que llama, verificar si existen en el kernel y entregan datos reales, y evaluar si un humano realmente lo necesita o si es telemetría de desarrollo/diagnóstico de agentes.
  2. **FASE 2 (Propuesta de Recorte a ~6 Vistas):** Presentar la reorganización a 6 vistas esenciales y definir qué paneles se retiran o quedan en modo "Avanzado/Depuración", **sin borrar código antes de recibir el OK formal**.
  3. **FASE 3 (Tauri Packaging):** Configurar y empaquetar la aplicación como ventana web ligera nativa mediante `crates/tylluan-gui` conectada al kernel local `127.0.0.1`.

---

## 2. Línea Base Verificada del Frontend

Antes de formular la propuesta, se ha comprobado la salud de la base de código actual (`399db1f`):
- **Vitest:** `19 test files passed, 151 passed (151 total)`, 0 fallos (tiempo de ejecución: 18.15s).
- **TypeScript & Vite Build:** Compilación limpia (`dist/` generado, 3709 módulos procesados sin fallos).
- **ESLint:** 0 errores, 36 advertencias preexistentes (respetando la cota máxima tolerada).

---

## 3. FASE 1: Tabla de Auditoría de Componentes y Paneles

La siguiente tabla audita cada componente visual y panel del dashboard contra el backend del kernel de Tylluan, evaluando la utilidad para un usuario u operador humano.

| Componente / Panel | Pestaña / Vista Actual | Endpoint(s) del Kernel que invoca | ¿Existe Endpoint en Kernel? | Calidad de Datos | ¿Uso Humano Diario? | Justificación / Dictamen |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `OverviewTab` | Overview | `/health`, `/health?verbose=true`, `/api/v1/health/golden-signals`, `/api/v1/guilds/utilization`, `/api/v1/memory/retention`, `/api/v1/slo/summary` | **SÍ** | Real (cálculo de estado, señales doradas, uptime) | **SÍ** | Esencial para saber en un golpe de vista si el kernel está vivo, si hay errores y el uso de memoria/guilds. |
| `InteroceptionTab` | Overview (Subtab) | `/api/v1/interoception`, `/api/v1/hormones`, `/api/v1/agent-profiles` | **SÍ** | Real (valores flotantes de estrés, homeostasis, hunger) | **NO** | Metáfora de agentes biológicos. Un operador humano no necesita ver "homeostasis" ni "hambre de conocimiento" para usar Tylluan. |
| `SystemTab` | Overview (Subtab) | `/api/v1/config`, `/api/v1/system/status`, `/api/v1/logs`, `/api/v1/maintenance/*` | **SÍ** | Real (configuración toml, logs de ejecución, vaciado SQLite) | **SÍ** | Muy útil para revisar logs del kernel, reiniciar servicios o limpiar base de datos. |
| `ColoquioTab` | Team (Subtab) | `/api/v1/coloquio/channels`, `/api/v1/coloquio/channels/{id}/messages`, `/post`, `/typing`, `/read` | **SÍ** | Real (mensajería síncrona/asíncrona entre humanos y agentes) | **SÍ (CRÍTICO)** | El canal principal donde el usuario interactúa, da instrucciones y conversa con el equipo de agentes. |
| `ColoquioCanvasWorkspace` | Team (Coloquio) | `/api/v1/do`, WebSockets tldraw | **SÍ** | Real | **NO (Sobredimensionado)** | Incrusta la librería completa de tldraw (>2.5 MB). Casi nunca usado por el operador común; genera sobrecarga masiva de bundle. |
| `FleetTab` | Team (Subtab) | `/api/v1/coloquio/channels`, `/api/v1/dashboard/summary` | **SÍ** | Real (lista agentes registrados) | **DUDOSO** | Relevante solo como monitor secundario; la presencia de agentes ya se ve en Coloquio o en el Navbar. |
| `FleetHealthPanel` | Team (Subtab) | `/api/v1/coloquio/channels/general` | **SÍ** | Real | **NO** | Telemetría interna de loops de agentes. Innecesario para el operador. |
| `WorkContractsPanel` | Team (Subtab) | `/api/v1/work-contracts` | **SÍ** | Real | **NO** | Diagnóstico del protocolo M10 de contratos acotados entre agentes. Cero utilidad para el usuario diario. |
| `AgentCompetencyPanel`| Team (Subtab) | `/api/v1/agents` | **SÍ** | Real | **NO** | Matriz de evaluación interna de competencias. Solo útil para research. |
| `SchedulerConfusionPanel` | Team (Subtab) | `/api/v1/scheduler/confusion` | **SÍ** | Parcial / Mock fallback | **NO (ELIMINAR)** | Detectado en auditorías previas como panel con datos de ejemplo estáticos. |
| `StigmergyHeatmapPanel` | Team (Subtab) | `/api/v1/stigmergy/zones` | **SÍ** | Parcial / Mock fallback | **NO (ELIMINAR)** | Concepto teórico de estigmergia entre agentes con datos de muestra. |
| `DispatchesPanel` | Team (Subtab) | `/api/v1/dispatches` | **SÍ** | Real | **NO** | Diagnóstico de colas de envío de red Noise P2P. |
| `CollectiveTab` | Team (Subtab) | `/api/v1/collective/*`, `/api/v1/setup-hint` | **SÍ** | Real | **NO** | Diagnóstico de reputación de agentes y sugerencias de ruteo. |
| `KnowledgeGraphTab` | Memory (Subtab) | `/api/v1/silva/graph`, `/api/v1/memory/search` | **SÍ** | Real | **SÍ (SIMPLIFICADO)** | Visualizar relaciones entre conceptos recordados es útil si es ligero; actualmente el visor 2D/3D es muy pesado. |
| `NodesTab` | Memory (Subtab) | `/api/v1/nodes`, `/api/v1/silva/nodes/{id}` | **SÍ** | Real (tabla de nodos de SilvaDB) | **SÍ (ALTO)** | Permite al operador buscar, ver contenido, editar o borrar recuerdos concretos en la memoria. |
| `ContradictionsPanel`| Memory (Subtab) | `/api/v1/silva/contradictions`, `/api/v1/silva/conflicted` | **SÍ** | Real | **SÍ** | Clave para saber si hay información contradictoria en la memoria del sistema y resolverla. |
| `QuarantinePanel` | Audit (Subtab) | `/api/v1/memory/quarantine`, `/release` | **SÍ** | Real (nodos bloqueados por ASI06 / CoherenceGate) | **SÍ (CRÍTICO)** | El operador debe poder ver qué memorias sospechosas fueron puestas en cuarentena y liberarlas. |
| `AuditTrailPanel` | Audit (Subtab) | `/api/v1/audit/logs`, `/verify`, `/latency` | **SÍ** | Real (registro append-only de seguridad) | **SÍ** | Permite auditar qué herramientas ejecutó cada agente y si hubo violaciones de permisos. |
| `TrustConsoleTab` | Audit (Subtab) | `/health?verbose=true`, `/api/v1/guilds` | **SÍ** | Real | **SÍ** | Muestra el estado del contrato CONTRACT-01 (6/6 sovereign tools verificadas). |
| `DecisionServicePanel`| Audit (Subtab) | `/api/v1/decide`, `/api/v1/decide/shadow` | **SÍ** | Real | **NO** | Comparador entre System 1 y System 2 de toma de decisiones agénticas. Solo para evals. |
| `CoherenceGatePanel` | Audit (Subtab) | `/api/v1/security/coherence-gate/stats` | **SÍ** | Real | **DUDOSO / AVANZADO** | Muestra métricas de filtros L1-L4. Mejor como telemetría resumida en Cuarentena. |
| `FrictionPanel` | Audit (Subtab) | `/api/v1/security/friction/stats` | **SÍ** | Real | **NO** | Estadísticas de fricción cognitiva en llamadas a herramientas. |
| `GuildsTab` | Guilds (Subtab) | `/api/v1/guilds`, `/start`, `/stop`, `/test` | **SÍ** | Real (estado y control de procesos guild) | **SÍ** | Permite al operador ver qué herramientas/servidores están corriendo y reiniciarlos si fallan. |
| `OutputsLedgerPanel` | Guilds (Subtab) | `/api/v1/outputs` | **SÍ** | Real | **NO** | Manifiesto de artefactos generados. Ruido para el uso diario. |
| `ConnectorsTab` | Guilds (Subtab) | `/api/v1/setup-hint` | **SÍ** | Real (snippets de configuración JSON para Claude Desktop/Cursor) | **SÍ** | Facilita la configuración de clientes externos en un clic. Puede integrarse en Ajustes. |
| `McpRegistryPanel` | Guilds (Subtab) | `/api/v1/mcp/external` | **SÍ** | Real | **NO** | Gestión de MCPs externos federados. Uso infrecuente. |
| `FederationTab` | Guilds (Subtab) | `/api/v1/federation/*`, `/api/v1/mesh/peers` | **SÍ** | Real | **NO / AVANZADO** | Sincronización P2P Noise XK. Interesante solo en modo federado entre múltiples máquinas. |
| `ModelsTab` | Laboratory | `/api/v1/models`, `/api/v1/config/device/status`, `/api/v1/config/inference-llama` | **SÍ** | Real (gestión de hardware, llama-server y centinela) | **SÍ (CRÍTICO)** | Permite al usuario elegir modelo, ver VRAM/RAM, apagar/encender centinela y servidor local. |
| `LaboratoryTab` | Laboratory (Subtab) | `/api/v1/autoresearch/*` | **SÍ** | Real | **NO** | Submódulo de investigación autónoma (AutoResearch loop). |
| `VisionTab` | Laboratory (Subtab) | `/api/v1/guilds/vision_moondream/start` | **SÍ** | Real | **NO** | Panel de prueba rápida para Moondream. Ruido si los guilds se gestionan centralizados. |
| `PlanModePanel` | Laboratory (Subtab) | `/api/v1/do` | **SÍ** | Real | **NO** | Interfaz secundaria de planes. Se hace directamente en Coloquio. |
| `ProjectSkillsPanel` | Laboratory (Subtab) | `/api/v1/skills` | **SÍ** | Real | **NO / AVANZADO** | CRUD de skills en markdown. |
| `BackgroundJobsPanel`| Laboratory (Subtab) | `/api/v1/jobs` | **SÍ** | Real | **NO** | Cola de trabajos en segundo plano. |

---

## 4. FASE 2: Propuesta de Recorte a ~6 Vistas Esenciales

Para transformar el dashboard en una **UI mínima, ágil, limpia y centrada en el ser humano**, proponemos consolidar toda la aplicación en **6 vistas únicas** en la barra de navegación lateral:

```
[1] 📊 Estado y Salud      (Overview: Uptime, Alertas, CPU/RAM, Golden Signals, 6/6 herramientas)
[2] 💬 Coloquio            (Chat multi-agente, canales, menciones, hilos de trabajo)
[3] 🛡️ Seguridad y Control (Aprobaciones HITL, Cuarentena ASI06, Auditoría de acciones)
[4] 🧠 Memoria SilvaDB     (Buscador semántico/BM25 de recuerdos, visor de nodos, contradicciones)
[5] ⚡ Modelos y Servidor  (Dispositivo CPU/GPU, Centinela ON/OFF, Llama-Server, Estado de Guilds)
[6] ⚙️ Ajustes y Conexión  (Configuración del kernel, snippets MCP Claude/Cursor, Bearer Token)
```

### Detalle de las 6 Vistas:

1. **Estado y Salud (`/overview`):**
   - Incorpora: `OverviewTab` + tarjeta de estado `TrustConsoleTab` (6/6 sovereign tools).
   - Elimina: `InteroceptionTab`, `ResumeSessionWidget`, `RepoMapWidget`, `TeamPulseWidget`.
   - Propósito: Responder a *"¿Está funcionando todo bien en mi Tylluan local?"*.

2. **Coloquio (`/coloquio`):**
   - Incorpora: `ColoquioTab` (selector de canales, hilos de mensajes, campo de redacción, selector de menciones).
   - Retira: `ColoquioCanvasWorkspace` (tldraw) para eliminar la pesadez del bundle y la complejidad de canvas visual.
   - Propósito: La consola central de conversación y colaboración humano-agentes.

3. **Seguridad y Cuarentena (`/security`):**
   - Incorpora: `QuarantinePanel` (inspección y liberación de memorias bloqueadas) + `AuditTrailPanel` (verificación de firmas y registro de auditoría) + Modal de autorización HITL (`pendingGrant`).
   - Retira: `DecisionServicePanel`, `CoherenceGatePanel` y `FrictionPanel` (se mueven a un modal/subvista de diagnóstico avanzado).
   - Propósito: Dar control total al operador sobre qué se bloquea y qué acciones requieren aprobación humana.

4. **Memoria SilvaDB (`/memory`):**
   - Incorpora: `NodesTab` (tabla con buscador rápido de texto/tags, detalle de contenido, borrado seguro) + `ContradictionsPanel` (alerta de conflictos epistémicos).
   - Retira: `KnowledgeGraphTab` y `KnowledgeCortex2D` (el grafo 2D interactivo con force-layout, que consume CPU excesiva y no aporta valor de búsqueda operativa).
   - Propósito: Permitir al usuario consultar qué sabe Tylluan sobre su proyecto y limpiar datos erróneos.

5. **Modelos y Servidores (`/models`):**
   - Incorpora: `ModelsTab` (con los controles de hardware CPU/VRAM, el interruptor del Centinela implementado en T-DSH-114 y el conector de `llama-server` de T-DSH-115) + lista resumida de `GuildsTab` (para encender/apagar herramientas auxiliares).
   - Retira: `OutputsLedgerPanel`, `McpRegistryPanel`, `VisionTab`.
   - Propósito: Gestionar la inferencia local sin tocar la terminal.

6. **Ajustes y Conexión (`/settings`):**
   - Incorpora: `SystemTab` (configuración `tylluan.toml`, mantenimiento/vaciado de base de datos) + `ConnectorsTab` (`/setup-hint` para copiar la configuración a Claude Code, Desktop o Cursor) + Gestión del Bearer Token (`AuthModal`).
   - Propósito: Conectar nuevos harnesses y administrar el archivo de configuración.

### ¿Qué ocurre con los otros ~30 paneles?
- Los paneles puramente experimentales (`SchedulerConfusionPanel`, `StigmergyHeatmapPanel`, `A2aPanel`, `InteroceptionTab`, `WorkContractsPanel`, `LaboratoryTab`) se eliminan del árbol de navegación principal.
- Si en el futuro un desarrollador requiere inspeccionar telemetría interna, se puede habilitar una bandera `dev_mode: true` en Ajustes que renderice una pestaña extra de "Diagnóstico de Agentes", sin sobrecargar la experiencia del usuario general.

---

## 5. FASE 3: Empaquetado en Kernel (`bundled-dashboard`) y Tauri Opcional

Conforme a la precisión del TL y @jose (Turno 1944):
- **Vía Principal:** La UI mínima se embebe directamente en el binario del kernel mediante la feature `bundled-dashboard` (`rust-embed` + `mime_guess`). Un único ejecutable soberano, cero dependencia de WebView externo y funcionamiento offline garantizado.
- **Vía Secundaria / Opcional:** `crates/tylluan-gui` (Tauri 2.0.0-rc) actúa únicamente como ventana de escritorio opcional consumiendo la misma UI recortada (`dashboard/dist`) contra `127.0.0.1:<puerto>`. Sin duplicar código ni instalar dependencias nuevas (T1872).
- **Verificación de Compilación de `bundled-dashboard`:**
  - Ejecutado `cargo check -p tylluan-kernel --features bundled-dashboard` en este ciclo: **COMPILACIÓN EXITOSA (Exit 0)** en 3m 36s.

---

## 6. Métricas Reales de Rendimiento (Línea Base Pre-Recorte)

Conforme a la regla de no afirmar mejoras sin medición empírica:

| Parámetro | Línea Base (Pre-Recorte v0.17.2) | Objetivo Post-Recorte (AG-12) | Delta / Impacto Medido |
| :--- | :--- | :--- | :--- |
| **Tamaño Total de `dist/`** | **3.458.730 bytes (3,30 MB)** | < 1.200.000 bytes (< 1,15 MB) | Reducción esperada > 60% al retirar `tldraw` (1,76 MB JS + 80 KB CSS) y `force-graph` (181 KB). |
| **Peticiones HTTP Periódicas al Abrir** | **15 peticiones concurrentes** (10 en `refreshData` inicial de `useNexus` + 5 en `usePolling` en `App.tsx` y `OverviewTab`) | **<= 4 peticiones concurrentes** (consolidación a `/health`, `/api/v1/coloquio/unread` y `dashboard_summary`) | Reducción de más del 70% del tráfico interno periódico contra el kernel local. |
| **Tiempo de Transformación / Build** | 33,60s (3709 módulos) | < 15,00s (< 1500 módulos) | Menor tiempo de compilación tanto en CI como en `rust-embed`. |
| **Tests de Vitest** | 19 test files (151 passed, 18,15s) | 100% pasando adaptados a las 6 vistas | Cero regresiones en tests. |

---

## 6. Siguientes Pasos (DoD AG-12)

1. **Esperar el OK formal del Tech Lead (@claude-code)** sobre esta propuesta de 6 vistas antes de tocar o borrar un solo componente en `dashboard/`.
2. Una vez aprobado:
   - Reestructurar el enrutador de `App.tsx` en `dashboard/` para las 6 vistas acordadas.
   - Depurar las importaciones no utilizadas y asegurar que `pnpm test`, `pnpm build` y `pnpm lint` pasen limpios sin advertencias nuevas.
   - Ajustar `tauri.conf.json` y `main.rs` en `crates/tylluan-gui` para apuntar a `pnpm` y al puerto kernel correspondiente.
   - Probar la compilación de `tylluan-gui` y entregar el informe final.

---

## Impact
- **Riesgo funcional:** Nulo en el kernel (cero cambios en `crates/tylluan-kernel` ni en la API REST).
- **Riesgo en frontend:** Reducción sustancial del bundle (eliminación de librerías pesadas como `tldraw`), menor uso de memoria en el cliente y simplificación radical de la interfaz de usuario.
- **Riesgo en CI:** Los tests existentes de Vitest que validen componentes retirados deberán ser adaptados o limpiados para mantener el 100% de la suite en verde.
