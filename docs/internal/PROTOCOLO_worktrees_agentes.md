# Protocolo de worktrees de agentes: baratos y desechables

Decidido con José el 23-oct-2026 tras medir que las carpetas de compilación de los worktrees sumaban unos 450 GB y que cinco agentes compilando a la vez saturaban la CPU. Se mantienen los worktrees (aíslan a cada agente y evitan las colisiones de un checkout compartido), pero cambian dos cosas: la compilación vive fuera del worktree y el worktree se reinicia desde `origin/main` en lugar de arrastrar trabajo viejo.

## Las cinco reglas

1. **Un worktree por agente, siempre en su sitio:** `E:\tylluan-wt-<agente>` (Hermes: `D:\hermes-projects\tylluan-wt-hermes`). Trabajas solo ahí. Nunca en `E:\tylluan`, que es el checkout principal y el de producción.
2. **La compilación va fuera del worktree:** `E:\cargo-target\<agente>`. Lo configura `agent_wt.sh setup`; no hace falta que lo toques. Así borrar o rehacer un worktree no cuesta una recompilación completa de dependencias, y limpiar es mirar una sola carpeta.
3. **Empieza cada tarea con un reinicio, no con una carpeta sucia:** `bash /e/tylluan/scripts/agent_wt.sh reset <agente>`. Deja el worktree en el `origin/main` actual. Si había cambios sin guardar, los guarda antes (parche, copia de los ficheros nuevos y stash) en `E:\tylluan-patches\<agente>\_saved\`; **no pierde nada**.
4. **La entrega es un diff y un informe, nunca un commit:** el parche en `E:\tylluan-patches\<agente>\` (o el diff sin commitear en tu worktree) y un informe `IMPL_*` con `## Impact` y «No verificado». Solo el TL aplica, comitea y empuja.
5. **Una tarea, un reinicio, una entrega.** Cuando el TL acepta y empuja tu entrega, haz `reset` antes de la tarea siguiente. No acumules.

## Órdenes de `scripts/agent_wt.sh`

| Orden | Qué hace |
|-------|----------|
| `status` | tabla con el HEAD de cada worktree, cuántos cambios sin guardar tiene, cuánto se ha quedado atrás de `origin/main` y dónde compila |
| `setup <agente>` | escribe `.cargo/config.toml` en el worktree (compilación compartida y caché de modelos común); está excluido de git |
| `reset <agente>` | guarda lo que haya, trae `origin/main`, deja el worktree limpio y lo configura |
| `migrate <agente>` | borra la carpeta `target` antigua que haya dentro del worktree (solo si no hay cargo ni rustc en marcha) |
| `gc [--days N] [--apply]` | lista, o borra con `--apply`, las carpetas de `E:\cargo-target` sin uso en N días (por defecto 7) |

## Qué no hace

- No toca `E:\tylluan` ni su carpeta `target` (producción).
- No borra nada si hay `cargo` o `rustc` en marcha.
- No comitea, no empuja, no toca tokens ni configuración del kernel.

## Cómo se comprobó

`reset` se probó con un worktree desechable con un fichero modificado y otro nuevo: guardó el parche y la copia del fichero nuevo, hizo el stash, dejó el worktree limpio en `origin/main` y escribió la configuración; `cargo metadata` confirmó que la carpeta de compilación pasa a `E:\cargo-target\<agente>`. La primera versión usaba `git add -N` y el stash fallaba: lo encontró la prueba y se corrigió antes de darlo a nadie. **No verificado:** el efecto sobre el tiempo de compilación con varios agentes compilando a la vez, y el tamaño final de cada carpeta de compilación.

## Si algo falla

- `reset` dice «no toco nada»: no ha perdido nada; copia el mensaje al informe y avisa al TL.
- `cargo` se queja de un bloqueo de la carpeta de compilación: otro proceso tuyo la está usando; espera o cierra el otro.
- Dudas: escribe `DECISION PENDIENTE: ...` en el informe y sigue con otro paso.
