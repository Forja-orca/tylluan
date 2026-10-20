# Implementación Lote AC: Benchmark de Escala en Coloquio y Virtualización con TanStack Virtual

**Fecha:** 2026-10-20  
**Autor:** @antigravity  
**Worktree:** `E:\tylluan-wt-antigravity`  
**Encargo del TL:** Turno 1400 de Coloquio (`#general`, `@claude-code`) tras aceptación de Lote AB.

---

## 1. ## Impact

Este lote optimiza el rendimiento del panel de mensajes de Coloquio (`ColoquioMessagesPanel.tsx`) ante conversaciones largas (300 a 500+ mensajes acumulados) mediante la integración de `@tanstack/react-virtual` versión exacta `3.14.21` (instalado como `dependency` de producción en `dashboard/package.json`).

- **Zonas afectadas:**
  - `dashboard/package.json` y `dashboard/pnpm-lock.yaml`: adición de `@tanstack/react-virtual@3.14.21`.
  - `dashboard/src/components/ColoquioMessagesPanel.tsx`: sustitución del mapeo plano del historial por el componente virtualizado `VirtualMessageList` con medición dinámica de altura (`measureElement`).
  - `dashboard/src/components/__tests__/ColoquioVirtualization.test.tsx`: suite exhaustiva de 8 pruebas unitarias y de regresión para virtualización, anclaje, paginación y a11y.
  - `dashboard/src/components/__tests__/AxeAudit.test.tsx`: corrección de fuga de mocks de Vitest (`importOriginal`).
- **Zonas intactas:**
  - `crates/` (kernel Rust intacto al 100%).
  - `dist/` (cero modificaciones).
  - Protocolos de enrutado MCP, contratos soberanos CONTRACT-01 y lógica de red.
- **Riesgo y Mitigación:**
  - **Riesgo:** Pérdida de foco o salto de scroll al recibir mensajes nuevos o cargar anteriores.
  - **Mitigación:** Preservación de posición relativa en prepending (`prevScrollHeightRef` delta), auto-scroll condicional (`isAtBottom.current` threshold < 120px) y anclaje al fondo explícito vía `requestAnimationFrame` + `scrollToIndex(length - 1, { align: 'end' })`.

---

## 2. AC1: Benchmark de Escala en Navegador Real (Chrome Headless v155)

El benchmark se ejecutó sobre el kernel real (`:47004`) y el frontend de Coloquio en el canal `#general`, emulando cargas paginadas sucesivas (50 mensajes por paso, de 50 a 500 mensajes) mediante clicks reales en `"Cargar mensajes anteriores"`. Los datos se guardaron en `visual_reviews/coloquio_bench_ac.json`.

### Tabla Comparativa de Rendimiento (Lista Plana Sin Virtualizar)

| Métrica | 50 Mensajes (Base) | 300 Mensajes (5 clicks) | 500 Mensajes (9 clicks) | Umbral Lote AC | Estado |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Tiempo de Render por Paginación** | 522.33 ms | 2,518.93 ms | **9,607.20 ms (9.6s)** | $\le 1.0\text{ s}$ | ❌ **Supera umbral en 9.6x** |
| **Nodos DOM Totales** | 1,644 nodos | 7,694 nodos | **12,468 nodos** | < 3,000 | ❌ Explosión de DOM |
| **Nodos DOM de Lista de Mensajes** | 1,149 nodos | 6,849 nodos | **11,409 nodos** | - | 22.8 nodos/mensaje |
| **JS Heap en Uso** | 18.76 MB | 19.36 MB | **30.35 MB** | - | +61.8% memoria |
| **JS Heap Total Asignado** | 29.74 MB | 79.53 MB | **103.94 MB** | - | Crecimiento lineal |
| **Duración de Scroll Completo** | 309 ms | 6,429 ms | **7,897 ms** | < 1,000 ms | ❌ Congelamiento severo |
| **Scroll Framerate Promedio** | 29.1 FPS | 5.4 FPS | **2.5 FPS** | $\ge 30\text{ FPS}$ | ❌ Jank extremo |
| **Congelamiento Máximo de Frame** | 50.0 ms | 566.7 ms | **1,033.3 ms** | $\le 16.6\text{ ms}$ | ❌ Bloqueo del hilo principal |

**Conclusión AC1:** Dado que el render de 500 mensajes (9.6 segundos) superó holgadamente la condición de activación del TL ($\le 1.0$ s), se procedió obligatoriamente con la implementación de virtualización (AC2).

---

## 3. AC2: Implementación de Virtualización (`@tanstack/react-virtual@3.14.21`)

Se implementó el componente `VirtualMessageList` en `dashboard/src/components/ColoquioMessagesPanel.tsx` haciendo uso de `useVirtualizer`:

```tsx
const virtualizer = useVirtualizer({
  count: messages.length,
  getScrollElement: () => parentRef.current,
  estimateSize: () => 100, // Estimación inicial base por mensaje
  overscan: 5,             // Margen de 5 items fuera del viewport para scroll suave
});
```

### Funcionalidades Preservadas y Garantizadas

1. **Scroll Anclado al Fondo en Mensajes Nuevos:**
   - Se monitorea `isAtBottom.current` en el listener de scroll del contenedor con un umbral de proximidad de 120px (`c.scrollHeight - c.scrollTop - c.clientHeight < 120`).
   - Al detectarse un incremento en `messages.length` estando en el fondo, se despacha un `requestAnimationFrame` que ejecuta:
     ```tsx
     virtualizer.scrollToIndex(messages.length - 1, { align: 'end' });
     ```
   - Si se solicita scroll al fondo explícito (`needsScrollToBottom = true`), se sincroniza `parentRef.current.scrollTop = parentRef.current.scrollHeight` y se invoca `scrollToIndex` finalizando con `onScrollToBottomDone()`.
2. **"Cargar Anteriores" Sin Salto de Posición (Scroll Delta Stability):**
   - Antes de que los mensajes anteriores se inserten en el array (prepending), cuando `loadingOlder` pasa a `true`, se almacena el `scrollHeight` previo:
     ```tsx
     prevScrollHeightRef.current = parentRef.current.scrollHeight;
     ```
   - Al completarse la carga y re-renderizar, se calcula el delta exacto de altura y se compensa el scroll de manera imperativa:
     ```tsx
     const diff = parentRef.current.scrollHeight - prevScrollHeightRef.current;
     if (diff > 0) {
       parentRef.current.scrollTop += diff;
     }
     prevScrollHeightRef.current = 0;
     ```
3. **Salto a `#turno` (`jumpTurn`):**
   - El prop `jumpTurn` se resuelve buscando el índice del turno en `messages`:
     ```tsx
     const idx = messages.findIndex(m => m.turn === jumpTurn);
     if (idx !== -1) {
       virtualizer.scrollToIndex(idx, { align: 'center', behavior: 'smooth' });
     }
     ```
4. **Selección y Copia de Texto Nativa:**
   - El contenedor conserva la clase utilitaria `select-text`.
   - Las filas virtuales se posicionan mediante `transform: translateY(...)` absoluto, permitiendo que la selección con ratón o teclado (`Ctrl+C`) funcione de forma nativa sin romper el layout.

---

## 4. AC3: Estimación Honesta de Accesibilidad (a11y) en Listas Virtualizadas

La virtualización de listas introduce un dilema fundamental de accesibilidad que debe declararse con absoluta transparencia:

### A. Ventajas Técnicas y Mitigaciones Implementadas
- **Reducción de sobrecarga del árbol de accesibilidad:** Un DOM con 12,000 nodos satura los motores de accesibilidad en navegadores (Chrome/Edge IA2, Safari AX). La virtualización mantiene un número contenido de nodos (~25-35 en el viewport).
- **Semántica de región interactiva:**
  - El contenedor raíz tiene `role="region"`, `aria-label="Historial de mensajes de Coloquio"` y `tabIndex={0}` con anillo de foco visible (`focus-visible:ring-1`).
- **Posicionamiento relativo para lectores de pantalla:**
  - Cada fila virtual expone:
    ```tsx
    aria-setsize={messages.length}
    aria-posinset={virtualRow.index + 1}
    ```
    Esto permite que lectores como NVDA o JAWS anuncien: *"Mensaje 45 de 500"*, informando al usuario de su ubicación dentro del flujo completo.

### B. Limitaciones Reales Inevitables (Trade-offs Honestos)
- **Modo Virtual Buffer / Lectura Continua:**
  - Los lectores de pantalla (NVDA en modo cursor de lectura, VoiceOver en iOS/macOS) construyen un búfer virtual del DOM presente en memoria. Al desmontar los elementos fuera de pantalla, el usuario no puede leer secuencialmente el historial completo de arriba a abajo usando flechas del teclado sin provocar scroll activo que fuerce el montaje de nuevos nodos.
- **Búsqueda nativa del navegador (`Ctrl + F`):**
  - La búsqueda nativa del navegador solo encuentra el texto renderizado en el viewport. Los mensajes anteriores desmontados no coincidirán en `Ctrl + F`.
  - *Mitigación:* Se dispone de la caja de búsqueda integrada en la cabecera de Coloquio (`msgSearch`), la cual filtra sobre el array completo de mensajes en memoria antes de pasarlos al virtualizador.
- **Foco de teclado al saltar a turnos lejanos:**
  - Al ejecutar un salto de turno (`jumpTurn`), el elemento destino debe ser montado en el DOM antes de poder recibir `focus()`. Si la animación de scroll es virtualizada, se requiere esperar a que el virtualizador complete el ciclo de renderizado.

---

## 5. Verificación de Quality Gates

Todas las verificaciones se ejecutaron en el worktree `E:\tylluan-wt-antigravity`:

1. **TypeScript (`npx tsc --noEmit`):**
   - **0 errores** (se corrigió el tipado de `selectedChannel` en `ColoquioVirtualization.test.tsx` incorporando `message_count: 0` y `last_turn: 0`).
2. **Vitest Suite Completa (`npx vitest run`):**
   - **15 archivos de pruebas pasados (15 / 15)**.
   - **125 pruebas pasadas (125 / 125)** — 100% verde (incluyendo las 8 pruebas de `ColoquioVirtualization.test.tsx` y las 28 de `AxeAudit.test.tsx`).
3. **ESLint (`npm run lint`):**
   - **0 errores, 36 advertencias** (idéntico a la base de main, $\le 36$).
4. **Integridad de Repositorio:**
   - `crates/` y `dist/`: 0 modificaciones.
   - Diff sin commitear listo para inspección del Tech Lead.

---

## 6. No Verificado

1. **Pruebas con lectores de pantalla físicos en Windows:** No se probó la lista virtualizada directamente con una instancia activa de NVDA o JAWS corriendo en el escritorio del usuario (solo se verificaron los atributos ARIA `role`, `aria-setsize`, `aria-posinset` y `tabIndex` en jsdom y Chrome Headless).
2. **Virtualización con imágenes de gran tamaño dinámicas sin aspect-ratio previo:** Si un mensaje contiene imágenes adjuntas que tardan en cargar su tamaño intrínseco, `virtualizer.measureElement` recalcula la altura tras el evento `onload`, lo cual podría producir un micro-ajuste visual si ocurre mientras el usuario tiene el scroll fijado en medio del mensaje.
