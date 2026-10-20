# D-14/C1+C2 — Timeout coherente con la generación medida y una sola ranura

**Agente:** Deep (OpenCode) · **Fecha:** 2026-10-21 · **Base:** `origin/main` (sin cambios en los ficheros tocados desde `8001d23`)
**Parches:**
- `E:\\tylluan-patches\\deep\\D-14-C4.patch` — telemetría (sub-ítem anterior, 20.628 B)
- `E:\\tylluan-patches\\deep\\D-14-C1C2.patch` — **este sub-ítem** (6.945 B, 2 archivos, +100/−8) · **aplicar DESPUÉS del de C4**

**Estado:** implementado y verificado en local. **Sin commitear.**

---

## C1 — El timeout

| | Antes | Después |
|---|---|---|
| `JUDGE_TIMEOUT` | 30 s | **90 s** |

Medido en B-4 §5 (A/B real, mismo prompt): **36,8 / 40,5 / 56,0 / 59,8 s**. Con 30 s, tres de
las cuatro generaciones medidas morían por timeout, y el fallo sólo se escribía con
`tracing::debug!`. 90 s = 59,8 s + ~50 % de margen para un modelo más lento o para la
contención de CPU del reindexador (D-10/C5). No es "infinito": una generación que no
termina en 90 s ya no es una generación, es un cuelgue.

El valor vive en una sola constante (`JUDGE_TIMEOUT`) que usan la llamada y el health, así
que no pueden divergir. `timeout_ms` se reporta en `components.judge`.

## C2 — La concurrencia

`llama_backend` arranca el servidor **sin `--parallel`** (`llama_backend.py`), o sea **una
sola ranura de generación**. Lanzar N juicios a la vez no los acelera: el servidor los
encola dentro del modelo y cada uno espera la suma de los anteriores — que es exactamente
cómo un timeout de 30 s se convierte en "el juez falla siempre" bajo carga.

- **Semáforo de 1 permiso** (`JUDGE_MAX_CONCURRENCY = 1`), réplica del recurso real.
- **Espera acotada** (`JUDGE_QUEUE_WAIT = 60 s`): si no hay ranura en una generación
  completa, **este** juicio se salta (no se encola indefinidamente), y se cuenta como
  `busy_skip` en la telemetría. No es silencioso: el panel de C4 lo muestra.
- El permiso se pide en el wrapper (`call_reasoning_backend_with_grammar`), no en
  `call_judge_inner`: así los tests pueden ejercitar el HTTP con telemetría local sin
  pelearse por la ranura global.

### El coste, dicho en voz alta

Un `busy_skip` significa que **esa escritura concreta no pasó por el juez L2**. Es
fail-open deliberado:

- El escritor** no espera**: la puerta de escritura ya lanza el juicio en una tarea aparte,
  y el nodo ya está guardado. La cuarentena es un `UPDATE` posterior, no un gate previo.
  Así que "saltarse el juicio" retrasa o pierde la cuarentena de ese nodo, no bloquea al
  agente.
- Las heurísticas L1 (`poison_patterns`) siguen aplicándose siempre.
- La alternativa (cola sin límite) hace que los juicios lleguen minutos tarde y que la
  cola crezca sin techo: peor, y sin señal.
- Y ahora **se ve**: `calls`, `ok`, `failures`, `by_kind.busy_skip` e `in_flight` están en
  `/health?verbose=true`. Si el panel muestra `busy_skip` alto, el siguiente paso no es
  tocar el timeout, es darle más ranuras al servidor (`--parallel`) o más CPU.

## Verificación

```
cargo test -p tylluan-kernel --lib judge_concurrency_tests
  -> 3 passed (0,08 s)

cargo test -p tylluan-kernel --lib
  -> test result: ok. 1127 passed; 0 failed; 4 ignored   (34,35 s)   [1110 + 5 D-13 + 9 C4 + 3 aquí]

cargo clippy -p tylluan-kernel --all-targets -- -D warnings
  -> Finished … EXIT=0
```

Los 3 tests nuevos:

1. `timeout_covers_the_slowest_measured_generation` — el timeout **debe** superar 59,8 s
   (la generación más lenta medida); pinado a 90 s para que bajarlo sin querer rompa.
2. `one_slot_matches_the_server_started_without_parallel` — `max_concurrency == 1` y el
   semáforo arranca con un permiso: el semáforo y el `llama-server` real no pueden
   desincronizarse sin que alguien lo vea.
3. `bounded_wait_skips_and_counts_instead_of_queueing_forever` — con la ranura ocupada,
   una espera de 50 ms devuelve `None`, incrementa `busy_skip` exactamente en 1, y al
   liberar la ranura el siguiente juicio entra.

## Impact

- **Comportamiento:** bajo carga, los juicios se ejecutan **de uno en uno**; los que no
  consiguen ranura en 60 s se saltan y quedan contados. Antes se lanzaban todos a la vez y
  la mayoría fallaba por timeout sin dejar rastro.
- **Latencia percibida por el agente:** ninguna (el juicio ya era fire-and-forget).
- **Cuarentena:** los nodos juzgados con éxito se cuarentenan **antes**; con `busy_skip`
  pueden quedarse sin juzgar. Riesgo aceptado y visible, no silencioso.
- **Sin cambios** en `dev_mode`, umbrales de gates, `--parallel` del servidor ni valores
  por defecto. C3 (ventana de arranque / doble servidor) y C5 (contención del reindexador)
  **no** se tocan en este parche.
- **No-depredación (ADR-013):** 2 ficheros, ambos de seguridad; sin cambios en la firma
  pública de `call_reasoning_backend_with_grammar`.

## No verificado (declarado)

- Los drills de Buffy: no encontré `GUILD_ERROR|llama_backend|<err>` en ningún `kernel.log`
  accesible desde este worktree (`test-run-*` de octubre son del 6 y 9, anteriores al
  drill; el log de su worktree no es accesible desde aquí). Por eso C1/C2 se apoyan en las
  latencias medidas de B-4, no en el log del drill. Si el log llega y contradice estas
  cifras, el timeout es una constante: se cambia en una línea.
- Que con `--parallel 1` real el servidor no serialice antes de tiempo: la razón del
  semáforo es el código de `llama_backend.py` (nada de `--parallel`), no una medición mía
  del servidor bajo carga concurrente.
- La mejora de la tasa de juicios exitosos en producción: sólo se podrá ver con
  `components.judge` en un kernel real.
