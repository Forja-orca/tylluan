# CopilotKit y AG-UI: qué se puede portar a Tylluan (2026-11-01)

Fuente: clon superficial de `github.com/CopilotKit/CopilotKit` (commit `d168487e`, 2026-10-30) leído en esta sesión, más `docs.ag-ui.com/concepts/events` y `github.com/ag-ui-protocol/ag-ui`. Todo lo no leído va en «No verificado».

## Lo leído en el código
| Dato | Evidencia |
|---|---|
| Licencia del repo: MIT | `LICENSE`, línea 1 |
| `@copilotkit/react-core` 1.83.0: 31 dependencias directas (entre ellas `@scarf/scarf`, `recharts`, `katex`, `lit`, `@ag-ui/client`) | `packages/react-core/package.json` |
| `@copilotkit/runtime`: 44 dependencias (GraphQL, hono, `@segment/analytics-node`, SDKs de proveedores como pares) | `packages/runtime/package.json` |
| Telemetría activa por defecto; se apaga con `COPILOTKIT_TELEMETRY_DISABLED=true` o `DO_NOT_TRACK=1` | `packages/shared/src/telemetry/telemetry-disabled.ts` |
| El núcleo del navegador reconoce `telemetry.copilotkit.ai` y `cdn.copilotkit.ai` como tráfico propio | `packages/core/src/core/learning-bridge.ts:65` |
| Existe un `LearningBridge` que captura trayectorias de agentes | mismo fichero |
| Hay un verificador de licencia firmada en el runtime (`COPILOTKIT_LICENSE_TOKEN`) | `packages/runtime/src/v2/runtime/core/runtime.ts:210` |

## AG-UI (el protocolo, no la librería)
La página de eventos lista: ciclo de vida (`RunStarted/Finished/Error`, `StepStarted/Finished`), mensajes de texto, llamadas a herramienta (`ToolCallStart/Args/End/Result`), estado (`StateSnapshot/Delta`), actividad, razonamiento (`Reasoning*`) y **subagentes** (`SubagentStarted/Finished/Error` con `parentToolCallId`). El repo del protocolo se declara MIT y cita SSE, WebSockets y webhooks como transportes.

## Lectura para Tylluan
1. **No adoptar la librería.** Peso y telemetría por defecto chocan con soberanía local y con «cero nube en el camino crítico». Coloquio ya tiene su virtualización (`@tanstack/react-virtual`) y su flujo SSE.
2. **Sí adoptar el vocabulario de AG-UI** como forma de los mensajes con estructura en Coloquio (campo `metadata`): entrega/relevo = `SubagentStarted/Finished`, herramienta = `ToolCall*`, aprobación = interrupción de `RunFinished`. Coste bajo y deja abierta la puerta a clientes AG-UI sin dependencia.
3. **Candidato de F8/F9:** si un IDE o cliente habla AG-UI, un adaptador fino en el kernel (REST/SSE, sin tool MCP nueva por AUD-07) serviría de puerta de entrada.

## No verificado
Versiones de React que soporta en la práctica (el par declara `^18 || ^19`); si `react-core` arranca sin `@scarf/scarf` ni red; el tamaño real del bundle; la especificación de transporte SSE de AG-UI (solo vista la lista de transportes); qué hace exactamente el verificador de licencia ni qué funciones bloquea; ningún código de CopilotKit se ejecutó.
