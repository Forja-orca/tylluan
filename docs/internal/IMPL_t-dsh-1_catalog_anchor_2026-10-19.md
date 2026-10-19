# T-DSH-1 — `builtin_catalog` sin depender de que el exe esté dentro del repo

- **Agente:** dsh (DeepSeek Harness) · **Fecha:** 2026-10-19
- **Asignación:** TL (claude-code) en Coloquio, turno T1304, sobre el diagnóstico del piloto
  (`IMPL_piloto_dsh_2026-10-19.md`, aceptado como diagnóstico).
- **Sincronización:** `git fetch origin` hecho 2026-10-19; `origin/main` = `60fc4f9`
  (contiene ya el fix oauth de Buffy `1a2b6a0` y la accesibilidad de Mimo `60fc4f9`).
  El worktree se actualizó con `merge --ff-only` a `60fc4f9` **antes** del gate final; el
  diff de este informe va sobre esa base, **sin commitear** (regla: solo Claude Code
  comitea/pushea). Los gates de las primeras corridas corrieron sobre `50b880c`; tras el
  ff (que añade 5 líneas a `transport/http/oauth.rs`, mismo crate) se **re-ejecutaron los
  tres gates** sobre el árbol final — citas abajo, log `t-dsh-1_verification_run2.log`.

## 1. Problema (resumen del diagnóstico del piloto, confirmado A/B)

`builtin_catalog()` (en `crates/tylluan-kernel/src/router/catalog.rs`) resolvía `guilds/`
así: CWD, su padre, y en fallback subía hasta 6 niveles desde `current_exe()` — asumiendo
que el ejecutable vive dentro del repo. Con `CARGO_TARGET_DIR` externo el exe queda fuera,
el catálogo resolvía **0 guilds** y 41 tests fallaban en cascada (mismo exe de test: CWD
en la raíz del repo → 19/19 verde; CWD en el crate como pone cargo → 9 fallidos). Los
41 no eran deuda real: era el fixador de rutas.

## 2. Fix (exacto, en `router/catalog.rs`)

- Nuevo `fn guild_root_candidates(cwd, exe_dir, env_root) -> Vec<PathBuf>`: lista **pura**
  de candidatos (sin tocar el disco) en este orden:
  1. `TYLLUAN_REPO_ROOT` si está definido (override explícito, se sonda el primero).
  2. CWD y su padre — **orden de producción intacto** (era el orden previo; `cargo test`
     pone el CWD en el directorio del crate, el sondéo del padre lo cubre).
  3. Directorio del exe subiendo 6 niveles — igual que antes (binarios construidos dentro
     del repo).
  4. **Ancla de compilación** `env!("CARGO_MANIFEST_DIR")` + `../..` (raíz del repo vista
     desde `crates/tylluan-kernel`). Esto es lo que ya usaban los tests de este módulo para
     fijar la raíz; ahora lo usa el propio resolver.
- `builtin_catalog()` resuelve: primer candidato con `guilds/` real; si no hay ninguno
  (binario instalado sin repo visible), conserva el comportamiento previo: escanea el último
  candidato → catálogo vacío como antes.
- Un `TYLLUAN_REPO_ROOT` inválido (sin `guilds/` debajo) **no secuestra** la resolución: se
  sondea primero pero el filtro `is_dir` lo salta (fijado con test).
- Semántica `OnceLock` y orden de catálogo inalteradas. Sin cambios fuera de `catalog.rs`
  en código.

## 3. Tests nuevos (fijan la resolución con rutas simuladas, como pidió la asignación)

1. `test_guild_root_candidates_order_env_first_anchor_last` — orden exacto de candidatos
   con rutas simuladas (`/sim/...`), el ancla es la última y **debe resolver** dentro del
   checkout real.
2. `test_resolve_guilds_root_external_exe_falls_back_to_anchor` — ni CWD ni exe dentro de
   un repo (el caso del piloto: exe en target externo): la resolución cae al ancla; env
   root válido gana; env root inválido no secuestra. (tempdirs; el directorio del exe
   simulado no existe en disco — no hace falta.)

Kernel lib: 1057 → **1059** tests.

## 4. Criterios de aceptación (comando + salida real)

Rutina: Git Bash/cargo con `RUSTUP_TOOLCHAIN=stable`, worktree `E:\tylluan-wt-dsh`.
Log completo del gate final: `t-dsh-1_verification_run2.log` (post-ff, sobre `60fc4f9`).

- **Criterio TL — `cargo test -p tylluan-kernel --lib` con `CARGO_TARGET_DIR` externo ya
  NO da 41 fallos:**
  `CARGO_TARGET_DIR=E:\tylluan-targets\dsh cargo test -p tylluan-kernel --lib`
  → `test result: ok. 1059 passed; 0 failed; 3 ignored` · `EXIT_GATE2_EXTERNO=0`
  (antes del fix, en este mismo directorio externo: `1016 passed; 41 failed`).
- Gate interno (same command, target del worktree): `test result: ok. 1059 passed; 0 failed;
  3 ignored` · `EXIT_GATE1_INTERNO=0`.
- **Clippy:** `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` →
  `EXIT_GATE3_CLIPPY=0`.
- **Cifras:** `scripts/check_test_count.sh --fix` reescribió README/AGENTS a 1146
  (`kernel=1059 link=75 fsrs=12`); verificación posterior en verde. STATUS.md actualizado a
  mano (líneas de cabecera y canónica). Badge: `scripts/generate_architecture_svg.py` →
  `1,146 TESTS GREEN`, `docs/assets/architecture.svg` regenerado con python
  (`Generated docs\assets\architecture.svg`).
- `scripts/verify.sh --docs` → en verde tras el ff (la cita de líneas 1000 y 1069 del
  informe del piloto se reformuló: el gate de realidad leía «1069» como puerto;
  `check_head_sync` ahora cita un único HEAD).

## Impact

- **Producción:** el kernel de José y cualquier despliegue conservan el orden de búsqueda
  previo (puntos 2-3 del nuevo orden eran exactamente el código anterior). Donde antes el
  fallback por exe funcionaba (builds dentro del repo), sigue funcionando y gana posición
  sobre el ancla. Con `TYLLUAN_REPO_ROOT` definido y válido el catálogo pasa a leerse de
  ahí — es un override **explícito**, documentado en el doc-comment de la función.
- **No afecta a procesos en marcha:** `builtin_catalog` se calcula perezosamente una vez
  por proceso del kernel; el kernel en ejecución (`:47004`, build `0ac9dfb`)
  no cambia su comportamiento hasta que José reconstruya/reinicie — decisión suya, no se
  tocó el kernel (regla dura).
- **Agents/flota:** desaparece el falso negativo de los 41 fallos con target externo
  (práctica documentada de aislamiento de builds). CI no se ve afectada (target dentro del
  repo iba verde antes y va igual ahora; solo +2 tests).
- **Riesgo bajo residual:** si alguien define `TYLLUAN_REPO_ROOT` a un repo distinto con
  `guilds/` dentro, el catálogo se lee de ahí (es el comportamiento pedido); queda fijado
  por test para que no cambie de orden en el futuro.

## No verificado (honesto)

- Efecto en el kernel en vivo: prohibido tocarlo; la deriva que imprime `verify.sh`
  (kernel 21+ commits detrás) es decisión de José.
- `docs/assets/architecture.png` sigue mostrando 1,144: el script del badge solo genera
  SVG; no hay rasterizador en el pipeline de este repo y nadie autorizó instalar uno.
  Queda pendiente para quien tenga el flujo de render (Antigravity renderizó el PNG en
  `89f589d`).
- Ejecución real de `TYLLUAN_REPO_ROOT` en un proceso kernel (verificado a nivel de la
  función de resolución, no de proceso vivo — mismo motivo que el punto 1).
- `cargo clippy` con target externo y el `verify.sh --rust` completo post-ff: se citan los
  tres gates individuales; el pre-push de Claude Code al integrar hará la corrida completa.

## Diff entregado (untracked: este informe, el del piloto y los 3 logs de gates)

```
 M AGENTS.md                                   |   2 +-
 M README.md                                   |   2 +-
 M STATUS.md                                   |   6 +-
 M crates/tylluan-kernel/src/router/catalog.rs | 204 ++++++++++++++++++++++----
 M docs/assets/architecture.svg                |   2 +-
 M scripts/generate_architecture_svg.py        |   2 +-
?? docs/internal/IMPL_piloto_dsh_2026-10-19.md
?? docs/internal/IMPL_t-dsh-1_catalog_anchor_2026-10-19.md
```

(Base: `60fc4f9` == origin/main tras `git fetch` del 2026-10-19; nada comiteado ni pusheado.)
