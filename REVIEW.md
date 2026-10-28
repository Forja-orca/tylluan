# REVIEW.md — como se revisa en Tylluan

<!-- RUTAS-DE-LECTURA:BEGIN (bloque identico en todos los ficheros agenticos; lo vigila scripts/check_read_routes.sh) -->
## Rutas de lectura (orden obligatorio, rutas desde la raiz del repo)

| Puesto | Fichero | Que te da | Quien |
|---|---|---|---|
| 1 | tu fichero de entrada: `CLAUDE.md`, `AGENTS.md` o `.agents/AGENTS.md` | reglas de tu runtime | Claude Code lee `CLAUDE.md`; Antigravity lee `.agents/AGENTS.md`; cualquier otro agente (Codex, OpenCode, Deep, Mimo, DSH, Hermes, Qwen) lee `AGENTS.md` |
| 2 | `STATUS.md` | estado real verificado: que funciona hoy | todos |
| 3 | `REVIEW.md` | como se revisa y que cuenta como hallazgo, antes de tocar o entregar nada | todos |
| 4 | `docs/internal/WORK_PROTOCOL.md` | ciclo de trabajo, entregas y reglas de flota | todos |
| 5 | segun la tarea: `SPEC.md`, `ROADMAP.md`, `CONTRIBUTING.md`, `SECURITY.md`, `docs/reference/adr/ARCHITECTURE_DECISIONS.md` | contrato, plan, contribucion, seguridad, decisiones | quien lo necesite |

`REVIEW.md` es siempre el tercer fichero que se lee. `CLAUDE.md` lo importa con una linea `@REVIEW.md`; el resto entra por esta tabla.
<!-- RUTAS-DE-LECTURA:END -->

> Para quien revisa o entrega: agentes, humanos y, si se activa, el servicio Code Review de GitHub (lee este fichero desde la rama por defecto protegida; `/code-review` local NO lo lee, por eso `CLAUDE.md` lo importa). Solo reglas que cambian la revision; el contexto general vive en `CLAUDE.md` y `AGENTS.md`.

## Que significa Importante aqui

Importante = rompe comportamiento, filtra datos, viola un invariante, abre un hueco de seguridad, impide un rollback, o afirma una cifra sin su comando. Estilo, nombres y refactors son Nit como mucho. Maximo cinco Nits por revision.

## Invariantes (violarlos es siempre Importante)

- Seis tools soberanas exactamente: `tylluan_do`, `tylluan_remember`, `tylluan_recall`, `tylluan_think`, `tylluan_graph`, `tylluan_decide`.
- `vector_dimensions = 1024`, nunca 768. Puerto unico del kernel, sin proxy.
- Nunca `host = "0.0.0.0"` con `dev_mode = true`. Nunca tokens en ficheros trackeados.
- `[inference] device` gobierna toda inferencia local; ningun guild pesado se auto-invoca sin opt-in explicito; la nube solo por opt-in por punto (ADR-020).
- No bajar timeouts de guilds de inferencia en CPU. Dashboard con `pnpm`, nunca `npm`.
- El logo de la lechuza (`dashboard/public/tylluan-logo.jpg`, `dashboard/public/forja-logo.png`) es identidad del equipo: ningun cambio de UI, marca, README o Tauri lo borra ni lo sustituye.
- Todo commit responde la no-depredacion (ADR-013); tocar `transport/`, `guilds/`, `integrations/` o `tylluan*.toml` exige `## Impact` en el mensaje.

## Siempre comprobar

- Que el diff hace lo que dice el informe: se lee el diff, no el mensaje. Un informe sin diff no se acepta.
- Que cada cifra viene con su comando y que la recalculas: clippy `-D warnings` en 0, tests, gates `scripts/check_*.sh`.
- Que el recuento de tests coincide en `README.md`, `README.es.md`, `AGENTS.md` y `STATUS.md` (`scripts/check_test_count.sh`).
- Que no hay `--no-verify`, ni secretos, ni rutas de otras maquinas, ni `:NNNN` que parezca un puerto en la documentacion.
- Que la entrega trae `## Impact` y un apartado `No verificado` con lo que no se probo.
- Que lo borrado se ha mirado antes: `git diff --stat` no puede eliminar ficheros que nadie pidio quitar.

## Evidencia antes de publicar un hallazgo

- Toda afirmacion sobre comportamiento lleva `fichero` y `linea N` del codigo, no una inferencia por el nombre.
- "No existe" se comprueba con busqueda sobre todo el arbol.
- Un hallazgo que cita historial cerrado como abierto se descarta tras `git fetch` contra `origin/main`.

## No reportar

- Lo que ya impone CI: formato, lint, tipos.
- `dashboard/dist/`, `Cargo.lock` y otros ficheros generados.
- Tests o ejemplos que violan a proposito una regla de produccion, si lo dicen.
- Documentos historicos marcados como tales.

## Por area

- Seguridad (auth, rutas HTTP, ejecucion de procesos, secretos, nube): subir un nivel y exigir prueba con peticion real.
- Memoria y recall: comprobar que no se reduce la fiabilidad del Coherence Gate sin medida.
- Dashboard: textos en espanol, un solo acento (salvia), estados por simbolo y palabra, nada de naranja decorativo.
