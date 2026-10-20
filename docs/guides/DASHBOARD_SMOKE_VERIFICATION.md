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
5. **Criterios de Aceptación por Vista:**
   - Renderizado íntegro del shell HTML (`#root` y `<header>`).
   - Cero errores en la consola de JavaScript (`console.error`).
   - Cero peticiones fallidas (códigos HTTP 4xx o 5xx) hacia `/api`.
   - Presencia de textos y elementos clave esperados en el DOM.
   - Generación de captura de pantalla en formato PNG almacenada fuera del repositorio.

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

Al finalizar, la herramienta imprime la tabla exhaustiva de las 26 vistas y subpestañas auditadas:

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
```

- **Código de Salida 0:** Todas las 26 vistas y subpestañas superaron los chequeos de renderizado, consola, red y textos.
- **Código de Salida 1:** Al menos una vista presentó errores en consola, peticiones 4xx/5xx o fallo de renderizado.

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
