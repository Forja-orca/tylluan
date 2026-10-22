# ADR-022: Modelo de confianza de extensiones y sandbox de ejecución

- **Estado:** 📐 **PROPUESTO** — borrador del Tech Lead (Claude). **La aprobación es de José**; nada de lo que sigue se implementa como decisión hasta que la dé punto por punto (§9).
- **Fecha:** 2026-10-22
- **Origen:** petición de José (Coloquio T1510): ¿qué son las guilds en 2027?, ¿conector MCP universal tipo IDE?, ¿encapsular Tylluan (Podman, mini-Linux del agente) y cuándo? Opinaron DSH (T1511), Deep (T1512), Antigravity (T1514), el TL (T1515-T1516, T1523) y Buffy (ronda T1518).
- **Insumos (todos en el repo):**
  - `docs/reference/research/2026-10-22_sandboxing_de_agentes_como_lo_hacen_los_grandes.md` (cómo lo hacen Codex, Claude Code, Grok Build, SPACE, AutoGLM).
  - `docs/internal/IMPL_T-DSH-26_capacidades_guilds_2026-10-21.md` y `docs/internal/IMPL_T-DSH-28_vias_escritura_2026-10-22.md` (inventarios de DSH con `archivo:línea`).
  - `docs/reference/research/2026-10-22_H2_conectores_mcp_en_ides_hermes.md` y `..._H3_sandbox_fuentes_pendientes_hermes.md` (fuentes primarias de Hermes; un solo lector, ver §10).
  - `docs/reference/adr/ADR016_nvidia_openshell_runtime.md` (NO-GO de OpenShell como dependencia) y `docs/architecture/repo_to_guild_sandbox_spec.md` (Sandbox OCI, Podman preferido).
  - `docs/internal/IMPL_asi06_redteam_envenenamiento_buffy_2026-10.md` y `docs/internal/IMPL_m-buffy-2_redteam_vias_juez_procedencia_buffy_2026-10.md`.
- **Ámbito:** quién puede ejecutar código dentro y alrededor de Tylluan, con qué privilegios y con qué aislamiento; cómo se incorporan servidores MCP de terceros; cómo se conectan los agentes si el kernel se encapsula. **Fuera de ámbito:** el envenenamiento de memoria (D-31, D-32, D-33), la identidad de agente (binding token a `agent_id`) y `dev_mode`: son decisiones aparte, pero este ADR las cruza y lo dice (§8).

---

## 1. Resumen en diez líneas

1. Hay **tres clases de confianza**, no dos: núcleo del kernel (T0), guilds propias (T1) y conectores/código de terceros (T2).
2. Hoy las guilds corren con **los privilegios de tu usuario** y reciben **el token maestro** del kernel. Una guilda comprometida es operador.
3. La comprobación de capacidades **existe pero está apagada por defecto**, solo cubre procesos y ficheros, y es un filtro de argumentos, no una contención.
4. Regla central, **fail-closed**: lo que ejecuta código ajeno (conectores, `bash`, `code`) se desactiva si no hay un runtime de sandbox. El perfil `portable` sigue nativo y ligero, sin código ajeno.
5. Escalera de aislamiento de cuatro niveles (N0 a N3) detrás de **una interfaz común** con un backend por sistema operativo; no existe «agnóstico al SO» en sandbox fuerte.
6. **Manifiesto de capacidades** obligatorio para T2 y, por fases, para T1.
7. **Credencial de guilda distinta del token maestro**, aditiva (no se rota nada).
8. Conectores: **cliente de registros + lista de permitidos curada**, nunca un marketplace propio; todo lo que devuelven es dato no confiable.
9. El contrato de conexión de los agentes **no cambia** (MCP por loopback con token, y un puente para stdio).
10. El mini-Linux del agente es un **producto aparte** (workspace con copia y diff), no parte del kernel.

---

## 2. Estado real verificado (árbol `27e3a57`, 2026-10-22)

| # | Hecho | Evidencia |
|---|---|---|
| H1 | Las guilds comparten usuario del SO con el kernel («Guilds share OS user with kernel») | `docs/concepts/SECURITY.md` línea 51 |
| H2 | El sandbox de guilds (`security.sandbox`) está **apagado por defecto** y solo soporta Docker | `tylluan.toml` `[security.sandbox] enabled = false`; `config.rs` (`SandboxConfig`) |
| H3 | La contención de capacidades es **opt-in y está apagada**: `capabilities_enforce = false` | `config.rs` (`default_capabilities_enforce`), `tylluan.toml` línea 221 |
| H4 | Cuando está encendida, solo cubre `process_execution` y `filesystem_scope`; `network_hosts` es solo orientativo; un guild sin `CAPABILITIES` «is never enforced» | `config.rs` líneas 1368-1376; `guild_process.rs` `enforce_capabilities` (líneas 182-239) |
| H5 | `enforce_capabilities` es un **filtro de argumentos al despachar**: mira el argumento `command` y heurísticas sobre el nombre de la herramienta; no limita lo que hace el proceso hijo | `guild_process.rs` líneas 163-239 |
| H6 | **Cada guilda stdio recibe `TYLLUAN_TOKEN`**, el token que acepta el kernel (el maestro) | `guild_process.rs` líneas 478-491 |
| H7 | La configuración de producción tiene `default_role = "admin"` y `dev_mode = true`: un agente **sin listar** en el contrato es administrador; figurar como `reader` **reduce** permisos | `tylluan.toml` líneas 198 y 230; `.tylluan/agents.toml` |
| H8 | De las 6 guilds auditadas solo `bash` declara `CAPABILITIES` (en todo el repo la constante existe en 3 guilds: `bash`, `vision` y `websearch`); `bash` ejecuta un shell crudo (la allowlist del primer token no es frontera), `git` no valida `cwd`, `code` lee fuera del workspace, `docker` habla con el daemon (capacidad de host) | `IMPL_T-DSH-26` |
| H9 | Ya existe consumo de servidores MCP externos por API (alta, baja, edición, descubrimiento) | `routes.rs` líneas 105-107 (`/api/v1/mcp/external`) |
| H10 | ADR-016 ya dijo NO-GO a OpenShell como dependencia (exige contenedor o microVM, rompe `portable` y ARM64) y conservó tres patrones de diseño | `ADR016_nvidia_openshell_runtime.md` |
| H11 | Existe una especificación previa de Sandbox OCI: «Podman preferido, Docker CLI de reserva» | `docs/architecture/repo_to_guild_sandbox_spec.md` |
| H12 | El sandbox **no cierra** el envenenamiento de memoria: de las seis vías de escritura, cuatro (y una séptima) no tienen defensa; la entrada es por confianza de datos, no por ejecución | `IMPL_m-buffy-2`, `IMPL_T-DSH-28` |

## 3. Decisiones ya tomadas por José que este ADR respeta

1. «La seguridad no debe suponer un estorbo de trabajo ni a agentes ni a humanos» (2026-10-18): presupuesto de fricción por cada control.
2. Nunca perder el acceso: **se pueden generar y rotar tokens, pero José no puede quedarse sin acceso** (2026-10-21). Este ADR no rota el token maestro ni `.tylluan-token`.
3. Hardware modesto como ancla de diseño (médico, Raspberry Pi, sin internet): ningún control puede exigir una VM permanente en el perfil `portable`.
4. Red por equipos, sin malla global (2026-10-13) y `dev_mode=false` solo después de OAuth fase 2 (2026-10-18).
5. Tylluan usa CLI genérico de contenedores, **nunca depende de Docker Desktop**.

## 4. Decisión propuesta

### 4.1 Tres clases de confianza

| Clase | Qué es | Dónde corre | Confianza |
|---|---|---|---|
| **T0 Núcleo** | Memoria, router, seguridad, decisión, Coloquio | Dentro del kernel (Rust) | Plena; se firma y se versiona con el kernel |
| **T1 Guilds propias** | Código nuestro en Python (45 enrutables) | Subproceso, hoy con los privilegios del usuario | **Semiconfiable**: código nuestro, pero con dependencias de terceros y con capacidad de ejecutar |
| **T2 Terceros** | Servidores MCP externos, herramientas de IDE, cualquier cosa de un registro | Siempre aislado | **No confiable**; lo que devuelve es dato no confiable (`untrusted_content`) |

La separación es una **etiqueta de confianza con consecuencias de aislamiento**, no una arquitectura distinta: T1 y T2 llegan al kernel por la misma puerta (`call_tool`), y lo que cambia es qué se les entrega y dónde corren.

### 4.2 Regla fail-closed

Una capacidad que ejecuta código ajeno (conectores T2, y las guilds `bash`, `code` y `docker` de T1) **se desactiva si no hay un runtime de sandbox disponible**. El perfil `portable` queda sin código ajeno. Quien quiera conectores instala el runtime. Esto reconcilia seguridad y «cómputo tostadora»: el coste de aislamiento solo lo paga quien activa lo que lo necesita.

### 4.3 Escalera de aislamiento detrás de una interfaz común

| Nivel | Mecanismo | Cuándo | Coste |
|---|---|---|---|
| **N0** | Nativo, sin sandbox | Perfil `portable`; solo T0 y T1 sin ejecución de terceros | Nulo |
| **N1** | Primitivas del SO: `bwrap` + `seccomp` (Linux), Seatbelt (macOS); en Windows, vía WSL2 | Ejecución puntual (`bash`, `code`) y conectores de bajo riesgo | Bajo, sin VM en Linux/macOS |
| **N2** | Contenedor sin privilegios (Podman o Docker, **runtime configurable**): rootfs de solo lectura, usuario no root, montajes declarados, red por política | Conectores T2 por defecto; kernel como despliegue soportado | Medio; en Windows y macOS Podman requiere una máquina virtual |
| **N3** | microVM (Firecracker; SPACE de Perplexity como referencia Apache-2.0 en Rust) | Código no confiable de alto riesgo | Alto; requiere KVM |

Se define **una interfaz** (`SandboxRuntime`: preparar, ejecutar con política, recoger resultado como diff, destruir) y **un backend por sistema operativo**. Los números de arranque y memoria de cada backend **se miden con nuestra carga real** (`cargo build` y una guilda Python) antes de fijar el backend por defecto de cada SO; ninguna cifra de este ADR proviene de documentos de marketing.

### 4.4 Manifiesto de capacidades

Cada guilda y cada conector declara, en un manifiesto leído por el kernel:

- `process_execution` (lista de binarios o `false`), `filesystem_scope` (rutas, por defecto solo su espacio de trabajo), `network_hosts` (dominios y métodos permitidos, por defecto vacío), `container_daemon` (nuevo: acceso al daemon es capacidad de host), `kernel_credential` (nuevo: qué clase de credencial del kernel necesita, §4.5), `env_inherit` (nuevo: si hereda el entorno del kernel), `timeouts`.
- **Valor por defecto para T2: denegar todo.** Para T1: se declara primero (fase F0) y se aplica después (`capabilities_enforce` encendido por defecto cuando las guilds núcleo estén declaradas y se haya medido qué se rompe).
- El inventario de DSH deja claro qué declarar: `bash` tiene shell real y su allowlist es defensa en profundidad, no frontera; `code` debe validar rutas o declarar lectura de host; `git` debe forzar el espacio de trabajo; `docker` declara `container_daemon`; `browser` queda declarada inactiva.

### 4.5 Credencial de guilda distinta del token maestro

Hoy cada guilda recibe el token maestro (H6). Se propone una **credencial adicional de clase agente**, emitida por el kernel solo para guilds, con el alcance mínimo que usan de verdad (por ejemplo `silva/node`, `memory/write`, `coloquio/*`) y **sin** rutas de operador (administración, mantenimiento, configuración, despachos, aprobaciones). `guilds/core/kernel_auth.py` recoge esta credencial en lugar de la maestra.

Es **aditiva**: el token maestro y `.tylluan-token` no se tocan ni se rotan, y el kernel sigue funcionando igual para un cliente con el Bearer maestro (regla de José). Esta pieza (D-34) es independiente del sandbox y, sola, ya reduce el daño de una guilda comprometida de «operador» a «agente».

### 4.6 Conectores MCP de terceros: cliente de registros y lista de permitidos

- **No se construye un marketplace.** Curar y responder legalmente por él es una carga. Se construye un **cliente** del registro oficial de MCP y de otros que se configuren, más una **lista de permitidos curada por el operador**.
- El registro oficial se describe a sí mismo como «a community driven registry service», está en preview y avisa de que «breaking changes or data resets may occur» (README del repositorio `modelcontextprotocol/registry`): sirve de fuente de descubrimiento, **no de garantía**. Verifica la propiedad del espacio de nombres al publicar (GitHub OAuth/OIDC, DNS o HTTP).
- Cada conector admitido lleva: **versión fijada y hash**, manifiesto de capacidades, permisos **por herramienta** (permitir, preguntar, bloquear), secretos por el corredor de §4.7, auditoría de cada llamada, y su salida marcada `untrusted_content`.
- Patrones tomados de los IDE (H-2, fuentes de Hermes): lista de permitidos y de denegados con precedencia de la denegación (VS Code), política por herramienta `allow`/`ask`/`blocked` (Claude Desktop gestionado), configuración de proyecto más global fusionadas (Cursor), aprobación por defecto antes de usar una herramienta.
- **Regla dura:** ningún conector de terceros antes de que exista N2 y el manifiesto aplicado. Los servidores MCP externos arrancan **envueltos por nuestro lanzador**: sin eso corren sin restricción en el anfitrión (advertencia explícita de la documentación de Claude Code).
- Los conectores entran por el router como un tipo propio, no por la superficie MCP: se mantiene CONTRACT-01 (seis herramientas soberanas).

### 4.7 Corredor de secretos y red en dos fases

- **Los secretos no se montan ni se pasan por entorno al sandbox.** Un proxy del lado del kernel los guarda y el sandbox ve un marcador. Patrón verificado en Docker Sandboxes («the key itself never enters the sandbox») y en Claude Code en la nube (un proxy guarda el token de GitHub).
- **Dos fases de red** para conectores: instalar/descargar con una lista de dominios; ejecutar sin red por defecto. Si se abre, lista de dominios y de métodos (Codex recomienda solo `GET`, `HEAD` y `OPTIONS`).
- **Rutas protegidas de solo lectura** en cualquier espacio de trabajo: `.git`, la configuración de Tylluan, el directorio de datos, el token, las claves de federación y los scripts de arranque. Los perfiles base de Grok no protegen `~/.ssh`: los nuestros llevan lista `deny` propia.
- **Resultado como diff:** el espacio de trabajo es una copia; el operador (o el modo plan) aprueba el diff antes de aplicarlo al proyecto real.

### 4.8 Aprobaciones y aislamiento son dos ejes

Las aprobaciones deciden **si** algo se ejecuta; el aislamiento limita **qué alcanza** una vez ejecutado. Ni el juez de la capa 2 ni un clasificador cuentan como frontera. Lo dicen con casi las mismas palabras Anthropic («Isolation restricts what a command can access once it runs») y xAI («Permissions gate whether a tool call runs; the sandbox limits what an approved call can do»).

### 4.9 Conexión de los agentes y encapsulado del kernel

- **El contrato no cambia.** MCP define dos transportes estándar, stdio y Streamable HTTP, y «Clients SHOULD support stdio whenever possible»; para HTTP exige validar `Origin`, enlazar a localhost y autenticar. Con el kernel encapsulado: el mismo URL en loopback con token, y para clientes que solo hablan stdio un **puente en el host**. Ningún IDE cambia su configuración.
- **El kernel en contenedor es un artefacto soportado** (un `Containerfile` endurecido: sin privilegios, rootfs de solo lectura, puerto publicado solo en `127.0.0.1`), **no el modo por defecto del desarrollo** ni del perfil `portable`.
- **Separación de dominios:** la memoria (datos sensibles) no comparte dominio con los ejecutores (capacidad peligrosa). Un contenedor del kernel entero no cambia el modelo de amenaza si las guilds siguen montando el espacio de trabajo y recibiendo el token maestro; por eso §4.4, §4.5 y §4.7 van **antes** de encapsular por defecto.
- **El «ordenador del agente» (mini-Linux):** posible, pero como **producto aparte**, un espacio de trabajo por agente (contenedor o microVM con copia y diff, cuotas e identidad). Los agentes (Claude Code, OpenCode, Codex, Cursor) corren en la máquina del usuario con su propio sandbox; el mini-Linux no es su anfitrión sino un **destino de ejecución** al que las guilds `bash` y `code` envían comandos.

## 5. Fases y criterios de salida medibles

| Fase | Contenido | Criterio de salida (comando y resultado esperado) |
|---|---|---|
| **F0** (sin cambio de comportamiento) | Declarar `CAPABILITIES` en las guilds que no lo hacen; medir qué se rompe con `capabilities_enforce = true` en un kernel de prueba; runtime del sandbox configurable (`docker`\|`podman`); `Containerfile` del kernel con prueba de arranque en CI opcional | Tabla guilda × declaración completa; informe «qué se rompe» con 0 sorpresas sin documentar; `cargo test` verde; imagen que arranca y responde en loopback |
| **F1** (privilegios) | Credencial de guilda (D-34); manifiesto con esquema y analizador; `network_hosts` deja de ser orientativo para T2 | Una guilda de prueba con la credencial nueva recibe 403 en una ruta de operador y 2xx en `silva/node`; el Bearer maestro sigue funcionando |
| **F2** (conectores) | Cliente de registros, lista de permitidos, versión fijada y hash, permisos por herramienta, interfaz en el dashboard, arranque envuelto en N2 | Un conector de prueba fuera de la lista no arranca; con hash distinto no arranca; su salida llega marcada `untrusted_content` |
| **F3** (backends por SO) | N1 nativo (`bwrap`, Seatbelt) y medición de Podman en Windows con carga real | Tabla backend × SO × arranque × RAM medida por nosotros |
| **F4** (tras v1.0 o cuando el producto lo pida) | Encapsulado por defecto para usuarios finales y espacio de trabajo de agentes | Decisión de José con los datos de F3 |

## 6. Alternativas consideradas

| Alternativa | Veredicto | Motivo |
|---|---|---|
| Adoptar NVIDIA OpenShell como dependencia | **No** (ADR-016) | Exige contenedor o microVM; rompe `portable` y ARM64. Se conservan tres patrones |
| Encapsular el kernel entero por defecto, ya | **No ahora** | No cambia el modelo de amenaza mientras las guilds tengan el token maestro y monten el espacio de trabajo; coste de GPU/ONNX, red de host para P2P y mDNS, rendimiento de ficheros |
| Marketplace propio | **No** | Curación y responsabilidad legal; basta un cliente de registros con lista de permitidos |
| WebAssembly para conectores | **Opción futura** | Independiente del SO, pero no ejecuta `bash` ni Python sin un runtime aparte; sirve para conectores compilados a Wasm |
| microVM para todo (Firecracker) | **N3 opcional** | Requiere KVM; cifras de arranque no medidas por nosotros |
| gVisor | **No como defecto** | Su propia documentación avisa de «poor performance for system call heavy workloads» y de compatibilidad incompleta de llamadas al sistema |
| Hospedar a Claude Code, Cursor y OpenCode dentro del mini-Linux | **No** | Corren en la máquina del usuario con su propio sandbox; es otro producto |

## 7. Riesgos y límites declarados

- **El sandbox no cierra el envenenamiento de memoria.** Es el trabajo de D-31, D-32 y D-33 (y de la identidad). Aislar ejecución no protege contra datos hostiles que entran por una vía de escritura sin defensa.
- **Windows y macOS:** Podman necesita una máquina virtual (WSL por defecto o Hyper-V en Windows), con coste de memoria justo en hardware modesto; el bloqueo de red de procesos hijos de los perfiles de Grok solo se aplica en Linux. La interfaz común existe precisamente para no prometer lo mismo en todos los sistemas.
- **`default_role = "admin"` con `dev_mode = true`** anula buena parte del valor del contrato de agentes mientras siga así; no se cambia aquí (§9).
- **Rotura de guilds al encender `capabilities_enforce`:** se mide en F0 antes de decidir; las guilds sin `CAPABILITIES` no se ven afectadas, pero declarar mal puede bloquear herramientas legítimas.

## 8. Relación con otras decisiones abiertas

- **Binding token a `agent_id`:** aislar no sirve si `agent_id` es un texto declarado; cualquier agente dentro de un sandbox necesitará una credencial propia. Hasta que José decida, D-34 cubre solo a las guilds.
- **`dev_mode=false` tras OAuth fase 2:** este ADR no lo adelanta.
- **ASI06:** D-31 (normalizar antes de comparar), D-32 (defensa en las vías sin ninguna) y D-33 (juez cableado) son independientes y de más prioridad.

## 9. Decisiones que se piden a José

1. **Guild = plugin interno, y conectores MCP como clase aparte (T2)** (decisión 1 de Deep): recomendación **sí**.
2. **Runtime de sandbox configurable, Podman o Docker**, no atado a uno (decisión 2): recomendación **sí**.
3. **Contenedor del kernel como despliegue soportado, no por defecto en el desarrollo** (decisión 3): recomendación **sí**.
4. **Mini-Linux como producto separado** (decisión 4): recomendación **sí**.
5. **Regla fail-closed** (§4.2): sin runtime, se desactiva lo que ejecuta código ajeno: recomendación **sí**.
6. **Credencial de guilda aditiva** (§4.5): recomendación **sí**, sin rotar el token maestro.
7. **Autorizar a DSH a medir Podman y Docker Sandboxes en tu Windows** (instalar software y bajar una máquina virtual de varios GB): necesaria para fijar el backend de F3.
8. **Si se enciende `capabilities_enforce` por defecto** una vez medida la rotura en F0.

**Qué dato me haría cambiar de opinión:** si la medición de F0 muestra que ninguna guilda necesita ficheros ni red fuera de su carpeta, el manifiesto basta y N2 puede esperar; si Podman en tu Windows consume más de lo que cabe en el perfil `clinic`, N1 vía WSL2 pasa a ser el nivel por defecto en Windows; si el registro oficial cambia su formato antes de F2, el cliente se retrasa sin afectar al resto.

## 10. No verificado

- Las citas de Hermes (H-2, H-3) tienen un solo lector; el TL no ha reabierto las fuentes que usa este ADR. En H-2, algunas celdas de VS Code reutilizan documentación de Cursor, y así consta en su fichero.
- Cifras de arranque y memoria de Firecracker, gVisor, Podman y SPACE: no medidas por nosotros ni publicadas en sus README.
- Si `check_capabilities` (que DSH describe ligado al perfil de sandbox) y `enforce_capabilities` (ligado a `capabilities_enforce`) son dos mecanismos distintos o uno solo con dos puertas: pendiente de contraste en F0.
- La tarjeta del sistema de ChatGPT (modo agente), la documentación oficial de Gemini sobre ejecución de código y las imágenes de uso de ordenador de Anthropic: pendientes de H-3.

## Impact

- **Solo documentación.** No cambia código, configuración ni comportamiento. Todo punto con consecuencia (§9) queda como decisión de José.
