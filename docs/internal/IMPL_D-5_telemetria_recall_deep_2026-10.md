# D-5 — Telemetría de uso de `tylluan_recall` (Deep, 2026-10-19)

**Estado: no verificado** por terceros. Diff **sin commitear** en el worktree
`E:\tylluan-wt-deep`, base `17465b9`.
Parches: `E:\tylluan-patches\deep\D-5.patch` y, con toda la cola,
`E:\tylluan-patches\deep\COLA_COMPLETA_deep_2026-10-19.patch`.

## Problema

La cola lo dice con números: **2504 llamadas a `tylluan_recall` frente a 121 a
`tylluan_remember`**. Eso puede significar tres cosas muy distintas y hoy no hay
forma de distinguirlas: (a) los agentes buscan mucho y encuentran poco, (b)
buscan mucho porque el contexto se pierde y repiten consultas, (c) buscan y
aciertan, en cuyo caso el problema está en la escritura. La cola pedía medir
antes de optimizar; esto es la medida.

## Qué se entregó

### 1. Módulo `crates/tylluan-kernel/src/recall_telemetry.rs` (nuevo)

Contadores **en memoria**, todos atómicos, sin I/O y sin persistencia:

| Contador | Qué responde |
|----------|--------------|
| `calls` | ¿se usa el recall? ¿cuánto? |
| `empty_calls` / `empty_ratio` | ¿devuelve algo o los agentes buscan en el vacío? |
| `latency_histogram` (10 cubos: `<=5ms` … `<=2500ms`, `>2500ms`) | ¿es rápido o cada recall cuesta cientos de ms? |
| `mean_latency_ms` | orden de magnitud, con un decimal |

Decisiones de diseño, explícitas:

- **Sin llamadas no hay métricas inventadas**: `empty_ratio` y
  `mean_latency_ms` son `null` hasta la primera llamada, en vez de `0.0` (un cero
  se lee como "todo va perfecto").
- **El histograma es un histograma**, no percentiles: 10 cubos suman exactamente
  `calls` (hay test), y no prometemos un p50 que no medimos. El cubo de
  desbordamiento recoge todo lo que pase de 2500 ms.
- **Latencias por debajo del milisegundo** caen en el primer cubo, no se pierden
  (test).
- **No se guarda el texto de la consulta ni el `agent_id`**: la métrica responde
  "cuánto y cómo", no "qué preguntan". Guardar queries sería telemetría de
  contenido de la memoria del usuario, que no es lo que se pidió.
- Instancia de proceso vía `OnceLock` (no un campo nuevo en `TylluanServer`):
  el handler del tool no recibe el estado HTTP, y así el handler y `/health` leen
  exactamente los mismos contadores sin pasar referencias nuevas por medio
  mundo.

### 2. Medición en `handler_recall.rs`

El cuerpo existente pasa a `handle_tylluan_recall_inner`; el `handle_tylluan_recall`
público lo envuelve, mide el tiempo y registra:

- `empty` = la respuesta es exactamente el mensaje canónico sin memorias. Para
  no comparar un literal duplicado, ese texto pasa a ser `pub const
  NO_MEMORIES_FOUND`, usado también donde se genera. Así la telemetría no se
  desincroniza si alguien cambia el mensaje: cambia la constante o el test lo
  canta.
- Se mide también la **ruta de caché** (la envoltura está por fuera), que es
  justo la que interesa comparar con la búsqueda real.
- Los `Err` de transporte **no** cuentan como llamada: mezclar fallos con
  búsquedas ensuciaría la media de latencia y el ratio de vacías.

### 3. Exposición en `GET /health?verbose=true`

Dentro de `components`, junto a `kernel`/`embeddings`/`silva`/`guilds`/`mesh`:

```json
"recall": {
  "calls": 2504,
  "empty_calls": 210,
  "empty_ratio": 0.084,
  "mean_latency_ms": 41.3,
  "latency_histogram": { "<=5ms": 12, "<=10ms": 80, "...": 0, ">2500ms": 3 }
}
```

Sin endpoint nuevo, como pedía la cola. Se lee con el health que ya se consulta.

## Verificación (salidas reales)

```
cargo test -p tylluan-kernel --lib recall_telemetry::                 → 5 passed
cargo test -p tylluan-kernel --lib recall_telemetry_counts            → 1 passed
cargo test -p tylluan-kernel --lib -- --test-threads=1                → 1094 passed; 0 failed; 4 ignored
cargo test -p tylluan-kernel --test health_sovereign_count --test integration_mcp -j 3  → 1 + 2 passed
cargo clippy -p tylluan-kernel --all-targets -- -D warnings           → EXIT 0
scripts/check_test_count.sh                                           → EXIT 0 (kernel=1094 link=75 fsrs=12 total=1181)
scripts/check_docs_reality.sh                                         → sólo el ❌ preexistente ROADMAP_O3.md:42
```

Tests nuevos: 5 en el módulo (límites de cubo, ratio/media, suma del histograma,
forma del JSON, sub-milisegundo) + 1 en `handler_recall` que comprueba que una
llamada real incrementa `calls` y `empty_calls` (con `>`, nunca `+1`, porque los
contadores son de proceso y otros tests corren en paralelo).

## Hallazgo colateral (no es de esta cola, pero afecta a todos)

La suite **en paralelo no es estable en esta máquina**, y no por mi cambio. En
tres pasadas completas con el comando canónico falló un test distinto cada vez,
siempre en ficheros que no toqué:

| Pasada | Test que falló | Fichero |
|--------|----------------|---------|
| 1 (en paralelo con clippy) | (nombre no capturado) | — |
| 2 | `memory::inference_budget::tests::condvar_waiter_does_not_poll_while_parked` | `inference_budget.rs` |
| 3 | `handler_do::audit::tests::audit_store_seam_write_and_read_land_on_same_file` + `handler_do::tests::test_lesson_penalty_on_failure` | `handler_do/audit.rs`, `handler_do/mod.rs` |
| 4 | `handler_do::tests::test_lesson_deprecated_on_low_success` | `handler_do/mod.rs` |

Serial (`--test-threads=1`): **1094/1094 en verde**. Evidencia de mecanismo, no
sospecha:

- El test del seam de auditoría hace `set_var("TYLLUAN_AUDIT_DB", …)` — variable
  de entorno **de proceso** — y su propio comentario admite que solo se protege
  de "un segundo usuario del seam", no de cualquier otro test que llame a
  `log_audit_entry` mientras tanto. En la pasada 3 la consulta devolvió 3 filas
  donde esperaba 1: las otras dos las escribieron tests concurrentes.
- Los tests de lecciones comparten un store global (`test_lesson_*`), mismo
  patrón.
- El de `condvar` mide que un hilo parked no haga `try_take`, con `sleep(50ms)`
  y un contador exacto de intentos: bajo carga (111-150 s de suite, con clippy
  y el kernel de este equipo en marcha) el conteo exacto se rompe.

Consecuencia práctica: `scripts/check_test_count.sh` **también** falla a veces,
porque corre la suite y cuenta los `passed`; una pasada con un flake reporta
1180 en vez de 1181 y marca MISMATCH contra la documentación. Me pasó una vez en
esta entrega y tuve que repetir la puerta.

Recomendación (no lo arreglo yo: es de otra zona y no está en la cola): aislar
los tests de `audit`/`lesson` (store propio por test, sin `set_var`, o
`serial_test`), y relajar o marcar `#[ignore]` el conteo exacto del test de
`condvar`. Mientras eso siga así, cualquier agente que reporte "suite verde"
usando el comando en paralelo está reportando una moneda al aire.

## Impact

- **¿Depreda a otro agente?** No. Módulo nuevo, un wrapper y una clave más en el
  health. El único cruce son las líneas de recuento, actualizadas en los 8 sitios.
- **¿Depreda a un usuario?** No: no hay I/O nuevo, no se guarda contenido de
  consultas, y los contadores viven y mueren con el proceso (reiniciar el kernel
  los pone a cero — deliberado, para no crear otro fichero que rotar).
- **¿Coste en el camino caliente?** Cuatro `fetch_add` relajados por llamada,
  despreciable frente a una búsqueda híbrida de decenas de ms.
- **¿Rompe consumidores del health?** Añade una clave dentro de `components`; no
  cambia ninguna existente ni el shape no-verbose (`HealthResponse`). Los tests
  de health pasan.
- **¿Se puede verificar sin mí?** Sí: los comandos de arriba, más
  `curl "http://127.0.0.1:47004/health?verbose=true" | jq .components.recall`
  contra un kernel arrancado y unas cuantas llamadas a `tylluan_recall`.

## Cómo usarlo para decidir (lo que la cola quería)

Con esto, la pregunta "¿por qué 2504 recalls y 121 remembers?" pasa a tener tres
respuestas medibles:

- `empty_ratio` alto (p. ej. >0.5) → los agentes buscan y no hay nada: el
  problema es de **escritura**, no de búsqueda.
- `empty_ratio` bajo y `mean_latency_ms` alto → encuentran, pero caro: optimizar
  el camino de búsqueda.
- `empty_ratio` bajo y latencia baja → el recall funciona; el desequilibrio
  2504/121 es de **uso** del agente, no del kernel.

Eso es lo que pedía el ítem: no adivinar.
