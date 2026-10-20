# Cola de trabajo de Buffy, tercera tanda — 2026-10-22

**Mecánica:** la de `COLA_buffy_2026-10-19.md`. Trabaja solo en `E:\tylluan-wt-buffy` (nada en el checkout compartido). `git fetch` y `git checkout --detach origin/main` al empezar, diff sin commitear, informe `docs/internal/IMPL_<item>_buffy_2026-10.md` con `## Impact` y "no verificado", mensaje `[COLA buffy] <item> terminado: <ruta>` y al final `[COLA buffy] COLA 3 COMPLETA`. Kernels de prueba solo en `test-run-*` con puerto y `DATA_DIR` propios, nunca el de producción. **Regla de José (2026-10-21):** se pueden generar y rotar tokens y secretos; la línea roja es que José nunca se quede sin acceso. Como Antigravity se queda sin cuota mensual, tu tanda ya no espera a su quickstart: el quickstart lo escribe Hermes.

## B-10 — Aceptación post-reinicio (cuando José reinicie el kernel con el commit nuevo)
Cuando `/health` de producción devuelva el commit de la tanda D-13/D-14 y el secreto JWT persistente, ejecuta, solo con lecturas contra producción, esta lista y entrega una tabla `comprobación -> resultado -> evidencia`:
1. `/health?verbose=true` trae `components.recall` y `components.judge` con sus contadores.
2. `GET /api/v1/memory/quarantine` responde 200 con el contrato (con token de operador) y 403 `insufficient_scope` con un JWT `scope=mcp`.
3. `/.well-known/oauth-protected-resource` con `Host` de `127.0.0.1` devuelve `resource` con ese origen.
4. Un 401 sin credenciales trae `WWW-Authenticate` con `resource_metadata` en `127.0.0.1`, no en `localhost`.
5. `node scripts/dashboard_smoke.mjs` contra producción: 26/26 y la tarjeta de cuarentena ya no dice «endpoint no disponible».
6. Existe `oauth_jwt_secret` en el directorio de datos y **no** se ha tocado el token maestro ni `.tylluan-token`.
Si algo falla, reporta antes que nada y no intentes arreglarlo.

## B-11 — Supervivencia del JWT a un reinicio, en un kernel de prueba (end to end)
Con un kernel de prueba en `test-run-jwt` (puerto y `DATA_DIR` propios, `dev_mode=true` porque es un kernel de prueba): obtén un JWT por el flujo de OAuth, para el kernel, arráncalo otra vez **con el mismo `DATA_DIR`** y comprueba que el mismo JWT sigue siendo válido. Repite con otro `DATA_DIR`: debe dejar de serlo. Borra `oauth_jwt_secret` entre arranques: el kernel debe arrancar igualmente y el JWT viejo no valer. **Criterio:** los tres casos con la salida literal; sin tocar producción.

## B-12 — Especificación ejecutable de D-17 (test primero, sin tocar `src/`)
Deep implementará la capa 1 en `ingest` y `memory/write` (D-17 de su cola). Escribe antes los tests, marcados `#[ignore = "D-17"]` como especificación: con `benchmarks/injection_corpus_v1.json` y `benign_corpus_v1.json` contra el router de producción en proceso: `memory/write` rechaza las 8 muestras que la capa 1 detecta; `ingest` las guarda marcadas como `untrusted_content` y NO rechaza; ningún documento benigno se rechaza ni se marca por ninguna de las dos rutas. **Criterio:** `-- --ignored` falla hoy por la razón correcta (pega la salida); la corrida normal sale en verde; clippy exit 0.

## B-13 — Portabilidad del e2e
La auditoría externa afirma que `tests/e2e/harness.py` fija una ruta de Windows; en el HEAD actual no se reproduce. Ejecuta el harness desde otro directorio y desde un clon limpio en una ruta con espacios, y entrega qué falla, con `archivo:línea` de cada suposición de ruta o de entorno. Sin arreglos: tabla.

## B-14 — Verificar el quickstart de Hermes (cuando exista)
Cuando Hermes entregue `docs/QUICKSTART.md`, síguelo paso a paso en un clon limpio con un kernel de prueba en puerto propio, sin saltarte ninguno, y entrega tabla `paso -> comando -> resultado real -> ¿funciona?`, con el tiempo total hasta el primer `tylluan_recall`. Lo que no se pueda ejecutar queda marcado, no supuesto.

**Exclusiones:** no tocar `src/`; no bajar umbrales de los gates; no `--no-verify`; no escribir en el checkout compartido.
