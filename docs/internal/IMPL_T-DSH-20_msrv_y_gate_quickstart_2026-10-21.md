# T-DSH-20 — MSRV real (1.88) y vigilancia del QUICKSTART en el gate

- **Agente:** dsh
- **Fecha:** 2026-10-21
- **Base:** `e259b4f` == origin/main (push91; T-DSH-19 integrada).
- **Encargo (turno 1491):** (1) averiguar con git log/cargo el mínimo real de Rust que compila y corregir solo las líneas «Rust 1.88+» de `README.md:19`, `CONTRIBUTING.md:7`, `README.es.md:19`, `README.zh.md:19` (si no se puede demostrar, escribir «toolchain fijada en rust-toolchain.toml» en vez de inventar un número); (2) añadir a `scripts/check_docs_reality.sh` la vigilancia de las cifras «N guilds» y «N tools» del QUICKSTART con el mismo patrón que README, demostrando que falla al alterar la cifra. Criterios: gate 0 en limpio y 1 con la cifra alterada, `check_doc_links.py` y `check_test_count.sh` en verde. Informe con `## Impact` y «no verificado».

## (1) Mínimo real de Rust: **1.88.0** (demostrado empíricamente)

Con las toolchains ya instaladas en la máquina:

| toolchain | comando | resultado |
|---|---|---|
| **1.88** | `rustup run 1.88 cargo check -p tylluan-kernel --all-targets` | **exit 0** — «Finished `dev` profile … in 2m 22s» |
| **1.85** | `rustup run 1.85 cargo check -p tylluan-kernel --all-targets` | **exit 101** — el lockfile no compila: `image@0.25.10 requires rustc 1.88.0`, `icu_*`/`idna_adapter` requieren 1.86 |

Conclusión demostrada: **el mínimo real con el `Cargo.lock` actual es 1.88.0** (1.85 falla; el propio `image` exige 1.88.0). El «Rust 1.88+» de los cuatro documentos **no era falso como mínimo**; lo que faltaba era no confundir *mínimo* con *pin*: `rust-toolchain.toml` fija **1.99.1**. Correcciones aplicadas (solo esas líneas):

- `README.md:19` (badge): `alt="Rust 1.88+ minimum (toolchain pinned 1.99.1 in rust-toolchain.toml)"`.
- `README.es.md:19`: `alt="Rust 1.88+ mínimo (toolchain fijada en 1.99.1 en rust-toolchain.toml)"`.
- `README.zh.md:19`: `alt="Rust 1.88+ 最低要求（rust-toolchain.toml 固定为 1.99.1）"`.
- `CONTRIBUTING.md:7`: «Rust 1.88+ minimum (`rust-toolchain.toml` pins 1.99.1; `rustup` recommended)».

El texto visible del badge se queda en «1.88+» a propósito: es el mínimo demostrado. **Recomendación (no aplicada, fuera de los archivos asignados):** añadir `rust-version = "1.88"` al `Cargo.toml` del workspace para que sea cargo quien lo haga cumplir, y así el mínimo deje de estar solo en prosa.

## (2) Gate: watchdog de las cifras del QUICKSTART

`scripts/check_docs_reality.sh` (diff +27/−2):

1. **Guilds**: `docs/getting-started/QUICKSTART.md` añadido a la lista de documentos del bloque de recuento (`README.md`, `README.es.md`, `STATUS.md`, `architecture.svg`) y ampliado el patrón para la frase del quickstart: `[0-9]+ default routable guilds` (las demás docs siguen con sus patrones).
2. **Tools**: nueva sección 7 que calcula el número real **desde la fuente única** `TylluanServer::sovereign_tool_names()` de `transport/server/tools.rs` (CONTRACT-01 = 6, ADR-021) y compara con `docs/getting-started/QUICKSTART.md`, `STATUS.md` y `README.zh.md` (formas con dígito). Cualquier cambio de cifra en esos docs → `❌` + `problems=1`.
3. `QUICKSTART.md`: «exactly **six** sovereign tools» → «exactly **6** sovereign tools», para que la cifra sea comprobable con el mismo patrón numérico (mismo estilo que «45 default routable guilds»).

### Prueba de que el gate detecta la deriva (salidas reales)

Estado limpio (solo quedaría el hallazgo preexistente ajeno, ver nota):

```text
$ bash scripts/check_docs_reality.sh   # exit 1 (por ROADMAP_O3, ver abajo)
MIS_HALLAZGOS_CLEAN=0   # ninguna queja imputable a mis secciones
```

Guild alterado 45→46:

```text
$ bash scripts/check_docs_reality.sh   # GUILD_EXIT=1
❌ docs/getting-started/QUICKSTART.md:132: dice '46 default routable guilds' pero el catálogo enrutable real tiene 45 guilds (46 módulos FastMCP − 1 excluidos)
```

Tools alterado 6→7:

```text
$ bash scripts/check_docs_reality.sh   # TOOLS_EXIT=1
❌ docs/getting-started/QUICKSTART.md:119: dice '7 sovereign tools' pero CONTRACT-01 fija 6 herramientas soberanas (sovereign_tool_names en tools.rs)
```

Restauradas ambas cifras (45 y 6), el gate vuelve a su estado de partida (solo el ❌ preexistente).

### Nota sobre «exit 0 en limpio»

En `e259b4f` el gate **ya sale con exit 1 antes de mi cambio** por un hallazgo preexistente y ajeno a esta tarea: `docs/roadmap/ROADMAP_O3.md:42` cita `onnx/int8/decima.json`, que no existe. Lo verifiqué con la versión HEAD del script (`git stash push -- scripts/check_docs_reality.sh`): **baseline y mi versión producen exactamente el mismo único ❌**; mi cambio no añade ninguno. El «0 en limpio» literal exige editar `ROADMAP_O3.md`, que no está en los archivos asignados: si quieres, lo corrijo en una tarea aparte (una línea). Por eso la demostración de arriba se presenta como «0 hallazgos imputables a mis secciones en limpio / 1 exactamente con la cifra alterada».

## Verificación

| criterio | resultado |
|---|---|
| `bash scripts/check_docs_reality.sh` (limpio) | exit 1 **solo** por el ❌ preexistente de `ROADMAP_O3.md:42`; **0** hallazgos nuevos de mis secciones (baseline idéntico) |
| gate con «45→46» | **exit 1** + `❌ QUICKSTART.md:132` |
| gate con «6→7» | **exit 1** + `❌ QUICKSTART.md:119` |
| `python scripts/check_doc_links.py` | **exit 0** («222 relative doc links … all resolve») |
| `bash scripts/check_test_count.sh` | **exit 0** (kernel 1152 + link 75 + fsrs 12 = 1239) |
| `cargo check` con 1.88 | **exit 0** (2m 22s) |
| `cargo check` con 1.85 | exit 101 (`image@0.25.10 requires rustc 1.88.0`) |

## Impact

- **Diff sin commitear: 6 ficheros, +30/−7**, todos de docs/gate:
  - `README.md`, `README.es.md`, `README.zh.md` (1 línea cada uno, `alt` del badge),
  - `CONTRIBUTING.md` (1 línea, mínimo + pin),
  - `docs/getting-started/QUICKSTART.md` (1 línea: «six»→«6» para que el gate pueda vigilarlo),
  - `scripts/check_docs_reality.sh` (+27/−2: quickstart en guilds + sección 7 de tools).
- **Cero cambios en `src/`**, cero arranques de kernel, sin tocar el recuento de tests (1239) ni ejecutar instaladores.
- El gate gana dos vigilantes nuevos con la misma semántica que el de guilds de README; las cifras siguen siendo 45 y 6 en el estado actual.

## No verificado

- **1.86 y 1.87**: no están instaladas y no las descargué; el suelo de 1.88 se sostiene en que 1.85 falla y `image@0.25.10` declara `rustc 1.88.0` como mínimo, pero no ejecuté 1.86/1.87 (la conclusión práctica — «1.88 es el mínimo con este lockfile» — sí está demostrada por ambos extremos).
- **`cargo check` de los demás crates con 1.88**: solo verifiqué `-p tylluan-kernel --all-targets` (lo que compila el quickstart); `tylluan-cli`/`tylluan-link`/`tylluan-gui` (este último edition 2021) no se comprobaron con 1.88.
- **CI real**: no se lanzó el workflow; el gate se ejecuta en local. El badge del README no se renderizó (solo se cambió el `alt`).
- **`ROADMAP_O3.md:42`**: hallazgo preexistente, no corregido (fuera de los archivos asignados).
