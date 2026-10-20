# Guía Operativa: Smoke Test del Dashboard (`dashboard_smoke.mjs`)

Esta guía documenta el uso de la herramienta de verificación automatizada `scripts/dashboard_smoke.mjs`, diseñada para validar de manera hermética y no invasiva cualquier entrega de interfaz de usuario del Dashboard de Tylluan antes de su aceptación e integración.

---

## 1. Propósito y Alcance

`dashboard_smoke.mjs` automatiza las siguientes fases de verificación:
1. **Compilación Aislada:** Construye el dashboard en un directorio temporal del sistema operativo, garantizando que el `dist/` integrado en el kernel de producción permanezca 100% intacto.
2. **Servidor HTTP con Proxy Read-Only:** Levanta un servidor local en un puerto dinámico libre (`<puerto-libre>`) y redirige las peticiones a la API del kernel (`<puerto-kernel>`) en modo estrictamente de solo lectura (GET/HEAD y búsquedas idempotentes).
3. **Auditoría en Navegador Real (Chrome Headless vía CDP):** Controla una instancia headless de Google Chrome a través del protocolo nativo Chrome DevTools Protocol (sin dependencias de frameworks externos como Puppeteer o Playwright).
4. **Descubrimiento Dinámico de Todas las Pestañas y Subpestañas:**
   - Inspecciona el DOM de `App.tsx` y la barra lateral para identificar todas las pestañas principales (`Overview`, `Memory`, `Team`, `Guilds`, `Laboratory`, `Audit Trail`).
   - Descubre dinámicamente las barras de subnavegación dentro de cada módulo, alcanzando un total de 26 vistas y subpestañas navegables sin necesidad de listas estáticas manuales.
5. **Criterios Endurecidos de Aceptación por Vista (Lote AJ):**
   - **Renderizado íntegro del shell:** Confirmación de `#root`, `<header>` y `<main>`.
   - **Verificación de pestaña activa tras clic:** Confirmación de que el botón de la subpestaña o pestaña recibe `aria-pressed="true"`, `aria-selected="true"`, `aria-current="page"` o estilos de clase activos en el DOM.
   - **Contenido específico dentro de `<main>` (no en body):** Validación de palabras clave o selectores semánticos descubiertos del código fuente del componente para cada una de las 26 vistas.
   - **Detección de pantallas de error y fugas JS:** Inspección de `<main>` buscando ErrorBoundaries (`Tab Error`, `An error occurred`), banners de fallo en alertas rojas (`Failed to...`, `Error al cargar...`), y fugas directas de JS sin procesar (`undefined`, `NaN`, `[object Object]`).
   - **Exclusión de texto libre de usuario:** Se descartan explícitamente contenedores de mensajes, logs y bloques markdown (`.break-words`, `.whitespace-pre-wrap`, `pre`, `code`, `mark`, `[data-user-content]`, `.prose`, `details > div`) para que el detector solo analice el cromo estructural de la UI (cabeceras, badges, contadores, estados).
   - **Cero errores en consola:** Cero excepciones no capturadas ni llamadas a `console.error`.
   - **Cero peticiones API fallidas:** Cero respuestas HTTP 4xx o 5xx hacia `/api`, con arrastre de errores durante la fase de arranque (`bootApiErrors`).
   - **Capturas PNG:** Generación de captura de pantalla en formato PNG almacenada fuera del repositorio.

---

## 2. Requisitos Previos

- **Node.js:** Versión 20 o superior (cuenta con soporte nativo de `WebSocket`).
- **Navegador:** Google Chrome o Chromium instalado en el sistema.
- **Kernel de Tylluan:** Kernel en ejecución escuchando en su puerto canónico configurado (`<puerto-kernel>`).

---

## 3. Modo de Uso

### Ejecución Básica (Recomendada)
Para ejecutar la verificación completa contra el kernel local en ejecución:

```bash
node scripts/dashboard_smoke.mjs
```

El script detectará automáticamente:
- El token de autenticación desde `.tylluan-token`.
- La ruta del ejecutable de Chrome en el sistema operativo.
- Puertos libres disponibles tanto para el servidor web como para el puerto de depuración CDP.

### Suite de Pruebas Negativas Automáticas (`--negative-test`)
Para comprobar de forma automática que el arnés detecta anomalías, tolera texto libre y captura fugas JS:

```bash
node scripts/dashboard_smoke.mjs --negative-test
```

Esta suite ejecuta 3 subtests automatizados contra Mock Kernels aislados:
1. **Subtest 1 (Fallo HTTP 500):** Responde con error 500 en `/api/v1/guilds` y verifica que el smoke test falla con código 1 y marca `❌ FAIL`.
2. **Subtest 2 (Caso A — Inmunidad en Texto Libre):** Sirve mensajes en Coloquio conteniendo `undefined`, `NaN` y `[object Object]` en texto libre, y verifica que el smoke test pasa limpiamente con código 0 (`✅ OK`).
3. **Subtest 3 (Caso B — Detección de Fuga Real en Cromo):** Sirve métricas inválidas en `/api/v1/decide/shadow` provocando un valor `NaN` en una tarjeta de cromo estructural, y verifica que el smoke test falla con código 1, reporta `❌ FAIL` y nombra la causa `Fuga JS: NaN`.

### Pruebas Específicas de Discriminación de Fugas (`--test-leak-detection`)
Para ejecutar únicamente los subtests de discriminación de fugas JS (Caso A y Caso B):

```bash
node scripts/dashboard_smoke.mjs --test-leak-detection
```

### Filtrar por Pestaña Específica
Si se desea auditar únicamente una sección (por ejemplo, `Team` y todas sus 9 subpestañas):

```bash
node scripts/dashboard_smoke.mjs --tab Team
```

### Ejecución contra un Kernel Específico
Si el kernel se ejecuta en una URL o interfaz distinta:

```bash
node scripts/dashboard_smoke.mjs --kernel http://127.0.0.1:<puerto-kernel>
```

### Omitir el Paso de Build (Para iteraciones rápidas)
Si ya has compilado previamente el dashboard en una carpeta temporal:

```bash
node scripts/dashboard_smoke.mjs --skip-build --outDir /ruta/temporal/dist
```

### Directorio Personalizado para Capturas de Pantalla
Para almacenar las capturas PNG en una ubicación específica:

```bash
node scripts/dashboard_smoke.mjs --screenshotsDir /ruta/temporal/capturas
```

### Especificar la Ruta de Chrome
Si Chrome se encuentra en una ruta no estándar:

```bash
node scripts/dashboard_smoke.mjs --chrome /ruta/hacia/chrome
```

---

## 4. Interpretación de Resultados

Al finalizar, la herramienta imprime la tabla exhaustiva y endurecida de las 26 vistas y subpestañas auditadas:

```text
==============================================================================================================
  📊 RESUMEN DE RESULTADOS: AUDITORÍA ENDURECIDA DE TODAS LAS VISTAS Y SUBPESTAÑAS
==============================================================================================================
Vista                              Estado    Activa    Errs Main     Consola   API Errs  Contenido   Motivo
--------------------------------------------------------------------------------------------------------------
Overview > SUMMARY COCKPIT         ✅ OK      OK        0             0         0         OK          —
Overview > INTEROCEPTION           ✅ OK      OK        0             0         0         OK          —
Overview > SYSTEM CONFIG           ✅ OK      OK        0             0         0         OK          —
Memory > KNOWLEDGE GRAPH           ✅ OK      OK        0             0         0         OK          —
Memory > NODES                     ✅ OK      OK        0             0         0         OK          —
Memory > CONTRADICTIONS            ✅ OK      OK        0             0         0         OK          —
Team > FLEET STATUS                ✅ OK      OK        0             0         0         OK          —
Team > LOOP HEALTH                 ✅ OK      OK        0             0         0         OK          —
Team > WORK CONTRACTS              ✅ OK      OK        0             0         0         OK          —
Team > COMPETENCIES                ✅ OK      OK        0             0         0         OK          —
Team > CONFUSION                   ✅ OK      OK        0             0         0         OK          —
Team > HEATMAP                     ✅ OK      OK        0             0         0         OK          —
Team > COLOQUIO CHAT               ✅ OK      OK        0             0         0         OK          —
Team > AGENT COLLECTIVE            ✅ OK      OK        0             0         0         OK          —
Team > HITL DISPATCHES             ✅ OK      OK        0             0         0         OK          —
Guilds > GUILDS                    ✅ OK      OK        0             0         0         OK          —
Guilds > OUTPUTS LEDGER            ✅ OK      OK        0             0         0         OK          —
Guilds > CONNECTORS                ✅ OK      OK        0             0         0         OK          —
Guilds > MCP REGISTRY              ✅ OK      OK        0             0         0         OK          —
Guilds > FEDERATION                ✅ OK      OK        0             0         0         OK          —
Laboratory > LABORATORY            ✅ OK      OK        0             0         0         OK          —
Laboratory > VISION                ✅ OK      OK        0             0         0         OK          —
Laboratory > PLAN MODE             ✅ OK      OK        0             0         0         OK          —
Laboratory > PROJECT SKILLS        ✅ OK      OK        0             0         0         OK          —
Laboratory > BACKGROUND JOBS       ✅ OK      OK        0             0         0         OK          —
Audit Trail                        ✅ OK      OK        0             0         0         OK          —
==============================================================================================================
```

- **Código de Salida 0:** Todas las 26 vistas y subpestañas superaron los chequeos de renderizado, activación, contenido semántico en `<main>`, detección de errores y ausencia de fallos en consola y red.
- **Código de Salida 1:** Al menos una vista presentó errores en consola, peticiones 4xx/5xx, fallos de renderizado, fugas JS o falta del contenido esperado.

---

## 5. Verificación de Compilación Limpia Estilo Dockerfile (`--check-docker-build`)

Para evitar regresiones causadas por dependencias transitivas o declaraciones de tipos no declaradas (como la ausencia de `@types/node` en entornos aislados de Docker), la herramienta incluye una verificación idéntica a las líneas 25-27 de `Dockerfile`:

1. Copia `packages/` y `dashboard/` a un directorio temporal aislado **sin** `node_modules` ni `pnpm-lock.yaml`.
2. Ejecuta `npm install && npm run build` en `packages/tylluan-ui-core`.
3. Ejecuta `npm install && npm run build` en `dashboard` (ejecutando `tsc` estricto y el empaquetado de Vite).

### Uso:

```bash
# Ejecutar verificación de build Docker antes del smoke test
node scripts/dashboard_smoke.mjs --check-docker-build

# Ejecutar únicamente la verificación de compilación Docker en CI o pre-push
node scripts/dashboard_smoke.mjs --check-docker-build-only
```
