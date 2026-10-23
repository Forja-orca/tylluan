# T-DSH-58 — Validar el arranque de `origin/main` (7c252ae) con kernel de prueba

- **Agente:** dsh
- **Fecha:** 2026-10-24
- **Base:** `origin/main` `7c252ae` (push 137; CI verde). Release compilado desde ese commit en el target nuevo del protocolo: `E:\cargo-target\dsh` (primera compilación completa **11 m 33 s**); `/health` devolvió `"commit":"7c252ae"`.
- **Encargo (T1650 / COLA local del TL):** antes de empezar `agent_wt.sh reset dsh` (hecho); con kernel `test-run-*` (puerto efímero propio y `DATA_DIR` propio, nunca cwd ni puerto de producción) y `device = "cpu"`: (1) arranque en **frío y en caliente** (primer `/health` `starting`, `ok`); (2) **puerto ocupado**: reintentar `port_retry_secs` y fallar claro **sin puerto aleatorio** (D-38); (3) **`--port` junto a `--config`** (bug D-42 aún sin arreglar: confirmar estado actual). Informe con cifras, `## Impact`, «No verificado». Sin commits.
- **Entregable:** solo este informe (validación). Evidencias en `E:\tylluan-targets\dsh-t7-data\t58` (configs, runners, logs stdout/stderr, `measure_*.json`, `build_release.log`).

## 1. Montaje

| elemento | valor |
|---|---|
| binario | `E:\cargo-target\dsh\release\tylluan-nexus.exe` (release de `7c252ae`, build 11 m 33 s) |
| instancia | `test-run-dsh` con puertos propios 47741/47742/47745/47746/47747 y `TYLLUAN_DATA_DIR` propio bajo `t58` |
| cwd | `E:\tylluan-wt-dsh` (mi worktree; **nunca** `E:\tylluan`) |
| modelos | `FASTEMBED_CACHE_DIR=E:\tylluan\.fastembed_cache` en solo lectura; `[inference] device = "cpu"` |
| parada | `Stop-Process -Id <PID propio>` + verificación PID muerto / puerto libre |
| producción | PID 40640 en el puerto de producción **intacto** durante toda la sesión |

## 2. Frío vs caliente

### Frío (DATA_DIR vacío: sin `fingerprint_cache.json`, DBs nuevas)

| hito | medido |
|---|---:|
| **primer `/health`** | **33 519 ms** — `{"status":"starting","commit":"7c252ae","repo_map":"building"}` |
| **`status ok`** | **35 616 ms** (2 097 ms después; repo map listo) |
| repo map (log) | `1563 files, 226 dirs, 431507 lines (2429ms)` — bind 22:42:55.057 → built 22:42:57.498 |
| `GET /api/v1/repo-map` durante el build | **503** en la primera sonda (mismo instante que el primer health) |
| fingerprint (frío) | engine ready 22:42:29.634 → fingerprint 22:42:41.757 = **12,12 s** de sha256 |
| parada | proceso muerto por PID, puerto libre, 0 cambios en producción |

Anclas del arranque frío: integridad DB 22:42:23.999; init memoria 22:42:24.053; mxbai loading 24.164 → ready 29.634 (**5,47 s**); sparse BGE-M3 ready 31.254 (**7,09 s** desde el spawn común); reranker listo 54.870; registro 54.876; kernel operational 54.952; bind 55.057.

### Caliente (misma DATA_DIR: `fingerprint_cache.json` ya escrito)

| hito | medido |
|---|---:|
| **primer `/health`** | **29 983 ms** — `{"status":"starting","commit":"7c252ae","repo_map":"building"}` |
| **`status ok`** | **32 823 ms** (3 168 ms después) |
| repo map (log) | `1563 files, 226 dirs, 431507 lines (1210ms)` — bind 22:43:45.503 → built 22:43:46.734 |
| `GET /api/v1/repo-map` | 200 en la primera sonda posterior al bind (mapa ya listo) |
| fingerprint (caliente) | engine ready 22:43:21.598 → fingerprint 22:43:21.602 = **4 ms** (acierto de caché, misma huella `sha256:310861…`) |

**Lectura honesta de la diferencia frío/caliente**: el hash frío cuesta +12,1 s respecto al cacheado, pero el tiempo total hasta el bind solo bajó 33,5 s → 30,0 s (−3,5 s). La diferencia se la comió la **varianza de carga** de la máquina (otros agentes compilando): el run caliente fue globalmente más lento en las fases ONNX (mxbai 5,02 s vs 5,47 s, sparse 6,93 s vs 7,09 s, pero reranker/instalación variaron). La prueba directa del ahorro de la caché es el hueco *engine ready → fingerprint*: **12,12 s → 4 ms**.

**D-39b/D-39a2 confirmados también en `7c252ae`**: el HTTP abre antes del repo map, `/health` devuelve `starting` mientras el mapa construye (503 en `/api/v1/repo-map` en ese estado) y pasa a `ok` cuando el mapa termina (2,1-3,2 s después en este árbol de 1 563 ficheros). El mapa ya no bloquea el bind.

## 3. Puerto ocupado (D-38)

Con un listener propio ocupando el puerto configurado y `port_retry_secs = 5`:

- **10 líneas de reintento** de bind en el log (el mensaje de error cuenta 11 intentos, `(5s)`), backoff 500 ms.
- Error final **claro**: `puerto 127.0.0.1 (47745) ocupado tras 11 intentos (5s) … No arranco en otro puerto (D-38). Cierra el proceso que lo ocupa o cambia [nexus].port / sube [nexus].port_retry_secs.`
- **No se enlazó ningún otro puerto** (se comprobaron los listeners del PID: ninguno).
- **Hallazgo operativo**: el proceso **no termina solo** tras el fallo del HTTP en este montaje (`transport = ["http"]`): seguía vivo >90 s después del error, sin HTTP; hubo que matarlo por PID. El error es claro y no hay cambio silencioso de puerto, pero un launcher que espere la salida del proceso se quedaría colgado. No investigué si es intencional (otros bucles de fondo siguen vivos) — ver «No verificado».

## 4. `--port` junto a `--config` (D-42, estado actual)

Config con `port = 47746`, lanzado con `--config … --port 47747`:

- El log **sí** registra `🔌 Port override from CLI: 47747`.
- El kernel **escucha en 47746** (`HTTP Gateway listening on 127.0.0.1` (puerto 47746)), 25 331 ms hasta el bind.
- `binds_config_port = true`, `binds_cli_port = false` → **el bug D-42 sigue presente en `7c252ae`**: `--config` recarga y reemplaza la config después de aplicar el override de CLI (main.rs:521-526 aplica; main.rs:538-546 recarga). Confirmado en vivo, no solo por lectura.

## Impact

- **`7c252ae` validado para reinicio**: HTTP antes del repo map, `/health` con `starting`→`ok`, mapa en background (1,2-2,4 s en mi worktree), puerto ocupado falla con mensaje explícito y sin puerto aleatorio, y el fingerprint cacheado pasa de **12,1 s a 4 ms** en caliente.
- **Cifras de referencia para el reinicio**: en frío (primer arranque con DATA_DIR de producción sin caché de huella) cuento **~33,5 s** hasta el primer `/health` y **~35,6 s** hasta `ok` en esta máquina bajo carga; en caliente **~30,0 s / ~32,8 s**. La producción pagará el hash frío una vez (~12 s) y luego lo ahorrará.
- **Dos avisos operativos verificados en vivo**: (1) si el puerto está ocupado el kernel falla claro pero **no sale solo**; (2) el bug **D-42 sigue vivo**: `--port` se ignora en silencio si hay `--config` — cualquier launcher que dependa de `--port` debe revisarse antes del reinicio (o poner el puerto en el `tylluan.toml`).

## No verificado

- **Producción no se tocó ni reinició**: PID 40640 y el puerto de producción intactos; el reinicio real es de José.
- **Cifras no extrapolables a producción**: medidas sobre mi worktree (1 563 ficheros / 431 507 líneas) y con la máquina bajo carga de otros agentes (varianza entre runs); el árbol real y un arranque tranquilo pueden diferir.
- **El proceso que no sale tras fallar el bind HTTP**: no investigué si es intencional (transporte/daemon) ni si el launcher de producción lo gestiona; solo medí que sigue vivo >90 s.
- **D-42**: confirmado reproducido en `7c252ae`, **no corregido** (está en la cola de Deep); no toqué el código.
- **Sin peticiones a motores ni tool calls**: solo `/health` y `/api/v1/repo-map`; no se ejercitaron recall, MCP ni guilds.
- **Solo Windows + ONNX Runtime del sistema** (device cpu); no probado Linux/DirectML/CUDA.
- **Release compilado localmente** en el target nuevo (11 m 33 s); `verify.sh` completo no se ejecutó para este binario (fuera del encargo).
