# Informe de Implementación — AG-12 Paso 5: Textos en Español, Salvia y Telemetría Viva

**Fecha:** 2026-10-28  
**Agente:** @antigravity (Antigravity)  
**Worktree:** `E:\tylluan-wt-antigravity`  
**Base:** `origin/main` (`ea963b1` tras PUSH184)  
**Estado:** Diff limpio sin commitear, listo para revisión e integración.  
**Aviso:** No verificado por el Tech Lead.

---

## 1. Alcance y Directivas Cumplidas (AG-12 Paso 5)

En atención a las directivas asignadas en el Turno 1987 y ratificadas en el Turno 1993:

1. **Eliminación Total de Textos Residuales en Inglés:**
   - `dashboard/src/components/WelcomeEmptyState.tsx`:
     - Título: `Bienvenido a Tylluan` (eliminado `"Welcome to"`).
     - Tarjeta checklist: `Lista de Inicio` (reemplaza a `"Onboarding Checklist"`).
     - 5 ítems traducidos a español con coherencia terminológica: `Instalar Tylluan`, `Crear Perfil de Agente`, `Motor de Embeddings`, `Conectar Cliente MCP`, `Primera Consulta / Acción`.
     - Placeholder y botón de acción: `¿Qué es Tylluan?` y `Consultar` (reemplaza a `"Ask"`).
   - `dashboard/src/components/ModelsRoleAssignment.tsx`:
     - Encabezado: `Asignación de Roles Cognitivos` (reemplaza a `"Cognitive Role Model Assignment"`).
     - Roles cognitivos: `Inferencia Principal (Agente Primario)`, `Razonador Nocturno (Coordinador)`, `Enrutamiento e Intención`, `Análisis Visual (Modelo de Visión)`.
     - Avisos de estado en disco: `✓ En disco (<tam> MB)` y `⚠️ No detectado` (eliminado `"NOT DETECTED"` / `"Not detected"`).
     - Cuadro de confirmación y advertencias traducido a español puro.
   - `dashboard/src/components/DeviceStatusBadge.tsx`:
     - Estado sin datos: `No detectado` con tooltips contextuales en español.
   - Tests de regresión adaptados:
     - `dashboard/src/components/__tests__/OverviewTab.test.tsx`: Aserciones actualizadas a `Bienvenido a` y `Lista de Inicio`.
     - `dashboard/src/components/__tests__/WiringAuditFixes.test.tsx`: Aserción actualizada a `Conectar Cliente MCP`.

2. **Limpieza del Ámbar Decorativo (Salvia y Semántica Estricta):**
   - En `ModelsRoleAssignment.tsx`:
     - Icono de cabecera cambiado de `text-amber-400` a `text-primary` (salvia soberano).
     - Botón de guardado cambiado a fondo y texto salvia (`bg-primary/20 text-primary`).
     - Fondos adaptados a tokens CSS neutrales (`bg-card`, `bg-background`, `border-border`).
     - El color ámbar se reservó de forma estricta y exclusiva a avisos de degradación/ausencia (`bg-amber-500/10 text-amber-600 dark:text-amber-400`).
   - En `OverviewTab.tsx`:
     - `History` en `Sovereign Activity Stream`: cambiado de `text-amber-400` decorativo a `text-primary`.
     - `HITL Gateway` badge: cambiado de `text-amber-400` a `text-primary`.
     - `Guild Utilization` icono: cambiado de `text-amber-400` a `text-primary`.
     - Estados semánticos de advertencia (Health < 80%, latencia alta, aprobaciones bloqueadas) preservan el ámbar conforme a la especificación de diseño soberano.

3. **Garantía de Telemetría Inmediata en Conexión:**
   - En `packages/tylluan-ui-core/src/hooks/useNexus.tsx`:
     - Se añadió la llamada inmediata a `bridge.health_detailed()` durante el montaje inicial del hook (previamente diferida 30s al primer tick del `setInterval`), asegurando que la telemetría detallada del kernel se sincronice en el arranque en frío.

---

## 2. Resultados de Verificación Técnica

1. **Vitest (`pnpm test --run`):**
   - **16 archivos de tests evaluados**.
   - **131 tests pasando al 100% (0 fallos)**.
   - Verificaciones estructurales y de accesibilidad Axe-Core en verde en todos los viewports (1440px y 390px).

2. **Compilación y Linter (`pnpm build` & `pnpm lint`):**
   - `pnpm run lint`: **0 errores** (18 advertencias preexistentes).
   - `pnpm run build`: **Bundle generado exitosamente en 11.20s** (`tsc` 0 errores, `vite build` OK).
   - `npm --prefix packages/tylluan-ui-core run build`: **0 errores** (`tsc` limpio).

3. **Verificación de Crate GUI (`cargo check -p tylluan-gui`):**
   - Compilación exitosa en **0.46s** (0 errores, 0 warnings).

4. **Capturas de Pantalla:**
   - Las capturas actualizadas de las vistas se encuentran disponibles en `docs/internal/screenshots/` listas para inspección manual y validación por el Tech Lead.

---

## 3. Impact

- **Archivos Modificados:**
  - `dashboard/src/components/DeviceStatusBadge.tsx`
  - `dashboard/src/components/ModelsRoleAssignment.tsx`
  - `dashboard/src/components/OverviewTab.tsx`
  - `dashboard/src/components/WelcomeEmptyState.tsx`
  - `dashboard/src/components/__tests__/OverviewTab.test.tsx`
  - `dashboard/src/components/__tests__/WiringAuditFixes.test.tsx`
  - `packages/tylluan-ui-core/src/hooks/useNexus.tsx`
- **Riesgo:** Bajo. Modificaciones puramente de interfaz de usuario, traducción semántica, corrección de acentos cromáticos y sincronización inicial de telemetría. Sin cambios en APIs del kernel, firmas MCP ni estructuras de datos en base de datos.
