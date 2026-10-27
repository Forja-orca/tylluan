# IMPL_H-38 — tool `process_top` en `guilds/watchers/plugins/system_metrics.py`

- **Asignación:** T1869 (TL, spec reafirmada en T1888/T1890/T1899). Fecha: 2026-10-27.
- **Worktree:** `D:\hermes-projects\tylluan-wt-hermes`, HEAD base `ccbf3d5` (origin/main al postear: `2142e57f`; el TL rebasea al integrar). Diff SIN commitear, regla del TL.

## Qué se hizo

1. **`collect_process_top(n=10, window_secs=5.0, iter_procs=None, sleep_fn=None)`** (`system_metrics.py:121`):
   - **Núcleos medios en ventana, no acumulado:** prime `cpu_percent(None)` en todos los procesos → `sleep(window_secs)` → segunda lectura delta. convención psutil: 100 % = 1 núcleo → `avg_cores = pct/100` (docstring en `:125-129` lo documenta explícitamente).
   - Tolerante a procesos que mueren en la ventana (`NoSuchProcess/AccessDenied/ZombieProcess` → se omite, `:146` y `:157`).
   - `iter_procs`/`sleep_fn` inyectables para testear con psutil simulado sin tocar procesos reales.
   - Campos por fila: PID, **PPID**, nombre, `avg_cores`, `mem_mb`, etiqueta de familia.
2. **`_family_label(name, ppid, kernel_pids)`** (`system_metrics.py:105`): normaliza a minúsculas y **recorta `.exe`** (Windows devuelve `tylluan-nexus.exe`; sin esto todo caía en «otros» — lo pilló el test). Familias: `tylluan-kernel` (nexus/cli por nombre), `guild-hijo-del-kernel` (ppid ∈ PIDs del kernel, `:150-151`), `llama-server`, `strata/flota` (prefijo `strata` o `watcher`), `otros`.
3. **`@mcp.tool() async def process_top(n=10, window_secs=5.0)`** (`system_metrics.py:172`): formatea la tabla. **Solo lectura pura**: cero kill/señal/modificación — psutil passivo (criterio del TL).

Sin cambios de routing: la guild `system_metrics` ya está registrada en el catálogo (AGENTS.md regla 2 aplica a guilds nuevas, no a tools dentro de una guild existente).

## Tests — `tests/python/test_system_metrics_process_top.py` (5 tests, ?? sin comitear)

- 3 con **psutil simulado** (`FakeProc` con muestras de cpu_percent): media de ventana (250 % → 2.5 cores), sort top-N, familias completas, proceso que muere en la ventana.
- 1 unit de `_family_label` (incluye variantes `.exe`).
- 1 **llamada real** smoke (ventana 0.3 s) sobre procesos vivos.

```
$ PYTHONPATH= E:/tylluan/.venv/Scripts/pytest.exe tests/python/test_system_metrics_process_top.py -q
5 passed in 18.66s
```

**Nota de entorno crítica para reproducir:** hay que limpiar `PYTHONPATH`. Las sesiones de Hermes exportan `PYTHONPATH=…/hermes/installs/…/site-packages` (contiene mcp 2.x cuyo shim `mcp/server/fastmcp.py` lanza ModuleNotFoundError), que ensombrece el **mcp 1.30.0 propio del venv** (`guilds/requirements.txt: mcp>=1.27.0,<2`). Con `PYTHONPATH=` el import de la guild funciona sin tocar dependencias (T1872 respetado: cero instalaciones en el venv del repo).

## ruff

```
$ uvx ruff@0.14.5 check guilds/watchers/plugins/system_metrics.py tests/python/test_system_metrics_process_top.py
All checks passed!
```

vía uvx (caché transitorio de uv; no se instala nada en el repo/venv — T1872).

## Ejemplo real (`python -c`, ventana 5.0 s, salida cruda)

```
Top 8 por nucleos medios (ventana 5.0s, solo lectura):
  45.75 cores  PID   19552  PPID   15424     9785.6 MB  [tylluan-kernel]  tylluan-nexus.exe
   3.06 cores  PID       0  PPID       0        0.0 MB  [otros]  System Idle Process
   1.23 cores  PID   23960  PPID   12776      656.2 MB  [otros]  Hermes.exe
   0.98 cores  PID   44936  PPID   12776      189.8 MB  [otros]  Hermes.exe
   0.74 cores  PID   30912  PPID   22984      781.3 MB  [otros]  python.exe
   0.50 cores  PID   52664  PPID   34028       66.7 MB  [otros]  python.exe
   0.43 cores  PID       4  PPID       0       21.1 MB  [otros]  System
   0.17 cores  PID    3452  PPID    2628       31.8 MB  [otros]  svchost.exe
```

Dato colateral para el equipo: el kernel de producción marca **45.75 núcleos medios** — el busy-loop que el TL diagnóstico sigue visible, ahora observable por cualquier usuario de la tool.

## Impact (regla guilds/ — gate mecánico)

- Tool nueva dentro de una guild ya registrada: cero cambios de routing/catálogo/transport.
- Si `process_top` falla: solo esa tool falla (`tools/call` devuelve error); ningún otro tool de `system_metrics` (cpu/mem/disks) se ve afectado — no comparten estado, solo el módulo.
- Coste de la llamada: dos barridos de `psutil.process_iter` (~10–20 s en Windows con ~500 procesos) + la ventana pedida. No toca el busy-loop del kernel ni spawn nada.
- Solo lectura: sin `kill`, `terminate`, `send_signal`, ni escrituras. El diseño de test (inyección de `iter_procs`) garantiza que la suite no toca procesos reales salvo el smoke intencional.

## No verificado

1. **End-to-end vía MCP**: no ejecuté `tools/call process_top` contra el kernel vivo con el plugin nuevo (kernel de producción corriendo con el código viejo; recargarlo habría requerido reinicio — fuera de alcance por la regla de no tocar prod).
2. **Familias `strata/flota` y `guild-hijo-del-kernel`** son heurísticas nombre/ppid; no hay procesos strata en este host ahora mismo, así que esas ramas están ejercitadas solo con mocks.
3. **PID 0 (`System Idle Process`) aparece en el top** — decisión de no filtrar (fiel a psutil, y la spec no pedía filtrar); podría comerse un slot del top-N en máquinas muy ociosas.
4. **Rendimiento bajo carga total**: el ejemplo real tardó 54 s de pared (import en frío del módulo mcp ~35 s + escaneo + ventana), medido sin aislar componentes.

## Diff

```
guilds/watchers/plugins/system_metrics.py | 91 +++++++++++++++++++++++
 1 file changed, 91 insertions(+)
?? tests/python/test_system_metrics_process_top.py
```
