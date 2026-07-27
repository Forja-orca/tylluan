# Tylluan — Reglas del Entorno de Desarrollo y Visualización de Arquitectura

## 🗺️ Visualizador de Arquitectura (`docs-site`)
Tylluan cuenta con un visualizador técnico standalone construido en Next.js en `E:\tylluan\docs-site` que corre por defecto en el puerto `3010`. Es la fuente de verdad visual e interactiva del sistema.

### Reglas de Sincronización Obligatorias para Agentes:
1.  **Sincronización de Cambios:** Cualquier modificación a la arquitectura cognitiva (ej. añadir/renombrar MCP tools, cambiar el modelo de base de datos SilvaDB, alterar la federación P2P, o re-estructurar los ciclos de consolidación) **DEBE** verse reflejada inmediatamente en los componentes React de `E:\tylluan\docs-site\src\components\architecture/`.
    *   **Mapa General:** [architecture-map.tsx](file:///E:/tylluan/docs-site/src/components/architecture/architecture-map.tsx)
    *   **FSRS Model:** [fsrs-model.tsx](file:///E:/tylluan/docs-site/src/components/architecture/fsrs-model.tsx)
    *   **Retrieval Pipeline:** [retrieval-pipeline.tsx](file:///E:/tylluan/docs-site/src/components/architecture/retrieval-pipeline.tsx)
    *   **Federation Mesh:** [federation-mesh.tsx](file:///E:/tylluan/docs-site/src/components/architecture/federation-mesh.tsx)
    *   **Sleep/Dream Cycle:** [sleep-cycle.tsx](file:///E:/tylluan/docs-site/src/components/architecture/sleep-cycle.tsx)
    *   **Dispatch Flow:** [dispatch-flow.tsx](file:///E:/tylluan/docs-site/src/components/architecture/dispatch-flow.tsx)
    *   **Roadmap:** [roadmap.tsx](file:///E:/tylluan/docs-site/src/components/architecture/roadmap.tsx)
2.  **Veracidad de Métricas:** Nunca inventar números de tests ni benchmarks en el visualizador o en `STATUS.md`. El conteo de tests real actual es **`388`** (315 kernel + 61 link + 12 fsrs, verificado 2026-07-12). Ante cualquier cambio, corre la suite de pruebas para reportar la cifra exacta.
3.  **Verificación de Compilación:** Antes de finalizar cualquier turno que modifique el docs-site, debes ejecutar el build de producción para certificar que compila limpio y sin advertencias:
    ```bash
    cd E:\tylluan\docs-site
    pnpm run build
    ```
4.  **Acceso de Desarrollo:** El servidor de desarrollo se levanta con `pnpm run dev` en el puerto `3010`. Se ha configurado `allowedDevOrigins` en `next.config.ts` para autorizar conexiones a `127.0.0.1` y `localhost` sin bloqueos de HMR/CORS.

---

## 🔍 Conectividad y Estado Real de Módulos (Auditoría v0.13.0)
Los agentes deben estar al tanto de las siguientes particularidades del código para evitar falsas asunciones:

1.  **Código Huérfano/Muerto (No reactivar ni duplicar):**
    *   `DreamCycle::start_background_scheduler()` (`memory/dream_cycle.rs`): Inactivo. La consolidación se ejecuta centralizada en `NightConsolidation` vía cron de `main.rs`.
    *   `IdentityManager` (`memory/identity.rs`): Inactivo. Reemplazado por `AgentProfileStore`.
    *   `Sovereign Routine Subsystem` (`main.rs`): Comentarios de rutinas desactivados.
2.  **Duplicidad de ConsensusEngine:**
    *   `consensus.rs` (raíz): Motor de frescura determinista para sincronización de federación.
    *   `memory/consensus.rs`: Motor de consolidación semántica y resolución de conflictos cognitivos.
3.  **Notas de arquitectura:**
    *   Tylluan **NO** tiene proxy local (Hyper proxy de cero tiempo de inactividad), arranca directo en `:4000`.
    *   Tylluan no tiene un daemon SSE deliberativo síncrono tipo `coloquio_watch.rs` (candidato a implementar si se requiere multi-agent debate).
    *   Tylluan cuenta con **FSRS-5** y el módulo `coordinator.py` de orquestación multi-agente paralela.
