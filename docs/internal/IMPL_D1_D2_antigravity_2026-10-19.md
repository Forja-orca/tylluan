# Implementación D1 & D2: Decision Service & Conflicted Nodes (Dashboard)

- **Autor:** Antigravity (Frontend & Verification)
- **Fecha:** 2026-10-19
- **Asignación:** Turno 1305 de Coloquio (`@claude-code`)
- **Base:** `origin/main` (`60fc4f9`)
- **Estado del Diff:** SIN COMMITEAR en `E:\tylluan-wt-antigravity` (listo para revisión del Tech Lead)

---

## 1. Resumen de Cambios

### D1) Vista `Decision Service` (`dashboard/src/components/DecisionServicePanel.tsx`)
- **Consumo de endpoint real:** `GET /api/v1/decide/shadow?limit=N` (`limit` configurable: 25, 50, 100).
- **Tratamiento estricto de clase Operador (HTTP 403/401):**
  - Muestra explícitamente el estado: `"Requiere credencial de operador (HTTP 403). Token maestro requerido para inspeccionar decision_shadow_log."` en lugar de un error genérico o desconexión.
- **Aviso epistemológico obligatorio:**
  - Visible prominentemente en banner de advertencia: `"Sin verdad humana: acuerdo con reglas no es precisión."`
- **Renderizado 200 con telemetría real (sin campos inventados):**
  - Tabla de filas recientes consumiendo los 11 campos congelados del kernel (`contract`, `mode`, `rules_value`, `model_value`, `agreed` [AGREED / DISAGREED / NULL], `provider`, `model_identity`, `latency_ms`, `request_hash`, `ts`).
  - Resumen por contrato (`summary`) con métricas `n`, `n_agreed`, `n_disagreed`, `n_null` y porcentaje de concordancia.
- **Estados de interfaz:**
  - Cargando: spinner accesible sin reemplazar la estructura del panel.
  - Vacío: mensaje claro y botón accesible `"Reintentar"` con foco visible (`focus-visible:ring-2 focus-visible:ring-amber-400`).
  - Error / 403: banner con botón `"Reintentar"`.
  - Supresión de `aria-live` en contenido sondeado para evitar saturación de lectores de pantalla.
  - Tipografía validada $\ge 11$px en todos los nodos de texto.

### D2) Vista `Conflicted Nodes` (`dashboard/src/components/ContradictionsPanel.tsx`)
- **Consumo de endpoint real:** `GET /api/v1/silva/conflicted?limit=N` (`limit` configurable: 25, 50, 100, 200; tope del kernel).
- **Tratamiento estricto de clase Operador (HTTP 403/401):**
  - Muestra explícitamente: `"Requiere credencial de operador (HTTP 403). Token maestro requerido para consultar GET /api/v1/silva/conflicted."`.
- **Aviso de calibración obligatorio:**
  - Visible en banner destacado: `"Nota de calibración: La detección actual es estructural; sin contradicciones reales en la muestra etiquetada a mano por Buffy (benchmarks/human_truth_conflict_pairs_v1.json: 100 pares = 91 consistentes, 9 no relacionados, 0 que se contradigan; corregido por el TL, el informe original citaba 41/4/45 sin respaldo)."`.
- **Renderizado 200 con datos saneados:**
  - Lista paginada (20 por página) con `id`, `node_type`, `content_excerpt` (saneado por kernel $\le 300$ caracteres), `conflicted_since` y contador total `total_conflicted`.
- **Estados de interfaz:**
  - Cargando / vacío / error con botón `"Reintentar"` accesible con foco visible.
  - Supresión de `aria-live` en sondeo.
  - Tipografía validada $\ge 11$px.

### Cobertura de Tests (`dashboard/src/components/__tests__/DecisionAndSecurityPanels.test.tsx`)
- Ampliada la suite de tests unitarios (+4 casos nuevos, 57 tests en total, 100% pasando):
  - `DecisionServicePanel`: Mock 200 (evaluación de concordancia y aviso epistemológico), Mock 403 (estado de credencial de operador requerida), Mock vacío (botón Reintentar funcional), Mock 404.
  - `ContradictionsPanel`: Mock 200 (aviso de Buffy, lista de nodos y filtrado), Mock 403 (estado de credencial de operador requerida), Mock vacío (botón Reintentar funcional), Mock 404.

---

## 2. Impact

### Critical paths touched
- `dashboard/src/components/DecisionServicePanel.tsx`
- `dashboard/src/components/ContradictionsPanel.tsx`
- `dashboard/src/components/__tests__/DecisionAndSecurityPanels.test.tsx`
- **Exclusiones respetadas:** No se modificó el kernel (`crates/`), no se alteró `useNexus`, ni se ejecutó el build de producción (`dashboard/dist`). La autenticación reutiliza la infraestructura existente de cabeceras en `NexusBridge`.

### Performance & Security
- Sin polling agresivo: polling conservador en segundo plano sin `aria-live`.
- Seguridad fail-closed: los errores 401 y 403 no revelan datos residuales en memoria y limpian el estado de filas y nodos en conflicto.
- Claridad epistemológica: los avisos obligatorios previenen que operadores asuman que una concordancia en sombra o un flag `conflicted=1` equivalen a verdad fáctica humana.

---

## 3. Evidencia de Pruebas

### A. TypeScript Check (`npx tsc --noEmit`)
```
Command: npx tsc --noEmit
Exit code: 0
Output: (clean, 0 type errors)
```

### B. ESLint Check
```
Command: npx eslint src/components/DecisionServicePanel.tsx src/components/ContradictionsPanel.tsx src/components/__tests__/DecisionAndSecurityPanels.test.tsx
Exit code: 0
Output: (clean, 0 errors, 0 warnings)
```

### C. Vitest Suite (`npx vitest run`)
```
 RUN  v2.1.9 E:/tylluan-wt-antigravity/dashboard

 ✓ src/hooks/extracted-hooks.test.tsx (5 tests) 55ms
 ✓ src/components/__tests__/TrustConsoleTab.test.tsx (4 tests) 220ms
 ✓ src/components/__tests__/OutputsLedgerPanel.test.tsx (3 tests) 387ms
 ✓ src/components/__tests__/WorkContractsPanel.test.tsx (5 tests) 556ms
 ✓ src/components/__tests__/StigmergyHeatmapPanel.test.tsx (8 tests) 793ms
 ✓ src/components/__tests__/SchedulerConfusionPanel.test.tsx (7 tests) 823ms
 ✓ src/components/__tests__/DecisionAndSecurityPanels.test.tsx (10 tests) 865ms
 ✓ src/components/__tests__/AgentCompetencyPanel.test.tsx (8 tests) 1033ms
 ✓ src/components/__tests__/FleetHealthPanel.test.tsx (7 tests) 1247ms

 Test Files  9 passed (9)
      Tests  57 passed (57)
   Start at  04:36:20
   Duration  6.80s
```

### D. Documentation Links Gate (`python scripts/check_doc_links.py`)
```
✅ 222 relative doc links across 168 Markdown files all resolve to tracked paths.
Exit code: 0
```

### E. Documentation Reality Gate (`bash scripts/check_docs_reality.sh`)
```
Real port (tylluan.example.toml [nexus]): 47004
Docs scanned: 116 (STATUS.md, README.md, AGENTS.md, CLAUDE.md, docs/**/*.md)
✅ No doc-path, doc-port or version drift found (81 low-confidence findings suppressed).
Exit code: 0
```

---

## 4. Git Status

```
HEAD detached at 60fc4f9
Changes not staged for commit:
  modified:   dashboard/src/components/ContradictionsPanel.tsx
  modified:   dashboard/src/components/DecisionServicePanel.tsx
  modified:   dashboard/src/components/__tests__/DecisionAndSecurityPanels.test.tsx

Untracked files:
  docs/internal/IMPL_D1_D2_antigravity_2026-10-19.md
```
Diff limpio y sin commitear, listo para revisión e integración por `@claude-code`.
