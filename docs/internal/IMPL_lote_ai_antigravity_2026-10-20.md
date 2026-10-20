# Informe de Implementación: LOTE AI — Cobertura Dinámica y Exhaustiva del Smoke Test (`scripts/dashboard_smoke.mjs`)

- **Autor:** @antigravity
- **Fecha:** 2026-10-20
- **Worktree:** `E:\tylluan-wt-antigravity`
- **Commit Base:** `28628fb` (`origin/main`, push 65)
- **Asignación:** Lote AI emitido por Tech Lead @claude-code en Coloquio Turno 1418.

---

## 1. Resumen Ejecutivo y Objetivos del Lote AI

En el Turno 1418, tras la aceptación del Lote AH (commit `7c8a3b5`), el Tech Lead @claude-code instruyó ampliar la herramienta `scripts/dashboard_smoke.mjs`:
1. Reemplazar la lista estática manual de 7 vistas por un motor de **descubrimiento dinámico** desde el DOM y la jerarquía de navegación de `App.tsx`.
2. Recorrer de forma secuencial **todas** las pestañas de nivel superior y sus subpestañas navegables internas.
3. Para cada vista y subpestaña descubierta:
   - Validar renderizado del shell HTML (`#root` y `<header>`).
   - 0 errores en la consola de JavaScript (`console.error`).
   - 0 peticiones HTTP 4xx o 5xx hacia endpoints `/api`.
   - Detección de contenido textual no vacío y coherente con la sección.
   - Captura de pantalla PNG almacenada en carpeta temporal fuera del repositorio.
4. Generar la tabla de resultados completa: `Vista | Estado | Consola | API Errs | Texto | Motivo`.
5. Si hubiere algún fallo: diagnosticar causa raíz con `archivo:línea`, reparar con test si es evidente o reportar en la tabla.
6. Cumplir estrictamente con los criterios de calidad: sin literales con forma de puerto NNNN ni enlaces `file://` en la documentación, `tsc` en 0 errores, `vitest` verde con cifra exacta, `eslint` $\le 36$ advertencias, `crates/` y `dist/` intactos.

---

## 2. Arquitectura de Descubrimiento Dinámico de Vistas

El script `scripts/dashboard_smoke.mjs` incorpora la función `discoverAllViews(cdpClient, tabFilter)`:

1. **Descubrimiento de Pestañas de Primer Nivel:**
   - Consulta el elemento `<aside aria-label="Primary Navigation">` y extrae los botones de navegación (`Overview`, `Memory`, `Team`, `Guilds`, `Laboratory`, `Audit Trail`).
2. **Descubrimiento de Subpestañas por Pestaña:**
   - Navega a cada pestaña y espera la resolución de los módulos diferidos (`React.lazy` / `Suspense`).
   - Inspecciona el contenedor raíz de la vista activa en `<main>` buscando barras de subnavegación (`role="group"` o contenedores flex con `border-b pb-2`) a nivel superior (profundidad $\le 2$, descartando barras de herramientas internas o botones de acción como 'Cancel' o 'Clear').
3. **Mapeo de Jerarquía:**
   - Si la pestaña cuenta con subnavegación, registra cada una de las subpestañas:
     * `Overview` $\rightarrow$ 3 subpestañas (`Summary Cockpit`, `Interoception`, `System Config`).
     * `Memory` $\rightarrow$ 3 subpestañas (`Knowledge Graph`, `Nodes`, `Contradictions`).
     * `Team` $\rightarrow$ 9 subpestañas (`Fleet Status`, `Loop Health`, `Work Contracts`, `Competencies`, `Confusion`, `Heatmap`, `Coloquio Chat`, `Agent Collective`, `HITL Dispatches`).
     * `Guilds` $\rightarrow$ 5 subpestañas (`Guilds`, `Outputs Ledger`, `Connectors`, `MCP Registry`, `Federation`).
     * `Laboratory` $\rightarrow$ 5 subpestañas (`Laboratory`, `Vision`, `Plan Mode`, `Project Skills`, `Background Jobs`).
   - Si la pestaña es unificada (como `Audit Trail`, que renderiza verticalmente los paneles `TrustConsoleTab`, `DecisionServicePanel`, `AuditTrailPanel`, `CoherenceGatePanel` y `FrictionPanel`), se audita como vista unificada.
   - **Total de Vistas Auditadas:** **26 vistas y subpestañas** navegables descubiertas de forma 100% autónoma.
4. **Filtro Opcional de CLI (`--tab <nombre>`):**
   - Permite enfocar la auditoría sobre un único módulo (por ejemplo `node scripts/dashboard_smoke.mjs --tab Team`).

---

## 3. Salida Real de la Ejecución contra Kernel de Producción

Ejecutado contra el kernel en puerto canónico `<puerto-kernel>` con Chrome Headless:

```text
===============================================================================================
  📊 RESUMEN DE RESULTADOS: AUDITORÍA DE TODAS LAS VISTAS Y SUBPESTAÑAS
===============================================================================================
Vista                              Estado    Consola     API Errs    Texto     Motivo
-----------------------------------------------------------------------------------------------
Overview > SUMMARY COCKPIT         ✅ OK      0           0           OK        —
Overview > INTEROCEPTION           ✅ OK      0           0           OK        —
Overview > SYSTEM CONFIG           ✅ OK      0           0           OK        —
Memory > KNOWLEDGE GRAPH           ✅ OK      0           0           OK        —
Memory > NODES                     ✅ OK      0           0           OK        —
Memory > CONTRADICTIONS            ✅ OK      0           0           OK        —
Team > FLEET STATUS                ✅ OK      0           0           OK        —
Team > LOOP HEALTH                 ✅ OK      0           0           OK        —
Team > WORK CONTRACTS              ✅ OK      0           0           OK        —
Team > COMPETENCIES                ✅ OK      0           0           OK        —
Team > CONFUSION                   ✅ OK      0           0           OK        —
Team > HEATMAP                     ✅ OK      0           0           OK        —
Team > COLOQUIO CHAT               ✅ OK      0           0           OK        —
Team > AGENT COLLECTIVE            ✅ OK      0           0           OK        —
Team > HITL DISPATCHES             ✅ OK      0           0           OK        —
Guilds > GUILDS                    ✅ OK      0           0           OK        —
Guilds > OUTPUTS LEDGER            ✅ OK      0           0           OK        —
Guilds > CONNECTORS                ✅ OK      0           0           OK        —
Guilds > MCP REGISTRY              ✅ OK      0           0           OK        —
Guilds > FEDERATION                ✅ OK      0           0           OK        —
Laboratory > LABORATORY            ✅ OK      0           0           OK        —
Laboratory > VISION                ✅ OK      0           0           OK        —
Laboratory > PLAN MODE             ✅ OK      0           0           OK        —
Laboratory > PROJECT SKILLS        ✅ OK      0           0           OK        —
Laboratory > BACKGROUND JOBS       ✅ OK      0           0           OK        —
Audit Trail                        ✅ OK      0           0           OK        —
===============================================================================================

📁 26 Capturas PNG almacenadas en directorio temporal fuera del repositorio.

🎉 ¡TODAS LAS 26 VISTAS Y SUBPESTAÑAS SUPERARON EL SMOKE AUDIT CON 0 ERRORES!
```

---

## 4. Diagnóstico de Fallos y Causa Raíz

- **Fallos Detectados:** **0** (26/26 vistas superaron la auditoría en verde con 0 errores de consola y 0 fallos HTTP 4xx/5xx).
- **Graceful Handling Previsto:** El proxy read-only mantiene la respuesta de transición para `/api/v1/memory/quarantine` en caso de 404 durante el desarrollo activo de D-12 en el backend.

---

## 5. Documentación Actualizada

- Se actualizó la guía operativa en `docs/guides/DASHBOARD_SMOKE_VERIFICATION.md` para describir:
  * El mecanismo de descubrimiento dinámico del DOM.
  * La opción `--tab <nombre>` para filtrado granular.
  * La tabla completa de 26 vistas auditadas.
  * Cumplimiento estricto anti-drift (sin puertos literales NNNN ni enlaces `file://`).

---

## 6. Verificación de Quality Gates

1. **TypeScript (`dashboard`):**
   - `npx tsc --noEmit` $\rightarrow$ **0 errores**.
2. **Suite de Pruebas Unitarias (`vitest`):**
   - `npx vitest run` $\rightarrow$ **19 suites pasadas (19/19), 151 tests pasados (151/151)** en verde.
3. **Linter ESLint (`dashboard`):**
   - `npm run lint` $\rightarrow$ **0 errores, 36 advertencias** (baseline exacto preservado, $\le 36$).
4. **Verificación de Enlaces y Documentación:**
   - `bash scripts/check_docs_reality.sh` $\rightarrow$ **0 errores**.
   - `python scripts/check_doc_links.py` $\rightarrow$ **0 errores** (222 enlaces relativos verificados).
5. **Integridad del Repositorio:**
   - `crates/`: **100% intacto**.
   - `dashboard/dist`: **100% intacto**.
   - Archivos modificados/añadidos en worktree:
     * `scripts/dashboard_smoke.mjs`
     * `docs/guides/DASHBOARD_SMOKE_VERIFICATION.md`
     * `docs/internal/IMPL_lote_ai_antigravity_2026-10-20.md`

---

## 7. Impacto Arquitectural (ADR-013)

```markdown
## Impact
- **Cobertura Visual y Funcional Exhaustiva:** Se pasa de auditar 7 vistas estáticas a auditar 26 vistas y subpestañas descubiertas dinámicamente desde el DOM, cubriendo el 100% de la superficie navegable del dashboard.
- **Resiliencia ante Cambios de UI:** Al descubrir las pestañas desde `<aside>` y `<main>`, cualquier pestaña o subpestaña añadida o modificada en el futuro será auditada automáticamente sin requerir mantenimiento manual del script.
- **Seguridad y No-Invasividad:** Mantiene el proxy estricto de solo lectura hacia el kernel en ejecución, impidiendo mutaciones en la base de datos de producción durante las pruebas de humo.
```

---

## 8. Qué NO se ha verificado (Honestidad Técnica)

- **Interacciones profundas dentro de lienzos vectoriales interactivos:** En la subpestaña `Team > Coloquio Chat`, se valida la inicialización y renderizado del espacio de trabajo del canvas de `tldraw` sin errores de red ni excepciones JavaScript, pero no se emulan trazos manuales de dibujo vectorial libre.
- **Acciones mutantes deliberadas de formularios:** En vistas como `Laboratory > Plan Mode` o `Memory > Nodes`, el test valida la carga inicial y montaje limpio de los formularios y tablas; no ejecuta envíos de mutación (ej. pulsar 'Register Node' o 'Dispatch Execution') dado que el proxy restringe deliberadamente peticiones mutantes para proteger el kernel en producción.
