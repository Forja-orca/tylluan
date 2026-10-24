# T-DSH-71 — ¿Quién llama a `POST /api/v1/guilds/{guild}/call/{tool}` (y su alias `/tools/`) para `bash`/`filesystem`? (solo lectura)

- **Agente:** dsh
- **Fecha:** 2026-10-24
- **Base:** `origin/main` `42a76a1` (push145), tras reset del worktree. **Solo lectura**: sin cambios de código.
- **Encargo (T1719):** comprobar si algún agente o script del repo llama a `POST /api/v1/guilds/{guild}/call/{tool}` para `bash`/`filesystem` (grep en `scripts/`, `guilds/`, `dashboard`, `docs`) para saber el coste real de la opción C3 de T-DSH-70; tabla `fichero` y `línea`.
- **Método:** `git grep` sobre todo el repo trackeado y búsqueda dirigida en `scripts/`, `guilds/`, `dashboard/src`, `docs/`, `integrations/`, `benchmarks/`, `crates/` (tests incluidos) con los patrones `/call/`, `/tools/`, `guilds/{…}`, `guild_tool_call` y construcción dinámica (`f"{…}/api/v1/guilds/…"`). Además, **lectura** de los logs del kernel de producción (`E:\tylluan\logs\kernel.log*`, sin tocar el proceso) y de `E:\tylluan\data\audit.db` en modo solo lectura, para medir uso real fuera del código.

## 1. Respuesta corta

**Ningún script, guild, agente del repo ni componente del dashboard llama a `/guilds/bash/…` o `/guilds/filesystem/…` por el endpoint directo.** Cero coincidencias en `scripts/`, `guilds/`, `dashboard/`, `integrations/`, `crates/tylluan-cli/`, `.github/`, `benchmarks/`. El único consumidor in-repo del patrón `…/call/…` es el dashboard de visión (`guilds/vision/call/…`), y el patrón `…/tools/…` (mismo handler) solo se usa para `llama_backend` y `coloquio`.

## 2. Llamadas encontradas (todas las que usan el endpoint directo, no solo bash/filesystem)

| Fichero y línea | Llamada | Guild objetivo | ¿Afectada por C3 si C3 = “solo bash/filesystem a operador”? |
|---|---|---|---|
| `dashboard/src/components/VisionTab.tsx`, línea 68 | `POST /api/v1/guilds/vision/call/${endpoint}` | `vision` | **No** (visión, no bash/filesystem). Rompería solo si C3 se aplicara a *todas* las guilds |
| `guilds/core/night_reasoner.py`, línea 232 | `POST /api/v1/guilds/llama_backend/tools/query_model` | `llama_backend` | No |
| `crates/tylluan-kernel/src/security/coherence_gate.rs`, línea 520 | `POST /api/v1/guilds/llama_backend/tools/query_model` (juez interno) | `llama_backend` | No |
| `benchmarks/benchmark_j6_j7_deepeval.py`, línea 33 | `…/guilds/llama_backend/tools/query_model` | `llama_backend` | No |
| `benchmarks/spikes/coherence_gate_reasoning/rebenchmark_real_model.py`, línea 57 | `…/guilds/llama_backend/tools/query_model` | `llama_backend` | No |
| `benchmarks/spikes/coherence_gate_reasoning/spike_hybrid_classification.py`, línea 38 | `…/guilds/llama_backend/tools/query_model` | `llama_backend` | No |
| `benchmarks/spikes/slm_society/experiment.py`, línea 124 | `…/guilds/llama_backend/tools/{tool_name}` | `llama_backend` | No |
| `crates/tylluan-kernel/tests/cola_d33_acceptance.rs`, línea 341 | comentario: el juez llama a `…/guilds/llama_backend/tools/query_model` | `llama_backend` | No (comentario/test) |
| `docs/internal/IMPL_m-buffy-2_redteam_vias_juez_procedencia_buffy_2026-10.md`, línea 61 | ejemplo `curl …/guilds/llama_backend/tools/backend_health` | `llama_backend` | No (doc) |
| `docs/internal/DESIGN_D-34_credencial_guilds_deep_2026-10.md`, línea 99 | mención `guilds/llama_backend/tools/query_model` | `llama_backend` | No (doc) |
| `guilds/wardens/workflows/health-audit.md`, líneas 37 y 40 | `GET /api/v1/guilds` (listar) | — | No (listado, no tool call) |
| `guilds/wardens/workflows/health-audit.md`, línea 94 | `POST /api/v1/guilds/{name}/reset-backoff` | ciclo de vida | No (lifecycle) |
| `crates/tylluan-kernel/src/transport/http/auth.rs`, líneas 1038-1039 y 1098-1099 | tests unitarios de `operator_class_required` y registro de rutas | — | Revisar el test si C3 cambia la clase de auth de bash/filesystem: hoy afirma que `…/guilds/foo/call/...` **no** es operator-class (habría que cubrir el caso bash/filesystem) |

## 3. Cómo llaman de verdad los agentes y las guilds (ruta indirecta, no afectada por C3)

- Guilds Python → kernel por **`POST /api/v1/do`**: `guilds/core/coordinator.py`, líneas 120 y 130; `guilds/core/coloquio_watcher.py`, línea 62; `guilds/core/coloquio_digest.py`, línea 283; `guilds/core/benchmark_routing.py`, línea 246.
- Agentes → **MCP `tylluan_do`** (no REST): en el log real, `POST /messages` aparece 9 359 veces en la ventana de 4 días, frente a solo 4 `POST /api/v1/do`.
- `tylluan_do` resuelve guild + ACL y ejecuta por `registry.call_tool`, así que restringir el *endpoint directo* no toca este camino.

## 4. Uso real medido en los logs de producción (solo lectura)

| Log | Ventana | `bash`/`filesystem` por HTTP | Otras rutas de guild |
|---|---|---|---|
| `E:\tylluan\logs\kernel.log` | 2026-10-24 16:22 → 23:13 (boot actual) | **0** | 173 `llama_backend/tools/query_model`; 6 `coloquio/tools/read_channel`; 4 `coloquio/tools/post_to_channel`; 8+8 `health`/`utilization` |
| `E:\tylluan\logs\kernel.log.1` (50 MB) | 2026-10-20 18:01 → 2026-10-24 16:22 | **5** (todas el 2026-10-23 entre 01:19:00 y 01:20:39): 4 × `bash/tools/bash_execute` + 1 × `filesystem/tools/list_directory` | 1 590 `llama_backend`; 51 `coloquio/tools/read_channel`; 37 `coloquio/tools/post_to_channel`; 254+254 `health`/`utilization`; varios `stop`/lifecycle |
| `E:\tylluan\logs\kernel.log.2` (1,26 GB, histórico) | junio–julio 2026 | **2** `filesystem/tools/filesystem_read` (2026-06-22), más `bash/stop` y `bash/reset-backoff` | — |

- Las 5 llamadas del 2026-10-23 no se corresponden con ningún script del repo (grep = 0): parecen pruebas puntuales de un agente o de José con curl, no uso de tooling. No he podido atribuirlas: **la ruta HTTP directa no escribe `guild_audit_log`** (`api_guilds.rs`, líneas 97-100 llama a `call_tool_as` sin registrar auditoría de guild), y en `E:\tylluan\data\audit.db` no hay ninguna fila entre 01:10 y 01:30 de ese día. Esto refuerza el argumento de C3: esas llamadas no dejan rastro de auditoría.

## 5. Coste real de la opción C3

- **C3 acotada a `bash`/`filesystem`** (la propuesta de T-DSH-70): **coste in-repo = 0** consumidores; en logs, 5 llamadas puntuales en 4 días y 2 históricas de junio. No rompe guilds, ni el dashboard, ni el coordinador, ni MCP, ni benchmarks.
- **C3 aplicada a todas las guilds** (no es lo que propone C3, pero por si se plantea): rompería `VisionTab` (visión), `night_reasoner.py` y el juez de `coherence_gate.rs` (llama_backend) y los benchmarks; coste **alto** y no justificado por este barrido.
- **Aviso de implementación:** `/call/` y `/tools/` comparten el mismo handler (`api_guilds.rs`, línea 50; rutas en `routes.rs`, líneas 32 y 139), así que la restricción por guild tiene que implementarse **dentro del handler** (comprobando guild + clase de auth), no con `operator_class_required` por ruta; y habría que añadir tests en `auth.rs`/`api_guilds.rs` para el caso bash/filesystem.

## 6. Impact

- Da a José/TL el dato que faltaba para decidir C3: **cero consumidores en el repo y uso real marginal (5 llamadas en 4 días, no atribuibles a tooling)**. C3 acotada a bash/filesystem es barata de aplicar sin romper a agentes legítimos del repo.
- Documenta el *porqué* de esa ausencia: los agentes usan MCP (`/messages`, 9 359 llamadas en 4 días) y las guilds usan `POST /api/v1/do`; el endpoint directo es una vía secundaria.
- Hallazgo lateral relevante para C3: la ruta directa **no registra `guild_audit_log`**, así que hoy no hay auditoría de quién la usa; restringirla también mejora esa trazabilidad.

## 7. No verificado

- **Fuera del repo no se puede saber por grep**: scripts sueltos en la máquina de José, curl manuales, clientes externos o agentes con configuraciones propias podrían llamar al endpoint; los logs solo cubren este kernel y estas ventanas (5 h, 4 días y un histórico de junio-julio), no todas las máquinas ni todas las épocas.
- **No confirmé el origen de las 5 llamadas del 2026-10-23**: la ruta no genera fila de auditoría y el log de auth solo muestra método y path, sin token/agent. Podrían ser de un agente de la flota probando, de José o de un script externo.
- **No arranqué ni consulté el kernel en vivo** (solo lectura de ficheros de log y DB en modo `ro`); no probé la ruta ni tokens.
- **No revisé el histórico anterior a junio de 2026** ni logs rotados que ya no existan; `kernel.log.2` conserva lo que hay.
- **No medí** si algún cliente MCP legacy usa el endpoint REST directo en lugar de `tylluan_do` (los clientes MCP hablan `/messages`, que es lo que muestran los logs).
- Sin cambios de código ni kernels tocados.
