# Informe de Implementación y Verificación: Lote AJ — Endurecimiento del Smoke Test del Dashboard

**Fecha:** 2026-10-20  
**Agente:** Antigravity (Frontend / Dashboard & Verificación Hermética)  
**Destinatarios:** `@claude-code` (Tech Lead), `@jose`, `@deep`  
**Referencia:** Lote AJ (Evolución de `scripts/dashboard_smoke.mjs` como criterio riguroso de aceptación)  
**Worktree:** `E:\tylluan-wt-antigravity`  
**Base commit:** `8001d23` (sincronizado con `origin/main`)

---

## 1. Resumen Ejecutivo y Alcance del Lote AJ

El objetivo del **Lote AJ** ha sido elevar `scripts/dashboard_smoke.mjs` de un comprobador de navegación básica a un **criterio riguroso y hermético de aceptación de UI**.

Tras la devolución en Turno 1424 por parte del Tech Lead `@claude-code` (debido a que el detector de fugas JS leía cadenas legítimas como `"undefined"` o `"NaN"` dentro del cuerpo de los mensajes de chat en Coloquio), se implementó la discriminación precisa entre cromo estructural y contenido de usuario, junto con una suite completa de pruebas automáticas en el arnés.

Se han implementado cuatro capas de aserción sin añadir dependencias externas a `package.json` (operando exclusivamente sobre Node.js stdlib y Chrome DevTools Protocol nativo):

1. **Validación de contenido propio dentro de `<main>` (no en body):**
   - Catálogo `VIEW_SPECS` con palabras clave semánticas descubiertas directamente de los componentes de las 26 vistas.
   - Evaluación de que dentro del elemento `<main>` (aislado del sidebar y header) exista el contenido específico correspondiente.
2. **Detección de pantallas de error y fugas JS con exclusión de texto libre:**
   - Detección de React Error Boundaries (`Tab Error`, `An error occurred`).
   - Detección de títulos de fallo general (`Something went wrong`).
   - Detección de banners de error de red o carga en alertas (`Failed to...`, `Error al cargar...`).
   - Detección de fugas de variables JavaScript no bindeadas (`undefined`, `NaN`, `[object Object]`).
   - **Exclusión de texto libre de usuario:** Se descartan explícitamente contenedores de mensajes, logs y bloques markdown (`.break-words`, `.whitespace-pre-wrap`, `pre`, `code`, `mark`, `[data-user-content]`, `.prose`, `details > div`) para que el detector solo analice el cromo estructural de la UI (cabeceras, badges, contadores, estados).
3. **Verificación de activación real tras clic:**
   - Confirmación en el DOM de que el elemento clicado recibe `aria-pressed="true"`, `aria-selected="true"`, `aria-current="page"` o clases CSS activas del sistema de diseño (`bg-emerald-500/10`, `text-emerald-400`, `bg-amber-500/10`, etc.).
4. **Suite de Pruebas Negativas Automáticas en el Arnés (`--negative-test`, `--test-leak-detection`):**
   - **Subtest 1 (Fallo HTTP 500):** Mock Kernel responde con HTTP 500 en `/api/v1/guilds`; el script certifica que sale con código 1, reporta `❌ FAIL` y nombra la ruta causante.
   - **Subtest 2 (Caso A — Inmunidad en Texto Libre):** Mock Kernel sirve en Coloquio mensajes conteniendo `"undefined NaN [object Object]"`; el script verifica que sale con código 0 y `✅ OK` sin falsos positivos.
   - **Subtest 3 (Caso B — Detección de Fuga Real en Cromo):** Mock Kernel sirve métricas inválidas en `/api/v1/decide/shadow` provocando un valor `NaN` en una tarjeta de cromo estructural; el script certifica que sale con código 1, reporta `❌ FAIL` y nombra la causa `Fuga JS: NaN`.

---

## 2. Resultados de la Auditoría Endurecida contra el Kernel Real (:47004)

Ejecutado con el comando `node scripts/dashboard_smoke.mjs` contra el kernel en ejecución en su puerto canónico:

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

📁 26 Capturas PNG almacenadas en: C:\Users\FoRJa\AppData\Local\Temp\tylluan-smoke-shots-KxTsr5

🎉 ¡TODAS LAS 26 VISTAS Y SUBPESTAÑAS SUPERARON EL SMOKE AUDIT CON 0 ERRORES!
```

---

## 3. Resultados de la Suite de Pruebas Negativas y Discriminación JS

Ejecutado con el comando `node scripts/dashboard_smoke.mjs --negative-test`:

```text
🧪 [SUITE DE PRUEBAS NEGATIVAS AUTOMÁTICAS]
   Verificando discriminación de fallos, tolerancia a texto libre y detección de fugas JS...
       Compilando dashboard para las pruebas negativas...

[1/3] Verificando detección de fallo HTTP 500 en /api/v1/guilds...
       ✅ Subtest 1 superado: Código 1 y fallo 500 detectado.

[2/3] Verificando Caso A: Inmunidad a falso positivo por texto libre en Coloquio...
       ✅ Subtest 2 superado: Código 0 y Coloquio Chat inmune a falsos positivos.

[3/3] Verificando Caso B: Detección de fuga real JS (NaN) en cromo de interfaz...
       ✅ Subtest 3 superado: Código 1 y Fuga JS: NaN detectada y nombrada.

=====================================================================================
  ✅ SUITE COMPLETA DE PRUEBAS NEGATIVAS Y DISCRIMINACIÓN JS SUPERADA CON ÉXITO:
  - Subtest 1: Fallo HTTP 500 detectado con salida 1 y estado ❌ FAIL.
  - Subtest 2 (Caso A): Texto libre con undefined/NaN/[object Object] inmune a falsos positivos (salida 0).
  - Subtest 3 (Caso B): Fuga JS en cromo estructural capturada con salida 1 y causa "Fuga JS: NaN".
=====================================================================================
```

---

## 4. Estado de los Quality Gates

Todos los comandos fueron ejecutados en `E:\tylluan-wt-antigravity`:

| Gate / Comprobación | Comando | Resultado | Estado |
|---|---|---|---|
| **TypeScript (dashboard)** | `npx tsc --noEmit` | 0 errores | ✅ PASS |
| **Vitest (dashboard)** | `npx vitest run` | 19 suites pasadas, 151/151 tests verdes | ✅ PASS |
| **ESLint (dashboard)** | `npm run lint` | 0 errores, 36 warnings (baseline previo) | ✅ PASS |
| **Smoke Audit (26 vistas)** | `node scripts/dashboard_smoke.mjs` | 26/26 vistas en ✅ OK, 0 errs en `<main>`, 0 API errs, 0 consola | ✅ PASS |
| **Suite Pruebas Negativas** | `node scripts/dashboard_smoke.mjs --negative-test` | 3/3 subtests superados (500 -> 1, Caso A -> 0, Caso B -> 1) | ✅ PASS |
| **Fugas JS Específico** | `node scripts/dashboard_smoke.mjs --test-leak-detection` | 2/2 subtests superados (Caso A -> 0, Caso B -> 1) | ✅ PASS |
| **Doc Reality** | `bash scripts/check_docs_reality.sh` | 0 errores, 0 drift de puertos o rutas | ✅ PASS |
| **Doc Links** | `python scripts/check_doc_links.py` | 222 enlaces relativos resueltos correctamente | ✅ PASS |
| **Integridad del Workspace** | `git status` | Solo `scripts/dashboard_smoke.mjs` y documentación modificados. `crates/` y `dashboard/dist` intactos. | ✅ PASS |

---

## 5. Impacto de Arquitectura y No-Depredación (ADR-013)

### ## Impact
- **Módulos tocados:** `scripts/dashboard_smoke.mjs` y documentación en `docs/guides/DASHBOARD_SMOKE_VERIFICATION.md` y `docs/internal/IMPL_lote_aj_antigravity_2026-10-20.md`.
- **Riesgo:** Nulo para el kernel de Rust (`crates/`) y para el runtime del dashboard en producción (`dashboard/dist`).
- **Soberanía y dependencias:** Cero dependencias npm añadidas. Opera 100% en Node nativo y CDP sobre Chrome.
- **Invariantes respetados:**
  - Puerto del kernel intacto y protegido por proxy de solo lectura.
  - No-depredación: Ningún archivo de otros agentes ha sido modificado.
  - Trabajo exclusivo dentro del worktree `E:\tylluan-wt-antigravity`.

---

## 6. Límites y lo No Verificado

- **Interacción con mutaciones de datos:** La herramienta deliberadamente NO prueba acciones de escritura (POST a guilds, POST a dispatch, creación de contratos) para preservar la inmutabilidad y seguridad de la base de datos de producción durante el smoke test.
- **Entornos sin Google Chrome instalado:** Requiere de un binario Chrome/Chromium disponible en el host. Si no se encuentra en las rutas estándar de Windows/Linux/macOS, debe especificarse vía `--chrome`.
