# B-1 (H1) — Higiene del ROADMAP_O3.md: secciones ADR-019/020/021, OAuth, dashboard y producción

- **Autora:** Buffy · **Fecha:** 2026-10-19 · **Árbol:** `E:\tylluan-wt-buffy`, detached en `origin/main` = `17465b9`
- **Alcance:** verificación de las casillas esas secciones de `docs/roadmap/ROADMAP_O3.md` (952 líneas) contra código, historial de git y el kernel vivo (`/health` → `{"status":"ok","version":"0.17.1","commit":"2458b4a"}`). **El ROADMAP no se ha editado** (lo hace el TL).
- **Criterio:** cada afirmación se comprobó con `git show`/`merge-base --is-ancestor`, `grep` sobre el código actual, o `curl` al kernel vivo. Donde no pudo verificarse desde esta máquina se marca explícitamente.

## Tabla — ADR-019 (redes por equipo)

| Casilla (línea aprox.) | Estado declarado | Estado real | Evidencia |
|---|---|---|---|
| Paso 5 — spike iroh NO-GO, rama solo local | [x] | **Confirmado** | `git branch --list '*iroh*'` → solo `spike/iroh` local; sin rama remota |
| Paso 2 — permisos por par/espacio (`742eacf`, "publicado en `9070c66`") | [x] | **Confirmado con matiz** | `742eacf` existe y es ancestro de `main` y de `9070c66`. Matiz: `9070c66` solo toca `STATUS.md` ("docs(status): sync") — la lectura correcta es "anunciado en STATUS", no "publicado ahí el código" |
| Paso 2 deuda T1277-C (`052313e`): 3 tests e2e gate dispatch P2P + motivo de denegación en `dispatch/remote` | [x] | **Confirmado** | `tests/mesh_audit.rs:702` c1, `:755` c2, `:818` c3 (listener Noise real vía `start_p2p_listener_noise`); `api_mesh.rs:195` "surface the remote's own failure reason" |
| federation_audit 12/12, mesh_audit 11/11 (en esa casilla) | [x] | **Consistente** (hoy mesh_audit = 14 tests: T1277-C añadió 3) | recuento `#[test]`/`#[tokio::test]`: federation_audit 12, mesh_audit 14 |
| OAuth Fase 1 (`61bc97e`) "No desplegada aún en el kernel de José" (línea 54) | [x] | **OBsoleto en su parte descriptiva** — la Fase 1 SÍ está desplegada en el kernel vivo | `git merge-base --is-ancestor 61bc97e 2458b4a` → **SI**; `/health` en vivo = `2458b4a` |
| "Fase 2 (consentimiento, registro de clientes, scopes) sin aprobar" | descrito | **Confirmado** | no existe `client_registry`/`register_client` en `src/transport/http/`; no hay doc T1281 en `docs/internal/` |
| Paso 3 — invitación de un solo uso [ ] | abierto | **Consistente** | sin código de invitaciones |
| Paso 4 — revocación inmediata [ ] | abierto | **Consistente con matiz** | existe solo `revoked: Mutex<HashSet<String>>` in-memory en `oauth.rs:43` (se pierde al reiniciar); revocación persistente = no |
| Paso 6 — Coloquio federado; "coloquio_channels existe pero es inerte" [ ] | abierto | **Confirmado** | `federation/permissions.rs:5`: "`coloquio_channels` is stored but INERT until step 6" |
| Migración de memorias a espacios [ ] | abierto | **Confirmado** | sin uso de `metadata.space` en `memory/silva/mod.rs` |
| T1277-A: tests layer2 federation_node y untrusted_content | [x] | **Confirmado** | `coherence_gate.rs:792` y `:812` |

## Tabla — ADR-020 / ADR-021

| Casilla | Estado declarado | Estado real | Evidencia |
|---|---|---|---|
| ADR-020 redactado v7 PROPUESTO (`3369bb8`) | [~] | **Confirmado** | `git show 3369bb8` existe; `ADR020_inference_gateway.md` línea 3 = "📐 PROPUESTO (v7)" |
| ADR-020 "Hecho en código: `embed` y `decide` (`[decision.provider]`, `config.rs:1944`)" | [~] | **Confirmado** (cita de línea desplazada: el campo `provider` está en `config.rs:1940-1941`, no 1944) | `grep` en `config.rs` |
| ADR-020 "clase de privacidad graduada (hoy solo el booleano loopback, `systemone_http.rs:39`)" | [~] | **Confirmado** | `systemone_http.rs`: `is_loopback_endpoint() -> bool`, sin graduación |
| ADR-020 "Falta: … **filtro por huella de modelo en `search.rs`**" | [~] | **OBsoleto** — el filtro YA existe (T1277-D `c00b902`, T1277-E `8ad317b`), en **modo observación** por defecto | `search.rs:10-26` (fingerprint gate), `search.rs:87` `fingerprint_filter_enabled()`, `search.rs:1317` "observation by default"; test `observation_mode_counts_without_excluding` |
| ADR-021 "sin proveedor de modelo todavía" (cabecera del ADR) | descrito | **OBsoleto** — hay proveedor cableado: `[decision.provider]` kind `systemone_http` y el shadow log vivo ya registra evaluaciones de modelo | `config.rs:1980`; `GET /api/v1/decide/shadow` en vivo → fila `id 2116`, `model_identity:"systemone-http-v1:decima-small"`, `model_value:"false"` |
| ADR-021 "Fase 1 hecha: tool + REST + reglas + sombra" (línea 66) | [~] | **Confirmado** | `tylluan_decide` en `server/mod.rs:114,297`; `routes.rs:21` (`/api/v1/decide`) y `:307` (`/decide/shadow`); `rules.rs`/`service.rs` |
| "Faltan … **evaluación pareada**" (línea 66) | [~] | **OBsoleto** — la evaluación pareada existe: `paired_eval.rs` con McNemar exacto bilateral sobre `decision_shadow_log`, e informes T1253 (`IMPL_T1253_shadow_conflicted_deep_2026-10.md`, revisión `REVISION_T1253_buffy_2026-10-18.md`) | `paired_eval.rs:18,41,140-180,296` |
| "…y **calibración**" (línea 66) | [~] | **Abierto — confirmado** | sin escalado de temperatura en `decision/service.rs` |
| Casilla línea ~72: "filtro por `model_hash` en `search.rs` (sin aprobar) y trait `Embedder`" [ ] | abierto | **Parcialmente obsoleto**: el filtro existe (modo observación); `trait Embedder` no existe | grep `trait Embedder` → 0 resultados |

## Tabla — OAuth (línea 83) y Producción (líneas 92-94)

| Casilla | Estado declarado | Estado real | Evidencia |
|---|---|---|---|
| "OAuth: `redirect_uri` con userinfo/backslash rechazado (`1a2b6a0`); suite adversarial F2: 8 casos, 7 `#[ignore]` hasta T1281" | [x] | **Confirmado en el detalle** | spec en `main`: 8 `#[tokio::test]`, 7 `#[ignore = …]` (solo C2b activo, porque su fix ya landing); `1a2b6a0` es ancestro del kernel vivo |
| "OAuth F2 sigue abierto: el diseño de Codex (T1281) no se ha entregado" | descrito | **Confirmado** | `ls docs/internal/` sin ficheros T1281/consentimiento |
| Producción: "Reinicio del kernel: … **Hoy corre `f52be7c`**; la Fase 1 de OAuth (`61bc97e`) no está desplegada" (línea 92) [ ] | abierto | **OBsoleto en ambas partes** — el kernel vivo corre `2458b4a` (no `f52be7c`) y SÍ incluye la Fase 1 (`61bc97e`) y el fix userinfo (`1a2b6a0`) | `/health` en vivo; ambos commits ancestros de `2458b4a`. Consecuencia operativa: la razón declarada para esperar el reinicio ya no aplica; **sigue abierto el matiz de que `dev_mode = true` en producción** (`tylluan.toml:198`), así que `/oauth/authorize` auto-aprueba aún |
| "[p2p] enabled = false como contención" [x] | [x] | **Confirmado** | `/e/tylluan/tylluan.toml` `[p2p] enabled = false`; puerto 9123 sin escuchar en `netstat` |
| "Rotación del token maestro: recomendada al pasar a `dev_mode=false`" [ ] | abierto | **Consistente** | `dev_mode` sigue `true`; rotación no verificable desde aquí (**no verificado**) |

## Tabla — Dashboard (línea ~89)

| Casilla | Estado declarado | Estado real | Evidencia |
|---|---|---|---|
| Vistas/avisos (`5cd3f1f`, `351cac2`, `6b4ea57`, `ec75b90`, `20fd160`; eslint 129→39) | [x] | **Confirmado** | los 5 commits existen en `main`; recuento de avisos no remedido (**no verificado** — requiere `pnpm lint` con el entorno del dashboard) |
| "Pendiente: reconstruir el build del dashboard (directorio dist)" | pendiente | **Hecho** — `dist` reconstruido hoy | `/e/tylluan/dashboard/dist` mtime 2026-10-19 22:09; `GET /` del kernel → 200 |
| "…y reiniciar el kernel (hoy corre `0ac9dfb`)" | pendiente | **Hecho** — vivo corre `2458b4a` (descendiente de `0ac9dfb`) | `/health` |
| "captura de Coloquio en tema claro sin demostrar" | pendiente | **Sigue sin demostrarse desde esta máquina** — el informe lote W (`8cf87f4`) cita `visual_reviews/batch_claude/coloquio_light_1440.png`, pero esos PNG **no existen** en este worktree ni en `E:\tylluan` | `ls visual_reviews/batch_claude/` → no encontrado en ambos árboles; el informe de Antigravity puede tenerlos en su propio entorno (**no verificado**) |

## Hallazgos para el TL (resumen)

1. **Tres casillas con hechos desactualizados sobre el kernel vivo** (producción línea 92, OAuth línea 54, dashboard línea ~89): el vivo corre `2458b4a`, que ya incluye Fase 1 OAuth (`61bc97e`) y fix userinfo (`1a2b6a0`), y el `dist` del dashboard ya está reconstruido. La premisa "esperar a cerrar OAuth para reiniciar" quedó consumida por hechos; queda como decisión pendiente el `dev_mode = true` de producción.
2. **Dos "faltan" de ADR-020/021 que ya existen**: el filtro por huella de embeddings en `search.rs` (modo observación, T1277-D/E) y la evaluación pareada McNemar (`paired_eval.rs`, T1253). Deben pasar a [~] con su matiz (observación ≠ enforcement; calibración sigue abierta).
3. **Matiz menor de cita**: "`config.rs:1944`" (campo `provider`) está desplazado a `config.rs:1940-1941`; y "publicado en `9070c66`" del paso 2 es un sync de `STATUS.md`, no un commit de código.

## Impact (ADR-013)

- **Zonas tocadas:** solo este informe (`docs/internal/IMPL_b1_roadmap_buffy_2026-10.md`). Cero cambios en `src/`, `guilds/`, `integrations/` o `tylluan*.toml`. El `ROADMAP_O3.md` NO se editó (exigencia del ítem: lo actualiza el TL).
- **Efecto en despliegue/nucleo:** ninguno. Verificación en modo lectura sobre el worktree propio (`E:\tylluan-wt-buffy`, detached `origin/main`); el kernel de producción solo se consultó con `GET /health` y `GET /api/v1/decide/shadow` (lectura).
- **Riesgo:** residual mínimo — un lector del ROADMAP puede fiarse de "no desplegada"/"falta X" obsoletos hasta que el TL aplique las correcciones sugeridas.
- **No verificado:** recuento real de avisos de eslint (requiere entorno dashboard); existencia de los PNG de Coloquio en claro en el entorno de Antigravity; rotación efectiva del token maestro.

## Checks ejecutados

- `git fetch` + `git checkout --detach origin/main` (regla de la cola) → HEAD `17465b9`, worktree limpio salvo `PROPUESTA_advisory_gate_lane_buffy_2026-10-17.patch` (entrega propia previa sin commitear).
- Verificaciones por comando en vivo citadas en cada fila. Sin `cargo` en esta fase (sin cambios de código).
