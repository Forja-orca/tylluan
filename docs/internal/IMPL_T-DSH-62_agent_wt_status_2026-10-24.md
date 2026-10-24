# T-DSH-62 — `agent_wt.sh status`: por qué falla/va lento en este host (solo lectura)

- **Agente:** dsh
- **Fecha:** 2026-10-24
- **Base:** `origin/main` `7b24da2` (reset de worktree hecho antes de empezar). **No se editó `scripts/agent_wt.sh`** ni ningún otro fichero; sin kernels.
- **Encargo (T1679):** solo lectura de `scripts/agent_wt.sh`; reportar por qué `status` falla o va lento en este host: comando exacto, salida literal, tiempo medido con `time`, causa probable con fichero y línea, arreglo mínimo propuesto. Informe con `## Impact` y «No verificado».
- **Evidencias:** `E:\tylluan-targets\dsh-t7-data\t62` (logs literales `wsl_status.log`, `wsl_reset.log`, `probe_gitbash.log`, `probe_wsl.log`, `spawn_cost.log` y scripts de medida).

## 1. Comando exacto y salidas literales

**A) Con Git Bash (el shell que asume el protocolo): funciona, pero tarda ~6 s**

```
$ bash /e/tylluan/scripts/agent_wt.sh status
origin/main = 7b24da2
AGENTE       HEAD      CAMBIOS  ATRASO    COMPILACION
deep         7c252ae   15       8         compartida (/e/cargo-target/deep)
buffy        c3a1066   11       58        EN EL WORKTREE (antigua: usar setup y migrate)
dsh          7b24da2   0        0         compartida (/e/cargo-target/dsh)
antigravity  5e49bf8   9        180       (ninguna)
codex        0ac9dfb   1        313       (ninguna)
readme       97ba01e   3        76        EN EL WORKTREE (antigua: usar setup y migrate)
hermes       4cb3d26   2        0         compartida (/e/cargo-target/hermes)
scratch      (sin worktree)
EXIT=0

real    0m5,846s
user    0m1,378s
sys     0m2,078s
```

`user+sys` (3,4 s) es muy inferior a `real` (5,8 s): el proceso pasa la mayor parte del tiempo **esperando procesos hijos** (`git.exe`), no calculando.

**B) Con el `bash` por defecto del host (WSL): "falla" en silencio y devuelve datos vacíos con exit 0**

En esta máquina, `bash` resuelve a **`C:\Windows\system32\bash.exe` (WSL)**:

```
$ bash -lc 'bash /e/tylluan/scripts/agent_wt.sh status'
fatal: cannot change to '/e/tylluan': No such file or directory
origin/main = 
AGENTE       HEAD      CAMBIOS  ATRASO    COMPILACION
deep         (sin worktree)
buffy        (sin worktree)
dsh          (sin worktree)
antigravity  (sin worktree)
codex        (sin worktree)
readme       (sin worktree)
hermes       (sin worktree)
scratch      (sin worktree)
EXIT=0
```

Y el mismo shell para `reset`:

```
$ bash -lc 'bash /mnt/e/tylluan/scripts/agent_wt.sh reset dsh'
ERROR: no existe el worktree /e/tylluan-wt-dsh
```

Es decir: bajo WSL el `status` **no falla con error**: pinta la tabla entera con `(sin worktree)` y `origin/main =` vacío, y sale con **0**. Eso es lo que parece "roto" desde fuera: datos falsos sin señal de error (coincide con lo que reportó Hermes: «agent_wt roto en este host», T1665).

## 2. Sonda de host (misma máquina, dos shells)

| | Git Bash (MSYS/MinGW) | WSL (Ubuntu) |
|---|---|---|
| `uname -s` | `MINGW64_NT-10.0-26100` | `Linux` |
| `/e/tylluan` existe | **sí** | **no** |
| `/mnt/e/tylluan` existe | no | **sí** |
| `cygpath` | presente (`/usr/bin/cygpath`) | **MISSING** |
| `tasklist` | presente (`/c/Windows/system32/tasklist`) | **MISSING** |
| `git` | `/mingw64/bin/git` | `/usr/bin/git` |

El script asume MSYS en todo: rutas `/e/...` (`agent_wt.sh:20-23`), `wt_path` con `/d/...` para hermes (`agent_wt.sh:28-33`), `cygpath` (`agent_wt.sh:50`) y `tasklist` (`agent_wt.sh:47`). Bajo WSL ninguna de esas rutas/herramientas existe.

## 3. Por qué va lento (medido, Git Bash)

Coste fijo de arrancar `git.exe` en este Windows:

```
10 x git --version => 1378 ms  =>  137 ms por invocación
git -C /e/tylluan fetch origin                 => 1123 ms
git -C /e/tylluan rev-parse --short origin/main => 229 ms
deep:        rev-parse 208 | status 224 | rev-list 239 ms
buffy:       rev-parse 207 | status 265 | rev-list 279 ms
dsh:         rev-parse 208 | status 225 | rev-list 216 ms
antigravity: rev-parse 201 | status 231 | rev-list 261 ms
codex:       rev-parse 200 | status 219 | rev-list 277 ms
readme:      rev-parse 207 | status 254 | rev-list 229 ms
hermes:      rev-parse 199 | status 225 | rev-list 209 ms
TOTAL agent_wt.sh status (Git Bash) => 6084 ms
```

`cmd_status` (`agent_wt.sh:103-122`) hace **22 procesos `git` secuenciales**:

- 1 `git fetch` (`:104`) → **1123 ms** (red; no hace falta para pintar el estado);
- 1 `rev-parse origin/main` en el principal (`:106`) → 229 ms;
- por cada uno de los 7 worktrees con `.git`: `rev-parse` (`:114`) + `status --porcelain` (`:115`) + `rev-list` (`:116`) → **3 procesos × 7 = 21** → ~200-280 ms cada uno ≈ **4,7 s**.

Total ≈ 1,1 s (fetch) + 0,2 s (principal) + 4,7 s (21 spawns) + comprobaciones de fichero ≈ **6,1 s**, que coincide con el `real 5,8 s`. El coste no está en el tamaño de los repos (dsh está limpio y tarda igual): está en **arrancar `git.exe` 22 veces** en Windows.

## 4. Causa probable (fichero:línea)

1. **Falla bajo WSL**: rutas MSYS hardcodeadas y sin guardia de shell:
   - `agent_wt.sh:20-23` (`MAIN_REPO="/e/tylluan"`, `TARGET_ROOT`, `PATCH_ROOT`);
   - `agent_wt.sh:28-33` (`wt_path` devuelve `/e/tylluan-wt-<agente>` y `/d/hermes-projects/...`);
   - `agent_wt.sh:112` (`[ ! -e "$wt/.git" ]` → cierto para todos bajo WSL → `(sin worktree)`);
   - `agent_wt.sh:104-106` (`git -C "$MAIN_REPO"` falla; `origin/main` queda vacío);
   - `agent_wt.sh:43` (`need_agent` muere con `no existe el worktree` en `reset/setup`);
   - `agent_wt.sh:47` y `:50` (`tasklist`/`cygpath` no existen en WSL; `cargo_running` devuelve siempre falso y `win_path` rompería).
   No hay ninguna comprobación de `uname`/existencia de `$MAIN_REPO` al arrancar.
2. **Lentitud bajo Git Bash**: `cmd_status` lanza 22 `git.exe` secuenciales (`:104`, `:106`, y `:114-116` en bucle) con ~137-280 ms de coste fijo cada uno, más un `fetch` de red (`:104`) que `status` no necesita. La ejecución es secuencial aunque los worktrees son independientes.

## 5. Arreglo mínimo propuesto (NO aplicado: solo lectura)

1. **Guardia de host** al principio (tras `AGENTS`, sobre `agent_wt.sh:24`):
   ```bash
   [ -d "$MAIN_REPO" ] || die "MAIN_REPO=$MAIN_REPO no existe: usa Git Bash (MSYS), no WSL. Bajo WSL los paths son /mnt/e/... y cygpath/tasklist no existen"
   ```
   Opcional (auto-adaptación, 4 líneas): si `[ ! -d "$MAIN_REPO" ] && [ -d /mnt/e/tylluan ]`, reescribir `MAIN_REPO=/mnt/e/tylluan`, `TARGET_ROOT=/mnt/e/cargo-target`, `PATCH_ROOT=/mnt/e/tylluan-patches` y `wt_path` a `/mnt/e/...` (hermes `/mnt/d/...`). Con esto el `status` bajo WSL deja de mentir (hoy sale 0 con tabla vacía).
2. **`status` sin red por defecto** (`agent_wt.sh:104`): mover el `fetch` detrás de un flag, p. ej.
   ```bash
   [ "${AGENT_WT_FETCH:-0}" = "1" ] && git -C "$MAIN_REPO" fetch -q origin 2>/dev/null
   ```
   (o `timeout 2 git … fetch`). Ahorra ~1,1 s y quita la dependencia de red a un comando de estado.
3. **Recortar spawns en `cmd_status`** (`agent_wt.sh:114-116`): el arreglo de mayor impacto y mínimo cambio es ejecutar el bloque de cada agente **en paralelo** (7 subshells en background escribiendo a un temporal y `wait`), con lo que el muro pasa de ~4,7 s a ~0,3 s (el coste de un spawn). Alternativa más conservadora: fusionar `rev-parse`+`status` en un solo `git -C "$wt" status --porcelain=v2 --branch` (2 spawns por worktree en vez de 3: ~1,6 s menos), dejando `rev-list` para el `ATRASO`.
4. **Portabilidad de helpers**: `cargo_running` (`:47`) con fallback `command -v tasklist >/dev/null || pgrep -qx cargo`; `win_path` (`:50`) con fallback cuando no hay `cygpath`.

## Impact

- **Explica el "agent_wt roto en este host"**: en esta máquina `bash` es WSL, no Git Bash; el script no detecta el shell y devuelve `status` **vacío con exit 0** (mentira silenciosa) en vez de un error claro. Un guard de 3 líneas lo convierte en un fallo explícito y accionable.
- **Cuantifica la lentitud**: 22 arranques de `git.exe` (~137-280 ms cada uno) + 1,1 s de `fetch` = ~6 s para un `status` que no debería tocar la red; paralelizar el bucle y quitar el fetch por defecto lo deja por debajo de ~0,5-1 s sin cambiar el significado de la tabla.
- **Sin cambios de comportamiento propuestos** en `reset/setup` (los arreglos son de robustez/rendimiento de `status` y helpers).

## No verificado

- **No apliqué el arreglo** (el encargo era solo lectura): las propuestas no están probadas; en particular la paralelización de `cmd_status` y el auto-remap `/e`→`/mnt/e` para WSL.
- **No lo probé en el host real de Hermes**: la reproducción es en esta máquina con `bash` = WSL (`C:\Windows\system32\bash.exe`); otro host puede tener `bash` de Git Bash y no sufrir el fallo (solo la lentitud).
- **Tiempos variables**: medidos con la máquina cargada (otros agentes); en frío/caliente cambia, pero el orden de magnitud (coste por spawn de `git.exe`) es estable.
- **No medí** el efecto de exclusiones de Windows Defender ni de `core.fsmonitor`; no probé `timeout` en el fetch ni el paralelismo con `wait` (solo lo propongo).
- **`status` bajo WSL puede depender de la distro**: aquí Ubuntu; no probé docker-desktop ni otras.
