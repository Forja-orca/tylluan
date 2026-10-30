# Informe de Implementación: Tarea AG-19

**Fecha:** 2026-10-30  
**Autor:** @antigravity  
**Tarea:** AG-19 (Corrección de reproducibilidad en construcción de Dockerfile para Dashboard)  
**Base:** `origin/main` en `a06fa08`

---

## 1. Contexto y Causa Raíz
El paso de GitHub Actions `Build Docker image` (job `Docker Smoke`) fallaba en `a06fa08` (run ID `41107676747`) con error HTTP 404 al intentar resolver paquetes volátiles mediante `npm install` sin lockfile en el directorio `dashboard/`.

Dado que `dashboard/package-lock.json` no está versionado (el proyecto fija dependencias mediante `dashboard/pnpm-lock.yaml`), el uso de `npm install` violaba el principio de builds reproducibles y la regla arquitectónica de la flota.

---

## 2. Cambios Realizados
Se modificó estrictamente `Dockerfile` (fase de construcción de frontend):
1. **Habilitación de pnpm oficial vía Corepack:**  
   Se añadió `corepack enable` tras instalar Node.js 22. Corepack activa automáticamente la versión fija definida en `dashboard/package.json` (`"packageManager": "pnpm@10.30.3"`).
2. **Build determinista de UI Core:**  
   En `packages/tylluan-ui-core` (que sí cuenta con `package-lock.json` versionado), se reemplazó `npm install` por `npm ci` para garantizar coherencia reproducible.
3. **Build determinista del Dashboard:**  
   En `dashboard/`, se reemplazó `npm install && npm run build` por `pnpm install --frozen-lockfile && pnpm run build`.

---

## 3. Verificaciones Realizadas

### A. Simulación fuera del repositorio
En un directorio temporal limpio (`E:\sim_root`), copiando exclusivamente el código fuente de `packages/` y `dashboard/` sin directorios `node_modules` ni artefactos previos:
1. `packages/tylluan-ui-core`: `npm ci` y `npm run build` completados con éxito (exit code 0).
2. `dashboard`: `pnpm install --frozen-lockfile` (453 paquetes resueltos y enlazados sin errores, 0 añadidos fuera del lockfile).
3. `dashboard`: `pnpm run build` completado con éxito (exit code 0, 1866 módulos transformados, dist generado en 5,21 s).

### B. Pruebas de calidad en el worktree (`E:\tylluan-wt-antigravity\dashboard`)
- **Lint:** `pnpm run lint` -> **0 errores** (36 warnings menores preexistentes de tipado).
- **TypeScript:** `tsc` -> **0 errores**.
- **Tests unitarios:** `pnpm test` (Vitest) -> **139/139 tests pasando** (23 test files verdes).
- **Compilación de producción:** `pnpm run build` -> **Exit 0**, bundles generados sin discrepancias.

---

## Impact
- **Ficheros modificados:** `Dockerfile` (+3/-2 líneas).
- **Riesgo:** Bajo. Se alinea el contenedor Docker con la misma estrategia determinista que ya utiliza CI en `ci.yml`.
- **Efectos colaterales:** Ninguno en dependencias o lockfiles (no se alteró ningún `package.json` ni `pnpm-lock.yaml`).

---

## No verificado
- **Contenedores Docker locales:** Conforme a las exclusiones de la tarea, no se ejecutó `docker build` en la máquina local (Docker Desktop no se utiliza). La validación real del contenedor completo será certificada por el workflow de CI `Docker Smoke` en GitHub Actions una vez empujado el cambio.
- No verificado en arquitecturas ARM64 nativas (el build de cross-compilación de ONNX runtime se delega al pipeline de CI).
