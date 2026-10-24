# T-DSH-65 — ROADMAP.md: anclas `fichero:línea` driftadas (hallazgos 11, 12 y 13 de H-30), solo lectura

- **Agente:** dsh
- **Fecha:** 2026-10-24
- **Base:** `origin/main` `d2e71cf` (reset de worktree con `agent_wt.sh reset dsh` antes de empezar; `git fetch` y lectura con `git show origin/main:...`).
- **Encargo (T1697):** solo lectura; tabla con el **valor real y la línea real** de las anclas `fichero:línea` driftadas de `ROADMAP.md` (hallazgos 11, 12 y 13 del informe H-30 de Hermes). **Sin editar** `ROADMAP.md` ni ningún otro fichero del repo.
- **Método:** cada dato se ha re-verificado con `grep`/`git rev-list` contra el árbol actual (`d2e71cf`), sin fiarme de las cifras del informe de Hermes (que estaban tomadas sobre `da2ab9e`).

## Tabla: reclamo → valor real → línea real

| # | Cita en `ROADMAP.md` (commit `d2e71cf`) | Valor real verificado | Línea real de la fuente |
|---|---|---|---|
| **11** | `ROADMAP.md` línea 102: «the system already reindexes stale nodes automatically every 10 min in `main.rs (línea 1235)` via `get_stale_embeddings()`» | **El valor (10 min = 600 s) es correcto, pero el ancla está driftada**: `main.rs (línea 1235)` no es el reindexer. En el **kernel**, la línea 1235 cae al final de un `println!`/snapshot (`);` de cierre + `system_snapshot`), y en el **CLI** la línea 1235 es el texto del **PID rancio** («PID file ... is stale»). El reindexer real está en el kernel | `crates/tylluan-kernel/src/main.rs`, **líneas 1918-1921**: `tokio::time::interval_at(... Duration::from_secs(600), Duration::from_secs(600))` con el comentario `// 10 minutes for toaster-friendly` (línea 1921); la llamada `get_stale_embeddings()` está en la **línea 1947** |
| **12** | `ROADMAP.md` línea 113: «34 `description_override()` entries for routing-critical guilds» | **34 es una cifra histórica; hoy son 46 entradas nombradas** (47 brazos si se cuenta el `_ => return None` final). La función sigue existiendo y es la que alimenta el catálogo | `crates/tylluan-kernel/src/router/catalog.rs`: función en **175-225**; brazos con nombre en **177-222** (46); brazo por defecto `_ => return None` en **223** |
| **13** | `ROADMAP.md` líneas 242, 252 y 265: «269 commits since v0.13.0», «155 commits since v0.14.0», «85 commits since v0.15.0» | **Los tres driftan**: el conteo real entre tags es **275 / 157 / 88**. El «170 commits since v0.16.0» de la línea 281 **sí es exacto** | `git rev-list --count`: `v0.13.0..v0.14.0` = **275**; `v0.14.0..v0.15.0` = **157**; `v0.15.0..v0.16.0` = **88**; `v0.16.0..v0.17.0` = **170** (comprobado con los tags locales, lista `v0.4.0`…`v0.17.1`) |

### Comandos usados (solo lectura)

```
git show origin/main:ROADMAP.md          # líneas 102, 113, 242, 252, 265, 281
grep -n "from_secs(600)\|10 minutes for toaster\|get_stale_embeddings" crates/tylluan-kernel/src/main.rs
grep -n "=>" (bloque de description_override en catalog.rs, líneas 176-224)
git rev-list --count v0.13.0..v0.14.0    # 275
git rev-list --count v0.14.0..v0.15.0    # 157
git rev-list --count v0.15.0..v0.16.0    # 88
git rev-list --count v0.16.0..v0.17.0    # 170
```

## Corrección mínima propuesta (NO aplicada: solo lectura)

Solo como sugerencia al TL para cuando edite `ROADMAP.md` (no he tocado el fichero):

1. **Línea 102**: cambiar `main.rs (línea 1235)` por la ruta y línea reales: `crates/tylluan-kernel/src/main.rs` (líneas 1918-1921) — o, si se prefiere no citar líneas que envejecen, «el reindexer agnóstico del kernel (`get_stale_embeddings`, main.rs)». Ojo: el `main.rs (línea 1235)` actual no solo está mal de línea, también es ambiguo entre kernel y CLI.
2. **Línea 113**: `34` → **46** (o 47 si se cuenta el brazo por defecto).
3. **Líneas 242/252/265**: `269` → **275**, `155` → **157**, `85` → **88**. La línea 281 («170») se queda igual.

## Impact

- Deja a punto las tres correcciones de `ROADMAP.md` con **valor y línea exactos re-verificados** contra `d2e71cf` (no contra el informe previo): sin esto, el TL tendría que re-descubrir las líneas reales al editar.
- Confirma que el drift de la #11 no es solo de número: el ancla `main.rs (línea 1235)` apunta a **otro fichero y otro contenido** (PID rancio del CLI), que es el tipo de error que un lector no detecta sin abrir el archivo.
- Cero escrituras en `E:\tylluan` y cero commits; no se tocó `ROADMAP.md`.

## No verificado

- **No verifiqué si hay más anclas driftadas de las que listó H-30** (el encargo era los hallazgos 11-13); el informe H-30 ya avisa de que su tabla era de las 15 cifras más expuestas, no exhaustiva.
- **`git rev-list --count` depende de los tags locales**: comprobé `v0.13.0`…`v0.17.1` presentes y son los que cita ROADMAP; si el repo remoto tuviera tags distintos no lo sé (no hice `git ls-remote --tags`, solo `git tag --list` local tras `fetch`).
- **El recuento de `description_override()`**: doy 46 entradas nombradas y 47 brazos; si se quiere contar "entries" como solo las cadenas con nombre, el número correcto es 46. La diferencia (46 vs 47) es de criterio, no de datos.
- **No comprobé** que el valor 600 s siga siendo el que usa el reindexer en tiempo de ejecución (leí el código, no un kernel en marcha); en el kernel de producción el reindexer estaba en marcha pero no lo instrumenté.
- Push142 del TL estaba en curso al hacer el trabajo: si `ROADMAP.md` cambia de línea en ese push, las líneas de la columna «Cita» pueden desplazarse (el contenido citado se verifica por texto, no solo por número).
