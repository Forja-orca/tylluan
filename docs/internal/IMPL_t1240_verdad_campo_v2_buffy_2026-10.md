# IMPL T1240 — verdad de campo v2: muestra de pares de conflicto (Buffy)

- **Estado: IMPLEMENTADO — diff sin commitear.** Base: `origin/main` `f01cd9f`.
- **Autor:** Buffy. **Fecha:** 2026-10-18. Encargo del TL: muestra v2 con ≥60 pares
  nuevos, sin repetir nodos ni contenido de v1, ≥30 `contradice` y ≥15 de cada otra
  clase; `contradice` construida editando la afirmación de un nodo real porque las
  contradicciones reales casi no existen (v1 midió 0/100); la ficha DEBE declarar la
  sinteticidad de esa clase y la prohibición de mezclar con v1 para el baseline.
- **Continúa:** [IMPL_t1240_verdad_campo_buffy_2026-10.md](IMPL_t1240_verdad_campo_buffy_2026-10.md)
  (muestra v1, 100 pares reales) — es la respuesta a la limitación que v1 declaró
  ("si Deep quiere recall de esa clase, haría falta una muestra enriquecida").

## 1. Entregables (6 ficheros nuevos, aditivos)

| Fichero | Qué es |
|---|---|
| `benchmarks/human_truth_conflict_pairs_v2.json` | **Dataset: 66 pares** con ficha integrada |
| `benchmarks/datasets/build_conflict_pairs_v2.py` | Extracción del pool (BD `mode=ro`, determinista, excluye v1) |
| `benchmarks/datasets/conflict_pairs_v2_contra.json` | Anotación: 34 ediciones sintéticas (patrón → reemplazo) + razón |
| `benchmarks/datasets/conflict_pairs_v2_pairs.json` | Anotación: 32 pares de nodos reales (16+16) + razón |
| `benchmarks/datasets/finalize_conflict_pairs_v2.py` | Genera el JSON validando cuotas, dedupe y ediciones |
| `benchmarks/datasets/check_conflict_pairs_v2.py` | Verificador **independiente** (re-lee el entregable, v1 y la BD) |

## 2. Cuotas cumplidas

| Requisito | Entregado |
|---|---|
| ≥ 60 pares nuevos | **66** (`cp2-001`…`cp2-066`) |
| ≥ 30 `contradice` | **34** (todas `origin: synthetic_edit`) |
| ≥ 15 `consistente` | **16** |
| ≥ 15 `no relacionado` | **16** |
| Sin repetir nodos de v1 | **98 nodos únicos en v2, solape 0** (ids, hashes y textos) |
| Contradicción = edición de nodo real | 34/34: lado B = sustitución regex de **exactamente 1 match** sobre el contenido crudo del nodo publicado en lado A (`edit_pattern`/`edit_replacement` incluidos en cada registro) |

Tipos de nodo usados (calculados, no declarados): `coloquio_memory` 29,
`episode` 29, `lesson` 16, `routing_anchor` 15, `session_digest` 9. Fuentes
elegidas por recuento **antes** de leer contenidos (semilla `20261019`, ventana
150–300 chars para publicar el texto **completo**, sin depender de la BD viva).

## 3. Declaraciones de la ficha (verificadas por el checker)

- `clase_contradice_sintetica`: "LA CLASE `contradice` DE v2 ES SINTÉTICA AL 100%
  (34/34)… esas afirmaciones editadas NO existen en la BD… Toda métrica publicada
  debe etiquetar esta clase como sintética."
- `baseline_sin_v1`: "**PROHIBIDO mezclar** este dataset con
  `human_truth_conflict_pairs_v1` para calcular el baseline… el baseline de v2 se
  calcula SOLO sobre estos 66 pares (siempre `contradice` = **51.5%**); v1 conserva
  su propio baseline (siempre `consistente` = 91%, sobre sus 100 pares reales)."
- `sesgos_y_limitaciones`: marca léxica de las ediciones (negación, número opuesto,
  ✅/❌ → un clasificador puede resolverla por atajo), correlación intra-muestra
  (agrupar por evento/nodo en splits), BD viva, sesgo de selección del anotador.

## 4. Verificación (re-ejecutada tras cada cambio; exit preservado)

- `check_conflict_pairs_v2.py` → **OK, CHECK_EXIT=0**: 66 pares; dist
  34/16/16; baseline 51.5% solo-v2; 34/34 ediciones reproducidas **también desde la
  BD en vivo**; solape con v1 = 0 en nodos/hashes/textos; firmas duplicadas 0;
  `bd_drift: []` (98/98 nodos coinciden hoy con su hash publicado).
- `data/silva.db` abierta **solo** con `file:...?mode=ro` en los tres scripts
  (sin ningún `UPDATE/INSERT/DELETE` en los entregables).
- El checker **falló en dos ocasiones y se corrigió la causa** (no se relajó):
  1. `agent_reputation:*` cambió en la BD **en minutos** (puntuaciones recalculadas
     en caliente) → los 3 `contradice` de esa clase se sustituyeron por nodos
     estables (`coloquio_equipo_96`, `coloquio_equipo_380`,
     `session_digest__0c32e34…`).
  2. `lesson` con id `intent lee_el_turno` mutó en la BD → sustituido en su par
     `no relacionado` por el episodio `hybrid` de pool [20] (registro de ping
     a `mcp_bridge`).
  Deriva de la BD viva reportada como `bd_drift` (solo falla si >10 nodos: la
  fuente de verdad es el fichero versionado, declaración ya presente en la ficha).

## 5. Qué NO es esto

- **No es verdad humana**: el anotador es Buffy (agente), un solo anotador, sin
  kappa — declararlo en cualquier métrica, igual que v1.
- **La clase `contradice` no mide prevalencia real**: mide que el verificador NLI
  sabe distinguir afirmaciones contradictorias cuando existen. La prevalencia real
  sigue siendo la de v1: **0/100** (hallazgo intacto).
- **No mezclar v1+v2** para ninguna cifra agregada (poblaciones distintas por
  diseño: muestra real de fan-out vs muestra construida).
- No modifica código, config ni la BD.

## Impact

- Aditivo: 6 ficheros nuevos en `benchmarks/` (+ este informe); ningún fichero
  versionado existente cambia; cero cambios de código, config o esquemas.
- `data/silva.db` de producción: **solo lectura** (`mode=ro` verificado en los
  tres scripts).
- Los dos datasets conviven sin solape verificado (0 ids, 0 hashes, 0 textos):
  v1 y v2 siguen siendo poblaciones separadas y la ficha de v2 obliga a tratarlas
  así al calcular el baseline.
- Desbloquea el **recall de la clase `contradice`** para el duelo P2 (Deep ya
  cerró su duelo sobre v1 en `IMPL_p2_duelo_deep_2026-10.md`; con v2 puede medir
  ahora la clase que en v1 no era evaluable), sin tocar su informe ni su baseline.
- Nada de esto entra en producción ni afecta al runtime del kernel.
