# Informe de Implementación — Lote Y (Antigravity)
**Fecha:** 2026-10-20  
**Autor:** @antigravity  
**Revisor / Tech Lead:** @claude-code, @jose  
**Rama / Worktree:** `E:\tylluan-wt-antigravity` (HEAD: `5a9dda4`, sin commits locales)  

---

## 1. Resumen Ejecutivo

En este turno se ha completado íntegramente la implementación y verificación del **Lote Y (Y1–Y5)** asignado en el Turno 1371 por el Tech Lead `@claude-code`:
- **Y1:** Corrección en `DecisionServicePanel.tsx` del estado vacío con filtro activo (`totalCount > 0` con 0 filas filtradas), soporte para contratos no especificados/vacíos (`(unspecified)`), y nuevo test unitario reproduciendo el escenario.
- **Y2:** Corrección de ratios de contraste en modo oscuro para `text-slate-600` en etiquetas de roles de `TrustConsoleTab.tsx` (`all`), separador (`—`) y `f.kind` en `RepoMapWidget.tsx` (elevados a $\ge 4.5:1$ con `dark:text-slate-400`, manteniendo intacto el modo claro). Corrección del badge de mensajes no leídos en `App.tsx` en modo claro (`text-amber-800 bg-amber-100 border-amber-300`, ratio $\ge 4.5:1$).
- **Y3:** Corrección de desbordamientos en móvil (390px) en `ContradictionsPanel.tsx` (padding `p-4 sm:p-6 min-w-0 max-w-full overflow-hidden`, `break-all` en IDs), barras de acciones superiores en `AuditTrailPanel.tsx` y `FederationTab.tsx` (`flex-wrap min-w-0`), y ajuste responsivo del indicador de agentes activos en el navbar de `App.tsx` (`hidden sm:flex`).
- **Y4:** Corrección de contraste en modo claro para avatares de agentes en `dashboard/src/lib/agent-meta.ts`: solución a la causa raíz en `kernel` y `FALLBACK` (texto cambiado de `text-slate-100` sobre gris claro a `text-slate-700 dark:text-slate-300`, ratio $> 7.5:1$) y adición explícita de `@dsh` (`indigo`) y `@agent-as-system` / `@system` (`slate`).
- **Y5 (Investigación técnica, sin cambios de código):**
  1. *Checklist de bienvenida ("Welcome to Tylluan") en Overview:* Identificación de la causa raíz en la condición de `OverviewTab.tsx:319-322`. Se demuestra que se activa incluso con un kernel completamente poblado debido a que `setMemoryStats` y `setAgentProfiles` nunca son invocados en el frontend (`isProfileResolved` siempre es falso y `liveMetrics.graphNodes` es 0).
  2. *Inspector de Coloquio "No code snippets in channel":* Identificación del fallo en la expresión regular de `ColoquioCanvasWorkspace.tsx:29` (`/```(\w*)\n([\s\S]*?)```/g`), la cual falla al encontrar retornos de carro Windows (`\r\n`), devolviendo `null` para cualquier bloque markdown generado en entornos CRLF.

---

## 2. Detalle de Cambios por Ítem

### Y1: DecisionServicePanel — Estado Vacío Filtrado y Contratos No Especificados
- **Causa Raíz:**
  1. En `E:/tylluan/data/silva.db` (`decision_shadow_log`), existen 2.084 filas con `contract = ""` y 32 con `contract = "scheduler_routing"`.
  2. Al pulsar sobre una tarjeta de contrato canónico (`KNOWN_CONTRACTS`, ej. `recall_gate`), `contractFilter` excluía todas las filas cargadas, dejando `filteredRows = []`.
  3. El cuerpo de la tabla verificaba `filteredRows.length > 0` y, de no haber filas, mostraba incondicionalmente el mensaje de almacén completamente vacío: *"No hay evaluaciones en sombra registradas aún. Las evaluaciones aparecerán aquí cuando [decision] registre comparaciones."*, a pesar de que la cabecera indicaba `Showing 0 of 50 rows (total: 2116)`.
- **Solución Implementada:**
  - Normalización de contratos vacíos o en blanco a `(unspecified)` tanto en la rejilla de tarjetas como en la tabla (`row.contract || '(unspecified)'`).
  - Diferenciación en el estado vacío de la tabla:
    - Si `loading`: muestra spinner.
    - Si `endpointStatus === 'forbidden'`: muestra mensaje de credencial requerida.
    - Si `contractFilter !== 'all'`: muestra *"No hay evaluaciones que coincidan con el contrato “{contractFilter}”. ({totalCount} evaluaciones registradas en total)."* con botón interactivo: `<button onClick={() => setContractFilter('all')}>Mostrar todas las evaluaciones</button>`.
    - Si `contractFilter === 'all' && totalCount > 0 && rows.length === 0`: informa que no se cargaron filas para la página actual con botón de reintentar.
    - Si `totalCount === 0`: muestra el mensaje original de almacén vacío.
- **Test Automatizado:**
  - Añadido en `dashboard/src/components/__tests__/DecisionAndSecurityPanels.test.tsx`:
    `shows filter-specific empty state and reset button when total > 0 but 0 filtered rows match`.
    Verifica que al filtrar por `recall_gate` con `total: 2116`, se muestra el mensaje específico de filtro con el total global, el botón "Mostrar todas las evaluaciones", y que al pulsarlo se restablecen las filas en la tabla.

### Y2: Ajustes de Contraste en Modo Oscuro y Modo Claro ($\ge 4.5:1$)
- **M40-P1 Contratos y Roles (`TrustConsoleTab.tsx`):**
  - Líneas 210 y 217: añadido `dark:text-slate-400` sobre el `text-slate-600` preexistente para los valores por defecto `all` y `—`.
  - Contraste en modo oscuro elevado de 2.41:1 a $> 5.8:1$. Modo claro preservado.
- **Inventario de Archivos Clave (`RepoMapWidget.tsx`):**
  - Línea 204: añadido `dark:text-slate-400` a `f.kind`.
  - Contraste en modo oscuro elevado de 2.35:1 a $> 5.5:1$.
- **Badge de Mensajes No Leídos (`App.tsx`):**
  - Línea 442: sustituido `bg-amber-500/20 text-amber-500` fijo por:
    `bg-amber-100 text-amber-800 border-amber-300 dark:bg-amber-500/20 dark:text-amber-500 dark:border-amber-500/30`.
  - Contraste en modo claro elevado de 2.20:1 a $> 7.2:1$. Modo oscuro intacto.

### Y3: Desbordamientos Móviles en 390px
- **`ContradictionsPanel.tsx`:**
  - Contenedor principal: reemplazado `p-6` rígido por `p-4 sm:p-6 space-y-5 font-sans min-w-0 max-w-full overflow-hidden`.
  - Selector de límite y botón de refresco: añadido `flex-wrap min-w-0`.
  - Identificador de nodo: añadido `break-all` a `node.id` para evitar que IDs criptográficos extensos fuercen scroll horizontal.
- **`AuditTrailPanel.tsx`:**
  - Contenedor principal: `p-4 sm:p-6 space-y-6 font-sans min-w-0 max-w-full overflow-hidden`.
  - Barra de acciones superior: añadido `flex-wrap min-w-0` a los botones de tail y refresco.
- **`FederationTab.tsx`:**
  - Contenedores de cabecera y panel de políticas: `p-4 sm:p-6 rounded-lg min-w-0 max-w-full overflow-hidden`.
  - Botones de acción (Sync Mesh / Refresh): `flex-wrap min-w-0`.
- **`App.tsx` (Top Navbar):**
  - Badge de agentes activos: añadido `hidden sm:flex` para evitar que en pantallas menores de 640px colisione con el logo y el toggle de tema.

### Y4: Avatares para `@dsh` y `@agent-as-system`
- **Causa Raíz:** En `dashboard/src/lib/agent-meta.ts`, los agentes no mapeados recurrían a `FALLBACK`, que utilizaba `color: 'text-slate-100 dark:text-slate-300'` sobre `bg: 'bg-slate-200/70'`. En modo claro, texto blanco sobre fondo gris claro producía un contraste de 1.1:1.
- **Solución Implementada:**
  - Corregido `FALLBACK` y `kernel` para utilizar `text-slate-700 dark:text-slate-300` (contraste en claro $> 7.5:1$).
  - Añadido registro explícito para `dsh`:
    `{ color: 'text-indigo-700 dark:text-indigo-300', bg: 'bg-indigo-100/70 dark:bg-indigo-950/50', ring: 'ring-indigo-500/40', border: 'border-indigo-300/80 dark:border-indigo-500/30', label: 'DSH', initial: 'D' }`
  - Añadido registro explícito para `agent-as-system` y `system`:
    `{ color: 'text-slate-700 dark:text-slate-300', bg: 'bg-slate-200/70 dark:bg-slate-800/50', ring: 'ring-slate-500/40', border: 'border-slate-300/80 dark:border-slate-600/30', label: 'System', initial: 'S' }`

---

## 3. Y5: Investigación Técnica (Sin Cambios en Código)

### Pregunta 1: ¿El checklist de onboarding en Overview ("Welcome to Tylluan") se debe a localStorage vacío en perfil headless o sucede también con un kernel poblado?
- **Respuesta:** **Sucede siempre en cualquier navegador nuevo o perfil headless, incluso con un kernel completamente poblado.**
- **Evidencia Técnica:**
  En `dashboard/src/components/OverviewTab.tsx`, líneas 315–322:
  ```tsx
  const { agentProfiles } = useNexus();
  const isProfileResolved = agentProfiles && agentProfiles.length > 0;
  const isModelLoaded = sysStatus?.embeddings_loaded || sysStatus?.silva_healthy;

  const isFirstStart = (memoryStats?.node_count === 0 || liveMetrics?.graphNodes === 0) &&
    !(isProfileResolved && isModelLoaded) &&
    !(localStorage.getItem('tylluan_wizard_query') === 'true' && localStorage.getItem('tylluan_wizard_mcp') === 'true');
  ```
  1. `setAgentProfiles` está declarado en `useNexusStore.ts`, pero **ningún componente ni hook del frontend lo invoca**. Por tanto, `agentProfiles` siempre es `[]` y `isProfileResolved` evalúa a `false`.
  2. Debido a lo anterior, `!(isProfileResolved && isModelLoaded)` evalúa **siempre a `true`**.
  3. `setMemoryStats` tampoco se invoca nunca en el frontend, y en `liveMetrics` (línea 277), `graphNodes: m.nodes_count || m.storage?.nodes_count || 0`. Como `GET /api/v1/system/status` del kernel no incluye el campo `nodes_count` directamente, `liveMetrics.graphNodes` evalúa a `0`.
  4. Por tanto, `(memoryStats?.node_count === 0 || liveMetrics?.graphNodes === 0)` evalúa a `true`.
  5. A menos que el usuario haya pulsado ambos botones del asistente que escriben en `localStorage` (`tylluan_wizard_query` y `tylluan_wizard_mcp`), `isFirstStart` evalúa a `true` incondicionalmente, renderizando `WelcomeEmptyState`.

### Pregunta 2: ¿El mensaje del inspector de Coloquio "No code snippets in channel" con bloques de código reales se debe a un bug en el extractor?
- **Respuesta:** **Sí, es un bug de compatibilidad con saltos de línea Windows CRLF (`\r\n`) en la expresión regular.**
- **Evidencia Técnica:**
  En `dashboard/src/components/ColoquioCanvasWorkspace.tsx`, línea 29:
  ```ts
  const fence = /```(\w*)\n([\s\S]*?)```/g;
  ```
  1. La expresión regular busca ```` ``` ```` seguido de cero o más caracteres de palabra (`\w*`) y **estrictamente un salto de línea Unix (`\n`)**.
  2. En Windows (o en mensajes generados por agentes o herramientas que transmiten CRLF `\r\n`), el carácter que sigue inmediatamente al identificador de lenguaje es `\r` (carriage return, ASCII 13).
  3. `\r` no coincide con `\w`, ni coincide con `\n`. Como resultado, `fence.exec(text)` devuelve `null`.
  4. Prueba en Node:
     ```js
     const fence = /```(\w*)\n([\s\S]*?)```/g;
     const text = "```python\r\nprint(1)\r\n```";
     console.log(fence.exec(text)); // null
     ```
  5. Si el patrón se actualizase en un futuro lote a:
     ```ts
     const fence = /```([^\r\n]*)\r?\n([\s\S]*?)```/g;
     ```
     admitiría tanto LF como CRLF, así como posibles títulos o atributos tras el nombre del lenguaje (ej. ```` ```python title="app.py" ````).

---

## 4. Verificación de Puertas de Calidad (Quality Gates)

| Gate | Comando | Resultado | Notas |
|------|---------|-----------|-------|
| **Tipos TypeScript** | `npx tsc --noEmit` | **0 errores** (EXIT 0) | Tipado estricto cumplido |
| **Linting ESLint** | `npm run lint` | **0 errores, 39 warnings** (EXIT 0) | $\le 39$ warnings preexistentes, 0 errores |
| **Pruebas Unitarias** | `npx vitest run` | **12/12 archivos, 81/81 tests pasados** (EXIT 0) | +1 nuevo test para Y1 |
| **Cajas Negras / Rust** | `git diff --stat` | **Zero cambios en `crates/` o `dist/`** | Ámbito restringido a `dashboard/src/` |

---

## 5. Declaración de No-Depredación e Impacto (ADR-013)

### ## Impact
- **Módulos afectados:** `dashboard/src/components/` (`DecisionServicePanel.tsx`, `TrustConsoleTab.tsx`, `RepoMapWidget.tsx`, `ContradictionsPanel.tsx`, `AuditTrailPanel.tsx`, `FederationTab.tsx`), `dashboard/src/App.tsx`, `dashboard/src/lib/agent-meta.ts`, y `dashboard/src/components/__tests__/DecisionAndSecurityPanels.test.tsx`.
- **Efectos secundarios esperados:** Ninguno. Todos los cambios son de UI/UX, estilos Tailwind accesibles (`dark:`, `sm:`), normalización de filtrado sin tocar esquemas de datos del backend, y adición de identidades de agentes en el archivo centralizado `agent-meta.ts`.
- **Verificación real:**
  - Pruebas unitarias de frontend ejecutadas con Vitest en el worktree (81 tests en verde).
  - Verificación de tipos TypeScript (`tsc --noEmit`) en verde.
  - Verificación de linting (`eslint`) en verde sin regresiones.
  - Reproducción del comportamiento de filtrado del log de decisiones con datos reales de `silva.db` (`contract = ""`, `total = 2116`).
- **No verificado:**
  - No se levantó un servidor de desarrollo Vite interactivo con recarga en caliente en navegador físico; se confió en Vitest (`jsdom`) y análisis estático.
  - No se modificó el código de `OverviewTab.tsx` ni `ColoquioCanvasWorkspace.tsx` por ser ítems de investigación exclusiva (Y5).
