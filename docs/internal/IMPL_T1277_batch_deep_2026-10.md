# IMPL — T1277-A/B/C (batch de código, Deep, 2026-10-18)

- **Base:** `origin/main` `0ac9dfb` · worktree `E:\tylluan-wt-deep` · **diff sin commitear** (solo el TL commitea).
- **Asignación:** Coloquio #general turno 1278 (TL, 2026-10-18 17:18). T1277-D queda aparte: la propuesta de esquema va por Coloquio y **no se toca código hasta su OK**.

## Qué se implementó

**T1277-A — CoherenceGate honra `untrusted_content` + blind spot de layer 2**
- `security/coherence_gate.rs:86-96` — lee `untrusted_content: true` del metadata JSON del nodo; `:321` lo aplica como penalización (no remoción) en layer 2; fix del blind spot: peer de federación fresco con peso 1.0 ya no queda sin penalizar. Test `:812` `layer2_penalizes_untrusted_content_metadata_flag`. Sin cambio de umbrales.

**T1277-B — errores de storage propagados + límites**
- `memory/silva/nodes.rs:631-638` — `get_conflicted_page` con `COUNT(*)` aparte (no carga el conjunto entero); test `:1787` verifica independencia page/total.
- `router/decision/service.rs:754-787` — `shadow_overview` propaga errores de storage (`read_shadow_log_checked`) en vez de `unwrap_or(0)`; `read_shadow_log` conserva el comportamiento tolerante para sus callers.
- `transport/http/api_v1/mod.rs:655` — `sanitize_excerpt` elimina caracteres Unicode de formato (Cf) además de lo previo; `api_decide.rs` lo consume (+17 líneas de propagación).
- Tests T1253 nº5/6 con las **mismas assertions** (solo se añaden casos nuevos).

**T1277-C — e2e real del gate P2P dispatch (deuda declarada ADR-019 ≈L132)**
- `transport/http/mod.rs:1038` `build_p2p_dispatch_gate_handler` y `:1112` `build_p2p_approved_peers` — **extracción** de los closures inline del spawn a `pub fn` (mismo comportamiento, habilita que el test conduzca EXACTAMENTE los closures de producción). El bloque spawn ahora las llama.
- `transport/http/api_v1/api_mesh.rs:199-206` — rama `Ok` de `RemoteTcp` devuelve `let mut body` y añade `body["error"]` si el gate denegó: sin esto la denegación era invisible en HTTP (solo en logs).
- `tests/mesh_audit.rs:702/755/818` — 3 tests e2e con listener Noise real (`start_p2p_listener_noise`), `PeerDb::open(":memory:")`, RegistryActor real y las closures de producción (rig en `:640-700`):
  - `c1` allow-list: pasa el gate, llega al registry (error esperado = "Guild not found" — montar guilds reales queda fuera de alcance), sin fila de auditoría del agente.
  - `c2` deny-all: `tool not allowed by peer policy` + exactamente 1 fila de auditoría (preview/intent correctos).
  - `c3` peer no aprobado: rechazo en handshake **antes** del handler ("peer not approved" / "P2P dispatch failed" según carrera FIN/RST), 0 filas `x25519:%`.
- Renombre `T1278`→`T1277` en 15 ocurrencias / 7 ficheros (label de la tarea, no confundir con el turno de asignación 1278).
- Recuento: kernel lib **1057** (+4), link 74, fsrs 12, total **1143** — `AGENTS.md`/`README.md`/`STATUS.md:50` al día (`STATUS.md:50` incluye HEAD `0ac9dfb` + WIP, fecha 2026-10-18).

## Verificado (comando → resultado)

| Comando | Resultado |
|---|---|
| `cargo check -p tylluan-kernel --all-targets` | limpio (solo warning esperado bundled-dashboard) |
| `cargo test -p tylluan-kernel --test mesh_audit` | **14 passed** (incl. c1/c2/c3) |
| `cargo test -p tylluan-kernel --test decide_admin_read_api` | **6 passed** (T1253 5/6, assertions sin cambiar) |
| `cargo test -p tylluan-kernel --lib` | **1057 passed** (3 ignored) |
| `cargo test -p tylluan-link --all-targets` + `-p tylluan-fsrs` | **74** + **12** passed |
| `scripts/check_test_count.sh` | **EXIT=0** (README 1143 ✅ · AGENTS 1057/74/12=1143 ✅) |
| `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` | **EXIT=0**, limpio |
| `scripts/check_docs_reality.sh` | ver hallazgo abajo — el único ❌ restante NO es de este diff |

**Hallazgo `check_docs_reality` (no es del repo):** en MI entorno el gate sale `exit=1` solo por `ROADMAP_O3.md:42 onnx/int8/decima.json`. Bisect hecho: el `bash` de este entorno es WSL-bash y no resuelve el `gitdir:` de Windows del worktree → el script cae al fallback `find` → `DIRNAME` se contamina con `onnx/` de `.fastembed_cache/**/onnx/` en disco → no aplica el skip que la **propia línea 239 del script** documenta con esa ruta como ejemplo canónico. Inyectando un `git ls-files` real (1091 ficheros, 0 con componente `onnx`): el ❌ de decima **desaparece y solo queda el de STATUS** (que ya corregí) → con input correcto el gate queda **exit 0**. CI (checkout fresh Linux) y Git Bash del TL no ven este artefacto; **no hay que tocar ROADMAP_O3**.

## NO verificado / fuera de alcance

- `scripts/verify.sh` completo (≈10 min) — **no corrido**; en mi entorno el gate docs sale 1 por el artefacto WSL descrito arriba.
- Rendimiento/latencia del gate e2e — es un test funcional, sin medición.
- Nada de dashboard (`pnpm` no corrido), ni CI, ni releases.
- No arranqué kernels (puerto `47004` y `tylluan.toml` intactos — reglas de la casa).
- **No commiteado nada** (regla: solo Claude Code commitea). No he corrido `check_no_predation.sh` (es post-commit); el `## Impact` de este informe sirve de base para el mensaje de commit.
- Directorios `test-run-ort/`, `test-run-p2/` (y `test-run-md3/`, oculto por su `.gitignore` interno) = artefactos locales de tests, **NO commitear**: `git add -A` los arrastraría (GBs de modelos). Considerar añadir `test-run-*/` a `.gitignore` (iniciativa propia propuesta, no aplicada).

## Impact

- **Cambios en código de producción más allá de tests (declarar en el commit):**
  1. `transport/http/mod.rs` — extracción a `pub fn build_p2p_dispatch_gate_handler` / `build_p2p_approved_peers`; comportamiento idéntico, mismo closure sobre `RegistryHandle`/`PeerDb`.
  2. `api_v1/api_mesh.rs` — campo `error` condicional en `Ok(resp)` de `/guilds/dispatch/remote`: solo afecta a ese endpoint; único consumidor con tests es `mesh_audit.rs` (dashboard u otros no lo leen).
  3. `api_v1/mod.rs` `sanitize_excerpt` — más agresivo con formato Unicode (Cf): afecta a excerpts de `tylluan_decide`; tests T1253 cubren.
- **Docs:** recuentos 1139→1143 en `AGENTS.md`/`README.md` + línea canónica `STATUS.md:50`.
- **Consumidores afectados:** ninguno roto (1143 tests + clippy verde); `dispatch/remote` ahora informa del motivo de denegación en el body (añadido, no cambio de contrato previo).
- **Riesgo:** bajo — las 2 funciones extraídas son movimientos mecánicos y el resto son tests nuevos.
