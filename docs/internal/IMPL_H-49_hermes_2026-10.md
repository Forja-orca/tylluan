# H-49 — Diseño de la excepción de poda de `coloquio_memory` (solo informe)

**Asignación:** T2083 (TL claude-code). **Base:** `d4b8868` (push195; `git rev-parse --short HEAD` confirmado — ver turno COMPLETA en Coloquio). **Sin cargo** (solo informe; el TL dejó «cargo libre» en T2084 pero este informe no lo usa). Todas las líneas reancladas en `d4b8868` con grep/sed (las de H-48 se movieron +13 con push194: el flywheel ya no está en 1876 sino en línea 1891). Convención de referencias suelta: `línea NNNN` (adoptada tras ver el ajuste del TL en push195 para pasar el gate de docs).

**Cómo se midió (metodología, para reproducir sin tocar producción):** copia de `E:/tylluan/data/silva.db` (205 MB) con la API de backup de SQLite desde una conexión `file:…?mode=ro` hacia un fichero en el directorio temporal — jamás se copiaron `.db-wal`/`.db-shm` a pelo ni se abrió la BD de escritura. Todas las cifras vienen de `SELECT` sobre la copia (instantánea 12:47, WAL incluido). El store de Coloquio vive en `mailbox.db` (`main.rs:705` `mailbox_path`, línea 713 `ColoquioDb::new(&mailbox_path…)` — no en `coloquio.db`, que está a 0 B); se leyó igual, `mode=ro`.

## 1. El gap 7 se invierte con la config de producción: hoy la poda NO puede tocar `coloquio_memory`

Tres bloqueadores independientes, cada uno verificado en código y en los datos de la copia:

1. **`apply_decay` no corre.** `tylluan.toml` de producción, línea 282: `decay_enabled = false`; el bucle de decay de `main.rs` está gateado (línea 1871 captura `config.silva.decay_enabled`, línea 1873 `if !decay_enabled` sale). Sin decay, el peso nunca se recalcula: FSRS manda (`decay.rs:141` `apply_decay`; el parámetro de vida media se eliminó el 2026-08-24 — comentario en decay.rs líneas 169-172 — y el campo de config es compatibilidad muerta).
2. **La poda interna de decay no dispararía igualmente.** Su condición (decay.rs línea 206-210) exige `fsrs_stability < 7.0`; el default de esquema es 14.0 (`schema.rs:75`) y el mínimo medido en los 1 653 nodos `coloquio_memory` es **11.56**. Falso siempre. Además 1 418/1 653 tienen `fsrs_last_review = 0` (default `schema.rs:77`) → `elapsed_days=0` → R=1 → peso 1.0 (lógica decay.rs línea 182-191).
3. **La poda del bucle de mantenimiento tampoco.** `prune_dead_nodes(0.05)` (llamada en `main.rs:2190`, sin gate, definición `maintenance.rs:470`) exige `weight < 0.05`. Histograma medido en la copia (1 653 nodos): **1 649 con peso ≥ 0.99, 3 en 0.5-0.99, 1 en 0.05-0.15 (0.1478), 0 por debajo de 0.05**. Candidatos hoy: **0**.

Conclusión honesta: **la excepción que pide la spec (protected=1 o exclusión por tipo) es innecesaria hoy y contraproducente como política**: añadiría inmunidad legal a un tipo que ya es inmortal de hecho. El riesgo real que muestran los datos es el opuesto: **crecimiento sin cota + pérdida de señal de recencia**.

## 2. Cifras reales (copia de la BD de prueba)

| medida | valor |
|---|---|
| nodos totales en silva.db | 10 775 (BD 205 MB) |
| nodos `coloquio_memory` | **1 653** (15.3 % de los nodos) |
| rango de vida | 2026-07-05 → 2026-10-28 (116 días) |
| ritmo | media 14.2/día; racha de flota 25-28 oct: 877 en 4 días (**~220/día**, pico diario 464) |
| proyección | 5.2k nodos/año (media) → **80k/año** al ritmo de octubre |
| coste por nodo | contenido medio 1 397 B (total 2.31 MB) + embed 4.1 KB (41.7 % tienen) + sparse + FTS + fila ≈ **7.5 KB/nodo** → 40 MB/año (media) a **~600 MB/año** (ritmo flota) |
| coverage del store | `mailbox.db`: 2 896 mensajes vs 1 653 nodos = **57 %**; el hueco (1 243 turnos) es anterior al flywheel (mensaje más viejo 2026-06-24, nodo más viejo 2026-07-05) — no es poda |
| backlog de embeddings | **964 sin vector** (689/1 653 con) — mismo orden que los 963 de H-44; el gap 2 de H-48 sigue vivo |
| canales | general 755 / equipo 491 / mision-activa 352 nodos |
| señal de recencia | 1 649/1 653 con peso ~1.0: el peso deja de diferenciar turnos recientes de los de julio |

## 3. Las dos opciones de la spec, evaluadas

- **A. `protected = 1` en la escritura** (añadir flag al `NodeWriteOptions` y columna en el INSERT de `upsert_node_with_validity`, ~3 líneas): salta decay (consulta decay.rs línea 154 `protected = 0`) y ambas podas (`protected = 0` en decay.rs línea 207 y `maintenance.rs` línea 475). Contra: sella la inmortalidad sin cota; un canal cerrado no limpia nada (retención/privacidad en un producto MIT público); el `ON CONFLICT` de peso (`nodes.rs` línea 227) preserva el peso de los protegidos, así que ni la re-ingesta los toca. Veredicto: **no** — es la bala correcta contra un enemigo que no existe.
- **B. Excepción por tipo vía `DURABLE_SUMMARY_EXCLUSION`** (`silva/mod.rs:95-96`; los usos `type NOT IN {…}` son 5: 3 en `decay.rs` (líneas 206, 815 y `prune_cold_nodes`) y 2 en `maintenance.rs` (líneas 474 y la de `purge`)): un solo término más y `coloquio_memory` queda exento en todas las rutas de poda. Veredicto: **no ahora** — mismo efecto de inmortalidad total con radio mayor (afecta a cualquier poda futura que reutilice el const). Útil solo si el TL activa `decay_enabled=true` Y las mediciones de logs muestran podas de estos nodos; hoy: 0 candidatas y la condición de estabilidad (≥11.5) lo impediría años (R FSRS ≈ (1+t/9S)^-0.5 → bajar de 0.15 con S≈11.5 exige ~6 años — fórmula del crate, aproximada).

## 4. Propuesta recomendada (C): retención acotada, opt-in

Lo que los datos piden no es proteger sino **limitar**:

- Clave nueva `silva.coloquio_retention_turns` (default `0` = apagado, patrón ADR-013).
- Con N>0, eliminar `coloquio_memory` cuyo `json_extract(metadata,'$.turn')` quede a más de N turnos del máximo por canal (`json_extract(metadata,'$.channel_id')` agrupa). Con N=3 000 el techo es ~3 000×n-canales (hoy 5 activos) ≈ 15k nodos ≈ 110 MB — y los resúmenes `consolidated_summary` nocturnos (ya en la lista durable) son la capa de memoria larga real.
- Ejecutar en NightConsolidación (semáforo de fases existe) o en el bucle de mantenimiento **no** cada 300 s: `json_extract` sobre 80k filas no es para un tick corto. Pasar por `delete_nodes_where` (guarda anti-deriva del FTS en el test de `fts.rs` línea 489: todo `DELETE FROM nodes` debe ir por ahí) y replicar la cascada de `prune_dead_nodes` (edges, `node_embeddings`, `node_sparse_embeddings` — maintenance.rs líneas 478-488).
- Coste estimado: ~25-40 líneas con su test en silva/tests.rs + la clave en config; cero cambio de semántica con default apagado.

**Descarte de A y B como paquete único:** con 0 candidatos de poda medidos y 964 pendientes de embed, legislar inmunidad solo congela el crecimiento (600 MB/año en la era de flota) y la dilución de recencia (1 649 pesos idénticos).

## Impact

Informe puro: cero ficheros de código tocados, cero cargo. Producción solo se leyó (copia `mode=ro` + backup API; config `mode=ro`/grep). Afecta a: decisión de ADR-025 sobre activar decay (el `decay_enabled=false` de producción es el hallazgo central), H-44/H-48 gap 2 (backlog de 964 medido vivo), y cualquier trabajo futuro sobre NightConsolidación/retención. Riesgo del informe: ninguno por sí solo; documenta 3 bloqueadores de poda que el propio repo no tiene escritos en ningún doc.

## No verificado

- [no verificado] La fórmula exacta de retrievability del crate `tylluan_fsrs` (la proyección «~6 años con S≈11.5» usa R=(1+t/9S)^-0.5; no leí el crate completo).
- [no verificado] Qué llamadores exactos revisaron los 235 nodos `coloquio_memory` con `fsrs_last_review>0` (solo confirmé un `touch_node` en `api_guilds.rs:83`, que apunta a nodos `agent:`; la ruta de recall no apareció en mi grep — falta el mapa de `touch_node`).
- [no verificado] Que los 1 243 mensajes sin nodo sean todos pre-flywheel (lo infiero por fechas extremo-a-extremo; no recorrí el git history del deploy del flywheel).
- `created_at` del store está con tipos mezclados (enteros unix y texto conviviendo; el propio código lo gestiona con `get_flexible_created_at`, `coloquio.rs:85`): los MIN/MAX de esa tabla son orientativos.
- La copia es de las 12:47 con WAL; dos horas de escritura no están incluidas. El ritmo de octubre puede no sostenerse (incluye rachas de automatización H-4x).
- Si el TL prefiere B como solución nominal «por coherencia con durable summaries», es barata (1 línea) pero debe ir SIEMPRE acompañada de C; sola, garantiza el crecimiento que mide este informe.
