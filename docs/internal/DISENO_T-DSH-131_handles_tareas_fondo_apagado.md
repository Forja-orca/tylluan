# T-DSH-131 — Diseño: registrar los `JoinHandle` de `main.rs` con nombres estables (para que `wait_for` no reciba lista vacía)

- Agente: dsh
- Fecha: 2026-10-29
- Base: `origin/main = 0441cda` (push191). Solo diseño/informe, sin compilar (silencio de carga T2062).
- Encargo: guardar los `JoinHandle` de las tareas de fondo de `main.rs` con nombres estables (lista del inventario Fase 0), ordenar por riesgo y decir cuáles son seguras de esperar sin abortar (ADR-025 R6).

## 1. Problema

`ShutdownDeadline::wait_for` (T-DSH-130) está implementado y testeado, pero `main.rs` sigue descartando los `JoinHandle`: los ~28 `tokio::spawn` de arranque no se guardan, así que en producción `wait_for` recibe `Vec::new()` y el deadline solo sirve para el corte duro. Sin handles no se puede:
- esperar de forma ordenada a las tareas que sí terminan solas,
- registrar por nombre las que no respondieron,
- medir qué tarea concreta retrasa el apagado.

## 2. Hecho de partida (verificado por lectura, `main.rs` @ 0441cda)

- Solo **dos** tareas observan cancelación: el servidor HTTP (`cancel_token`, ya unido por `wait_for_shutdown`) y la tarea de `doc_token` (`main.rs` 1272-1290, `select!` con `doc_token_inner.cancelled()`). El resto de bucles no comprueba ningún token (coincide con el hallazgo de Hermes: 27/28 bucles sin token).
- Hay tareas **one-shot** (duermen 5/10/20 s y terminan, descubren NAT, consolidan IVF una vez, cargan corpus semilla, descargan modelo, chequeo de versión) que son seguras de esperar.
- Hay bucles **periódicos** (intervalos de 5-600 s) que hoy no terminan solos: esperarlos consume el deadline entero y luego el corte duro los mata.
- Algunos `tokio::spawn` no son de arranque (p. ej. `Proposal Embedding` en 1025 se lanza por evento): **no** deben registrarse como tareas de fondo del apagado.

## 3. Diseño propuesto

### 3.1 `BackgroundTask` con política de espera

En `runtime/shutdown.rs` (extensión del módulo de T-DSH-130):

```
pub enum WaitPolicy { Wait, Drain, Skip }
pub struct BackgroundTask {
    pub name: &'static str,          // nombre estable
    pub subruntime: &'static str,    // Memoria/Inferencia/Procesos/Decision/Coordinacion/Red
    pub handle: JoinHandle<()>,
    pub policy: WaitPolicy,
    pub cooperative: bool,           // comprueba el token maestro entre unidades
}
```

`wait_for` pasa a recibir `Vec<BackgroundTask>`:
- `Wait`: se espera hasta el deadline (tareas cortas/cooperativas).
- `Drain`: se espera hasta el deadline pero **sin abortar**; si no termina, se registra por nombre (R6).
- `Skip`: **no se espera**; se deja morir con el proceso. Su handle se dropea (no se hace `abort`).

Devuelve `pending: Vec<String>` con los nombres de `Wait`/`Drain` que no respondieron; los `Skip` no cuentan como pendientes (se registran aparte como `skipped`).

### 3.2 Registro en `main.rs`

- Al inicio del arranque: `let mut background_tasks: Vec<BackgroundTask> = Vec::new();`
- Cada `tokio::spawn` de arranque pasa a:
  ```
  background_tasks.push(BackgroundTask::wait("nat-discovery", "Red", tokio::spawn(async move { ... })));
  ```
  (constructores `wait`/`drain`/`skip` para no repetir campos).
- **No** se registran: el servidor HTTP (1427, ya unido por `wait_for_shutdown`), el stdio (1668, consumido por `wait_for_shutdown`), ni los spawns por evento (1025 y similares).
- Al apagar: `let (pending, skipped) = deadline.wait_for(background_tasks).await;` y se pasan a `force_exit_after_shutdown` (log con `pending` y `skipped`).

### 3.3 Tabla de tareas y política propuesta (ordenada por riesgo de esperar)

Riesgo alto = esperar puede colgarse dentro de una unidad ONNX/SQLite (R6: no se puede abortar). Riesgo bajo = unidad corta o cooperativa.

| # | línea main.rs | nombre propuesto | subruntime | disparador | política | riesgo al esperar |
|---|---|---|---|---|---|---|
| 1 | 1918 | `reindexer` | Memoria | intervalo 600 s | `Skip` (hoy) / `Drain` (tras token) | **alto**: `spawn_blocking` ONNX dentro de la ronda; el guard ya avisa de rondas >120 s |
| 2 | 1876 | `flywheel` | Memoria | intervalo 60 s | `Skip` / `Drain` | **alto**: embebe propuestas (ONNX) y escribe SQLite |
| 3 | 1856 | `memory-decay` | Memoria | intervalo | `Skip` / `Drain` | **alto**: UPDATE masivo en SQLite |
| 4 | 2015 | `hnsw-rebuild` | Memoria | intervalo 600 s | `Skip` / `Drain` | **alto**: reconstrucción CPU/ONNX |
| 5 | 2100 | `consensus` | Memoria | intervalo 60 s | `Skip` / `Drain` | medio-alto: escrituras SQLite |
| 6 | 2138 | `night-consolidation` | Memoria | horario | `Skip` / `Drain` | **alto**: fases pesadas (ONNX + SQLite) |
| 7 | 2154 | `maintenance` (+2173 `prune`) | Memoria | intervalo 300 s | `Skip` / `Drain` | medio: prune escribe |
| 8 | 2044 | `embedding-migrate` | Inferencia | intervalo | `Skip` / `Drain` | **alto**: ONNX |
| 9 | 1822 | `hormones` | Coordinacion | intervalo 5 s | `Skip` | bajo-medio: estado en memoria |
| 10 | 1601 | `coloquio-loop` | Coordinacion | bucle | `Skip` | medio: escribe coloquio/SQLite |
| 11 | 1622 | `blackboard-loop` | Coordinacion | bucle | `Skip` | medio |
| 12 | 1253 | `metrics-interval` | Coordinacion | intervalo 60 s | `Skip` | bajo |
| 13 | 1314 | `ivf-interval` | Memoria | intervalo 6 h | `Skip` | bajo |
| 14 | 1247 | `registry-actor` | Procesos y guilds | bucle de mensajes | `Skip` | medio: gobierna guilds |
| 15 | 966 | `mailbox-listener` | Coordinacion | bucle de eventos | `Skip` | bajo-medio |
| 16 | 1456 | `always-on-guilds` | Procesos y guilds | una vez (arranque) | `Wait` | bajo: arranque corto |
| 17 | 1480 | `warm-pool` | Procesos y guilds | una vez | `Wait` | bajo |
| 18 | 1528 | `external-mcps` | Procesos y guilds | una vez | `Wait` | bajo-medio: puede esperar a un MCP externo; deadline lo cubre |
| 19 | 1274 | `doc-token-guard` | Coordinacion | intervalo 60 s **cooperativo** | `Wait` | bajo: ya comprueba `doc_token` |
| 20 | 1446 | `version-check` | Red | una vez | `Wait` | bajo |
| 21 | 1498 | `model-download` | Inferencia | una vez | `Drain` | medio: descarga de red; deadline |
| 22 | 1302 | `ivf-consolidate-once` | Memoria | una vez (arranque) | `Wait` | bajo-medio: escritura SQLite corta |
| 23 | 1707 | `seed-corpus` | Memoria | una vez | `Wait` | bajo-medio: upsert de semillas |
| 24 | 675 | `nat-discovery` | Red | una vez | `Wait` | bajo: STUN con timeout |
| 25 | 1798 | `retro-link-5s` | Memoria | una vez (sleep 5 s) | `Wait` | bajo |
| 26 | 1810 | `retro-link-20s` | Memoria | una vez (sleep 20 s) | `Wait` | bajo (20 s > deadline: quedará pendiente y se registra) |
| 27 | 2128 | `retro-link-10s` | Memoria | una vez (sleep 10 s) | `Wait` | bajo (10 s > deadline) |
| 28 | 1427 | `http-server` | Red | — | **no registrar** | ya unido por `wait_for_shutdown` |
| 29 | 1668 | `stdio` | Red | — | **no registrar** | ya consumido por `wait_for_shutdown` |
| 30 | 1025 | `proposal-embedding` | Inferencia | por evento | **no registrar** | no es tarea de arranque |

### 3.4 Por qué esta política (R6)

- **Nunca `abort`** sobre `Skip`/`Drain`: se dropea el handle y el proceso termina; no se interrumpe una escritura a mitad (R6).
- **`Skip` en los bucles sin token**: esperarlos hoy no aporta nada (no terminan) y consume el deadline; el corte duro los mata igual. Cuando se añada la comprobación del token maestro al inicio de cada bucle (fase 2 de ADR-025 R1), pasan a `Drain` y sí drenan entre unidades.
- **`Wait` en one-shot cortos**: reducen el riesgo de perder trabajo ya empezado (consolidaciones, semillas, descargas) sin colgar el apagado.
- **`Drain` en descarga/one-shot largos**: se espera hasta el deadline y se registra si no termina.

## 4. Tests propuestos (implementación futura)

1. Unitario: `wait_for` con `Wait` terminada + `Skip` colgada ⇒ `pending` vacío y `skipped` contiene la colgada; no se hace `abort`.
2. Unitario: `Wait` colgada 30 s con deadline 50 ms ⇒ aparece en `pending`.
3. Unitario: nombres duplicados en el registro ⇒ error en debug (o `debug_assert`), para no perder trazabilidad.
4. E2E (montaje T-DSH-123/130): con `force_exit=true` y una tarea `Skip` de 30 s ⇒ salida ~1 s y log con `skipped`; con `Wait` cooperativa ⇒ termina antes del deadline.
5. E2E: el log de apagado lista nombres de pendientes y no rutas/contenido (sin fuga).
6. Regresión: `[runtime] shutdown_timeout_ms=0` + `force_exit=false` reproduce el comportamiento histórico.

## 5. No verificado

- **Solo diseño**: no hay código ni compilación; la tabla de líneas/nombres es lectura de `main.rs` @ 0441cda (los números pueden moverse con el próximo push).
- La clasificación `Wait`/`Drain`/`Skip` es por lectura de código, no por medición: no se sabe cuánto tarda cada tarea en su unidad actual ni si alguna se cuelga de verdad.
- No se ha comprobado qué handles son `Send`/`'static` en cada sitio (algunos capturan `Arc` y closures; el registro puede requerir pequeños cambios de firma).
- No se ha medido el efecto de esperar 5 s a las tareas `Wait` largas (20 s) en el apagado normal: quedarían pendientes y el corte duro actuaría tras el deadline.
- El token maestro no lo comprueba ningún bucle hoy: la fase `Drain` real depende de añadirlo (ADR-025 R1/fase 2).
- No se toca `E:\tylluan` ni el kernel de producción (:47004).
