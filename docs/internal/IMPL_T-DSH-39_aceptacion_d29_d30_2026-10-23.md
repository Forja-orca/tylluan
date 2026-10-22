# T-DSH-39 — Aceptación ejecutable de D-29 y D-30 (cola de Deep)

- **Agente:** dsh
- **Fecha:** 2026-10-22/23
- **Base:** `E:\tylluan-wt-dsh` en `--detach origin/main` (`6316db3`, push 111: T-DSH-38 integrada); limpio antes de empezar.
- **Encargo (turno 1567):** un test nuevo, sin tocar `src`; acceptance ejecutable de D-29 y D-30 igual que se hizo con D-32. Leer `docs/internal/COLA_deep_2026-10-22.md` (D-29/D-30), `src/transport/http/oauth_consent.rs` y el almacén de planes (`PLAN_STORE`, test ignorado en `tests/continuity_probes.rs`). Crear `tests/cola_d29_d30_acceptance.rs` con (a) `#[ignore = "D-29"]` corrompiendo `consents.json` y afirmando que NO se sobrescribe; (b) `#[ignore = "D-30"]` creando un plan, simulando reinicio y afirmando que sigue pendiente; (c) un test VIVO que fije el comportamiento actual de ambos. Solo directorios temporales, API pública o `pub(crate)` accesible; si algo no es accesible, decirlo. Criterio: test pasa; `--ignored` fallan 2 por la razón correcta; clippy 0; sin `cfg`; informe con `## Impact` y no verificado.

## Qué contiene el fichero

`crates/tylluan-kernel/tests/cola_d29_d30_acceptance.rs` (nuevo, `src/` intacto):

| test | tipo | qué afirma |
|---|---|---|
| `d29_corrupt_consents_must_be_preserved` | **`#[ignore = "D-29"]`** | corrompe `consents.json`, aprueba un cliente nuevo: el ilegible queda conservado como `consents.json.corrupt-*` con sus bytes originales y el nuevo `consents.json` solo tiene el cliente nuevo |
| `d29_current_corrupt_consents_is_overwritten` | **vivo** | comportamiento actual: **no** se crea copia; el ilegible se sobrescribe (contenido perdido) |
| `d30_pending_plan_must_survive_restart` | **`#[ignore = "D-30"]`** | crea un plan con `DATA_DIR` temporal; un **proceso hijo** arranca de cero, llama a `init_plan_store()` y debe encontrarlo |
| `d30_current_pending_plan_is_lost_on_restart` | **vivo** | comportamiento actual: el hijo no encuentra el plan (sale con 2); `PLAN_STORE` es solo memoria de proceso |

Detalles de implementación:

- **D-29** usa la API pública: `ConsentStore::with_file(dir)` (relee el fichero, falla → estado vacío fail-closed) y `record_consent(...)` (dispara `persist` privado). Nada de `src` tocado.
- **D-30** usa `security::grants::{init_plan_store, store_plan, get_plan}` (todos públicos). El reinicio **in-process no es medible** porque `PLAN_STORE` es un `OnceLock` (`grants.rs` línea 46): se simula lanzando **un proceso hijo del propio binario de test** (`current_exe()` + `--exact <test> [--ignored]`) con el mismo `DATA_DIR`; el hijo escribe un marcador (`child_ran.marker`) para probar que de verdad ejecutó el helper y sale con 0 = encontrado, 2 = no encontrado.
- La API necesaria **sí es accesible** desde tests de integración; no hubo que forzar cambios en `src`.

## Salidas

Corrida normal (vivos; los deseados quedan fuera):

```text
test d29_corrupt_consents_must_be_preserved ... ignored, D-29
test d30_pending_plan_must_survive_restart ... ignored, D-30
test d29_current_corrupt_consents_is_overwritten ... ok
test d30_current_pending_plan_is_lost_on_restart ... ok

test result: ok. 2 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.06s
```

Corrida `-- --ignored` (los deseados fallan **por la razón correcta**):

```text
running 2 tests
test d29_corrupt_consents_must_be_preserved ... FAILED
test d30_pending_plan_must_survive_restart ... FAILED

thread 'd29_corrupt_consents_must_be_preserved' panicked:
D-29 pendiente: no se conservó copia del consents.json ilegible
(consents.json.corrupt-<ts>); el contenido original se perdió

thread 'd30_pending_plan_must_survive_restart' panicked:
D-30 pendiente: el plan 'd30-plan-18000' no sobrevive al reinicio
(el hijo salió con Some(2); 2 = no encontrado). PLAN_STORE sigue siendo solo memoria de proceso

test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.06s
```

Cuando Deep cierre D-29/D-30: quitar el `#[ignore]` de cada test deseado **sin tocar el test**, e **invertir** los dos tests vivos (hoy fijan el fallo) según pide la cola.

## Verificación

| criterio | resultado |
|---|---|
| `cargo test -p tylluan-kernel --test cola_d29_d30_acceptance` | **2 passed / 0 failed / 2 ignored** (0,06 s) |
| `cargo test --test cola_d29_d30_acceptance -- --ignored` | **2 failed**, ambos por la razón correcta (salida arriba) |
| `cargo clippy -p tylluan-kernel --all-targets -j 3 -- -D warnings` | **exit 0** |
| `git status` | **solo** el test nuevo; `src/` intacto |
| `check_test_count.sh` | exit 0 (1272; test de integración) |
| `check_doc_links.py` / `check_docs_reality.sh` | links 0; reality solo el hallazgo preexistente de `ROADMAP_O3.md` (artefacto WSL) |

Sin `cfg(unix)`/`cfg(windows)` en ninguna función.

## Supuesto de compatibilidad con el arreglo de D-30

El test deseado llama a `grants::init_plan_store()` y `grants::get_plan(id)` **con la firma actual** (públicas, `grants.rs` líneas 57-80) y fija `DATA_DIR` al temporal antes de crear el plan, que es la convención que ya usa el harness de continuidad (`tests/continuity_probes.rs`). Si D-30 cambia la firma de `init_plan_store` (p. ej. para recibir la ruta de la base de datos) o usa otra variable de entorno, el test **no compilará** y habrá que adaptarlo; lo dejo señalado aquí porque la cola de Deep no fija la firma. En ese caso, el criterio observable sigue siendo el mismo: proceso nuevo + carga desde disco → plan presente.

## Impact

- **Solo test, fichero nuevo**: cero cambios en `src/`, guilds, configuración o corpus; no se arranca el kernel ni se tocan puertos; todo ocurre en directorios temporales (y un proceso hijo del binario de test, que se mata solo al salir). No se tocó `E:\tylluan`.
- **Para D-29/D-30**: deja la aceptación en un comando: 2 vivos que fijan el fallo actual (Deep tendrá que invertirlos) y 2 ignorados que pasan al arreglar, con mensajes que dicen exactamente qué falta. El «reinicio» de D-30 queda medido con un proceso real (no in-process, que era el motivo por el que el test antiguo era ignorado).
- Coste: ~0,07 s; sin red, sin BD compartida (SQLite en temporales).

## No verificado

- **Con D-29/D-30 implementados**: no existen todavía (la cola está pendiente), así que no se comprobó que los 2 deseados pasen al des-ignorarlos; solo que hoy fallan por la razón correcta.
- **Firma futura de `init_plan_store`**: supuesto de compatibilidad arriba; si cambia, el test no compila (lo detectará Deep/TL al integrar).
- **Persistencia real de planes**: el test crea el plan en el proceso padre con `DATA_DIR` fijado, pero hoy `store_plan` no toca disco; el hijo sale 2 por el `OnceLock` vacío, que es también lo que pasaría en un reinicio real. No verifiqué el estado de la BD (no existe tabla aún).
- **D-29**: el test usa `ConsentStore` público, no el endpoint OAuth HTTP completo; no pasa por `/oauth/authorize`, solo por la ruta de carga+persistencia que describe D-29. El `warn!` con la ruta (parte del criterio de D-29) no se aserta (los logs no son observables desde un test de integración); sí la copia `.corrupt-*`.
- **Proceso hijo**: depende de `current_exe()` + filtro `--exact` de libtest; el marcador `child_ran.marker` protege contra un filtro que no case (si no ejecutara, el test fallaría diciéndolo). No probado en CI con otro harness.
- **`DATA_DIR`**: la convención se toma de `tests/continuity_probes.rs`; no verifiqué todas las rutas del kernel que leen esa variable.
