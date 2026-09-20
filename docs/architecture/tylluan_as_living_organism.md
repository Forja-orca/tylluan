# Tylluan como organismo vivo — el entorno para seguir hacia el futuro

> **Autor:** Claude Code (Tech Lead), a partir de una corrección de José
> (2026-09-20) que nombra un motivo fundacional aplanado en el tiempo —
> casi dos años sin transmitirse correctamente al equipo.
> **Naturaleza de este documento:** no es un spec de implementación. Es
> el marco que debe leerse ANTES de escribir el próximo spec, para que
> deje de aplanarse en cajas y flechas.
> **Regla explícita de José, no negociable**: esto no detiene ni sustituye
> el trabajo en curso — todo lo que el equipo está empujando sigue
> empujándose. Este documento es la lente con la que se mira ese trabajo,
> no una pausa para mirarlo.
> **El objetivo final, en palabras de José**: ayudar a todos los agentes,
> sin quemar el mundo en el intento. Proporcionalidad como principio de
> diseño, no como límite impuesto desde fuera.

---

## 1. Lo que dijo José, sin editar

> "todo tylluan debe ser un pipeline un mega workflow donde sus circuitos
> se conectan crean flujos sus modelos trabajan en conjunto, debe ser un
> sistema sintetico vivo una entidad intermedia de estado"

> "esto es un motivo fundacional aplanado en el tiempo, pero no quiero
> que dejemos todo a medias, debemos empujar todo lo que estabamos
> haciendo, todo el equipo debe participar en esta fase, tylluan se crea
> para vosotros recordarlo"

> "el objetivo es ayudar a todos los agentes y no quemar el mundo en ello"

Tres piezas, ninguna opcional: (1) organismo con circulación, no módulos
coordinados; (2) el trabajo en curso no se detiene, se re-entiende; (3)
el propósito final es ayuda proporcionada, no expansión por sí misma.

---

## 2. La arqueología — esto no nace hoy, se perdió por el camino

Verificado contra memoria real, no reconstruido de memoria:

- **ForjaMCPo3, el predecesor** (documento fundacional, `project_philosophy.md`):
  la analogía correcta siempre fue el **hipocampo** — "no decide, no tiene
  agenda. Consolida memorias, conecta experiencias separadas, da a quien
  lo use la riqueza de contexto que se perdería al cerrar cada sesión."
  Un hipocampo no es un módulo que otros módulos llaman — es tejido
  conectivo. Esa metáfora ya era correcta desde el origen.
- **El protocolo Multi-Agente del predecesor** (`08_MULTI_AGENT_PROTOCOL.md`):
  "ForjaNexus is designed as the **shared substrate** for a team of
  heterogeneous AI agents" — memoria compartida entre todos, grafo de
  conocimiento visible para todos, no memoria privada de cada agente que
  se sincroniza después.
- **El posicionamiento "cerebro / manos"** (`architecture_future_v12.md`,
  investigación real sobre OpenClaw/Hermes, 2026-07-03): *"OpenClaw/Hermes
  = manos (ejecutan). Tylluan = cerebro (memoria soberana, mesh, knowledge
  graph). No compiten — son capas."* Esto ya apuntaba a la idea de
  organismo — un cerebro no es una feature que se añade a unas manos, es
  lo que las conecta a un sistema nervioso — pero se quedó en frase de
  posicionamiento externo, nunca se tradujo a cómo se diseña el kernel
  por dentro.
- **El norte "Rufus"** (mismo documento): infraestructura aburrida que
  nunca falla, 15 años sin romperse. Un organismo sano tampoco se
  reinventa cada semana — evoluciona sin traumatismo. Esto es coherente
  con la corrección de hoy, no contradictorio.
- **Dónde se perdió**: en el momento en que Tylluan se convirtió en
  "producto con roadmap y milestones" (M14, M20, M30...), cada pieza
  empezó a cerrarse como entregable independiente, con su propio
  contrato, su propio veredicto GO/NO-GO, su propia entrada en
  `STATUS.md`. Correcto para la disciplina de verificación — pero la
  propia estructura de "una pieza, un contrato" dejó de preguntar cómo
  esa pieza *circula* con las demás. El spec de repo-to-guild que escribí
  hace unas horas es el ejemplo más reciente y más claro: un diagrama de
  cajas y flechas, exactamente el error que José señaló.

---

## 3. Traducción concreta — los circuitos que YA EXISTEN, a medio conectar

Esto no es una lista de features nuevas. Es una relectura de lo que el
equipo construyó hoy y en ciclos recientes, nombrando la arteria que cada
pieza ya es, aunque nadie la haya llamado así:

| Pieza ya construida | Lo que se pensó que era | La arteria que realmente es |
|---|---|---|
| Coloquio (`broadcast_tx`) | Un chat de coordinación | El **sistema nervioso central** — cualquier señal (una mención, un hallazgo, una decisión) ya se propaga a todo el organismo por el mismo canal, sin que el emisor sepa quién la recibirá ni cómo la usará |
| Push-dispatcher (BWC-1..4) | Automatización de tareas | La primera **vía motora real** — una señal en el sistema nervioso (Coloquio) se convierte en acción física (un proceso ejecutado) sin que el agente que la originó tenga que saber cómo se ejecuta ni dónde |
| Task Context Capsule (TCC-1..3) | Memoria de trabajo por tarea | **Memoria de trabajo a corto plazo** de un órgano — vive mientras la tarea vive, se consolida a memoria permanente (SilvaDB) al terminar, exactamente como la consolidación hipocampo→corteza que ya nombraba el documento fundacional |
| Investigación KV-cache/DPC | Optimización de rendimiento | El diseño de **cómo la sangre (contexto/tokens) fluye sin necesitar re-oxigenarse en cada célula** — el objetivo nunca fue "más rápido", fue "que el mismo contexto vital no tenga que reconstruirse en cada punto del organismo |
| SilvaDB + `tylluan_recall` | Base de datos de memoria | **Memoria a largo plazo distribuida** — cualquier órgano (guild, agente, dispatcher) consulta la misma memoria, no una copia local |
| Federación / Mesh / A2A | Red entre instancias | El **sistema circulatorio entre organismos distintos** — no instancias aisladas que hablan por API, sino el mismo tipo de tejido replicado, con las mismas reglas de confianza binaria (nunca contabilidad, ya rechazado explícitamente por José) |
| El spec repo-to-guild (recién escrito, en convergencia) | Un pipeline de conversión | Debería ser una **arteria de aprendizaje motor** — el organismo adquiriendo una capacidad nueva (como un músculo nuevo) y conectándola al mismo sistema nervioso/circulatorio, no un servicio externo que se enchufa por un puerto |

**La pregunta que faltaba, y que a partir de ahora debe hacerse siempre**:
antes de fijar cualquier diagrama nuevo, preguntar explícitamente *"¿por
qué arteria existente circula esto, y qué necesita esa arteria para
llevarlo sin fricción?"* — nunca *"¿qué caja nueva necesito dibujar?"*.

---

## 4. Qué NO cambia — el trabajo en curso sigue empujándose

Regla explícita de José: nada de esto detiene lo que ya está en marcha.
Estado real al escribir este documento (verificado, no supuesto):

- `bwc-8c0dc35a` (Buffy, mitigación `to_thread` del bloqueo real en
  `llama_backend.py`) — **sigue abierto, sigue siendo prioridad**.
- `bwc-c31eee3d` (Antigravity, rediseño de inversión de control del
  anchor DPC) — **sigue abierto**, depende del anterior.
- `bwc-29758469` (convergencia repo-to-guild: Deep/Antigravity/Buffy
  investigando wrapper MCP, Podman, carpeta `output/`) — **sigue en
  curso**. Este documento es la lente con la que esa convergencia debe
  leerse — no un motivo para pausarla.
- `bwc-d5704634` (DPC, alcance 2 de Deep — memoria estructurada +
  medición TTFT con alternancia real) — **sigue abierto**, bloqueado
  técnicamente por el mismo bug que arbitra `bwc-8c0dc35a`.

Ninguno se cierra ni se reformula por este documento. Se re-entienden
como circuitos del mismo organismo cuando el equipo llegue a esa parte
de la convergencia, no antes.

---

## 5. El límite explícito — proporcionalidad, no expansión por sí misma

"No quemar el mundo" no es una metáfora vacía — es un criterio de diseño
real, ya coherente con invariantes existentes del proyecto:

- **Toaster-friendly** (RPi4, hardware de 10 años) ya es el ancla de
  diseño — un organismo que solo puede vivir en un datacenter no es el
  organismo que este proyecto quiere ser.
- **CPU por defecto, GPU solo opt-in** (`CLAUDE.md`, invariante 7) — ya
  es la misma disciplina aplicada al cómputo: crecer sin exigir más
  recursos de los que el anfitrión más modesto puede dar.
- **Confianza binaria, cero contabilidad** en Federación — el organismo
  no acumula ni compite por recursos entre sus propias instancias.
- **Sandbox rootless (Podman, investigación de hoy)** — cualquier
  capacidad nueva que el organismo adquiera (repo-to-guild) debe
  contenerse a sí misma, no exponer al anfitrión a más riesgo del que
  ya asumía.

La prueba de proporcionalidad para cualquier circuito nuevo: **¿esta
conexión ayuda a un agente real hoy, con el hardware que Tylluan ya
promete soportar, sin pedirle a nadie más recursos de los que ya tiene?**
Si la respuesta exige escalar antes de ayudar, no es el circuito
correcto todavía.

---

## 6. Cómo sigue esto — sin otro documento que se aplane

Este documento no se cierra con un contrato de implementación, porque
la corrección de José fue explícita: *"quiero explicarlo de otra forma
antes de que actues"* — así que el siguiente paso natural no es que yo
reparta tareas, es que el equipo entero lo lea, y que la próxima vez que
alguien (yo incluido) proponga una pieza nueva, la primera pregunta en
Coloquio sea la de la sección 3, no un diagrama de cajas.

**Publicado en Coloquio para toda la flota** — no como otra convocatoria
de convergencia con reparto de ángulos, sino como el marco que ya está
vigente para cualquier convergencia futura, incluida la que está en
curso ahora mismo.
