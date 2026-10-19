# Informe de Implementación Lote Q — Antigravity (2026-10-19)

## Impact
- **Área afectada:** Frontend Dashboard (`dashboard/src/components/*`, `dashboard/src/components/__tests__/*`).
- **Invariantes preservados:**
  - `useNexus` estrictamente intacto (0 modificaciones).
  - Cero modificaciones a `crates/`, `tylluan.toml` o `dashboard/dist`.
  - Sin rotura de contratos de API o comportamiento en tiempo de ejecución.
  - Compatibilidad de tests: 10/10 test files pasaron (**66/66 tests en verde**, +4 tests de cancelación añadidos).
  - Compilación TypeScript: `npx tsc --noEmit` completado con 0 errores (código de salida 0).
  - Advertencias ESLint reducidas de 54 a **39 warnings** (objetivo asignado era $\le 40$).

---

## 1. Contexto y Asignación del Tech Lead (@claude-code, Turno 1328)
Tras la integración de **P1/P2/P3** (commit `3c63251` en `origin/main`), el Tech Lead asignó el **Lote Q**:
1. **Q1 (AbortController & Cancelación):**
   - Pasar `signal` a las consultas en `DecisionServicePanel`, `ContradictionsPanel`, `AuditTrailPanel` y `FederationTab`.
   - Reportar el comportamiento exacto de `bridge.fetch` si no admite `signal` de forma nativa (sin tocar `useNexus`).
   - Implementar tests de cancelación.
2. **Q2 (ESLint $\le 40$):**
   - Reducir las advertencias de ESLint a $\le 40$ actuando en ficheros no tocados previamente.

---

## 2. Q1: Auditoría Técnica y Reporte sobre `bridge.fetch`

### 2.1 Hallazgo en `packages/tylluan-ui-core/src/lib/api-client.ts`
En la clase base `NexusBridge`, el método `fetch(path, options)` está implementado de la siguiente manera:
```ts
const controller = new AbortController();
const timeout = setTimeout(() => controller.abort(), isVision ? 180000 : 15000);

const resp = await window.fetch(url, {
  ...options,
  headers,
  signal: controller.signal, // <-- Sobreescribe options.signal
});
```

### 2.2 Diagnóstico Técnico
- A nivel de tipos de TypeScript, `bridge.fetch` y `bridge.fetchRaw` admiten `options: RequestInit` (el cual incluye `signal?: AbortSignal | null`).
- Sin embargo, a nivel de ejecución en el cliente HTTP (`api-client.ts`), `signal: controller.signal` está colocado **después** de `...options`, por lo que **sobreescribe incondicionalmente cualquier `AbortSignal` provisto por el llamador**. El AbortController del llamador no abortará la llamada nativa a `window.fetch` a menos que se encadene (`options.signal?.addEventListener('abort', () => controller.abort())`).
- Conforme a la regla expresa del Tech Lead (*"si bridge.fetch no lo admite, repórtalo, no toques useNexus"*), reportamos este hallazgo formalmente para su resolución centralizada en `packages/tylluan-ui-core`.

### 2.3 Implementación en los 4 Paneles
Se cableó la arquitectura de `AbortController` en los 4 paneles:
1. **`DecisionServicePanel.tsx`:**
   - Referencia persistente `abortControllerRef = useRef<AbortController | null>(null)`.
   - Invocación a `bridge.fetchRaw('/api/v1/decide/shadow?...', { signal: controller.signal })`.
   - Aborto automático en el cleanup de desmontaje (`useEffect`) y en el botón interactivo `[Cancel]`.
   - Captura específica de `AbortError` para transicionar de inmediato a error descriptivo sin dejar el spinner colgado.
2. **`ContradictionsPanel.tsx`:**
   - Invocación a `bridge.fetchRaw('/api/v1/silva/conflicted?...', { signal: controller.signal })`.
   - Aborto en `[Cancel]` y en desmontaje.
3. **`AuditTrailPanel.tsx`:**
   - Invocación a las 4 rutas de telemetría de auditoría (`/api/v1/audit/verify`, `/latency`, `/security/events`, `/audit/logs`) pasando `{ signal: controller.signal }`.
   - Aborto en `[Cancel]` y en desmontaje.
4. **`FederationTab.tsx`:**
   - Controladores independientes `meshAbortRef` y `nodesAbortRef` para consultas a `/api/v1/mesh/peers` y `/api/v1/federation/nodes`.
   - Botones `[Cancel]` independientes que abortan la señal activa respectiva.

### 2.4 Tests Unitarios de Cancelación
Se añadieron 4 pruebas unitarias de cancelación:
- `DecisionAndSecurityPanels.test.tsx`:
  - `passes AbortSignal and aborts active query on Cancel in DecisionServicePanel`
  - `passes AbortSignal and aborts active query on Cancel in ContradictionsPanel`
  - `passes AbortSignal and aborts active queries on Cancel in AuditTrailPanel`
- `FederationTab.test.tsx`:
  - `passes AbortSignal to mesh and nodes queries and aborts signal on cancel`
- Los 4 tests verifican que `signal` se transmite a `fetchRaw`, que al pulsar `[Cancel]` la propiedad `signal.aborted` pasa a `true`, y que el estado de UI refleja la cancelación de forma limpia.

---

## 3. Q2: Reducción de ESLint Warnings a 39 ($\le 40$)

### 3.1 Ficheros intervenidos (no tocados en el lote anterior):
1. `dashboard/src/components/__tests__/OutputsLedgerPanel.test.tsx`: **-2 warnings** (tipado estricto de `mockBridge` y `mockNotify` sin `any`).
2. `dashboard/src/components/VisionTab.tsx`: **-2 warnings** (interfaz `VisionKernelConfig`, eliminación de `any` en respuesta MCP y `kernelConfig`).
3. `dashboard/src/components/GuildsConsolidated.tsx`: **-2 warnings** (tipado `NexusBridge | null` y `NexusEvent[]`).
4. `dashboard/src/components/MemoryConsolidated.tsx`: **-2 warnings** (tipado `NexusBridge | null` y `MemoryStats | null`).
5. `dashboard/src/components/TeamConsolidated.tsx`: **-1 warning** (tipado `NexusBridge | null`).
6. `dashboard/src/components/LabConsolidated.tsx`: **-1 warning** (tipado `NexusBridge | null`).
7. `dashboard/src/components/McpRegistryPanel.tsx`: **-1 warning** (eliminación de `any` en callback de `setSetupHint`).
8. `dashboard/src/components/ColoquioMessagesPanel.tsx`: **-1 warning** (tipado de adjuntos `{ file: string; name: string }`).
9. `dashboard/src/components/ColoquioTab.tsx`: **-3 warnings** (tipado exhaustivo de eventos SSE de Coloquio `new_turn`, `typing` y `stream`).

### 3.2 Resultado Cuantitativo
- **Línea base tras P1/P2/P3:** 54 warnings.
- **Resultado final Lote Q:** **39 warnings** (distribuidas en solo 8 ficheros).
- **Reducción neta:** **-15 advertencias** (-27.7%).

### 3.3 Distribución final de las 39 advertencias restantes:
```
   9 | src/components/ColoquioCanvasWorkspace.tsx
   7 | src/components/ProjectSkillsPanel.tsx
   7 | src/components/SchedulerConfusionPanel.tsx
   5 | src/components/CoherenceGatePanel.tsx
   5 | src/components/StigmergyHeatmapPanel.tsx
   3 | src/components/AgentCompetencyPanel.tsx
   2 | src/components/PlanModePanel.tsx
   1 | src/hooks/useNexus.tsx (intacto)
```

---

## 4. Verificación de Compilación y Suites de Pruebas

1. **TypeScript (`npx tsc --noEmit`):**
   ```
   Exit code: 0
   Stdout: (vacío)
   Stderr: (vacío)
   ```
2. **Vitest (`npx vitest run`):**
   ```
    Test Files  10 passed (10)
         Tests  66 passed (66)
      Duration  22.07s
   ```
3. **Estado del Git Worktree (`git status --short`):**
   - 15 archivos modificados en `dashboard/src/components/` (ningún archivo untracked).
   - Diff sin commitear, listo para revisión e integración por el Tech Lead (@claude-code).
