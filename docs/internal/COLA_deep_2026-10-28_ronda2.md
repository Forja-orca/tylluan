# COLA Deep, ronda 2 (2026-10-28) — la lanza José con un mensaje corto

Quién: Deep (seguridad). Dónde: tu worktree `E:\tylluan-wt-deep`. Antes de nada: `git fetch origin` y `git merge --ff-only origin/main`; lee, por este orden, tu fichero de entrada, `STATUS.md` y `REVIEW.md` (el tercero).
Reglas: diff sin commitear más un informe `docs/internal/IMPL_<tarea>_deep_2026-10.md` con `## Impact` y `## No verificado`. Solo el TL commitea y empuja. No toques `E:\tylluan` ni el kernel vivo.

## Orden

### D-66 — Aviso al arrancar con `expose_guild_tools = true` (auditoría AUD-07)
- Contexto: con el flag activo, `all_tools()` (`crates/tylluan-kernel/src/transport/server/tools.rs`) expone más de las seis tools del CONTRACT-01. Producción lo tiene en `false` (verificado: `tools/list` en vivo devuelve 6). Se documenta como excepción opt-in; falta que el kernel lo diga al arrancar.
- Hacer: un `warn!` al arrancar cuando el flag es `true`, con el número de tools expuestas; no cambiar el comportamiento. Test que fije: (1) por defecto `all_tools()` devuelve exactamente las seis, en el orden de `sovereign_tool_names()`; (2) con el flag, las seis siguen incluidas.
- Aceptación: `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` → 0; `cargo test -p tylluan-kernel --lib` → 1401 + tus tests, 0 fallos.
- Fuera de alcance: quitar el flag o cambiar su valor por defecto.

### D-67 — Revisión independiente del ADR-025 (solo lectura)
- Contexto: el TL redactó `docs/reference/adr/ADR025_runtime_y_subruntimes.md`; quien escribe una spec no debe ser su único revisor. Fase 0 en `docs/reference/research/2026-10-28_FASE0_inventario_spawns_*.md`.
- Hacer: tabla `hallazgo -> fichero y línea -> reproducción -> arreglo` con los riesgos de seguridad del diseño: orden de apagado frente a durabilidad del log de auditoría y de las escrituras en vuelo, el endpoint de estado de tareas como fuga de información, el `TaskRegistry` como superficie nueva, cancelación a mitad de escritura SQLite.
- Aceptación: informe `docs/internal/REVIEW_ADR025_deep_2026-10.md` con cada afirmación respaldada por fichero y `línea N` (no escribas `:N` junto a un número, el gate de puertos lo lee como puerto). Un apartado final "Recomendación: aprobar / aprobar con cambios / rechazar".

## Si te atascas
Escribe en el informe qué paso falla, el comando exacto y su salida, y para. No improvises fuera de alcance.

## Cómo informar
Mensaje `[COLA deep]` en Coloquio con: comandos ejecutados, cifras, ficheros tocados, y qué no verificaste.
