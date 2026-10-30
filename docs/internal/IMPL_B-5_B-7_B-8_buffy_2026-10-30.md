# B-5 / B-7 / B-8 (+ verificación de B-9 y B-1) — Buffy, 2026-10-30

**Base:** `44c68fc` (= `origin/main`, confirmado con `git rev-parse --short HEAD` en el primer turno;
el worktree arrancó en `7ed537e` y se rebasó). **COLA:** `E:\tylluan-patches\COLA_buffy_2026-10-29.md`.
**Silencio de carga:** levantado. Todo lo de abajo se ejecutó de verdad, no por lectura de código.

## 0. Dos premisas de la COLA no se sostenían — y ya están resueltas

| Premisa de la COLA | Realidad en `7ed537e` | Estado en `44c68fc` |
|---|---|---|
| «Tus tests de B-1 pasan sin `#[ignore]`, 3 de 3» | Falso: los dos seguían con `#[ignore]` y `api_contracts.rs` tenía **0** menciones de `current_bound_agent_id` | **Cierto**: D-69 entró en `8060a5b` y verifiqué 3/3 sin `ignore` (§4) |
| «Auditar los 72 `#[ignore]`» | La cifra no es 72: son atributos reales **35** (el `git grep "#\[ignore"` cuenta además comentarios: 74 líneas) | **33** hoy, de los cuales **2** siguen sin motivo (§5) |

## 1. B-9 y B-1 — verificación con D-69 dentro (era lo que faltaba)

| Test | Resultado | Qué prueba |
|---|---|---|
| `bwc_agent_binding` (3 tests) | **3 passed, 0 failed, 0 ignored** (57,9 s) | deliver/vote como otro agente → 403 |
| `bwc_close_binding` (2 tests) | **2 passed, 0 failed, 0 ignored** (4,7 s) | cerrar como otro agente → 403; el consolidador real sigue cerrando → 200 |

`close_rejects_claim_of_another_bound_agent` **pasa** con D-69: el hueco que dejé documentado como
abierto está cerrado. Los `#[ignore]` que el TL quitó al aterrizar el push no eran cosméticos —
debajo había arreglo real.

## 2. B-5 — rutas del panel con `dev_mode = false` y `default_role = "guest"`

Método: binario **debug** construido de `44c68fc` (`tylluan-nexus`, 69.127.168 B), puerto **47093**,
`TYLLUAN_DATA_DIR` propio (`test-run-buffy-b5/data`, vacío), cwd `test-run-buffy-b5`. Producción
(`:47004`) intacta todo el tiempo. Config: `dev_mode = false`, `default_role = "guest"`,
`roles = { admin = ["*"], guest = [] }`, y el token maestro mapeado a `admin` en `acl.tokens`.
**Kernel detenido al terminar; puerto 47093 libre (`taskkill` sobre el PID identificado).**

Sondas: `probe_b3.py` (rutas reales extraídas del dashboard) y `probe_b2.py` (flujo OAuth PKCE completo).

### 2a. Matriz ruta → con token → sin token

**85 rutas reales** (86 extraídas; descartada 1 por artefacto de extracción: `/api/v1/coloquio/channels/1${query ?`).

| Resultado | Rutas |
|---|---|
| **protegido: 401 sin token, 200 con token** | **48** |
| escritura (405 en GET, 401 sin token) | 31 |
| público (200 con y sin token) | **3**: `/health`, `/api/v1/mcp/probe`, `/.well-known/agent-card.json` |
| 404 no registrada | 2: `/api/v1/sessions/1`, `/api/v1/skills/1` |
| 400 con token (falta parámetro) | 1: `/api/v1/sessions/resume` |
| **RECHAZAN un token válido** | **0** |

Matriz completa en `test-run-buffy-b5/b5_matrix.json`. **13 rutas devolvieron 429** en el primer
barrido (limitador por IP, 300/min); se repitieron tras 70 s y los estados de la tabla son los de la
repetición — los 429 no se reportan como estado.

Conclusión: cerrar `dev_mode` no rompe ninguna ruta del panel. La única condición sigue siendo la de
la ronda 2: quien abra el panel debe introducir el token una vez (`AuthModal`) o las 48 rutas de datos
devuelven 401.

### 2b. JWT OAuth sin mapeo contra guild restringido

Flujo real, no simulado: PKCE S256 → `GET /oauth/authorize` (200, `consent_id`+`csrf`) →
`POST /oauth/authorize` con la credencial de operador → **307** con `code` → `POST /oauth/token` (200,
JWT con `{"scope":"mcp","sub":"mcp"}`). Consentimiento recordado en `DATA_DIR`.

| Credencial | `POST /api/v1/guilds/filesystem/call/list` | Lectura |
|---|---|---|
| token maestro (mapeado a `admin`) | **200** `Unknown tool: list` | el ACL dejó pasar; falló el tool, no el auth |
| **JWT sin mapeo** (`guest`) | **403** `Role 'guest' does not have access to guild 'filesystem'` | **aceptación cumplida** |
| JWT + `X-Agent-Id: agente-inexistente` | 403 (idéntico) | la cabecera no escala rol |
| JWT → guild `core` | 403 | `guest` es deny-all, no solo `filesystem` |

D-71/D-72 no cambiaron esto a peor: un JWT emitido por el propio `/oauth/authorize` sigue sin rol
propio y hereda `default_role`. **Es la razón por la que `default_role = "guest"` importa**: con
`admin` (valor anterior), ese mismo JWT tendría `["*"]`.

Nota lateral, no es hallazgo: el JWT lleva `aud: "http://localhost:47093"` mientras el issuer es
`127.0.0.1` — inconsistencia de host, no de seguridad, en este montaje.

## 3. B-7 — suite repetida 10 veces

`cargo test -p tylluan-kernel --lib` con `CARGO_TARGET_DIR=/e/tylluan-target-buffy`, binario
`tylluan_kernel-12d13c2237f42b8d.exe`, 10 ejecuciones seguidas.

| Corrida | Duración | Resultado |
|---|---|---|
| 1 | 56 s | 1429 passed · 0 failed · 4 ignored |
| 2 | 47 s | 1429 passed · 0 failed · 4 ignored |
| 3 | 49 s | 1429 passed · 0 failed · 4 ignored |
| 4 | 56 s | 1429 passed · 0 failed · 4 ignored |
| 5 | 53 s | 1429 passed · 0 failed · 4 ignored |
| 6 | 53 s | 1429 passed · 0 failed · 4 ignored |
| 7 | 52 s | 1429 passed · 0 failed · 4 ignored |
| 8 | 57 s | 1429 passed · 0 failed · 4 ignored |
| 9 | 44 s | 1429 passed · 0 failed · 4 ignored |
| 10 | 52 s | 1429 passed · 0 failed · 4 ignored |

**Tests intermitentes: 0/10.** Ninguna corrida imprimió bloque `failures:`.

Los 4 ignorados de la suite lib, todos con motivo escrito (esto cierra el punto que abrí en B-6 sobre
atributos sin justificar *dentro de la lib*):

| Test | Motivo |
|---|---|
| `memory::silva::graph::tests::pagerank_warm_start_benchmark` | benchmark |
| `memory::silva::search::embedding_fingerprint_scale_tests::t1277f_scale_ranking_and_counters` | corpus de 12,5k nodos, minutos |
| `router::embeddings::tests::test_real_inference_bge_m3` | descarga 1,2 GB + ~50 s CPU |
| `router::embeddings::tests::test_real_inference_minilm` | descarga de modelo + ~50 s CPU |

### Panel (`pnpm test`)

Primera tanda, 3 corridas: **4 ficheros fallan siempre** (no intermitente) con
`Failed to resolve import "axe-core"` (3 ficheros) y `"@tanstack/react-virtual"` (1).
**No era un defecto del repo:** ambos paquetes están declarados en `package.json` y presentes en
`pnpm-lock.yaml` desde `e131231`; lo que estaba viejo era mi `node_modules`. Tras
`pnpm install --frozen-lockfile`: **17/17 ficheros, 3 de 3 corridas**. Cero intermitentes.
Lo dejo escrito porque es exactamente el tipo de falso hallazgo que se reporta como bug.

## 4. B-8 — ataques a `tylluan update` con D-70 dentro

Ejecutados como tests reales en `crates/tylluan-cli` (no lectura): `cargo test -p tylluan-cli` →
**47 passed, 0 failed** (41 previos + los 6 nuevos de `mod b8_tests`).

| Caso | Esperado | Obtenido |
|---|---|---|
| Entrada `../../tylluan-nexus` en el tar | no escribir fuera del staging | **rechazado/neutralizado**: el destino se elige por `file_name()`; el fichero queda en el stage con el nombre desnudo y su contenido; ningún ancestro recibe el nombre |
| Entrada anidada `sub/dir/tylluan-nexus` | no recrear la ruta del archivo | **neutralizado**: extrae al nombre desnudo, `stage/sub` no existe |
| `tylluan-nexus` duplicado en el tar | rechazar | **rechazado**: «Release archive contains duplicate …» |
| Hardlink con nombre de binario | rechazar | **rechazado**: «… is not a regular file» |
| Solo `checksums.txt` autenticando otro asset | no autorizar éste | **rechazado**: `checksum_for_asset` → `None` |
| Hash con formato inválido en la línea correcta | ignorar la línea | **rechazado**: `None` |
| Hash válido de otro archivo | fallar la verificación | **rechazado**: `verify_sha256` → error |
| Versión publicada menor que la instalada (`0.17.1` con `0.17.2`) | no degradar | **protegido**: `compare_versions` → `Less`, el brazo no instala |
| `v0.17.9` vs `0.17.10` | comparación numérica, no léxica | **correcto**: `Greater` |
| Pre-release `0.17.2-rc.1` sobre `0.17.2` | no tratarla como actualización | **correcto**: `Less` |

Los cuatro huecos que B-4 documentó el 2026-10-28 (symlink, directorio, bomba de descompresión,
y el checksum sin objeto aparte) están **cerrados** por D-70 y tienen test: `d70_tests` cubre symlink,
directorio y tope de 512 MiB; los seis de arriba cubren lo que faltaba.

**Corrección de mi propia sonda:** la primera versión del test de `..` comprobaba el nombre del
*contenido* (`evil-nexus`) en el directorio padre en vez del nombre del *fichero* que se filtraría
(`tylluan-nexus`), así que no habría detectado una fuga real. Corregido antes de dar nada por bueno:
ahora lee el contenido extraído y recorre los tres ancestros del stage buscando el nombre del binario.

## 5. Estado de `#[ignore]` hoy (actualización de B-6)

| Medición | Valor |
|---|---|
| Atributos `#[ignore]` reales en `crates/` | **33** (eran 35; los 2 de B-1 los quitó el push) |
| Sin motivo escrito (`#[ignore]` pelado) | **2**: `tests/sparse_cascade_bench.rs:67`, `tests/sparse_signature_spike.rs:30` |
| En la suite lib | 4, todos con motivo |
| Deuda mía | mi `bwc_close_binding.rs` ya no necesita `#[ignore]`: D-69 lo cerró y lo verifiqué |

Los 2 pelados son los mismos que señalé en B-6: el código justifica la exclusión (descargan/entrenan),
pero el atributo no lo dice. Sigue siendo una línea cada uno para quien los toque.

## 6. Impact

- **Sin cambios en código de producción.** El único cambio de código es la adición de
  `mod b8_tests` en `crates/tylluan-cli/src/main.rs` (tests de extracción y versiones, sin tocar
  ninguna función de `update`).
- El endurecimiento de `update` (D-70) queda **verificado en ejecución** por primera vez: 6 ataques
  nuevos rechazados, más los 3 que ya traía su propio módulo.
- B-5 confirma que **G1 no requiere tocar el dashboard**: cerrar `dev_mode` con
  `default_role = "guest"` deja 0 rutas rechazando un token válido, siempre que el operador introduzca
  el token una vez. Lo que falta es el aviso en el panel (AG-14), no un arreglo de rutas.
- B-7 no encontró flakiness en 10 corridas de la suite lib ni en 3 del panel; sí encontró que un
  `node_modules` desfasado rompe 4 ficheros de test de forma determinista — útil para quien vea ese
  rojo en local y crea que es del repo.

## 6b. Artefactos e higiene

`test-run-buffy-b5/` queda con las sondas y las matrices (`probe_b2.py`, `probe_b3.py`, `b5_matrix.json`,
`reprobe_result.json`, `kernel.log`, `roles_probe.json`) y **sin secretos**:

- se borró la copia de `.tylluan-token` que usé para la sonda;
- `tylluan.toml` (que llevaba el token maestro mapeado a `admin`) se sustituyó por
  `tylluan.toml.example` con `<TU_TOKEN_MAESTRO>`; para reproducir hay que rellenarlo con el valor de
  `.tylluan-token` del repo;
- se borró `.fastembed_cache` (**1,25 GB** de modelo) y `data/`; el directorio pasó de 1,4 GB a 316 KB.

Comprobado con `grep` del token real sobre cada artefacto: **0 apariciones**. También borré una copia
huérfana de `.tylluan-token` que quedaba en `test-run-buffy-r2/` de la ronda anterior.

## 7. No verificado

- **Release build**: B-5 se hizo con binario **debug**. Comportamiento de rutas y ACL es el mismo
  código, pero los tiempos y el arranque (≈20 min hasta `/health` en debug por carga ONNX y hashing
  del modelo de 1,25 GB) no representan a producción.
- **`tylluan update` contra una release real**: los ataques se ejecutan sobre las funciones de
  extracción/verificación, no contra `api.github.com`. El camino de red (descarga de `checksums.txt`,
  `latest_tag` malformado) sigue sin prueba en vivo.
- **OAuth con `consent_id` ya recordado**: en esta corrida el consentimiento se pidió desde cero (200
  con formulario), pero el camino «recordado» no se forzó.
- **Panel bajo carga**: `pnpm test` 3 veces en reposo. La intermitencia que se sospechaba bajo carga
  no se reprodujo ni se descartó en esas condiciones.
- **Los 33 `#[ignore]` restantes**: B-6 los auditó por lectura en `7ed537e`; aquí solo verifiqué los 4
  de la suite lib y los 2 sin motivo. Los demás no se ejecutaron.
- **`E:\tylluan` (checkout compartido)**: no se tocó. Todos los comandos corrieron en
  `E:\tylluan-wt-buffy` y el kernel de prueba en `test-run-buffy-b5`.
