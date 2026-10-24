# ADR-024: Contención de ejecución de procesos por HTTP

- **Estado:** 📝 **PROPUESTO** — borrador de DSH (2026-10-24). **La aprobación es de José**, punto por punto (§9); nada de lo que sigue se implementa hasta que la dé. Este ADR no decide por él.
- **Fecha:** 2026-10-24
- **Origen:** asignación del TL (T1719 y T1721) a partir de T-DSH-70 (documento de decisiones para José) y T-DSH-71 (coste real de C3). Las filas citadas son las 1, 2, 3, 7, 9 y 10 de T-DSH-69.
- **Insumos:**
  - `docs/internal/IMPL_T-DSH-69_paths_procesos_2026-10-24.md` — inventario de rutas HTTP/MCP que construyen paths o lanzan procesos desde entrada externa.
  - `docs/internal/IMPL_T-DSH-70_decisiones_seguridad_2026-10-24.md` — opciones, riesgo, fricción y preguntas para José.
  - `docs/internal/IMPL_T-DSH-71_costes_c3_2026-10-24.md` — consumidores in-repo del endpoint directo de guild y uso real en logs.
  - `crates/tylluan-kernel/src/transport/http/auth.rs`, `api_v1/api_mcp.rs`, `api_v1/api_admin.rs`, `api_v1/api_guilds.rs`, `api_v1/routes.rs`, `registry/guild_process.rs`, `registry/service_manager.rs`, `security/dispatch_executor.rs`, `tylluan.toml` de producción (lectura, no versionado).
- **Ámbito:** cómo se contiene el lanzamiento de procesos y la escritura de config desde HTTP en el kernel (alta/edición/descubrimiento de servidores MCP, guardado de `tylluan.toml`, llamada directa a guilds). **Fuera de ámbito:** sandbox/aislamiento de guilds (ADR-022), modelo de confianza de extensiones (ADR-022), envenenamiento de memoria, `dev_mode` como decisión de producto, y las opciones A2/A3/E que este ADR deja explícitamente para más adelante (§6).

---

## 1. Resumen en diez líneas

1. Las rutas de alta/edición de MCP externo (filas 1-2), guardado crudo de config (fila 3) y descubrimiento (fila 9) son **operator-class**: exigen el token maestro por cabecera.
2. La llamada directa a guild (fila 7) es **agent-class**: no exige operador; su ACL solo actúa si `acl.roles` no está vacío y `default_role` puede hacer que un token no listado sea `admin`.
3. **Operator-class no es contención**: una vez dentro, el kernel ejecuta `cmd /c <command>` con `args`/`cwd`/`env` del body, escribe el TOML crudo y registra comandos desde configs de terceros sin allowlist ni confirmación.
4. Este ADR propone **seis controles independientes**: A1, A4, B1, C1, C3 y D1, cada uno con su riesgo, fricción y alcance técnico.
5. **Ninguno se implementa sin aprobación de José**; el orden recomendado es una propuesta, no una decisión.
6. El orden recomendado agrupa primero lo que **no requiere migración de configuración** (A4, D1), luego lo que define listas (B1, A1), después el endpoint directo (C3) y al final el ACL fail-closed (C1).
7. **C3 tiene coste in-repo cero** (T-DSH-71): ningún script/guild/agente del repo llama al endpoint directo para `bash`/`filesystem`; solo el dashboard de visión usa el patrón `/call/`.
8. **C1 es el cambio sistémico** (ACL sin `default_role`): la producción actual tiene `dev_mode = true` y `default_role = "admin"`; hacerlo exige decidir cómo se declaran los roles de los agentes y aceptar fricción de migración.
9. Quedan **fuera** de esta propuesta A2 (allowlist de ejecutables MCP), A3 (confinar `cwd`/`env`) y E1/E2/E3 (`/api/v1/bash` y `dev_mode`): son decisiones posteriores o de otro ADR.
10. Si José no aprueba nada, el estado actual no cambia; si aprueba un subconjunto, cada control es independiente y se puede implementar por fases.

---

## 2. Estado real verificado (árbol `ca76e1b`; evidencia de T-DSH-69/70/71 y lectura directa)

| # | Hecho | Evidencia |
|---|---|---|
| H1 | La clase operador exige `MasterToken` por cabecera o `DevBypass`; el token por query-string **no** vale, un JWT OAuth tampoco, la credencial de guild nunca (salvo la excepción auditada de `silva/edge`) | `auth.rs`, líneas 59-77 (`AclAuthKind`) y 336-345 (`operator_gate_allows`) |
| H2 | Son operator-class `/api/v1/config*`, `/api/v1/mcp/external*`, `/api/v1/dispatches/` y `POST /api/v1/bash` | `auth.rs`, líneas 261-273 |
| H3 | El alta/edición de MCP externo acepta `command`, `args`, `cwd` y `env` del body, registra el server y lo arranca (`ensure_running`); en Windows el spawn es `cmd /c <command>` con `current_dir` del request | `api_mcp.rs`, líneas 106-112 (alta) y 213-236 y 270-271 (edición); `guild_process.rs`, líneas 644-666 |
| H4 | `POST /api/v1/config` valida solo que el body sea TOML y lo escribe **tal cual** en `tylluan.toml`; de ahí salen `[[external_mcp]].command`, `[services].command` y `[wake].command` que se ejecutan al arrancar o al despachar | `api_admin.rs`, líneas 715-732; `service_manager.rs`, líneas 58-85; `dispatch_executor.rs`, líneas 103-118 |
| H5 | El descubrimiento importa `command`, `args` y `env` de configs de terceros (Claude/Gemini/etc.) al registry de MCP externos, sin aprobación humana | `api_mcp.rs`, líneas 311-353 |
| H6 | `POST /api/v1/guilds/{guild}/call/{tool}` (y su alias `/tools/`) comparten handler; pasan `Json(args)` crudo a la guild; el ACL solo se aplica si `acl.roles` no está vacío; `resolve_role_for_token` cae a `default_role` si el token no está listado | `routes.rs`, líneas 32 y 139; `api_guilds.rs`, líneas 50-100; `auth.rs`, líneas 136-139 |
| H7 | La configuración de producción (no versionada) tiene `dev_mode = true` y `[security.acl] default_role = "admin"` con `roles.admin = ["*"]` | `E:\tylluan\tylluan.toml` (leído 2026-10-24, solo lectura); ADR-022 H7 |
| H8 | La ruta directa a guild **no escribe `guild_audit_log`**: ejecuta `call_tool_as` sin registrar la llamada | `api_guilds.rs`, líneas 97-100; audit.db sin filas en la ventana de las llamadas del 2026-10-23 (T-DSH-71) |
| H9 | Ningún consumidor in-repo llama al endpoint directo para `bash`/`filesystem`; el único consumidor del patrón `/call/` es el dashboard de visión; en logs de producción solo 5 llamadas HTTP a `bash`/`filesystem` en 4 días (todas el 2026-10-23, 01:19-01:20) y 2 históricas de junio | T-DSH-71, §2-§4 |
| H10 | Los agentes usan MCP (`POST /messages`: 9 359 en 4 días) y las guilds usan `POST /api/v1/do` (4 en el mismo periodo); el endpoint directo es vía secundaria | T-DSH-71, §3-§4 |

---

## 3. Decisiones de José que este ADR respeta

1. **Presupuesto de fricción:** «la seguridad no debe suponer un estorbo de trabajo ni a agentes ni a humanos» (2026-10-18); cada control declara su coste.
2. **Nunca perder el acceso:** se pueden generar y rotar tokens, pero José no puede quedarse fuera (2026-10-21). C1 no toca el token maestro ni `.tylluan-token`.
3. **La decisión de C3 es de José** (T1721): el TL confirma que el coste in-repo es bajo, pero no la adopta por él.
4. **`dev_mode = false` solo después de OAuth fase 2** (2026-10-18): E3 (poner `dev_mode=false`) no forma parte de esta propuesta.
5. **Sin dependencia de Docker Desktop** ni de runtimes externos para ninguno de los seis controles; todos son cambios de kernel/config.

---

## 4. Decisiones propuestas (ninguna aprobada)

Notación de fricción: **baja** = no cambia el flujo normal o +1 clic ocasional; **media** = hay que configurar/excepcionar algo la primera vez; **alta** = cambia el flujo o bloquea casos hoy legítimos hasta adaptarlos.

### A1. Ejecución directa sin `cmd /c` para rutas absolutas verificadas

- **Qué cambia:** en el spawn de guilds externas (`guild_process.rs`, líneas 644-666), si `command` es una ruta absoluta a un ejecutable (`.exe` en Windows), lanzarlo directo; usar `cmd /c` solo para `.cmd`/`.bat` de una lista de extensiones conocidas. Rechazar metacaracteres de shell cuando no haya wrapper.
- **Riesgo que cierra:** que un `command` con espacios/flags pase por el intérprete `cmd.exe` (inyección de argumentos al shell) desde el alta/edición de MCP externo.
- **Fricción:** **baja** para servidores nativos; **media** en Windows para `.cmd`/`.bat` (npx/npm/yarn), que necesitan wrapper restringido.
- **Alcance técnico:** `api_mcp.rs`, líneas 106-112 y 213-271; `guild_process.rs`, líneas 644-666. Flag opcional `[security.process] allow_shell_wrapper` (default seguro) para no romper el caso `.cmd`.
- **Tests previstos:** unit de construcción de argv (ruta absoluta `.exe` → sin `cmd`; `.cmd` → con `cmd` y extensión permitida; metacaracteres → rechazo); test del handler con `command` malicioso.
- **Dependencias:** ninguna; independiente de A4.

### A4. Confirmación humana + auditoría del diff al alta/edición de MCP externo

- **Qué cambia:** `POST /api/v1/mcp/external` y `PUT /api/v1/mcp/external/{name}` exigen un campo de confirmación explícito con el comando normalizado; el kernel escribe una fila de auditoría con el comando anterior y el nuevo antes de registrar/arrancar.
- **Riesgo que cierra:** uso indebido del token de operador (filtrado, extensión del dashboard comprometida, error humano) y registro silencioso de procesos.
- **Fricción:** **baja** — alta/edición de MCP es una operación rara; +1 clic y ver el comando.
- **Alcance técnico:** `api_mcp.rs`, líneas 106-112 y 213-271; reutiliza `log_audit_entry`/`audit_db_path` (`handler_do/audit.rs`); el dashboard (`/api/v1/mcp/external`) debe enviar la confirmación.
- **Tests previstos:** sin confirmación → 400; con confirmación → 201/200 y fila de auditoría con `command` before/after; no se arranca el server si falta confirmación.
- **Dependencias:** ninguna; base natural para D1.

### B1. Allowlist de claves escribibles en `POST /api/v1/config`

- **Qué cambia:** tras parsear el TOML (`api_admin.rs`, líneas 715-732), rechazar desde la API las secciones que ejecutan procesos: `[[external_mcp]]` con `command`/`args`/`env`/`cwd`, `[services]` y `[wake].command`. Esas secciones se siguen pudiendo editar a mano en el fichero; la API deja de ser un camino para sembrarlas.
- **Riesgo que cierra:** que un body HTTP deje un proceso ejecutándose en el siguiente arranque o despacho (cadena con H4).
- **Fricción:** **baja** para el grueso del editor de config; **media** para quien quiera tocar esas secciones desde el dashboard (puede seguir por fichero).
- **Alcance técnico:** `api_admin.rs`, líneas 715-732; definir la lista de claves permitidas y devolver un 400 explicando qué se bloqueó.
- **Tests previstos:** body con `[[external_mcp]].command` → 400; body con `[services]`/`[wake].command` → 400; config segura (embedding, puerto, silva) → 200; el fichero no se reescribe si se rechaza.
- **Dependencias:** ninguna; no toca la lectura de config.

### C1. ACL fail-closed real (sin `default_role` para guilds sensibles)

- **Qué cambia:** `resolve_role_for_token` (`auth.rs`, líneas 136-139) y el ACL de guilds (`api_guilds.rs`, líneas 50-68) dejan de tratar un token no listado como `default_role` cuando la guild es sensible (`bash`, `filesystem`, `code`, `docker`, `git`); se exige binding explícito (`acl.tokens` o `agents.toml`). El token maestro sigue siendo operador y conserva el acceso.
- **Riesgo que cierra:** que un token no bindeado entre como `admin` con `"*"` (hoy `default_role = "admin"`) y use `bash`/`filesystem` sin rol declarado.
- **Fricción:** **media/alta** — hay que declarar roles de los agentes actuales; los tokens sin binding se bloquean hasta configurarlos. Requiere plan de migración y comprobación de que José no pierde acceso.
- **Alcance técnico:** `auth.rs`, líneas 136-139 y `resolve_acl_role_inner` (líneas ~171-199); `api_guilds.rs`, líneas 50-68; `.tylluan/agents.toml`/`tylluan.toml` (config, no código).
- **Tests previstos:** token no listado + guild sensible → 403; token bindeado a rol con permiso → 200; operador → 200; matriz de roles; test de que el token maestro sigue funcionando.
- **Dependencias:** decisión de configuración de José (bindings); va después de C3 para reducir superficie antes de la migración. Si se aprueba sin migración, riesgo de bloquear agentes legítimos.

### C3. Endpoint directo de guild restringido a operador para `bash`/`filesystem`

- **Qué cambia:** en `guild_tool_call_handler` (`api_guilds.rs`, línea 50), antes de ejecutar, si `guild` es `bash` o `filesystem` y la clase de auth no es operador (`MasterToken`/`DevBypass`) → 403 con mensaje claro; el resto de guilds siguen agent-class. Aplica a `/call/` y `/tools/` porque comparten handler (`routes.rs`, líneas 32 y 139).
- **Riesgo que cierra:** que un agente ejecute comandos o lea ficheros por la puerta HTTP directa (fila 7 de T-DSH-69), saltándose el camino `tylluan_do`.
- **Fricción:** **baja en el repo** (T-DSH-71: 0 consumidores in-repo para `bash`/`filesystem`; solo el dashboard de visión usa `/call/`, y es `vision`); **desconocida fuera** (scripts/curl externos). Los agentes siguen usando MCP `tylluan_do` sin fricción.
- **Alcance técnico:** `api_guilds.rs`, línea 50; lista de guilds sensibles configurable; tests en `auth.rs` (líneas 1038-1039) y `api_guilds.rs` para el caso `bash`/`filesystem`.
- **Tests previstos:** agente + `bash`/`filesystem` → 403; agente + `coloquio`/`llama_backend` → 200; operador + `bash` → 200; alias `/tools/` cubierto; auditoría de la denegación.
- **Dependencias:** ninguna; independiente de C1 (C3 es clase de ruta; C1 es modelo de roles). Si José quiere un cierre rápido, C3 puede adelantarse a fase 1 (coste in-repo 0).

### D1. Descubrimiento de MCP como propuesta (diff + aprobación), sin registro automático

- **Qué cambia:** `POST /api/v1/mcp/external/discover` deja de registrar y arrancar servers; devuelve una lista de propuestas con el comando y el diff contra lo ya configurado, y solo se registra lo que se apruebe explícitamente (reutiliza el mecanismo de A4).
- **Riesgo que cierra:** que un fichero de perfil de terceros manipulado se convierta en ejecución de proceso sin intervención humana.
- **Fricción:** **baja** — descubrir sigue siendo cómodo; +1 confirmación por servidor nuevo.
- **Alcance técnico:** `api_mcp.rs`, líneas 311-353 (y el endpoint de aprobación que se añada con A4).
- **Tests previstos:** discover con 2 servers → 0 registrados, 2 propuestas; aprobar una → 1 registrado y spawn; el fichero de config no se modifica hasta aprobar.
- **Dependencias:** se apoya en A4 si este se aprueba; puede implementarse solo (con su propio mecanismo de aprobación).

---

## 5. Orden recomendado (propuesta, no decisión)

| Fase | Controles | Por qué en este orden | Fricción añadida | Requiere migración de config |
|---|---|---|---|---|
| **1** | **A4 + D1** | Cierran las **dos vías de registro silencioso** de procesos (alta/edición y descubrimiento) sin tocar configuración ni ACL; fricción baja y operaciones poco frecuentes | Baja | No |
| **2** | **B1 + A1** | Cierran la siembra de procesos vía config y el wrapper de shell; B1 define la lista de claves escriturables y A1 el manejo de `.cmd`/`.bat`. Sin migración de config | Baja/media | No (solo definir listas) |
| **3** | **C3** | Cierra la puerta más expuesta (agente → `bash`/`filesystem` por HTTP) con coste in-repo cero (T-DSH-71); se hace antes de C1 para reducir superficie mientras se diseña la migración | Baja (repo) / desconocida (externos) | No |
| **4** | **C1** | Cambio sistémico del ACL: exige decidir bindings de roles de los agentes y convivir con `dev_mode = true`/`default_role = "admin"`; riesgo de bloquear agentes si no hay migración. Se hace al final, con plan y con la regla «nunca perder el acceso» | Media/alta | **Sí** |

- **Si José quiere el cierre más rápido:** C3 es independiente y su coste in-repo es 0; puede adelantarse a fase 1 sin afectar a las demás. Lo mismo A4 (no depende de nada).
- **C1 no debe ir antes de C3** salvo que José lo pida: toca la semántica de auth de producción y necesita preparar bindings; C3 reduce entre tanto la superficie expuesta.
- **Cada control es independiente:** se puede aprobar solo A4, o solo C3, sin aprobar el resto. La tabla es una recomendación de secuencia, no un paquete indivisible.

---

## 6. Alternativas descartadas o fuera de esta propuesta

| Opción | Por qué queda fuera |
|---|---|
| **A2** (allowlist de ejecutables MCP) | Reduce más el riesgo que A1, pero exige mantener una lista y permite `npx`/`uvx`/`python`/`node`; decisión posterior con datos de uso. Se puede añadir sobre A1. |
| **A3** (confinar `cwd` y `env`) | Rompe servers legítimos que necesitan HOME/perfil o variables de API; requiere declaración por servidor. Mejor como fase propia con inventario de servers reales. |
| **B2/B3** (mantener config crudo con diff, o deshabilitar el guardado crudo) | B2 no impide el abuso, solo lo hace visible; B3 cambia el flujo del dashboard para todo. B1 es el punto medio propuesto; si José prefiere B2/B3, sustituyen a B1 sin tocar el resto. |
| **D2/D3** (no importar `env`/`cwd`, o desactivar discover) | D2 complementa a D1 pero puede romper servers que dependen de `env`; D3 elimina comodidad de onboarding. D1 conserva la comodidad con aprobación. |
| **E1/E2/E3** (`/api/v1/bash`, HITL en dev, `dev_mode=false`) | Es la fila 10 de T-DSH-69, fuera del encargo de este ADR; E3 es cambio de configuración y `dev_mode` es decisión de producto de José. Se pueden tratar en un ADR/decsión posterior. |
| **C3 aplicada a todas las guilds** | Rompería el dashboard de visión, `night_reasoner.py`, el juez de `coherence_gate.rs` y benchmarks (T-DSH-71: uso real de `/call/`/`/tools/` para `vision` y `llama_backend`). C3 se propone solo para `bash`/`filesystem`. |

---

## 7. Consecuencias

**Positivas**
- Cierra las dos cadenas completas de ejecución de procesos desde HTTP (MCP externo y config cruda → services/wake) y la puerta directa agente → `bash`/`filesystem`, con fricción baja en el repo.
- C3 y A4 son prácticamente gratis en coste de uso; B1 y D1 no cambian el día a día de los agentes.
- Deja a José decidir el cambio sistémico (C1) con datos y con un plan de migración, no como efecto colateral.

**Negativas / costes**
- A1 tiene un caso Windows incómodo (`.cmd`/`.bat`) que hay que mantener acotado.
- B1 obliga a definir y documentar la lista de claves escribibles; si se olvida una clave legítima, el dashboard dará 400 hasta corregirla.
- C3 puede romper consumidores externos al repo (scripts sueltos, curl manuales); T-DSH-71 no pudo auditarlos.
- C1 puede bloquear agentes legítimos sin binding si se aprueba sin migración; exige plan y verificación con José.

**Riesgos residuales aceptados si se aplica todo**
- Un token de operador comprometido sigue pudiendo dar de alta/editar MCP externo y guardar config no-sensible; A1/A4/B1 hacen el abuso visible y acotado, no imposible.
- Las guilds sensibles siguen ejecutando con los privilegios del usuario; la contención fuerte es ADR-022 (sandbox), no este ADR.
- El endpoint directo sigue existiendo para el resto de guilds (visión, llama_backend, coloquio).

---

## 8. Plan de implementación y DoD por fase (si se aprueba)

1. **Fase 1 — A4 + D1:** PR(s) con `log_audit_entry` en alta/edición/discover, validación de confirmación, tests de handler; DoD: tests nuevos en verde, `clippy -D warnings`, sin cambios de comportamiento en `/api/v1/do`/MCP.
2. **Fase 2 — B1 + A1:** filtro de claves en `save_config_handler` con tests de rechazo/aceptación; cambio de spawn con tests de argv; DoD: mismos gates + prueba manual de guardado seguro y de alta MCP nativa.
3. **Fase 3 — C3:** comprobación por guild dentro del handler, tests de matriz (agente/operador × bash/filesystem/coloquio) y actualización de los tests de `auth.rs`; DoD: agentes siguen usando `tylluan_do` sin cambios.
4. **Fase 4 — C1:** diseño del binding de roles con José, migración documentada, tests de no-bloqueo del token maestro; DoD: matriz de roles completa y comprobación de que ningún agente legítimo queda fuera sin aviso.

Cada fase es un PR independiente; ninguna se ejecuta sin la aprobación de José de su control.

---

## 9. Qué debe decidir José (punto por punto)

1. **A1** — ¿se lanza directo sin `cmd /c` cuando el comando es ruta absoluta, manteniendo wrapper solo para `.cmd`/`.bat`?
2. **A4** — ¿se exige confirmación humana + auditoría del diff en el alta/edición de MCP externo?
3. **B1** — ¿se bloquean desde la API las claves `[[external_mcp]]`/`[services]`/`[wake].command`, dejando su edición solo por fichero?
4. **C3** — ¿se restringe el endpoint directo a operador para `bash`/`filesystem` (y solo esas dos, o también `code`/`docker`/`git`)?
5. **C1** — ¿se implanta ACL fail-closed sin `default_role` para guilds sensibles? Si sí, ¿cómo se declaran los roles de los agentes actuales y qué hacemos con `dev_mode = true` y `default_role = "admin"` de la config de producción?
6. **D1** — ¿el descubrimiento de MCP se convierte en propuesta con aprobación, perdiendo el registro automático?
7. **A2/A3/E** — ¿se dejan para un ADR/decisión posterior (recomendado) o José quiere incluirlos ya?

---

## 10. No verificado

- **Análisis estático y de solo lectura:** no arranqué el kernel ni probé las rutas con tokens; las líneas se verificaron contra `ca76e1b` (reset antes de redactar). Los efectos de fricción son estimaciones de T-DSH-70, no mediciones con agentes reales.
- **C3:** el coste in-repo está medido (T-DSH-71: 0 consumidores), pero el uso externo al repo no se puede auditar por grep; los logs cubren este kernel y ventanas limitadas (5 h, 4 días, histórico junio-julio).
- **C1:** no probé en vivo la resolución de rol para JWT OAuth ni credencial de guild; la afirmación se apoya en `resolve_role_for_token`/`resolve_acl_role_inner` y en la config actual. El riesgo de bloqueo de agentes es real y no está medido.
- **A1:** no probé el spawn de `.cmd`/`.bat` en Windows en vivo; el caso está deducido de `guild_process.rs`.
- **Índice de ADRs:** no he actualizado `ARCHITECTURE_DECISIONS.md` (ADR-019 a ADR-022 tampoco figuran en el índice); si el TL quiere que el ADR-024 aparezca, es una entrada de una línea que dejo a su criterio.
- **Ninguna decisión implementada:** este documento es una propuesta; sin aprobación de José no se toca código.
