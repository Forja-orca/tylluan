# FLAKY — `hybrid_classify_is_inert_in_test_builds` (2026-10-15)

**Origen:** tarea de Coloquio (T1190) + corrección del Tech Lead del mismo día, que refutó mi
primera entrega y acertó en la causa. Este informe sustituye al anterior: la causa raíz, las
cifras y la recomendación cambian.

**Resultado en una línea:** el fallo intermitente de CI es una **carrera sobre `RECALL_COUNTER`**,
un contador global de proceso (no la DB de telemetría) que dos familias de test mueven a la vez:
el canario lo lee, espera 25 `yield` y exige que **no** se haya movido, mientras tres tests de
`workflow_id_*` lo incrementan con cada llamada. Arreglado con un guard compartido, **sin tocar
ninguna aserción**, y validado con el mismo estrés sobre binarios reales: antes **10 fallos en 180
runs**, después **0 en 300**.

**Alcance:** solo código de test (`#[cfg(test)]`) en
`crates/tylluan-kernel/src/security/coherence_gate.rs`, más el endurecimiento secundario ya
entregado en `crates/tylluan-kernel/src/security/friction_log.rs`. La ruta de producción no cambia.
En `E:\tylluan-wt-buffy`, sin commitear.

---

## 1. Causa raíz

`RECALL_COUNTER` es un `AtomicI64` **compartido por todo el proceso de test**
(`crates/tylluan-kernel/src/security/coherence_gate.rs:239`). Lo tocan dos familias de test:

| Test | Qué hace con el contador |
|---|---|
| `hybrid_classify_is_inert_in_test_builds` (canario, línea 633) | Lo **lee** en la línea 666 (`before`), cede 25 veces el hilo, y en la línea 685-689 exige que siga igual. Esa inmovilidad **es** la aserción: si el contador se movió, el cuerpo del juez corrió en un build de test. |
| `workflow_id_is_nonzero_and_deterministic` (línea 1047) | Llama dos veces a `generate_workflow_id` → **+2**, y exige `after - before == 2` (línea 1057). |
| `workflow_id_groups_same_query` (línea 1069) | Una llamada → **+1**. |
| `workflow_id_different_for_different_queries` (línea 1091) | Dos llamadas → **+2**. |

`generate_workflow_id` (línea 1035) incrementa el contador y es el **único escritor del lado de
test** (verificado: `grep -rn RECALL_COUNTER` solo aparece en este fichero, y el otro escritor —la
línea 375— está dentro de `#[cfg(not(test))]`).

libtest ejecuta tests en paralelo. Si un test `workflow_id_*` cae dentro de la ventana
`before` → 25 yields → assert del canario, el contador se mueve y el canario falla con la
diferencia exacta de las llamadas que hayan coincidido. La ventana abarca un `await`, así que no
es un instante: bajo la planificación real de libtest es alcanzable de forma **intermitente**, que
es exactamente lo que se veía en CI.

**Por qué el testigo en CI fue el canario y no el test hermano:** CI ejecuta
`cargo test -p tylluan-kernel --lib` sin flags de paralelismo, así que libtest usa los núcleos del
runner (2-4 vCPU). En ese rango de paralelismo los tests `workflow_id_*` arrancan *después* del
canario, dentro de su ventana, y el que falla es el canario; con paralelismo alto arrancan todos a
la vez y el que falla es el hermano (`+3` donde espera `+2`). Verificado en ambas direcciones en
§2: es la misma carrera vista por dos testigos distintos.

**Correspondencia con CI, verificada contra el código que CI ejecutó:** en `HEAD`, la línea 653 es
el `assert_eq!` del canario comparando `RECALL_COUNTER.load(Ordering::Relaxed)` contra `before` —
las cifras «4 y 6» del ROADMAP son **valores del contador**, no filas de telemetría, y pertenecen
a esta aserción. La línea 1004 del mismo `HEAD` es el `after - before == 2` del test hermano.

## 2. Reproducción antes del arreglo (binarios reales, sin espejo)

Artefacto: el binario de la suite lib (`cargo test -p tylluan-kernel --lib --no-run`), filtrado al
módulo `security::coherence_gate` (26 tests), sin ningún test nuevo ni harness espejo. Señal =
código de salida; clasificación por la sección `---- <test> stdout ----`.

| `--test-threads` | Runs | Fallos | Test que falla |
|---|---|---|---|
| 4 (lo que usa CI en un runner de 4 núcleos) | 60 | **5** | canario, los 5 |
| 8 | 60 | **3** | canario, los 3 |
| 64 | 60 | **2** | `workflow_id_is_nonzero_and_deterministic`, los 2 |

Fallo verbatim (uno de los 5, `--test-threads=4`):

```
---- security::coherence_gate::tests::hybrid_classify_is_inert_in_test_builds stdout ----
thread 'security::coherence_gate::tests::hybrid_classify_is_inert_in_test_builds' panicked
  at crates\tylluan-kernel\src\security\coherence_gate.rs:653:9:
assertion `left == right` failed: hybrid_classify must not run the Layer 4 judge body in test builds
  left: 4
 right: 3
```

y el del mismo mecanismo visto desde el otro test (`--test-threads=64`):

```
---- security::coherence_gate::tests::workflow_id_is_nonzero_and_deterministic stdout ----
thread '...' panicked at crates\tylluan-kernel\src\security\coherence_gate.rs:1017:9:
assertion `left == right` failed: RECALL_COUNTER must increment on each generation
  left: 3
 right: 2
```

Deltas observados en las 8 salidas capturadas: **+1, +2, +3 y +4** — es decir, el número de
llamadas a `generate_workflow_id` que coincidieron con la ventana (1 o 2 por test, tres tests).
El `+2` que vio CI (4 → 6) es uno de esa misma familia. **Misma carrera, dos testigos distintos.**

## 3. El arreglo

Un guard compartido que ambas familias toman, en el propio módulo de test
(`crates/tylluan-kernel/src/security/coherence_gate.rs:544`):

```rust
static RECALL_COUNTER_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
```

- El canario lo toma como **primera** sentencia y lo mantiene durante toda la ventana
  (línea 636: `let _counter_guard = RECALL_COUNTER_LOCK.lock().await;`). Tiene que ser
  `tokio::sync::Mutex`: el guard atraviesa un punto de `await` (el bucle de 25 yields) y
  `clippy::await_holding_lock` está denegado en CI.
- Los tres tests `workflow_id_*` lo toman al entrar, con `blocking_lock()` (líneas 1050, 1070,
  1092): son tests síncronos, sin runtime, y el guard cubre las **dos** generaciones que su
  aserción compara.
- El guard va en el **test**, no dentro de `generate_workflow_id`: si se tomara dentro del helper,
  dos tests podrían intercalar sus dos llamadas y uno de ellos vería +4 donde exige +2.
- Cualquier test futuro que lea o mueva `RECALL_COUNTER` debe tomar el mismo guard. Está escrito
  en el comentario del estático y en el del helper.

Precedente en el repo para el mismo problema (protocolo de test serializado): `TEST_DB_MUTEX` en
`crates/tylluan-kernel/src/security/friction_log.rs:125`.

## 4. Prueba de que el arreglo NO debilita la detección (refutación)

Un guard mal puesto pondría el canario en verde *tapando* la regresión que vigila. Comprobado
forzando la regresión: se reactivó temporalmente el incremento de producción dentro del bloque
`#[cfg(test)]` de `hybrid_classify` (`RECALL_COUNTER.fetch_add(1, Ordering::Relaxed); // TEMP-REGRESSION-PROBE`),
se reconstruyó el binario y se corrió **el canario solo, con el guard ya en su sitio**:

```
run 1: exit=101 test result: FAILED. 0 passed; 1 failed; 0 ignored; 1022 filtered out
run 2: exit=101 test result: FAILED. 0 passed; 1 failed; 0 ignored; 1022 filtered out
run 3: exit=101 test result: FAILED. 0 passed; 1 failed; 0 ignored; 1022 filtered out
```

3 de 3, con el mensaje exacto de la aserción. El incremento de producción no toma el guard de
test, así que el contador se mueve dentro de la ventana y el canario falla igual. Mutación
revertida acto seguido (verificado: marcador ausente, 7 referencias al guard intactas) y binario
reconstruido antes de medir el estado final.

## 5. Verificación después del arreglo

| Comprobación | Medición | Resultado |
|---|---|---|
| Módulo, `--test-threads=4` | 60 runs | **0 fallos** |
| Módulo, `--test-threads=8` | 60 runs | **0 fallos** |
| Módulo, `--test-threads=64` | 60 runs | **0 fallos** |
| Módulo, artefacto **final** (tras mutar y revertir, reconstruido) | 60 runs a t=4 + 60 a t=64 | **0 fallos** |
| **Total** | **300 runs**, 0 errores de arnés | **0 fallos** (antes: 10/180) |
| Suite lib completa, canónica | `cargo test -p tylluan-kernel --lib` | `1020 passed; 0 failed; 3 ignored`, exit 0 |
| Suite lib completa, **paralelismo real de CI** | `--test-threads=4` (CI no pasa flags: usa los 2-4 vCPU del runner) | `1020 passed; 0 failed; 3 ignored`, exit 0 (148 s) |
| Suite lib completa, `--test-threads=64` | binario directo | `1020 passed; 0 failed; 3 ignored`, exit 0 (89 s) |
| Clippy (lo que exige CI) | `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` | **exit 0**, sin warnings |
| Refutación §4 | 3 runs con la regresión forzada | **3/3 fallan** el canario, como debe |

Contraste honesto: la suite completa **no** reproduce el fallo de forma fiable (1 run verde de 1020
tests en 192 s en la configuración pre-arreglo). La reproducción fiable vive en el filtro de módulo
con paralelismo bajo, que es donde el canario y los tests `workflow_id_*` comparten región de
planificación.

## 6. Retracción: qué afirmé mal y por qué

Mi entrega anterior sostenía que el test nombrado **no podía** producir el síntoma, que las cifras
«4 vs 6» **no existían** en el kernel y que la entrada del ROADMAP atribuía mal el fallo. **Las
tres afirmaciones son falsas** y el Tech Lead lo demostró con el código delante.

- Busqué aserciones de **recuento de filas** (`1`, `1`, `3`, `40`) y no encontré ningún `4`; nunca
  hice `grep` del identificador **`RECALL_COUNTER`**. Las cifras pertenecen a la comparación del
  contador, no a un recuento — el nombre y el mensaje del assert (`"must not run the Layer 4 judge
  body"`) apuntan a telemetría y me llevaron a leer el síntoma como filas.
- Afirmé además que «no existe otro escritor» del contador. No lo había comprobado. Sí existe:
  los tres tests `workflow_id_*`.
- Mi «baseline verde» (4 ejecuciones completas de la suite) era correcto y **insuficiente**: la
  tasa nativa del fallo en el filtro de módulo a `--test-threads=4` ronda el 8 % por run, así que
  cuatro runs verdes lo esquivan con ~70 % de probabilidad. Un verde no es evidencia de ausencia
  en un fallo de tasa baja; el propio informe debió decirlo y no lo dijo.
- Lo que **sí** se mantiene de aquella entrega: la pista del thread-local (el cuerpo del test no
  migra de hilo tras un `await`; el thread-local sobrevive) queda descartada con evidencia
  ejecutable, y el defecto de la DB scratch por PID es real — pero **no es la causa** de este
  fallo. Ver §8.

## 7. Dos fallos de método en mi propio arnés (misma clase de error que §6)

1. **Detector roto:** la primera versión de mi bucle clasificaba con `grep -E "test result: FAILED|[0-9]+ failed"`.
   `"0 failed"` también casa con `[0-9]+ failed`, así que contó como rojos **10 de 10** runs que
   habían pasado. Un «10/10 de reproducción» que era falso. Corregido: la señal es el código de
   salida y la sección `---- <test> stdout ----`.
2. **Runs que no corrieron:** en la primera tanda, 18 de 60 ejecuciones ni siquiera arrancaron
   (`Device or resource busy` al exec del binario recién copiado en Windows). Contadas como
   fallos de test, habrían dado otro «reproduce» falso. Ahora se cuentan aparte como
   `harness_errors` y el resultado final fue 0.

Los dos son la misma enfermedad que §6: dar por bueno un número sin comprobar qué lo produjo.

## 8. Endurecimiento secundario: scratch DB de test por PID (real, verificado, NO la causa)

Sigue en pie como endurecimiento independiente, ya entregado y verificado:

- `test_scratch_db_path()` (`crates/tylluan-kernel/src/security/friction_log.rs:93`) generaba el
  scratch DB de builds de test **solo con el PID** y nunca lo borraba. Windows reutiliza PIDs; el
  proceso siguiente heredaba filas del anterior. El mismo repositorio ya documentó ese síntoma al
  arreglar el *otro* camino (comentario de `set_unique_test_db`, línea 137: *"a later process
  inherited an earlier run's rows (exact-count asserts saw 43 vs 3, 4 vs 1)"*).
- Medido en `%TEMP%` el 2026-10-15: **4060** ficheros residuales del prefijo `tylluan_friction_`
  (el Tech Lead contó 4916 más tarde ese mismo día; crece con cada proceso que corre la suite).
- **Arreglo:** la ruta pasa a ser única por instancia de proceso (`pid + nonce`, como
  `set_unique_test_db`) y borra restos (`-wal`, `-shm`) antes de usarla.
- **Test determinista rojo→verde:** `scratch_db_never_inherits_a_previous_process_file`
  (`crates/tylluan-kernel/src/security/friction_log.rs:859`): planta el fichero que el esquema
  antiguo habría reutilizado, lanza un hilo ajeno que escribe y exige que la ruta resuelta **no**
  sea ese fichero. **3 ejecuciones rojas consecutivas** sobre el código previo (mismo mensaje las
  tres), **7 verdes** después.
- **Lo que NO es:** este defecto no puede producir el fallo intermitente del canario, porque el
  canario no lee filas del scratch: lee el contador global. Mi informe anterior lo presentaba como
  «la» causa raíz de la familia; esa atribución era errónea.

Nota de alcance: el arreglo elimina la **herencia de filas**, no la **acumulación de ficheros**
(cada proceso sigue dejando el suyo). Un barrido puntual del prefijo es seguro y opcional (§11.3).

## 9. Canario con fallo autoexplicativo (endurecimiento ya entregado)

En `crates/tylluan-kernel/src/security/coherence_gate.rs` el test resuelve la ruta de su DB **una vez**, comprueba que es la suya
(prefijo `tylluan_friction_test_`) y cuenta a través de esa ruta capturada. Antes la re-resolvía
sin comprobarla: si el redirect se perdía, el fallo salía como un `expected 1, saw N` opaco. Ahora
dice qué pasó:

```
the per-test DB redirect was lost before the read-back: <path> is the shared scratch DB,
so these counts are not this test's
```

## 10. Inventario: otros artefactos de test con el mismo patrón (verificados, NO tocados)

| Fichero:línea | Artefacto | Veredicto |
|---|---|---|
| `crates/tylluan-kernel/src/router/scheduler/confusion.rs:362` | `confusion_real_<pid>` | **OK**: hace `remove_file` antes de usar |
| `crates/tylluan-kernel/src/router/scheduler/confusion.rs:440` | `confusion_seam_rt_<pid>` | **OK**: mismo `remove_file` previo |
| `crates/tylluan-kernel/src/transport/http/api_v1/api_audit.rs` | directorios `audit_<variante>_<pid>` | Limpian con `remove_dir_all` al final; un proceso muerto hace fallar en alto al siguiente run — ruidoso, no silencioso |
| `crates/tylluan-kernel/src/security/llm_examples.rs:315` | `tylluan_examples_<variante>_<pid>.jsonl` | **Benigno**: `File::create` trunca |
| `crates/tylluan-kernel/src/memory/mmap_store.rs:327` | `tylluan_mmap_test_<pid>_<name>.fjv1` | **Candidato**: hecho por PID y persiste. No verifiqué la semántica de creación del store, así que no afirmo que sea un defecto |
| `crates/tylluan-kernel/src/router/embeddings.rs:1348` | fichero con PID | **Candidato sin verificar** |

## 11. Recomendaciones para el Tech Lead

1. **La entrada del ROADMAP estaba bien atribuida** (`docs/roadmap/ROADMAP_O3.md:80`): el test
   nombrado sí falló en CI, y las cifras 4/6 sí son suyas. Solo la descripción es imprecisa —
   «filas de telemetría» son **valores del contador global**. Propuesta: cerrarla y ajustar la
   redacción:

   ```markdown
   - [x] Test intermitente de Rust `hybrid_classify_is_inert_in_test_builds`: carrera sobre el
     contador global `RECALL_COUNTER` (el canario lo leía y exigía inmovilidad mientras tres
     tests `workflow_id_*` lo incrementaban). Causa raíz cerrada 2026-10-15 (Buffy): guard
     compartido sin tocar aserciones; antes 10 fallos en 180 runs, después 0 en 300 + refutación
     de que el guard tape la regresión. Diff sin commitear.
   ```
2. **Asignar los hermanos de §10** (`mmap_store.rs:327`, `embeddings.rs:1348`) solo si se confirma
   el defecto; el patrón correcto ya existe en `crates/tylluan-kernel/src/router/scheduler/confusion.rs`.
3. **Limpieza opcional de `%TEMP%`**: hay ~4000 ficheros `tylluan_friction_` acumulados y el
   esquema sigue dejando uno por instancia de proceso. No es necesario para la corrección.
4. **La puerta `scripts/check_docs_reality.sh` no comprueba lo que dice comprobar** (§12.1). No la
   he tocado: es tarea aparte por decisión del Tech Lead.

## 12. Hallazgos colaterales (no forman parte del arreglo)

### 12.1 Check 1 de `scripts/check_docs_reality.sh` es vacuo

El script documenta «Every backtick-cited path in STATUS.md, README.md, AGENTS.md, CLAUDE.md,
docs/**/*.md» y recorre los tokens con `grep -nEo '\`[^\`]+\`'`. Ejecutado tal cual, ese patrón
devuelve **0 coincidencias** incluso en documentos llenos de citas en backticks (probado con este
informe: 0 con el patrón del script, 74 con el patrón sin escapar; con
`docs/concepts/SECURITY.md`: 29). **El gate de rutas citadas nunca ha verificado nada** — la misma
clase de confianza falsa que el incidente del 2026-09-27.

- **Por qué no lo he arreglado:** reparar el patrón haría aflorar de golpe las citas inválidas de
  los 85 documentos escaneados — por ejemplo la DB de auditoría que cita
  `docs/concepts/SECURITY.md`, que no se versiona y no existe en un clon limpio — y dejaría CI en
  rojo sin un barrido previo y sin decidir qué excepciones son legítimas. Es una decisión de
  alcance, no un arreglo de pasada en un diff de flake.
- **Dos fallos, no uno:** con el patrón tal como está escrito devuelve 0 coincidencias (no
  comprueba nada); y aunque se escapara el patrón, el token que llega al bucle conserva los
  backticks que lo envuelven y la cadena de *strip* nunca los quita, así que marcaría como
  inexistente **cada** ruta citada. Un arreglo real necesita las dos cosas.

### 12.2 `docs/internal/AUDIT_buffy_2026-10-14.md` rompe la puerta de puertos si se commitea

`scripts/check_docs_reality.sh` sale con **exit 1** en este worktree por tres ❌ del Check 2 en ese
documento (líneas 78, 87 y 157). Cita los puertos secundarios de los watchers (`:3030` y `:3033`)
en vez del `:47004` real, que es el del kernel. El fichero sigue **sin trackear**, así que hoy no
afecta a CI; si se commitea tal cual, la puerta de documentación falla.
No lo he modificado: es un entregable anterior y su corrección debería decidirse con su contexto
delante.

## 13. Límites de este informe

- **No vi el fallo de CI con mis ojos**: parto del parte del Tech Lead (línea 653, valores 4 y 6) y
  lo cruzo contra el código de `HEAD` (§1) y contra mi reproducción local del mismo mecanismo
  (§2). Las tres piezas concuerdan, pero el log original no está transcrito en el repo.
- La tasa nativa del fallo es baja y depende de la máquina: 5/60 a `--test-threads=4` aquí, 0 en un
  run completo de la suite. No he medido la tasa de CI.
- No he verificado en vivo la herencia de filas por colisión de PID (§8): el fichero tiene la misma
  ruta y se sobrescribe, así que el rastro desaparece a posteriori. Lo verificado es el mecanismo y
  el rojo→verde del test que lo hace imposible.
- No he tocado producción, ni el ROADMAP, ni el script de la puerta de documentación.
- **Nota de proceso:** una primera aplicación de este arreglo fue a parar por error al checkout
  compartido `E:\tylluan` (el tool resolvió una ruta relativa contra la raíz del proyecto). Se
  detectó y se revirtió en el mismo paso —`git status` limpio, 1044 líneas = `HEAD`— y el parche se
  reaplicó dentro de `E:\tylluan-wt-buffy`. No se commiteó nada en el checkout compartido.
