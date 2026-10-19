# IMPL — `identity.key`: escritura atómica + aislamiento de data-dir en los claims de CI (Deep, 2026-10-18)

- **Base:** `origin/main` `d44766a` · worktree `E:\tylluan-wt-deep` (detached en `a220fb8`) · **diff sin commitear** (solo el TL commitea).
- **Asignación:** Coloquio #general (TL): **(a)** `TYLLUAN_DATA_DIR="$CONFIG_DIR"` en cada script de `scripts/ci/claims/*.sh` que arranque kernel; **(b)** escritura atómica de `identity.key` + test de regresión con N hilos sobre `load_or_create`. Reglas explícitas del TL: **no** cambiar el formato del fichero (68 B, MAGIC `TLID`), **no** auto-heal de claves corruptas (el hard error se mantiene), `clippy -D warnings`, tests de `tylluan-link`, `check_test_count`, sección `## Impact`.
- **Origen:** flake de CI run `39174682288` sobre `d44766a` — 3 claims ❌, entre ellas `p2p-rejects-unapproved-peer` con `identity.key corrupted — public key does not match secret key`; el **rerun del MISMO commit quedó 100 % verde** → carrera transitoria confirmada, no reproducida.

## Causa raíz (por qué existe este cambio)

1. **Escritura no atómica.** Único write site de producción: `crates/tylluan-link/src/identity.rs` `generate()` — `fs::write(path, &buf)` (antes, línea 76). Un `fs::write` truncado a medias deja un fichero de ≠68 B (o con pares secret/public desalineados si dos escritores se intercalan). No hay ningún otro write site en el kernel (grep).
2. **Sin recuperación.** `read_existing` **nunca** regenera ante corrupción (política deliberada: borrar la clave rompe a los peers de federación) → una escritura interrumpida es un **hard error permanente** hasta que intervenga un humano. Y la identidad se escribe **muy pronto** en `main.rs`: antes de `TylluanServer::new`, antes del log `listening on` y antes de la carga de modelos (60–120 s) → hay una ventana real en la que un shutdown temprano (p. ej. EOF de stdin, como en `write_gate_rejects.sh`) corta justo ese write.
3. **Amplificador: data-dir compartido.** Los 3 scripts de claims solo overridan `--config` con un `mktemp -d`; `run_claims.py` los lanza **secuencialmente** con `cwd=repo_root` y nadie exporta `TYLLUAN_DATA_DIR`. El kernel calcula `data_dir = config.memory.db_path.parent()` con el default `./data/tylluan.db` → **todos los claims comparten `<repo>/data/identity.key`** (y la db). Un claim que corrompe envenena al siguiente de la misma run; de ahí que un flake "de uno" tumbe dos.

## (a) Aislamiento del data-dir en los claims de CI

`scripts/ci/claims/host_devmode_force_corrects.sh`, `write_gate_rejects.sh`, `p2p_rejects_unapproved.sh` — `export TYLLUAN_DATA_DIR="$CONFIG_DIR"` (+ bloque de comentario explicativo) inmediatamente después de `CONFIG_DIR=$(mktemp -d)` (13 líneas por script, solo comentario + export). Cada claim pasa a usar su propio directorio temporal para identidad **y** sqlite: sin estado compartido entre claims, sin veneno cruzado. No toca producción.

## (b) Escritura atómica + convergencia

En `crates/tylluan-link/src/identity.rs`:

- `load_or_create` toma un `IDENTITY_LOCK` (`static Mutex<()>`, recuperando el guard en caso de poison — la sección es filesystem puro, sin estado compartido) y delega en `read_existing` / `generate`. Esto serializa la creación **dentro del proceso**.
- `read_existing` extraído: **mismo cuerpo, mismas dos condiciones de error**, misma política (jamás regenera). Única diferencia: es reutilizable.
- `write_atomic(path, buf)`: temp en el **mismo directorio** (`identity.key.tmp.<pid>.<contador_atómico>`, para que `rename` sea same-filesystem), `0600` **en la creación** del temp en Unix (`OpenOptionsExt::mode`), `write_all` + `sync_all()` (fsync) antes de `fs::rename`, y refuerzo de permisos `0600` post-rename en Unix (comportamiento previo conservado). Sin pre-borrado del temp: el nombre es único por escritor y un temp huérfano de un proceso muerto es ignorable (nunca se lee).
- **Convergencia:** tras el rename, `generate` **re-lee el fichero** y devuelve lo que hay en disco. Si dos procesos primerizos corren a la vez, ambos acaban devolviendo el ganador en vez de que el perdedor firme con una clave que ya no existe en disco.

**Hallazgo del propio test (vale la pena registrarlo):** mi primera versión nombraba el temp solo con el PID. El test de N hilos la tumbó — 16 hilos del **mismo** proceso comparten PID → mismo temp → un hilo hace `rename` y otro falla con ENOENT (`identity.rs:363` del primer intento). Corregido con contador atómico **+** el mutex. Es exactamente lo que el test debía cazar; sin él habría entregado un fix con una carrera in-process intacta.

## Test de regresión

`identity::tests::test_concurrent_load_or_create_converges_on_one_identity`: 16 hilos sincronizados con `Barrier` sobre el **mismo path inexistente**; asserts: (1) las 16 claves son idénticas, (2) el fichero final es de 68 B con MAGIC `TLID`, (3) la clave en disco coincide con la que devolvieron los 16, (4) no queda ningún `*.tmp.*` huérfano. Falla con el código anterior (sin atómico/convergencia) y con mi primera versión.

## Verificado (comando → resultado)

| Comando | Resultado |
|---|---|
| `cargo test -p tylluan-link --all-targets` | **75 passed**, 0 failed (era 74; +1) |
| `cargo clippy -p tylluan-link --all-targets -- -D warnings` | **EXIT=0**, limpio |
| `cargo test -p tylluan-kernel --test mesh_audit` | **14 passed** (identidad intacta end-to-end) |
| `scripts/check_test_count.sh` | **EXIT=0** — el `exit 0` solo se alcanza tras validar README **y** AGENTS (el `exit 1` de mismatch está en la línea 172) |
| `git diff --stat` | 7 ficheros, **+212 / −44** |

Recuentos al día: `README.md` (1144), `AGENTS.md` (1057 + **75** + 12 = **1144**, y la línea de validación "Esperado: 75 link tests"), `STATUS.md:10` y `STATUS.md:50` (1143→1144, link 74→75).

## NO verificado / límites (declarado, no escondido)

- **Atomicidad frente a muerte del proceso:** no hay test con `kill` (no es unit-testeable de forma fiable aquí). La propiedad "un lector nunca ve un fichero parcial" se garantiza **por construcción** (temp + fsync + rename same-dir) y revisión manual, no por test.
- **Convergencia entre procesos = "el último rename gana, todos re-leen".** En teoría un proceso A puede leer *antes* del rename de B y quedarse con la clave que B acaba de sustituir. Sin un lock **inter-proceso** no hay garantía dura. No implementé lockfile a propósito: un lock huérfano tras un crash **colgaría el arranque** del kernel, y el coste/beneficio de esa complejidad es una decisión del TL, no mía. La ventana real (dos kernels primerizos sobre el mismo data-dir) es de milisegundos en arranque.
- No corrí `scripts/verify.sh` (≈10 min). No arranqué kernels (puerto `47004` y `tylluan.toml` intactos).
- `STATUS.md:50`: actualicé el recuento pero **no** la cita de HEAD (`d44766a`) ni la fecha — eso lo re-sincroniza el TL al commitear, como en todo el ciclo.
- **Nada commiteado** (regla: solo Claude Code commitea).

## Impact

- **Código de producción tocado:** `crates/tylluan-link/src/identity.rs` — identidad Ed25519 del mesh/federación. Sin cambio de formato (68 B, MAGIC `TLID`), sin cambio de comportamiento observable en el camino de un solo escritor (escribe y vuelve a leer exactamente esos bytes), sin auto-heal (el hard error ante corrupción sigue siendo un hard error).
- **Seguridad:** no se debilita; se refuerza. Los permisos `0600` se aplican ahora **también en la creación del temp**, no solo con un `set_permissions` post-hoc sobre el fichero final (que dejaba una ventana con el umask por defecto). Ganancia neta.
- **Dependencias:** ninguna nueva (solo `std`: `Mutex`, `AtomicU64`, `OpenOptionsExt`/`PermissionsExt` ya usados bajo `cfg(unix)`).
- **CI:** `scripts/ci/claims/*.sh` (solo CI: no afecta a producción ni a usuarios). Elimina el veneno cruzado de identidad/db entre claims.
- **Consumidores afectados:** ninguno roto — 75 link + 1057 kernel + 12 fsrs verdes, clippy limpio, `mesh_audit` (que ejercita identidad real) 14/14.
- **No depredación (ADR-013):** no toco ficheros ajenos ni WIP de otro agente; trabajo en mi worktree; entrego diff sin commitear + este informe al TL.
- **Riesgo:** bajo en producción (una escritura, en arranque). Punto abierto para el TL: si se quiere arbitraje inter-proceso real (lockfile con detección de stale), no implementado a propósito.
