# T-DSH-60 — `scripts/verify_restart.sh`: comprobación posterior al reinicio (solo lectura)

- **Agente:** dsh
- **Fecha:** 2026-10-24
- **Base:** `origin/main` `7b24da2` (push139). Primer comando: `bash /e/tylluan/scripts/agent_wt.sh reset dsh` (OK).
- **Encargo (T1663 / COLA local del TL):** crear `scripts/verify_restart.sh` que, con `curl` a `http://127.0.0.1:<puerto>/health` (**puerto por parámetro o por `tylluan.toml`, no fijado en el script**), compruebe: (1) `commit` == `git rev-parse --short origin/main`; (2) `status` pasa de `starting` a `ok` dentro de un máximo configurable; (3) `repo_map` llega a `ok`; (4) sale con 0/1 y **una línea por comprobación con `OK`/`FALLO`**. Excluye: arrancar/parar kernels, escribir en `data/`, tocar `verify.sh` (el cableado lo hace el TL). **No lancé ningún kernel** (orden del TL en T1663: no test-run-* con cargas pesadas hasta que avise).
- **Entregable (SIN commitear):** `scripts/verify_restart.sh` (nuevo) + este informe. Evidencias de ejecución en `E:\tylluan-targets\dsh-t7-data\t60`.

## 1. Qué hace el script

```
bash scripts/verify_restart.sh [--port N] [--timeout SEG] [--interval SEG]
                               [--host H] [--config ruta/tylluan.toml]
```

- **Puerto**: `--port`; si no, `[nexus].port` del `tylluan.toml` (raíz del repo o `./tylluan.toml`, o el de `--config`). **No hay puerto 47004 ni ningún otro fijado en el script**. El parseo es robusto a **BOM y CRLF** (el `tylluan.toml` real de producción tiene BOM: verificado en solo lectura; sin el `sed` de BOM el parseo daba puerto vacío — lo cazó la prueba).
- **Comprobación 1 — commit**: compara `commit` de `/health` con `git rev-parse --short origin/main` (si no hay `origin/main`, cae a `HEAD`). Acepta prefijo (el kernel embebe el short hash).
- **Comprobación 2 — status**: sondea `/health` cada `--interval` (defecto 2 s) hasta `--timeout` (defecto 300 s). Éxito si llega a `ok`; informa de la transición `starting@Xs -> ok@Ys`. Si el kernel ya estaba `ok` al invocarlo, lo dice explícitamente (no inventa un `starting` que no vio).
- **Comprobación 3 — repo_map**: exige `components.repo_map.status == "ok"` (subestado de D-39b); si el `status` global es `ok` pero el mapa aún no, sigue sondeando hasta el timeout.
- **Salida**: exactamente una línea por comprobación con `OK`/`FALLO` y el detalle medido; el progreso intermedio va a stderr. **Exit 0 si las tres OK, 1 si alguna falla** (2 = argumento inválido).
- **Solo lectura**: únicamente `curl -sS --max-time 5 .../health`; no escribe en `data/`, no arranca ni para nada, no toca `verify.sh`.
- **Sin Python obligatorio**: usa `python`/`python3` si están para parsear JSON; si no, hay fallback con `grep`/`sed` (probado con `PATH` sin Python).

## 2. Salidas reales (servidor /health falso, sin kernel)

Para validar la lógica sin lanzar un kernel (y sin cargas), usé un servidor HTTP falso (`fake_health_server.py`) que sirve el mismo esquema JSON de `/health` (`status`, `commit`, `components.repo_map.status`) con secuencias controladas. Resultados literales:

**A. Arranque normal (warming_up → starting → ok, repo_map building → ok)**

```
$ bash scripts/verify_restart.sh --port 47851 --timeout 20 --interval 1
OK    commit    /health.commit=7b24da2 == origin/main 7b24da2
OK    status    starting@0s -> ok@4s (timeout 20s)
OK    repo_map  components.repo_map.status=ok@4s (total 5s)
exit: 0
```

**B. Kernel ya arrancado (sin observar `starting`)**

```
OK    commit    /health.commit=7b24da2 == origin/main 7b24da2
OK    status    ok@0s (no se observó starting: el kernel ya estaba ok)
OK    repo_map  components.repo_map.status=ok@0s (total 1s)
exit: 0
```

**C. Commit distinto (p. ej. binario viejo)**

```
FALLO commit    /health.commit=deadbee != origin/main 7b24da2
OK    status    ok@0s (no se observó starting: el kernel ya estaba ok)
OK    repo_map  components.repo_map.status=ok@0s (total 1s)
exit: 1
```

**D. Nunca llega a `ok` (timeout configurable a 4 s)**

```
OK    commit    /health.commit=7b24da2 == origin/main 7b24da2
FALLO status    no llegó a ok en 4s (último: starting)
FALLO repo_map  components.repo_map.status=building tras 4s
exit: 1
```

**E. Puerto resuelto desde `tylluan.toml` con BOM (sin `--port`)**

```
$ bash scripts/verify_restart.sh --config .../tylluan.toml --timeout 15
OK    commit    /health.commit=7b24da2 == origin/main 7b24da2
OK    status    starting@0s -> ok@4s (timeout 15s)
OK    repo_map  components.repo_map.status=ok@4s (total 6s)
exit: 0
```

**F. Sin Python en el PATH (fallback `grep`/`sed`)**: mismo resultado A (`exit 0`, tres `OK`).
**G. Sin `--port` y sin `tylluan.toml`**: `FALLO puerto: no hay --port ni [nexus].port en tylluan.toml`, `exit 1`.
**H. `--help`**: imprime el uso, `exit 0`.
**Sintaxis**: `bash -n scripts/verify_restart.sh` → OK en Git Bash.

## Impact

- **Gate determinista para el reinicio**: con una sola orden se comprueba lo que importa tras reiniciar — que el binario en marcha es el `origin/main` correcto, que el arranque pasó de `starting` a `ok` en un margen configurable y que el repo map terminó (D-39b) — con salida `OK`/`FALLO` por comprobación y exit code apto para el `verify.sh`/CI que cablee el TL.
- **Sin constantes peligrosas**: no fija puerto (lo toma de argumento o de `tylluan.toml`), es solo lectura y no arranca/para nada; no toca `data/` ni `verify.sh`.
- **Hallazgo útil incorporado**: el `tylluan.toml` de producción lleva **BOM**; sin normalizarlo el puerto se leía vacío. El script lo maneja (y CRLF) con `sed`/`tr`.

## No verificado

- **No probado aún contra un kernel real / `test-run-*`**: el TL pidió no lanzar test-run-* con cargas hasta avisar del reinicio. Las salidas de arriba son de un **servidor falso** con el esquema JSON de `/health` tomado del código; cuando el TL avise del reinicio, lo ejecutaré contra un test-run propio y publicaré los tiempos reales (`starting`→`ok` y repo_map).
- **Supuestos del esquema**: asumo `status ∈ {warming_up, starting, ok}` y `components.repo_map.status ∈ {building, ok, failed}` por el código de `http/mod.rs`/`repo_map.rs`; no los he visto en vivo en esta máquina (el kernel de producción no se consultó).
- **`git rev-parse --short origin/main`**: exige que el worktree tenga `origin/main` actualizado; si alguien ejecuta el script con un fetch viejo, la comprobación 1 puede dar FALLO aunque el binario sea correcto (se avisa en el detalle).
- **Fallback sin Python**: probado solo en Git Bash/Windows (con `PATH` recortado); no probado en Linux ni con JSON con campos anidados distintos.
- **No cableado en `verify.sh`/CI** (excluido a propósito; lo hace el TL).
- **Solo Git Bash (Windows)**; el script es bash POSIX pero no lo ejecuté en Linux.
