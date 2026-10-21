# B-14 — Verificación del quickstart en clon limpio (PARCIAL)

**Autor:** Buffy (worktree `E:\tylluan-wt-buffy`, HEAD `514058e`)
**Fecha:** 2026-10-22
**Clon limpio:** `scratchpad/tylluan-clean` en `514058e` (`git clone --local --no-hardlinks`)
**Objetivo:** seguir `docs/getting-started/QUICKSTART.md` paso a paso, sin saltarse ninguno.

---

## Estado: PARCIAL — pasos estáticos verificados, arranque NO ejecutado

**Motivo del corte, y es una decisión, no un olvido:** `AGENTS.md` prohíbe iniciar procesos vía
Bash en este entorno Windows («Para arrancar procesos: proporcionar el comando al usuario, no
ejecutarlo vía Bash»). Los pasos 3, 5, 6 y 8 del quickstart **arrancan el kernel**, así que no
los ejecuté. Lo que sí se puede verificar sin arrancar nada, está verificado abajo.

---

## Tabla de verificación (lo ejecutado)

| # | Paso del quickstart | Comando / método | Resultado real | ¿Funciona? |
|---|---|---|---|---|
| 1 | §1 «Rust: 1.99.1 (pinned in `rust-toolchain.toml`)» | `cat rust-toolchain.toml` | `channel = "1.99.1"` | ✅ |
| 2 | §2B `pip install -r guilds/requirements.txt` | `ls -la guilds/requirements.txt` | existe, 915 bytes | ✅ |
| 3 | §7 «45 default routable guilds» | criterio del propio gate (`check_docs_reality.sh:419-430`): `.py` con `FastMCP(`, sin `_x`/`test_`, menos `EXCLUDED_GUILDS` | 46 − 1 = **45** | ✅ |
| 4 | §4 perfiles `portable` / `clinic` / `server` | `grep` en `tylluan-cli/src/main.rs` | los tres existen (`main.rs:28-30`) | ✅ |
| 5 | §2B `cargo build --release -p tylluan-kernel` | — | no ejecutado | ⬜ |
| 6 | §3 arrancar el kernel (`tylluan-cli start` / binario) | — | no ejecutado (prohibido vía Bash) | ⬜ |
| 7 | §5 conexión MCP (`/sse`, token en `.tylluan-token`) | — | no ejecutado | ⬜ |
| 8 | §8 `tylluan_recall`, `/health`, `silva/stats`, `doctor`, `audit` | — | no ejecutado | ⬜ |

**Tiempo total hasta el primer `tylluan_recall`: no medido** — no se arrancó el kernel.

---

## Un error de medición MÍO, corregido

Mi primera pasada contó «37 `.py` en `guilds/`» con un globo `guilds/**/*.py` de Bash. Ese número
es **falso**: con `globstar` desactivado el patrón no recursa. Remediado con `find guilds -name '*.py'`
→ **73** ficheros `.py` en total. Y la cifra que importa —la de *routable*— no es el recuento de
ficheros sino el del gate: **45**. Conviene dejarlo anotado porque es exactamente el tipo de
«cifra que el gate vigila» que ha causado incidentes previos en este repo (el propio
`check_docs_reality.sh:8` documenta un «49 guilds» publicado cuando el catálogo real tenía 46).

---

## Cómo terminar B-14 (para quien pueda arrancar procesos)

En el clon limpio ya creado, con puerto propio y `DATA_DIR` propio — **nunca** el de producción:

```bash
cd scratchpad/tylluan-clean
cargo build --release -p tylluan-kernel
TYLLUAN_DATA_DIR="$PWD/data-test" ./target/release/tylluan-nexus --port 47099 &
# §5: leer el token generado
cat .tylluan-token
# §8: verificar
curl -s http://127.0.0.1:<puerto-de-prueba>/health
```

El paso que falta de verdad es el **tiempo hasta el primer `tylluan_recall`**, que requiere un
cliente MCP conectado por `/sse`. Sin eso, B-14 no está cerrado.

---

## Impact

**Documental, sin cambios de código.** `src/` intacto. Ningún kernel arrancado. Ningún proceso
iniciado. Trabajo realizado sobre un clon limpio separado, no sobre el checkout compartido.

**Verificado:** cuatro afirmaciones fácticas del quickstart son correctas contra `514058e`
(toolchain 1.99.1, `guilds/requirements.txt`, 45 guilds routables por el criterio del propio gate,
y los tres perfiles de instalación).

**No verificado:** los pasos que exigen arrancar el kernel (build release, arranque, conexión MCP
por `/sse`, y el primer `tylluan_recall`). El tiempo de arranque hasta el primer recall **no se
midió**. B-14 queda **parcial**, no cerrado.
