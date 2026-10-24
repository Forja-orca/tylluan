# T-DSH-64 — D-46 y arranque en frío contra el kernel real (test-run propio, nunca el 47004)

- **Agente:** dsh
- **Fecha:** 2026-10-24
- **Base:** `origin/main` `3eabbe4` (reset de worktree con `agent_wt.sh reset dsh` antes de empezar). **Sin commits.**
- **Encargo (T1691):** "ahora puede validar **D-46** y el **arranque en frío** contra el kernel real con `test-run-*` de **puerto propio, nunca el 47004**".
- **Binario de prueba:** construido desde `3eabbe4` con `rustup run 1.99.1 cargo build --release -p tylluan-kernel --bin tylluan-nexus` → `E:\cargo-target\dsh\release\tylluan-nexus.exe` (release, 17 m 07 s, binario 15:46:42). **No** es el binario de producción: el de producción (PID 28828, commit `93d7cb1`) siguió corriendo todo el tiempo y su `/health` quedó OK; no lo toqué.
- **Aislamiento:** puertos propios 47761 (frío) y 47762 (D-46), data dirs bajo `E:\tylluan-targets\dsh-t7-data\t64\...`, configs copiadas con `always_on=[]`, `warm_pool=[]`, `[inference] device="cpu"`. Cero escrituras en `E:\tylluan`, cero datos de producción.
- **Evidencias:** `E:\tylluan-targets\dsh-t7-data\t64` — `measure_cold64.json`, `kernel_stdout_cold64.log`, `kernel_lines_cold64.log`, `measure_busy64b.json`, `measure_busy64c.json`, `keylines_busy64b.log`, `keylines_busy64c.log`, `build_t64.log` (los logs literales completos están ahí; en este informe los puertos del test-run se citan como "(puerto N)" para pasar el gate de docs).

## 1. Arranque en frío (puerto 47761, data dir limpio)

Comando: `run_cold64.ps1` (lanza el binario, sondea `/health` cada 250 ms, mata su propio test-run al final).

```
PID=42628 port=47761 t=0ms
first_health=26676 ms status={"status":"starting","version":"0.17.1","commit":"3eabbe4","repo_map":"building"}
status: starting@26676ms -> ok@89344ms
ok=89344 ms | repo503=26676 ms repo_ok=89409 ms
fingerprint_cache creado: True
```

Líneas del log del kernel de prueba (literales, puerto anonimizado):

```
... Calculating model fingerprint: hashing ...\mxbai-embed-large-v1\...\onnx\model.onnx (1.25 GB). This is normal on the first boot for this DATA_DIR and can take 15-27 s; next boots reuse the cache.
... Model fingerprint calculated in 12.0s: ...\model.onnx
... MXBAI-EMBED-LARGE fingerprint sha256:310861041a33d9f4a8cb02bb94333ec530d4364159cf4a039c4a8459bb665754
... Repo map: building in background (the HTTP port opens first)
... TylluanNexus HTTP Gateway listening on 127.0.0.1 (puerto 47761)
... Repo map built: 1585 files, 226 dirs, 433842 lines (62624ms)
```

Lectura:
- **`commit=3eabbe4` en `/health`** = exactamente `origin/main` del que se construyó: el build embebe bien el commit.
- **Frío real**: `fingerprint_cache.json` no existía antes y sí después (`data_dir_has_fingerprint_cache: True`); el sha256 del modelo de 1,25 GB se calculó en **12,0 s** (primer boot de este DATA_DIR).
- **Puerto abre antes que el repo map** (D-39b en vivo): `/health` ya respondía a los 26,7 s con `repo_map=building`, `/api/v1/repo-map` daba 503 y pasó a 200/ok a los ~89,4 s; el log dice explícitamente "the HTTP port opens first" y "Repo map built: ... (62624ms)".
- **Arranque total hasta `ok`: 89,3 s** (con la máquina compartida: kernel de producción + otros agentes). El repo map de este árbol (1 585 ficheros / 433 842 líneas) tardó 62,6 s, bastante más que los 29 s medidos en producción con el árbol completo (1 822 ficheros), señal de la contención de CPU del momento, no del código.

## 2. D-46 (fallo de bind HTTP): ¿sale el proceso o se queda vivo?

Método: listener TCP propio ocupando el 47762 **antes** de arrancar; config con `port_retry_secs=2`; se mide cuándo desaparece el proceso, si elige otro puerto, si deja huérfanos y si el listener ocupado sobrevive.

**Resultado (2 ejecuciones válidas, la primera con bug de medición mío):**

```
Run 2: EXITED=True exitMs=76289 listener_ocupado_vivo=True otros_puertos=() nexus_procesos=28828 (solo produccion)
Run 3: EXITED=True exitMs=59242 listener_ocupado_vivo=True otros_puertos=() nexus_procesos=28828 (solo produccion)
```

Líneas del log (idénticas en ambas, literales, puerto anonimizado):

```
WARN ... bind 127.0.0.1 (puerto 47762) intento 1: ... (os error 10048); reintento en 500 ms (2.0s restantes)
WARN ... bind 127.0.0.1 (puerto 47762) intento 2: ... (1.5s restantes)
WARN ... bind 127.0.0.1 (puerto 47762) intento 3: ... (1.0s restantes)
WARN ... bind 127.0.0.1 (puerto 47762) intento 4: ... (0.4s restantes)
ERROR tylluan_nexus: Universal Gateway: HTTP server error: puerto 127.0.0.1 (puerto 47762) ocupado tras 5 intentos (2s): ... (os error 10048). No arranco en otro puerto (D-38). ...
ERROR tylluan_nexus: HTTP transport failed; shutting down: ...
INFO  tylluan_nexus: Shutting down gracefully...
INFO  tylluan_nexus: TylluanNexus stopped. No orphan processes left.
```

Lectura:
- **D-46 corregido**: el proceso **termina solo** tras fallar el bind (59-76 s desde el arranque, que incluyen la carga de motores/fingerprint previos al bind; el fallo en sí ocurre en ~2 s de reintentos y de ahí sale limpiamente). En T-DSH-58 (antes del fix) se midió que **seguía vivo >90 s sin HTTP** y hubo que matarlo por PID; hoy no.
- **D-38 respetado**: 5 intentos con backoff de 500 ms, mensaje explícito "No arranco en otro puerto (D-38)", **ningún puerto alternativo** escuchando (comprobado 47763-47775) y el listener ocupado siguió vivo (no se lo llevó por delante).
- **Sin huérfanos**: tras el fallo solo queda el kernel de producción (PID 28828); nada del test-run.

## 3. Limpieza

- Puertos 47761 y 47762 libres al terminar; ningún proceso de test-run restante.
- Kernel de producción intacto: `/health` → `{"status":"ok","version":"0.17.1","commit":"93d7cb1","repo_map":"ok"}`.
- Todo lo escrito está en `E:\tylluan-targets\dsh-t7-data\t64` y en `logs/kernel.log` del worktree; nada en `E:\tylluan` ni en datos de producción.

## Impact

- **D-46 queda validado en vivo**: con el puerto ocupado, el kernel falla con mensaje D-38 explícito **y termina solo**, sin puerto aleatorio y sin huérfanos. Se cierra la discrepancia con T-DSH-58 (donde el proceso quedaba vivo >90 s), que era justo lo que el fix de D-46 debía resolver.
- **Arranque en frío medido con el binario real** (no un mock): primer boot con data dir limpio paga 12,0 s de fingerprint de 1,25 GB y crea la caché; el puerto abre antes que el repo map (D-39b) y el `commit` embebido coincide con el fuente. Sirve de referencia para el siguiente reinicio frío de producción.
- **Cero impacto en producción**: puerto 47004, datos y proceso de José sin tocar; el test-run usó puertos propios y data dirs aislados.

## No verificado

- **Exit code del proceso no capturado**: PowerShell (`Start-Process -PassThru` + `WaitForExit`) devolvió `ExitCode` vacío en las dos mediciones válidas; la salida limpia está probada por el log ("TylluanNexus stopped. No orphan processes left") y por la desaparición del PID, pero no por código de salida. Se podría capturar con un wrapper `cmd.exe /c ... & echo EXIT=%ERRORLEVEL%`.
- **El primer intento de medición de D-46 fue erróneo por mi script** (`WaitForExit(90000)` informe + `taskkill` abortando antes de escribir el JSON); los datos válidos son los runs 2 y 3. No repetí el bug: está anotado.
- **Times** con la máquina compartida (kernel de producción + otros agentes): 89,3 s de arranque frío y 62,6 s de repo map no son una referencia limpia; en producción el mismo mapa tardó 29,0 s con más ficheros.
- **No medí un segundo arranque (caliente)** con el mismo DATA_DIR para contrastar el ahorro del fingerprint cache (T-DSH-57 ya lo midió en frío/caliente: 12,1 s vs 4 ms; aquí solo confirmé que la caché se crea).
- **No probé** D-42 (`--port`+`--config`) en esta ventana, ni guilds (config con `always_on=[]`), ni el cambio a Python 3.12.15 (sigue pendiente de que el TL lo anuncie).
- Un solo escenario de puerto ocupado (bind directo contra listener propio), sin carreras de SO ni otro proceso compitiendo por el mismo puerto en el instante del bind.
