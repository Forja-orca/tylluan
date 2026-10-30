# T-DSH-147 — Suite repetida 10× para cazar tests intermitentes

- Agente: dsh
- Fecha: 2026-10-30
- Base: `origin/main = a85526b` (CI verde), HEAD confirmado `a85526b`.
- Encargo (T2187): ejecutar 10 veces seguidas `cargo test -p tylluan-kernel --lib` (1434 tests) y 3 veces los tests de integración, guardar log de cada pasada, tabla test → veces fallado/10 → mensaje → causa probable; reproducir aislado con `--test-threads=1` y en paralelo si algo falla. Solo arreglar fallos de una línea y de test; lo demás, listarlo. Sin tocar kernel de producción, `Cargo.toml` ni dashboard.

## Entorno y comandos

```
$env:CARGO_PROFILE_DEV_DEBUG="line-tables-only"
$env:CARGO_INCREMENTAL="0"
$env:CARGO_TARGET_DIR="D:\cargo-target\shared-dev"
cd E:\tylluan-wt-dsh

cargo test -p tylluan-kernel --lib --no-run -j 8
cargo test -p tylluan-kernel --tests --no-run -j 8
# 10 pasadas:
cargo test -p tylluan-kernel --lib            # logs: t147\logs\lib-pass1..10.log
# 3 pasadas de integración (todos los binarios de tests/):
cargo test -p tylluan-kernel --tests          # logs: t147\logs\int-pass1..3.log
```

## Resultados

### Lib (10 pasadas) — todas verdes

| pasada | resultado |
|---|---|
| 1-10 | **`ok. 1434 passed; 0 failed; 4 ignored`** en las 10 (tiempo 51,0-59,7 s por pasada) |

→ **0 fallos en 10/10 pasadas de la suite lib**: no se encontró ningún test lib intermitente en esta máquina.

### Integración (3 pasadas) — 1 test falla en las 3

| pasada | resultado |
|---|---|
| 1 | exit 101: 1 target FAILED (`asi06_judge_hang_writes`), resto ok |
| 2 | exit 101: mismo target, mismo test |
| 3 | exit 101: mismo target, mismo test |

| test | fallado | mensaje | causa probable |
|---|---|---|---|
| `asi06_judge_hang_writes::hung_judge_does_not_block_memory_writes` | **3/3 pasadas** (y 3/3 aislado normal, 1/1 aislado `--test-threads=1`) | `panicked at tests\asi06_judge_hang_writes.rs:105:5: con el backend colgado el juez debe estar in_flight (la escritura volvió igualmente: fire-and-forget confirmado)` | **test obsoleto tras T-DSH-115/116**: write_gate ya no usa `TYLLUAN_KERNEL_URL` ni el juez HTTP legacy; ahora llama a `grammar_judge::judge_grammar()` (servidor efímero / inference point). El test levanta un servidor falso colgado vía env y espera que `judge_telemetry().in_flight()` suba, pero el camino actual falla rápido (sin `model_path` configurado → `Err`) y `in_flight` nunca sube; el contador de spawns sí sube (por eso pasa la aserción de línea 98). |

Evidencia de código:
- `security/write_gate.rs:203`: `crate::security::grammar_judge::judge_grammar(&content).await` (T-DSH-115: servidor efímero aislado).
- `security/coherence_gate.rs:378`: la ruta HTTP legacy (`call_reasoning_backend_with_grammar`, que lee `TYLLUAN_KERNEL_URL`) está marcada `#[allow(dead_code)] // T-DSH-115: ruta HTTP legacy del juez de gramatica (write_gate usa servidor efimero)`.
- `security/grammar_judge.rs::judge_returns_error_when_not_configured`: sin `model_path`, `judge_grammar` devuelve `Err` antes de arrancar nada (fail-open).

### Reproducción aislada

| comando | resultado |
|---|---|
| `cargo test -p tylluan-kernel --test asi06_judge_hang_writes` (×3) | **3/3 FAILED** (1,0 s) |
| `cargo test -p tylluan-kernel --test asi06_judge_hang_writes -- --test-threads=1` | **1/1 FAILED** |

→ **No es intermitente: falla de forma determinista**. No se debe a estado global compartido, orden, puerto ni reloj; es un test que ya no describe el camino actual del juez.

### ¿Por qué CI está verde?

`.github/workflows/ci.yml` L28 ejecuta **`cargo test -p tylluan-kernel --lib`** (y binarios de auditoría concretos), no `cargo test --tests` completo: `asi06_judge_hang_writes` no está en CI, por eso el rojo no aparece allí.

## Arreglo

**No se aplicó ningún cambio.** El fallo **no es de una línea**: el test entero se apoya en un mecanismo retirado (juez HTTP por `TYLLUAN_KERNEL_URL`); arreglarlo exige reescribir su premisa (configurar un `[security.write_gate_judge] model_path` real o un inference point falso, o limitar la aserción al contador de spawns), lo que queda fuera del permiso «solo si es de una línea y de test». Queda listado para decisión del TL.

## Impact

- **Suite lib: 10/10 verde, 1434 tests, 0 fallos** → no hay test lib intermitente en esta máquina en 10 pasadas.
- **Integración: 1 test determinista en rojo** (`asi06_judge_hang_writes::hung_judge_does_not_block_memory_writes`, 3/3 pasadas), por premisa obsoleta tras T-DSH-115/116, no por flakiness. Cualquier `cargo test -p tylluan-kernel --tests` completo saldrá rojo hasta actualizarlo; CI no lo detecta porque solo corre `--lib`.
- Sin diff de código: no se tocó `Cargo.toml`, dashboard ni kernel de producción.

## No verificado

- Una sola máquina y una sola ventana temporal; 10 pasadas lib y 3 de integración (no cientos). No se ejecutó la suite completa `cargo test` de todos los crates (`tylluan-cli`, `tylluan-link`, `tylluan-fsrs`) ni con `--all-targets`.
- No se probó con `[security.write_gate_judge]` configurado (model_path real) ni con centinela activo: no se descarta que el test pase en esa configuración, pero su premisa (env `TYLLUAN_KERNEL_URL`) seguiría siendo la equivocada.
- La causa se infiere leyendo el código (write_gate → grammar_judge, ruta legacy `dead_code`), no instrumentando `in_flight`/spawns en vivo.
- No se verificó el resto de jobs de CI en ejecución (solo se leyó `ci.yml`).
