# COLA Buffy — 2026-10-28 (la relaya Jose) — inventario tanda 6 y reproduccion de Project Skills

Contexto: ya estas en a564880 con la re-medicion hecha (T1931), gracias. Dos frentes. Compila SOLO con tu `CARGO_TARGET_DIR=E:\cargo-target\buffy` y `-j 4`. En informes nunca `:NNNN` (usa `<puerto>`; en JSON, espacio tras los dos puntos). No borres ni muevas ningun `target/` ni `test-run-*` (el TL ya limpio los viejos).

## M-BUFFY-14 — Project Skills: reproducir, NO arreglar (hallazgo E-08 de Deep)
Deep reporto que el panel Project Skills mezcla ficheros y SilvaDB: guardar, listar, abrir y borrar no se corresponden. Tu trabajo es dejarlo demostrado con tests, sin tocar codigo de produccion:
1. Localiza con `grep` todos los puntos de guardar/listar/abrir/borrar skills (kernel y dashboard) y haz una tabla `operacion -> almacen que usa (fichero | SilvaDB) -> fichero:linea`.
2. Escribe tests de integracion en un fichero nuevo (p. ej. `crates/tylluan-kernel/tests/project_skills_store_mismatch.rs`) que fallen por la razon correcta: lo guardado no aparece en la lista, lo listado no se puede abrir, borrar un skill de fichero falla. Marcalos `#[ignore = "M-BUFFY-14: ..."]` como hiciste con ADR-024 para no romper la suite normal.
3. Aceptacion: `cargo test -p tylluan-kernel --test project_skills_store_mismatch -- --ignored` falla en cada caso con el motivo esperado; la suite lib normal no cambia de cifra (1331 passed / 4 ignored en este momento).
4. Informe `docs/internal/IMPL_M-BUFFY-14_buffy_2026-10.md` con la tabla, el fix propuesto en 5 lineas y `## Impact` + "No verificado".

## Inventario real — tanda 6 en adelante
Sigue `docs/internal/INVENTARIO_REAL/PROGRESO.md`: tanda 6 (Kernel: mapa de modulos y cableado), lectura pura. Una tanda por vez, actualizando PROGRESO.md e inventario.json al terminar cada una. No inventes filas de areas que exigen ejecucion.

## Entrega
Diff SIN commitear en tu worktree + resumen `[COLA buffy]` en Coloquio. Solo el TL comitea y empuja. Si te atascas: para y publica el error literal.
