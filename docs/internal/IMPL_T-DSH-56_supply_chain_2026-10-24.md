# T-DSH-56 — Cadena de suministro: ignore obsoleto fuera + script de comprobación (solo informe)

- **Agente:** dsh
- **Fecha:** 2026-10-24
- **Base:** `origin/main` `fa39592` (push 130, el último que vi en `origin/main`; el push 131 seguía **en curso** cuando terminé — sus ficheros no se solapan con este diff: aquí solo se toca `deny.toml` y se añade `scripts/check_supply_chain.sh`, ninguno de ellos presente en push 131, así que el parche aplica limpio sobre ese hash).
- **Encargo (T1631):** (1) quitar de `deny.toml` el ignore obsoleto `RUSTSEC-2026-0381` (cargo deny avisa `advisory-not-detected`); (2) crear `scripts/check_supply_chain.sh` **solo-informe** (exit 0 siempre) con `cargo deny check advisories`, `npm audit --omit=dev` en `dashboard`, y comprobación de `certifi>=2026.10.19` y `pip>=26.2.2` en `.venv` y `guilds-python`, imprimiendo una tabla OK/FALLA.
- **Excluido y respetado:** no se toca `Cargo.lock`, `package.json` ni workflows de CI.
- **Entregable (diff SIN commitear):** `deny.toml` (modificado), `scripts/check_supply_chain.sh` (nuevo) + este informe.

## 1. `deny.toml`: fuera el ignore de RUSTSEC-2026-0381

Antes: `cargo deny check advisories` terminaba con `advisories ok` **pero** emitía

```
warning[advisory-not-detected]: advisory was not encountered
   ┌─ deny.toml:44:6
44 │     "RUSTSEC-2026-0381",
   │      ━━━━━━━━━━━━━━━━━ no crate matched advisory criteria
```

El `Cargo.lock` actual contiene `webpki-roots 0.26.11` **y** `1.0.10`, y aun así la base de advisories que usa `cargo-deny 0.19.9` ya no casa `RUSTSEC-2026-0381` con ninguna entrada: el ignore sobra y el comentario (que describía el estado del lock de entonces, con ureq 2.12.1 → hf-hub → fastembed) ya no describe la realidad. Se elimina la entrada **y su bloque de comentario** (7 líneas) en `deny.toml`; no se toca ningún otro ignore ni el resto de secciones (licenses/bans/sources).

Después (mismo `Cargo.lock`, misma versión de cargo-deny): **exit 0, `advisories ok`, cero warnings**.

## 2. `scripts/check_supply_chain.sh` (solo informe, exit 0 siempre)

Comprueba y pinta:

| fila | qué hace |
|---|---|
| `cargo deny advisories` | `cargo deny check advisories`; exige exit 0 **y** texto `advisories ok`, y falla si aparece `advisory-not-detected` (así el script vigila este mismo caso en el futuro) |
| `npm audit (dashboard, prod)` | si existe `dashboard/package-lock.json`: `npm audit --omit=dev --json`; OK solo con `total == 0` |
| `.venv (certifi/pip)` | `certifi.__version__ >= 2026.10.19` y `pip.__version__ >= 26.2.2` |
| `guilds-python (<plataforma>)` | lo mismo en el runtime vendorizado de **la plataforma actual** (`windows-x86_64` en Windows/Git Bash, `linux-x86_64` en Linux) |

Detalles de implementación:

- Comprobación de versiones con `sort -V` (`version_ge`), sin dependencias externas.
- Título y detalle por fila; resumen final `RESUMEN: todo OK` o número de FALLAs. **Siempre `exit 0`** (no bloquea CI, por diseño).
- `ROOT` = raíz del repo calculada desde la ruta del script, con override `TYLLUAN_REPO_ROOT` para poder probarlo contra otro checkout sin escribir nada (`TYLLUAN_REPO_ROOT=/e/tylluan bash scripts/check_supply_chain.sh`).
- Si falta una herramienta o un artefacto (p. ej. `package-lock.json`, `.venv` o `guilds-python` en un worktree recién clonado), la fila sale `FALLA` con el motivo, en vez de romper el script.

## 3. Verificación (evidencia real de esta sesión)

| comprobación | resultado |
|---|---|
| `cargo deny check advisories` en el worktree con el `deny.toml` nuevo (lock de push 130) | **exit 0, `advisories ok`, sin warnings** |
| `bash scripts/check_supply_chain.sh` en el worktree (Git Bash) | tabla correcta; las 3 filas de entornos salen FALLA **porque `.venv`, `guilds-python` y `dashboard/package-lock.json` son artefactos gitignored ausentes en el worktree** (no hay repo de trabajo con ellos) — el `cargo deny` sale OK |
| `bash` del sistema (WSL) | tabla correcta, pero cargo-deny/npm no existen en WSL → FALLA (honesto); el shell del equipo (Git Bash) es el que encuentra las herramientas |
| misma raíz contra `E:\tylluan` (solo lectura, `TYLLUAN_REPO_ROOT`) | `npm audit` **OK (0 vulnerabilidades)**, `.venv` **OK (certifi 2026.10.23, pip 26.2.2)**, `guilds-python/windows-x86_64` **OK (2026.10.23 / 26.2.2)**; la fila `cargo deny` sale FALLA **solo por el `deny.toml` antiguo todavía en ese checkout** (con el ignore 0381), que es justo lo que este diff elimina |
| raíz temporal equivalente (junctions a `.venv`, `guilds-python` reales + `package-lock.json` real + `deny.toml` nuevo + `Cargo.lock` del repo) | **`RESUMEN: todo OK`**: las 4 filas OK — criterio «hoy todo OK» verificado end-to-end sin tocar `E:\tylluan` |
| diff | `deny.toml` modificado + `scripts/check_supply_chain.sh` nuevo + este informe; sin commitear; sin tocar `Cargo.lock`/`package.json`/CI |

## Impact

- **Ruido de auditoría eliminado**: `cargo deny` deja de avisar de un ignore que ya no oculta nada; el gate de advisories queda limpio y cualquier `advisory-not-detected` futuro vuelve a significar algo (el script lo convierte en FALLA).
- **Una orden para fotografiar la cadena de suministro**: `bash scripts/check_supply_chain.sh` resume Rust (advisories), dashboard (producción) y los dos entornos Python con umbrales explícitos; sirve antes de un push o en un ciclo de revisión.
- **Cero riesgo operativo**: no se cambia ninguna dependencia ni lockfile, el script es solo-informe y no entra en CI.

## No verificado

- **El push 131 no estaba en `origin/main` al terminar** (el real checkout ya iba por `d6b53cd` con commits locales de gates): el diff está calculado sobre `fa39592`; al no tocar ningún fichero de ese push, aplica limpio, pero no lo probé sobre el árbol exacto de `d6b53cd`/push 131.
- **Por qué el advisory 0381 ya no casa**: lo verifiqué por el comportamiento de `cargo-deny` (`no crate matched advisory criteria` con `webpki-roots 0.26.11` y `1.0.10` en el lock), no inspeccionando la entrada concreta de la base de advisories (no localicé el directorio de la DB en esta máquina); la causa exacta (advisory retirada o rango afectado distinto) no está comprobada.
- **`npm audit` en worktrees nuevos**: la fila sale FALLA si no hay `dashboard/package-lock.json` (gitignored). Solo la validé en el checkout real, que sí lo tiene; no probé `pnpm audit` como alternativa.
- **`guilds-python` de la otra plataforma**: el script comprueba solo el runtime de la plataforma actual (decisión para no dar falsos FALLA desde Windows). El `linux-x86_64` vendorizado localmente tiene `certifi 2026.7.22` y `pip 26.2.1` (por debajo de los mínimos) — no es ejecutable en este host y en CI se reconstruye con `scripts/build_guilds_python.sh` desde `guilds/requirements.txt`, que ya pide `certifi>=2026.10.19`; si se quiere vigilar también, habría que leer los `*.dist-info` de la otra plataforma (no incluido).
- **Cargo-deny necesita su base de advisories**: la primera ejecución en una máquina sin DB puede requerir red; offline daría FALLA (no probado).
- **`sort -V`**: requiere coreutils GNU (Git Bash/Linux OK); en BusyBox podría no existir (no probado).
- **CI**: no se añadió el script a ningún workflow (excluido por el encargo), así que no hay verificación automática continua de esta tabla.

## Verificacion del TL (2026-10-24)
Copie `deny.toml` y `scripts/check_supply_chain.sh`: `bash scripts/check_supply_chain.sh` imprime la tabla con las 4 filas OK (cargo deny advisories, npm audit del dashboard, .venv y guilds-python con certifi 2026.10.23 y pip 26.2.2) y `RESUMEN: todo OK`. Con el ignore RUSTSEC-2026-0381 quitado, `cargo deny check advisories` = advisories ok.
