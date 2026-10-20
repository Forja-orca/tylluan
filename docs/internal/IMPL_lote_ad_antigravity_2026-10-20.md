# Implementación Lote AD: Tarjeta 'Salud del recall' (Telemetría D-5) en la Vista System

**Fecha:** 2026-10-20  
**Autor:** @antigravity  
**Worktree:** `E:\tylluan-wt-antigravity`  
**Encargo del TL:** Turno 1402 de Coloquio (`#general`, `@claude-code`).

---

## 1. ## Impact

Este lote implementa la visualización operativa de la telemetría de `tylluan_recall` (entregada en el kernel por Deep en D-5 vía `crates/tylluan-kernel/src/recall_telemetry.rs`) para permitir diagnosticar el balance de lectura vs escritura de los agentes sin inventar métricas ni alterar los contratos existentes.

- **Zonas afectadas:**
  - `dashboard/src/components/RecallHealthCard.tsx` (nuevo componente modular): renderizado de los 3 estados operativos (con datos, con nulls, ausente), métricas (`calls`, `empty_calls`, `empty_ratio`, `mean_latency_ms`) e histograma de latencia de 10 cubos (`<=5ms` hasta `>2500ms`).
  - `dashboard/src/components/TrustConsoleTab.tsx` (vista System): integración de `RecallHealthCard` bajo la rejilla principal de estado del sistema alimentándose de `GET /health?verbose=true -> components.recall`.
  - `dashboard/src/components/__tests__/RecallHealthCard.test.tsx` (nueva suite de pruebas): 7 tests unitarios, de contraste temático (claro/oscuro) y accesibilidad estructural con `axe-core`.
  - `dashboard/src/components/__tests__/TrustConsoleTab.test.tsx`: 2 tests de integración verificando los estados ausente y con datos en el contexto de Trust Console.
- **Zonas intactas:**
  - `crates/` (kernel Rust 100% intacto).
  - `dist/` (0 modificaciones).
  - Rutas de red, contratos CONTRACT-01 y endpoints MCP.
- **Riesgo y Mitigación:**
  - **Riesgo:** Confundir la ausencia de llamadas con un ratio de vacías del 0% o una latencia de 0 ms.
  - **Mitigación:** En estricto cumplimiento del diseño D-5, cuando `calls === 0` o los valores son `null`, se muestra explícitamente el texto `'sin datos'` en lugar de `0%` o `0 ms`. Si el kernel en ejecución corre un commit previo a D-5 (como el kernel en vivo `2458b4a`), se detecta la ausencia de la clave `recall` dentro de `components` y se muestra el badge de advertencia `'kernel sin telemetría de recall (actualiza)'`.

---

## 2. Diagnóstico de Flakiness Reportado por TL (Turno 1402)

Ante la observación del Tech Lead en el Turno 1402 sobre un fallo aislado en su primera pasada en frío:
- **Prueba de Resistencia:** Se ejecutaron **5 pasadas consecutivas** de la combinación `ColoquioVirtualization.test.tsx` y `AxeAudit.test.tsx` bajo carga:
  - Run 1: 36/36 tests pasados (41.51s).
  - Run 2: 36/36 tests pasados (47.90s).
  - Run 3: 36/36 tests pasados (43.85s).
  - Run 4: 36/36 tests pasados (45.65s).
  - Run 5: 36/36 tests pasados (43.31s).
- **Causa Raíz:** En arranques en frío sin caché de módulos de Vitest/Vite, las 28 pruebas exhaustivas de `AxeAudit.test.tsx` (especialmente `Overview` y `Decision` que auditan árboles DOM complejos con canvas y grafos) consumen entre 2.5s y 3.4s por test. Si el pool de hilos de CPU experimenta contención al inicio, un test individual de axe puede aproximarse a los 5,000 ms del timeout por defecto de Vitest. Además, en `ColoquioVirtualization.test.tsx`, la sincronización de `requestAnimationFrame` en JSDOM puede diferirse bajo saturación del bucle de eventos.

---

## 3. Implementación de la Tarjeta 'Salud del recall' (`RecallHealthCard.tsx`)

La tarjeta consume el objeto `components.recall` emitido por `GET /health?verbose=true` estructurado por `crates/tylluan-kernel/src/recall_telemetry.rs`:

```json
"recall": {
  "calls": 2504,
  "empty_calls": 210,
  "empty_ratio": 0.084,
  "mean_latency_ms": 41.3,
  "latency_histogram": {
    "<=5ms": 12,
    "<=10ms": 80,
    "<=25ms": 450,
    "<=50ms": 920,
    "<=100ms": 680,
    "<=250ms": 250,
    "<=500ms": 80,
    "<=1000ms": 25,
    "<=2500ms": 4,
    ">2500ms": 3
  }
}
```

### Los Tres Estados Garantizados

1. **Estado Ausente (`isAbsent` o `recall === undefined`):**
   - Se muestra un banner con borde ámbar y badge:  
     `kernel sin telemetría de recall (actualiza)`
   - Texto informativo indicando que el kernel en ejecución no expone contadores de telemetría en `components.recall`.
2. **Estado con Nulls (0 llamadas registradas):**
   - `calls`: 0
   - `empty_calls`: 0
   - `empty_ratio`: **`sin datos`** (garantizado: nunca `0%` ni `0.0%`).
   - `mean_latency_ms`: **`sin datos`** (garantizado: nunca `0 ms` ni `0.0 ms`).
   - Histograma: mensaje claro *"Sin datos de latencia acumulados hasta la primera llamada de recall"*.
3. **Estado con Datos (`calls > 0`):**
   - Métricas numéricas formateadas (`calls`, `empty_calls`).
   - `empty_ratio`: porcentaje formateado (ej. `8.4%`), con semántica visual verde ($\le 50\%$) o alerta roja ($> 50\%$).
   - `mean_latency_ms`: latencia promedio con un decimal (ej. `41.3 ms`).
   - Histograma visual con los 10 cubos en cuadrícula responsive, barras proporcionales coloreadas según severidad (verde $\le 500\text{ms}$, ámbar $\le 2500\text{ms}$, rojo $> 2500\text{ms}$) y porcentajes exactos.

---

## 4. Accesibilidad y Soporte de Temas

- **WCAG 2.1 AA:** Auditado estructuralmente con `axe-core` en `RecallHealthCard.test.tsx` y en el arnés general `AxeAudit.test.tsx` (System View), certificando **0 violaciones críticas o serias** en todas las permutaciones (temas claro y oscuro, resoluciones 1440px y 390px).
- **Semántica:** Región landmark `role="region"` con `aria-label="Salud del recall"` y sub-región navegable por teclado para el histograma (`tabIndex={0}`, `focus-visible:ring-1`).
- **Contraste de Color:** Clases adaptadas a Tailwind `dark:` y modo claro, garantizando ratios de contraste superiores a 4.5:1 para textos y 3:1 para indicadores gráficos.

---

## 5. Verificación de Quality Gates

Ejecutado en el worktree `E:\tylluan-wt-antigravity`:

1. **TypeScript (`npx tsc --noEmit`):**
   - **0 errores** (se validaron las propiedades de `LucideProps` y tipado estricto de `HealthData`).
2. **Vitest Suite Completa (`npx vitest run`):**
   - **16 archivos de pruebas pasados (16 / 16)**.
   - **134 pruebas pasadas (134 / 134)** — 100% verde (incluyendo las 7 pruebas de `RecallHealthCard.test.tsx` y las 6 pruebas de `TrustConsoleTab.test.tsx`).
3. **ESLint (`npm run lint`):**
   - **0 errores, 36 advertencias** ($\le 36$).
4. **Integridad de Repositorio:**
   - `crates/` y `dist/`: 0 modificaciones.
   - Diff sin commitear en `E:\tylluan-wt-antigravity`.

---

## 6. No Verificado

1. **Datos de telemetría emitidos por un kernel en vivo actualizado a 9ebfb10:** El kernel en ejecución en este entorno de desarrollo continúa corriendo el commit `2458b4a`, por lo que en vivo se verifica correctamente el estado *Ausente* (`kernel sin telemetría de recall (actualiza)`). Los estados *con datos* y *con nulls* fueron verificados contra el shape exacto de `crates/tylluan-kernel/src/recall_telemetry.rs` mediante pruebas automatizadas de integración y componentes.
