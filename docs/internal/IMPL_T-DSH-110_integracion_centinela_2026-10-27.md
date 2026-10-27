# T-DSH-110 — Integración REAL del centinela opt-in en un kernel test-run

- **Agente:** dsh
- **Fecha:** 2026-10-27
- **Base:** `origin/main = 83df99e` (push174, con T-DSH-109 `bdb67e7`). Diff sin commitear **solo** `security/sentinel.rs` (+99/−1: reaper de inactividad + Job Object kill-on-close, ver "Hallazgos").
- **Encargo (T1914/T1915):** prueba de integración REAL del centinela en un kernel `test-run-*` (puerto y DATA_DIR propios, GGUF Qwen3Guard Q4 de `guard-eval`, 4 hilos): 20 notas benignas + 10 maliciosas, medir latencia/RSS dentro del kernel, que `Unsafe` quede en cuarentena y `Controversial` marcado, que el servidor se descargue por inactividad y que no quede proceso huérfano (PID exacto).

## Montaje

- `E:\tylluan-targets\dsh-t7-data\t110`: junctions a `guilds`/`.venv`, **puerto propio 47023**, DATA_DIR `./data`, BD limpia.
- `[security.sentinel]`: `enabled = true`, `model_path = E:\tylluan-patches\guard-eval\models\Qwen3Guard-Gen-0.6B.Q4_K_M.gguf` (ruta explícita, sin descarga), `llama_server_path = …\llama-server.exe`, `port = 0` (efímero), `threads = 4`, `ctx_size = 2048`, `timeout_ms = 60000`, `idle_unload_secs = 10`.
- 30 notas: **20 benignas** (Q4 las clasificó Safe en T-DSH-108) + **5 maliciosas que Q4 clasificó Unsafe** + **5 que clasificó Controversial**, escritas por `POST /api/v1/silva/node`.
- Kernel arrancado con BELOW_NORMAL y afinidad 8 cores; se para **solo su PID exacto** (nunca 47004). Verificación de estado por lectura SQLite de la BD del test (`quarantined`, `quarantine_reason`) y conteo de hijos `llama-server.exe` por `ParentProcessId`.

## Resultado (run final limpio, 4.ª pasada)

| Métrica | Valor |
|---|---|
| Boot del kernel test-run | 24,1 s |
| Benignas que pasan (sin cuarentena ni marca) | **20/20** |
| `Unsafe` en cuarentena (`quarantined=1`, razón `T-DSH-109 centinela: Unsafe`) | **5/5** |
| `Controversial` marcadas para revisión (`quarantined=0`, razón `T-DSH-109 centinela: Controversial (pendiente de revisión)`) | **5/5** |
| Latencia hasta veredicto (10 maliciosas) | p50 **1.849,6 ms**; máx **40.570,8 ms** (1.ª nota = arranque en frío del modelo); n=10 |
| RSS kernel (baseline → pico con centinela) | 484,9 MB → **513,5 MB** (+28,6 MB) |
| RSS pico del hijo `llama-server` del centinela | **1.575,1 MB** |
| Hijos del centinela antes de la 1.ª nota | 0 |
| Descarga por inactividad (10 s, reaper) | **sí** (0 hijos tras esperar 15 s) |
| Recarga bajo demanda (nota extra) | **sí** (el servidor vuelve a arrancar y dictamina) |
| Huérfanos con el GGUF tras parar el kernel | **0** |

## Hallazgos y cambios hechos durante T-DSH-110 (diff sin commitear)

1. **Sin reaper, el servidor no se descargaba por inactividad.** La 1.ª pasada dio `children_after_idle = 1`: el diseño de T-DSH-109 solo miraba la inactividad *al entrar la siguiente llamada*, así que un kernel ocioso mantenía el `llama-server` vivo indefinidamente. **Corregido** con un **reaper** (`spawn_idle_reaper`) que cada `idle_unload_secs/2` comprueba y mata el servidor si lleva más de `idle_unload_secs` sin uso; log confirmado: `centinela: descargo llama-server tras 14s inactivo (reaper)`.
2. **Job Object KILL_ON_JOB_CLOSE** añadido como defensa en profundidad al hijo (se cierra en `kill`/`Drop`; si el kernel muere de golpe, Windows cierra el handle y mata al hijo). Nota: el propio kernel ya crea un job object con `kill-on-job-close` al arrancar (log `Windows job object set: kill-on-job-close`, `setup.rs`), así que los hijos ya morían con el kernel; la asignación del centinela es redundante y **no se ha verificado que `AssignProcessToJobObject` tenga éxito** (si falla, se ignora silenciosamente).
3. **Falso positivo en la detección de huérfanos del script de test**: la consulta PowerShell buscaba el nombre del modelo en *todas* las líneas de comando y se contaba a sí misma (1 "huérfano" fantasma). Corregido filtrando por `Name='llama-server.exe'`; con eso el resultado real es **0 huérfanos** (las 3 pasadas anteriores también lo eran; solo fallaba la medición).
4. **Arranque en frío**: con el `timeout_ms = 20000` por defecto, la primera llamada tras cargar el modelo (≈20–40 s en este host) agotaba el timeout y caía a **fail-open** (correcto por diseño, pero la 1.ª nota maliciosa no se bloqueaba). El run final usa 60 s; **recomendación**: subir el default o pre-calentar el centinela antes de la primera nota real.

## Verificación final

```
cargo clippy -p tylluan-kernel --all-targets -- -D warnings → exit 0
cargo test -p tylluan-kernel --lib                          → 1315 passed; 0 failed; 4 ignored (47,88 s)
git diff --stat → crates/tylluan-kernel/src/security/sentinel.rs +99/−1 (único fichero)
```

## Impact

- **Confirma en vivo** que el centinela opt-in funciona dentro del kernel real: 20/20 benignas pasan, 5/5 Unsafe en cuarentena, 5/5 Controversial marcadas, latencia p50 ≈ 1,85 s, coste RSS kernel +28,6 MB y RSS del hijo ≈ 1,58 GB, descarga por inactividad y recarga bajo demanda, **0 huérfanos** al parar el kernel.
- **Cierra dos huecos reales** del backend de T-DSH-109 (reaper de inactividad; defensa de job object) con evidencia de log y medición, no solo diseño.
- Todo con puerto y DATA_DIR propios, 4 hilos, PID exacto; no se tocó `:47004`, `E:\tylluan` ni el kernel de producción.

## No verificado

- **Un solo host y un solo modelo** (Qwen3Guard Q4): no se probó Q8 ni otros GGUF en integración.
- **La 1.ª nota de cada arranque en frío tarda 20–40 s** (carga del modelo) y con `timeout_ms` por defecto (20 s) cae a fail-open: medido una vez; no se ha caracterizado la varianza ni el efecto de precargar.
- **Job Object del centinela**: no se comprobó que la asignación tenga éxito (el kernel ya mata a los hijos con su propio job); si se quiere garantía explícita, verificar con `IsProcessInJob`.
- **Reaper**: comprobación cada `idle_unload_secs/2`; con el valor de test (10 s) descarga entre 10 y ~15 s. No medido con el default de 300 s.
- **Solo 30 notas** y una BD nueva; no se probó con la BD real de producción ni con `llama_backend` del kernel ya corriendo (el centinela usa puerto efímero propio, pero no se ejercitó la convivencia).
- **No se midió CPU del hijo** ni contención con otros procesos; solo RSS.
- **No se probó la ruta `tylluan_remember`/MCP** (se usó `POST /api/v1/silva/node`, que es una de las rutas que disparan write_gate L2); las demás rutas comparten `spawn_write_gate_judge`, pero no se ejercitaron una a una.
- El diff de `sentinel.rs` (reaper + job object) **queda sin commitear** para que lo integre el TL; requiere su verificación en CI.
