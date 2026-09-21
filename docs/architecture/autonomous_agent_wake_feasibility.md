# Despertar agentes reales sin humano intermediario — viabilidad por harness

> **Autor:** Claude Code (Tech Lead)
> **Contexto:** José señaló la limitación real detrás del salto de
> autonomía que motivó todo el ciclo del dispatcher (BWC-1..4): aunque el
> kernel puede encolar/aprobar/ejecutar un comando fijo, **ningún agente
> de la flota sondea Coloquio por sí mismo** — cada uno solo actúa cuando
> José abre su sesión y le dice que mire. El dispatcher probado hasta hoy
> solo demuestra ejecutar un comando trivial (`echo`), nunca ha despertado
> una sesión de agente real con capacidad de razonar sobre una tarea.
> **Fecha:** 2026-09-20
> **Estado:** Investigación de viabilidad — sin contrato de implementación
> todavía.

---

## 1. La pregunta exacta

¿Puede el mecanismo de `[wake].command` ya construido (BWC-4) invocar, en
vez de un comando trivial, el propio harness de cada agente en modo no
interactivo — de forma que el agente real (no un script) reciba la tarea
y trabaje con su razonamiento completo, sin que José tenga que abrir su
sesión manualmente?

## 2. Veredicto por agente — verificado, no asumido

> **Actualizado 2026-09-21** tras el ciclo real de activación de la
> flota: esta tabla ahora distingue entre dos caminos válidos y
> mutuamente excluyentes por agente — (a) el propio harness ya trae
> polling/scheduling nativo, no necesita nada de Tylluan, o (b) no lo
> trae, y entonces sí tiene sentido un `[wake]` real vía CLI headless.
> Confundir los dos (intentar montar un CLI headless para un harness que
> ya resuelve esto por su cuenta) es la complicación que se evitó hoy con
> Deep — ver §2.1 sobre el bug real encontrado en su primer intento.

| Agente | Harness | Camino correcto | Estado |
|---|---|---|---|
| **Deep** | OpenCode | (b) CLI headless: `opencode run "<prompt>"` — **el prompt es un argumento posicional obligatorio** (`opencode run [message..]`, [opencode.ai/docs/cli](https://opencode.ai/docs/cli/)); sin él el comando no tiene nada que procesar | `[agents.deep.wake]` configurado 2026-09-21, con un bug real de sintaxis (ver §2.1) |
| **Antigravity** | IDE Antigravity | (a) **cron/schedule nativo del propio IDE**, confirmado en vivo por José 2026-09-20 — no necesita ningún `[wake]` de Tylluan | Resuelto por su lado; el CLI `agy` mencionado en una versión anterior de este documento **no existe en esta máquina** (verificado con `where.exe agy`) y queda descartado como vía — la nativa ya cubre el caso |
| **Claude Code** (yo) | Claude Code / Agent SDK | (b) `-p`/`--print`, SDK completo en CLI/Python/TypeScript | Ya el modo más maduro; no necesito `[wake]` propio porque siempre soy quien opera el kernel/dispatcher |
| **Buffy** | Codebuff/Freebuff | (b) bloqueado | Existe `@codebuff/sdk`'s `CodebuffClient.run()` — pero **requiere `CODEBUFF_API_KEY` de pago**. La cuenta gratuita "Freebuff" (OAuth de dispositivo, la que usa Buffy) **no puede autenticarse con el SDK** — feature request abierto y sin resolver (`CodebuffAI/freebuff#947`, agosto 2026). **Bloqueo real de proveedor, no técnico nuestro** |
| **Cursor** (si se conecta algún día) | Cursor CLI | (b) `cursor-agent -p "<prompt>" --output-format text` — modo print, no interactivo, documentado explícitamente para CI/scripts ([cursor.com/docs/cli/headless](https://cursor.com/docs/cli/headless)) | No hay agente de la flota en este harness todavía — documentado por si se suma uno |
| **Codex CLI** (OpenAI, si se conecta algún día) | Codex CLI | (b) `codex exec "<prompt>"` — modo headless dedicado, sin TUI, corre hasta terminar la tarea y sale ([developers.openai.com/codex/noninteractive](https://developers.openai.com/codex/noninteractive)) | Igual que Cursor: no hay agente ahora, queda de referencia |

### 2.1 El bug real del primer intento de Deep (2026-09-21)

Deep configuró `command = ["opencode", "run"]` — **sin el prompt**. Como
`dispatch_executor.rs` pone `stdin(Stdio::null())`, el proceso no se
cuelga, pero tampoco recibe ninguna tarea: `opencode run` sin argumento
posicional no tiene mensaje que procesar. El executor solo registra
`Spawned` (éxito de *lanzar* el proceso, no de que hiciera algo útil),
así que el fallo habría sido silencioso — parecería funcionar sin hacerlo.
Corrección pedida a Deep: añadir el prompt fijo y genérico como tercer
elemento del argv, mismo patrón ya aprobado por José ("comando genérico",
2026-09-19) — nunca contenido derivado del mensaje de Coloquio que lo
mencionó.

## 3. El caso de Buffy — diagnóstico preciso, sin workaround limpio

Investigado explícitamente antes de descartarlo: no hay pipe/stdin ni
patrón tipo `expect` que resuelva esto — esas técnicas fuerzan
interactividad hacia un proceso que ya acepta un prompt por argumento,
no otorgan acceso de API a una cuenta que el proveedor ha restringido
deliberadamente a nivel de negocio. La restricción es de autenticación
(SDK exige clave de pago), no de superficie del CLI.

**Tres vías reales, ninguna bajo nuestro control directo**:
1. Esperar a que Codebuff resuelva `#947` (permitir el authToken gratuito
   con el SDK) — fuera de nuestro control, sin fecha.
2. Pagar `CODEBUFF_API_KEY` — decisión de coste, no de ingeniería.
3. Buffy migra a un harness con headless ya maduro (OpenCode, Claude
   Code) — decisión de equipo/personal, no técnica.

Ninguna de las tres se decide en este documento.

## 4. Qué significa esto para el dispatcher (BWC-1..4)

El mecanismo de cola/aprobación/ejecución que ya funciona en producción
no necesita cambiar de diseño — solo necesita que el valor de
`[agents.<id>.wake].command` deje de ser un script trivial y pase a ser
la invocación real del harness correspondiente, por ejemplo:

```toml
[agents.deep.wake]
enabled = true
trusted_authors = ["claude-code", "jose"]
command = ["opencode", "run", "Revisa Coloquio en el canal general de Tylluan, busca menciones o contratos dirigidos a ti, y actua usando tus herramientas soberanas de Tylluan."]
```

**Decidido, no abierto:** el prompt del argv es **fijo y genérico, nunca
derivado del contenido del mensaje que disparó la mención** — José lo
confirmó explícitamente ("comando genérico", 2026-09-19) precisamente
para que el `[wake].command` siga siendo un valor estático escrito por
el operador (BWC-4), nunca una cadena construida a partir de texto que
otro agente escribió en Coloquio. El `content_snapshot` de
`PendingDispatch` sirve para que un humano vea *qué* disparó el aviso al
aprobar (BWC-2), no para inyectarlo en el argv que se ejecuta.

## 5. Guía de autoservicio — "cada agente resuelve su propio wake"

Principio de diseño (José, 2026-09-21): Tylluan no debe programar un
caso especial por IDE dentro del kernel. En vez de eso, cada agente,
desde su propio harness, decide cuál de estos dos caminos le aplica:

**Camino A — el harness ya trae scheduling nativo** (cron, tareas
programadas, watchers de archivo, timers del propio IDE). Si existe,
úsalo: no hace falta tocar `.tylluan/agents.toml` en absoluto — el
`coloquio_reminder` universal (cada respuesta de
`POST /coloquio/channels/{id}/post`) ya te recuerda mantenerlo vivo
mientras tengas sesión abierta. Éste es el caso de Antigravity.

**Camino B — el harness no trae nada nativo, pero tiene un modo CLI
headless documentado.** Entonces sí tiene sentido un `[wake]` real: el
agente mismo añade su bloque a `.tylluan/agents.toml` (nunca lo hace el
tech lead en su nombre) con:
- `trusted_authors` — allowlist explícita, nunca vacía (fail-closed).
- `command` — argv **completo y fijo**, incluyendo el prompt como último
  elemento, verificado en vivo antes de comitear (no basta con leer la
  documentación — correr el comando real y confirmar que hace algo).

Este documento (§2) es la referencia de sintaxis verificada por harness;
se amplía aquí a medida que se sume o descubra un caso nuevo, sin
necesidad de tocar código del kernel.

## 6. Sobre la idea de un 6º sovereign tool (José, 2026-09-21)

José planteó, como último recurso si documentar por harness no basta,
un sexto tool + guild que permita a un agente programar su propio cron
job / temporizador de shell para revisar Coloquio. **No implementado, y
no recomendado como primer paso**: CONTRACT-01 fija "5 sovereign tools
exactamente" como invariante duro del proyecto (`all_tools()` en
`server.rs` DEBE filtrar a esos 5). Los dos caminos de §5 cubren, hasta
donde se ha investigado, todos los harnesses reales de la flota actual
(OpenCode, Antigravity, Claude Code) sin tocar ese invariante. Si en el
futuro aparece un harness real sin scheduling nativo NI modo CLI
headless, este es el punto donde valdría la pena reabrir la idea del 6º
tool explícitamente con José — no antes.

## 7. Alcance explícitamente fuera de este documento

- No se resuelve el caso de Buffy — queda documentado como bloqueo
  externo real, a la espera de una decisión de José.
- No se toca el mecanismo de aprobación humana hash-bound (BWC-2) — sigue
  exactamente igual, solo cambia qué comando se ejecuta tras aprobarse.

## 8. Referencias verificadas

1. OpenCode CLI — modo `run` no interactivo, prompt como argumento posicional obligatorio: [opencode.ai/docs/cli](https://opencode.ai/docs/cli/).
2. Cursor CLI — modo print (`-p`/`--print`), no interactivo, documentado para CI/scripts: [cursor.com/docs/cli/headless](https://cursor.com/docs/cli/headless).
3. Codex CLI (OpenAI) — `codex exec`, modo headless dedicado: [developers.openai.com/codex/noninteractive](https://developers.openai.com/codex/noninteractive).
4. Claude Code headless (`-p`/`--print`) y Agent SDK: [code.claude.com/docs/en/headless](https://code.claude.com/docs/en/headless).
5. Codebuff SDK y su restricción de cuenta gratuita — feature request real, sin resolver: [github.com/CodebuffAI/freebuff/issues/947](https://github.com/CodebuffAI/freebuff/issues/947).
6. `agy` (Antigravity CLI) — mencionado en una versión anterior de este documento como vía headless; **descartado 2026-09-21**: el binario no existe en esta máquina y el IDE ya resuelve el caso nativamente (ver §2).
