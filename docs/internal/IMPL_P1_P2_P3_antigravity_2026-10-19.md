# Informe de Implementación P1, P2 y P3 — Antigravity (2026-10-19)

## Impact
- **Área afectada:** Frontend Dashboard (`dashboard/src/components/*`, `dashboard/src/lib/timeout.ts`).
- **Invariantes preservados:** 
  - `useNexus` intacto (0 cambios).
  - Cero modificaciones a `crates/`, `tylluan.toml` o `dashboard/dist`.
  - Sin rotura de contratos de API o comportamiento en runtime.
  - Compatibilidad de tests: 10/10 test files pasaron (62/62 tests en verde).
  - Compilación TypeScript: `npx tsc --noEmit` completado con 0 errores (código de salida 0).
  - Advertencias ESLint reducidas de 91 a 54 (objetivo era $\le 60$).

---

## 1. Contexto y Objetivos Asignados por Tech Lead (@claude-code, Turno 1325)
Tras la integración de los batches previos (E1/E2/E3 en commits `507789f`, `92b6516`, `80d7d65`), el Tech Lead asignó el siguiente paquete de tareas en `E:\tylluan-wt-antigravity`:

1. **P1 (Reducción de ESLint warnings):**
   - Reducir las 91 advertencias de ESLint restantes (principalmente `@typescript-eslint/no-explicit-any` y `@typescript-eslint/no-unused-vars`) en archivos aún no saneados.
   - Meta asignada: $\le 60$ advertencias documentadas, preservando `tsc 0` y `vitest` en verde.
2. **P2 (Unificación de idioma a inglés):**
   - Auditar paneles `Doctor`, `Maintenance` y `System` que mezclaban español e inglés.
   - Determinar idioma dominante (inglés) y unificar cadenas residuales en español, calculando recuentos antes/después con `grep`.
3. **P3 (Resiliencia ante cuelgues de red en vistas clave):**
   - Asegurar que ninguna de las nuevas vistas (`DecisionServicePanel`, `ContradictionsPanel`, `AuditTrailPanel`, `FederationTab`) deje al usuario sin salida si un endpoint tarda $> 10$ segundos.
   - Incorporar timeout visible o botón de cancelación sin bloquear la UI ni dejar temporizadores colgados en segundo plano.

---

## 2. P3: Timeouts de 10s y Controles de Cancelación en Vistas Críticas

### 2.1 Módulo `dashboard/src/lib/timeout.ts`
Se implementó la utilidad genérica `withTimeout<T>(promise, timeoutMs = 10000, errorMsg)`:
```ts
export function withTimeout<T>(
  promise: Promise<T>,
  timeoutMs = 10000,
  errorMsg = 'Request timed out after 10s'
): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | null = null;
  const timeoutPromise = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error(errorMsg)), timeoutMs);
  });

  return Promise.race([
    promise.then(res => {
      if (timer) clearTimeout(timer);
      return res;
    }),
    timeoutPromise
  ]).catch(err => {
    if (timer) clearTimeout(timer);
    throw err;
  });
}
```
- **Diseño limpio:** Limpia el timer explícitamente en el `then` y en el `catch`, evitando fugas de temporizadores en Node.js/Vitest.

### 2.2 Integración en las 4 vistas requeridas:
1. **`DecisionServicePanel.tsx`:**
   - Envuelto endpoint `/api/v1/decide/shadow` con `withTimeout(..., 10000, 'Decision service request timed out after 10s')`.
   - Agregado botón de cancelación `[Cancel]` visible cuando `loading === true`, permitiendo abortar la espera sin congelar el panel.
2. **`ContradictionsPanel.tsx`:**
   - Envuelto endpoint `/api/v1/silva/conflicted` con `withTimeout(..., 10000, 'Conflicted nodes request timed out after 10s')`.
   - Agregado botón de cancelación `[Cancel]` junto al estado de carga.
3. **`AuditTrailPanel.tsx`:**
   - Envuelto `Promise.allSettled` de logs con `withTimeout(..., 10000, 'Audit trail request timed out after 10s')`.
   - Agregado botón de cancelación `[Cancel]` con icono `X` junto a `Refresh Logs`.
4. **`FederationTab.tsx`:**
   - Envueltas llamadas `/api/v1/mesh/peers` y `/api/v1/federation/nodes` con timeout de 10s.
   - Incorporados botones de cancelación para refresco de pares de malla y nodos federados.

---

## 3. P2: Unificación de Idioma en Doctor, Maintenance y System

### 3.1 Auditoría inicial de cadenas en español (`grep -iE ...`):
- `DoctorTab.tsx`: 0 ocurrencias de cadenas en español (100% en inglés).
- `MaintenanceTab.tsx`: 0 ocurrencias de cadenas en español (100% en inglés).
- Subpaneles de `System`: Se detectaron 4 cadenas en español que rompían la consistencia visual y de depuración:
  - `ScopesPanel.tsx`: `"Error consultando nodos por ámbito:"` (1 cadena en español).
  - `A2aPanel.tsx`: `"Error al cargar agent-card.json:"` (1 cadena en español).
  - `LogsTab.tsx`: Placeholder `"Buscar en logs..."` y botón `"Limpiar"` (2 cadenas en español).

### 3.2 Correcciones aplicadas:
- `ScopesPanel.tsx`: $\to$ `"Error querying nodes by scope prefix:"`
- `A2aPanel.tsx`: $\to$ `"Error loading agent-card.json:"`
- `LogsTab.tsx`: $\to$ Placeholder `"Search logs..."`, Botón `"Clear"`

### 3.3 Verificación post-cambio:
- Cadenas en español en paneles de Doctor, Maintenance y System: **0**. 100% unificado en inglés.

---

## 4. P1: Reducción de ESLint Warnings ($\le 60$)

### 4.1 Métricas Calculadas
- **Línea base previa (`origin/main` en commit `80d7d65`):** 91 warnings, 0 errores.
- **Resultado post-intervención:** **54 warnings**, 0 errores.
- **Total de advertencias eliminadas:** **37 warnings** (reducción del 40.6%).

### 4.2 Archivos intervenidos y warnings saneados:
1. `dashboard/src/components/OutputsLedgerPanel.tsx`: **-6 warnings** (eliminados iconos huérfanos `ArrowUpDown`, `ShieldCheck`, `ChevronDown`, tipado seguro de respuestas `bridge.fetchRaw` y errores `catch (err: unknown)`).
2. `dashboard/src/components/IngestPanel.tsx`: **-6 warnings** (tipado `IngestRawNode`, `IngestParsedMeta`, safe error catching, limpieza de variables).
3. `dashboard/src/components/DispatchesPanel.tsx`: **-4 warnings** (tipado `bridge: NexusBridge | null`, tipado `catch` y eliminación de icono no utilizado).
4. `dashboard/src/components/InteroceptionTab.tsx`: **-4 warnings** (eliminación de `useCallback`, `Activity`, `Clock` no utilizados, prefijado `_memoryStats`).
5. `dashboard/src/components/ModelsEmbeddingsDisk.tsx`: **-4 warnings** (definición de `DetectedLocalModel` con `size_mb`, `ModelsConfigData`, `ModelsResponseData`, tipado seguro de errores).
6. `dashboard/src/components/ConnectorsTab.tsx`: **-3 warnings** (interfaz formal `SetupHintData`, tipado de estado de esquema e hints).
7. `dashboard/src/components/LogsTab.tsx`: **-2 warnings** (eliminación de `EVENT_COLOR_MAP` y función muerta `clearLogs`).
8. `dashboard/src/components/ModelsExternalProviders.tsx`: **-2 warnings** (tipado `testResults` con `TestResult | undefined` y safe error handling).
9. `dashboard/src/components/ModelsTab.tsx`: **-2 warnings** (tipado de configuración de modelos sin `any`).
10. `dashboard/src/components/NodesTab.tsx`: **-2 warnings** (limpieza de `ShieldCheck` no usado y tipado de `notify`).
11. `dashboard/src/components/ScopesPanel.tsx`: **-2 warnings** (`useCallback` reactivo sobre `fetchNodes`, tipado seguro de catch).

### 4.3 Distribución final de las 54 advertencias restantes:
- `src/components/ColoquioCanvasWorkspace.tsx`: 9
- `src/components/ProjectSkillsPanel.tsx`: 7
- `src/components/SchedulerConfusionPanel.tsx`: 7
- `src/components/CoherenceGatePanel.tsx`: 5
- `src/components/StigmergyHeatmapPanel.tsx`: 5
- `src/components/AgentCompetencyPanel.tsx`: 3
- `src/components/ColoquioTab.tsx`: 3
- `src/components/GuildsConsolidated.tsx`: 2
- `src/components/MemoryConsolidated.tsx`: 2
- `src/components/PlanModePanel.tsx`: 2
- `src/components/VisionTab.tsx`: 2
- `src/components/__tests__/OutputsLedgerPanel.test.tsx`: 2
- `src/components/ColoquioMessagesPanel.tsx`: 1
- `src/components/LabConsolidated.tsx`: 1
- `src/components/McpRegistryPanel.tsx`: 1
- `src/components/TeamConsolidated.tsx`: 1
- `src/hooks/useNexus.tsx`: 1 (completamente intacto según regla de asignación)

---

## 5. Verificación de Compilación y Suite de Pruebas

1. **TypeScript (`npx tsc --noEmit`):**
   ```
   Exit code: 0
   Stdout: (vacío)
   Stderr: (vacío)
   ```
2. **Vitest (`npx vitest run`):**
   ```
    Test Files  10 passed (10)
         Tests  62 passed (62)
      Start at  10:03:23
      Duration  7.68s
   ```
3. **Estado del Git Worktree (`git status --short`):**
   - 1 archivo nuevo: `dashboard/src/lib/timeout.ts`
   - 16 archivos modificados en `dashboard/src/components/`
   - Diff sin commitear, listo para revisión e integración por el Tech Lead (@claude-code).
