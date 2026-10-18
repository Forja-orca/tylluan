# IMPL T1240 (Buffy) — verdad de campo: intents de enrutado + pares de conflicto

- **Estado: IMPLEMENTADO — diff sin commitear.** Base: `origin/main` `6b9b177`.
- **Autor:** Buffy. **Fecha:** 2026-10-18. Encargo: Coloquio T1240, GO del TL en T1244.
- **Bloquea:** el duelo P2 de Deep (su parte arranca cuando este dataset aparezca).

## 1. Entregables (JSON versionado en `benchmarks/`)

### A. `benchmarks/human_truth_routing_intents_v1.json` — 105 intents, P1

- **105 intents redactados a mano** (no derivados de ningún registro del
  enrutador: cero filas de `guild_audit_log`), cada uno con `guild`, `lang`,
  `domain` y `reason` (razón de una frase).
- **Conjunto cerrado fijado y documentado** en la ficha: las 7 etiquetas
  dominantes de `decision_fabric_dataset.jsonl` (4794/4827 = 99.3%) —
  `kernel, bash, coloquio, system_metrics, monitor, code_analysis, other` —
  con definición operativa por etiqueta. Las 8 etiquetas de cola se excluyen
  documentadamente (1-32 muestras y errores de etiquetado visibles allí).
- Distribución balanceada 15/15/14/15/16/14/16 (sesgo declarado en ficha);
  idioma 79 es / 26 en.
- **Dedupe ejecutado y verificado** (NFKD + minúsculas + sin puntuación;
  Jaccard de tokens con umbral 0.7): **0 duplicados internos, 0 solapes
  exactos con el fabric, 0 near-dups, máx Jaccard observado 0.400** sobre
  los 1317 intents normalizados únicos del fabric. Resultados en el propio
  JSON (`validation`).

### B. `benchmarks/human_truth_conflict_pairs_v1.json` — 100 pares, P2

- **100 pares** de nodos `conflicted` de `data/silva.db` abierta
  **EXCLUSIVAMENTE con `file:...?mode=ro`** (script
  `benchmarks/datasets/build_conflict_pairs_v1.py`: SQLite read-only, imposible
  escribir). Réplica del agrupamiento de `flag_contradiction_nodes` sobre
  aristas activas; en el momento de la extracción: **375 conflicted de 9945**
  (la cita "368 de 9896" de la síntesis del TL era de una medición anterior),
  196 grupos candidatos.
- **Muestreo determinista** (seed 20261018): cobertura completa de los tipos
  con texto semántico (`same_topic` 7/7 grupos únicos, `remembers` 4/4,
  `contributed_to` 1/1) + proporcional entre simbólicos (`defines` 59,
  `imports` 29). Cuota decidida por recuento de tipos, **antes** de leer
  contenidos. **Dedupe por firma sha256(A|B): 0 duplicados.**
- **Etiquetado manual** (`contradice`/`consistente`/`no relacionado`) con
  criterio NLI de 3 vías fijado antes de etiquetar y **frase de razón por
  pareja**. Las 12 parejas ricas se etiquetaron con el **texto completo** de
  la BD; el fichero publica solo extracto mínimo (300 chars, saneado con las
  regex de `anonymize_silva.py`) + `content_sha256` del crudo + `node_id`
  (recuperable en solo-lectura cuando el duelo necesite el texto entero).
- **Resultado: consistente 91 / no relacionado 9 / contradice 0.**
  **Baseline trivial "siempre consistente" = 91%** (declarado en la ficha;
  obligatorio en cualquier cifra que se publique).

### C. Fichas

Ambos JSONs llevan su ficha integrada (`ficha`: origen, muestreo/criterio,
quién etiquetó, sanitización, distribución, baseline, sesgos y limitaciones).

## 2. Hallazgo principal (para el duelo P2)

**Ninguna de las 100 parejas es contradictoria.** La heurística estructural
fan-out (`defines`/`imports` = 88/100) marca compatibles, no contradicciones:
coherente con el incidente de 2026-09-26 (marcado estructural → CPU runaway).
Es el argumento medido a favor de P2C/P2A: el verificador semántico solo
podaría — no hay conflicto real que resolver en esta muestra. El duelo debe
publicar el baseline trivial junto a cada cifra y reconocer que **la clase
`contradice` no es evaluable en esta muestra** (limitación explícita en la
ficha; si Deep quiere recall de esa clase, haría falta una muestra enriquecida
con pares contradictorios conocidos, decisión del TL).

## 3. Evidencia

- Intents: JSON parsea, 105 ids únicos, validación dedupe en verde (cifras
  arriba), distribución de etiquetas verificada por conteo.
- Pares: JSON parsea, 100 registros, 100 etiquetas válidas, 100 razones > 30
  chars, 0 firmas duplicadas; distribución `{"contradice":0,"consistente":91,"no relacionado":9}`.
- `data/silva.db` tocada solo en `mode=ro` (dos firmas de conexión: URI
  `mode=ro`; ningún `UPDATE/INSERT/DELETE` en los scripts entregados).
- Ficheros entregados: los 2 datasets + 2 scripts reproducibles
  (`build_conflict_pairs_v1.py`, `finalize_conflict_pairs_v1.py`) + este
  informe.

## 4. Qué NO es esto

- **No es verdad humana en el sentido fuerte**: el anotador es un agente
  (Buffy). El encargo lo pide así ("con tu criterio razonado"), pero debe
  declararse en métricas; la verdad humana de José sigue pendiente (sesgo
  #1 de ambas fichas).
- Un solo anotador: sin kappa.
- No modifica código, config ni la BD.

## Impact

- Aditivo: 4 ficheros nuevos en `benchmarks/` (+ 1 informe); ningún fichero
  versionado existente cambia; cero cambios de código, config o esquemas.
- `data/silva.db` de producción: **solo lectura** (`mode=ro` verificado en el
  script; sin escrituras posibles desde los entregables).
- Los datasets contienen texto mínimo de la memoria: saneado con las regex
  de `benchmarks/datasets/anonymize_silva.py` + hashes SHA-256 del crudo;
  ningún estado completo ni credencial en claro (regla del encargo:
  "hash + texto mínimo").
- Desbloquea el duelo P2 de Deep (su T1240 depende de estos 100 pares) y
  da a P1 un conjunto de evaluación sin circularidad frente al fabric.
- Nada de esto entra en producción ni afecta al runtime del kernel.