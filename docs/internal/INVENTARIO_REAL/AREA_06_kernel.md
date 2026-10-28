# ÁREA 6 — Kernel: mapa de módulos y cableado

Base: `a564880` · 2026-10-28 · lectura pura (sin compilar nada para esta tanda) · `crates/tylluan-kernel/src`

## Resumen

**24 módulos de primer nivel**, todos declarados en `src/lib.rs` (líneas 15-38) y todos con al menos
un consumidor real. No hay ningún módulo huérfano: los cuatro que primero salieron con cero
referencias internas (`logging`, `guard`, `tunnel`, `version_check`) se usan desde `src/main.rs`
con la ruta cualificada `tylluan_kernel::<módulo>`, no con `crate::<módulo>`. Quedan por eso fuera
del alcance de la librería: solo el binario los usa.

Peso: `transport` (68 ficheros, 36 374 líneas) + `memory` (59, 25 798) + `security` (21, 9 756) +
`router` (17, 9 896) + `registry` (12, 7 140) = **89 %** del kernel. El resto son 14 módulos de
100-900 líneas.

## Tabla de módulos

`refs` = ficheros **fuera del propio módulo** que lo referencian con `crate::<m>` o
`tylluan_kernel::<m>`; `main` = referencias en `src/main.rs`.

| Módulo | Ficheros / líneas | Qué es | refs | main | Estado |
|---|---|---|---|---|---|
| `config` | 1 / 3 244 | `TylluanConfig`, perfiles, `find_config_file`, override de sandbox por sesión | 56 | 7 | REAL |
| `security` | 21 / 9 756 | CoherenceGate (4 capas), puerta de escritura, trust/cuarentena, grants, rate limit, hooks, Agent Contract | 30 | 3 | REAL |
| `transport` | 68 / 36 374 | Servidor MCP + REST/A2A/SSE, handlers de las 6 soberanas, OAuth, guardas Origin/Host | 7 | 8 | REAL |
| `registry` | 12 / 7 140 | `GuildRegistry`, proxy MCP (stdio/HTTP/SSE), `GuildProcess`, actor, supervisor, service manager | 35 | 3 | REAL |
| `memory` | 59 / 25 798 | Silva (nodos, grafo, búsqueda híbrida), FSRS, mailbox, coloquio, task context, jobs, agent nodes | 47 | 14 | REAL |
| `router` | 17 / 9 896 | Matcher semántico, catálogo de guilds, embeddings ONNX, servicio de decisión (`decide`), scheduler | 40 | 10 | REAL |
| `doctor` | 1 / 378 | Diagnóstico del kernel: guilds, Silva, curriculum | 12 | 1 | REAL |
| `curriculum` | 1 / 367 | CurriculumLearner (aprendizaje por ejemplos del Signal Loop) | 11 | 1 | REAL |
| `federation` | 2 / 928 | `PeerDb`, permisos de federación | 5 | 0 | REAL |
| `repo_map` | 1 / 498 | Mapa de identificadores del workspace (alimenta `/api/v1/repo-map`) | 3 | 0 | REAL |
| `recall_telemetry` | 1 / 188 | D-5: latencias de `tylluan_recall`, expuesto en `/health` | 2 | 0 | REAL |
| `hormones` | 1 / 156 | `HormoneSystem`: señal global que modula el matcher | 2 | 1 | REAL |
| `consensus` | 1 / 242 | Resolución determinista de frescura (paper 1.3), usada por la ingesta de federación | 1 | 0 | REAL |
| `metrics_ring` | 1 / 129 | Ring buffer de métricas de proceso/CPU para el dashboard | 1 | 0 | REAL |
| `maintenance` | 1 / 386 | Descarga de modelos/actualizaciones con progreso por broadcast | 1 | 4 | REAL |
| `process_isolation` | 1 / 98 | Job Object de Windows: cap de CPU por guild | 1 | 0 | A_MEDIAS (ver H-2) |
| `eval` | 2 / 343 | Evaluaciones internas expuestas por `/api/v1/eval` | 1 | 0 | REAL |
| `mlp` | 2 / 216 | `MlpScorer` + `ReplayBuffer` (reranker aprendido); se crea en `main.rs` y se inyecta con `set_mlp` | 1 | 1 | REAL (uso efectivo en routing: **SIN_VERIFICAR**) |
| `metrics_exporter` | 1 / 37 | `/metrics` formato Prometheus | 1 | 0 | A_MEDIAS (ver H-3) |
| `version_check` | 1 / 208 | Aviso de versión nueva al arrancar (T999) | 0 | 1 | REAL (solo binario) |
| `logging` | 1 / 238 | Rotación de logs por tamaño (D-4) | 0 | 2 | REAL (solo binario) |
| `guard` | 1 / 102 | `GuardedTask`: tareas con nombre y manejo de panic | 0 | 1 | REAL (solo binario) |
| `tunnel` | 1 / 224 | `TunnelManager` para la URL de túnel WSL | 0 | 1 | REAL (solo binario) |

Además: `setup.rs` y `cleanup.rs` son módulos **del binario** (`mod setup; mod cleanup;` en
`main.rs:5-6`), no de la librería; `test_support.rs` es `pub(crate) mod test_support`
(`lib.rs:41`), solo para tests unitarios.

## Hallazgos de esta área

**H-1 · Project Skills: dos almacenes para una sola operación** — MAL_PLANTEADO, ALTA, S.
Detalle completo y tests en `docs/internal/IMPL_M-BUFFY-14_buffy_2026-10.md` (M-BUFFY-14).
`transport/http/api_v1/api_admin.rs:1459-1570` (fichero) contra
`transport/server/handler_do/skill.rs:31-150` (SilvaDB).

**H-2 · `process_isolation`: 20 % de CPU fijo a todo guild stdio, después del handshake, y el
fallo solo deja un `warn!`** — A_MEDIAS, ALTA, S.
`registry/proxy.rs:130` llama `isolate_guild_process(guild_name, 20)`; el 20 no viene de
configuración ni del catálogo de guilds: está escrito en la llamada. En Windows crea un Job
Object con `JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP`
(`process_isolation.rs:2-16`, `CpuRate = 20 * 100` = 20 %). Tres consecuencias verificables por
lectura: (a) el cap se aplica **después** del handshake MCP, así que el arranque del guild no está
limitado; (b) los guilds HTTP/SSE (`HttpMcpProxy`, `SseMcpProxy`) **no** pasan por ahí, solo los
stdio; (c) si el Job Object falla, el guild sigue corriendo sin límite y en el log queda un
`⚠️ Could not apply CPU isolation`. No hay ninguna prueba de que 20 % sea suficiente para un guild
de inferencia; **no lo he medido** y no lo afirmo.

**H-3 · `/metrics` no existe en los binarios publicados** — A_MEDIAS, MEDIA, S.
`metrics_exporter` está detrás de `#[cfg(feature = "observability")]` (`lib.rs:31-32`), la ruta se
registra bajo el mismo `cfg` (`transport/http/mod.rs:1385-1386`) y `observability` **no** está en
`default` (`Cargo.toml`, `default = ["semantic"]`). `release.yml` no pasa `--features` en ningún
sitio (`grep -c features release.yml` → 0), así que la release se construye con los defaults: ni
`/metrics` ni dashboard embebido. Quien quiera métricas Prometheus tiene que compilarse el kernel.

**H-4 · `tylluan://skills` no son las skills del proyecto** — MAL_PLANTEADO, MEDIA, S.
El recurso devuelve un catálogo de intents de ejemplo **escrito a mano**, duplicado literal en dos
ficheros: `transport/http/api_v1/mcp.rs` y `transport/server/mod.rs` (2 apariciones de
«Tylluan Skill Catalog» en cada uno). Un cliente MCP que lo lea buscando las habilidades del
proyecto recibe texto fijo.

**H-5 · Cuatro módulos solo usables desde el binario** — nota, BAJA, S.
`logging`, `guard`, `tunnel`, `version_check` no tienen ningún consumidor dentro de la librería.
No es código muerto (el binario los usa), pero cualquier consumidor de `tylluan_kernel` como
librería —los tests de integración, por ejemplo— no puede tocar rotación de logs ni el chequeo de
versión sin pasar por `main.rs`.

## Autoauditoría (5 filas re-verificadas con su comando)

| Fila | Comando | Resultado |
|---|---|---|
| `config` 56 refs | `grep -rl 'crate::config\b' --include=*.rs src \| wc -l` | 56 ✅ |
| H-2 cap de 20 | `grep -c 'isolate_guild_process(guild_name, 20)' registry/proxy.rs` | 1 ✅ |
| H-3 `observability` | `grep -c 'cfg(feature = "observability")' src/lib.rs` | 1 ✅ |
| H-3 release sin features | `grep -c 'features' .github/workflows/release.yml` | 0 ✅ |
| H-4 catálogo duplicado | `grep -rc 'Tylluan Skill Catalog' --include=*.rs src` | `mcp.rs:2`, `server/mod.rs:2` ✅ |

## No verificado

- **No** he ejecutado una llamada real a `mlp`/`MlpScorer` en routing: sé que `main.rs:1369-1370`
  lo crea e inyecta, no que sus puntuaciones cambien una decisión en producción.
- **No** he medido el efecto del cap del 20 % sobre un guild de inferencia (H-2). Sin medición, no
  afirmo que sea la causa de ninguna lentitud.
- **No** he comprobado qué módulos se compilan fuera de Windows (todo el análisis es del árbol
  fuente; `process_isolation` tiene rama `not(target_os = "windows")` que solo avisa).
- Los propósitos de `eval`, `mlp`, `hormones`, `guard` los deduzco de su uso, no de documentación:
  esos módulos no tienen cabecera `//!`.
