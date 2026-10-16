# ADR-021: Servicio de decisión y la sexta herramienta soberana `tylluan_decide`

- **Estado:** 📐 **PROPUESTO** — borrador del Tech Lead. Solo diseño en papel: **cero código** hasta que José lo apruebe.
- **Fecha:** 2026-10-16
- **Depende de:** ADR-018 (Decision Fabric, Fase 0) y ADR-020 (Inference Gateway: capacidad `decide`, dialectos, clases de privacidad).
- **Sustituye en parte:** la regla "exactamente 5 herramientas soberanas" de CONTRACT-01 (enmienda formal en §5).

---

## 1. Qué decidió José y qué no

### Decidido por José (2026-10-15)
1. **Principio:** el modelo de decisión se consulta solo cuando un módulo tiene una decisión semántica concreta, acotada y evaluable. Las reglas deterministas conservan la autoridad sobre permisos, seguridad y límites; las acciones de riesgo exigen aprobación humana.
2. **Dos puertas** sobre un mismo servicio: los módulos internos del core y una herramienta directa para los agentes IDE.
3. **Sexta herramienta real**, de nombre **`tylluan_decide`** (se descartó exponerla como operación de `tylluan_do`).
4. **Desactivable:** si el usuario no quiere usarla, el core funciona como hasta ahora.
5. Dialectos `/v1/systemone` y `/v1/decisions` desde el principio, con contrato interno neutro (ADR-020).
6. Datos clínicos hacia la nube solo con aprobación explícita por petición; cambio de perfil automático solo hacia local (ADR-020).
7. "Solo de salida" es una decisión de arquitectura actual, revisable, no un invariante.

### Propuesto por el TL (pendiente de confirmar por José)
- Regla de adopción: un módulo adopta el modelo solo si, en modo sombra, supera a sus propias reglas en sus métricas.
- Estados `desactivado / sombra / consultivo / puerta` por contrato.
- `tylluan_decide` **desactivada por defecto** (ver §5.2 y §10).
- Filtro de salida como capa de verificación independiente del generador (§7).

---

## 2. Estado real del código (VERIFICADO contra el árbol, 2026-10-16)

| Pieza | Estado | Dónde |
|---|---|---|
| Contrato de decisión | Existe el trait `DecisionProvider` y el proveedor de reglas. Las variantes LAYA, Clef-Flash y Decima viven en el harness de evaluación, no en producción | `crates/tylluan-kernel/src/router/decision/mod.rs` |
| Resultado de la Fase 0 | LAYA y Clef-Flash NO-GO en nuestra tarea; Decima-small GO-PROVISIONAL (+5 aciertos netos de 245, sin test pareado) | `docs/roadmap/ROADMAP_O3.md`, ADR-018 |
| Mecanismos de decisión dispersos | ADR-018 inventaría 9 mecanismos reales e independientes (Scheduler, CoherenceGate, Stigmergy, etc.) | ADR-018 §0 |
| Lista de herramientas soberanas | `all_tools()` filtra a exactamente 5 nombres; el contrato está marcado con `@CONTRACT: SOVEREIGN-TOOLS` | `transport/server/tools.rs` |
| Tests que fijan el número 5 | `test_sovereign_count_is_exactly_5` y su par (`tools.rs`, L663 y L669), `sovereign_tools_e2e.rs`, `integration_mcp.rs`, `guilds_v2_integration.rs` (L380), la Agent Card A2A con 5 skills (`a2a.rs`, L800) y la sonda de salud `tool_count == 5` (`api_monitor.rs`, L450) | ver arriba |
| Documentación que cita el contrato | AGENTS.md, CLAUDE.md, CONTRIBUTING.md, README (EN/ES), ROADMAP.md, SPEC.md, STATUS.md, `docs/concepts/*`, `docs/guides/CLIENT_INTEGRATION_GUIDE.md`, el mapa de arquitectura de `docs-site`, `docs/assets/architecture.svg` y los componentes del dashboard `ConnectorsTab` y `ColoquioCanvasWorkspace` | `git grep` sobre ficheros versionados |
| Frontera de autenticación | La Fase 1 de OAuth (en revisión) separa la clase Operador; `tylluan_decide` será de clase **agente** | `DESIGN_oauth_operator_2026-10.md` |

> El inventario de ficheros de §5.4 se vuelve a generar con `git grep` en el momento de implementar. Esta tabla es el punto de partida, no la lista definitiva.

---

## 3. El servicio de decisión (puerta interna)

### 3.1 Contrato por módulo (`DecisionContract`)
Cada módulo del core que quiera consultar el modelo **declara un contrato**; sin contrato no hay consulta. Un contrato fija:
- **Identificador y módulo dueño** (p. ej. `scheduler.route`).
- **Pregunta tipada** y su **conjunto cerrado de opciones**, que construye el sistema, nunca el modelo.
- **Evidencia permitida** como entrada (qué datos pueden entrar en el estado) y su **sensibilidad** (eje de datos de ADR-020).
- **Métrica de éxito** del módulo (con qué se compara el modelo en sombra).
- **Regla de reserva**: la decisión determinista que se aplica si el modelo se abstiene, falla o está desactivado.
- **Clase de riesgo** de la acción que gobierna y **modo por defecto**.

Los candidatos naturales salen del inventario de ADR-018 (los 9 mecanismos); este ADR **no** elige cuáles migran, eso se decide por módulo con datos (§6).

### 3.2 Modos de activación (por contrato)
| Modo | Qué hace |
|---|---|
| `off` | No hay consulta ni llamada externa. El módulo usa su regla de siempre. **Es el valor por defecto de todos los contratos** |
| `shadow` | Consulta el modelo, **registra** su respuesta y la compara con la regla, **sin afectar** la decisión real |
| `advisory` | La respuesta se muestra o se añade como señal, pero la decide la regla |
| `gate` | La respuesta puede bajar confianza, marcar o podar según la regla aditiva (§3.3). Solo para contratos con evidencia medida |

Existe además un **interruptor global**. Con él apagado no hay ninguna llamada oculta y el comportamiento es idéntico al actual.

### 3.3 Regla de autoridad (aditiva)
- El modelo **elige entre opciones que el sistema ya acotó**. No inventa comandos, rutas ni argumentos.
- Puede **bajar la confianza, marcar, podar o abstenerse**. **No puede** anular el rechazo de un validador determinista, revivir lo que una regla rechazó, conceder permisos ni autorizar una acción de riesgo.
- ACL, permisos, límites duros, validación de esquema y reglas de seguridad **permanecen deterministas**.
- Una probabilidad de riesgo **no es una autorización**.

### 3.4 Abstención y fallo
- **Abstenerse es una respuesta de primera clase** (`Abstain { reason }` en el contrato neutro de ADR-020).
- Si el proveedor falla o se abstiene: riesgo bajo, se aplica la **regla de reserva**; acción crítica, **se pide aprobación humana o se deniega**. El modelo nunca es el único guardián.
- Un cambio de proveedor de local a nube **nunca es silencioso** (ADR-020).

### 3.5 Evidencia y registro
Cada decisión deja una fila de auditoría con: contrato, pregunta, **referencias** a la evidencia (identificadores, no el contenido cuando es clínico), proveedor y **identidad del modelo** (ADR-020), modo, respuesta y probabilidades, abstención, latencia, resultado de la regla y resultado final. Es la materia prima de la evaluación en sombra y de la calibración. El registro respeta la matriz de privacidad de ADR-020: no copia contenido clínico fuera del perímetro permitido.

---

## 4. Calibración y adopción por módulo
- **Umbral por contexto**, no uno global: la literatura reciente usa umbrales distintos para alta garantía y para exploración (fuente secundaria, a validar con nuestros datos).
- **Calibración por combinación** modelo + proveedor + tarea (p. ej. escalado de temperatura con pares evidencia–veredicto del dominio). Un cambio de proveedor obliga a recalibrar.
- **Criterio de adopción** de un contrato a `advisory` o `gate`: supera a la regla en su métrica **con respaldo estadístico** (test pareado, McNemar o bootstrap pareado sobre las discordancias), sin aumentar los errores peligrosos ni las abstenciones injustificadas, y con latencia aceptable. Decima-small, hoy GO-PROVISIONAL con +5 aciertos netos de 245, **no cumple todavía** ese criterio y sirve de primer candidato a medir.
- El resultado, positivo o negativo, se registra como GO / NO-GO con el dato, como en la Fase 0.

---

## 5. La sexta herramienta soberana: `tylluan_decide` (enmienda de CONTRACT-01)

### 5.1 Propósito y límites
Fachada directa sobre el mismo servicio, para que un agente IDE haga **preguntas acotadas y explícitas** ("¿cuál de estas tres herramientas?", "¿esta respuesta necesita revisión?").
- **Solo lectura y sin efectos:** no ejecuta acciones ni concede permisos.
- **Clase agente** en el modelo de autenticación (no operador).
- Pasa por el Inference Gateway: privacidad, presupuesto y auditoría de ADR-020.

### 5.2 Activación y compatibilidad (propuesta)
- **Desactivada por defecto.** Con ella apagada, `tools/list` devuelve **exactamente las 5 herramientas de hoy**: ningún cliente ve un cambio y los tests actuales siguen válidos.
- Al activarla en la configuración, `tools/list` añade la sexta. CONTRACT-01 pasa a decir: *"5 herramientas soberanas siempre, más `tylluan_decide` cuando el servicio de decisión está habilitado."*
- **Alternativa** (más estricta): fijar siempre seis herramientas y que la sexta devuelva `status: disabled`. Se descarta como recomendación porque obliga a todos los clientes a ver un cambio aunque no lo usen.

### 5.3 Contrato de la herramienta (propuesta)
Entrada: `state` (texto o estructura acotada), `questions` (mapa nombre → tipo `choice | score | boolean`, con opciones o niveles e instrucciones), opcionalmente `contract` para invocar un contrato interno nombrado, y un límite de latencia.
Salida: por pregunta, el valor, las probabilidades, la confianza o la **abstención con motivo**; además `provider`, `model_identity`, `mode` y una bandera `degraded` si se aplicó una regla de reserva. El detalle de campos se fija al aprobar el ADR.

### 5.4 Seguridad propia de la herramienta
- **Contenido adversarial en `state`:** TypeSafe advierte de que puede influir en el modelo y de que confianza no es certeza. Las respuestas son **consultivas** y llevan procedencia explícita; no se usan para autorizar.
- Límites de tamaño y de preguntas por petición; presupuesto de inferencia y limitación de tasa.
- Datos clínicos o privados en `state`: aplica la matriz de ADR-020 (no salen a la nube sin aprobación por petición).
- Auditoría de cada llamada.

### 5.5 Qué hay que actualizar (a regenerar con `git grep` al implementar)
- **Código:** la lista de herramientas soberanas en `all_tools()` y sus comentarios de contrato; `test_sovereign_count_is_exactly_5` y su par, `sovereign_tools_e2e.rs`, `integration_mcp.rs`, `guilds_v2_integration.rs`; la Agent Card A2A (5 skills) y la sonda de salud `tool_count == 5`, para que acepten 5 por defecto y 6 con el servicio habilitado.
- **Documentación pública y diagramas:** README (EN/ES), ROADMAP, SPEC, AGENTS, CLAUDE, CONTRIBUTING, STATUS, `docs/concepts/*`, `CLIENT_INTEGRATION_GUIDE`, el mapa de arquitectura de `docs-site`, `architecture.svg` y el diagrama interactivo.
- **Interfaz:** `ConnectorsTab` y `ColoquioCanvasWorkspace` del dashboard.

---

## 6. Primer caso de prueba medible
**Hipótesis:** un modelo de decisión pequeño, junto al generador, mejora al agente completo en decisiones acotadas. No vuelve más inteligente al generador por dentro.
- **Caso:** un agente de código que propone cambios.
- **Diseño:** el mismo modelo generador de unos 4B, los mismos casos y las mismas herramientas; se compara el agente solo contra el agente con el servicio en dos o tres puntos concretos (elección de herramienta, si un recuerdo merece recuperarse, si una acción necesita revisión o escalado).
- **Métricas:** finalización correcta, herramientas innecesarias o equivocadas, errores peligrosos, escalados a humano, latencia, coste y calibración. Se incluyen ejemplos ambiguos y contenido que intenta manipular la decisión.
- **Resultado:** GO o NO-GO registrado con el dato. Un NO-GO es un cierre válido.

---

## 7. Filtro de salida (diseño posterior)
El filtro de lo que se entrega al agente externo es una **capa de verificación separada del generador**, no otra pregunta al mismo modelo que redactó la respuesta.
- **Primero comprobaciones deterministas:** citas que existen, rutas, hashes, versiones.
- **Veredictos de tres estados:** pasa, corrige, bloquea; el bloqueo va a revisión humana.
- Un clasificador pequeño puede actuar de prefiltro. Si generador y juez comparten errores, el filtro puede ratificar una alucinación con seguridad aparente.
- La promesa medible **no es "cero alucinación"** sino menos afirmaciones sin respaldo y abstención explícita cuando falta evidencia.

## 8. REA como proveedor de evidencia
REA (servidor MCP de ingeniería inversa) entra como **MCP externo opcional**, fuera de la ruta crítica. Sus resultados son **evidencia**, nunca autoridad, con procedencia `agent_generated`, y se debe respetar su marca `untrusted_content` (hoy sin manejo en Rust; trabajo nuevo). Antes de registrarlo: verificar licencia en el repositorio oficial, versión del paquete, qué ejecuta sobre el objetivo y dependencias.

---

## 9. Fases propuestas
| Fase | Contenido | Condición de avance |
|---|---|---|
| 0 | Contratos, interruptor global, la tool detrás de la bandera **apagada**, solo el proveedor de reglas. Tests: con la bandera apagada son 5 herramientas; con ella encendida son 6 | ADR aprobado |
| 1 | `shadow` en uno o dos contratos con el proveedor local (`/v1/systemone`) | Registro de auditoría funcionando |
| 2 | Evaluación pareada y calibración; `advisory` donde gane | Criterio de adopción de §4 |
| 3 | `gate` selectivo y filtro de salida | Evidencia medida y revisión adversaria |

**Tests obligatorios** (a concretar): bandera apagada = 5 herramientas idénticas a hoy; encendida = 6; opciones cerradas (una respuesta fuera del conjunto se rechaza); abstención respetada; el modelo no puede revertir un rechazo determinista; no hay llamadas externas con el servicio apagado; privacidad clínica; inyección en `state`; herramienta no accesible como operador.

## 10. Riesgos y límites
- Añadir una herramienta amplía la superficie que ven los clientes; por eso nace apagada.
- Un modelo de decisión puede **empeorar** un módulo (LAYA: 0,351 de acierto frente a 0,531 de las reglas); la adopción es por módulo y con datos.
- Los dos dialectos no son equivalentes (ADR-020); no todas las preguntas son portables.
- Este ADR no define el filtro de salida completo ni elige qué módulos migran.

## 11. Marcado de veracidad
- **VERIFICADO:** lo de §2 contra el árbol; el comportamiento de `all_tools()` y los tests que fijan el 5.
- **HIPÓTESIS / PROPUESTA:** contrato de la herramienta (§5.3), activación apagada por defecto, criterio de adopción, fases y umbrales.
- **No verificado:** las cifras externas citadas por terceros sobre umbrales y mejora de cascadas.

## 12. Decisiones que necesita José
1. ¿`tylluan_decide` **apagada por defecto** (recomendado) o encendida?
2. ¿Confirmas el nombre con guion bajo, `tylluan_decide`, como las otras cinco?
3. ¿Aprobamos los cuatro modos (`off / shadow / advisory / gate`) y el criterio de adopción por módulo con test pareado?
4. ¿Con qué uno o dos contratos empezamos en modo sombra? (propuesta: uno de enrutado del Scheduler y uno de decisión de revisión humana, donde Decima ya tiene datos)
5. ¿Existe también un endpoint REST equivalente, o solo la herramienta MCP?
