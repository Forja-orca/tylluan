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

| Agente | Harness | Modo headless real | Bloqueo |
|---|---|---|---|
| **Deep** | OpenCode | ✅ `opencode run --model provider/model "<prompt>"` — maduro, sin TUI, documentado como caso de uso explícito para scripting/automatización/CI | Ninguno conocido |
| **Antigravity** | Antigravity CLI (`agy`, sucesor de Gemini CLI) | ✅ Modo headless/print — un solo prompt, respuesta a stdout, exit code 0/no-cero según éxito. Desde v1.1.13 (2026-08-14) soporta `GEMINI_API_KEY` + `modelProvider: "gemini"` en `settings.json` **sin login interactivo** — pensado explícitamente para CI | Ninguno conocido, la vía sin sesión interactiva ya existe |
| **Claude Code** (yo) | Claude Code / Agent SDK | ✅ `-p`/`--print`, SDK completo en CLI/Python/TypeScript, ya el modo más maduro de los cuatro | Ninguno |
| **Buffy** | Codebuff/Freebuff | ⚠️ Existe `@codebuff/sdk`'s `CodebuffClient.run()` — pero **requiere `CODEBUFF_API_KEY` de pago**. La cuenta gratuita "Freebuff" (OAuth de dispositivo, la que usa Buffy) **no puede autenticarse con el SDK** — feature request abierto y sin resolver (`CodebuffAI/freebuff#947`, agosto 2026) pidiendo exactamente esa vía para cuentas gratuitas | **Bloqueo real de proveedor, no técnico nuestro** |

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
trusted_authors = ["jose"]
command = ["opencode", "run", "--model", "deepseek/deepseek-v4", "<tarea insertada por el dispatcher>"]
```

Esto **no está implementado ni probado todavía** — es la conclusión de
la investigación, no un anuncio de que ya funciona. Falta decidir cómo
el dispatcher construye el prompt final a partir del contenido de
Coloquio (ver `coloquio_dispatcher_spec.md`, el `content_snapshot` ya
existente en `PendingDispatch` es el candidato natural), y probarlo en
vivo con al menos un agente antes de generalizar.

## 5. Alcance explícitamente fuera de este documento

- No se decide todavía si esto se implementa para Deep/Antigravity ya,
  ni con qué prioridad frente al resto del trabajo en curso.
- No se resuelve el caso de Buffy — queda documentado como bloqueo
  externo real, a la espera de una decisión de José.
- No se toca el mecanismo de aprobación humana hash-bound (BWC-2) — sigue
  exactamente igual, solo cambia qué comando se ejecuta tras aprobarse.

## 6. Referencias verificadas

1. OpenCode CLI — modo `run` no interactivo: [opencode.ai/docs/cli](https://opencode.ai/docs/cli/), confirmado por múltiples fuentes independientes 2026.
2. Antigravity CLI (`agy`) headless/print mode + soporte CI sin login: [antigravity.google/docs/cli/headless](https://antigravity.google/docs/cli/headless/).
3. Claude Code headless (`-p`/`--print`) y Agent SDK: [code.claude.com/docs/en/headless](https://code.claude.com/docs/en/headless).
4. Codebuff SDK y su restricción de cuenta gratuita — feature request real, sin resolver: [github.com/CodebuffAI/freebuff/issues/947](https://github.com/CodebuffAI/freebuff/issues/947).
