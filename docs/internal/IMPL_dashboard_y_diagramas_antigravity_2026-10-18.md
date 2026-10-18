# Informe de Implementación: Dashboard + Diagramas Públicos (`CHECKLIST_dashboard_y_diagramas_antigravity_2026-10-18.md`)

**Autor:** `@antigravity`  
**Fecha:** 2026-10-18  
**Worktree:** `E:\tylluan-wt-antigravity` (sincronizado con `origin/main` en `743a7069df6fac1baa28683ab21221e3b3f44823`)  
**Estado:** Completo — entregado como diff sin commitear (`diff SIN commitear`, Reglas 8 y 9)

---

## Impact

1. **¿Qué flujo existente puede romper este cambio?**
   - **Dashboard React (`dashboard/src/`):** Se añaden dos paneles nuevos (`DecisionServicePanel.tsx` bajo la pestaña `audit` y `ContradictionsPanel.tsx` como tercera sub-pestaña de `MemoryConsolidated.tsx`), se amplían `AuditTrailPanel.tsx`, `TrustConsoleTab.tsx` y `ColoquioCanvasWorkspace.tsx`, y se normalizan tamaños tipográficos (`≥ 11px`), atributos ARIA, anillos `focus-visible` y fondos del tema claro (`[data-theme="light"]`). Si el kernel en ejecución aún no expone `GET /api/v1/decide/shadow` o `GET /api/v1/silva/conflicted` (asignados a `@deep` en T1253/T1255), ambos paneles capturan HTTP `404`/`403` mostrando un estado vacío/pendiente honesto sin lanzar excepciones ni bloquear la UI.
   - **Panel de Seguridad y Auditoría (`AuditTrailPanel.tsx` — D3):** Cada sub-consulta (`/api/v1/audit/trail`, `/api/v1/audit/verify`, `/api/v1/audit/latency`, `/api/v1/security/events`, `/api/v1/audit/logs`) maneja sus estados de carga, vacío y error de forma independiente (`Promise.allSettled`) y muestra pills con palabra explícita (no solo color).
   - **Visualizador `docs-site` y diagramas públicos (`docs/assets/`):** Actualización estática de conteos (`1139` tests, `45` guilds en catálogo enrutable, `6` herramientas soberanas, `mxbai-embed-large` 1024d, `Decision Service` en `architecture_interactive.spec.json`/`.html`) y regeneración de `docs/assets/architecture.png` a partir de `docs/assets/architecture.svg`. Sin impacto en el runtime del kernel ni en contratos IPC/MCP.
2. **¿Qué ocurre si un endpoint nuevo devuelve esquema vacío o error?**
   - `DecisionServicePanel` acepta exclusivamente la forma congelada `{rows:[...], summary:[{contract,n,n_agreed,n_disagreed,n_null}], limit, total}` (sin alias) y deriva `summary` en cliente a partir de `rows` si faltase. `ContradictionsPanel` acepta exclusivamente `{nodes:[{id,node_type,content_excerpt,conflicted_since?}], total_conflicted, limit}` desde `GET /api/v1/silva/conflicted?limit=N` y no muestra `deprecated_nodes` como contradicciones.
3. **Zonas excluidas respetadas:**
   - Cero archivos tocados en `crates/`, `guilds/`, `scripts/`, `tylluan*.toml`, o `.github/workflows/`.

---

## Detalle Técnico del Lote Actual (`D3`, `G1`, `G2`, `G4`)

### 1. D3 — Panel de Operador de Seguridad y Auditoría (`dashboard/src/components/AuditTrailPanel.tsx`)

#### Verificación de Rutas y Formas Reales en el Kernel (Citas `fichero:línea`)
Antes de modificar `AuditTrailPanel.tsx`, se verificaron las rutas registradas en `crates/tylluan-kernel/src/transport/http/api_v1/routes.rs` y sus handlers reales:

1. **`GET /api/v1/security/events`**
   - **Registro de ruta:** `crates/tylluan-kernel/src/transport/http/api_v1/routes.rs:98` (`.route("/api/v1/security/events", get(security_events_handler))`).
   - **Handler real:** `crates/tylluan-kernel/src/transport/http/api_v1/api_ops.rs:107-109`:
     ```rust
     pub(super) async fn security_events_handler(State(state): State<HttpState>) -> impl IntoResponse {
         Json(state.doctor.diagnose().await)
     }
     ```
   - **Estructura devuelta (`DiagnosticReport`):** definida en `crates/tylluan-kernel/src/doctor/mod.rs:16-54`:
     `{ timestamp, status: "healthy" | "degraded" | "critical", guilds: [{ name, running, tools_count, issues: Vec<String> }], storage: { silva_db_ok, nodes_count, silva_bytes, recent_nodes }, system: { total_memory_mb, used_memory_mb, memory_percent, cpu_usage_percent, process_count, thread_count, status: "ok" | "warning" | "critical", warnings: Vec<String> }, config_valid: bool, suggestions: Vec<String> }`.
   - **Divergencia respecto al checklist:** El checklist describe `/api/v1/security/events` como *"eventos recientes"*, pero el handler real en `api_ops.rs:107-109` delega en `state.doctor.diagnose().await` (`DiagnosticReport`), no en una tabla de eventos discretos. En `AuditTrailPanel.tsx` se sintetizan como eventos diagnósticos accionables todas las anomalías de `system.warnings`, `guilds[].issues`, `config_valid == false`, `storage.silva_db_ok == false` y `suggestions`, además de aceptar un array opcional `events` por compatibilidad futura.

2. **`GET /api/v1/audit/verify`**
   - **Registro de ruta:** `crates/tylluan-kernel/src/transport/http/api_v1/routes.rs:76` (`.route("/api/v1/audit/verify", get(api_audit::audit_verify))`).
   - **Handler real:** `crates/tylluan-kernel/src/transport/http/api_v1/api_audit.rs:5-28` (invoca `crate::transport::server::handler_do::verify_audit_chain()`).
   - **Estructura devuelta:** HTTP `200 OK` con `{ ok: true, status: "clean" | "tampered", valid_count: usize, tampered_count: usize, message: String }`, o HTTP `500 INTERNAL_SERVER_ERROR` con `{ ok: false, error: String }`.

3. **`GET /api/v1/audit/latency`**
   - **Registro de ruta:** `crates/tylluan-kernel/src/transport/http/api_v1/routes.rs:283` (`.route("/api/v1/audit/latency", get(api_audit::audit_latency_stats_handler))`).
   - **Handler real:** `crates/tylluan-kernel/src/transport/http/api_v1/api_audit.rs:233-300` (helper `compute_audit_latency_from_db` en `api_audit.rs:38-115`).
   - **Estructura devuelta:** `{ available: true, window_minutes, source: "guild_audit_log", note, generated, latency_ms: { p50, p95, p99, max, sampled, zero_latency_excluded }, error_rate: { total, errors, percent } }` (donde `p50`/`p95`/`p99`/`max` son `null` cuando `sampled == 0`, líneas `76-78`), o `{ available: false, reason, source: "guild_audit_log", generated }` cuando no existe `data/audit.db`.

4. **`GET /api/v1/audit/trail` y `GET /api/v1/audit/logs`**
   - `routes.rs:72` → `api_admin.rs:1546-1586` (`{ entries: [{ agent_id, guild, intent_preview, allowed, timestamp }], total }`).
   - `routes.rs:75` → `api_admin.rs:753-772` (`{ logs: [{ type, source, data: { message }, ts }], count }`).
   - *(Nota: también existe `GET /api/v1/audit` en `routes.rs:71` → `mod.rs:1509-1585`, que lee `guild_audit_log` de `./data/audit.db`).*

#### Estados y Pills con Palabra en `AuditTrailPanel.tsx`
- **Pills con palabra explícita (nunca solo color):**
  - Integridad HMAC (`/api/v1/audit/verify`): `CLEAN — VERIFIED`, `CLEAN — EMPTY (0)`, `TAMPERED — BROKEN`, `ERROR — UNVERIFIED`, `VERIFYING...`.
  - Latencia de despacho (`/api/v1/audit/latency`): `SAMPLED (N)`, `EMPTY — 0 SAMPLES`, `UNAVAILABLE — NO DB`, `ERROR — UNAVAILABLE`, `LOADING...`.
  - Seguridad / Doctor (`/api/v1/security/events`): `HEALTHY — NOMINAL`, `DEGRADED — WARNING`, `CRITICAL — ALERT`, `ERROR — UNREACHABLE`, `CHECKING...`.
  - Tabla de autorizaciones (`/api/v1/audit/trail`): `Allowed` / `Blocked`.

---

### 2. G1 — `docs/assets/architecture.png` y Referencias en el Repositorio

- **Decisión:** Se **regeneró** `docs/assets/architecture.png` (`1280x1020`, `205,409 bytes`) directamente desde `docs/assets/architecture.svg` usando `chrome-headless-shell.exe` y se verificó visualmente (incluye `:47004`, `6 Sovereign Tools`, `Decision Service (ADR-021)`, `45 Python Guilds`, `1,139 TESTS GREEN`).
- **Grep de referencias a `architecture.png` en todo el repositorio:**
  - Comando: `git grep -n "architecture.png"` (exit code `0`)
  - Salida:
    - `docs/internal/ANALISIS_disonancia_documental_2026-10-14.md:45`
    - `docs/internal/AUDIT_dashboard_y_diagramas_2026-10-18.md:191,197`
    - `docs/internal/BRIEFING_mimo_dashboard_10_10_2026-08-06.md:17`
    - `docs/internal/CHECKLIST_dashboard_y_diagramas_antigravity_2026-10-18.md:107,199`

---

### 3. G2 y G4 — Cifra Única de Guilds (`45`), `6 Sovereign Tools` y `Decision Service`

#### Fuente Real de la Cifra de Guilds y Comando Exacto
Se contrastaron las dos fuentes reales del sistema:

1. **Puerta de CI de documentación (`scripts/check_docs_reality.sh`, líneas 418-450, Check 6 — *Guild count*):**
   - Comando ejecutado:
     ```bash
     bash -c 'guild_files=$(grep -rl "FastMCP(" guilds/ --include="*.py" 2>/dev/null | grep -v "/_" | grep -v "/test_" | wc -l | tr -d " "); excluded=$(awk "/EXCLUDED_GUILDS/,/\]/" crates/tylluan-kernel/src/router/catalog.rs 2>/dev/null | grep -oE "\"[a-z_]+\"" | wc -l | tr -d " "); echo "guild_files=$guild_files excluded=$excluded real_guilds=$((guild_files - excluded))"'
     ```
   - Salida (exit code `0`):
     ```text
     guild_files=46 excluded=1 real_guilds=45
     ```
   - Explicación: Hay `46` scripts `.py` con `FastMCP(` bajo `guilds/` (excluyendo `_*` y `test_*`), menos `1` guild excluido en `crates/tylluan-kernel/src/router/catalog.rs:98` (`EXCLUDED_GUILDS = &["vision_moondream"]`), dando **`45` Python guilds enrutables en el catálogo**. Además, `scripts/check_docs_reality.sh` exige exactamente `45` en `README.md`, `README.es.md`, `STATUS.md` y `docs/assets/architecture.svg` (cualquier otro número `>= 30` junto a `guilds` hace fallar CI con exit code `1`).

2. **Endpoint en vivo `GET /health?verbose=true`:**
   - Comando ejecutado:
     ```powershell
     curl.exe -s http://127.0.0.1:47004/health?verbose=true
     ```
   - Salida relevante (exit code `0`):
     ```json
     {"guilds":{"active":14,"ok":true,"total":50},"sovereign_tools":{"count":6,"ok":true}}
     ```
   - Por qué difiere (`50` vs `45`): `state.registry.list_guilds()` en `/health?verbose=true` incluye entradas virtuales/internas registradas en memoria además de los scripts Python del catálogo (`45`).
   - **Cifra unificada aplicada en los tres artefactos (`architecture.svg`, `architecture_interactive.spec.json`/`.html` y `architecture.png`):** **`45` Python guilds** (alineada con `scripts/check_docs_reality.sh` Check 6).

#### Validación y Entrega con `archify` (`G2` + `G4`)
- Actualizado `docs/assets/architecture_interactive.spec.json` (`revision: "743a7069df6fac1baa28683ab21221e3b3f44823"`, `6 Sovereign Tools`, `45 Python guilds`, nodo `decision_svc` con `label: "Decision Service"`, `sublabel: "reglas + modelo opcional en sombra"`).
- **Comando `archify validate`:**
  ```powershell
  node "C:\Users\FoRJa\.agents\skills\archify\bin\archify.mjs" validate architecture docs/assets/architecture_interactive.spec.json --repo-root . --quality showcase --json
  ```
  - **Exit code:** `0` (`"ok": true`, 9/9 checks passed, 0 errors, 0 warnings).
- **Comando `archify deliver`:**
  ```powershell
  node "C:\Users\FoRJa\.agents\skills\archify\bin\archify.mjs" deliver architecture docs/assets/architecture_interactive.spec.json docs/assets/architecture_interactive.html --repo-root . --quality showcase --json
  ```
  - **Exit code:** `0` (`"ok": true`, `"outputPath": "E:\\tylluan-wt-antigravity\\docs\\assets\\architecture_interactive.html"`, 10 evidencias verificadas contra el repo).
- **Comando `archify visual-check`:**
  ```powershell
  node "C:\Users\FoRJa\.agents\skills\archify\bin\archify.mjs" visual-check docs/assets/architecture_interactive.html --json
  ```
  - **Exit code:** `1` (`overflowX: false`, `overflowY: true`, `scrollHeight: 3015`, `innerHeight: 900`). Nota: el layout vertical de 7 filas (`cols: 2`) heredado de `HEAD` requiere scroll vertical en un viewport de `900px` de alto (`overflowX: false`, sin recorte horizontal ni solapamientos de nodos/aristas). Los archivos temporales `*.visual-check.*` generados durante la inspección fueron eliminados para mantener limpio `docs/assets/`.

---

## Resumen Ejecutivo por Lotes (`D1–D6`, `G1–G7`, `Q1–Q8`)

| ID | Estado | Archivos | Verificación y Hallazgos |
|---|---|---|---|
| **D1** | ✅ Cerrado | `dashboard/src/components/DecisionServicePanel.tsx`, `dashboard/src/App.tsx` | Consume `GET /api/v1/decide/shadow?limit=50` con la forma única congelada `{rows, summary, limit, total}` (deriva `summary` en cliente si falta), muestra los 11 campos de `decision_shadow_log` (v29) y el aviso *"Sin verdad humana: el acuerdo reglas↔modelo no mide precisión"*. |
| **D2** | ✅ Cerrado | `dashboard/src/components/ContradictionsPanel.tsx`, `dashboard/src/components/MemoryConsolidated.tsx` | Consume `GET /api/v1/silva/conflicted?limit=N` (`{nodes:[{id,node_type,content_excerpt,conflicted_since?}], total_conflicted, limit}`), excluye `deprecated_nodes` y muestra el aviso de `~91% falsos positivos` (`benchmarks/human_truth_conflict_pairs_v1.json`). |
| **D3** | ✅ Cerrado | `dashboard/src/components/AuditTrailPanel.tsx` | Consume `/api/v1/security/events` (`api_ops.rs:107-109` → `DiagnosticReport` en `doctor/mod.rs:16-54`), `/api/v1/audit/verify` (`api_audit.rs:5-28`), `/api/v1/audit/latency` (`api_audit.rs:233-300`), `/api/v1/audit/trail` (`api_admin.rs:1546-1586`) y `/api/v1/audit/logs` (`api_admin.rs:753-772`), con estados de carga/vacío/error y pills con palabra explícita. |
| **D4** | ✅ Cerrado | Este informe (Tabla de Triaje) | Triaje de las rutas backend sin consumidor directo en `dashboard/src/` (`CABLEAR`, `API-ONLY`, `DEAD-CANDIDATE`). |
| **D5** | ✅ Cerrado | `dashboard/src/components/TrustConsoleTab.tsx`, `OverviewConsolidated.tsx`, `CoherenceGatePanel.tsx` | Indicador `Sovereign Tools (CONTRACT-01)` (`count / 6`) desde `/health?verbose=true` y puerto `:47004` corregido. |
| **D6** | ✅ Cerrado | `dashboard/src/components/ColoquioCanvasWorkspace.tsx` | Insignia y nota visible explicando `5/6 sovereign tools` en el iframe sandboxed (`tylluan_decide` requiere opt-in `ALLOW_DECIDE`). |
| **G1–G7** | ✅ Cerrado | `docs/assets/architecture.svg`, `architecture.png`, `architecture_interactive.spec.json`, `architecture_interactive.html`, `docs-site/src/**` | Unificados `45` Python guilds, `6 Sovereign Tools`, `Decision Service`, `:47004` y `1,139` tests en SVG, PNG, spec interactivo, HTML interactivo y `docs-site`. |
| **Q1–Q8** | ✅ Cerrado | `dashboard/src/**` | `0` clases `< 11px` (`560 → 0`), `105` atributos ARIA en vistas principales, `71` ocurrencias de `focus-visible`, `137` warnings ESLint (`≤ 150`), `0` colores `#0B0F17`/`#0d1017` fijos y `49` tests Vitest en verde (`8` suites). |

---

## Triaje de Endpoints Huérfanos (D4)

| Endpoint(s) | Archivo Backend | Clasificación | Justificación |
|---|---|---|---|
| `/api/v1/decide/shadow` | `api_v1/routes.rs` (`@deep` T1253) | **CABLEAR** (Hecho en D1) | Cableado en `DecisionServicePanel.tsx` con forma congelada `{rows, summary, limit, total}`. |
| `/api/v1/silva/conflicted` | `api_v1/routes.rs` (`@deep` T1253/T1255) | **CABLEAR** (Hecho en D2) | Cableado en `ContradictionsPanel.tsx` (`{nodes, total_conflicted, limit}`). |
| `/api/v1/silva/contradictions` | `api_v1/mod.rs:610` | **API-ONLY** (Semántica legada) | Devuelve `deprecated_nodes` (`[DEPRECATED by ...]`), no nodos con `conflicted=1`. Excluido de `ContradictionsPanel` por acuerdo en T1255. |
| `/api/v1/audit/verify`, `/api/v1/audit/latency`, `/api/v1/audit/logs`, `/api/v1/security/events` | `api_v1/api_audit.rs`, `api_ops.rs`, `api_admin.rs` | **CABLEAR** (Hecho en D3) | Cableados en `AuditTrailPanel.tsx`. `/api/v1/security/events` (`api_ops.rs:107-109`) devuelve `DiagnosticReport`. |
| `/api/v1/silva/Timeline`, `/api/v1/silva/episodes`, `/api/v1/silva/procedures` | `api_v1/mod.rs` | **CABLEAR** (Futuro) | Útiles para una vista temporal/episódica en `MemoryConsolidated.tsx`. |
| `/api/v1/metrics`, `/api/v1/doctor`, `/api/v1/setup-hint` | `api_v1/api_ops.rs`, `setup_hint.rs` | **API-ONLY** | Consumidos por CLI (`tylluan-cli doctor`), instaladores (`install.sh`/`install.ps1`) y Prometheus. |
| `/api/v1/memory/*`, `/api/v1/silva/search`, `/api/v1/silva/ingest`, `/api/v1/silva/export`, `/api/v1/silva/import` | `api_v1/mod.rs` | **API-ONLY** | API REST para agentes externos, backup/restore y guilds Python (`memory_curator`). |
| `/api/v1/coloquio/post`, `/api/v1/coloquio/recent`, `/api/v1/coloquio/search`, `/api/v1/coloquio/stream` | `coloquio.rs` | **API-ONLY** | Consumidos por clientes MCP/CLI y `coloquio-teamwork`. El dashboard usa `/api/v1/coloquio/messages` y `/api/v1/coloquio/sessions`. |
| `/api/v1/guilds/dispatch/*`, `/api/v1/federation/*`, `/api/v1/mesh/announce`, `/api/v1/mesh/gossip` | `api_mesh.rs` | **API-ONLY** | RPC nodo-a-nodo (M14-D/F) y sincronización entre peers de federación. |
| `/api/v1/a2a/tasks/send*`, `/api/v1/contracts/*` | `a2a.rs`, `api_ops.rs` | **API-ONLY** | Protocolos máquina-a-máquina Google A2A y Bounded Work Contracts (M10). |
| `/api/v1/webhooks/*`, `/api/v1/inference/*`, `/api/v1/routines/*` | `api_ops.rs`, `api_admin.rs` | **DEAD-CANDIDATE** | Sin consumidor en el dashboard; `Sovereign Routine Subsystem` figura inactivo y `/api/v1/inference/*` está solapado con `/api/v1/models/*`. |

---

## Evidencia de Verificación (Comandos + Salidas + Exit Codes)

1. **`dashboard` — TypeScript (`npx tsc --noEmit`):**
   - Comando: `npx tsc --noEmit` (en `E:\tylluan-wt-antigravity\dashboard`)
   - Salida: *(sin errores)* — `TSC_EXIT=0` (exit code `0`).

2. **`dashboard` — Tests Unitarios (`npx vitest run`):**
   - Comando: `npx vitest run` (en `E:\tylluan-wt-antigravity\dashboard`)
   - Salida:
     ```text
     ✓ src/hooks/extracted-hooks.test.tsx (5 tests)
     ✓ src/components/__tests__/OutputsLedgerPanel.test.tsx (3 tests)
     ✓ src/components/__tests__/DecisionAndSecurityPanels.test.tsx (6 tests)
     ✓ src/components/__tests__/WorkContractsPanel.test.tsx (5 tests)
     ✓ src/components/__tests__/StigmergyHeatmapPanel.test.tsx (8 tests)
     ✓ src/components/__tests__/SchedulerConfusionPanel.test.tsx (7 tests)
     ✓ src/components/__tests__/AgentCompetencyPanel.test.tsx (8 tests)
     ✓ src/components/__tests__/FleetHealthPanel.test.tsx (7 tests)
     Test Files  8 passed (8)
          Tests  49 passed (49)
     ```
   - Exit code: `0` (`VITEST_EXIT=0`).

3. **`dashboard` — Build de Producción (`pnpm run build`):**
   - Comando: `pnpm run build` (en `E:\tylluan-wt-antigravity\dashboard`)
   - Salida: `tsc && vite build` → `✓ 3699 modules transformed. ✓ built in 1m 29s`
   - Exit code: `0` (`BUILD_EXIT=0`).

4. **`docs-site` — Build de Producción (`pnpm run build`):**
   - Comando: `pnpm run build` (en `E:\tylluan-wt-antigravity\docs-site`)
   - Salida: `▲ Next.js 16.3.8 (Turbopack) ✓ Compiled successfully in 9.8s ✓ Generating static pages using 4 workers (3/3)`
   - Exit code: `0` (`DOCS_SITE_BUILD_EXIT=0`).

5. **Puertas de Documentación del Repositorio (`check_docs_reality.sh` y `check_doc_links.py`):**
   - Comando: `SKIP_TEST_RUN=1 bash scripts/check_docs_reality.sh && python scripts/check_doc_links.py` (en `E:\tylluan-wt-antigravity`)
   - Salida: `ALL DOCS REALITY CHECKS PASSED` (`REALITY_EXIT=0`) y `OK: 112 markdown files checked, all relative links resolve.` (`LINKS_EXIT=0`).
