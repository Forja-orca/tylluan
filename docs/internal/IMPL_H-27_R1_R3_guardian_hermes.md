# IMPL_H-27 — R1 + R3 implementadas en check_readme_claims.sh (H-27, T1673)

Base: `159c068` (punta del main local del TL; origin/main aún `7b24da2` — el script canónico vive en la punta local, verificado `cat-file -e` en ambos). Reset previo cumplido (stash `reset-hermes-H27-*`; mi `??` de H-26 era byte-igual al integrado `159c068`). Un solo fichero modificado, sin commitear. R2 queda para después (spec del TL).

## Diff: `scripts/check_readme_claims.sh` +72 (secciones 6 y 7, antes del RESULT)

- **R1 paridad bilingüe**: SET de valores afirmados por sustantivo compartido (`tests|questions|preguntas|guilds`) debe coincidir entre README.md y README.es.md; paridad de menciones semver aparte. Diseño deliberado: SET de valores, no multiset de frecuencias — el orden de palabras y la frecuencia legítima difieren entre idiomas (verificado vivo: hoy difieren en «0.4/3.2 ms», «38–68%» de la tabla de perfiles y frecuencias de «45 guilds» sin que haya deriva; un diff global de tokens habría dado falsos positivos). Sustantivo `tools/herramientas` excluido a conciencia: el recuento soberano ya se gatesea nombre-a-nombre contra `tools.rs` en la sección 1, bilingüalmente (FP real que me comí en el primer run: el mermaid dice «6 Sovereign Tools» en EN y «6 Herramientas Soberanas» en ES — mismo valor, distinto sustantivo).
- **R3 respaldo de benchmarks**: cada cifra `Recall@K`/`Latencia p50` de las tablas debe existir como número en el `benchmarks/*.json` citado por los READMEs, y la tabla debe llevar disclaimer versión+fecha a ≤10 líneas. MRR se salta (no es clave del JSON; documentado en el comentario).

## Casos de prueba (salidas literales de esta sesión)

**Caso limpio** (árbol real `159c068`) → exit 0:
```
OK bilingual: numeric claim values (tests/questions/guilds) identical in README.md and README.es.md: 13,45,50,1329,
OK bilingual: version mentions match across READMEs: v0.12.0,v0.15.0,v0.17.0,v0.17.1,v1.0.0,
OK benchmark: 8 benchmark table figures all present in benchmarks/longmemeval_v0.12.0.json
OK benchmark: README.md benchmark table carries version+date historical disclaimer
OK benchmark: README.es.md benchmark table carries version+date historical disclaimer
RESULT: all README claims verified against the tree (…)
```
**Caso deriva** (sandbox git en scratch con dos deriva-ciones inyectadas: README.es «1329 tests»→«999 tests»; README.md «Recall@5 | 82%»→«97%») → exit 1, detecciones exactas:
```
DRIFT bilingual: README.md claims {value set: 13,45,50,1329,} but README.es.md claims {13,45,50,999,} — one language is stale
DRIFT benchmark: benchmark figures without backing value in benchmarks/longmemeval_v0.12.0.json: README.md:Recall@5=97
```
(Las otras 39 líneas DRIFT del sandbox son ruido esperado de secciones 1-5 por ficheros ausentes en el mínimo — el sandbox solo porta lo que necesitan R1/R3; las dos detecciones objetivo son las únicas bilingual/benchmark.)

El cableado ya existe (H-25: verify.sh modo informe + REPORT_ONLY_GATES en test_verify_semantics.sh) — no hubo que tocar verify.sh: el gate ampliado hereda el cable.

## ## Impact

- Zona `scripts/` (gate ADR-013): puramente aditivo sobre `check_readme_claims.sh` — cero cambios a secciones 1-5, cero edición de READMEs, cero red, cero cargo. Semántica sigue siendo read-only y report-only vía el cable H-25.
- Falsos positivos acotados y justificados: exclusiones documentadas en el propio script (frecuencias, ms/%, MRR, herramientas).
- Riesgo asumido: R1 compara sets de valores; una cifra nueva en UN solo idioma que sea intencionada (no-drift) daría DRIFT — es el comportamiento deseado (fuerza a sincronizar ambos READMEs o a decidir el alcance de la cifra).

## No verificado

- R2 (conteo de guilds con criterio del kernel) sigue pendiente por decisión del TL («queda para después»).
- Comportamiento bajo `verify.sh --docs` completo en CI Linux: heredado del cable H-25 (allí T5 del meta-test no está afectado; mi cambio no toca el meta-test).
- Falsos positivos históricos de R1/R3 sobre READMEs pasados: no corrido contra 20 versiones previas (coste bajo si el TL lo pide: `git log -p README.md` + correr el script por commit).

## Fe

Errata del TL sobre H-26 (ruta real `memory/silva/hnsw.rs`) registrada; la corrección del fichero ya integrado te corresponde a ti como comiteador.
