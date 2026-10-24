# T-DSH-70 — Resumen de decisiones de seguridad para José: quién puede lanzar procesos y llegar a guilds con entrada externa

- **Agente:** dsh
- **Fecha:** 2026-10-24
- **Base:** `origin/main` `e85a6b7` (push144). **Solo lectura**: no se ha tocado código; este documento es el único artefacto.
- **Fuente:** filas 1, 2, 3, 7, 9 y 10 de T-DSH-69 (informe `docs/internal/IMPL_T-DSH-69_paths_procesos_2026-10-24.md`). Las filas 7 y 8 fueron verificadas en el árbol por el TL (T1715); el resto las verifiqué yo contra `e85a6b7`.
- **Objetivo:** que José decida. Cada opción se describe por el riesgo que cierra, la fricción que añade a agentes/operador legítimos y una recomendación **no vinculante**. Este documento no aprueba nada.

## 1. Quién puede lanzar procesos hoy (hechos, no propuestas)

- **Clase operador = solo token maestro por cabecera `Authorization` (o `dev_mode` local).** `operator_gate_allows` acepta únicamente `MasterToken` y `DevBypass` (`crates/tylluan-kernel/src/transport/http/auth.rs`, líneas 336-345). El token maestro por query-string **no** vale para rutas de operador (líneas 62-66); un JWT OAuth tampoco (líneas 67-68); la credencial de guild **nunca** cruza a operador salvo la excepción auditada de `POST /api/v1/silva/edge` (líneas 69-73 y 330-334).
- **Rutas de operador que tocan las filas 1, 2, 3, 9 y 10:** `operator_class_required` marca `/api/v1/config*` y `/api/v1/mcp/external*` (líneas 261-266), `/api/v1/bash` (línea 273) y `/api/v1/dispatches/` (línea 268). Es decir: **las cinco rutas de lanzamiento de proceso/discovery exigen token maestro de operador**, no un token de agente.
- **La fila 7 (llamada directa a guild) es agent-class**, no operador: `/api/v1/guilds/{guild}/call/{tool}` solo exige un token de agente/JWT/guild que resuelva a un rol ACL (`api_guilds.rs`, líneas 50-68). El ACL comprueba `acl.roles` no vacío (línea 55) y `resolve_role_for_token` cae a `default_role` si el token no está listado (auth.rs, líneas 136-138).
- **Estado actual de la config de producción (leída hoy, solo lectura):** `dev_mode = true`; `[security.acl]` con `default_role = "admin"` y `roles.admin = ["*"]`. Consecuencias concretas: (a) la fila 10 (`POST /api/v1/bash`) está **activa** en producción; (b) en la fila 7 el chequeo ACL se ejecuta pero un token no listado se resuelve como `admin` con `"*"`, así que el ACL no está frenando a tokens no bindeados.
- **Lo que no he verificado:** no arranqué ni probé el kernel; esto es lectura de config y de código, no una prueba en vivo de tokens.

## 2. Qué cubre ya la clase operador (y qué no)

- **Cubre:** no hay una ruta sin credencial para las filas 1, 2, 3, 9 y 10; el token por query-string no basta; una credencial de guild no puede usarlas (salvo silva/edge, auditada). Un tercero sin el token de operador no llega a lanzar procesos por estas rutas.
- **No cubre:** una vez que eres operador **no hay guardia intra-operador**. No hay allowlist de comandos, ni confirmación HITL específica para "alta de MCP externo", ni diff/auditoría de la escritura de `tylluan.toml`, ni restricción de `cwd`/`env` del proceso lanzado. El riesgo residual es: token de operador filtrado/robado, extensión del dashboard comprometida, o el propio operador equivocándose — y en ese caso el kernel ejecuta sin fricción.
- **Fila 7:** el riesgo no es "sin credencial", es "credencial de agente + ACL que por defecto es admin" (ver estado actual arriba).

## 3. Tabla de opciones (riesgo → fricción → recomendación no vinculante)

> Fricción = coste para agentes legítimos y para el operador. «Baja» = no cambia el flujo normal o +1 clic ocasional; «media» = hay que configurar/excepcionar algo la primera vez; «alta» = cambia el flujo de trabajo o bloquea casos hoy legítimos hasta adaptarlos.

### A. Alta/edición de MCP externos (filas 1 y 2: `POST`/`PUT /api/v1/mcp/external`)

| Opción | Riesgo que cierra | Fricción | Recomendación (no vinculante) |
|---|---|---|---|
| **A1.** Ejecución directa sin `cmd /c` para rutas absolutas verificadas; mantener el wrapper `cmd` solo para `.cmd`/`.bat` conocidos | Que un `command` con espacios/flags de shell pase por `cmd.exe` (inyección de argumentos al intérprete) | **Baja** para servidores nativos (`.exe`); **media** si el servidor es `.cmd`/`.bat` (hay que mantener el wrapper restringido, que además es necesario en Windows para npx/npm) | Buen primer paso, coste bajo; no cierra el fondo (sigue permitiendo ejecutar cualquier binario si se acepta) |
| **A2.** Allowlist de ejecutables en config (p. ej. `allowed_commands`) | Ejecutar *cualquier* binario desde el body; un MCP server malicioso importado | **Media**: cada MCP nuevo fuera de la lista requiere editar config; npx/uvx/python/node necesitan permitir sus rutas/interprete. Afecta al operador, no a los agentes | Recomendable si José acepta mantener la lista; sin ella el resto de opciones solo reduce, no cierra |
| **A3.** `cwd` confinado al workspace + `env` limitado a claves declaradas por servidor | Hijack de DLL/PATH vía `env` y lectura de directorios fuera del workspace por el proceso hijo | **Media/alta**: rompe servidores que necesitan HOME/perfil (p. ej. navegador) o variables de API si no se declaran; solución: declarar por servidor en config, pero es trabajo inicial | Útil y acotable por servidor; el coste es la declaración inicial y posibles soportes a usuarios |
| **A4.** Confirmación humana explícita + audit del diff al dar de alta/editar un MCP externo | Uso indebido del token de operador, CSRF/extensión del dashboard, error del propio operador | **Baja**: alta/edición de MCP es una operación rara; +1 clic y ver el comando que se va a registrar | Muy buena relación beneficio/fricción; complementa A1-A3, no los sustituye |

### B. Guardado crudo de `tylluan.toml` (fila 3: `POST /api/v1/config`)

| Opción | Riesgo que cierra | Fricción | Recomendación (no vinculante) |
|---|---|---|---|
| **B1.** Allowlist de claves escribibles: bloquear `command`/`args`/`env` y las secciones `[[external_mcp]]`, `[services]`, `[wake]` desde la API | Que un body HTTP siembre un proceso que se ejecuta en el siguiente arranque | **Baja** para el 90% del editor de config; **media** para quien quiera tocar esas secciones desde el dashboard (puede seguir editando el fichero a mano) | La opción más limpia si el editor del dashboard no necesita esas secciones; requiere definir bien la lista |
| **B2.** Mantener el guardado crudo, pero con confirmación + diff + audit + forzar reinicio cuando cambien secciones sensibles | Escritura silenciosa de config que ejecuta procesos | **Baja**: revisar un diff antes de guardar | Buena si se quiere conservar el editor completo; no impide el abuso, solo lo hace visible y deliberado |
| **B3.** Deshabilitar el guardado crudo (solo edición manual del fichero) | Toda la superficie de la fila 3 | **Alta**: el operador pierde la edición de config desde el dashboard para todo | Seguridad máxima, pero cambia un flujo que hoy existe; solo si José prioriza eso |

### C. Llamada directa a guild (fila 7: `POST /api/v1/guilds/{guild}/call/{tool}`)

| Opción | Riesgo que cierra | Fricción | Recomendación (no vinculante) |
|---|---|---|---|
| **C1.** Fail-closed real: exigir binding explícito token→rol y no caer a `default_role` para guilds con `bash`/`filesystem` (hoy `default_role = "admin"` en producción) | Que un token no listado entre como `admin` con `"*"` | **Media**: hay que declarar roles/bindings de agentes; tokens actuales sin binding se bloquean hasta configurarlos | Es el arreglo de fondo del ACL; el coste es una migración de configuración, no de código de agentes |
| **C2.** Contención de paths en los args de `filesystem` (misma canonicalización que `sandbox_files_handler`) | Leer ficheros fuera del workspace desde un agente | **Media**: tareas legítimas fuera del workspace (logs del sistema, `~/.config`) dejan de funcionar sin excepción o copia al workspace | Recomendable como default; prever una válvula de escape por guild/ruta permitida |
| **C3.** Restringir `/guilds/{guild}/call/{tool}` a operador para `bash`/`filesystem` (agentes solo vía `tylluan_do`, que ya resuelve guild con ACL) | Que un agente ejecute comandos/lea ficheros por la puerta HTTP directa | **Alta** para agentes que hoy usan el endpoint directo; **baja** si ya usan `tylluan_do` | Cierra la puerta más expuesta, pero hay que confirmar que ningún agente legítimo depende del endpoint directo |

### D. Descubrimiento de MCP externos (fila 9: `POST /api/v1/mcp/external/discover`)

| Opción | Riesgo que cierra | Fricción | Recomendación (no vinculante) |
|---|---|---|---|
| **D1.** Importar como *propuesta* (diff + aprobación humana) y no registrar/spawnear hasta aprobar | Registro silencioso de comandos desde ficheros de terceros (Claude/Gemini/…) que pueden estar manipulados | **Baja**: +1 confirmación por servidor descubierto (operación ocasional) | Muy buena relación beneficio/fricción; conserva la comodidad de descubrir |
| **D2.** No importar `env`/`cwd` y validar `command` contra allowlist (A2) | Inyección de entorno y ejecución arbitraria desde un perfil manipulado | **Media**: servidores que dependen de `env` se configuran a mano | Complemento natural de A2/D1 si se quiere mantener el descubrimiento |
| **D3.** Desactivar `discover` | Toda la superficie de la fila 9 | **Alta** para onboarding (se pierde la comodidad de importar configs) | Solo si José considera que el descubrimiento no aporta lo suficiente |

### E. `POST /api/v1/bash` (fila 10, hoy activa por `dev_mode = true`)

| Opción | Riesgo que cierra | Fricción | Recomendación (no vinculante) |
|---|---|---|---|
| **E1.** Eliminar la ruta (ya está marcada deprecada) | Ejecución de comandos por HTTP desde entrada externa | **Baja**: `tylluan_do` sigue disponible; solo rompe scripts dev que usen el endpoint directamente | Cierra del todo y el coste parece bajo; hay que confirmar que nadie lo usa en dev |
| **E2.** Exigir confirmación HITL + audit también en `dev_mode` | Abuso y errores en dev | **Baja/media**: +1 paso por comando en dev | Buena si se quiere conservar la comodidad de dev |
| **E3.** Dejar la ruta pero poner `dev_mode = false` en producción (hoy es `true`) | Exposición de la ruta en producción | **Nula** en producción; cambia el flujo de dev | Arreglo inmediato de configuración, independiente de E1/E2; no cierra el riesgo si algún día se vuelve a activar dev_mode |

## 4. Preguntas abiertas para José (no las decido yo)

1. ¿El editor de config del dashboard necesita poder escribir `[services]`/`[[external_mcp]]`/`[wake]`, o basta con editar el fichero a mano (decide entre B1/B2/B3)?
2. ¿Acepta mantener una allowlist de ejecutables MCP (A2/D2), sabiendo que cada servidor nuevo fuera de la lista requiere configuración?
3. ¿`default_role = "admin"` en producción es intencional? Si no, C1 es la corrección de fondo y hay que decidir cómo se declaran los roles de los agentes actuales.
4. ¿`dev_mode = true` en producción es intencional? Si no, E3 es cambio inmediato de configuración (y E1/E2 deciden si la ruta se mantiene para dev).

## 5. Impact

- Da a José, en una sola tabla, **qué opción cierra qué riesgo y cuánto cuesta** en los seis puntos que el TL marcó (filas 1, 2, 3, 7, 9 y 10 de T-DSH-69), separando lo que ya protege la clase operador de lo que no.
- Hace explícito el hallazgo de configuración de producción (`dev_mode = true`, `default_role = "admin"`): dos riesgos de la lista están **activos hoy** por config, sin necesidad de tocar código (E3 y C1).
- No propone una decisión cerrada ni prioriza por José: cada recomendación está condicionada a lo que él acepte en fricción.

## 6. No verificado

- **No hay prueba en vivo:** no arranqué el kernel ni probé tokens/rutas; todo sale de leer `auth.rs`, `api_mcp.rs`, `api_admin.rs`, `api_guilds.rs`, `api_ops.rs` y la config de producción (snapshot de hoy, solo lectura).
- **Fricción estimada, no medida** con agentes/operador reales; es una estimación de coste operativo, no un dato.
- **`resolve_acl_role_inner`:** no verifiqué en vivo la resolución de rol para JWT OAuth ni para credencial de guild; la conclusión de C1 se apoya en `resolve_role_for_token` (auth.rs, líneas 136-138) y en el `default_role` de la config actual.
- **Filas 1, 2, 3, 9 y 10:** las verifiqué contra `e85a6b7`, pero el TL solo confirmó en su turno las filas 7 y 8; no hay validación independiente de comportamiento en runtime.
- **No comprobé** si algún script/agente legítimo usa hoy `POST /api/v1/bash` o `/guilds/{guild}/call/{tool}` directamente (afecta al coste real de E1/C3); habría que consultar logs de uso antes de decidir.
