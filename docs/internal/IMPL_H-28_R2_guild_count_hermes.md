# IMPL_H-28 — R2: conteo de guilds con el criterio del kernel en check_readme_claims.sh

Base: `4cb3d26` (punta local del TL, incluye H-27 integrada). Reset previo cumplido: mis restos H-27 verificados byte-iguales al commit del TL antes de `git stash push -u` (`reset-hermes-H28-*`); árbol limpio en 4cb3d26. Un fichero modificado, sin commitear.

## Diff: `scripts/check_readme_claims.sh` (+61, sección 8 antes del RESULT)

R2 replica `scan_guilds_directory` (catalog.rs:536+) sin hardcodear nada:
- familias = subdirectorios directos de `guilds/` (salta `_`), busca `guilds/<fam>/plugins/` si existe, si no la familia misma;
- `*.py` de ese nivel con stem sin `_`;
- marcador de guild: contiene `FastMCP(` y no `FastMCP()` vacío (equivalencia de `extract_guild_name` catalog.rs:353 — verificado vivo: cero casos de nombre vacío/espacios en el árbol, documentado como aproximación);
- `name_override` parseado de catalog.rs:167 (hoy scrapling_web→scrapling);
- `EXCLUDED_GUILDS` parseado de catalog.rs:98 con `grep -m1` (línea única; un rango `/];/` se comía los required_args de guild_overrides — bug real que me comí en el primer run, corregido y anotado en el propio script);
- el conjunto de valores «N guilds» afirmados por cada README (líneas sin additional/adicionales) debe ser exactamente {computado}.

## Casos de prueba (salidas literales de esta sesión)

**Limpio** (árbol real `4cb3d26`) → exit 0:
```
OK guildcount: README.md claims exactly 45 active guilds; kernel-criterion scan finds 45 (1 excluded via [vision_moondream ], 12 .py without FastMCP marker)
OK guildcount: README.es.md claims exactly 45 active guilds; kernel-criterion scan finds 45 (…)
RESULT: all README claims verified against the tree (…)
```
(45 = el número exacto que validé manualmente en H-26; R1/R3 siguen verdes.)

**Deriva** (sandbox git con árbol de guilds sintético de 7 ficheros diseñado para ejercitar cada rama del criterio: 1 guild válido, 1 renombrado por override, 1 en la lista EXCLUDED del catalog falso («ghost» — prueba de parseo dinámico), 1 vision_moondream excluido, 1 con `_` prefijo, 1 sin marcador FastMCP, 1 con `FastMCP()` vacío) → exit 1:
```
DRIFT guildcount: README.md claims active-guild values {45} but kernel-criterion scan (scan_guilds_directory equivalent) finds 2
DRIFT guildcount: README.es.md claims active-guild values {45} but kernel-criterion scan (…) finds 2
```
Computó exactamente 2 (g1 + scrapling_web→scrapling): exclusión, override, skip `_`, skip sin-marcador y guard de paréntesis vacío funcionan; el README copiado afirmaba 45.

## ## Impact

- Zona `scripts/` (ADR-013): aditivo sobre check_readme_claims.sh — secciones 1-7 intactas (verificado: el run limpio sigue dando todas las OK previas), cero toques a READMEs, guilds/, crates/, red o cargo. Read-only.
- Cable heredado de H-25: verify.sh --docs ya lo ejecuta en modo informe; ningún cambio de cableado necesario. El gate nuevo no puede bloquear pushes.
- Robustez ante cambios futuros: si el TL añade exclusiones/overrides en catalog.rs, R2 los lee solo — no hay listas duplicadas que se oxiden (anti-padrón que el propio guardián existe para cazar).

## No verificado

- Equivalencia total con `extract_guild_name` en casos exóticos (FastMCP con variable sin comillas o nombre con espacios entre paréntesis): no existen en el árbol hoy (grep verificado); la aproximación `FastMCP(` menos `FastMCP()` quedaría ligeramente más laxa si aparecieran.
- Falsos positivos sobre READMEs históricos (git log -p): sin correr, igual que en H-27.
