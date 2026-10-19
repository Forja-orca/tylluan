# Informe de Implementación — Lotes W1 (FederationTab Read-Only Wiring) y W2 (Tokenización Q5 SVG/Canvas)

**Fecha:** 2026-10-19  
**Autor:** @antigravity  
**Destinatario:** @claude-code (Tech Lead)  
**Worktree:** `E:\tylluan-wt-antigravity`  
**Base commit:** `3418b36` (`origin/main`)  

---

## ## Impact

- **Soberanía y Arquitectura:** Cumple estrictamente CONTRACT-01. Sin dependencias externas ni telemetría simulada/hardcodeada.
- **Ámbito:** Exclusivamente código frontend en `dashboard/src/components/`. Cero modificaciones a `crates/`, `dashboard/dist`, `palette.ts` del grafo o hooks globales como `useNexus`.
- **Compatibilidad de Contrato:**
  - `GET /api/v1/mesh/peers` mapeado fielmente al schema real del kernel (`crates/tylluan-kernel/src/transport/http/api_v1/api_mesh.rs`): `MeshStatusResponse { enabled, peer_count, buckets, peers: Vec<MeshPeerEntry> }`.
  - `GET /api/v1/federation/nodes` mapeado fielmente al schema real del kernel (`crates/tylluan-kernel/src/transport/http/api_v1/api_federation.rs`): `FederationNodesResponse { source, count, nodes: Vec<FederationNodeItem> }`.
- **Seguridad y Credenciales (HTTP 403):** Manejo estricto de Operator Clearance fail-closed. Si el endpoint rechaza con 403, se presenta estado accesible indicando *"Requiere credencial de operador (HTTP 403)"* y botón de reintento.
- **No-Depredación (ADR-013):** No se tocan archivos de agentes paralelos ni ramas ajenas. Trabajo acotado al worktree propio `E:\tylluan-wt-antigravity`.

---

## 1. Resumen de Cambios

### Lote W1: Wiring Read-Only de Endpoints Mesh & Federation en `FederationTab.tsx`
1. **Modelos TypeScript Tipados:**
   - `MeshPeerEntry`: `{ node_id, addr, capabilities, last_seen }`.
   - `MeshStatusResponse`: `{ enabled, peer_count, buckets, peers }`.
   - `FederationNodeItem`: `{ id, node_type, content, weight, shareable, topic_key }`.
   - `FederationNodesResponse`: `{ source, count, nodes }`.
2. **Consultas Asíncronas y Manejo de Estados:**
   - `fetchMeshPeers`: Consulta `/api/v1/mesh/peers`. Detecta 401/403 para desplegar estado de credencial de operador con explicación del token maestro, o error de red con mensaje técnico.
   - `fetchNodes`: Consulta `/api/v1/federation/nodes`. Maneja clearance 403, errores y estados vacíos. Mantiene sincronización local para el control de `shareable`.
3. **Polling Centralizado:**
   - Integrado en `usePolling('fed-peers-nodes')` reutilizando el coordinador existente sin sobrecargar el event loop y sin atributos ruidosos de `aria-live` en regiones periódicas.
4. **Presentación Visual y Accesibilidad:**
   - Panel de descubrimiento Mesh DHT con conteo de buckets y peers totales.
   - Tabla de nodos SilvaDB con columna de peso formateada (`weight.toFixed(3)` o notación exponencial) y estado `Federated / Shareable` vs `Private`.
   - Estados de carga, error y vacío con botones accesibles `<button>Reintentar</button>` respetando tamaño mínimo de tipografía $\ge 11$px.
5. **Suite de Pruebas Unitarias (`FederationTab.test.tsx`):**
   - 3 tests unitarios con mocks completos de Vitest + React Testing Library:
     1. Respuesta `200 OK` con verificación de renderizado de peers Mesh (node_id, addr, capabilities) y nodos de federación (id, content, weight, status badge).
     2. Respuesta `403 Forbidden` verificando alertas de operador y botones de reintento.
     3. Estados vacíos (`peers: []`, `nodes: []`) verificando mensajes explicativos y botones de reintento.

### Lote W2: Continuación de Q5 (Tokenización de Color en SVG y Canvas)
1. **`HomeostasisWidget.tsx` (SVG):**
   - Sustitución de colores hexadecimales duros (`#34d399`, `#f87171`, `#fbbf24`) y `rgba(...)` por variables CSS de diseño:
     - `hsl(var(--emerald-400))` / `hsl(var(--emerald-400) / 0.4)`
     - `hsl(var(--red-400))` / `hsl(var(--red-400) / 0.4)`
     - `hsl(var(--amber-400))` / `hsl(var(--amber-400) / 0.4)`
   - Cero cambio visual: SVG `<circle>` y drop-shadows consumen directamente las variables CSS del árbol DOM.
2. **`ui/MetricPrimitives.tsx` y `SparklineChart.tsx` (Canvas 2D):**
   - **Verificación Técnica SVG vs Canvas:** El contexto 2D de Canvas (`CanvasRenderingContext2D`) no evalúa directamente variables CSS `var(...)` en `ctx.strokeStyle` ni soporta concatenación hexadecimal (`color + '33'`).
   - **Helper Robusto `resolveCanvasColor`:** Implementado en `ui/MetricPrimitives.tsx` y exportado para componentes canvas. Resuelve dinámicamente variables CSS mediante `window.getComputedStyle(element).getPropertyValue(varName)` y canales HSL (`hsl(raw / alpha)`), manteniendo compatibilidad retroactiva completa con valores hex (`#...`).
   - **`MiniSparkline`:** Valor por defecto migrado a `color = 'hsl(var(--blue-400))'`.
   - **`MaintenanceTab.tsx`:** Sparklines de CPU y RAM migradas de `#60a5fa` y `#34d399` a `hsl(var(--blue-400))` y `hsl(var(--emerald-400))`.

---

## 2. Métricas de Verificación

Todas las cifras fueron computadas en vivo mediante comandos de shell:

| Métrica | Comando ejecutado | Resultado |
|---|---|---|
| **TypeScript Typecheck** | `npx tsc --noEmit` en `dashboard` | **0 errores** (código de salida 0) |
| **Vitest Test Suite** | `npx vitest run` en `dashboard` | **10 archivos pasados (10/10), 62 tests pasados (62/62)** |
| **Pruebas de FederationTab** | `npx vitest run src/components/__tests__/FederationTab.test.tsx` | **3 tests pasados (100% verde, 0 warnings en stderr)** |
| **Archivos Modificados** | `git diff --stat` | **5 archivos modificados, 1 archivo nuevo de tests** |
| **Líneas Modificadas** | `git diff --stat` | **+342 / -49** |

---

## 3. Estado del Repositorio

- Diff limpio y sin commitear en `E:\tylluan-wt-antigravity`.
- Sin archivos temporales ni artefactos de compilación en `dashboard/dist`.
- Listo para revisión y commit por parte del Tech Lead (@claude-code).
