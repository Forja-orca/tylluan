# T-DSH-63 — `agent_wt.sh status`: fetch opt-in, un solo `git status --porcelain=v2` por worktree y `rd` portable

- **Agente:** dsh
- **Fecha:** 2026-10-24
- **Base:** `origin/main` `da2ab9e` (push140; reset de worktree con `agent_wt.sh reset dsh` antes de empezar). **Sin commits.**
- **Encargo (T1682/T1683):** diff sin commitear para `scripts/agent_wt.sh` `cmd_status`: (a) `fetch` solo con `AGENT_WT_FETCH=1`; (b) un solo `git status --porcelain=v2 --branch` por worktree en vez de `rev-parse` + `status`; criterio: `time bash scripts/agent_wt.sh status` antes/después con la **misma tabla** (diff de salida vacío salvo ATRASO si cambia el fetch) y `bash -n`. Además (T1683): `cmd //c rd` de las líneas 136/162 es idioma de Git Bash y falla en otros shells → usar `MSYS2_ARG_CONV_EXCL='*' cmd.exe /c rd /s /q` con fallback. Informe con `## Impact` y «No verificado».
- **Entregable:** `scripts/agent_wt.sh` modificado, sin commitear (`+29 / -6`); evidencias de medidas en `E:\tylluan-targets\dsh-t7-data\t63` (`diff.patch`, `compare.log`, `abba.log`, `bench_cpu_ops.log`, `microbench.log`).

## 1. Cambios (diff)

1. **`fetch` opt-in** en `cmd_status`:
   ```bash
   if [ "${AGENT_WT_FETCH:-0}" = "1" ]; then
     git -C "$MAIN_REPO" fetch -q origin 2>/dev/null
   fi
   ```
2. **Un solo `git` por worktree para HEAD + CAMBIOS**: se sustituyen `git rev-parse --short HEAD` + `git status --porcelain | wc -l` por una invocación a
   ```bash
   out="$(git -C "$wt" status --porcelain=v2 --branch 2>/dev/null)"
   ```
   de la que se extrae `# branch.oid` (HEAD, 7 caracteres) y se cuentan las líneas que no empiezan por `#` (CAMBIOS). El ATRASO sigue con `git rev-list --count HEAD..origin/main` (una invocación).
3. **`win_rm()` portable** (junto a `win_path`): usa `MSYS2_ARG_CONV_EXCL='*' cmd.exe /c rd /s /q` con `cygpath -w` cuando existe, y `rm -rf` como fallback; reemplaza `cmd //c rd /s /q …` en `cmd_migrate` y `cmd_gc --apply`.

## 2. Comprobaciones (salidas literales)

**`bash -n` y diff:**
```
$ bash -n scripts/agent_wt.sh && echo SINTAXIS-OK
SINTAXIS-OK
$ git diff --stat
 scripts/agent_wt.sh | 35 +++++++++++++++++++++++++++++------
 1 file changed, 29 insertions(+), 6 deletions(-)
```

**Tabla antes vs después** — idéntica. Los logs crudos difieren *solo* en la línea `dsh` (CAMBIOS 0→1) porque el worktree `dsh` contiene este mismo parche; ejecutando las dos versiones (orig `HEAD` vs parche) con el mismo estado de trabajo vía shim que cuenta `git`, el `diff` de las tablas sale **vacío**:

```
TABLA-IDENTICA (via shim)
```

**Nº de invocaciones de `git`** (shim en `PATH` que registra cada llamada; verificación determinista, independiente de la carga):

```
before: 23 invocaciones de git
after:  15 invocaciones de git   (-8, -35 %)
```

Antes: `fetch` + `rev-parse origin/main` + 7 × (`rev-parse`, `status --porcelain`, `rev-list`).
Después: `rev-parse origin/main` + 7 × (`status --porcelain=v2 --branch`, `rev-list`) y sin `fetch` por defecto.

**`AGENT_WT_FETCH` funciona:**
```
con AGENT_WT_FETCH=1 -> primera llamada: -C /e/tylluan fetch -q origin   (y 15 en total)
sin la variable      -> OK: sin fetch por defecto (15 invocaciones)
```

**`win_rm` sobre temporal con espacios y anidamiento:**
```
creado: /tmp/t63 win_rm test (con espacios, anidado)
OK: win_rm borro el temporal
```

**Coste CPU por operación** (20 iteraciones sobre `dsh` limpio, formato `real user sys`):
```
rev-parse:      2,962 0,539 0,957
status-v1:      3,370 0,567 0,956
status-v2:      3,388 0,505 0,894
status-v2-br:   3,441 0,494 0,908
rev-list:       3,190 0,397 1,050
sed+awk parse:  3,433 1,543 2,819
wc+tr parse:    3,340 1,482 2,946
```
Es decir: `status --porcelain=v2 --branch` cuesta ≈ lo mismo que `--porcelain` v1 (~70-76 ms CPU por invocación), y `sed`+`awk` cuesta ≈ lo mismo que `wc`+`tr`. El ahorro esperado viene de los **8 procesos `git` menos** (~0,6-1,1 s de CPU a 70-140 ms/spawn) **más** el `fetch` (~1,1 s de red) que ya no se paga por defecto.

**Tiempos wall `time` alternados A→B y B→A:** no concluyentes en esta máquina ahora mismo (otros agentes compilando): las mismas 6 ejecuciones de la versión *antes* dieron `12,066–13,585 s` y las del parche `13,877–15,916 s`, pero un benchmark por bloques de la misma operación varió entre `3,7 s` y `12,7 s` entre rondas idénticas — la carga domina la señal. La reducción de procesos y de red es la parte determinista y medida; el speedup wall hay que re-medirlo con la máquina libre (ver «No verificado»).

## Impact

- **`status` ya no toca la red por defecto**: deja de pagar el `fetch` (~1,1 s y dependencia de red) en cada ejecución; para datos frescos se usa `AGENT_WT_FETCH=1` explícito (documentado en el comentario del propio script).
- **−8 procesos `git` por ejecución (23→15, −35 %)**: dos spawns menos por worktree, con la tabla idéntica verificada; en un host donde cada `git.exe` cuesta 70-280 ms (medido en T-DSH-62), el ahorro estructural es del orden de 0,6-1,1 s de CPU más el fetch.
- **Borrado portable**: `migrate` y `gc --apply` dejan de depender de `cmd //c rd` (idioma de Git Bash) y funcionan con `MSYS2_ARG_CONV_EXCL` + fallback `rm -rf`, también en shells donde `cmd //c` se rompe.
- Sin cambios de comportamiento en `reset`/`setup`; sin tocar `E:\tylluan` (solo el script en el worktree) y sin kernels.

## No verificado

- **El speedup wall no está demostrado**: con la máquina cargada (otros agentes compilando) los tiempos son ruidosos; la mejora se apoya en el conteo determinista de spawns y en el coste CPU medido por operación, no en un `time` limpio antes/después.
- **`migrate` y `gc --apply` no se ejecutaron de verdad** (borrarían `target` reales, fuera de alcance): solo se probó `win_rm` sobre un temporal con espacios/anidamiento.
- **Fallback `rm -rf` de `win_rm` no se ejerció** en este host (hay `cmd.exe`); tampoco se probó `win_rm` bajo WSL/PowerShell.
- **Equivalencia CAMBIOS v1 vs v2**: comprobada en los 7 worktrees en el momento de la medida (`v1 == v2` en todos: 0,11,0,9,1,3,0); no probada con untracked masivos no colapsados ni con submodules.
- **Caso de error de `git status`**: si falla, el parche pinta HEAD `?` y CAMBIOS 0 (antes: HEAD vacío y CAMBIOS 0); no es un cambio de tabla esperado en operación normal, pero no está probado.
- **`AGENT_WT_FETCH`** solo está documentado en el comentario del script (no en el usage de cabecera); no probado en CI ni en otros hosts.
