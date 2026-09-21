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

## 0. Marco universal — por qué esto NO es "un caso por IDE" (2026-09-21)

> José, tras ver el caso concreto de un modelo local vía `llama.cpp`,
> preguntó por la solución **agnóstica de cliente**, no una más de la
> lista: "esto ya está resuelto en la literatura de arquitectura de
> harness o multiagentes". Lo está — esta sección lo fundamenta con la
> propia especificación de MCP y con la literatura de sistemas
> multiagente de producción en 2026, investigación a fondo, no intuición.

### 0.1 El protocolo mismo ya descartó el "push nativo"

`sampling/createMessage` era la única vía a nivel de protocolo MCP para
que el **servidor** empujara trabajo al **cliente** sin que éste
preguntara primero. Dos hechos la descartan como solución:

1. **Deprecada en la spec 2026-07-28** (SEP-2577, MCP9005) — puede
   desaparecer en una versión futura.
2. **Soporte desigual incluso antes de deprecarse**: Claude Code nunca
   la implementó como cliente; OpenCode la sumó recién en abril 2026.
   Ningún diseño que dependa de ella habría sido universal ni siquiera
   en su mejor momento.

Más revelador todavía: la propia spec **se movió a stateless** en esa
misma versión — "al dejar de permitir que los servidores mantengan una
conexión abierta y empujen actualizaciones por ella, un primitivo
asíncrono basado en poll dejó de ser opcional: es la única vía que
queda" (MCP Tasks extension, `io.modelcontextprotocol/tasks`). Esto
**confirma que el diseño de Tylluan (cola en el kernel + spawn externo,
nunca una conexión viva empujando)** no es una limitación nuestra: es
la dirección hacia la que fue todo el ecosistema MCP, por las mismas
razones (fiabilidad, no depender de que una conexión siga viva).

Nota aparte: MCP Tasks (la extensión que sí sobrevivió) resuelve un
problema distinto — una tool call que tarda mucho y el cliente sondea
su progreso. No resuelve "despertar a un agente sin ninguna sesión
corriendo", porque sigue exigiendo que el cliente ya esté conectado y
preguntando. No es la pieza que falta aquí.

### 0.2 La literatura de sistemas multiagente: esto ya tiene nombre

El patrón que resuelve exactamente este problema es el **Actor Model /
árbol de supervisión** (Erlang/OTP, 1986; retomado como fundamento
explícito por los frameworks multiagente de producción en 2026, p.ej.
LangGraph 1.0 GA): *un agente se activa al recibir un mensaje, se
desactiva por completo cuando está inactivo (no queda ningún proceso
sondeando), y su estado vive en almacenamiento durable para poder
reanudar tras un crash o un reinicio.* La responsabilidad de decidir
"cuándo despertar a quién" nunca vive en el propio agente — vive en un
supervisor externo, siempre encendido, que no es él mismo un LLM.

Esto **es literalmente lo que BWC-1..4 ya implementa**, sin que lo
hubiéramos etiquetado así:

| Concepto del Actor Model | Pieza equivalente en Tylluan |
|---|---|
| Buzón del actor | Coloquio (mención = mensaje en el buzón) |
| Checkpoint durable de "hay trabajo pendiente" | `PendingDispatch` en `dispatch_queue.rs` (SQLite) |
| Supervisor que activa el actor | `dispatch_executor.rs::spawn_fixed_argv` |
| El actor en sí, dormido hasta ser activado | El proceso del agente — no existe hasta que se spawnea |

No hace falta inventar arquitectura nueva. La pregunta correcta no era
"¿qué mecanismo de protocolo despierta a un cliente?" (esa vía la cerró
el propio MCP), sino "¿qué comando OS arranca el proceso correcto para
cada tipo de agente?" — y esa pregunta sí tiene una respuesta agnóstica.

### 0.3 La taxonomía real: 2 clases, no N-por-IDE

El error de encuadre inicial era pensar en "un caso por IDE conectado".
Investigado a fondo (§2 y hallazgos de hoy), todo cliente MCP conocido
cae en una de solo dos clases — y la segunda clase tiene un único
representante universal que cubre cualquier IDE, conocido o no:

**Clase A — el propio producto trae headless/scheduling nativo.**
El `[wake].command` (o nada, si hay scheduling nativo) invoca
directamente ese producto. Ejemplos verificados: Antigravity (cron del
IDE), OpenCode (`opencode run "<prompt>"`), Claude Code (`-p`/`--print`),
Cursor (`cursor-agent -p "<prompt>"`), Codex CLI (`codex exec "<prompt>"`),
Gemini CLI (`gemini -p "<prompt>"` o `--non-interactive`, distinto del
`agy` de Antigravity que no existe en esta máquina).

**Clase B — el producto NO trae ningún camino headless propio**
(verificado hoy: Windsurf/Cascade — "su bucle agéntico está atado a la
superficie del IDE, no es el camino que optimiza"), **o no hay ningún
producto encima, solo un modelo servido crudo** (llama.cpp, vLLM,
Ollama, cualquier endpoint OpenAI-compatible). En ambos casos, la
solución es la misma y es **universal por construcción**: un host MCP
genérico que invoca el **modelo subyacente directamente por su API**
(local o cloud), sin pasar por el IDE en absoluto. Representante real,
maduro y MIT: **[`mcphost`](https://github.com/mark3labs/mcphost)**
(mark3labs, Go) — conecta cualquier modelo OpenAI-compatible
(`--provider-url`, incluido `llama-server`) a cualquier servidor MCP
por HTTP, corre el ciclo de tool-calling completo, y tiene modo headless
de un solo prompt: `mcphost -p "<prompt>" --quiet --provider-url <url>
--model openai/<modelo> --config <mcp-config>`.

**Por qué esto cierra el problema para siempre, no solo para hoy**: la
Clase B no depende de que nosotros conozcamos el IDE. Cualquier IDE
futuro que no traiga headless mode sigue teniendo, por definición, un
modelo detrás accesible por alguna API — y `mcphost` (o equivalente)
llega a ese modelo sin tocar el IDE. La única precondición real es que
el modelo del agente sea alcanzable por alguna API, local o remota — eso
es universal, no depende de que sea "conocido o más usado".

### 0.4 Qué significa esto para el diseño del dispatcher

Nada cambia en `dispatch_executor.rs` ni en el contrato de seguridad
(`[wake]` fijo + allowlist + aprobación hash-bound BWC-2) — eso ya es
100% agnóstico del cliente, spawnea `Command::new(program).args(...)`
sin saber ni importarle qué hay dentro. Lo único que crece con cada
cliente nuevo es **el catálogo de recetas de argv por Clase A conocida**
(§2) más **una única receta de Clase B ya cerrada** (`mcphost`) que
cubre todo lo demás, conocido o no. No hace falta un 6º sovereign tool

### 0.5 Corrección importante — spawn-por-mención descartado para trabajo continuo (2026-09-21)

> Ciclo real completado el mismo día que expuso el error de encuadre:
> se implementó `auto_approve` (§ver dispatch_queue.rs/dispatch_executor.rs,
> commit `88c2129`), se activó para Deep (`63d81af`) y Buffy (`324d176`),
> se corrigió un bug real de spawn (`513a389`, argv sin ruta absoluta a
> `opencode.cmd` — `opencode` a secas resolvía al shim PowerShell que
> `Command::new` no puede ejecutar), y **entonces José lo descartó como
> mecanismo para trabajo continuo**, con una objeción correcta que no
> habíamos visto: sin límite agregado de conversaciones simultáneas, un
> día activo de Coloquio podría disparar un arranque en frío de `opencode`
> por cada mención — "no quiero cientos de instancias de opencode
> hablándose". El rate limit de 10/agente/min no lo evita, solo lo
> ralentiza.

**Qué se congela, qué se queda:**

- `[wake].auto_approve` y el spawn-por-mención (BWC-1..4 completo) siguen
  existiendo en el código, PERO se reservan para su caso de uso correcto:
  una acción puntual, rara, con aprobación humana (BWC-2) — no el motor
  del trabajo diario del equipo.
- El campo `auto_approve` en `.tylluan/agents.toml` queda desactivado
  para Deep y Buffy (ambos lo desactivaron ellos mismos desde su propio
  lado, mismo patrón que su activación).

**El patrón correcto para "el equipo trabaja 24/7 sobre una lista de
tareas"** (José, la idea original, nunca abandonada — solo mal resuelta
la primera vez): **UN loop persistente y supervisado por agente**, no un
proceso nuevo por evento:

1. El agente despierta cada X tiempo (minutos, no segundos) por su propio
   mecanismo — cron del SO, tarea programada de Windows, el scheduler
   nativo del harness si lo trae (Antigravity ya lo hace así con su cron
   de IDE), o el equivalente de `/loop`/`ScheduleWakeup` que Claude Code
   ya usa internamente para este mismo propósito.
2. En cada despertar, hace **una sola llamada** — revisa Coloquio
   (mensajes no leídos, la lista de tareas del día), y si hay trabajo
   pendiente para él, lo hace; si no, vuelve a dormir. No se lanza una
   instancia nueva por cada mensaje individual — una iteración del loop
   puede procesar varios mensajes acumulados desde la última vez.
3. José y el tech lead generan juntos, cada día, la lista de tareas del
   equipo en Coloquio; cada agente la consume a su propio ritmo desde su
   propio loop.

Esto es exactamente el `coloquio_watcher.py` que falló operativamente en
2026-09-16/17 (documentado en `event_driven_agent_triggers_research.md`)
— **el patrón nunca estuvo mal, la ejecución sí**: era un script de
terminal sin supervisión, moría al cerrar la ventana, sin visibilidad de
si seguía vivo. La lección correcta no era "abandonar el loop por push
del kernel", era "supervisar el loop de verdad" — con las herramientas
que cada harness ya trae (cron nativo, tareas programadas del SO,
`/loop` de Claude Code), no con un script huérfano en una terminal.
para esto — el mecanismo universal ya vive fuera del contador de tools,
en el dispatcher.

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
