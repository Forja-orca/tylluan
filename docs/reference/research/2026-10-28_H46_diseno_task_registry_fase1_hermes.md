# H-46 — Diseño TaskRegistry mínimo (Fase 1 ADR-025) — hermes

**Asignación:** T2022, reconfirmada T2024. **Base:** `4db12a6` (worktree rebasado; REVIEW.md leído tercero). Solo diseño, sin código. 2026-10-28 ~20:5x.

## 1. Trait y tipos (bocetos de diseño, no implementación)

- `trait BackgroundTask`: `id() -> &'static str`; `subruntime() -> Subruntime` (enum R3, 6 variantes); `run(self: Arc<Self>, ctx: &TaskCtx) -> Result<()>` — el cuerpo es el bucle actual de cada tarea.
- `TaskSpec` (declaración R1): `trigger` (Interval{secs} | Event), `clave_cadencia: Option<ClaveConfig>`, `budget_class` (None|SharedBackground|Reservada), `guard` (timeout + extensiones = los actuales de GuardedTask), `shutdown_grace` (heredado del maestro si None).
- `TaskCtx`: clon del **token maestro** + `BackgroundBudget` + `GuardedTask` ya construido + `JoinHandle` interior. `TaskRegistry`: `register(spec, task)`, `start_all()`, `stop_all(timeout) -> Vec<String>` (nombres de las que no respondieron, para el log R4), `snapshot() -> Vec<TaskState>` (insumo directo de R5).
- Reutilización estricta: el registro **envuelve** `GuardedTask` + `BackgroundBudget`; no crea semáforos nuevos ni toca el supervisor de guilds (R2).

## 2. Token maestro de apagado (cierra el hallazgo 27/28)

- Hoy `cancel_token` (main.rs:468) solo lo observan HTTP y stdio. Propuesta: ese mismo token se pasa a cada `TaskCtx`; los bucles sustituyen `tick().await` por `tokio::select! { _ = token.cancelled() => return Ok(()), _ = tick => {...} }` (patrón ya vivo en Doctor main.rs:1277).
- Secuencia de apagado: tras `wait_for_shutdown` → `registry.stop_all(shutdown_timeout)` ANTES de reap de guilds → cancela, hace `join` con espera acotada por `timeout(shutdown_timeout, join)`, registra por nombre las que sobran (R4) y sigue con el orden actual (tunnel→guilds→services→pid).
- El caso ONNX: un batch ya dentro de `spawn_blocking` no es cancelable (semántica tokio, verificado en FASE0/guard). Doble contención: (a) el bucle comprueba el token **entre chunks** (≤32 textos; lote ONNX medido en H-45 2-8 s con max_threads=8 → residuo acotado); (b) el runtime se construye con `shutdown_timeout` de tokio para que el proceso salga aunque un hilo blocking remate. La tabla de T-DSH-123 (en curso) fija el valor por defecto: si el residuo medido es >10 s, subir `shutdown_timeout` o bajar el tamaño de chunk, no ambas.

## 3. Claves de config (defaults = valores actuales; todo opt-in, ADR-013)

| clave | default hoy | papel |
|---|---|---|
| `runtime.enabled` | — (no existe; false) | apaga el registro; con false arrancan los bucles viejos tal cual |
| `runtime.shutdown_timeout_secs` | 10 (criterio R4; revisar con T-DSH-123) | espera máxima del `stop_all` |
| `tasks.reindexer.interval_secs` | 600 (main.rs:1919, hardcodeado) | cadencia |
| `tasks.reindexer.batch_cap` | 100 (main.rs, `.take(100)`) | tope — la opción B de H-42 entra aquí |
| `tasks.reindexer.guard_secs` | 120 (main.rs:1932) | timeout del guard con las 3 extensiones actuales |

## 4. Migración del reindexer (primera tarea)

Mover el cuerpo de main.rs:1909-2001 a `ReindexTask` (subruntime Memoria) sin cambiar su lógica: `interval_at` sigue retrasando el primer disparo; por ronda: `budget.acquire` → `guard.run` → chunks de 32 con chequeo de token entre chunks; cap/intervalo/guard leídos de las claves §3. Con `enabled=false` queda el bucle actual intacto. Gana de regalo: telemetría R5 y la corrección del re-embalado queda visible por nombre en el apagado.

## 5. Tests con criterio medible

1. `TaskRegistry` + tarea ficticia **cooperativa** → `stop_all(2 s)` devuelve en <1 s, `stragglers` vacío.
2. Tarea ficticia **que ignora el token** → `stop_all(2 s)` devuelve en <3 s y `stragglers == ["falsa"]` (cubre R4 "registra por nombre").
3. `ReindexTask` con motor falso que duerme 30 s en blocking → apagado completa en <`shutdown_timeout` (prueba del caso ONNX con (a)+(b)).
4. Test de config: fixture toml cambia `interval_secs`/`batch_cap` → la tarea los lee (afirma valores, no comportamiento temporal).
5. Criterio de fase del ADR en CI equivalente: N rondas forzadas con cadencia 1 s y motor falso → `avisos del guard == 0`; y el gate real post-despliegue: `grep -c "Agnostic RE-INDEXING" kernel.log == 0` en 24 h + tiempo de parada <10 s medido en el corte real (comparar con el >85 s documentado §2 del ADR).

## Impact

Solo lectura (ADR-025 en `4db12a6`, `main.rs`, `background_budget.rs`, `guard.rs`, REVIEW.md) + este doc. Cero código, cero toques a E:\tylluan. El diseño mantiene defaults idénticos y `enabled=false`, así que ni aprobado ni implementado no cambia nada.

## No verificado

- [no verificado] El residuo real de `spawn_blocking` tras cancelación: hipótesis del TL (>85 s) en comprobación por T-DSH-123; el valor de `shutdown_timeout` queda condicionado a su tabla.
- [no verificado] Que `shutdown_timeout` del builder de tokio cubra todos los flavors de runtime usados por los binarios (no he leído cómo se construye el runtime del CLI).
- [inferencia] Nombre de fichero de esta entrega: la spec no lo fija; sigo el patrón H41-H45.
- La clasificación fina por subruntime de tareas mixtas (flywheel Memoria/Coordinación) sigue abierta para el TL (AMPLIACION T2021).
