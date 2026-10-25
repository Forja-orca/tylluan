# IMPL_H-31 — R4 del guardian: anclas fichero:línea de ROADMAP/STATUS vivas y con contenido

Diff **sin commitear** sobre `aa67836` (origin/main, push152, CI verde según relay de José). Un único fichero tocado: `scripts/check_readme_claims.sh` (+87 líneas, sección 9; secciones 1–8 intactas). Spec T1697: "R4 del guardian (anclas fichero:linea citadas en ROADMAP/STATUS existen y apuntan a lo citado), diff sin commitear con caso limpio y caso deriva."

## Qué hace R4 (sección 9)

Para cada ancla `path:LINE[-LINE]` citada en ROADMAP.md y STATUS.md:
1. **Existe**: el fichero resuelve (ruta exacta, o por basename en todo el árbol excluyendo target/.git/node_modules/dist; basename ambiguo tipo `main.rs` = 5 candidatos, OK si ANY cumple).
2. **Rango**: las líneas citadas caben en el fichero (mensaje distinto si es más allá del EOF).
3. **Apunta a lo citado**: extrae tokens de código de la ORACIÓN del ancla (dividida en [. ! ? |] para evitar fuga de tokens entre claims vecinos): snake_case, CONSTANTES_CON_GUIONBAJO, camelCase y palabras dentro de `backticks`. Si alguno aparece en ±5 líneas del ancla → OK. Si NINGÚN candidato contiene ninguno de los tokens → la frase no describe contenido de ese fichero (claims de ausencia tipo "ya no lleva || echo", o contexto cross-file) → existence+range y nada más, sin sobre-alegar deriva. Si los tokens viven en el fichero pero lejos del ancla → **DRIFT "content moved"**.
4. **Líneas históricas** (contienen vN.N, fecha ISO, estaba/históric/fixed/was/fue): el contenido se congeló en el tiempo; se verifican existencia+rango y se salta el check de contenido **por diseño documentado**.

## Pruebas (salidas reales, injectables sin tocar docs vía `CHECK_README_ANCHOR_DOCS`)

- **Limpio** (3 anclas verdaderas: Cargo.toml:37, config.rs:2294+backtick, main.rs:2140): `exit 0`, "all 3 path:line anchors live and content-backed".
- **Deriva** (4 casos inyectados): `exit 1` y los 4 cazados — fichero inexistente, línea más allá del EOF (52 líneas vs 999999), `main.rs:100`+wal_checkpoint→"content moved" (vive en kernel main.rs:2140), y `config.rs:2100`+CRITICAL_SECURITY_TRIGGER→"content moved" (vive en 2294+).
- **Vivo** (ROADMAP.md+STATUS.md sobre aa67836, 46 s): **12 OK, 1 DRIFT real**: `ROADMAP.md:159 graph.rs:739` — el fix de degree de v0.10.0 está hoy en graph.rs:789+ (tokens `local_query_graph`, `test_local_query_graph_degree_penalty` viven ahí); la línea 159 no lleva marcador histórico y el ancla quedó 50 líneas vieja. Fix sugerido al TL (una palabra en el doc, no en el script): escribir "v0.10.0" en esa línea dispara el modo histórico, o actualizar a graph.rs:789.

## Bugs propios que el caso deriva cazó antes de entregar (razón de ser de exigir ambos casos)

1. `find` sin podar node_modules → timeout de 420 s del suite entero (arreglado: prune + caché por basename + guard bash puro por línea; 2m55s → 46s).
2. Tokens chatarra por recorte de palabras (`astMCP`, `githubworkflowsciyml110`) → FPs; resuelto con seg por oración + ancla eliminada del seg + extracción directa de backticks.
3. Un candidato **sin** el vocabulario otorgaba OK mientras otro sí lo tenía (fixture lo pilló; ahora el whole-file grep decide por todos).
4. `grep -qw -- -eX` → tras `--` las opciones pasan a ser operandos; orden correcto: opciones, `--`, fichero.
5. `tr` dejaba espacios en tokens vacíos y `-z` fallaba; y `CONSTANTES_MAYÚSCULAS` no entraban en la extracción.

## Impact

Solo `scripts/check_readme_claims.sh` (scripts/, no transport/guilds/integrations/toml → sin sección ## Impact obligatoria en el commit, pero la incluyo aquí por convención H-24..H-28). Cero cambios en docs ni src. Cero commits. No cableado en verify.sh (sigue siendo decisión del TL; R1-R3 ya están en modo informe). Coste medido en vivo: +46 s al script completo.

## No verificado

- Que CI no añada el script a ningún job (no lo toqué; H-25 dejó el cableado a criterio del TL).
- Anchors en AGENTS.md/README (fuera de spec; R4 cubre solo ROADMAP/STATUS como pidió T1697).
- Tensores con acentos/unicode dentro de backticks (no aparecen en los docs actuales).
