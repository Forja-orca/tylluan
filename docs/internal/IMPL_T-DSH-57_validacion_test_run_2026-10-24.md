# T-DSH-57 — Validación previa al reinicio de producción (kernel test-run propio)

- **Agente:** dsh
- **Fecha:** 2026-10-24
- **Base:** `origin/main` `85ce705` (push 133: D-39b, D-38, T-DSH-56, H-19). Release compilado desde ese commit; `/health` devolvió `"commit":"85ce705"` (el binario embebe el HEAD).
- **Encargo (T1640):** en mi worktree y con mi target dir: `cargo build --release -p tylluan-kernel`, arrancar un kernel `test-run-<dsh>` con **puerto y DATA_DIR propios** (nunca cwd/puerto de producción), medir segundos hasta `/health` responde, hasta `status ok`, hasta repo_map ok, y que `/health` devuelve `starting` mientras construye el mapa. Sin requests a motores. Matar solo por PID propio. Informe con `## Impact`, tabla de tiempos y "no verificado".
- **Entregable:** solo informe (no hay diff de código: es validación). Evidencias en `E:\tylluan-targets\dsh-t7-data\t57` (config, logs stdout/stderr por run, `measure_*.json`, runner `.ps1`, log de build).

## 1. Montaje (aislado de producción)

| elemento | valor |
|---|---|
| binario | `E:\tylluan-targets\dsh-t5-1991\release\tylluan-nexus.exe` (32 645 632 B), build release **14 m 37 s** |
| commit | `85ce705` (visible en `/health.commit`) |
| PUERTO | **47734** (test); producción `47004` nunca tocada |
| DATA_DIR | `E:\tylluan-targets\dsh-t7-data\t57\data` (vía `TYLLUAN_DATA_DIR`): DBs, PID, `fingerprint_cache.json`, `active_port.json` propios |
| cwd | `E:\tylluan-wt-dsh` (**mi worktree**, no `E:\tylluan`; el repo map se construyó sobre él) |
| modelos ONNX | `FASTEMBED_CACHE_DIR=E:\tylluan\.fastembed_cache` en **solo lectura** (mxbai+BGE-M3+Jina cargados de verdad; ninguna petición a motores) |
| device | `[inference] device = "cpu"` (ver hallazgo 6) |
| parada | `Stop-Process -Id <PID propio>` y verificación de que el PID desaparece y el puerto queda libre |

## 2. Tiempos medidos

### Run "final" (huella cacheada + caché de SO caliente, device cpu)

| hito | t desde `Start-Process` |
|---|---:|
| **primer `/health` responde** | **24 503 ms** — cuerpo `{"status":"starting","version":"0.17.1","commit":"85ce705","repo_map":"building"}` |
| **`status ok`** | **26 715 ms** (2 212 ms después del primer health; = repo map listo) |
| repo map construido (log) | 1 621 ms de build; bind 19:42:45.402 → `Repo map built` 19:42:47.026 |
| parada por PID | proceso desaparece, puerto libre |

Anclas del log de ese arranque (mismo run, para el desglose):

| fase | log |
|---|---|
| integridad DB | `[Integrity] Running startup database validation...` + `silva.db/tylluan.db healthy` (19:42:21.413) |
| init memoria | `Initializing Memory Layer` 19:42:21.440 |
| carga mxbai (paralela) | `Loading Mxbai…` 19:42:21.492 → `MXBAI-EMBED-LARGE engine ready` 19:42:27.023 (**5,53 s**) |
| **fingerprint** | `fingerprint sha256:310861…` 19:42:27.026 ⇒ **3 ms** (caché de T-DSH-54 acertada; en frío fueron ~27 s) |
| sparse BGE-M3 (paralela) | `BGE-M3 sparse engine ready` 19:42:29.691 (**8,20 s** desde el spawn común) |
| reranker (paralelo, esperado tras dense) | `Reranker BGE listo` 19:42:45.137 |
| registro/arranque | `Registry loaded: 21 guilds` 19:42:45.143; `Kernel operational` 19:42:45.246 |
| bind HTTP | `HTTP Gateway listening on 127.0.0.1` (puerto 47734) 19:42:45.402 → **health responde justo después (24,5 s)** |
| repo map (en background) | `Repo map built: 1550 files, 225 dirs, 429991 lines (1621ms)` 19:42:47.026 |

### Run frío (primer arranque con DATA_DIR vacío: huella sin cachear)

| hito | medido |
|---|---:|
| primer `/health` (`starting`) | **141 972 ms** (bind) |
| `status ok` | **189 180 ms** (47,2 s después: repo map tardó 46 717 ms bajo carga ~100 % de CPU de otros agentes) |
| fingerprint (frío) | ~27 s (engine ready 18:59:28 → fingerprint 18:59:55), hash `sha256:310861…` **idéntico** al cacheado |
| repo map | 46 717 ms (mismos 1550 ficheros); en repeticiones con SO caliente: 576 / 4 919 / 1 315 / 1 621 ms |

### Run con puerto ocupado (D-38)

Listener propio ocupando 47734 y kernel con `--port 47734` (config por defecto): **21 intentos de bind en ~10 s** (backoff 500 ms) y luego

> `ERROR … puerto 127.0.0.1 (47734) ocupado tras 21 intentos (10s): … No arranco en otro puerto (D-38). Cierra el proceso que lo ocupa o cambia [nexus].port / sube [nexus].port_retry_secs.`

Sin cambio silencioso de puerto. El proceso siguió vivo porque la config por defecto incluye transporte `stdio` (esperando entrada), no lo investigué más; se mató por PID propio.

## 3. Qué queda validado

- **D-39b**: el HTTP abre **antes** que el repo map (`bind` → `Repo map: building in background` → build) y `/health` devuelve **`starting`** mientras construye (`repo_map:"building"`), pasando a `ok` solo cuando el mapa está listo: run frío 47,2 s en `starting`, run caliente 2,2 s. `/api/v1/repo-map` respondió **503** durante la construcción en el run frío (primera sonda) y 200 después.
- **D-39a/D-39a2**: sobre mi worktree (1550 ficheros, 225 dirs, 429 991 líneas) el mapa ya no bloquea el bind; los 46,7 s del run frío son el peor caso bajo carga, no el camino crítico del puerto.
- **T-DSH-54**: huella cacheada en `<DATA_DIR>/fingerprint_cache.json`; acierto = **3 ms** vs ~27 s en frío, misma huella `sha256:310861…`.
- **T-DSH-55**: las tres sesiones ONNX arrancan juntas; mxbai lista a +5,53 s y sparse a +8,20 s **desde el mismo spawn** (en serie sparse habría terminado ≈+13,7 s) → solapamiento real; el orden de espera/instalación (sparse → dense → reranker) se mantiene.
- **D-38**: puerto ocupado ⇒ reintentos y fallo explícito, nunca puerto distinto en silencio.

## 4. Hallazgos (no corregidos: T-DSH-57 es validación)

1. **`--port` se pierde si también se pasa `--config`** (main.rs): el override se aplica en `main.rs:521-526` y después `--config` **recarga y reemplaza** `config` en `main.rs:538-546`, borrando el override. Observado: pedí 47732/47733 y el kernel escuchó en el `port` del fichero (47731). Es justo el tipo de fallo silencioso que D-38 quiere evitar; workaround usado: **puerto en el fichero de config**. Sugerencia: aplicar `--config` antes de los overrides de CLI (o re-aplicarlos después).
2. **`device=auto` → DirectML**: sin `[inference]`, un run frío enlazó a los 142 s (aceptable), pero otro intento con `device=auto` **no llegó a bindear en >20 min** (lo maté por PID). Con `device="cpu"` explícito: 24,5 s (caliente) de forma determinista. No aislé si fue DirectML o la carga brutal de la máquina; recomiendo `device="cpu"` explícito en configs de test.
3. **Rate limit 300/min**: mi primer sondeo a 25 ms disparó `429 RATE_LIMIT` (artefacto de medición, no del producto); las mediciones válidas usan 2-2,5 req/s.
4. **Primer reinicio real pagará la huella en frío**: el `DATA_DIR` de producción aún no tiene `fingerprint_cache.json`; el primer arranque con este código calculará el sha256 (1,34 GB, ~15-27 s) y los siguientes ya irán cacheados.

## Impact

- **Validación de pre-producción superada con matices**: el HTTP abre antes del repo map, `/health` expone `starting` mientras construye y pasa a `ok` al terminar; en caliente el arranque completo hasta `ok` fue **26,7 s** (24,5 s hasta el primer `/health`); la huella cacheada ahorra ~27 s; las tres sesiones ONNX se solapan; un puerto ocupado falla con mensaje explícito y sin cambio silencioso.
- **Riesgo operativo detectado antes de tocar producción**: el bug `--port`+`--config` hace que un puerto pedido por CLI se ignore en silencio (con el `port` del fichero ganando). Si el reinicio de producción se lanza con launchers que pasan `--port`, conviene verificar que el `port` del `tylluan.toml` es el correcto o no usar `--config` junto a `--port`.
- **Coste/pesimismo**: mediciones bajo carga ~100 % de CPU de otros agentes y sobre MI worktree (sin `guilds-python`/`models`), así que no son extrapolables 1:1 a `E:\tylluan`; aun así el orden de magnitud y el comportamiento de estados quedan verificados.

## No verificado

- **Producción no se tocó** (ni reinició): PID 40640 y puerto 47004 intactos durante toda la sesión; el reinicio real es de José.
- **Tiempos no extrapolables a producción**: el repo map se midió sobre mi worktree (1550 ficheros/430 k líneas), no sobre el árbol real; bajo carga extrema la varianza fue 0,6-46,7 s para el mismo árbol.
- **Causa del cuelgue con `device=auto`** (DirectML vs contención de la máquina) no aislada; solo probado Windows/CPU.
- **Bug `--port`+`--config`**: confirmado por comportamiento y lectura de código, **no corregido** (fuera del alcance de esta validación).
- **Comportamiento del proceso tras fallar el bind HTTP con config por defecto** (siguió vivo por el transporte stdio): no investigado en profundidad.
- **Sin peticiones a motores** (solo `/health` y `/api/v1/repo-map`); no se ejercitaron tool calls, recall ni el resto de endpoints.
- **Release compilado localmente** (14 m 37 s), no el binario de producción; `verify.sh` completo no se ejecutó para este release (fuera del encargo).
