# T-DSH-146 — Ensayo del reinicio de producción (test-run propio, sin tocar prod)

- Agente: dsh
- Fecha: 2026-10-30
- Base: `origin/main = bd327d6` (push211), HEAD confirmado `bd327d6`.
- Encargo (T2176): en test-run con puerto y DATA_DIR propios y una **copia de `silva.db`**, arrancar el **binario release de main** con el `tylluan.toml` de prod más `[runtime] shutdown_force_exit=true` y `[silva] coloquio_retention_turns=3000`; comprobar `/health` commit, que **NO borra nada** (conteo de nodos antes/después), apagado por POST admin con deadline y reinicio con la misma DB. Informe con comandos, cifras y `## No verificado`.

## Montaje

- Binario: release de `bd327d6` construido en `D:\cargo-target\shared-dev` (`cargo build -p tylluan-kernel --release -j 8`, `CARGO_INCREMENTAL=0`), copiado a `t146`.
- Config: copia del `tylluan.toml` de producción (`E:\tylluan\tylluan.toml`, leído en solo lectura) con overrides:
  - `[nexus] port = 47101` (propio), `dev_mode = true` (como prod);
  - `[silva] coloquio_retention_turns = 3000`;
  - `[runtime] shutdown_timeout_ms = 5000`, `shutdown_force_exit = true`;
  - **desviación documentada**: `[guilds] always_on = []`, `warm_pool = []` y sin `[[external_mcp]]` para no spawnear guilds ni procesos externos en el test-run (el resto del TOML de prod se conserva).
- DATA_DIR propio `t146\data` con **copia ro de `silva.db`** (199,6 MB, backup SQLite desde `mode=ro`): 11.033 nodos, 1.654 `coloquio_memory`, 6.447 embeddings, 6.439 sparse.
- Comandos:
  ```
  tylluan-nexus.exe --config tylluan.toml --port <puerto propio>     # cwd = t146
  GET  /health
  POST /api/v1/admin/shutdown
  # reinicio: mismo comando sobre el mismo DATA_DIR
  ```

## Resultados

| métrica | valor |
|---|---|
| `/health` commit (arranque 1 y 2) | **`bd327d6`** (los dos) |
| Nodos totales antes → después | **11.033 → 11.033 (delta 0)** |
| `coloquio_memory` antes → después | **1.654 → 1.654 (0 borrados)** |
| Embeddings | 6.447 → 6.448 (tras 1er arranque) → **6.449** (+2 por jobs de arranque, 0 borrados) |
| Sparse | 6.439 → 6.439 (0 cambios) |
| Arranque 1 | **114,52 s** (primer arranque sobre la copia: carga de modelos/jobs de arranque) |
| Apagado 1 (POST admin) | **5,06 s** (consume el deadline de 5.000 ms; `force_exit` tras la secuencia ordenada) |
| Arranque 2 (misma DB) | **19,49 s** |
| Apagado 2 | **0,04 s** |
| Errores/avisos de config en el log | ninguno (la clave `coloquio_retention_turns` se parsea sin warning) |

- **No se borró nada**: con `coloquio_retention_turns=3000` y el watermark actual del canal (≈2.176 turnos), la retención no tenía nada que borrar; los conteos de nodos, coloquio, embeddings y sparse se mantienen (los +2 de embeddings son re-embeds de arranque, no pérdidas).
- **Reinicio con la misma DB: OK** (segundo arranque 19,5 s, `/health` con el mismo commit y sin errores).
- El apagado con `force_exit=true` respeta el deadline: 5,06 s en el primer apagado (tareas registradas pendientes) y 0,04 s en el segundo (sin pendientes).

## Impact

- El **binario release de main (`bd327d6`) arranca y se reinicia correctamente** con el TOML de producción + los dos overrides del ensayo, sobre una copia de la BD real, en puerto/DATA_DIR propios: `/health` reporta el commit correcto y la BD no pierde nodos.
- `coloquio_retention_turns=3000` es **inerte con el volumen actual** (≈2.176 turnos): no borra nada; el ensayo confirma que activar la clave con 3000 no destruye datos hoy.
- El apagado ordenado con deadline (`shutdown_force_exit=true`) funciona en el binario release: 5,06 s / 0,04 s, sin dejar proceso vivo.

## No verificado

- **No se ejercitó el borrado por retención**: haría falta un canal con >3.000 turnos (o bajar N en el test-run) para comprobar que borra solo lo viejo y con cascada; aquí solo se verificó que **no borra** con el volumen actual.
- El `tylluan.toml` del test-run **no lleva guilds ni MCPs externos** (always_on/warm_pool vacíos y sin `[[external_mcp]]`): el arranque real de producción con 13 guilds + 5 MCPs externos no se ensayó (spawnear procesos ajenos en un test-run es lo que se quiso evitar).
- El primer arranque (114,5 s) incluye jobs de arranque/re-embed sobre la copia fría; no es comparable con un reinicio de producción ya caliente.
- No se contaron **edges** ni otras tablas antes/después (solo nodos, coloquio, embeddings y sparse); la afirmación «no borra nada» se limita a esas métricas.
- Solo dos ciclos de arranque/apagado; no se probó con la BD en uso concurrente ni con WAL de una sesión viva de producción.
- `dev_mode = true` (como el TOML de prod actual); no se ensayó el escenario `dev_mode=false` con token que José espera para producción.
- No se tocó `E:\tylluan` ni el kernel de producción (puerto de producción); kernel de prueba en puerto propio, parado al terminar.
