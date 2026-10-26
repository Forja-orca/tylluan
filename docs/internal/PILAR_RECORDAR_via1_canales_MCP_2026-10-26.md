# Pilar "Recordar", vía 1: qué puede hacer un servidor MCP sin tocar el cliente

Fecha: 2026-10-26. Fuente: especificación MCP oficial, revisión 2026-07-28 (las páginas citadas se abrieron hoy). Ninguna afirmación sobre el comportamiento de un cliente concreto está verificada salvo la indicada en el punto 1. No verificado: el resto.

## A. Qué cambió en el protocolo (y por qué lo intentado antes ya no vale tal cual)

Lista oficial de cambios: https://modelcontextprotocol.io/specification/latest/changelog

- Se elimina el apretón de manos `initialize` y las sesiones; cada petición lleva versión y capacidades en `_meta` ("Make MCP stateless").
- Los servidores "MUST send server-to-client requests (such as roots/list, sampling/createMessage, or elicitation/create) using the MRTR pattern": el servidor solo puede pedir algo al cliente DENTRO de la respuesta a una petición del cliente (prompts/get, resources/read, tools/call). "The previous pattern of server-initiated requests is no longer supported."
- Roots, Sampling y Logging pasan a obsoletos (SEP-2577); sampling era el único canal del servidor hacia el modelo del cliente.
- Las notificaciones empujadas se reducen a `subscriptions/listen`: cambios de lista y de recursos suscritos, no contenido para el modelo.
- Nuevo: `ttlMs` y `cacheScope` en los listados; extensiones opcionales (tareas, Skills sobre MCP, aplicaciones).

## B. Canales servidor a modelo que SÍ existen sin modificar el cliente

| Canal | Qué permite | Límite oficial | Utilidad para recordar |
|---|---|---|---|
| 1. `instructions` en `server/discover` | "Optional natural-language guidance for LLMs on how to use this server effectively". Evidencia local: en esta sesión Claude Code ha puesto en el prompt del sistema el texto `instructions` de nuestro kernel (se ve bajo "MCP Server Instructions"). | Estático (cacheable con `ttlMs`), una vez por conexión; no por turno. | Enseñar al modelo CUÁNDO llamar a recall y qué esperar. Hoy lo gastamos en texto de protocolo: `crates/tylluan-kernel/src/transport/http/api_v1/mcp.rs` línea 876. |
| 2. Descripciones de herramientas | Lo que el modelo lee para decidir si llama. | La lista "MUST NOT vary per-connection or as a side effect of other requests"; puede cambiar con el tiempo y por autorización; orden determinista recomendado. | Redactar la descripción de recall como disparador ("llámame antes de editar un fichero ya tratado..."). No sirve para contexto por turno. |
| 3. Resultado de cualquier herramienta de Tylluan | El resultado de `tools/call` entra en el contexto del modelo. | Solo ocurre si el modelo llama a alguna herramienta nuestra; el contenido es no confiable. | Adjuntar a cada respuesta un bloque corto de recuerdos relevantes escogido por la política, usando los argumentos de la llamada como pista. Es el único canal casi por turno. |
| 4. Recursos con anotaciones | `audience`, `priority`, suscripción a cambios. | "Application-driven": el cliente decide si los incluye. | Dependiente del cliente; hay que medir cuáles lo hacen. |
| 5. Skills sobre MCP (extensión) | Instrucciones y ficheros descubiertos por MCP. | Matriz oficial de clientes: casi vacía (ChatGPT y herramientas de prueba, soporte parcial). | Futuro, hoy sin cliente. |

Descartado por la propia especificación: sampling (obsoleto) y cualquier petición iniciada por el servidor. Elicitation pregunta al usuario, no al modelo.

## C. Conclusión provisional (hipótesis, no hecho)

1. Con el protocolo MCP actual, el servidor no puede "despertar" al agente ni inyectar por turno. Hablar solo puede hacerlo (a) al conectar (instrucciones), (b) al responder una llamada (resultado) y (c) con recursos si el cliente los usa.
2. El techo de la vía MCP pura es doble: influir en SI el modelo llama (canales 1 y 2) y enriquecer lo que devuelve cuando llama (canal 3). Entre llamadas, Tylluan es sordo.
3. Por tanto la pregunta experimental de la vía 1 es cuánto sube el recall efectivo con los canales 1 a 3 y a qué coste: cuantificar, no suponer.
4. Si el techo resulta insuficiente (modelo que casi nunca llama), eso es el dato que justifica pasar a la vía de la puerta de entrada local (nivel 2) o, solo después, a la vía 3 (plugin por IDE).

## D. Experimentos propuestos para la vía 1 (sin tocar clientes)

1. Medir cuántas veces llama el modelo a recall por tarea con la `instructions` actual frente a una `instructions` que enseñe cuándo llamar (A/B en kernel de prueba con las 40 preguntas).
2. Medir el efecto de reescribir la descripción de la herramienta de recall.
3. Prototipar el enriquecimiento de resultados (canal 3) detrás de un flag desactivado y medir la precisión de lo adjuntado y el daño por error.
4. Mapear qué clientes incorporan recursos o `instructions` realmente (prueba por cliente, no por documentación).

Seguridad: el canal 3 mete texto en el contexto desde el servidor; todo recuerdo adjunto debe llevar procedencia y tratarse como datos, nunca como instrucciones (ver el modelo de amenazas de S-DEEP-1).
