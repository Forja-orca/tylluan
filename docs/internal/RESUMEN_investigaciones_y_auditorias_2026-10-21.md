# Resumen de investigaciones y auditorías — 2026-10-21

**Autor:** TL (Claude). **Fuente de cada cifra:** el informe enlazado y, cuando se indica, una comprobación mía en este árbol. Estado a las 20:30 locales: `origin/main` en `514058e` con CI verde; producción sigue en `5e49bf8` (nada de lo de abajo está activo en el kernel de José hasta que él reinicie).

## 1. Qué cambió en `main` hoy (empujes 75 a 95)

| Bloque | Entrega | Estado |
|---|---|---|
| Aprobación de planes | D-13, D-16 (intención `approve action for plan`), D-26 (el audit nombra `operator`) | En `main`; matriz de audit de 13 filas sin mentiras |
| Memoria no confiable | D-17 (capa 1 en `ingest` y `memory/write`), D-18 (`untrusted_content` con banners, herencia y escritores), banner en `tylluan_think`, `@inbox` con payload antiguo, herencia retroactiva a resúmenes | En `main`: la **propagación** de la marca funciona. **Corrección del 22-oct:** el **marcado en escritura** cubre solo 2 de las 6 vías y la capa 1 es evadible (red-team de Buffy; D-31, D-32 y D-33 abiertos) |
| OAuth fase 2 | D-15 (página de consentimiento), D-25 (techo de fallos por IP con escalado, tope de pendientes, fail-closed ante fichero ilegible) | En `main`, **inactivo** hasta `dev_mode=false` (decisión de José) |
| Juez ASI06 | D-19 (`llama_backend` sin techo de 60 s; causa real distinta de la hipótesis C3), fuente única `CPU_INFERENCE_GUILDS` | En `main` |
| Índice FTS | D-23: un solo punto de borrado, contador real de desincronía, rebuild disponible y **sin ejecutar** | En `main`; reconstrucción de producción pendiente de José |
| Refactor | D-22a: `handler_do/mod.rs` de 2.783 a 1.531 líneas moviendo los tests | En `main` |
| Documentación | Quickstart en inglés (auditado: 46 afirmaciones, 4 falsas corregidas), MSRV 1.88 demostrado, el gate de docs vigila las cifras de guilds y tools del quickstart | En `main` |
| Tests | 1228 al empezar el día, 1261 ahora (lib 1174, enlace 75, fsrs 12) | Verificado con `scripts/check_test_count.sh` |

## 2. Auditorías adversariales (DSH, T-DSH-8 a T-DSH-25) — qué se probó y qué quedó abierto

| Informe | Resultado | Hallazgo que sigue vivo |
|---|---|---|
| T-DSH-9 (D-16) | Dos fallos corregidos | X1: `agent_id` declarativo (test ignorado) |
| T-DSH-10 (D-18) | 4 vectores abiertos: A4, A5, B2, C2 | B2: la localidad se decide por «existe una identidad»; **decisión de José** |
| T-DSH-11 | A5 cerrado; B3b es una carrera, no una propiedad | — |
| T-DSH-12, -13 | A4 y C2 cerrados | — |
| T-DSH-14 | OAuth: 7 vectores, 0 bypass | Riesgos de disponibilidad → D-25 |
| T-DSH-15 | B-12 se cumple con D-17; el choque de la spec de D-13 | Exigir operador en toda aprobación: **decisión de José** junto al binding |
| T-DSH-16, -17 | Lista única de guilds; juez: 13 casos sin fallo | — |
| T-DSH-18 | `consents.json`: 18 casos | Clave con separador sin escapar, revocación que fallaba abierta, permisos, carrera de `persist` → D-25 |
| T-DSH-19, -20 | Quickstart: 4 falsas corregidas; MSRV real 1.88 (1.85 falla) | `rust-version` sin declarar en `Cargo.toml` |
| T-DSH-21 | Audit de aprobaciones: filas idénticas por ruta, rechazos sin fila, REST sin ruta de aprobación | El operador queda como `anonymous` → D-26 (hecho) |
| T-DSH-22 | Escritores de federación y A2A: 10 filas | `untrusted_reason` falsificable → D-27 (en cola) |
| T-DSH-23 | **Q9: NO-GO** (+2,04 puntos, McNemar exacto p = 0,640) | El conjunto de 245 casos tiene 244 positivos y 1 negativo: no mide falsos positivos |
| T-DSH-24 | OAuth tras D-25 | 5 fallos locales = 30 s fuera para el operador (todos los clientes locales comparten `127.0.0.1`); un `consents.json` corrupto más una aprobación posterior pierde los consentimientos → D-29 (en cola) |

## 3. Investigaciones

- **Las preguntas correctas y el camino a v1.0** (`docs/reference/research/2026-10-21_preguntas_correctas_camino_a_v1.md`): 13 preguntas con evidencia graduada y criterio de cierre medible. Conclusiones que cambian el trabajo: la afirmación de la Raspberry Pi no está demostrada (el README dice que no se ha ejecutado en ARM físico); nunca hemos medido «con frente a sin Tylluan»; el hueco de seguridad es la identidad, no la detección; los modelos pequeños no han ganado a las reglas deterministas.
- **Triage de un análisis externo** (pegado por José): erró en dos puntos comprobables (la validez temporal con `valid_from` y `valid_until` y el reranker existen en el repo). Lo aprovechable queda como decisiones de arquitectura, no como trabajo.
- **Licencias de Decima y Clef** (Hermes): verifiqué por lectura directa que `decima-small` y `clef-flash` son Apache-2.0 y la cita sobre datos de entrenamiento de uso no comercial; el resto de la nota de Hermes queda como no verificado por el TL. Hermes tiene H-1 para cerrar la lista de no verificados (OWASP agéntico, Ley de Ciberresiliencia, SLMJury, Reglamento 2017/745).
- **Diseño de identidad D-24** (Deep): causa de las 490 filas sin atribuir (`agent_id` vacío en `tylluan_do`), el binding existe pero nunca rellena, no hay canonicalización (`deep` frente a `Deep`). Sin migración: la ejecuta José.
- **Índice FTS** (Deep, D-23): dos causas, no una (el contador contaba el contenido y no el índice; borrar del índice después de borrar el nodo es un no-op silencioso). 13.497 huérfanos en producción.
- **Causa real de los fallos del juez** (Deep, D-19): 30 fallos idénticos por el techo de 60 s del proxy; la hipótesis de un segundo servidor quedó refutada con el log.

## 4. Incidentes del día

- CI rojo en `a14dcdd`: un test con `#[cfg(unix)]` completo hacía que Linux contara un test más que Windows. Arreglado en `514058e`; regla nueva de flota: ningún `#[test]` con `cfg(unix)` ni `cfg(windows)` en la función entera.
- Lecciones de verificación: un `clippy exit 0` de un agente no coincidió con el mío; una afirmación de «gate rojo preexistente» no se reproducía en `origin/main`. Siempre se recalcula.
- Antes (madrugada): dos caídas de producción por un `Stop-Process` por nombre de imagen (regla: kernels de prueba solo se paran por su PID).

## 5. Decisiones que son de José (no se toman sin él)

1. Aceptar el NO-GO de los modelos pequeños en `hitl_resolve`.
2. Reconstruir el índice FTS de producción (13.497 huérfanos) tras reiniciar con D-23.
3. Las cinco decisiones de identidad del diseño D-24 (incluida la migración `deep`/`Deep`).
4. Eximir o no el loopback en el consentimiento OAuth.
5. El conflicto de la spec de Buffy con D-13 (exigir operador en toda aprobación).
6. El binding token a `agent_id` y su calendario.
7. La licencia: MIT hoy; AGPL obligaría a ofrecer el código de lo que se sirva por red.
8. Hardware ARM para la prueba del caso ancla.
9. El ADR de la red de equipo (destino del descubrimiento global y del chismorreo).
10. El reinicio de producción para activar D-15 a D-26 y, después de OAuth fase 2, `dev_mode=false`.

## 6. Trabajo en vuelo al cierre del día

Deep: cola 4 (D-27, D-22b, D-22c, D-28, D-29) más la megatarea nocturna. Buffy: megatarea nocturna. DSH: T-DSH-25 (sondas de continuidad). Hermes: H-1. Antigravity: sin cuota; el lote AK sigue pendiente de correcciones.
