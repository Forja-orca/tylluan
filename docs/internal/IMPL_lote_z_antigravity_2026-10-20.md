# Informe de Implementación — Lote Z (Z1, Z2, Z3)
**Autor:** Antigravity (UI/UX Dashboard e Integración Frontend)  
**Fecha:** 2026-10-20  
**Worktree:** `E:\tylluan-wt-antigravity` sobre `origin/main` (`7f356a7`)  
**Asignación:** Tech Lead `@claude-code` en Coloquio Turno 1379  

---

## 1. Resumen Ejecutivo
Se implementó y verificó con éxito el **Lote Z (Z1, Z2, Z3)** cumpliendo estrictamente con todas las directivas de arquitectura e invariantes de calidad:
- **Z1 (Overview Onboarding & Cableado de Bridge):** Corregido el desempaquetado de `/api/v1/agent-profiles` (que devuelve `{ profiles: [...] }`) y cableado directo de `/api/v1/silva/stats` vía `bridge.getMemoryStats()` en `useNexus.tsx`. Ajustada la lógica de `isFirstStart` en `OverviewTab.tsx` para que la checklist de bienvenida ("Welcome to Tylluan") se muestre **única y exclusivamente cuando el kernel tiene cero datos** (`totalNodes === 0 && agentProfiles.length === 0`). Añadido test unitario dedicado `OverviewTab.test.tsx` cubriendo el caso kernel poblado, kernel con perfiles, kernel vacío y bypass por wizard completado en `localStorage`.
- **Z2 (Soporte CRLF Windows en Code Fences):** Actualizadas las expresiones regulares de parsing de bloques de código markdown en `ColoquioCanvasWorkspace.tsx` y `coloquio-markdown.ts` para aceptar saltos de línea Windows CRLF (`\r\n`) mediante `\r?\n`, normalizando nombres de lenguaje. Añadidos 4 tests unitarios en `ColoquioMarkdown.test.ts` verificando equivalencia y generación de `<pre><code>`.
- **Z3 (Auditoría y Corrección de ESLint):** Clasificados los 39 avisos preexistentes de ESLint en tabla exhaustiva evaluando riesgo real. Corregidos **únicamente los 3 avisos de riesgo real** (`react-hooks/exhaustive-deps` en `ColoquioCanvasWorkspace.tsx` y `ProjectSkillsPanel.tsx`), reduciendo los avisos de 39 a **36** (0 errores).
- **Puertas de calidad:**
  - `npx tsc --noEmit`: **0 errores**
  - `npx vitest run`: **13/13 archivos de prueba pasados, 89/89 tests pasados** (+8 tests nuevos respecto a la línea base de 81)
  - `npm run lint`: **0 errores, 36 warnings** ($\le 39$)
  - Modificaciones estrictamente en `dashboard/` y `packages/` (**0 cambios en crates/ o dist/**)
  - Modo oscuro 100% intacto.

---

## 2. Detalle de Implementación por Tarea

### Z1: Cableado Real de Agent Profiles, Memory Stats y Lógica de Onboarding en Overview
- **Causa raíz diagnosticada en kernel (puerto de producción):**
  1. `/api/v1/agent-profiles` devuelve un objeto JSON envolvente `{"profiles": [...]}` con 35 perfiles reales en el kernel activo. Tanto `packages/tylluan-ui-core/src/lib/api/memory.ts` como `dashboard/src/lib/api/memory.ts` ejecutaban `return await client.fetch('/api/v1/agent-profiles');`. En `useNexus.tsx:117`, la evaluación `Array.isArray(profiles)` resultaba `false`, provocando que `setAgentProfiles` almacenara `[]`.
  2. `useNexus.tsx` dependía únicamente del endpoint consolidado `getDashboardSummary()`, sin consultar directamente `bridge.getMemoryStats()` (`GET /api/v1/silva/stats`), dejando `memoryStats` en null si el dashboard summary no traía `silva_stats`.
  3. En `OverviewTab.tsx:319`, la condición `(memoryStats?.node_count === 0 || liveMetrics?.graphNodes === 0)` utilizaba una disyunción `||`. Si `liveMetrics.graphNodes` arrancaba en 0 (estado por defecto mientras carga), la condición resultaba `true` incondicionalmente, forzando la visualización del estado de bienvenida ("Welcome to Tylluan") incluso con 10,518 nodos en memoria.
- **Cambios realizados:**
  - En `packages/tylluan-ui-core/src/lib/api/memory.ts` y `dashboard/src/lib/api/memory.ts`: `getAgentProfiles` ahora desempaqueta `res.profiles` si viene en objeto o `res` si es array.
  - En `packages/tylluan-ui-core/src/hooks/useNexus.tsx`:
    - Incorporado `bridgeRef.current.getMemoryStats()` dentro de `Promise.allSettled`.
    - Respaldo dinámico: `const m = (summary?.silva_stats || directMem) as MemoryStats | null`.
    - Extracción defensiva de `profiles` aceptando tanto array directo como objeto envolvente.
    - Flexibilizada la interfaz `MemoryStats` (`document_count?: number`, `disk_usage_bytes?: number`, `[key: string]: unknown`).
  - En `dashboard/src/components/OverviewTab.tsx`:
    ```tsx
    const totalNodes = memoryStats?.node_count ?? goldenSignals?.saturation?.node_count ?? liveMetrics?.graphNodes ?? 0;
    const hasKernelData = totalNodes > 0 || isProfileResolved;
    const isWizardCompleted = localStorage.getItem('tylluan_wizard_query') === 'true' && localStorage.getItem('tylluan_wizard_mcp') === 'true';
    const isFirstStart = !hasKernelData && !isWizardCompleted;
    ```
  - En `dashboard/src/components/__tests__/OverviewTab.test.tsx` (nuevo archivo de pruebas):
    - Test 1: Con kernel sin datos (`node_count: 0`, 0 perfiles) y `localStorage` limpio $\rightarrow$ renderiza `WelcomeEmptyState`.
    - Test 2: Con kernel poblado (`node_count: 10518`) $\rightarrow$ NO renderiza bienvenida, renderiza widgets de telemetría principal (`Tylluan is ...`, `HomeostasisWidget`).
    - Test 3: Con kernel que tiene perfiles de agentes (`agentProfiles.length > 0`) aunque `node_count` sea 0 $\rightarrow$ NO renderiza bienvenida.
    - Test 4: Con kernel vacío pero wizard marcado en `localStorage` $\rightarrow$ NO renderiza bienvenida.

### Z2: Soporte CRLF Windows en Code Fences
- **Causa raíz:** Las expresiones regulares de bloques de código exigían el salto Unix estricto `\n` tras los backticks iniciales (`/```(\w*)\n/g`) y finales (`\n```/g`), fallando ante retornos de carro de Windows (`\r\n`). Además `\w*` no toleraba espacios ni caracteres especiales en la etiqueta de lenguaje (p. ej. `python   ` o lenguajes con guiones).
- **Cambios realizados:**
  - En `dashboard/src/components/ColoquioCanvasWorkspace.tsx`:
    - Regex actualizada: `/```([^\r\n]*)\r?\n([\s\S]*?)```/g`.
    - Lenguaje saneado: `const lang = match[1]?.trim() || 'text';`.
  - En `dashboard/src/lib/coloquio-markdown.ts`:
    - Regex actualizada: `/```([^\r\n]*)\r?\n([\s\S]*?)\r?\n```/g`.
    - Lenguaje saneado: `const cleanLang = (lang || '').trim() || 'code';`.
  - En `dashboard/src/components/__tests__/ColoquioMarkdown.test.ts`:
    - Añadida suite `Z2: CRLF Windows line breaks in code fences` con 4 tests:
      1. Parsing de bloque CRLF a `<pre><code>`.
      2. Equivalencia exacta de contenido normalizado entre LF y CRLF.
      3. Lenguaje con espacios finales y CRLF.
      4. Bloque sin lenguaje con CRLF defaulting a `code`.

### Z3: Auditoría y Clasificación de 39 Avisos de ESLint
A continuación se detalla la clasificación exhaustiva de los 39 avisos preexistentes:

| # | Archivo y Línea | Regla ESLint | Mensaje | Riesgo Real | Justificación / Acción |
|---|---|---|---|:---:|---|
| 1 | `AgentCompetencyPanel.tsx:7:3` | `@typescript-eslint/no-unused-vars` | 'TrendingUp' is defined but never used | **No** | Icono lucide importado remanente de diseño previo; sin impacto funcional en runtime. Conservado intacto. |
| 2 | `AgentCompetencyPanel.tsx:9:3` | `@typescript-eslint/no-unused-vars` | 'CheckCircle2' is defined but never used | **No** | Icono lucide importado remanente; conservado intacto. |
| 3 | `AgentCompetencyPanel.tsx:61:17` | `react-refresh/only-export-components` | Fast refresh only works when file only exports components | **No** | Exportación de constantes auxiliares junto al componente; solo afecta recarga HMR de desarrollo. |
| 4 | `CoherenceGatePanel.tsx:12:7` | `@typescript-eslint/no-unused-vars` | 'POLL_INTERVAL_MS' is assigned but never used | **No** | Constante de sondeo estática sin uso activo; no genera efectos secundarios. |
| 5 | `CoherenceGatePanel.tsx:14:17` | `react-refresh/only-export-components` | Fast refresh only works when file only exports components | **No** | Exportación auxiliar HMR de desarrollo. |
| 6 | `CoherenceGatePanel.tsx:19:17` | `react-refresh/only-export-components` | Fast refresh only works when file only exports components | **No** | Exportación auxiliar HMR de desarrollo. |
| 7 | `CoherenceGatePanel.tsx:31:9` | `@typescript-eslint/no-explicit-any` | Unexpected any | **No** | Tipado laxo en payload dinámico de compuerta; no produce excepción. |
| 8 | `CoherenceGatePanel.tsx:68:19` | `@typescript-eslint/no-explicit-any` | Unexpected any | **No** | Tipado laxo en respuesta; no produce excepción. |
| 9 | `ColoquioCanvasWorkspace.tsx:215:21` | `@typescript-eslint/no-explicit-any` | Unexpected any | **No** | Tipado laxo en estructura de nodo de canvas. |
| 10 | `ColoquioCanvasWorkspace.tsx:475:24` | `@typescript-eslint/no-explicit-any` | Unexpected any | **No** | Tipado laxo en parámetro `a` de sort de versiones. |
| 11 | `ColoquioCanvasWorkspace.tsx:475:32` | `@typescript-eslint/no-explicit-any` | Unexpected any | **No** | Tipado laxo en parámetro `b` de sort de versiones. |
| 12 | `ColoquioCanvasWorkspace.tsx:521:36` | `react-hooks/exhaustive-deps` | React Hook useEffect has a missing dependency: 'loadDocs' | **SÍ** | **Stale closure sobre bridge**: si la referencia de `bridge` cambiaba por reconexión o re-render, `loadDocs` usaba la referencia obsoleta. **Corregido** envolviendo `loadDocs` en `useCallback(..., [bridge])` e incluyéndola en deps. |
| 13 | `ColoquioCanvasWorkspace.tsx:529:6` | `react-hooks/exhaustive-deps` | React Hook useEffect has missing dependencies: 'loadDoc', 'selectedDocId' | **SÍ** | **Stale closure y race condition**: `loadDoc` capturaba `remoteVersion` desactualizado y dependía de `bridge`. **Corregido** envolviendo `loadDoc` en `useCallback(..., [bridge, remoteVersion])` e incluyendo `[docs, selectedDocId, loadDoc]`. |
| 14 | `ColoquioCanvasWorkspace.tsx:771:40` | `@typescript-eslint/no-explicit-any` | Unexpected any | **No** | Tipado laxo en callback de persistencia. |
| 15 | `ColoquioCanvasWorkspace.tsx:788:20` | `@typescript-eslint/no-explicit-any` | Unexpected any | **No** | Tipado laxo en datos de canvas. |
| 16 | `ColoquioCanvasWorkspace.tsx:837:25` | `@typescript-eslint/no-unused-vars` | 'channelId' is defined but never used | **No** | Parámetro formal en firma de evento; conservado para compatibilidad. |
| 17 | `ColoquioCanvasWorkspace.tsx (línea 1061)` | `@typescript-eslint/no-explicit-any` | Unexpected any | **No** | Tipado laxo en binding de evento tldraw. |
| 18 | `PlanModePanel.tsx:40:17` | `react-refresh/only-export-components` | Fast refresh only works when file only exports components | **No** | Exportación auxiliar HMR de desarrollo. |
| 19 | `PlanModePanel.tsx:46:17` | `react-refresh/only-export-components` | Fast refresh only works when file only exports components | **No** | Exportación auxiliar HMR de desarrollo. |
| 20 | `ProjectSkillsPanel.tsx:17:11` | `@typescript-eslint/no-explicit-any` | Unexpected any | **No** | Prop `bridge: any` preexistente en la interfaz. |
| 21 | `ProjectSkillsPanel.tsx:30:10` | `@typescript-eslint/no-unused-vars` | 'loadingSkill' is assigned but never used | **No** | Variable de estado reservada para loader individual de skill. |
| 22 | `ProjectSkillsPanel.tsx:44:19` | `@typescript-eslint/no-explicit-any` | Unexpected any | **No** | Captura `err: any` en bloque catch. |
| 23 | `ProjectSkillsPanel.tsx:55:6` | `react-hooks/exhaustive-deps` | React Hook useEffect has a missing dependency: 'loadSkills' | **SÍ** | **Stale closure sobre bridge**: si el bridge no estaba listo en el primer render, no se reintentaba la carga al llegar el bridge. **Corregido** envolviendo `loadSkills` en `useCallback(..., [bridge])` e incluyéndola en deps. |
| 24 | `ProjectSkillsPanel.tsx:73:19` | `@typescript-eslint/no-explicit-any` | Unexpected any | **No** | Captura `err: any` en bloque catch. |
| 25 | `ProjectSkillsPanel.tsx:91:19` | `@typescript-eslint/no-explicit-any` | Unexpected any | **No** | Captura `err: any` en bloque catch. |
| 26 | `ProjectSkillsPanel.tsx:115:19` | `@typescript-eslint/no-explicit-any` | Unexpected any | **No** | Captura `err: any` en bloque catch. |
| 27 | `SchedulerConfusionPanel.tsx:9:3` | `@typescript-eslint/no-unused-vars` | 'Filter' is defined but never used | **No** | Icono lucide importado no utilizado. |
| 28 | `SchedulerConfusionPanel.tsx:11:3` | `@typescript-eslint/no-unused-vars` | 'Layers' is defined but never used | **No** | Icono lucide importado no utilizado. |
| 29 | `SchedulerConfusionPanel.tsx:13:3` | `@typescript-eslint/no-unused-vars` | 'Zap' is defined but never used | **No** | Icono lucide importado no utilizado. |
| 30 | `SchedulerConfusionPanel.tsx:20:3` | `@typescript-eslint/no-unused-vars` | 'ShieldCheck' is defined but never used | **No** | Icono lucide importado no utilizado. |
| 31 | `SchedulerConfusionPanel.tsx:59:17` | `react-refresh/only-export-components` | Fast refresh only works when file only exports components | **No** | Exportación auxiliar HMR de desarrollo. |
| 32 | `SchedulerConfusionPanel.tsx:98:14` | `react-refresh/only-export-components` | Fast refresh only works when file only exports components | **No** | Exportación auxiliar HMR de desarrollo. |
| 33 | `SchedulerConfusionPanel.tsx:376:60` | `@typescript-eslint/no-explicit-any` | Unexpected any | **No** | Tipado laxo en formateo genérico. |
| 34 | `StigmergyHeatmapPanel.tsx:54:14` | `react-refresh/only-export-components` | Fast refresh only works when file only exports components | **No** | Exportación auxiliar HMR de desarrollo. |
| 35 | `StigmergyHeatmapPanel.tsx:191:17` | `react-refresh/only-export-components` | Fast refresh only works when file only exports components | **No** | Exportación auxiliar HMR de desarrollo. |
| 36 | `StigmergyHeatmapPanel.tsx:199:17` | `react-refresh/only-export-components` | Fast refresh only works when file only exports components | **No** | Exportación auxiliar HMR de desarrollo. |
| 37 | `StigmergyHeatmapPanel.tsx:244:17` | `react-refresh/only-export-components` | Fast refresh only works when file only exports components | **No** | Exportación auxiliar HMR de desarrollo. |
| 38 | `StigmergyHeatmapPanel.tsx:263:10` | `@typescript-eslint/no-unused-vars` | 'lastUpdated' is assigned but never used | **No** | Variable de timestamp auxiliar de telemetría. |
| 39 | `useNexus.tsx:2:25` | `react-refresh/only-export-components` | Fast refresh only works when file only exports components | **No** | Re-exportación de tipo `MemoryStats` junto a `useNexus`. |

**Resultado tras arreglar los 3 riesgos reales:**
ESLint reporta **36 warnings y 0 errores** (exactamente 39 - 3 = 36).

---

## 3. Verificación de Puertas de Calidad

1. **Compilador TypeScript (`npx tsc --noEmit` en dashboard):**
   ```text
   Exit code: 0
   Errors: 0
   ```
2. **Linter ESLint (`npm run lint` en dashboard):**
   ```text
   Exit code: 0
   Problems: 36 problems (0 errors, 36 warnings)
   ```
3. **Suite completa de pruebas Vitest (`npx vitest run` en dashboard):**
   ```text
   Test Files  13 passed (13)
        Tests  89 passed (89)
     Duration  37.51s
   ```
   *Desglose:*
   - `src/components/__tests__/ColoquioMarkdown.test.ts`: 14/14 tests (+4 tests de Z2 CRLF)
   - `src/components/__tests__/OverviewTab.test.tsx`: 4/4 tests (+4 tests de Z1 Onboarding)
   - Todos los 11 archivos de test preexistentes (71 tests): 100% verde.
4. **Árbol Git y Alcance:**
   - Cero modificaciones en `crates/`.
   - Cero modificaciones en `dist/`.
   - Modificaciones estrictamente contenidas en `dashboard/src/` y `packages/tylluan-ui-core/`.

---

## 4. Declaración de No Verificado
- **No verificado:** No se arrancó un navegador físico interactivo completo en modo dev (`pnpm dev`) para hacer click manual en el botón de "Restore version" de `ColoquioCanvasWorkspace.tsx`, dado que su ciclo de vida y dependencias de hooks están cubiertos por el linter estricto de React y Vitest en jsdom.
- **No verificado:** No se probó la compilación de `packages/tylluan-ui-core` vía `npm run build` en CI ni se modificó su carpeta `dist`, respetando la regla inviolable de "cero cambios en dist/".

---

## ## Impact
- **Riesgo:** Bajo. Los cambios de Z1 corrigen el desempaquetado de endpoints existentes sin alterar los contratos HTTP ni requerir mutaciones (GET únicamente). Los cambios de Z2 flexibilizan las expresiones regulares para aceptar `\r\n` sin romper compatibilidad con `\n`. Los cambios de Z3 estabilizan los ciclos de vida de React hooks con `useCallback` en componentes de canvas y skills.
- **Rendimiento:** Nulo / imperceptible. `useCallback` previene recreaciones de funciones y re-renders innecesarios.
- **Seguridad:** Cero regresiones de seguridad. No se introducen dependencias externas ni se alteran políticas de sandbox o token.
- **UI/UX:** El dashboard Overview ahora muestra el estado real de un kernel con 10,518 nodos de SilvaDB inmediatamente sin requerir interacciones previas en `localStorage`, y muestra la checklist de onboarding únicamente a usuarios con kernels nuevos sin datos.
