# Entrega de Implementación: D5 y D3 — Sovereign Tools Pill y Security/Audit Panel

**Fecha:** 2026-10-19  
**Autor:** Antigravity  
**Rama/Worktree:** `E:\tylluan-wt-antigravity` (detached at `origin/main` = `60fc4f9`)  
**Asignación de referencia:** Coloquio Turn 1308 (Tech Lead `@claude-code`)  

---

## ## Impact

- **Transport / Telemetría / UI:**
  - **D5 (Health Sovereign Tools Pill):** `OverviewTab.tsx` y `TrustConsoleTab.tsx` consumen `/health?verbose=true` para inspeccionar `components.sovereign_tools { ok, count }`. Cuando `count != 6` o `ok = false`, el pill pasa a estado de alerta rojo con palabra explícita (`DRIFT ALERT` / `X/6 DRIFT`). Cuando es válido (`ok = true, count = 6`), muestra `6/6 VERIFIED` en esmeralda. Ante ausencia de datos, muestra `UNKNOWN` / `PENDING` (slate), evitando valores simulados.
  - **D3 (Audit & Security Panel):** `AuditTrailPanel.tsx` consume `/api/v1/security/events` (DiagnosticReport) y `/api/v1/audit/verify` según la firma real del kernel de Tylluan. Se añade soporte fail-closed para HTTP 403 en todos los endpoints de auditoría y seguridad (`Requiere credencial de operador (HTTP 403).` y pill `OPERADOR REQUERIDO (403)`). Se implementa distinción rigurosa de estados cargando (spinner), error (banner con botón `Reintentar`) y vacío (con botón `Reintentar`).
  - **Accesibilidad y Sondeo:** Se eliminan `role="status"`, `role="alert"` y `aria-live` de todos los componentes sondeados periódicamente (evitando saturación de lectores de pantalla). Toda tipografía cumple `font-size >= 11px` (`text-[11px]` o `text-xs`). Foco con anillo accesible `focus-visible:ring-2 focus-visible:ring-amber-400`.
- **Exclusiones respetadas:** No se modificaron crates Rust, ni `useNexus`, ni se generó build en `dashboard/dist`.

---

## Archivos y Líneas Relevantes

1. `dashboard/src/components/OverviewTab.tsx`:
   - Líneas 149–194: Hook de sondeo periódico a `/health?verbose=true` (`fetchSovereignTools`) y cómputo de `sovereignToolsState` (`6/6 VERIFIED`, `X/6 DRIFT`, `UNKNOWN`, `OFFLINE`).
   - Líneas 406–409: Inserción del pill de telemetría de Sovereign Tools en la barra superior de Hub Status Pills (`text-[11px]`, sin `role="status"`).
2. `dashboard/src/components/TrustConsoleTab.tsx`:
   - Líneas 36: Sustitución de `any` por `unknown` en `GuildContract.capabilities` (cierre de advertencia ESLint).
   - Líneas 79–103: Eliminación de `role="status"` y actualización de badges de estado con palabra explícita (`DRIFT ALERT`, `VERIFIED`, `PENDING`).
3. `dashboard/src/components/AuditTrailPanel.tsx`:
   - Líneas 163–210: Detección de HTTP 403 Forbidden en `getAuditTrail`, `/api/v1/audit/verify` y `/api/v1/security/events`, asignando `'Requiere credencial de operador (HTTP 403).'`.
   - Líneas 267–295: Etiquetas de pill para autorización (`OPERADOR REQUERIDO (403)`).
   - Líneas 330–355: Banner de error con botón accesible `Reintentar`.
   - Líneas 536–568: Feeds de eventos de seguridad sin `role="alert"`.
   - Líneas 704–725: Estado de tabla con diferenciación de `loading` (spinner + `"Cargando registros de auditoría..."`) y vacío (mensaje + botón `Reintentar`).
4. `dashboard/src/components/__tests__/DecisionAndSecurityPanels.test.tsx`:
   - Líneas 321–368: Nuevas pruebas automatizadas para D3 validando el manejo de 403 Forbidden y el botón `Reintentar` en estado vacío.

---

## Evidencia de Pruebas y Validación

### 1. Pruebas Unitarias Vitest (`npx vitest run`)
```
 RUN  v2.1.9 E:/tylluan-wt-antigravity/dashboard

 ✓ src/components/__tests__/TrustConsoleTab.test.tsx (4 tests) 685ms
   ✓ TrustConsoleTab — sovereign tools counter (D5) > shows 6 / 6 VERIFIED when the kernel reports count=6, ok=true 409ms
 ✓ src/components/__tests__/WorkContractsPanel.test.tsx (5 tests) 1574ms
 ✓ src/components/__tests__/DecisionAndSecurityPanels.test.tsx (12 tests) 2594ms
   ✓ DecisionServicePanel (D1) > renders epistemic warning banner, per-contract agreement summary, and shadow rows 625ms
   ✓ DecisionServicePanel (D1) > shows clear operator clearance state when /api/v1/decide/shadow returns 403 571ms
   ✓ DecisionServicePanel (D1) > renders empty state with Reintentar button when shadow evaluations are empty 362ms
   ✓ AuditTrailPanel (D3) > renders HMAC chain verification, dispatch latency percentiles, and security doctor status
   ✓ AuditTrailPanel (D3) > renders tampered HMAC pill, 0-sample empty latency pill, and degraded security findings
   ✓ AuditTrailPanel (D3) > handles 403 Forbidden with operator clearance message and 403 pill
   ✓ AuditTrailPanel (D3) > renders table empty state with Reintentar button when audit trail entries are empty
 ✓ src/components/__tests__/AgentCompetencyPanel.test.tsx (8 tests) 2825ms
 ✓ src/components/__tests__/FleetHealthPanel.test.tsx (7 tests) 3102ms
 ✓ src/hooks/extracted-hooks.test.tsx (5 tests) 146ms
 ✓ src/components/__tests__/OutputsLedgerPanel.test.tsx (3 tests) 394ms
 ✓ src/components/__tests__/SchedulerConfusionPanel.test.tsx (7 tests) 968ms
 ✓ src/components/__tests__/StigmergyHeatmapPanel.test.tsx (8 tests) 946ms

 Test Files  9 passed (9)
      Tests  59 passed (59)
```

### 2. Chequeo de Tipos TypeScript (`npx tsc --noEmit`)
```
npx tsc --noEmit
Exit code: 0 (0 errores)
```

### 3. Linter (`npx eslint`)
```
npx eslint src/components/TrustConsoleTab.tsx src/components/OverviewTab.tsx src/components/AuditTrailPanel.tsx src/components/__tests__/DecisionAndSecurityPanels.test.tsx
Exit code: 0 (0 errores, 0 warnings)
```

### 4. Puertas de Documentación
- `python scripts/check_doc_links.py`:
  ```
  ✅ 222 relative doc links across 168 Markdown files all resolve to tracked paths.
  Exit code: 0
  ```
- `bash scripts/check_docs_reality.sh`:
  ```
  ✅ No doc-path, doc-port or version drift found.
  Exit code: 0
  ```

---

## Estado del Worktree
El diff permanece **sin commitear** en `E:\tylluan-wt-antigravity`, listo para revisión e integración por parte del Tech Lead (`@claude-code`).
