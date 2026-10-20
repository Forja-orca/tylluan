# D-14/C4 — Instrumentar el juez: llamadas, fallos por tipo, latencia

**Agente:** Deep (OpenCode) · **Fecha:** 2026-10-21 · **Base:** `origin/main` @ `8001d23`
**Parche:** `E:\\tylluan-patches\\deep\\D-14-C4.patch` (20.628 B, 4 archivos, +450/−12)
**Sub-ítem:** primero de D-14 (C4). C1+C2 irán en un parche aparte, C3/C5 después.
**Estado:** implementado y verificado en local. **Sin commitear.**

---

## Qué cambió

| Archivo | Cambio |
|---|---|
| `security/judge_telemetry.rs` (nuevo) | Contadores en memoria + latencia (media, máximo y 11 cubos) |
| `security/coherence_gate.rs` | La llamada al juez se instrumenta; se parte en `call_judge` (wrapper) + `call_judge_inner` (testeable) |
| `security/mod.rs` | Módulo nuevo |
| `transport/http/mod.rs` | `components.judge` en `GET /health?verbose=true` |

### Por qué C4 primero

B-4 midió 69 escrituras juzgadas y **no pudo atribuir ni un fallo**: el error del juez
se registraba con `tracing::debug!` y no había contadores. "El juez falló" y "el juez dijo
que estaba limpio" eran indistinguibles desde fuera. Arreglar el timeout (C1) sin esto
habría sido cambiar un número a ciegas.

### Contrato de la telemetría (sin contenido, jamás)

```json
"judge": {
  "calls": 0, "ok": 0, "failures": 0,
  "by_kind": { "timeout": 0, "http": 0, "transport": 0, "parse": 0, "busy_skip": 0 },
  "in_flight": 0,
  "latency_ms_avg": null, "latency_ms_max": 0,
  "latency_buckets": [ { "le_ms": 100, "count": 0 }, … { "le_ms": null, "count": 0 } ],
  "timeout_ms": 30000
}
```

- **Nunca** se guarda el prompt, el veredicto ni el texto del nodo. Los tests lo fijan:
  el JSON serializeado no contiene las claves `prompt` ni `verdict`.
- `busy_skip` se cuenta aparte de `failures` a propósito: cuando C1+C2 añada el semáforo,
  saltarse una llamada porque el juez está ocupado es una decisión nuestra, no un fallo
  del modelo. Mezclarlos volvería a hacer ilegible el panel.
- Los cubos de latencia van de 100 ms a 90 s porque la generación medida en B-4 iba de
  36,8 s a 59,8 s: con los cubos del recall (máximo 2,5 s) todo caería en el de overflow.
- `in_flight` usa un guard con `Drop`, así que se decrementa también si la llamada se
  cancela o hace panic. Es el número que dice si hay cola.

### Clasificación de fallos (la parte que B-4 no podía adivinar)

| Origen | Clasificación |
|---|---|
| `reqwest` timeout | `timeout` |
| `reqwest` connect/request error | `transport` |
| Status no-2xx | `http` (el mensaje de error incluye el código: `judge returned status 500`) |
| Body no-JSON | `parse` |
| JSON válido sin `content[0].text` ni `text` | `parse` — **no** un veredicto vacío |

La última fila es importante: antes, un 200 sin texto devolvía `Err("Empty response")`,
pero nadie lo veía. Y un veredicto vacío no puede tratarse como "limpio" sin decirlo.

`JUDGE_TIMEOUT` (30 s) queda como constante pública con su motivo escrito y es la que
reporta el health, para que la telemetría y la llamada no puedan divergir cuando C1
cambie el valor.

## Verificación

```
cargo test -p tylluan-kernel --lib judge_call_tests
  -> 5 passed (0,35 s)
cargo test -p tylluan-kernel --lib judge
  -> 10 passed (incluye los 4 de judge_telemetry y el canario de D-9)

cargo test -p tylluan-kernel --lib
  -> test result: ok. 1124 passed; 0 failed; 4 ignored   (35,86 s)

cargo clippy -p tylluan-kernel --all-targets -- -D warnings
  -> Finished … EXIT=0
```

Los 9 tests nuevos:

**`judge_telemetry`** (4): índice de cubo con cotas inclusivas y overflow; `in_flight`
balanceado incluso soltando el guard pronto; `observe` clasifica y agrega (media, máximo,
`by_kind`) sin filtrar texto; `busy_skip` no suma a `failures` ni a `calls`.

**`judge_call_tests`** (5): un `llama-server` falso en proceso (`127.0.0.1:0`) sirve las
cinco respuestas y se llama por `call_judge_inner` con una telemetría **local** — nada de
mutar `TYLLUAN_KERNEL_URL` global, que es veneno para tests paralelos.

1. `ok_call_is_counted_with_latency` — CLEAN, `calls=1`, `ok=1`, `in_flight=0`.
2. `timeout_is_counted_as_timeout_not_as_generic_error` — juez que tarda 800 ms con
   timeout de 100 ms → `by_kind.timeout=1`, `transport=0`.
3. `http_error_status_is_counted_as_http` — 500 → `by_kind.http=1` y el mensaje nombra 500.
4. `non_json_body_is_counted_as_parse_error` — `not-json` → `by_kind.parse=1`.
5. `response_without_text_is_a_parse_error_not_a_silent_empty_verdict` — `{"content":[]}`
   → `parse`, `ok=0`.

## Impact

- **Observabilidad:** `/health?verbose=true` gana `components.judge`. Sin endpoint nuevo,
  sin persistencia, reinicio limpio con el kernel (igual que `components.recall` de D-5).
- **Coste:** dos `AtomicU64` por llamada y un cubo; la latencia se mide con `Instant`
  alrededor del HTTP que ya existía. Cero I/O añadida.
- **C6 (recall Layer 4)** queda cubierto gratis: comparte `call_reasoning_backend_with_grammar`,
  así que sus fallos entran en los mismos contadores. El desglose por capa no existe; si
  el TL lo quiere, es un `label` más y otro parche.
- **Sin cambios** en `dev_mode`, umbrales ni valores por defecto. El timeout sigue en 30 s
  en este sub-ítem: cambiarlo es C1.
- **No-depredación (ADR-013):** `coherence_gate.rs` conserva la firma pública
  (`call_reasoning_backend_with_grammar`) y su comportamiento; sólo añade instrumentación
  y una función interna.

## Observación (no es mía, no la toco)

En una de las pasadas completas falló
`memory::silva::search::embedding_fingerprint_index_tests::fingerprint_gate_cost`
(`p95 < 50_000 us`), y en las dos siguientes pasó. Es un microbenchmark de tiempo con
umbral absoluto: bajo carga paralela es sensible. No toqué `search.rs`. Lo dejo anotado
para la cola de tests inestables (familia de D-8) por si el TL quiere estabilizarlo.

## No verificado (declarado)

- El health real contra un kernel vivo: no arranco procesos. La forma del JSON está
  fijada por los tests de `to_json`, y el cableado en `health_handler` es una línea.
- Que las cifras se muevan con un `llama-server` real: los tests usan un juez falso en
  proceso; la latencia y los fallos que se medirían en producción son los de B-4 (36,8–59,8 s).
