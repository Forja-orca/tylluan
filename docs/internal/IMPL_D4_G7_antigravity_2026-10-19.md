# Entrega de Auditoría e Implementación: D4 y G7

**Fecha:** 2026-10-19  
**Autor:** Antigravity  
**Rama/Worktree:** `E:\tylluan-wt-antigravity`  
**Asignación de referencia:** Coloquio Turn 1312 (Tech Lead `@claude-code`)  

---

## ## Impact

- **Decisiones de Arquitectura y Dashboard (D4):**
  - Auditoría exhaustiva de los 9 endpoints huérfanos del dashboard contra el árbol de código fuente en `crates/tylluan-kernel/src/`.
  - Se identifican qué rutas son internas para coordinación agéntica/resiliencia (`solo-API`), cuáles representan stubs deprecados candidatos a eliminación (`borrar`), y cuáles aportan valor observable directo para paneles existentes del dashboard (`cablear`).
  - Se verifica la clase de autorización (operador vs. genérica/agente) y el shape JSON real de cada endpoint según `crates/tylluan-kernel/src/transport/http/auth.rs`.
- **Veracidad Arquitectónica en Visualizador docs-site (G7):**
  - Auditoría sistemática de los 9 subsistemas, herramientas soberanas (CONTRACT-01: 6 tools), puertos (:47004) y recuentos de tests y guilds contra `STATUS.md`, `README.md`, `architecture.svg` y `architecture_interactive.spec.json`.
  - Corrección de cifra desactualizada en `docs-site/src/app/page.tsx:43` (actualizado de `1,139` a `1,144` pruebas pasando, alineado con README.md y STATUS.md).
  - Corrección en `docs-site/src/components/architecture/dispatch-flow.tsx:26` para reflejar las 6 sovereign tools (CONTRACT-01) en lugar del sublabel parcial.
  - Verificación estricta de compilación con Turbopack/Next.js: `pnpm run build` en `docs-site/` exitoso (exit code 0).
- **Exclusiones respetadas:** No se modificaron crates Rust, ni `useNexus`, ni `dashboard/dist`, ni `puntos_neuronales`.

---

## D4: Tabla de Endpoints Huérfanos → Decisión

| Endpoint | Existe en Kernel? | Clase Operador? | Shape Real en Kernel | Decisión | Motivo / Justificación |
|---|---|---|---|---|---|
| **`/api/v1/capability-registry`** | Sí (`api_guilds.rs:200`) | No (Lectura genérica) | `Vec<GuildCapabilityEntry>` (JSON del catálogo estático declarativo) | **solo-API** | Es el índice declarativo estático para el enrutamiento agéntico de `TaskContext v2` (ADR-014); el dashboard ya cubre herramientas en `FleetTab`/`TrustConsoleTab`. |
| **`/api/v1/journal`** (`{agent}/checkin`, `{agent}/recover`) | Sí (`api_journal.rs:281,296,318`) | No (Ciclo de vida agente) | In: `{"task": string}` / Out: `{"ok": bool, "entry": {"agent_id", "current_task", "checkpoint_data", "updated_at", "stale", "stale_secs"}}` | **solo-API** | Protocolo de recuperación de fallos y puntos de control para loops autónomos en reinicio; no es una vista interactiva de operador humano. |
| **`/api/v1/mesh/peers`** | Sí (`api_mesh.rs:31`) | No (Lectura genérica) | `{"enabled": bool, "peer_count": usize, "buckets": usize, "peers": Vec<MeshPeerEntry>}` | **cablear** | Permite visualizar en `FederationTab` el estado de la tabla de enrutamiento DHT Kademlia / mDNS y el conteo de buckets en vivo. |
| **`/api/v1/federation/nodes`** | Sí (`api_federation.rs:762`) | No (Lectura genérica) | `{"source": string, "count": usize, "nodes": [{"id", "node_type", "content", "weight", "shareable", "topic_key"}]}` | **cablear** | Consulta de procedencia de nodos sincronizados (M11-C); encaja directamente como sub-vista de trazabilidad en `FederationTab`. |
| **`/api/v1/routing/anchors`** (`/reembed`) | Sí (`api_federation.rs:898,910,920`) | GET: No / POST: No (reembed requiere matcher activo) | GET: `{"anchors": [...], "count": usize}` / POST: `{"inserted": N, "errors": M}` | **solo-API** | Semántica de anclas de enrutamiento vectorial en SilvaDB para despacho rápido de intents; inspección diagnóstica técnica de bajo nivel. |
| **`/api/v1/eval/run`** & **`/api/v1/eval/results`** | Sí (`api_eval.rs:51,100`) | No (Run aislado en temp DB) | Run in: `{"benchmark", "num_queries", "seed"}` / Results out: `{"ok": bool, "results": [EvalResult], "total": usize}` | **cablear** | Evaluación de benchmarks de recuperación (`longmemeval-s`); puede integrarse limpiamente en `LaboratoryTab` para disparar e inspeccionar benchmarks de precisión. |
| **`/api/v1/stigmergy/traces`** | Sí (`api_silva.rs:258`) | No (Escritura agente) | In: `{"target_uri", "target_kind", "agent_id", "trace_type", "weight"}` / Out: `201 Created` | **solo-API** | Endpoint de emisión de rastros de trabajo por agentes autónomos (ADR-015); el dashboard ya actúa como observador visual mediante `GET /api/v1/stigmergy/zones` en `StigmergyHeatmapPanel`. |
| **`/api/v1/system/signals`** | Sí (`api_ops.rs:111`) | No (Lectura genérica) | `{"traffic": {...}, "errors": {...}, "latency": ..., "saturation": ...}` | **cablear** *(ya activo)* | Señales doradas del sistema; ya está activamente en uso en el dashboard a través de su alias canónico `/api/v1/health/golden-signals` en `useNexus` y `OverviewTab`. Se recomienda consolidar la referencia al path canónico `/system/signals`. |
| **`/api/v1/inference/providers`** | Sí (`api_admin.rs:877`) | POST: Sí (`auth.rs:309`) / GET: No | Devuelve `StatusCode::OK` vacío (stub sin cuerpo) | **borrar** | Stub huérfano sin implementación real. La gestión real de proveedores externos ya fue implementada y conectada formalmente en `/api/v1/external-providers` (`ModelsExternalProviders.tsx`). |

---

## G7: Verificación de Realidad en `docs-site/`

### 1. Auditoría de los "9 Subsistemas" (`docs-site/src/app/page.tsx:39`)
La cabecera de `docs-site` cita:
> *"LinearRAG · Decision Service (ADR-021) · Scheduler · Coloquio · Guilds (45) · SilvaDB · Federation · DreamCycle · Dashboard"*

Verificación contra el código fuente real:
1. **LinearRAG:** Real (`crates/tylluan-kernel/src/memory/silva/search.rs`: BM25 + FTS5 + mxbai-embed-large 1024d + PageRank con penalización de grado).
2. **Decision Service (ADR-021):** Real (`crates/tylluan-kernel/src/router/decision.rs`, `api_decide.rs`, sexta herramienta soberana `tylluan_decide`).
3. **Scheduler:** Real (`crates/tylluan-kernel/src/router/scheduler.rs`, `api_v1/scheduler/confusion`).
4. **Coloquio:** Real (`crates/tylluan-kernel/src/transport/http/api_v1/api_coloquio.rs`, `guilds/core/coloquio.py`).
5. **Guilds (45):** Real. `STATUS.md:11` confirma: *"46 módulos FastMCP, 45 enrutables por defecto (vision_moondream está excluido en router/catalog.rs; uno de los 45, sandbox, es experimental)"*.
6. **SilvaDB:** Real (`crates/tylluan-kernel/src/memory/silva/`).
7. **Federation:** Real (`crates/tylluan-kernel/src/federation/`, `crates/tylluan-link/`).
8. **DreamCycle:** Advertencia terminológica. En el código actual, `DreamCycle::start_background_scheduler()` es código huérfano inactivo (`.agents/AGENTS.md`); el subsistema activo de consolidación es `NightConsolidation` (`api_silva.rs:48` y cron en `main.rs`). `docs-site/src/components/architecture/architecture-map.tsx:166` ya lo aclara correctamente: *"ACTIVE: DreamCycle — NightConsolidation (hourly)"*.
9. **Dashboard:** Real (`dashboard/`, paquete React/Vite/Tailwind, incrustable con `--features bundled-dashboard`).

---

### 2. Tabla de Contrastación: Afirmación → Real → Fichero:Línea → Propuesta

| Afirmación en Docs | Estado Real en Código | Fichero : Línea | Corrección / Propuesta |
|---|---|---|---|
| `Tests: 1,139 (1053 kernel + 74 tylluan-link + 12 FSRS)` | **1,144** pruebas (1057 kernel + 75 link + 12 fsrs) según `STATUS.md:10`, `README.md:198` y `architecture.svg:106`. En `AGENTS.md`: 1,146 tras T-DSH-1. | `docs-site/src/app/page.tsx:43` | **CORREGIDO**: Actualizado a `1,144` (`1057 kernel + 75 tylluan-link + 12 FSRS — all passing`). |
| `Guild Call: tylluan_do / recall / remember` | Existen **6 herramientas soberanas** invariantes bajo CONTRACT-01 (`tylluan_do`, `tylluan_remember`, `tylluan_recall`, `tylluan_think`, `tylluan_graph`, `tylluan_decide`). | `docs-site/src/components/architecture/dispatch-flow.tsx:26` | **CORREGIDO**: Actualizado el sublabel a `"6 sovereign tools (CONTRACT-01)"`. |
| `Guilds (45)` | Coincide con `STATUS.md:11` (45 enrutables por defecto en `router/catalog.rs` de 46 módulos FastMCP). | `docs-site/src/app/page.tsx:39` | Sin cambios requeridos (verificado exacto). |
| Puertos de Red | En `docs-site/` no existen menciones a puertos obsoletos (los antiguos 4000 y 3030). El kernel escucha directamente en `:47004` sin proxy. | `docs-site/src/**/*.tsx` | Sin cambios requeridos (limpio de drift). |
| `LinearRAG: PageRank with Degree Penalty / (1 + deg * 0.1)` | Invariante verificado en `graph.rs:188` (`/ (1 + deg * 0.1)`, penaliza hubs genéricos). | `docs-site/src/components/architecture/architecture-map.tsx:88` | Sin cambios requeridos (exacto y verificado). |

---

## Evidencia de Validación
1. **Compilación de `docs-site` (`pnpm run build`):**
   ```
   > tylluan-architecture-viz@0.1.0 build E:\tylluan-wt-antigravity\docs-site
   > next build

   ▲ Next.js 16.3.8 (Turbopack)
   ✓ Running next.config.ts took 192ms
   ✓ Compiled successfully in 9.4s
   ✓ Generating static pages using 4 workers (3/3) in 1772ms
   ○ (Static) prerendered as static content
   Exit code: 0
   ```
2. **Puertas de Enlaces:**
   - `python scripts/check_doc_links.py`: 222 enlaces relativos resueltos OK.
