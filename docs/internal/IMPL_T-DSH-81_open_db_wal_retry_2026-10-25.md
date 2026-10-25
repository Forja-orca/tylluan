# T-DSH-81 — open_db: consulta de journal_mode y reintento de WAL ante BUSY

- **Agente:** dsh (informe original perdido al resetear su worktree; este resumen lo reconstruye el TL con cifras que verificó él mismo).
- **Base:** `origin/main` `aa67836`.
- **Cambio:** `crates/tylluan-kernel/src/config.rs` — `PRAGMA journal_mode=WAL` sale del batch inicial de `open_db`; `enable_wal_best_effort()` consulta el modo y solo cambia a WAL si no lo es; ante `SQLITE_BUSY/LOCKED` reintenta 5 veces con 20 ms; si persiste, `tracing::warn!` y continúa. Test nuevo `concurrent_open_db_of_new_file_never_fails` (50 iteraciones × 16 hilos).
- **Causa que corrige (T-DSH-80):** dos aperturas concurrentes de un fichero nuevo colisionaban en el PRAGMA y se perdía la escritura de auditoría (mb7_rows fallaba 8/30).

## Verificación del TL
- `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` → exit 0.
- `cargo test -p tylluan-kernel --lib` → 1276 passed (base 1275 + 1).
- `--test audit_behavior_mb7_rows` en bucle 20× → 0/20 fallos.
- `--test mesh_audit` 25× sueltos → 0/25; una vez falló `test_t1277_c2_gate_denies_unlisted_tool_and_audits` en la primera ejecución tras recompilar.

## Impact
Toca `config::open_db`, usado por todas las aperturas SQLite. La firma no cambia y un fallo de WAL ya no devuelve error, solo avisa.

## No verificado
- Coste máximo del reintento (≈4×20 ms) bajo carga real sostenida.
- La causa del único fallo de `mesh_audit` tras recompilar (misma clase de pérdida de fila de auditoría, no reproducida).
