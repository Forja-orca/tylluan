# COLA Buffy (2026-10-29) — la lanza José con un mensaje corto

Quién: Buffy (seguridad y tests). Dónde: tu worktree `E:\tylluan-wt-buffy`. Antes de nada: `git fetch origin`, rebasa a `origin/main`, y confirma en tu primer turno `git rev-parse --short HEAD`. Lee, por este orden, tu fichero de entrada, `STATUS.md` y `REVIEW.md`.
Reglas: solo informe y, si el hallazgo lo pide, un test, como diff sin commitear más `docs/internal/IMPL_<tarea>_buffy_2026-10.md` con `## Impact` y `## No verificado`. Solo el TL commitea y empuja. Kernel de prueba obligatorio: binario propio, puerto propio (47093), `DATA_DIR` propio vacío, nunca el cwd de producción. Tu regresión de B-1 ya está en `main` y Deep cerró el hueco: tus tests pasan sin `#[ignore]` (3 de 3).

## Orden

### B-5 — Repetir B-3 contra el código nuevo
- Hacer: con el binario construido de `origin/main`, `dev_mode = false` y `default_role = "guest"`, recorre otra vez las rutas del panel (como en la ronda 2) con y sin token, y prueba un JWT OAuth sin mapeo contra un guild restringido.
- Aceptación: tabla `ruta -> estado con token -> estado sin token`; el JWT sin mapeo recibe 403; cero rutas rechazan un token válido. Si alguna falla, el comando exacto y la salida.

### B-6 — Auditar los `#[ignore]` (M-BUFFY-12; 35 reales a 7ed537e, 33 tras D-69)
- Hacer: `grep -rn --include=*.rs -E '^[[:space:]]*#[[]ignore' crates/` y, para cada uno: motivo, si sigue siendo válido, y acción (activar, borrar o mantener con justificación).
- Aceptación: tabla completa, una fila por atributo; los que se puedan activar sin tocar código, activados en tu diff y pasando.

### B-7 — Suite repetida 10 veces
- Hacer: `cargo test -p tylluan-kernel --lib` 10 veces seguidas con tu `CARGO_TARGET_DIR`; anota cualquier test que falle alguna vez. Haz lo mismo con `pnpm test` del panel (una vez falló bajo carga).
- Aceptación: lista de tests intermitentes con la tasa de fallo; 0 si no hay ninguno.

### B-8 — Ataque a `tylluan update` con D-70 ya dentro
- Hacer: las pruebas adversariales de B-4 (ruta `..`, enlace simbólico, directorio con nombre de binario, checksum alterado, versión menor, bomba de descompresión) contra el código nuevo.
- Aceptación: tabla `caso -> esperado -> obtenido`; todo rechazado.

### B-9 — Test de `close` en contratos
- Hacer: añade a `tests/bwc_agent_binding.rs` el caso de cerrar un contrato como otro agente (403 esperado; Deep también cubrió `close`).
- Aceptación: el test pasa; `cargo clippy -p tylluan-kernel --test bwc_agent_binding -- -D warnings` → 0.

## Si te atascas
Escribe en el informe qué paso falla, el comando exacto y su salida, y para.

## Cómo informar
Mensaje `[COLA buffy]` en Coloquio con comandos, cifras, ficheros tocados y qué no verificaste.
