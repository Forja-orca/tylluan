# Informe de Implementación — Lotes R1 (Revisión Visual Real Chrome Headless) y R2 (Preparación Q8 Reconstrucción Dist)

**Fecha:** 2026-10-19  
**Autor:** @antigravity  
**Destinatario:** @claude-code (Tech Lead)  
**Worktree:** `E:\tylluan-wt-antigravity`  
**Base commit:** `69f9e9e` (`origin/main`)  

---

## ## Impact

- **Soberanía y Estabilidad (CONTRACT-01 & ADR-013):** Trabajo confinado exclusivamente al worktree `E:\tylluan-wt-antigravity`. Ningún fichero en `crates/`, `dist/`, ni `useNexus` fue modificado.
- **Ámbito Acotado:** 3 ficheros UI ajustados con correcciones mínimas de layout móvil y contraste (`AuditTrailPanel.tsx`, `ContradictionsPanel.tsx`, `DecisionServicePanel.tsx`) con un diff neto de +25 / -22 líneas.
- **Verificación de Realidad en Blink Engine (R1):** Ejecución automatizada con `chrome-headless-shell` contra un backend local simulado en puerto efímero (`http://127.0.0.1:<puerto-efimero>`), evaluando 16 estados (4 vistas × 2 temas × 2 viewports: 390px móvil y 1440px escritorio). Detección y eliminación completa de desbordamientos horizontales (de 2 a 0).
- **Plan de Despliegue de Producción (R2):** Protocolo exacto y reproducible para que el Tech Lead ejecute la reconstrucción de `dashboard/dist` y verifique el servicio estático desde el kernel TylluanNexus (`:47004`).
- **Métricas:** TypeScript `tsc --noEmit` en 0 errores, Vitest 69/69 tests en verde (11 suites), ESLint preservado en 39 avisos (0 errores).

---

## 1. Lote R1: Revisión Visual Real con Chrome Headless

### Arquitectura de Prueba Visual
1. **Mock Backend Local (`http://127.0.0.1:<puerto-efimero>`):**  
   Servidor HTTP ligero en Python con soporte CORS completo (`Access-Control-Allow-Origin: *`) respondiendo con esquemas JSON reales de producción para:
   - `/api/v1/decide/shadow?limit=...` (vistas de acuerdo y filas de sombra con proveedores `decima` y `laya`).
   - `/api/v1/silva/conflicted?limit=...` (nodos de SilvaDB con extractos y metadatos de conflicto).
   - `/api/v1/audit/trail`, `/api/v1/audit/verify`, `/api/v1/audit/latency`, `/api/v1/security/events`, `/api/v1/audit/logs`.
   - `/api/v1/mesh/peers`, `/api/v1/federation/nodes`, `/api/v1/federation/peers`, `/api/v1/federation/sharing`.
   - `/health` y `/api/v1/health/detailed`.
2. **Motor de Renderizado:**  
   Google Chrome Headless Shell (`chromium_headless_shell-1228\chrome-headless-shell.exe`) ejecutado con `--virtual-time-budget=2500` y `--hide-scrollbars` para permitir la estabilización completa de las llamadas asíncronas de React `useEffect` y el puente `NexusBridge`.
3. **Detección Automática de Overflows:**  
   Evaluación en DOM de `scrollWidth > clientWidth + 2` sobre todos los elementos para detectar cualquier recorte o ensanchamiento horizontal que rompa el viewport móvil.

### Matriz de Capturas y Resultados (16/16)

| Vista | Tema | Viewport | Fichero Generado | Tamaño | Overflows Iniciales | Overflows Finales |
|---|---|---|---|---|---|---|
| **DecisionServicePanel** | Oscuro | 1440 × 900 | `decision_dark_1440.png` | 220,431 B | 0 | **0** |
| **DecisionServicePanel** | Oscuro | 390 × 844 | `decision_dark_390.png` | 95,228 B | 1 (celdas métricas) | **0** |
| **DecisionServicePanel** | Claro | 1440 × 900 | `decision_light_1440.png` | 209,505 B | 0 | **0** |
| **DecisionServicePanel** | Claro | 390 × 844 | `decision_light_390.png` | 91,309 B | 1 (celdas métricas) | **0** |
| **ContradictionsPanel** | Oscuro | 1440 × 900 | `contradictions_dark_1440.png` | 195,359 B | 0 | **0** |
| **ContradictionsPanel** | Oscuro | 390 × 844 | `contradictions_dark_390.png` | 102,201 B | 1 (ruta sin wrap + título) | **0** |
| **ContradictionsPanel** | Claro | 1440 × 900 | `contradictions_light_1440.png` | 185,756 B | 0 | **0** |
| **ContradictionsPanel** | Claro | 390 × 844 | `contradictions_light_390.png` | 98,843 B | 1 (ruta sin wrap + título) | **0** |
| **AuditTrailPanel** | Oscuro | 1440 × 900 | `audit_dark_1440.png` | 192,920 B | 0 | **0** |
| **AuditTrailPanel** | Oscuro | 390 × 844 | `audit_dark_390.png` | 98,367 B | 0 | **0** |
| **AuditTrailPanel** | Claro | 1440 × 900 | `audit_light_1440.png` | 182,343 B | 0 | **0** |
| **AuditTrailPanel** | Claro | 390 × 844 | `audit_light_390.png` | 94,227 B | 0 | **0** |
| **FederationTab** | Oscuro | 1440 × 900 | `federation_dark_1440.png` | 203,155 B | 0 | **0** |
| **FederationTab** | Oscuro | 390 × 844 | `federation_dark_390.png` | 84,304 B | 0 | **0** |
| **FederationTab** | Claro | 1440 × 900 | `federation_light_1440.png` | 183,487 B | 0 | **0** |
| **FederationTab** | Claro | 390 × 844 | `federation_light_390.png` | 77,564 B | 0 | **0** |

*Nota:* Las 16 capturas PNG completas están almacenadas en `C:\Users\FoRJa\.gemini\antigravity\brain\7f88a3b8-c6df-4079-bb5b-d198fc6b7efb\visual_reviews\`.

### Problemas Detectados y Correcciones de Bajo Riesgo

1. **`ContradictionsPanel.tsx` — Desbordamiento horizontal en 390px (Grave en móvil):**
   - *Causa raíz 1:* En el banner de advertencia epistemológica (`Nota de calibración`), la cadena literal `benchmarks/human_truth_conflict_pairs_v1.json` se renderizaba como una única palabra no rompible dentro de un contenedor flex sin `min-w-0`. El texto forzaba un ancho de 343px sobre una columna de 272px, estirando el viewport de la página de 390px a 402px y provocando scroll horizontal accidental.
   - *Causa raíz 2:* El título del panel `Conflicted Nodes (/api/v1/silva/conflicted)` en `text-xl` superaba por 4px el ancho del contenedor en 390px.
   - *Corrección aplicada:* Se añadió `min-w-0 break-words` en el wrapper de texto y `break-all font-mono` en la etiqueta `<code>` de la ruta. En el título se flexibilizó el contenedor con `min-w-0`, `flex-wrap` y escala responsiva `text-lg sm:text-xl`. Desbordamiento eliminado a 0px.
2. **`DecisionServicePanel.tsx` — Desbordamiento en celdas de acuerdo en 390px:**
   - *Causa raíz:* En las tarjetas de resumen por contrato, la rejilla de 3 columnas (`grid grid-cols-3 gap-1.5`) asignaba un ancho de 74px a cada celda. La etiqueta monoespaciada `Abstain/Null` (12 caracteres) ocupaba ~84px, desbordando por 4px (`scrollWidth: 78 > clientWidth: 74`).
   - *Corrección aplicada:* Se ajustó la rejilla a `gap-1`, se añadió `min-w-0` y `truncate block text-[10px]` con atributo `title="Abstain/Null"`. Desbordamiento eliminado a 0px.
3. **`AuditTrailPanel.tsx` — Contraste de encabezado en tema claro:**
   - *Causa raíz:* A diferencia de `DecisionServicePanel` y `ContradictionsPanel` (que encapsulan su contenido en tarjetas obsidian `bg-slate-900/70 border-slate-800 rounded-2xl`), `AuditTrailPanel` carecía de contenedor semántico envolvente. Su título `text-slate-50` se renderizaba directamente sobre el fondo claro de la página en tema claro, resultando en un contraste casi nulo (~1.08:1, ilegible).
   - *Corrección aplicada:* Se encapsuló `AuditTrailPanel` en `<section aria-labelledby="audit-telemetry-heading" className="bg-slate-900/70 border border-slate-800 rounded-2xl p-6 space-y-6 font-sans">` con encabezado delimitado por `border-b border-slate-800 pb-4`. Esto unifica la jerarquía visual de todos los paneles de auditoría y garantiza contraste WCAG AAA en ambos temas.

---

## 2. Lote R2: Protocolo de Preparación Q8 (Reconstrucción de `dashboard/dist` y Verificación Kernel)

Conforme a la instrucción (*"lista, sin ejecutar, los pasos exactos para reconstruir dashboard/dist y verificar que el kernel lo sirve; yo lo ejecuto"*), se detalla la secuencia exacta y determinista:

### Paso 1: Reconstrucción del paquete base compartido (`@tylluan/ui-core`)
Dado que `dashboard` consume `@tylluan/ui-core` vía enlace local de workspace (`file:../packages/tylluan-ui-core`), cualquier cambio en `api-client.ts` o contratos compartidos debe compilarse previamente a `./dist`:
```bash
cd E:\tylluan\packages\tylluan-ui-core
npm run build
```
*Verificación:* Confirmar que `packages/tylluan-ui-core/dist/index.js` y `index.d.ts` existen y tienen fecha actualizada.

### Paso 2: Reconstrucción del bundle del Dashboard con Vite
En el directorio `dashboard`:
```bash
cd E:\tylluan\dashboard
pnpm run build
```
*Detalle técnico:* Este comando ejecuta `"tsc && vite build"`. La configuración de `vite.config.ts` tiene declarado `build.outDir: "../dashboard/dist"` y `emptyOutDir: true`, empaquetando React, Lucide, Tailwind y chunks de dependencias vendor en `dashboard/dist/assets/` y generando `dashboard/dist/index.html`.  
*Verificación en disco:*
```bash
Test-Path E:\tylluan\dashboard\dist\index.html
Get-ChildItem E:\tylluan\dashboard\dist\assets\*.js
```

### Paso 3: Verificación de Servicio Estático en Kernel TylluanNexus (`:47004`)
El kernel sirve `dashboard/dist` mediante dos mecanismos (`crates/tylluan-kernel/src/transport/http/mod.rs` L1209 y L1330):
1. **Modo Dinámico / Workspace:** Busca `find_workspace_root().join("dashboard/dist")` y sirve `index.html` en la raíz `/` y los recursos estáticos en `/assets/*`.
2. **Modo Bundled (`--features bundled-dashboard`):** Incluye la carpeta compilada mediante la macro `rust-embed`.

Para verificar que el kernel en ejecución sirve el nuevo dist:
```bash
# 1. Comprobar cabeceras HTTP y Content-Type de index.html
curl -sI http://127.0.0.1:47004/
# Esperado: HTTP/1.1 200 OK, Content-Type: text/html

# 2. Comprobar que index.html contiene el punto de entrada de la aplicación
curl -s http://127.0.0.1:47004/ | Select-String "Tylluan Dashboard"

# 3. Comprobar que los assets estáticos empaquetados responden 200 OK
# (Tomar el hash del JS principal generado en dist/assets/index-*.js)
curl -sI http://127.0.0.1:47004/assets/<nombre-del-bundle>.js
# Esperado: HTTP/1.1 200 OK, Content-Type: application/javascript
```

---

## 3. Verificaciones de Calidad

| Comprobación | Comando | Resultado |
|---|---|---|
| **TypeScript Typecheck** | `npx tsc --noEmit` en `dashboard` | **0 errores** (código 0) |
| **Vitest Test Suite** | `npx vitest run` en `dashboard` | **11 archivos pasados (11/11), 69 tests en verde (69/69)** |
| **ESLint Warnings** | `npm run lint` en `dashboard` | **0 errores, 39 avisos** (idéntico a la línea base aprobada en Lote Q) |
| **Overflows Visuales** | Blink Engine (`chrome-headless-shell`) en 16 estados | **0 desbordamientos horizontales detectados (16/16 verde)** |
| **Git Status** | `git diff --stat` | **3 ficheros modificados (+25 / -22 líneas)** |
| **Áreas Excluidas** | `crates/`, `dist`, `useNexus`, procesos kernel | **0 modificaciones (100% intactas)** |
