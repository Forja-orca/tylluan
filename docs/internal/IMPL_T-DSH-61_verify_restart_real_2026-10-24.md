# T-DSH-61 — Verificación real post-reinicio: propuesta de comprobaciones que faltan (previo al aviso del TL)

- **Agente:** dsh
- **Fecha:** 2026-10-24
- **Base:** `origin/main` `7b24da2` (reset de worktree hecho con `agent_wt.sh reset dsh` antes de empezar).
- **Encargo (T1667):** tras el reinicio (cuando el TL avise) ejecutar `scripts/verify_restart.sh` contra el kernel real y entregar informe con la **salida literal**; **mientras tanto, solo lectura**: lista de **5 comprobaciones post-reinicio que falten** (mencionadas por el TL: guilds en Python 3.12.15, D-46, puerto ocupado) **como propuesta, sin lanzar kernels**.
- **Estado:** este informe contiene de momento la **propuesta** (sin lanzar kernels). La sección de salida literal contra el kernel real se rellenará cuando el TL anuncie el reinicio.
- **Corrección del TL incorporada (T1667):** el `/health` público devuelve `repo_map` en el **nivel superior** (`HealthResponse { status, version, commit, repo_map }`, `mod.rs` línea 1660) y no `components.repo_map.status` (eso es `/health?verbose=true`). El script `scripts/verify_restart.sh` se ha ajustado para **aceptar ambas formas** (probado con un servidor falso que emite el esquema top-level real y el verbose). Salida actual:

```
OK    commit    /health.commit=7b24da2 == origin/main 7b24da2
OK    status    starting@0s -> ok@3s (timeout 20s)
OK    repo_map  /health repo_map=ok@3s (total 6s)
```

(igual resultado con el esquema `?verbose=true`; `FALLO` y exit 1 en commit distinto y en "nunca ok").

## Propuesta: 5 comprobaciones post-reinicio que faltan

Las tres primeras son las que citó el TL; las dos últimas son huecos reales medidos en esta sesión.

### 1. Guilds lanzadas con Python 3.12.15 (y no con el `.venv` viejo)
**Qué falta:** T-DSH-59 dejó `E:\python-guilds-3.12.15` (Python 3.12.15 + OpenSSL 3.5.9, sha256 verificado) pero `guild_process::find_python` prefiere `<raíz>/.venv/Scripts/python.exe`; no hay comprobación post-reinicio de **qué intérprete usa realmente cada guild** ni de que las always-on arrancan con el nuevo.
**Comprobación propuesta (solo lectura + arranque controlado de guilds, no del kernel):**
- para cada guild always-on (`bash`, `filesystem`, `monitor`, `system_metrics`, `vision`, `knowledge`, `comfy_ui`, `coloquio`, `websearch`, `deep_web_research`, `code_graph`, `scrapling`, `scheduler`): arrancar su `.py` con el intérprete declarado en la config y hacer `initialize` + `tools/list` por stdio (como el smoke de `filesystem` de T-DSH-59);
- comprobar `sys.version`/`ssl.OPENSSL_VERSION` del proceso de la guild;
- contrastar con los **9 guilds que con solo `requirements.txt` fallan por dependencias opcionales** (T-DSH-59): `vision`, `pdf`, `ingest`, `deep_web_research`, `night_reasoner`, `llama_backend`, `knowledge`, `code_graph`, `scrapling` → decidir si se instalan opcionales o se acepta degradación.

### 2. D-46: qué hace el proceso cuando falla el bind HTTP
**Qué falta:** medido en T-DSH-58 que, con el puerto ocupado, el kernel loguea `No arranco en otro puerto (D-38)` pero **sigue vivo >90 s sin HTTP** (hubo que matarlo por PID). No hay comprobación post-reinicio del comportamiento real: exit code, PID file, si el launcher/supervisor se cuelga, y si el proceso queda zombie.
**Comprobación propuesta:** con un listener propio ocupando un puerto de test y config aislada, lanzar el binario, medir: segundos hasta el ERROR, exit code, si termina solo (hoy no), estado de `data/active_port.json` y del PID file; documentar el comportamiento esperado para el arranque real.

### 3. Puerto ocupado (D-38): no cambio silencioso ni daño al kernel vivo
**Qué falta:** D-38 se validó en test-run aislado (T-DSH-58), pero no post-reinicio sobre la instalación de producción: que un segundo arranque con el puerto de producción ocupado **falle claro**, **no elija puerto aleatorio** y **no toque** al kernel vivo (anti-orphan/shutdown).
**Comprobación propuesta:** ocupar el puerto de producción con un listener propio (sin matar al kernel), intentar arrancar un binario de test con `port_retry_secs` corto; verificar el ERROR literal, que no se abre otro puerto y que el kernel original sigue respondiendo; repetir con `--port` + `--config` para confirmar D-42 (ya corregido por Deep).

### 4. repo_map real de producción: tiempo y cifras (no solo "ok")
**Qué falta:** `verify_restart.sh` comprueba que `repo_map` llega a `ok`, pero no **cuánto tarda** ni **qué cifras** da sobre el árbol real de producción (con `guilds-python`/`models` excluidos por D-39a2). T-DSH-52/57 midieron en worktree (1 563 ficheros), no en `E:\tylluan` (~16 k ficheros tras D-39a2).
**Comprobación propuesta:** registrar el intervalo entre el primer `/health` con `status=starting` y `repo_map=ok`, y luego leer `files/dirs/lines/build_duration_ms` (bien vía `/health?verbose=true`, bien vía el log `Repo map built: ...`); comparar con la medición de T-DSH-52 y confirmar que el puerto abre antes (D-39b) y que no vuelve el bloqueo de 16-25 min.

### 5. Huella cacheada y segundo arranque (T-DSH-54)
**Qué falta:** no se ha comprobado nunca en producción que, tras el primer arranque (frío, paga el sha256 de 1,34 GB), el `data/fingerprint_cache.json` se crea y un **segundo arranque** no re-hashea. T-DSH-57 midió 12,1 s frío vs 4 ms caliente en test-run.
**Comprobación propuesta:** tras el reinicio, comprobar que existe `data/fingerprint_cache.json` con entradas del `model.onnx` de mxbai + tokenizer; en un segundo arranque de test (o tras el siguiente reinicio), verificar en el log que el hueco `engine ready → fingerprint` es de milisegundos y no de ~12 s; opcionalmente instrumentar el log de T-DSH-44 (cache miss) que Deep añadió.

## Pendiente (se rellenará con el aviso del TL)

- Ejecutar `bash scripts/verify_restart.sh` contra el kernel recién reiniciado (puerto de producción **sin fijar en el script**, timeout configurable) y pegar la **salida literal** (3 líneas `OK/FALLO` + exit code).
- Anotar tiempos reales de `starting`→`ok` y de `repo_map`, y el `commit` reportado por `/health` (debe ser el mismo que `origin/main` en ese momento).
- Si el reinicio trae el 3.12.15 cableado o los opcionales instalados, revisar la comprobación 1 con datos reales.
- **No se lanzó ningún kernel en esta fase** (solo un servidor `/health` falso para validar el script).

## Impact

- Deja preparada la verificación post-reinicio con el **esquema real** de `/health` (corrección del TL) y una lista concreta y priorizada de lo que el script actual **no** cubre, para decidir qué merece automatizarse y qué se comprueba a mano tras el reinicio.
- Evita falsos positivos: sin la corrección del campo `repo_map` el script habría fallado contra el kernel real aunque todo estuviera bien.

## No verificado

- **Todo lo de este informe es propuesta/lectura**: no se ejecutó contra el kernel real ni se lanzaron kernels; los tiempos y salidas reales quedan pendientes del aviso de reinicio.
- Las cifras de referencia (1 563 ficheros, 12,1 s de huella, etc.) son de test-run/worktree de esta sesión, no de producción.
- No se implementaron los 5 scripts/extensiones propuestos (el encargo pedía la lista, no el código).
