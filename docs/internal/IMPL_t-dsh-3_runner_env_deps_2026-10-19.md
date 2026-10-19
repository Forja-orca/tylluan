# T-DSH-3 — Auditoría scripts/ + .github/workflows: riesgo de fallos falsos en runners (2026-10-19, dsh)

Asignación: TL T1324. Base: origin/main `986499c` (fetch hecho; diff T-DSH-2 previo ya integrado y descartado según precedente). Diff SIN commitear. Exclusiones respetadas: ningún cambio en umbrales/timeouts de guilds, ningún toque en claims de seguridad, ningún kernel tocado.

## Método

- Lectura COMPLETA de los 7 workflows: `ci.yml` (429 líneas), `release.yml` (420), `install-smoke.yml` (90), `docker-smoke.yml` (77), `guild-python-portable.yml` (59), `codeql-analysis.yml` (35), `portability-roundtrip.yml` (140).
- Los 38 ficheros de `scripts/` clasificados por referencia real: grep cruzado workflows+verify.sh+`docs/reference/security-claims.toml` (los 3 claims .sh NO están muertos: `run_claims.py` los lanza dinámicamente vía el manifiesto — verificado en `security-claims.toml` líneas 24/40/47).
- Grep sistemático: `uses:` sin pin SHA, `toolchain:`, `pip|npm|cargo install`, `curl … | sh`, rutas absolutas, supuestos bash/WSL/MSYS, puertos fijos, sleeps/loops de espera, caches.
- Ejecución real de los pasos candidatos a fix (lo único que justifica tocar algo, según la orden).

## TABLA de hallazgos

| # | Hallazgo | Fichero y línea | Cómo falla (fallo falso) | Arreglo |
|---|----------|-----------------|--------------------------|---------|
| 1 | `npm install` sin lockfile estricto en el paquete compartido | `ci.yml` línea 129 (`working-directory: packages/tylluan-ui-core`) | `npm install` re-resuelve rangos de `package.json` en cada corrida aunque exista `package-lock.json`; un patch publicado río arriba puede cambiar el build de `@tylluan/ui-core` sin ningún commit en el repo y romper `pnpm run build` del dashboard. | **ARREGLADO (una línea):** `npm ci` — instala exactamente el lock y convierte el drift en error explícito (equivalente npm de `pnpm --frozen-lockfile`). Ejecutado de verdad: `npm ci` exit 0 y `npm run build` (tsc) exit 0 en `packages/tylluan-ui-core`. |
| 2 | Toolchain flotante `stable` ×10 en CI contra `rust-toolchain.toml` (canal 1.88) | `ci.yml` líneas 24/53/68/143/166/323/398/423, `portability-roundtrip.yml` línea 41, `release.yml` línea 40 | `setup-rust-toolchain` con `toolchain: stable` fija RUSTUP_TOOLCHAIN y gana sobre el fichero: cada nuevo stable (≈6 semanas) añade lints y puede poner rojo `clippy -D warnings` sin cambio de código; además diverge de la verificación local, que usa 1.88 por el fichero. Es política EXPLÍCITA del repo (CLAUDE.md «el script fija el toolchain a stable … igual que ci.yml»), así que cambiarla no es una línea segura. | Solo listado — decisión de política del TL/José: o fijar `toolchain: "1.88"` en los 10 sitios + verify.sh, o quitar el input y dejar mandar al fichero. Requiere correr CI para validar cualquier opción. |
| 3 | `cargo install cargo-deny --locked` sin `--version` | `ci.yml` línea 55 | Instala el cargo-deny más nuevo en cada corrida; una nueva versión con bans/licenses más estrictos o formato de config cambiado rompe el job sin cambio en el repo. | Solo listado — el pin exige elegir versión y validar `deny.toml` contra ella (instalación de ~10 min por candidato); mejor lo decide el TL con la versión que quiera congelar. |
| 4 | `pip install ruff pytest pytest-asyncio` sin versión + deps de guilds por rango | `ci.yml` línea 107; `guilds/requirements.txt` (`mcp>=1.27.0,<2`, `fastmcp>=3.2.4`, `psutil>=5.9.0`) | Nuevo ruff con lints nuevos → job Python rojo sin cambio de código (mismo patrón que #2). Los rangos de guilds flotan en cada corrida de CI y del builder portable. | Solo listado — tocar `guilds/requirements.txt` es tocar dependencias de producción de los guilds (política). El pin de ruff/pytest es una línea pero debe elegirse la versión congelada y validarla en el runner real. |
| 5 | Boot-smokes con espera fija de 30 s sobre kernel cold-boot | `install-smoke.yml` líneas 51-59 y 75-89; `ci.yml` líneas 358-371; `release.yml` líneas 333-349 y 365-384 | Kernel recien instalado, primera creación de SQLite + migraciones en runner gratis despacio; 30 s es justo y es intermitente en carga alta de runners. `version-gate` del mismo release.yml ya aprendió esto (usa 60 s y espera `"status":"ok"`, líneas 250-261) — los otros 4 sitios no se actualizaron con la misma lección. | Solo listado — es subir timeouts (una línea cada uno), pero la regla de excluir cambios de timeouts sin validación en runner real aplica; propongo replicar el patrón 60 s + esperar ok. |
| 6 | Sincronización por sleep fijo en portability-roundtrip | `portability-roundtrip.yml` líneas 111-113 | `sleep 3` para forzar flush y `sleep 2` antes del copiado: el recordatorio se hace por REST (commit síncrono de SQLite) pero el kill sin graceful shutdown sobre un runner lento puede cazar un checkpoint WAL a medias → marcador «no encontrado» intermitente. | Solo listado — lo correcto es matar con SIGTERM y esperar a que el proceso salga (o `sqlite3 … 'PRAGMA wal_checkpoint'`), no otro sleep. Requiere validación en runner. |
| 7 | Runners flotantes mixtos | `release.yml` líneas 29-32 y 301-304 (`macos-latest`, `windows-latest`) vs `ubuntu-22.04` fijado en otros; `ubuntu-latest` en ci.yml | GitHub promueve `*-latest` a imágenes nuevas sin aviso: un cambio de imagen (Xcode/VS/Windows update) puede romper el build de release sin ningún commit. La mezcla (ubuntu fijado a 22.04 en release, latest en ci) es inconsistente ya hoy. | Solo listado — fijar imágenes concretas es una línea pero cambia la superficie de build de releases; decisión del TL. |
| 8 | Bootstrap `uv` por `curl | sh` sin versión | `scripts/build_guilds_python.sh` línea 28 (y espejo .ps1); el workflow `guild-python-portable.yml` lo declara intencional en su comentario | Nuevo uv con comportamiento de resolución distinto puede cambiar el entorno portable construido sin cambio de código → job rojo o artefacto distinto. | Solo listado — ya es un trade-off DOCUMENTADO (evitar un tercer SHA); el TL puede añadir `uv tool install uv==X` o `--version`. |
| 9 | Puerto obsoleto del kernel viejo en herramienta local | `scripts/coloquio_summarizer.py` línea 28 (`KERNEL_BASE` con puerto 3030; el kernel real escucha en 47004) | El script falla con connection-refused contra el kernel real (o se conecta a una instancia fantasma del puerto histórico del incidente forja). No lo corre CI; lo usan agentes locales. | Solo listado — la línea es obvia (3030→47004) pero la orden exige «ejecución real del script» y ejecutarlo significa leer/escribir Coloquio contra la producción de José, lo cual tengo prohibido por reglas del loop. Se corrige en un latido en cuanto el TL lo valide. |
| 10 | URL de org muerta en comentarios del instalador | `scripts/install.sh` líneas 39-40 (repo `TylluanMCP/tylluan-nexus` que no existe; el real es `Forja-orca/tylluan`) | Ninguno — son líneas comentadas (código muerto); el riesgo es que alguien las descomente creyéndolo vigente y el instalador apunte a un 404. | Solo listado — borrarlas es trivial pero no previene ningún fallo real; ruido mínimo dejarlo para el gate de dead-code. |
| 11 | Descargas de datasets/modelos sin checksum | `scripts/download_longmemeval.sh` líneas 123/190 y `.ps1` líneas 98/175 | Un truncado o error silencioso de la CDN produce ficheros corruptos que revientan los evals lejos del origen (fallo falso «el modelo va mal»). | Solo listado — los ficheros de HF no publican sha256 canónico en la URL directa; solución correcta (esperar `LFS oid`) es más de una línea. |

## Verificado conforme (buenas prácticas presentes, sin hallazgo)

- **Todos** los `uses:` — terceros incluidos (`setup-rust-toolchain`, `codeql-action`, `action-gh-release`, `setup-buildx-action`, `pnpm/action-setup`) — llevan pin a SHA completo con comentario `# vX`. Impecable.
- `fetch-depth: 0` declarado con su porqué en los 3 jobs que lo necesitan (head-sync, no-predation, semantics).
- Claims de seguridad (leídos en modo lectura, sin tocar): loops de espera `sleep 0.5` con tope explícito y mensaje del fallo de arranque (el timeout frío se subió ya de 10 s a 120 s aprendiendo de un flake real, documentado en el script), limpieza de procesos y `TYLLUAN_DATA_DIR` aislado por claim; `run_claims.py` impone timeout interno 180 s + backstop externo con kill.
- `verify.sh` detecta y se adapta a WSL y a pnpm.cmd de Windows (`OSTYPE` + `/proc/version` + fallback `cmd.exe /c pnpm`) — el supuesto bash de Git en Windows está trabajado.
- `no-absolute-paths.sh` es él mismo el gate anti rutas de máquina, con patrón documentado contra falsos positivos (`ui://`, `http://`).
- `docker-smoke` y `tylluan.docker.toml` usan su puerto propio de contenedor de forma COHERENTE (3030 interno del contenedor, declarado intencional con host 0.0.0.0 justificado por port mapping) — no es un hallazgo.
- version-gate de release.yml: patrón correcto ya aprendido en vivo (esperar `"status":"ok"` y no el primer 200; config http-only para no morir por stdin cerrado; cargo metadata en vez de grep por la herencia de versión).

## Fix aplicada (única) y su verificación

`ci.yml` línea 129: `npm install && npm run build` → `npm ci && npm run build`.

Ejecución real en `packages/tylluan-ui-core` (Windows, node v24.13.0 / npm 11.14.1): `npm ci` exit 0 (lock presente y en sincronía con package.json, que es exactamente la garantía que faltaba en CI) y `npm run build` (tsc) exit 0. La semántica de `npm ci` es estable entre versiones de npm (no depende de features de v24).

## Impact

- **Producción: cero.** Un único cambio en `.github/workflows/ci.yml` (job `dashboard-lint`, paso que construye `@tylluan/ui-core`), más este informe. Ninguna dependencia tocada, ningún timeout o umbral tocado, ningún claim o script de seguridad tocado, ningún kernel tocado.
- **Riesgo del cambio:** mínimo y acotado — `npm ci` falla explícitamente si algún día `package.json` y `package-lock.json` de ui-core divergen; eso es exactamente el fallo verdadero que hoy pasa flotante. Si el TL prefiere cero riesgo en release, puede integrar y la primera corrida verde de `dashboard-lint` lo confirma.
- **Recuento de tests:** invariado (ningún test añadido/borrado; no toca la cascada de cifras).
- No toca `transport/`, `guilds/`, `integraciones` ni `tylluan*.toml` → no activa requisito de Impact en commit (lo pongo igualmente por costumbre de flota).

## Sin verificar («no verificado»)

- Que el job `dashboard-lint` en runner Ubuntu/node-20 de CI pase con `npm ci` — no puedo ejecutar un runner; validé el paso exacto localmente (exit 0 en ambos comandos). Lo confirma la primera corrida.
- Los timeouts: no mido en runner real el cold-boot; la evidencia es el aprendizaje documentado en el propio release.yml (60 s + esperar ok).
- El estado de salud actual de los 4 jobs con esperas de 30 s en runs recientes (no tengo acceso a `gh` autenticado desde el worktree; no lo asumí, solo señalé la incoherencia de patrones).
- Que existan fallos falsos por cache: no encontré cache de cargo/sccache configurada en ningún job (ningún `actions/cache`, ni `cache:` en setup-rust-toolchain); si el action introdujera default distinto en otra versión (la suya está fija por SHA), no lo he probado.
