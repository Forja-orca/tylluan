# Plan de Pruebas End-to-End: TylluanNexus o3

> **Versión**: 1.0  
> **Estado**: Proyecto Malamadre v2.0 Soberano  
> **Fecha**: Abril 2026

## 1. Resumen Ejecutivo
Este plan valida la estabilización de TylluanNexus o3 en un entorno de producción simulado, con foco en cognición (modelos), core gremios MCP y flujo de lecciones (`lesson_proposals`). Se prioriza la robustez de los componentes centrales y la verificación de los endpoints HTTP/SSE, la persistencia en SilvaDB y la generación de embeddings mediante **Nomic Embed v2**, con **Qwen3.5-2B** como respaldo de LM.

## 2. Objetivo
Verificar la estabilidad operativa del Kernel TylluanNexus o3 en modo producción: carga de modelos cognitivos, handshake MCP con 19 gremios core, flujo end-to-end de lecciones, y re-indexación/consenso.

## 3. Alcance
- **Incluye**: Nomic Embed v2 (embedding), Qwen3.5-2B (LM), 19 gremios core operativos, flujo de lecciones, embeddings, consenso y re-indexación.
- **Excluye**: Gremios externos problemáticos (`postgres`, `slack`, `sentry`).

## 4. Casos de Prueba End-to-End (P1–P5)

### P1: Arranque y Handshake
- **Pasos**: Arrancar binario release; esperar estado Running para gremios core; verificar handshakes MCP en logs.
- **Esperado**: 19 gremios core handshake exitosos; ausencia de errores críticos.

### P2: Flujo de Lección y Embeddings
- **Pasos**: Inyectar una lección real a través del Hub/Mailbox; confirmar presencia de nodo en SilvaDB; confirmar generación de embedding con Nomic Embed v2; verificar progreso de consenso.
- **Esperado**: Nodo de lección persistido; embedding generado (768-dim); progreso de consenso observable.

### P3: Re-indexación
- **Pasos**: Provocar cambio significativo (nueva lección o modelo); verificar que Agnostic Indexer detecta y re-indexa embeddings asociados.
- **Esperado**: Log de re-indexación con estado de embeddings actualizados.

### P4: Endpoints HTTP/SSE
- **Pasos**: Consultar `/health`, `/discovery`, `/sse`; suscribirse a SSE y emitir eventos simples.
- **Esperado**: Respuestas estables (200 OK); SSE entrega eventos correspondientes.

### P5: Seguridad de Endpoints
- **Pasos**: Llamadas a `/messages` sin token y con token válido.
- **Esperado**: 401 Unauthorized sin token; 200 OK con el token local configurado en `TYLLUAN_TOKEN` o `.tylluan-token`.

## 5. Criterios de Aceptación
- 19 Gremios core en estado `Running`.
- Embeddings generados con Nomic Embed v2.
- Persistencia confirmada en SilvaDB y Mailbox.
- Endpoints HTTP protegidos por token.

---
*Tylluan E2E Test Plan*
