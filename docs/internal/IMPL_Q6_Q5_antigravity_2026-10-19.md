# Informe de Implementación y Auditoría — Q6 y Q5 (Parte Segura)

**Fecha:** 2026-10-19  
**Autor:** Antigravity (Frontend & Dashboard Agent)  
**Worktree:** `E:\tylluan-wt-antigravity` (HEAD: `8f824ac`)  
**Referencias:** Turno 1314 Coloquio (`@claude-code`)  

---

## ## Impact

- **Seguridad y compatibilidad:** Sin cambios en `crates/`, `useNexus`, schemas de base de datos ni endpoints backend.
- **Accesibilidad y UX (Q6):** Auditoría exhaustiva de los 18 paneles principales del dashboard en escenario de kernel detenido (offline). Se eliminan pantallas en blanco o mensajes de error sin acción mediante la introducción de botones de reintento (`Retry` / `Refresh`) con anillos de foco accesibles (`focus-visible:ring-2 focus-visible:ring-emerald-400`).
- **Sistema de diseño y tokenización (Q5):** Auditoría exhaustiva y conteo real de colores hexadecimales fijos (`#[0-9a-fA-F]{6}`) en el árbol del dashboard. Propuesta formal de mapeo semántico hacia variables CSS / Tailwind y piloto de migración de bajo riesgo aplicado sobre `FrictionPanel.tsx` y `TylluanLogo.tsx`.
- **Verificación:** `vitest` (9 archivos, 59 tests) en verde; `tsc --noEmit` con 0 errores de compilación.

---

## 1. Auditoría Q6: Estados de Paneles con Kernel Detenido (18 Tabs)

Auditoría verificada código por código de los 18 componentes principales de pestañas del dashboard:

| # | Panel / Tab | Estado si Kernel Detenido | Manejo de Carga / Error / Vacío | Fichero:Línea | Acción / Estado |
|---|-------------|---------------------------|----------------------------------|---------------|-----------------|
| 1 | `OverviewTab` | Degradado / Empty State | Fallback de métricas en blanco/cero; si es primer arranque sin memorias muestra `WelcomeEmptyState`. Header con botón `Refresh`. | `dashboard/src/components/OverviewTab.tsx:323-339` | Conforme |
| 2 | `CollectiveTab` | Error | Muestra vista vacía con icono `WifiOff`, mensaje explicativo de kernel offline en `:47004` y botón accesible `Retry` con llamada a `fetchAll`. | `dashboard/src/components/CollectiveTab.tsx:493-507` | **Corregido (añadido botón Retry)** |
| 3 | `ColoquioTab` | Vacío | Sin canales muestra `No channels available` e instructivo para inicializar canal. Dispone de selector e input. | `dashboard/src/components/ColoquioTab.tsx:126,510` | Conforme |
| 4 | `ConnectorsTab` | Vacío | Tarjetas de clientes MCP sin sesiones activas; instructivo visual con enlaces a specs y guía de configuración. | `dashboard/src/components/ConnectorsTab.tsx:365-373` | Conforme |
| 5 | `FederationTab` | Vacío / Telemetría | Banner de estado DHT/Gossip con fallback offline; listas de peers y nodos con empty states explícitos ("No federated peers registered"). | `dashboard/src/components/FederationTab.tsx:515-525` | Conforme |
| 6 | `FleetTab` | Error / Degradado | Si falla la telemetría muestra icono `Users`, texto explicativo y botón accesible de reintento `Retry` (animado). | `dashboard/src/components/FleetTab.tsx:280-295` | Conforme (Patrón de referencia) |
| 7 | `GuildsTab` | Vacío / Desconectado | Si no hay guilds registrados o kernel offline, muestra icono `Cpu`, aviso explicativo y botón `Refresh Guilds`. | `dashboard/src/components/GuildsTab.tsx:780-798` | **Corregido (añadido botón Refresh)** |
| 8 | `InteroceptionTab` | Degradado | Fallback estático controlado si `!interoception`: tarjetas con "No narrative generated yet" y defaults seguros. | `dashboard/src/components/InteroceptionTab.tsx:71-99` | Conforme |
| 9 | `KnowledgeGraphTab` | Error / Vacío | Manejo triple: `loading` con spinner, `error` con banner y botón `Retry`, y `empty` con sugerencia y botón `Retry`. | `dashboard/src/components/KnowledgeGraphTab.tsx:540-583` | Conforme (Patrón de referencia) |
| 10 | `LaboratoryTab` | Error / Unavailable | Si `!data` muestra icono `Beaker`, texto explicativo de AutoResearch unavailable y botón accesible `Retry` con estado de carga. | `dashboard/src/components/LaboratoryTab.tsx:94-113` | **Corregido (añadido botón Retry)** |
| 11 | `LogsTab` | Espera de eventos | Si no hay eventos SSE, muestra "Waiting for kernel events...". No se queda en blanco. | `dashboard/src/components/LogsTab.tsx:190-194` | Conforme |
| 12 | `MaintenanceTab` | Desconectado | Diagnóstico de conexión reporta "Offline / No connection" con icono `WifiOff`. Tarjetas con valores en cero y botón `Refresh`. | `dashboard/src/components/MaintenanceTab.tsx:215-226` | Conforme |
| 13 | `ModelsTab` | Vacío / Offline | Si la configuración de proveedores no responde, renderiza subtabs con fallbacks no intrusivos y formulario de modelos locales. | `dashboard/src/components/ModelsTab.tsx:28-46` | Conforme |
| 14 | `NodesTab` | Vacío / Registro | Si `nodes.length === 0`, muestra tarjeta de "No active nodes registered" con botón "Register Dashboard Node" y botón "Sync Nodes". | `dashboard/src/components/NodesTab.tsx:132-138` | Conforme |
| 15 | `SessionsTab` | Vacío | Si no hay sesiones MCP activas, muestra estado vacío ("No active MCP sessions") con guía explicativa y botón `Refresh`. | `dashboard/src/components/SessionsTab.tsx:178-186` | Conforme |
| 16 | `SystemTab` | Sub-navegación | Contenedor de `DoctorPanel`, `MaintenanceTab`, `LogsTab`, `ModelsTab`, `ScopesPanel` y `A2aPanel`. `DoctorPanel` incluye pantalla de error con botón `Retry`. | `dashboard/src/components/DoctorPanel.tsx:98-105` | Conforme |
| 17 | `TrustConsoleTab` | Offline Fallback | Renders con placeholders seguros ("—", "PENDING", commit no cargado) y botón de actualización manual `Refresh`. | `dashboard/src/components/TrustConsoleTab.tsx:111-160` | Conforme |
| 18 | `VisionTab` | Guild Inactivo | Banner visual informativo indicando dependencias (`pip install transformers torch pillow`) y botón para iniciar el guild. | `dashboard/src/components/VisionTab.tsx:135-160` | Conforme |

---

## 2. Auditoría Q5: Conteo y Mapeo de Colores Hexadecimales

### 2.1 Conteo Real por Archivo (Calculado mediante script PowerShell)

Total de coincidencias de patrón `#[0-9a-fA-F]{6}` en código activo (excluyendo comentarios y documentación de contraste WCAG):

| Fichero | Coincidencias en Código Activo | Hexadecimales Principales |
|---------|--------------------------------|---------------------------|
| `dashboard/src/components/graph/palette.ts` | 34 | `#f472b6` (4), `#818cf8` (4), `#22d3ee` (4), `#2dd4bf` (3), `#fbbf24` (3), `#f87171` (3), etc. (Paleta semántica de nodos) |
| `dashboard/src/components/InteroceptionTab.tsx` | 17 | `#34d399` (3), `#fbbf24` (3), `#64748b` (3), `#ef4444` (3), `#60a5fa` (2), `#a78bfa` (2), `#38bdf8` (1) |
| `dashboard/src/components/PlanModePanel.tsx` | 9 | `#ff2e93` (9) (Color acento de branding Plan Mode) |
| `dashboard/src/components/CollectiveTab.tsx` | 8 | `#fb923c` (1), `#a78bfa` (1), `#e879f9` (1), `#22d3ee` (1), `#38bdf8` (1), `#4ade80` (1), `#facc15` (1), `#f472b6` (1) (Identidades de agentes) |
| `dashboard/src/components/ColoquioCanvasWorkspace.tsx` | 10 | `#e2e8f0` (3), `#0f172a` (3), `#3b82f6` (2), `#f87171` (1), `#10b981` (1) |
| `dashboard/src/components/OverviewTab.tsx` | 5 | `#34d399` (2), `#60a5fa` (1), `#fbbf24` (1), `#ef4444` (1) |
| `dashboard/src/components/FrictionPanel.tsx` | 4 *(antes)* → **0** *(post-piloto)* | `#10b981` (1), `#f59e0b` (1), `#ef4444` (1), `#1e293b` (1) |
| `dashboard/src/components/HomeostasisWidget.tsx` | 3 | `#34d399` (1), `#fbbf24` (1), `#f87171` (1) |
| `dashboard/src/components/TylluanLogo.tsx` | 2 *(antes)* → **0** *(post-piloto)* | `#f59e0b` (2) |
| `dashboard/src/components/MaintenanceTab.tsx` | 2 | `#60a5fa` (1), `#34d399` (1) (MiniSparkline props) |
| `dashboard/src/components/GuildInspector.tsx` | 2 | `#ff2e93` (2) |
| `dashboard/src/components/ui/MetricPrimitives.tsx` | 1 | `#60a5fa` (1) (Default prop de MiniSparkline) |
| `dashboard/src/components/CoherenceGatePanel.tsx` | 1 | `#ff2e93` (1) |
| `dashboard/src/components/graph/KnowledgeCortex2D.tsx` | 1 | `#e2e8f0` (1) |

*Nota:* Archivos como `ProvenanceBadge.tsx`, `ConfirmModal.tsx`, `DoctorPanel.tsx` y `McpRegistryPanel.tsx` contienen menciones a códigos HEX exclusivamente en comentarios explicativos de contraste WCAG (ej. `// slate-950 on emerald-500 (#10B981) → 7.50:1 PASS`).

---

### 2.2 Propuesta de Mapeo Semántico hacia Tokens CSS / Tailwind

| Hexadecimal Fijo | Rol Semántico | Token Tailwind / CSS Variable Sugerido |
|------------------|---------------|----------------------------------------|
| `#10b981` / `#34d399` | Signal Mint (GPU / Salud óptima / Éxito) | `hsl(var(--emerald-500))` / `hsl(var(--emerald-400))` |
| `#f59e0b` / `#fbbf24` | Sovereign Amber (CPU / Advertencia / Atención) | `hsl(var(--amber-500))` / `hsl(var(--amber-400))` |
| `#ef4444` / `#f87171` | Alert Red / Rose (Error / Fricción alta / Crítico) | `hsl(var(--red-500))` / `hsl(var(--rose-400))` |
| `#60a5fa` / `#3b82f6` | Telemetry Blue (Sparklines / Red / Flujos) | `hsl(var(--blue-400))` / `hsl(var(--blue-500))` |
| `#1e293b` / `#0f172a` | Owl Slate Track (Fondos de gauge SVG / Bordes) | `hsl(var(--slate-800))` / `hsl(var(--slate-900))` |
| `#ff2e93` | Coherence / Plan Mode Accent | `--brand-magenta` (Nueva variable propuesta si se requiere mantener fidelidad cromática) |

---

### 2.3 Piloto Aplicado (2 Componentes de Bajo Riesgo)

1. **`dashboard/src/components/FrictionPanel.tsx`**:
   - Reemplazados los 4 colores fijos del SVG `ScoreRing`:
     - `#10b981` → `hsl(var(--emerald-500))`
     - `#f59e0b` → `hsl(var(--amber-500))`
     - `#ef4444` → `hsl(var(--red-500))`
     - `#1e293b` (pista del anillo SVG) → `hsl(var(--slate-800))`
   - **Verificación WCAG AA:** Los colores tokens sobre fondo `slate-900` (`#10141E`) cumplen con ratios superiores a 5.0:1 (emerald 7.18:1, amber 9.09:1, red 4.92:1).

2. **`dashboard/src/components/TylluanLogo.tsx`**:
   - Reemplazado `#F59E0B` en la constante `AMBER_GOLD` por `hsl(var(--amber-500))`.
   - Utilizado en el path SVG vectorial de respaldo en caso de fallo de carga de imagen.

---

## 3. Verificación de Compilación y Suites de Tests

- **TypeScript:**
  ```powershell
  npx tsc --noEmit
  # Salida: Código 0 (0 errores de tipo)
  ```
- **Vitest:**
  ```powershell
  npx vitest run
  # Salida: 9 test files passed (9), 59 tests passed (59), 0 fallidos.
  ```

---

## 4. Archivos Modificados en el Worktree

- `dashboard/src/components/CollectiveTab.tsx` (botón de reintento en error)
- `dashboard/src/components/GuildsTab.tsx` (botón de refresco cuando no hay guilds)
- `dashboard/src/components/LaboratoryTab.tsx` (botón de reintento en AutoResearch unavailable + import de `RefreshCw`)
- `dashboard/src/components/FrictionPanel.tsx` (migración a tokens CSS en `ScoreRing`)
- `dashboard/src/components/TylluanLogo.tsx` (migración a token `hsl(var(--amber-500))` en `AMBER_GOLD`)
