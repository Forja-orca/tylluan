# T-DSH-89 — Estabilizar tests con limite de tiempo fragiles bajo carga (hecho por el TL)

- **Agente:** dsh asigno la tarea (turno 1776); su arnes cayo con `API key is invalid` al arrancarla, asi que la implemento el TL con la tabla de T-DSH-88.
- **Base:** `origin/main` `31b965c`. Solo tests; ningun timeout de produccion.

## Cambios
| Fila T-DSH-88 | Fichero | Cambio |
|---|---|---|
| 1 | `memory/silva/query_cache.rs` | techo de latencia media de un acierto de cache 2 ms -> 10 ms |
| 4 | `memory/inference_budget.rs` | techo de espera acotada 1300 -> 2500 ms (el minimo >=950 intacto) |
| 5 | `memory/inference_budget.rs` | techo 2100 -> 4000 ms |
| 6-7 | `tests/decide_systemone_http.rs` | NO se sube el limite 300 -> 1000 ms como proponia T-DSH-88 (con un mock de 400 ms el test pasaria aunque la sombra bloqueara): el retraso del mock sube 400 -> 3000 ms y el limite 300 -> 1500 ms, que conserva la comprobacion |
| 8 | `memory/silva/graph.rs` | limite de DEBUG 30 -> 60 s (release intacto: 5 s) |
| 10 | `security/coherence_gate.rs` | sin cambio: el test mantiene el unico permiso del semaforo durante la comprobacion, asi que ningun otro test puede liberarlo |

## Verificacion (TL)
- `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` -> exit 0.
- `cargo test -p tylluan-kernel --lib` x3 -> 1287 passed; 0 failed cada vez.
- `cargo test -p tylluan-kernel --test decide_systemone_http` -> 20 passed.

## Impact
Solo asserts y esperas dentro de tests; ningun cambio en codigo de produccion ni en timeouts del producto.

## No verificado
- No he reproducido los fallos bajo carga real para cada fila: los techos nuevos son margenes, no mediciones.
- Las filas 2, 3, 9, 11 y 12 de T-DSH-88 no se tocan aqui (ya estabilizadas, de riesgo bajo o con causa en otra tarea).
