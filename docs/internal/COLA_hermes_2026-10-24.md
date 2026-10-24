# COLA DE HERMES — 2026-10-24 (escrita por el TL; el Coloquio está caído)

Mismas reglas de siempre: solo investigación, un documento en `docs/reference/research/`, cita literal buscable con Ctrl+F (URL o ruta de fichero local), inferencias marcadas, <= 60 líneas, SIN commitear, en tu worktree. Cuando vuelva el Coloquio, avisa con `[H-nn COMPLETA]`.

## H-11 (pendiente) — Licencia LFM 1.0 de d1-3B
Documento: `docs/reference/research/2026-10-24_H11_licencia_lfm1_d1_hermes.md`. Qué dice exactamente la licencia sobre uso comercial y umbrales, redistribución, uso como servicio, atribución y restricciones, y si recomendar o documentar el modelo en un repo MIT, o que el usuario lo descargue por su cuenta, exige algo. Fuente primaria: el fichero LICENSE del repo `LiquidAI/d1-3B` en Hugging Face. Solo hechos citados; si es compatible con MIT lo decide José.

## H-12 — Cerrar los «no verificados» del informe de configuración de Hermes
El TL escribió un informe sobre tu configuración con huecos. Tienes el código fuente de Hermes en `%LOCALAPPDATA%\hermes\hermes-agent` (fuente primaria, léelo con `grep`):
1. **Cabeceras de autenticación de un servidor MCP HTTP** en `config.yaml`: ¿`headers:`? ¿expansión de variables de entorno? ¿`auth:`? Cita el código o la doc que lo define.
2. **`windows-sandbox-fallback.json`** (`{"state":"fallback","reason":"boot-loop"}` en `%APPDATA%\hermes`): qué lo escribe, qué significa y qué cambia en el comportamiento del Desktop. Busca el nombre del fichero en `apps/desktop`.
3. **Issue #48820** (el gateway que muere por un Job Object tras `hermes update`): qué dice el arreglo, si ya está en v0.22.4 y qué comando lo recupera.
4. **Checkpoints**: cómo elige Hermes la raíz del proyecto a la que hace `git add -A` y por qué pudo agotar 60 s con `cwd = D:\hermes-projects` (la doc dice que omite directorios de más de 50 000 ficheros).

Entrega `docs/reference/research/2026-10-24_H12_config_hermes_huecos_hermes.md` con tabla pregunta -> cita -> fuente -> estado.


## Worktree: método nuevo (23-oct-2026)
Lee `docs/internal/PROTOCOLO_worktrees_agentes.md`. Resumen: tu compilación pasa a `E:\cargo-target\hermes`; antes de cada tarea nueva haz `bash /e/tylluan/scripts/agent_wt.sh reset hermes` (guarda todo lo que tengas antes de limpiar). Para liberar tu `target` antiguo: `bash /e/tylluan/scripts/agent_wt.sh migrate hermes` cuando no tengas cargo en marcha.

## H-22 — Estado del arte: MCP como concentrador (solo lectura, fuentes primarias)
Nota de ≤60 líneas en `docs/reference/research/` con citas literales, URL y «No verificado»: (1) especificación MCP vigente (autorización, transporte HTTP/stateless); (2) guías de seguridad de MCP (confused deputy, token passthrough, tool poisoning); (3) cómo hacen búsqueda de herramientas / divulgación progresiva Anthropic y otros clientes; (4) pasarelas MCP de código abierto existentes y qué permisos por agente ofrecen. Cita solo lo que hayas leído en esta sesión. Entrega: nota + informe `IMPL_H-22_*` con `## Impact`. Sin commits.

## H-23 — Borrador de ADR-023 (MCP como concentrador), solo propuesta
Con tu nota H-22 y el código existente (`[[external_mcp]]`, `handler_do/external_mcp.rs`, `/api/v1/mcp/external`), redacta `docs/internal/PROPUESTA_ADR-023_mcp_concentrador.md`: contexto, decisión propuesta (paso directo con argumentos tipados `tylluan_do(server, tool, args)` más descripción, importación de configuración de clientes, bóveda OAuth, permisos por agente, cuarentena y comprobación de envenenamiento de descripciones, búsqueda de herramientas), alternativas descartadas, riesgos y 6 decisiones concretas para José. Marca arriba «PROPUESTA, no aprobada». Cada afirmación sobre el código lleva `fichero` y línea (escribe «línea N», nunca `:N`); lo que no hayas leído va en «No verificado». Sin commits.

## H-24 — Guardián determinista del README (idea I-03)
Crea `scripts/check_readme_claims.sh` (solo lectura): extrae del `README.md` y `README.es.md` las afirmaciones comprobables y las contrasta con el código: número de herramientas soberanas (debe coincidir con `tools.rs`), recuento de tests (con `check_test_count.sh`), versión, nombres de guilds citados que existan en `guilds/`, enlaces a ficheros que existan. Informa una línea por afirmación con `OK`/`DRIFT` y sale con 1 si hay deriva. **Excluye:** editar los README, cablear en `verify.sh` (lo hace el TL) y cualquier frase de posicionamiento (eso lo decide José). Entrega: script + informe `IMPL_H-24_*` con salidas reales sobre el README actual de `main`, `## Impact`, «No verificado». Sin commits.
