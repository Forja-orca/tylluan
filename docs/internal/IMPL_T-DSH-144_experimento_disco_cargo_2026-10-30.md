# T-DSH-144 — Experimento de disco: tamaño/tiempo de `cargo build --tests` con perfil dev reducido y target compartido

- Agente: dsh
- Fecha: 2026-10-30
- Base: `origin/main = 3890ec6` (push209). Sin código: solo medición. **No se tocó `E:\tylluan` ni Strata-main**; todo en worktrees/targets propios de prueba en `D:`.
- Encargo (T2171): en copia de prueba propia, medir (A) línea base dev actual, (B) `[profile.dev] debug="line-tables-only" incremental=false` por entorno sin editar `Cargo.toml`, (C) segundo worktree contra el mismo target que B. Tabla, comandos, `## Impact`, `## No verificado`. Solo medir; no borrar fuera de carpetas propias.

## Montaje

- Toolchain: cargo/rustc **1.99.1**.
- Worktrees de prueba (creados desde `3890ec6`, detached): `D:\wt-dsh-144a` y `D:\wt-dsh-144b`.
- Targets propios: `D:\cargo-target\dsh-144a` (A) y `D:\cargo-target\dsh-144b` (B y C). Nada en `E:`.
- Tamaños medidos con recorrido recursivo de ficheros (`Get-ChildItem -Recurse | Measure-Object Length`) sobre el target completo y sobre `debug\incremental`. (El `robocopy /L` de la tarea no devolvió salida parseable en esta shell; se usó el equivalente exacto en bytes.)
- **`-j 8` en los tres builds**: con el paralelismo por defecto (56 hilos) el build A abortó por agotar el *commit* de Windows (`os error 1455: El archivo de paginación es demasiado pequeño`, mmap de `libtylluan_kernel*.rlib`), no por disco. Para que A/B/C sean comparables se fijó `-j 8`; se anota como restricción del entorno.
- Comandos exactos:
  ```
  # A (dev actual)
  $env:CARGO_TARGET_DIR="D:\cargo-target\dsh-144a"; Remove-Item Env:CARGO_PROFILE_DEV_DEBUG,Env:CARGO_INCREMENTAL
  cd D:\wt-dsh-144a; cargo build --tests -j 8 -p tylluan-kernel

  # B (line-tables-only, sin incremental; sin editar Cargo.toml)
  $env:CARGO_TARGET_DIR="D:\cargo-target\dsh-144b"
  $env:CARGO_PROFILE_DEV_DEBUG="line-tables-only"; $env:CARGO_INCREMENTAL="0"
  cd D:\wt-dsh-144a; cargo build --tests -j 8 -p tylluan-kernel

  # C (segundo worktree contra el MISMO target de B, mismo entorno)
  cd D:\wt-dsh-144b; cargo build --tests -j 8 -p tylluan-kernel
  ```

## Resultados

| | A · dev actual | B · line-tables-only + sin incremental | C · 2º worktree, target de B |
|---|---|---|---|
| Worktree | `D:\wt-dsh-144a` | `D:\wt-dsh-144a` | `D:\wt-dsh-144b` |
| `CARGO_TARGET_DIR` | `dsh-144a` | `dsh-144b` | `dsh-144b` (compartido con B) |
| `debug` | `2` (completo) | `line-tables-only` | `line-tables-only` |
| `incremental` | `true` | `false` | `false` |
| Tamaño total target | **48,58 GB** | **16,67 GB** | 16,69 GB |
| `debug\incremental` | **8,91 GB** | **0 GB** | 0 GB |
| Tiempo de build | **919,3 s** (15,3 min) | **648,0 s** (10,8 min) | 439,4 s (7,3 min) |
| Crates recompilados | — (build completo) | — (build completo) | **1** (solo `tylluan-kernel` + sus test bins; deps reutilizadas) |
| GB añadidos al target | — | — | **+0,02 GB (+0,1 %)** |

Derivados: B ahorra **31,91 GB (−65,7 %)** frente a A y **271,3 s (−29,5 %)** de tiempo; C sobre el target de B añade **20 MB** y recompila **solo el crate del workspace** (~7,3 min), no las dependencias.

## Impact

- Pasar el perfil dev a `debug="line-tables-only"` + `incremental=false` **solo por entorno** (`CARGO_PROFILE_DEV_DEBUG`, `CARGO_INCREMENTAL`), sin tocar `Cargo.toml`, reduce el target de `cargo build --tests -p tylluan-kernel` de **48,6 GB a 16,7 GB (−65,7 %)** y el tiempo de build ~30 % en esta máquina. Es la palanca más directa para el problema de disco de los agentes.
- El `incremental` es el mayor consumidor aislado (**8,9 GB** en A) y desaparece con `incremental=false`; el resto del ahorro viene de no emitir debug info completa en los rlibs.
- Un **segundo worktree contra el mismo target** reutiliza las dependencias: añade solo **~20 MB** y recompila únicamente `tylluan-kernel` (7,3 min). Compartir target entre worktrees del mismo commit es viable y no multiplica los GB.
- Medido en `D:` (359,3 GB libres tras el experimento); no se borró nada fuera de las carpetas de prueba.

## No verificado

- **`-j 8` fue forzado** por el límite de *commit*/page file de Windows (con el paralelismo por defecto A abortó con `os error 1455`); los tiempos absolutos serían distintos con otro `-j`, aunque la comparación A/B/C es consistente entre sí.
- Una sola corrida por variante; no hay intervalo de confianza. La máquina tenía otros procesos/agentes en marcha no controlados.
- Solo se midió `cargo build --tests -p tylluan-kernel` en perfil **dev**; no se midió `cargo test` (ejecución), release, ni otros crates/workspaces.
- Los tamaños incluyen el target completo (deps + build scripts + test bins), no se desglosa por crate; el ahorro de B se atribuye a `incremental` (8,9 GB medidos) y a la falta de debug completo, sin desglose fino por artefacto.
- `robocopy /L` no dio salida parseable en esta shell; se usó recorrido recursivo de ficheros (mismos bytes, algo más lento).
- C se midió con el worktree `D:\wt-dsh-144b` en el **mismo commit** `3890ec6`; no se probó con commits distintos (ahí sí habría recompilación real de deps).
- No se ha borrado nada de `E:` ni de otros agentes; los targets de prueba `D:\cargo-target\dsh-144a` (48,6 GB) y `dsh-144b` (16,7 GB) quedan en `D:` a la espera de decisión (son míos y borrables con OK).
