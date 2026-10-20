# Guía Operativa: Smoke Test del Dashboard (`dashboard_smoke.mjs`)

Esta guía documenta el uso de la herramienta de verificación automatizada `scripts/dashboard_smoke.mjs`, diseñada para validar de manera hermética y no invasiva cualquier entrega de interfaz de usuario del Dashboard de Tylluan antes de su aceptación e integración.

---

## 1. Propósito y Alcance

`dashboard_smoke.mjs` automatiza las siguientes fases de verificación:
1. **Compilación Aislada:** Construye el dashboard en un directorio temporal del sistema operativo, garantizando que el `dist/` integrado en el kernel de producción permanezca 100% intacto.
2. **Servidor HTTP con Proxy Read-Only:** Levanta un servidor local en un puerto dinámico libre (`<puerto-libre>`) y redirige las peticiones a la API del kernel (`<puerto-kernel>`) en modo estrictamente de solo lectura (GET/HEAD y búsquedas idempotentes).
3. **Auditoría en Navegador Real (Chrome Headless vía CDP):** Controla una instancia headless de Google Chrome a través del protocolo nativo Chrome DevTools Protocol (sin dependencias de frameworks externos como Puppeteer o Playwright).
4. **Verificación de 7 Vistas Principales:**
   - `Overview` (Telemetría central y estado del sustrato)
   - `Coloquio` (Consola deliberativa y canales de chat)
   - `Decision` (Panel de Decision Service y shadow decisions)
   - `Audit` (Historial y verificación HMAC del audit trail)
   - `Contradictions` (Nodos en conflicto y consistencia SilvaDB)
   - `Federation` (Malla de nodos y peers P2P)
   - `TrustConsole (System)` (Subpestaña System de gobernanza y salud)
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

Al finalizar, la herramienta imprime una tabla resumen:

```text
=====================================================================================
  📊 RESUMEN DE RESULTADOS: AUDITORÍA DE LAS 7 VISTAS DEL DASHBOARD
=====================================================================================
Vista                    Estado    Consola     API Errs    Texto     Motivo
-------------------------------------------------------------------------------------
Overview                 ✅ OK      0           0           OK        —
Coloquio                 ✅ OK      0           0           OK        —
Decision                 ✅ OK      0           0           OK        —
Audit                    ✅ OK      0           0           OK        —
Contradictions           ✅ OK      0           0           OK        —
Federation               ✅ OK      0           0           OK        —
TrustConsole (System)    ✅ OK      0           0           OK        —
=====================================================================================
```

- **Código de Salida 0:** Todas las vistas pasaron los chequeos de renderizado, consola, red y textos.
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
