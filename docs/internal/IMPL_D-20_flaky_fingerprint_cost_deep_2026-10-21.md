# D-20 — Test inestable `fingerprint_gate_cost`

- **Fecha:** 2026-10-21
- **Agente:** Deep (OpenCode)
- **Cola:** `docs/internal/COLA_deep_2026-10-21.md`, ítem **D-20**
- **Patch:** `E:\tylluan-patches\deep\D-20.patch` (1 archivo, +31/−15)
- **Archivo:** `crates/tylluan-kernel/src/memory/silva/search.rs` (`embedding_fingerprint_index_tests::fingerprint_gate_cost`)

## El fallo

`assert!(p95 < 50_000)` — un umbral **absoluto** sobre el p95 de una medición de tiempo. En la
suite completa (miles de tests en paralelo, varios de ellos cargados de CPU) una pausa de
planificación entra en el p95 y el test falla sin que la puerta de huella haya cambiado. Ocurrió
una vez en esta misma sesión (pasada con `1141 passed; 1 failed` antes de D-19) y la cola lo
documenta como "una de tres pasadas completas bajo carga".

El test, además, medía la puerta primero y la referencia después, en dos bloques separados: bajo
carga cambiante cada bloque veía una máquina distinta, así que ni siquiera el cociente era estable.

## El arreglo

Sin `#[ignore]`: la propiedad sí se puede medir de forma estable si se mide **en relativo**.

- **Medición intercalada**: en cada iteración se mide primero la puerta y después la referencia
  (`get_node_sync` × 300 candidatos, la resolución por candidato que ya existía en los bucles
  HNSW/IVF antes de la puerta). Las dos ven la misma contención dentro de la misma iteración.
- **Aserción relativa con margen explícito**: `gate_p95 <= max(reference_p50 * 4, 2_000us)`.
  - `MARGIN = 4` — margen holgado (medido: la puerta es ~5,6× más rápida que la referencia).
  - `FLOOR_US = 2_000` — si la máquina está ociosa y la referencia es diminuta, el cociente sería
    ruido de reloj; 2 ms para 300 candidatos es de sobra.
- El `println!` sigue existiendo y ahora imprime también el presupuesto calculado, para que un
  fallo diga de inmediato si el problema es la puerta o la máquina.

La propiedad que el test protege (la puerta acotada no añade un orden de magnitud sobre la
resolución que ya se hacía) se sigue detectando: una regresión a escaneo completo la haría superar
4× la referencia con cualquier carga.

## Verificación

Medición en máquina ociosa (`--nocapture`):

```
T1277-E fingerprint_gate_cost: rows=1000 candidates=300 iters=100 gate_p50=2009us gate_p95=2256us
(min=1787us max=2907us) — reference get_node_sync loop p50=12575us, budget=50300us (4x, floor 2000us)
```

Cinco pasadas consecutivas de la lib completa (paralelismo por defecto), como pide el criterio:

```
pasada 1: test result: ok. 1142 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 29.73s
pasada 2: test result: ok. 1142 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 31.47s
pasada 3: test result: ok. 1142 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 29.93s
pasada 4: test result: ok. 1142 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 29.12s
pasada 5: test result: ok. 1142 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 29.90s
```

`cargo clippy -p tylluan-kernel --all-targets -- -D warnings` → EXIT 0.

## Impact

- **Solo test.** No cambia código de producción, ni umbrales de la puerta, ni comportamiento.
- **Riesgo asumido:** al ser relativo, un p95 malo que afecte por igual a la puerta y a la
  referencia pasaría (por ejemplo, machine throttling global). Se acepta: ese caso no dice nada
  sobre la puerta, y el suelo de 2 ms acota el extremo contrario.
- Sin dependencias nuevas, sin procesos, sin tocar `dev_mode`/tokens/puertos.

## No verificado

- No he reproducido la contención exacta de una pasada completa de CI (los 5 runs son locales, con
  la misma suite completa en paralelo). Si vuelve a fallar en CI, el `println!` con el presupuesto
  dirá si fue la puerta o la máquina — pegar esa línea en el informe del fallo.
