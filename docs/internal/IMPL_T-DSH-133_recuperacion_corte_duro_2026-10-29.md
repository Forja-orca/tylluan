# T-DSH-133 — Recuperación tras el corte duro del apagado (`force_exit=true`)

- Agente: dsh
- Fecha: 2026-10-29
- Base: `origin/main = b9317b9` (push194). Sin código: solo informe y montaje de test-run.
- Encargo (T2080): en un kernel de PRUEBA (puerto y DATA_DIR propios) con `force_exit=true`, medir la recuperación tras el corte duro: carga con escrituras (`remember`/`coloquio`) en curso, matar con shutdown, rearrancar sobre el mismo DATA_DIR y comprobar `PRAGMA integrity_check` + `quick_check` en cada `.db` y que no se pierden escrituras confirmadas. **N=5 corridas.**

## Montaje

- Binario release propio de `b9317b9` (`CARGO_TARGET_DIR=E:/cargo-target/dsh`), puerto **47091**, `DATA_DIR` propio (`E:\tylluan-targets\dsh-t7-data\t133\data`), config de t123 + `[runtime] shutdown_timeout_ms=5000, shutdown_force_exit=true`.
- Por corrida: DATA_DIR limpio → arranque → **5 escrituras `POST /api/v1/memory/write` secuenciales** (200 confirmado, con embedding real) → **5 posts `POST /api/v1/coloquio/channels/general/post` concurrentes** (201) → a los 250 ms `POST /api/v1/admin/shutdown` (corte duro con fuerza) → **rearranque sobre el mismo DATA_DIR** → comprobación con el kernel ya arrancado (WAL recuperado) → parada.
- Comprobación: `PRAGMA integrity_check` y `quick_check` en **los 12 `.db`** del DATA_DIR, y presencia individual de cada escritura confirmada (marcador único `T133F<run> mem N` / `col N` en cualquier tabla/columna de texto).

## Resultados (5/5 corridas)

| run | confirmadas (200/201) | encontradas tras rearranque | perdidas | salida real | 2º arranque | integrity_check | quick_check |
|---|---|---|---|---|---|---|---|
| 1 | 10/10 | 10/10 | 0 | 1,38 s | OK | ok (12/12) | ok (12/12) |
| 2 | 10/10 | 10/10 | 0 | 1,47 s | OK | ok (12/12) | ok (12/12) |
| 3 | 10/10 | 10/10 | 0 | 1,69 s | OK | ok (12/12) | ok (12/12) |
| 4 | 10/10 | 10/10 | 0 | 1,41 s | OK | ok (12/12) | ok (12/12) |
| 5 | 10/10 | 10/10 | 0 | 1,23 s | OK | ok (12/12) | ok (12/12) |

- **0 escrituras confirmadas perdidas** en 5/5: 50/50 (25 `memory_write` + 25 mensajes de coloquio).
- **12/12 `.db` con `integrity_check=ok` y `quick_check=ok`** en las 5 corridas (agent_profiles, audit, contracts, curriculum, jobs, journal, mailbox, peers, pending_dispatches, silva, task_context, tylluan).
- Salida real con `force_exit=true`: **1,23-1,69 s** (coherente con T-DSH-132: ~1 s tras el corte; aquí algo más por la carga de escrituras en vuelo).
- El 2º arranque sobre el mismo DATA_DIR llega a `/health` correctamente en 5/5.

## Metodología: dos iteraciones descartadas (para que no se repita)

Las primeras versiones del montaje comprobaban las bases **inmediatamente después del corte, antes de rearrancar**. Eso produjo falsos negativos reales: varios `.db` daban `disk I/O error` al abrirlos (WAL pendiente de recuperar) y marcadores "perdidos" que sí estaban en el WAL. **El encargo pide comprobar tras rearrancar, y es lo correcto**: al abrir de nuevo el kernel se recupera el WAL y las escrituras confirmadas están todas. Los resultados de la tabla son los de la comprobación tras rearranque.

## Hallazgo colateral (fuera del alcance de T-DSH-133, pero real)

Durante un diagnóstico previo se detectó que `memory_write_handler` (`api_memory.rs:36`) genera el `node_id` como `memory_write:{timestamp_millis}`. **Con escrituras concurrentes en el mismo milisegundo, dos llamadas comparten id y la segunda sobrescribe a la primera**, ambas devolviendo 200: en el diagnóstico, 5 `POST /memory/write` concurrentes devolvieron 200,200,200,200,200 y en la BD había **4 nodos** con el kernel aún vivo (no es efecto del apagado). Recomendación: usar un id único (uuid/contador atómico) — no implementado aquí (fuera del encargo; solo informe).

## Impact

- Con el corte duro de T-DSH-132 (`force_exit=true`), **la recuperación es correcta**: 0 escrituras confirmadas perdidas en 5/5 corridas y 12/12 bases íntegras tras rearranque. El corte duro no deja el DATA_DIR corrupto en este escenario.
- La comprobación correcta es **después de rearrancar** (recuperación de WAL); comprobar antes produce falsos negativos y no mide lo que pide la tarea.
- Hallazgo colateral: colisión de `node_id` en `memory/write` con escrituras concurrentes (bug independiente del apagado).

## No verificado

- 5 corridas, un binario release propio, una sola máquina (Windows/CPU), sin guilds activos ni carga real de agentes; el volumen de escrituras es bajo (10 por corrida).
- Es **muerte de proceso**, no corte de energía ni `fsync` forzado: no se ha probado la durabilidad ante caída del sistema ni el estado del WAL tras un `power loss`.
- No se comprobó el contenido completo de las tablas (solo integridad y presencia de marcadores); no se verificaron índices FTS/embeddings por separado.
- Las escrituras **en vuelo no confirmadas** (sin 200/201 antes del corte) no se contabilizan como pérdida: pueden perderse, y en algunas corridas se perdieron (esperado).
- El hallazgo de colisión de `node_id` se observó una vez con 5 concurrentes; no se ha caracterizado su frecuencia ni se ha arreglado.
- No se tocó `E:\tylluan` ni el kernel de producción (:47004); el test-run usó su propio puerto y DATA_DIR y se paró al terminar.
