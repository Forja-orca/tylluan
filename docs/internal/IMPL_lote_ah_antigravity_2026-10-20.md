# Informe de Implementación: LOTE AH — Herramienta Reutilizable `scripts/dashboard_smoke.mjs`

- **Autor:** @antigravity
- **Fecha:** 2026-10-20
- **Worktree:** `E:\tylluan-wt-antigravity`
- **Commit Base:** `d4dc9d1` (`origin/main`, push 64)
- **Asignación:** Lote AH emitido por Tech Lead @claude-code en Coloquio Turno 1414 y ampliado en Turno 1416 (incorporación de comprobación de compilación limpia estilo Dockerfile:25-27).

---

## 1. Resumen Ejecutivo y Objetivos del Lote AH

En los Turnos 1414 y 1416, el Tech Lead @claude-code solicitó la creación de una herramienta hermética y reutilizable para auditar cualquier entrega de interfaz de usuario antes de su integración:
1. Crear el script `scripts/dashboard_smoke.mjs` en Node.js (ESM), sin dependencias externas fuera de la biblioteca estándar de Node y el protocolo nativo de Chrome DevTools (CDP).
2. Construir el dashboard en un directorio temporal aislado (`os.tmpdir()`), preservando intacto el `dist/` del kernel.
3. Servir la aplicación en un puerto libre local con proxy estricto de solo lectura hacia `--kernel` (por defecto `http://127.0.0.1:47004`).
4. Auditar las 7 vistas principales del dashboard en Chrome Headless (`1440x900`):
   - `Overview`
   - `Coloquio`
   - `Decision`
   - `Audit`
   - `Contradictions`
   - `Federation`
   - `TrustConsole (System)`
5. Comprobar para cada vista:
   - Renderizado del shell HTML (`#root` y `<header>`).
   - 0 errores en consola de JavaScript (`console.error`).
   - 0 peticiones HTTP 4xx/5xx hacia `/api`.
   - Detección de textos y elementos clave esperados en el DOM.
   - Guardado de capturas de pantalla PNG en directorio temporal FUERA del repositorio.
6. Imprimir tabla de resultados vista -> OK/FAIL -> motivo y salir con código 1 si algo falla y 0 en éxito.
7. Incorporar comprobación de compilación limpia estilo Dockerfile (`--check-docker-build` / `--check-docker-build-only`): emula las líneas 25-27 de `Dockerfile` en un entorno efímero sin `node_modules` ni `pnpm-lock.yaml`, ejecutando `npm install && npm run build` para asegurar que `tsc` compila limpio y sin dependencias no declaradas.
8. Documentar en `docs/` el uso sin puertos literales en formato `NNNN` (usando `<puerto>`).

---

## 2. Arquitectura de `scripts/dashboard_smoke.mjs`

1. **Cero Dependencias Externas (Pure Node 20+):**
   - Utiliza la clase global nativa `WebSocket` introducida en Node.js para interactuar directamente con Chrome DevTools Protocol (`Page.*`, `Runtime.*`, `Network.*`).
   - Elimina la necesidad de dependencias pesadas como Puppeteer o Playwright.
2. **Construcción Hermética:**
   - Compila vía `npx vite build --outDir <tempDist>` en un directorio efímero generado por `fs.mkdtempSync()`.
   - Compila previamente `@tylluan/ui-core` si es necesario.
   - El árbol de trabajo y el `dist` del kernel se mantienen 100% inalterados.
3. **Servidor HTTP con Proxy Read-Only Seguro:**
   - Detecta un puerto libre con `net.createServer().listen(0)`.
   - Sirve activos estáticos con encabezados MIME correctos y fallback SPA a `index.html`.
   - Redirige llamadas a `/api/` y `/health` hacia el kernel destino, inyectando el token Bearer de `.tylluan-token`.
   - **Regla de Soberanía:** Bloquea cualquier método mutante (`POST`, `PUT`, `DELETE`), permitiendo únicamente `GET`, `HEAD`, `OPTIONS` y `POST` exclusivamente para búsquedas idempotentes (`/api/v1/memory/search`).
4. **Driver de Navegador Headless (Chrome CDP):**
   - Autodetección del binario de Chrome en Windows, macOS y Linux.
   - Instanciación aislada con perfil temporal (`--user-data-dir`) en modo `--headless=new`.
   - Viewport fijo en `1440x900`.
   - Captura de pantalla nativa (`Page.captureScreenshot`) en PNG almacenada en carpeta temporal del sistema operativo.
5. **Verificación de Compilación Limpia Estilo Docker (`--check-docker-build` / `--check-docker-build-only`):**
   - Reproduce exactamente la secuencia de `Dockerfile` (líneas 25-27).
   - Copia en un directorio temporal aislado (`os.tmpdir()`) `packages/` y `dashboard/` omitiendo carpetas `node_modules/`, `dist/`, `.git/` y el archivo `pnpm-lock.yaml`.
   - Ejecuta `npm install && npm run build` en `packages/tylluan-ui-core` y a continuación en `dashboard`.
   - Garantiza que `tsc` compila con 0 errores bajo npm limpio y detecta de inmediato cualquier dependencia o declaración de tipos faltante (como `@types/node`).

---

## 3. Salida Real de la Ejecución contra Kernel de Producción

Ejecutado con:
```bash
node scripts/dashboard_smoke.mjs
```

**Log y tabla resultante obtenida en vivo:**

```text
===========================================================================
  🔍 TYLLUAN DASHBOARD SMOKE AUDIT — HERRAMIENTA DE VERIFICACIÓN
===========================================================================
Kernel Destino:     http://127.0.0.1:47004
Token Autenticación: (cargado desde .tylluan-token)
Binario de Chrome:   C:\Program Files\Google\Chrome\Application\chrome.exe

📦 [1/4] Compilando el dashboard en directorio temporal aislado...
       Destino temporal: C:\Users\FoRJa\AppData\Local\Temp\tylluan-smoke-dist-Pif7n2
vite v5.4.21 building for production...
transforming...
✓ 3709 modules transformed.
rendering chunks...
computing gzip size...
✓ built in 22.85s
       ✅ Compilación completada con éxito.

🌐 [2/4] Levantando servidor HTTP local con proxy de solo lectura...
       Dashboard servido en: http://127.0.0.1:<puerto-libre>/
       Proxying hacia:       http://127.0.0.1:47004 (SOLO GET)

🖥️  [3/4] Iniciando Chrome Headless...
       Conectado a Chrome CDP en puerto <puerto-cdp>

🧪 [4/4] Ejecutando pruebas sobre las 7 vistas principales...

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

📁 Capturas PNG almacenadas en: C:\Users\FoRJa\AppData\Local\Temp\tylluan-smoke-shots-dtGcKE

🎉 ¡TODAS LAS VISTAS SUPERARON EL SMOKE AUDIT CON 0 ERRORES!
```

### Ejecución de Comprobación Limpia Estilo Docker (`--check-docker-build-only`):

```bash
node scripts/dashboard_smoke.mjs --check-docker-build-only
```

```text
🐳 Verificando compilación limpia estilo Dockerfile:25-27...
       Directorio temporal limpio: C:\Users\FoRJa\AppData\Local\Temp\tylluan-docker-check-P3lG9r
       [1/2] Copiando packages/ y ejecutando npm install && npm run build en tylluan-ui-core...
             ✅ packages/tylluan-ui-core compiló limpio.
       [2/2] Copiando dashboard/ y ejecutando npm install && npm run build...
             ✅ dashboard (tsc + vite build) compiló limpio.

✅ Comprobación Dockerfile:25-27 superada con éxito en 148.2s (0 errores de tsc ni dependencias faltantes).

🎉 Comprobación --check-docker-build-only completada con éxito.
```

---

## 4. Documentación Generada

Se redactó la guía operativa en:
`docs/guides/DASHBOARD_SMOKE_VERIFICATION.md`

Cumple estrictamente con la regla anti-drift de documentación:
- No cita puertos literales en formato `NNNN` (utiliza placeholders semánticos como `<puerto-kernel>`, `<puerto-libre>`).
- Detalla los comandos para ejecución por defecto, kernel remoto, omisión de compilación (`--skip-build`) y especificación de directorio para capturas PNG.

---

## 5. Verificación de Quality Gates

1. **Compilación de paquetes y TypeScript:**
   - `packages/tylluan-ui-core`: `npm run build` $\rightarrow$ OK (tsc limpio).
   - `dashboard`: `npx tsc --noEmit` $\rightarrow$ **0 errores**.
2. **Suite de Pruebas (`vitest`):**
   - `npx vitest run` $\rightarrow$ **19 suites pasadas (19/19), 151 tests pasados (151/151)** en verde (100% de éxito).
3. **Linter ESLint (`dashboard`):**
   - `npm run lint` $\rightarrow$ **0 errores, 36 advertencias** (baseline exacto preservado, $\le 36$).
4. **Integridad del Repositorio:**
   - `crates/`: **100% intacto**.
   - `dashboard/dist`: **100% intacto**.
   - Archivos añadidos:
     - `scripts/dashboard_smoke.mjs`
     - `docs/guides/DASHBOARD_SMOKE_VERIFICATION.md`
     - `docs/internal/IMPL_lote_ah_antigravity_2026-10-20.md`

---

## 6. Impacto Arquitectural (ADR-013)

```markdown
## Impact
- **Riesgo Operativo:** Cero. La herramienta corre de forma externa en Node y solo realiza consultas idempotentes seguras de lectura al kernel en ejecución.
- **Calidad de Entregas:** Permite al Tech Lead y a cualquier agente verificar visual y funcionalmente el dashboard compilado en producción en < 30 segundos antes de integrar PRs.
- **Soberanía y Dependencias:** Implementado exclusivamente con APIs nativas de Node 20+ y Chrome CDP, sin agregar nuevas dependencias npm a package.json.
```

---

## 7. Qué NO se ha verificado (Honestidad Técnica)

- **Ejecución en entornos sin servidor gráfico ni Google Chrome instalado:** Si el runner de CI carece de un binario de Chrome o Chromium disponible en el PATH o en rutas estándar, el script fallará indicando la necesidad de instalar Chrome o proveer `--chrome <ruta>`.
- **Interacciones profundas complejas en el canvas de tldraw:** El smoke test valida que el workspace de ColoquioCanvasWorkspace carga, inicializa sus primitivas y renderiza sin errores de consola ni fallos de red; no ejecuta trazos interactivos de dibujo vectorial dentro del canvas de tldraw.
