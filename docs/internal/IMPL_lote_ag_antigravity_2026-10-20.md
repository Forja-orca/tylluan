# Informe de Implementación: LOTE AG — Rendimiento de Carga del Dashboard y Optimización de Chunks

- **Autor:** @antigravity
- **Fecha:** 2026-10-20
- **Worktree:** `E:\tylluan-wt-antigravity`
- **Commit Base:** `9912990` (`origin/main`, push 62)
- **Asignación:** Lote AG emitido por Tech Lead @claude-code en Coloquio Turno 1409.

---

## 1. Resumen Ejecutivo y Objetivos del Lote AG

En el Turno 1409, el Tech Lead @claude-code asignó a @antigravity el **LOTE AG**, enfocado en la auditoría y optimización del rendimiento de carga del dashboard:
1. Medir con Chrome headless sobre el kernel real (`:47004`) el peso de `dist` por chunk, tiempo hasta primer contenido (FCP) y hasta interactivo (TTI) en la vista por defecto (`Overview`) y en `Coloquio`.
2. Elaborar la tabla `chunk -> tamaño gzip -> qué lo causa` (`tldraw`, `react-force-graph`, verificación de que `axe-core` queda fuera de producción).
3. Si existe algún chunk > 500 KB gzip o la carga inicial supera los 2 segundos, aplicar code-splitting por ruta (`React.lazy` + `Suspense`) y particionamiento de chunks (`manualChunks`) sin alterar comportamiento, con tests.
4. Respetar estrictamente los quality gates: `tsc 0`, `vitest` verde con cifra exacta, `eslint <= 36`, `crates/` y `dist` del kernel 100% intactos (builds realizados en directorios temporales aislados), diff sin commitear, e informe con `## Impact` y "no verificado".

---

## 2. Diagnóstico Técnico y Causa Raíz

Al auditar la configuración previa de `vite.config.ts`, se detectaron anomalías críticas que degradaban el bundle inicial:

1. **Fuga de React hacia `vendor-tldraw`:**
   - La regla previa `manualChunks: { 'vendor-react': ['react', 'react-dom'] }` no incluía `react/jsx-runtime`, `react-dom/client` ni `scheduler`.
   - Debido a esto, Rollup consolidó el runtime de JSX y `scheduler` dentro del chunk `@tldraw/tldraw`.
   - Como resultado, `index.js` (y cualquier componente React que emitiera JSX) importaba estáticamente `vendor-tldraw`.
   - El bundle de `vendor-react` quedaba prácticamente vacío (0.07 KB) y `vendor-tldraw` engordaba hasta **1.95 MB (592.48 KB gzip)**, superando el umbral de 500 KB gzip.

2. **Preload agresivo de dependencias dinámicas:**
   - Vite inyectaba en el `<head>` de `index.html` etiquetas `<link rel="modulepreload">` para `vendor-tldraw` y `vendor-force-graph`.
   - Por tanto, al abrir la vista `Overview`, el navegador descargaba inmediatamente casi 2 MB de librerías destinadas exclusivamente a pestañas secundarias diferidas (`ColoquioCanvasWorkspace` y `KnowledgeCortex2D`).

3. **Inexistencia de `axe-core` en producción (Verificado):**
   - Se comprobó exhaustivamente que `axe-core` se encuentra confinado a `devDependencies: { "axe-core": "4.10.2" }` y únicamente es referenciado por suites de pruebas en `src/components/__tests__/`. Ningún módulo de producción lo importa ni lo incluye (0 bytes en el bundle final).

---

## 3. Optimizaciones Aplicadas en `dashboard/vite.config.ts`

1. **Aislamiento completo de React (`vendor-react`):**
   - Se agruparon `node_modules/react/`, `node_modules/react-dom/` y `node_modules/scheduler/` en `vendor-react`.
   - `vendor-react` pasó de 0.07 KB a **194.63 KB (60.86 KB gzip)**, desacoplando completamente el runtime central de cualquier librería externa.

2. **Partición limpia de `tldraw` sin dependencias circulares:**
   - Se dividió el ecosistema de tldraw en dos mitades jerárquicas:
     - `vendor-tldraw-engine` (880.75 KB / **272.33 KB gzip**): Contiene los módulos core sin UI (`@tldraw/editor`, `@tldraw/tlschema`, `@tldraw/store`, `@tldraw/state`, `@tldraw/utils`, `@tldraw/validate`, `@tldraw/driver`).
     - `vendor-tldraw-ui` (885.76 KB / **263.74 KB gzip**): Contiene las herramientas de UI, lienzo, primitivas y renderizado interactivo (`@tldraw/tldraw`).
   - Ninguno de los chunks de tldraw supera ya los 500 KB gzip (reducción del pico de 592 KB a 272 KB gzip, **-54% de reducción en el chunk más pesado**).

3. **Partición de utilidades (`vendor-utils`):**
   - Se extrajeron `clsx` y `tailwind-merge` hacia `vendor-utils` (27.51 KB / **8.68 KB gzip**), lo que redujo el chunk principal `index.js` de 261.58 KB a **232.83 KB (55.86 KB gzip)**.

4. **Desactivación de `modulePreload` indiscriminado (`modulePreload: false`):**
   - Evita la inyección de directivas de precarga obligatoria para assets que corresponden a rutas diferidas. Los componentes lazy (`React.lazy`) se descargan bajo demanda nativa mediante ECMAScript `import()` solo cuando el operador pulsa la pestaña correspondiente.

5. **Configuración de `preview` con proxy hacia el kernel real (:47004):**
   - Permite verificar de forma transparente builds de producción empaquetados en puertos locales sin comprometer el puerto de producción.

---

## 4. Tabla de Chunks: Antes vs Después

| Chunk | Tamaño Antes (gzip) | Tamaño Después (gzip) | Causa / Contenido |
|---|---|---|---|
| `vendor-tldraw` (unificado) | 1,951.14 kB (592.48 kB) | *Eliminado* | Antes contenía @tldraw + react/jsx + scheduler |
| `vendor-tldraw-engine` | *No existía* | **880.75 kB (272.33 kB)** | Motor @tldraw (editor, tlschema, store, state, utils) |
| `vendor-tldraw-ui` | *No existía* | **885.76 kB (263.74 kB)** | UI y canvas @tldraw/tldraw |
| `index.js` (entry point) | 261.58 kB (64.97 kB) | **232.83 kB (55.86 kB)** | Core dashboard, App.tsx, OverviewConsolidated |
| `vendor-react` | 0.07 kB (0.08 kB) | **194.63 kB (60.86 kB)** | React 19, react-dom, scheduler, jsx-runtime |
| `vendor-force-graph` | 181.92 kB (61.04 kB) | **181.92 kB (61.04 kB)** | react-force-graph-2d + d3-force |
| `vendor-lucide` | 62.70 kB (14.39 kB) | **52.68 kB (10.54 kB)** | Iconografía lucide-react |
| `vendor-utils` | *No existía* | **27.51 kB (8.68 kB)** | clsx + tailwind-merge desacoplados |
| `ColoquioTab` | 77.00 kB (23.19 kB) | **76.92 kB (23.17 kB)** | Vista principal de mensajería y canales |
| `MemoryConsolidated` | 60.63 kB (14.25 kB) | **60.46 kB (14.20 kB)** | Pestaña de memoria (FSRS, SilvaDB, graph) |
| `LabConsolidated` | 50.86 kB (11.61 kB) | **50.94 kB (11.64 kB)** | Pestaña de laboratorio / benchmarks |
| `ColoquioCanvasWorkspace` | 32.19 kB (10.35 kB) | **32.27 kB (10.39 kB)** | Integración canvas colaborativo tldraw |
| `KnowledgeCortex2D` | 5.12 kB (2.47 kB) | **5.12 kB (2.46 kB)** | Wrapper 2D react-force-graph diferido |
| `axe-core` | **0 B (Excluido)** | **0 B (Excluido)** | 100% confinado a tests y devDependencies |

> [!NOTE]
> Tras la optimización, **ningún chunk individual excede los 300 KB gzip**. El umbral crítico de 500 KB gzip impuesto por el Tech Lead se cumple con holgura.

---

## 5. Mediciones de Rendimiento en Vivo (Chrome Headless)

Mediciones realizadas con Chrome Headless (`chrome-devtools-mcp` y arnés Puppeteer/CDP sobre el kernel real `http://127.0.0.1:47004`):

### Vista por Defecto (`OverviewTab`)
- **First Paint (FP):** 288 ms (kernel) / 492 ms (preview en frío).
- **First Contentful Paint (FCP):** **372 ms** (kernel) / **608 ms** (preview en frío). Ambos valores ampliamente inferiores al límite de 2.000 ms (< 0.7 s).
- **DOMContentLoaded:** 259 ms (kernel) / 470 ms (preview).
- **Descarga Inicial de JavaScript:** Antes se forzaba la descarga de 580 KB transfer / 1.95 MB de tldraw en el overview; ahora la descarga del runtime core queda contenida en ~135 KB transfer (~480 KB descomprimidos) para React, Lucide y Utils.

### Transición a Pestaña `Coloquio`
- **Latencia de Activación:** Al seleccionar `Team -> Coloquio`, los módulos diferidos se recuperan bajo demanda:
  - `ColoquioTab.js`: **8 ms** (transfer: 23.4 KB).
  - `ColoquioCanvasWorkspace.js`: **8 ms** (transfer: 10.7 KB).
  - Canales iniciales (`/channels` y `/messages`): **13 ms**.
- **Tiempo hasta Interactivo en Coloquio:** **< 35 ms**. El textarea y la lista de mensajes quedan completamente listos y reactivos para entrada del operador sin bloqueos de hilo principal.

---

## 6. Verificación de Quality Gates

1. **Compilación de paquetes y TypeScript:**
   - `packages/tylluan-ui-core`: `npm run build` $\rightarrow$ OK (tsc limpio).
   - `dashboard`: `npx tsc --noEmit` $\rightarrow$ **0 errores**.
2. **Suite de Pruebas Unitarias (`vitest`):**
   - Nueva suite: `dashboard/src/components/__tests__/BundlePerformance.test.tsx` (6 tests específicos verificando la exclusión de axe-core de producción, la envoltura en React.lazy de todas las pestañas pesadas y la presencia de particiones en `vite.config.ts`).
   - `npx vitest run src/components/__tests__/BundlePerformance.test.tsx` $\rightarrow$ **6/6 tests pasando**.
   - `npx vitest run` (suite completa) $\rightarrow$ **19 suites pasadas (19/19), 151 tests pasados (151/151)** en verde (100% éxito).
3. **Linter ESLint (`dashboard`):**
   - `npm run lint` $\rightarrow$ **0 errores, 36 advertencias** (baseline exacto preservado, $\le 36$).
4. **Integridad del Repositorio:**
   - `crates/`: **100% intacto** (0 ficheros modificados).
   - `dashboard/dist`: **100% intacto** (todas las pruebas de build se realizaron en `scratch/temp_dist_*`).
   - Modificaciones contenidas exclusivamente en `dashboard/vite.config.ts` y test nuevo en `dashboard/src/components/__tests__/BundlePerformance.test.tsx`.

---

## 7. Impacto Arquitectural (ADR-013)

```markdown
## Impact
- **Riesgo Operativo:** Nulo. No se alteró la lógica interna de ningún componente ni endpoint del kernel.
- **Rendimiento de Red:** Eliminación del 100% de sobrecarga por precarga anticipada de tldraw en la pantalla inicial de inicio.
- **Soberanía y Dependencias:** axe-core verificado fuera del runtime de producción. React 19 aislado limpiamente en su propio chunk.
```

---

## 8. Qué NO se ha verificado (Honestidad Técnica)

- **Simulación en conexiones de red 3G/4G con estrangulamiento:** Las mediciones de FCP y TTI se obtuvieron sobre red local/loopback (`127.0.0.1`), por lo que las latencias de red en entornos remotos o móviles dependerán del ancho de banda y RTT del cliente.
- **Compatibilidad con navegadores legacy sin soporte ESM:** La arquitectura asume soporte de módulos ES nativos y `import()` dinámico (estándar en navegadores modernos desde 2020).
