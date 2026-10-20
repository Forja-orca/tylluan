# D-10 — Por qué falla el 36 % de las llamadas al juez (investigación, sin arreglos)

**Estado: informe, cero cambios de código.** Base `origin/main` = `cad1477`.
Origen del dato: `docs/internal/IMPL_b4_coherencegate_puerta_escritura_buffy_2026-10.md` §5 y §6.4.

## 0. Resumen en una frase

El 36 % no tiene una causa única: hay **un desajuste aritmético verificable**
(timeout del cliente 30 s contra generaciones medidas de 36,8-59,8 s), **dos
mecanismos de arranque/cola verificables en código** que convierten ese desajuste
en error visible, y **un defecto de observabilidad** que impide atribuir
proporciones — por eso B-4 no pudo pasar de "25/69".

Nada de esto está medido en vivo por mí: no puedo arrancar un kernel de prueba en
este entorno (§5). Lo que sigue distingue **verificado en el repo** de
**hipótesis con evidencia parcial**.

## 1. Tabla síntoma → causa → evidencia → arreglo propuesto

| # | Síntoma observado (B-4) | Causa | Evidencia (verificada) | Arreglo propuesto |
|---|---|---|---|---|
| C1 | Latencia 34-88 s; veredicto perdido | **El juez se rinde antes de que el modelo termine.** El cliente HTTP del juez corta a **30 s**; la generación medida en esa misma máquina es **36,8 / 40,5 / 56,0 / 59,8 s** para 2-3 tokens | `coherence_gate.rs:167` → `.timeout(Duration::from_secs(30))`. B-4 §5 (tabla A/B, misma máquina, mismo modelo). El cliente del guild usa **120 s** (`llama_backend.py:708`) | Alinear el timeout con el del guild (120 s) o hacerlo config `[security] judge_timeout_ms` con default ≥ p99 medido. **No elegir el número a ojo**: primero C4 |
| C2 | 25/69 fallan; "la cola serializa" | **Concurrencia sin límite contra un servidor de un solo slot.** Una tarea `tokio::spawn` por escritura, sin semáforo; llama-server se arranca **sin `--parallel`** (1 slot por defecto). Con 69 escrituras, las peticiones hacen cola detrás de generaciones de 40-60 s: solo las primeras caben en 30 s | `write_gate.rs:57` (`tokio::spawn`, sin semáforo); `llama_backend.py:611-628` (sin `--parallel`/`-np`). Consecuencia aritmética: si T_gen≈50 s y hay 2+ en cola, la 2.ª ya supera 30 s | Semáforo de 1-2 en el juez + deadline propio por encima del timeout de red; medir `queue_wait_ms` (C4). Alternativa: cola FIFO con descarte declarado en vez de silencio |
| C3 | `GUILD_ERROR|llama_backend` en llamadas concurrentes **y secuenciales** | **Ventana de arranque de 15 s y abandono prematuro de un servidor que aún carga.** El bucle espera 30×0,5 s = 15 s y luego `RuntimeError`; si el puerto está ocupado pero `/v1/models` no responde en **1 s**, cambia de puerto y arranca **otro** servidor (dos cargadores compitiendo por CPU) | `llama_backend.py:645-660` (`did not start within 15s`), `:588-608` (sondeo de 1 s → `_find_free_port`). B-4 observó justo eso: "`llama-server ... port 9000` con reutilización" | Ventana de arranque 120 s; antes de cambiar de puerto esperar hasta 60 s al que ya está cargando (un modelo que carga no es "un proceso ajeno"); log `warn` con la causa real |
| C4 | El 36 % **no se puede explicar** (B-4 no pudo atribuir causas) | **El fallo es invisible por diseño.** El error sólo se emite con `tracing::debug!` (filtrado con el nivel por defecto) y no hay ningún contador: ni intentos, ni fallos, ni clases, ni latencias | `write_gate.rs:69-72` (`tracing::debug!("write_gate: judge call failed ...")`). `grep` de métricas en `write_gate.rs`: ninguna. B-4 §"Limitaciones": "requiere instrumentación que el encargo prohibió" | Replicar el patrón de `recall_telemetry.rs` (D-5, ya en main): `spawns / ok / err_http / err_timeout / err_guild / latencia p50-p95`, expuesto en `/health?verbose=true`. Subir el log a `warn` **sin contenido de memoria** (solo `node_id` + clase) |
| C5 | Latencias altas y variables (34→88 s) | **Contención de CPU del reindexador ONNX** (B-4 #7), **correlación, no causalidad probada.** El juez genera en CPU y compite por los mismos núcleos; el reindexador no converge | B-4 #7: 8.192 stale; ~94 nodos por vuelta; ciclos de guard de 7 min; recalls 20-70 s. Aritmética declarada: 8.192/94 ≈ **87 vueltas**; a 7 min por vuelta ≈ **10 h** de reindexado continuo | No es "el mutex ONNX" bloqueando al juez (son procesos distintos): es **inanición de CPU**. Propuesta: acotar `--threads` del servidor de inferencia y darle prioridad, o pausar el reindexado mientras hay cola de jueces. Decidir con C4 midiendo `queue_wait_ms` vs `gen_ms` |
| C6 | (riesgo, no síntoma de B-4) | **La capa 4 del recall comparte función, timeout y silencio.** Un solo `call_reasoning_backend_with_grammar` sirve a la puerta de escritura y al filtro híbrido del recall | `coherence_gate.rs:147-186`; los dos call sites de `handler_recall.rs` pasan por ahí | Un único "cliente de razonamiento" con timeout, reintento y telemetría compartidos. Arreglar C1/C4 una vez, no dos |

## 2. Lo que el código **descarta** (para no repetir hipótesis)

- **No es `max_tokens`**: corregido en `1a58e8c` (`JUDGE_MAX_TOKENS = 16`,
  `coherence_gate.rs:140`), con test de compilación (`write_gate.rs:129`). Ese era
  el hallazgo H1, no H4.
- **No es el `_is_port_open`**: parece invertido (`bind` → `True` si el puerto está
  libre), pero el uso es correcto: `not _is_port_open(port)` = "hay alguien
  escuchando" (`llama_backend.py:514-523`, `:646`). Nombre engañoso, lógica sana.
- **No es un 503 por "modelo cargando" en cada llamada**: existe el sondeo
  `/v1/models` (`:548-555`) precisamente para eso. El problema es su **timeout de
  1 s** en el camino de reutilización (C3), no la falta de sondeo.

## 3. Orden en que lo arreglaría (si me lo piden)

1. **C4 primero, sin discusión.** Hoy cualquier arreglo de C1/C2/C3 es a ciegas:
   sin clases de error ni latencias, no se puede saber si el arreglo funcionó. Es
   el mismo argumento que justificó D-5 para recall, y cuesta un `AtomicU64` por
   clase.
2. **C1 + C2 juntos** (timeout y semáforo se compensan: subir el timeout sin
   acotar concurrencia sólo cambia 25 fallos rápidos por 25 fallos lentos).
3. **C3** (arranque), que afecta a todo el que use inferencia, no sólo al juez.
4. **C5** al final y con decisión de producto: frenar el reindexado o acotar
   hilos es una política de recursos, no un bug.

## 4. Lo que falta para cerrar esto de verdad (una línea de log)

`GUILD_ERROR|llama_backend|<err>` se construye en `registry/guild_process.rs:762`
a partir del error que devuelve el guild. **Ese `<err>` es la causa literal** (un
503, un `did not start within 15s`, un `ConnectionResetError`…). El drill de B-4
dejó el `kernel.log` de `test-run-gate/`, pero **ese directorio no existe en este
worktree** (`Test-Path test-run-gate` → `False`), así que no puedo leerlo.

Pregunta concreta al TL: ¿se conservó `test-run-gate/work/kernel.log` de la fase
3? Con un `grep 'GUILD_ERROR'` sobre ese archivo, la atribución de C1/C2/C3 pasa
de hipótesis a hecho en menos de un minuto.

## 5. Reproducción que **no** he podido hacer

Requiere kernel de prueba vivo (`test-run-*`, puerto y `DATA_DIR` propios) y
`llama-server` real; en este entorno no arranco procesos (regla del workspace).
Un test unitario no sirve: sin backend, el juez devuelve error y todo da 0.

Procedimiento para quien pueda lanzarlo:

```bash
# A) secuencial: 10 tylluan_remember con agent_id, separados 1 s
#    -> grep 'write_gate' en el log: cuántos ok, cuántos error, con qué texto
# B) concurrente: 6 tylluan_remember simultáneos
#    -> misma cuenta; comparar tasa con A
# C) con reindexado activo vs detenido (si se puede parar)
#    -> atribuye C5 (contenido) frente a C3 (arranque)
# D) con el timeout del juez a 120 s (una línea, build local)
#    -> si los fallos caen a ~0, C1 queda demostrado
```

## Impact

- **¿Código?** Ninguno. Este informe no toca `crates/`, ni `guilds/`, ni config.
- **¿Depreda a otro agente?** No. Lee `write_gate.rs`, `coherence_gate.rs`,
  `llama_backend.py` y el informe de Buffy; no modifica ninguno.
- **¿Qué cambia para el usuario?** Nada todavía. La consecuencia práctica de C1/C2
  es que **la cuarentena de la capa 2 depende de la suerte de la cola**: en un
  equipo cargado, el juez falla y un nodo hostil queda sin marcar. Cerrar D-9 (H2)
  aumentó el número de escrituras juzgadas, así que este riesgo **sube** hasta que
  C1/C2/C4 estén resueltos.
- **¿Coste de arreglarlo?** C4 es pequeño y sin riesgo (contadores + un `warn`).
  C1 es una constante. C2 introduce un semáforo en un camino fire-and-forget
  (revisar que no se acumulen tareas en espera). C3 toca el arranque de
  `llama-server` para todos los guilds de inferencia: es el único con superficie
  de regresión real.
