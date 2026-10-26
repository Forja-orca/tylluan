# IMPL H-34 — sincronización ROADMAP/STATUS con T-DSH-85/86/87, D-59, M-BUFFY-10 y cifras (hermes)

Spec T1773, vigente T1774/T1778. Base `5d21cc7` (origin/main al cerrar; incluye e6e19c5, 07bbbad, 31b965c, 4e2304e, 5d21cc7 — la resincronización que el propio TL pedía al cerrar ya está hecha). **Comparte el diff de H-32/H-33** (ROADMAP +14, STATUS +3/−1, guardian +56 — revisión conjunta, un solo diff sin commitear).

## Qué cubre del enunciado

- T-DSH-85 (hallazgo) + T-DSH-86/D-59/M-BUFFY-10 (cierre, push 158 `346cf65`) + T-DSH-87 (extensión del guard a `/api/v1/*` mutante, e6e19c5): sección nueva del ROADMAP "Closures of 2026-10-24 and 2026-10-25", con anclas `auth.rs:751` (middleware), `mod.rs:1522` (wiring), `auth.rs:761` (bloque `let guarded` REST), `mcp.rs:284` (415 Content-Type).
- tests 1374: verificado por Hermes ejecutando `check_test_count.sh` contra `346cf65` (exit 0, "1374 … all green"). Nota de discrepancia con el "1364" de T1757 ya reflejada en ROADMAP.
- "push 156-159": usé hashes de commit porque desconozco la numeración de pushes del TL tras el 158 (No verificado, abajo).

## Guardian R4

Exit 0 con MIS ediciones sobre `5d21cc7` (28 anclas OK, 13 antes del diff). Salida guardada: run completo `RESULT: all README claims verified against the tree`.

## Flag a la bandera 2 del TL (R5 vs cobertura REST)

R5 no falsea la cobertura REST: comprueba la constante `MCP_ORIGIN_GUARD_PATHS` (sigue `[&str; 5]`, auth.rs:672) frente a rutas registradas CON handler `mcp_handler|sse_handler`. Las rutas REST mutantes de e6e19c5 no usan esos handlers, así que no entran en el conjunto y no generan DRIFT. Evidencia: run limpio exit 0 con e6e19c5 ya en el árbol (base `5d21cc7`). La extensión por método (`let guarded`, auth.rs:761) la vigilo solo si el TL acepta la R6 opcional propuesta en IMPL_H-33.

## Impact

Docs + guardian, cero runtime. Si el TL integre este diff, ROADMAP/STATUS quedan como fuente canónica de los cierres del 24/25-oct y R4 vigila las anclas en CI al cablearse.

## No verificado

- Numeración de pushes 156/157/159 (no encontré el mapeo push→commit en el canal; anclado a hashes).
- Que 31b965c/4e2304e no cambiaran el recuento de tests (no reejecuté el conteo tras esos commits; la línea del TL en STATUS ya dice 1374).
- CI en `5d21cc7` (el TL afirmó verde en T1778 para `31b965c`; no consulté Actions).
