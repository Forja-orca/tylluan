# H-50 — Mapa de llamadores de `touch_node` (FSRS sobre `coloquio_memory`)

**Asignación:** T2088 (forma corta) / T2094 (spec completa del TL). **Base:** `efc2f40` (push196; HEAD confirmado en T2092). **Sin cargo** (respetado el SILENCIO DE CARGA de push197: cero `cargo`). Propósito: cerrar el `## No verificado` de H-49 («qué rutas escriben `fsrs_last_review`»). Referencias sueltas en forma `línea NNNN` (convención del gate de docs).

## 1. El mecanismo: una llamada, cinco efectos

`touch_node` (`decay.rs:338`) delega en `touch_node_with_rating` (`decay.rs:344`), que por cada llamada:
1. INSERT traza en `node_traces` (`decay.rs` línea 357).
2. `UPDATE nodes SET last_touched` (línea 362).
3. **Repaso FSRS + UPDATE `fsrs_stability, fsrs_difficulty, fsrs_last_review`** (consulta línea 371-374; `item.review` línea 380; UPDATE línea 382).
4. Stigmergy 1-hop: trazas `stigmergy_propagation` a ≤10 vecinos (líneas 405-418) — **solo traza, sin repaso FSRS** para los vecinos.
5. Calor semántico: trazas `diffuse_heat` a ≤20 candidatos por embedding (líneas ~451-452) — **solo traza**.

**Rating:** ningún llamador de los crates pasa `Some(Rating)` (grep `touch_node_with_rating|Rating::` fuera de tests = 0); `touch_node` pasa `None` → default `Rating::Good` (`decay.rs` línea 369 y doc línea 343). Con los pesos por defecto `W[5]=0.7, W[6]=0, W[7]=0` (`tylluan-fsrs/src/lib.rs` líneas 29-33), Good (valor 3) es **neutral**: `delta_d = -W[4]·(3-3) = 0` → dificultad no cambia (línea 157); `r_factor = e^0 − 1 = 0` → `mult = 1` → **estabilidad intacta** (líneas 139-142 y 167-171). El único efecto real de todo touch en producción: **`last_review = now`** (línea 174).

## 2. Tabla llamador → línea → condición (solo los que PUEDEN alcanzar `coloquio_memory`)

| ruta | fichero:línea | condición de toque | ¿llega a `coloquio_memory`? |
|---|---|---|---|
| MCP `tylluan_recall` (2 caminos de retorno, ambos dentro de `handle_tylluan_recall_inner`, fn en línea 153) | `handler_recall.rs` líneas 698 y 1019 | bucle sobre TODOS los nodos devueltos tras `truncate(limit)` (líneas 696 y 1017); además `reinforce_node(…,1.02)` en 697/1018 | **Sí** — el search no filtra por tipo. Medido: 3 259 trazas `recall` sobre **473 nodos** distintos |
| MCP `tylluan_think` | `handler_think.rs:101` | nodos usados como contexto del think | Sí — medido: 20 trazas / 20 nodos |
| MCP `tylluan_graph` | `handler_graph.rs:58,59` (`add_triple` subj/obj) y línea 83 (`query` subj) | solo si el agente usa un id `coloquio_*` como sujeto/objeto de trípode | En principio sí; **en los datos: 0 trazas** de esos tipos sobre coloquio |
| REST (`api_v1`: `api_memory.rs`, `api_coloquio.rs`, `routes.rs`, `api_monitor.rs`, `api_security.rs`) | — | **0 llamadas a `touch_node`** en los cinco ficheros | **No: el recall REST no refresca FSRS nunca.** La flota (REST Bearer) no toca `fsrs_last_review` |
| Resto de los 34 llamadores (`outcome.rs:41,255,278,355`; `routing.rs:141,265,357,358`; `handler_remember.rs:380,434,480,497`; `api_guilds.rs:83`; `mcp.rs:623`; `mod.rs:324`; `handler_graph.rs:19,189`; `handler_recall` en agente…) | ver ficheros | id explícitos no-coloquio (`agent:{id}`, `lesson:…`, `episode…`, `task_id`, subj/obj de trípodes) | No — el formato del id lo excluye |

## 3. Cierre empírico del hueco «473 recall vs 235 con `last_review>0`» (copia `mode=ro` + backup API, producción intocada)

-Primera traza `recall` sobre coloquio: **2026-07-26**; primer `fsrs_last_review>0` en coloquio: **2026-09-02**. El repaso FSRS se cableó con la migración FSRS (comentario en `decay.rs` líneas 169-172, parámetro de vida media eliminado el 2026-08-24). **238 de los 473 nodos recordados tienen `last_review=0`**: son los cuyo último recall quedó antes del cableo — el desajuste es cronología del deploy, no una ruta perdida. Distribución de estabilidad en coloquio: 1 649 en 14.0 (nunca repasados o Good), 3 en 12.2 y 1 en 11.6.

## 4. Consecuencias para las conclusiones de H-49 (todas se mantienen, dos se fortalecen)

1. **Fórmula exacta**: `R(t) = 2^(−t/S)` (`lib.rs` línea 113/123) — sustituye mi aproximación de ley de potencias en H-49. Con S=14: R=0.1414 (peso<0.15) en t≈39.6 días… **si `apply_decay` corriera**, que en producción no corre (`decay_enabled=false`).
2. **El recall fabrica inmortalidad activamente**: Good mantiene S=14 y `last_review=now` deja R≈1 para nodos en uso; `reinforce_node` sube el peso `MIN(weight·1.02, 10.0)` (`nodes.rs` línea 839) — medido: **45 nodos coloquio con peso >1.0, máx 1.37**; y el propio UPDATE de reinforce refresa `updated_at`, que mantiene falsa la condición de edad de la poda (`updated_at > 30 días`, `decay.rs` línea 209). El nodo más recordado es el más inmune a las dos podas.
3. La poda de `coloquio_memory` sigue siendo imposible: S nunca baja de 7 (Good neutral; Again bajaría — pero no existe llamador con Again en producción: grep 0), y `prune_dead_nodes(0.05)` exige peso <0.05 que la subida por reinforce aleja.
4. La re-ingesta del flywheel **no** resetea FSRS: `ON CONFLICT` de `upsert_node_with_validity` (`nodes.rs` líneas 234-253) no incluye `fsrs_*` en su SET — descarte limpio de la hipótesis alternativa para el desajuste 473/235.

## Impact

Informe puro: cero código tocado, cero cargo (push197 en silencio respetado). Dos `## No verificado` de H-49 quedan cerrados con evidencia (fórmula FSRS exacta; rutas de recall que escriben `fsrs_last_review`). La conclusión de diseño de H-49 no cambia: la recomendación sigue siendo retención acotada explícita, no confiar en la poda. Afecta a quien toque `handler_recall`/`handler_think` (cada retorno de recall son 2 escrituras + trazas + FSRS por nodo devuelto: el coste por consulta ya está medido en H-42/43 como parte de la carga del kernel).

## No verificado

- [no verificado] Origen de los 4 nodos con S<14 (12.2×3, 11.6×1): con el código actual Good preserva S y Again la subiría o bajaría según dificultad, pero no hay llamador con Again; queda como legado de versiones/código anterior — falta opinión del TL.
- [no verificado] Qué filtro `type_filter` exacto usan los clientes MCP de recall (el search soporta `type_filter`; si algún agente lo usa, el subconjunto de coloquio tocado cambia — los datos miden lo que hubo, no lo que cada cliente envía).
- [no verificado] Los 34 llamadores se agrupan por patrón de id, no línea a línea en la tabla (los relevantes para coloquio están enumerados todos; el resto se excluye por formato de id).
- La copia de la BD es de ~14:0x con WAL; las cifras de trazas son de esa instantánea.
