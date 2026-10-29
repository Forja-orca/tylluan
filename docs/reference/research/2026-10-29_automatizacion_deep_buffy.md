# Automatización 24/7 de Deep (OpenCode) y Buffy (Freebuff): qué se puede y qué no

> Nota del TL, 2026-10-29. Pedida por José: que los agentes de seguridad trabajen también por su cuenta, como Antigravity, Hermes, DSH y el TL. Todo lo citado se leyó en esta sesión; lo que no, va marcado.

## Cómo trabajan hoy los demás

| Agente | Mecanismo propio | Dónde |
|---|---|---|
| TL | `/loop` + cron de sesión | Claude Code |
| Antigravity | tarea `schedule` con cron `*/40` en su IDE | su documento local de automatizaciones (no versionado) |
| Hermes, DSH | temporizadores propios (40 min) | sus entornos |
| Buffy | `scripts/coloquio_loop_buffy.py`, **solo lee y escribe un buzón**, nunca ejecuta; tarea de Windows `TylluanBuffyColoquioLoop` **desactivada** (última ejecución 2026-10-21, 837 ejecuciones perdidas) | Programador de tareas |
| Deep | ninguno | — |

## Deep (OpenCode 1.19.0 instalado): SÍ se puede

Fuente: https://opencode.ai/docs/cli/ (leída hoy).
- `opencode run [message..]` ejecuta sin la interfaz. Flags: `--auto` ("Auto-approve permissions that are not explicitly denied"), `--format` (json), `--agent`, `--dir`, `--continue`, `--session`, `--attach`.
- `opencode serve` levanta una API HTTP; con `OPENCODE_SERVER_PASSWORD` usa autenticación básica.
- **No hay planificador propio**: la página no documenta cron ni bucle. La planificación la pone el sistema (Programador de tareas de Windows) llamando a `opencode run`.
- Medido en esta máquina: ya tiene el MCP de Tylluan (`http://127.0.0.1:47004/sse`), así que puede leer y escribir en Coloquio. Credenciales de pago registradas: OpenAI, Nvidia, OpenCode Go, OpenRouter, Ollama Cloud (`opencode auth list`).
- **Riesgo medido**: el agente por defecto `build` trae `permission "*" action "allow"` (`opencode agent list`). Lanzarlo sin supervisión con `--auto` le daría permiso para todo. Hace falta un agente propio con lista de denegados explícita.

## Buffy (Freebuff): NO se puede hoy

Fuente: https://github.com/CodebuffAI/freebuff/issues/947 (leída hoy; abierta, sin respuesta de los mantenedores).
- "purely interactive": la CLI no acepta `-p`, `--headless` ni `--json`.
- La ejecución sin interfaz solo existe con `@codebuff/sdk` (`CodebuffClient.run()`), que pide clave de pago; con el token gratuito "untested and may be rejected by the backend".
- Hay un PR que añade el prompt como argumento (#1477) y una petición de ACP (#319); ninguno es ejecución desatendida.
- Un comentario en esa petición lo dejó la cuenta Forja-orca el 2026-09-20.

## Opciones para Buffy

| Opción | Efecto | Coste | Veredicto |
|---|---|---|---|
| A. Pasar su rol de seguridad a OpenCode con el mismo ejecutor | trabaja 24/7 ya | cambia el modelo (calidad por comprobar) | **recomendada** |
| B. Reactivar el buzón de `coloquio_loop_buffy.py` | avisa, no ejecuta | ninguno | complemento, no solución |
| C. SDK de Codebuff con clave de pago | headless oficial | dinero | descartada (el valor de Freebuff es ser gratis) |
| D. Esperar #947 / #1477 / #319 | headless oficial futuro | tiempo | seguir de lejos |
| E. Automatizar la ventana con teclas simuladas | frágil | riesgo alto | rechazada |

## No verificado
- Qué modelo usa Deep y cuánto costaría una ejecución horaria (hay cuatro proveedores de pago configurados).
- Que `opencode run --auto` respete todas las denegaciones del agente propio: no lo probé, no lanzo modelos sin permiso.
- Los plugins comunitarios `opencode-scheduler` y `opencode-tasks` aparecieron en el buscador; no leí sus repositorios.
- El efecto de dos agentes más compilando Rust en continuo sobre el bloqueo del directorio de compilación del pre-push.

## Decisión de José (2026-10-29)
Buffy no puede automatizarse con Freebuff y Deep no se automatiza a través de Windows: ambos siguen bajo supervisión del TL y de José (relevo manual, Coloquio y archivo de cola). Las opciones de arriba quedan como referencia; no se activa ningún planificador.
