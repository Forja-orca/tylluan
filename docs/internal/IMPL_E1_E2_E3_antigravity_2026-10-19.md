# Informe de Implementación — Lotes E1 (Reducción ESLint), E2 (Stubs D4) y E3 (Verificación Navegador Real Q5)

**Fecha:** 2026-10-19  
**Autor:** @antigravity  
**Destinatario:** @claude-code (Tech Lead)  
**Worktree:** `E:\tylluan-wt-antigravity`  
**Base commit:** `6b4ea57` (`origin/main`)  

---

## ## Impact

- **Soberanía y Estabilidad:** Cumplimiento estricto de CONTRACT-01 y reglas de no-depredación (ADR-013).
- **Alcance Acotado:** Modificaciones exclusivas en `dashboard/src/`. Ningún archivo en `crates/`, `dist`, ni `useNexus` fue modificado.
- **Reducción de Deuda Técnica (E1):** Se redujeron los avisos de ESLint de **129** a **91** (38 avisos eliminados, meta $\le 100$ superada holgadamente). Los 10 ficheros bajo el ámbito directo de trabajo quedaron con 0 avisos.
- **Auditoría de Endpoints (E2):** Se documenta la localización exacta de los dos stubs huérfanos de `inference/providers` en `crates/` para que Deep o el Tech Lead procedan con su eliminación segura en el kernel.
- **Verificación de Realidad en Navegador (E3):** Ejecución directa en Google Chrome Blink Engine (Headless `chrome.exe`) validando la evaluación de `hsl(var(--...))` en SVG y drop-shadows tanto en tema oscuro como en tema claro.

---

## 1. Lote E1: Reducción y Clasificación de Avisos ESLint (129 → 91)

### Clasificación Inicial (129 avisos) vs Final (91 avisos)

| Regla ESLint | Avisos Iniciales | Avisos Finales | Reducción |
|---|---|---|---|
| `@typescript-eslint/no-explicit-any` | 70 | 53 | **-17** |
| `@typescript-eslint/no-unused-vars` | 38 | 22 | **-16** |
| `react-refresh/only-export-components` | 13 | 12 | **-1** |
| `react-hooks/exhaustive-deps` | 8 | 4 | **-4** |
| **TOTAL** | **129** | **91** | **-38** (Meta: $\le 100$) |

### Acciones de Corrección por Fichero
1. **`dashboard/src/lib/canvas-color.ts`:**
   - Creado módulo independiente para `resolveCanvasColor()` desacoplándolo de `ui/MetricPrimitives.tsx`.
   - Elimina la infracción de `react-refresh/only-export-components` en `MetricPrimitives.tsx`.
2. **`FederationTab.tsx` (7 avisos eliminados → 0 avisos):**
   - Eliminado estado huérfano `nodes` e interfaz `SilvaNode` (reemplazados por `fedNodes` / `FederationNodeItem`).
   - Eliminado estado huérfano `sharingStatusLoading`.
   - Sustituido `catch (err: any)` por `catch (err: unknown)` con extracción segura de mensaje en 5 bloques asíncronos (`handleToggleSharing`, `handleAddPeer`, `handleDeletePeer`, `handleSyncAll`, `handleToggleShareable`).
3. **`HomeostasisWidget.tsx` (1 aviso eliminado → 0 avisos):**
   - Tipado explícito de `health` mediante interfaz `HomeostasisHealth`.
4. **`MaintenanceTab.tsx` (2 avisos eliminados → 0 avisos):**
   - Eliminado import no utilizado `Settings`.
   - Tipado explícito de `probe` mediante interfaz `ProbeResult`.
5. **`LaboratoryTab.tsx` (5 avisos eliminados → 0 avisos):**
   - Eliminados imports no utilizados `GitCommit`, `RotateCcw`, `AlertTriangle`.
   - `fetchData` estabilizado con `useCallback(..., [bridge])` y conectado como dependencia de `useEffect`.
6. **`CanaryStatusWidget.tsx` (3 avisos eliminados → 0 avisos):**
   - Eliminado import no utilizado `AlertCircle`.
   - `fetchCanary` estabilizado con `useCallback(..., [bridge])` y tipado de error.
7. **`DoctorPanel.tsx` (5 avisos eliminados → 0 avisos):**
   - `loadReport` estabilizado con `useCallback(..., [bridge])`.
   - Sustituido `any` en tipado de errores y promesas (`Promise<void>[]`).
8. **`DreamStatusWidget.tsx` (5 avisos eliminados → 0 avisos):**
   - Eliminados imports no utilizados `useCallback`, `RefreshCw`.
   - Creada interfaz `DreamStatusData` para tipar completamente el estado `dreamStatus`.
9. **`RepoMapWidget.tsx` (5 avisos eliminados → 0 avisos):**
   - Eliminado import no utilizado `ServerOff`.
   - Tipado de `bridge: NexusBridge | null`.
   - Renderizado del estado `error` en caso de fallo.
   - `fetchRepoMap` estabilizado con `useCallback(..., [bridge])`.
10. **`ResumeSessionWidget.tsx` (4 avisos eliminados → 0 avisos):**
    - Eliminado import no utilizado `ServerOff`.
    - Tipado de `bridge: NexusBridge | null`.
    - Renderizado del banner de `error` y tipado seguro de excepciones.

---

## 2. Lote E2: Localización de Stubs Huérfanos de `inference/providers` en Kernel (para Deep / Tech Lead)

De acuerdo con la instrucción de no tocar `crates/`, se detalla la localización exacta para su poda:

1. **Definición de Handlers (stubs vacíos):**
   - Fichero: `crates/tylluan-kernel/src/transport/http/api_v1/api_admin.rs`
   - Línea 877: `pub async fn list_inference_providers_handler() -> impl IntoResponse { StatusCode::OK }`
   - Línea 878: `pub async fn add_inference_provider_handler() -> impl IntoResponse { StatusCode::OK }`
2. **Registro de Rutas en Router:**
   - Fichero: `crates/tylluan-kernel/src/transport/http/api_v1/routes.rs`
   - Línea 102: `.route("/api/v1/inference/providers", get(list_inference_providers_handler).post(add_inference_provider_handler))`
3. **Aserciones en Tests de Autorización:**
   - Fichero: `crates/tylluan-kernel/src/transport/http/auth.rs`
   - Línea 309: `assert!(!operator_class_required("GET", "/api/v1/inference/providers"), "reads stay generic");`
4. **Endpoint que SÍ permanece intacto:**
   - Línea 907 de `api_admin.rs`: `test_inference_provider_handler` (`POST /api/v1/inference/providers/{name}/test`), ya que realiza pings reales con medición de latencia sobre servidores MCP y es consumido.

---

## 3. Lote E3: Verificación en Navegador Real (Google Chrome Headless)

Se ejecutó `chrome.exe` (`C:\Program Files\Google\Chrome\Application\chrome.exe`) con flags `--headless=new --disable-gpu` sobre un banco de pruebas DOM con los tokens de `index.css`.

### Lecturas de `window.getComputedStyle(element)` en Blink Engine:

| Componente | Propiedad Declarada | Tema Claro (`:root`) | Tema Oscuro (`.dark`) |
|---|---|---|---|
| **`FrictionPanel` (ScoreRing)** | `stroke: hsl(var(--emerald-500))` | `rgb(16, 183, 127)` | `rgb(16, 183, 127)` |
| **`TylluanLogo` (Path)** | `fill: hsl(var(--amber-500))` | `rgb(233, 148, 12)` | `rgb(230, 171, 61)` |
| **`HomeostasisWidget` (Gauge Stroke)** | `stroke: hsl(var(--emerald-400))` | `rgb(5, 148, 103)` | `rgb(54, 211, 159)` (equiv. `#34d399`) |
| **`HomeostasisWidget` (Drop Shadow)** | `filter: drop-shadow(... hsl(var(--emerald-400) / 0.4))` | `drop-shadow(rgba(5, 148, 103, 0.4) 0px 0px 4px)` | `drop-shadow(rgba(54, 211, 159, 0.4) 0px 0px 4px)` |

- **Captura Renderizada:** Generada exitosamente por Chrome Headless (`74,625 bytes`), almacenada en el directorio de artefactos: `C:\Users\FoRJa\.gemini\antigravity\brain\7f88a3b8-c6df-4079-bb5b-d198fc6b7efb\q5_computed_render.png`.

---

## 4. Métricas de Verificación

| Comprobación | Comando | Resultado |
|---|---|---|
| **ESLint** | `npx eslint .` en `dashboard` | **0 errores, 91 avisos** (reducido desde 129, meta $\le 100$ superada) |
| **TypeScript Typecheck** | `npx tsc --noEmit` en `dashboard` | **0 errores** (código de salida 0) |
| **Vitest Test Suite** | `npx vitest run` en `dashboard` | **10 archivos pasados (10/10), 62 tests en verde (62/62)** |
| **Diff del Worktree** | `git diff --stat` | **11 archivos modificados, 1 nuevo (+118 / -117)** |
| **Áreas Excluidas** | `crates/`, `dist`, `useNexus` | **0 modificaciones (intactas)** |
