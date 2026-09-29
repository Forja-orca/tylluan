# ADR-016: NVIDIA OpenShell 0.1.0 como Runtime de Control de Agentes

- **Estado:** 🔴 **NO-GO como dependencia del kernel** (bloqueante: exige substrato de contenedor/MicroVM, incompatible con el perfil `portable` y con el requisito ARM64/Pi 4 de `SPEC.md`) · 🟢 **GO para tres patrones de diseño a portar** (§5) · 🟢 **GO para receta de despliegue opcional** (§6, cero cambios de arquitectura) · 🟡 **Revisar cuando salga de alpha**
- **Fecha:** 2026-09-29
- **Autores:** Claude Opus 5 (auditor externo, evaluación documental) — pendiente de revisión por la flota
- **Ámbito:** `crates/tylluan-kernel/src/security/` (`ExecutionGuard`, `grants`, `hooks`), `guilds/builders/plugins/bash.py`, perfiles de instalación (`portable`/`clinic`/`server`), `docs/guides/`

---

## 0. Declaración de límites del evaluador

Cumpliendo la regla obligatoria de `AGENTS.md` (2026-07-30) y la disciplina nº1 de la flota:

- **Sincronización: SÍ.** `git fetch` + `git pull` al inicio. HEAD evaluado: **`0d46c4d`** (`docs: sync STATUS.md HEAD to eeeed9d post merges`), 2026-09-28 23:03 +0200.
- **OpenShell es posterior a la fecha de corte de conocimiento del evaluador (mayo 2026).** Todo lo que sigue proviene de la documentación pública leída el 2026-09-29: el blog técnico de NVIDIA, el `README.md` del repo y la página de overview de los docs. **No se ha leído el código fuente de OpenShell, no se ha instalado, y no se ha medido nada.** Ninguna afirmación de este ADR sobre su comportamiento real está verificada por ejecución.
- **Ninguna cifra de rendimiento de OpenShell se cita**, porque no se ha medido ninguna. Los bloqueantes de §4 son de arquitectura y de requisitos declarados, no de medición.

Este ADR es por tanto un **veredicto de aplicabilidad**, no un spike. Si la flota decide explorar §5, el spike correspondiente tendrá que medir por su cuenta.

---

## 1. Contexto y Problema

`STATUS.md` lo dice sin rodeos en su sección de no-producción: *"Kernel is a research lab — executes real code on your machine."* Esa frase es el problema de seguridad estructural de Tylluan, y el historial del proyecto lo confirma con incidentes reales:

- **D25** (`132d2d2`): el allowlist de `bash_execute` validaba solo el primer token de `shlex` mientras la cadena cruda iba a un shell real. Bypass trivial por encadenado (`;`, `&&`, `|`, backticks, `$()`). Encontrado por auditoría externa. Mitigado con `_SHELL_METACHAR_PATTERN`, una regex de metacaracteres sobre segmentos no citados.
- **RCE no autenticado por P2P** (`4674f84`, verificación real de peer en `ebbc998`): el listener despachaba `reg.call_tool(guild, tool)` — bash, git, filesystem, docker — sin comprobar la pubkey del iniciador.
- **`ExecutionGuard`** decide por canal (`trusted`/`untrusted`) y nivel de riesgo, pero **corre dentro del mismo proceso que vigila**: mismo dominio de confianza que el código vigilado.

En ese contexto, NVIDIA publica **OpenShell 0.1.0** (2026), descrito como *"the safe, private runtime for autonomous AI agents"* — un runtime de código abierto para definir y aplicar a qué sistemas y datos puede acceder un agente. Este ADR evalúa si Tylluan debe adoptarlo, y si lo necesita.

---

## 2. Qué es OpenShell (según su documentación)

Tres componentes:

| Componente | Función |
| :--- | :--- |
| **Gateway** | Gestiona ciclos de vida y políticas de múltiples sandboxes |
| **Supervisor** | Corre **fuera** del workload del agente; inspecciona peticiones salientes contra política |
| **Sandbox** | Ejecuta el workload con controles de filesystem y procesos **a nivel de kernel** |

Mecanismos declarados:

- Restricciones de filesystem y procesos a nivel de kernel
- Inspección de red a nivel **HTTP, GraphQL y MCP**
- **Sustitución de credenciales fuera del workload** del agente
- Actualización dinámica de políticas sin reiniciar el sandbox (políticas de red)
- **Verificación formal de políticas** con lógica OPA/Rego
- Trazas de auditoría en formato **OCSF**

Datos relevantes: **Rust**, **Apache 2.0**, estado **alpha**. Soporta Docker, Podman, MicroVM y Kubernetes (este último experimental). GPU opcional y experimental (requiere drivers NVIDIA + Container Toolkit en el host); la imagen de sandbox por defecto no lleva soporte GPU. Linux, macOS (Apple Silicon) y Windows con WSL 2 (experimental).

---

## 3. Por qué es relevante para Tylluan

No es un runtime de inferencia y no compite con `ort`/`fastembed`. Cae exactamente encima de la capa de seguridad que Tylluan ha construido a mano, y la resuelve de forma estructural donde Tylluan la resuelve de forma artesanal:

| Problema de Tylluan | Cómo lo aborda Tylluan hoy | Cómo lo abordaría OpenShell |
| :--- | :--- | :--- |
| Ejecución arbitraria vía `bash_execute` | regex de metacaracteres en el guild (D25) | controles de proceso a nivel de kernel — la clase entera del bug deja de ser expresable |
| Guard en el mismo proceso que vigila | `ExecutionGuard` dentro del kernel | Supervisor fuera del workload |
| Política de acceso | ACL por guild + `RiskLevel` en código Rust | política como datos, verificable formalmente (OPA/Rego) |
| Secretos en manos del guild | el guild recibe y maneja el token | sustitución de credenciales fuera del workload |
| Auditoría | `guild_audit_log` con chain hash propio | OCSF (formato estándar de la industria) |

La conclusión de esta tabla **no** es "hay que adoptarlo". Es que el problema que OpenShell resuelve es real en Tylluan, y que el enfoque de Tylluan es más frágil por construcción.

---

## 4. Bloqueantes para adoptarlo como dependencia — 🔴 NO-GO

### 4.1 Destruye el diferenciador declarado (bloqueante duro)

OpenShell exige **Docker, Podman o virtualización MicroVM** como substrato. `SPEC.md` define la soberanía de Tylluan sobre exactamente lo contrario: ejecución en Raspberry Pi 4 / ARM64, PCs de una década, memoria USB, **sin Docker ni cloud obligatorio**. El perfil `portable` corre con BM25 puro en ~30 MB de RAM y 0 MB de modelos.

Adoptar OpenShell como dependencia no le añade seguridad a Tylluan: le **retira el único terreno donde no tiene competencia**. El análisis de ecosistema del propio proyecto identificó el "cerebro portátil soberano" como el diferenciador genuino. Un requisito de contenedor lo anula.

### 4.2 ARM64 en Linux no está confirmado

La documentación declara macOS (Apple Silicon), lo que implica ARM64, pero **no menciona explícitamente ARM64 en Linux**. El target `aarch64-unknown-linux-gnu` es release de primera clase en Tylluan y job propio de CI. Sin esa confirmación, el perfil Pi 4 queda fuera por defecto.

### 4.3 Es alpha 0.1.0

Badge alpha explícito, con Kubernetes y GPU passthrough en desarrollo activo. Poner un runtime alpha de un vendor **debajo de todo el stack** es el cambio más caro y menos reversible disponible, y llega en un momento en que el proyecto arrastra 9 entradas `ABIERTO` en el registro de deuda de medición y los unwraps en rutas de red crecieron al +94% en dos meses.

### 4.4 No es medible hoy — el filtro que decide

El criterio del proyecto (§4 del marco de evaluación de deuda de medición) es: **¿qué métrica mueve, y existe fuente consultable para ella?** OpenShell movería postura de seguridad. Tylluan no tiene hoy una medición de postura de seguridad contra la que comparar un antes y un después: los 30 tests de seguridad automatizados cubren la implementación propia, no una superficie de amenaza medida.

Adoptar un runtime cuyo beneficio no se puede falsar es **el fallo de MD-1 a escala de arquitectura** en lugar de a escala de benchmark. MD-1 sigue `ABIERTO`.

### 4.5 Coste oculto: invalidación de líneas base

Cambiar el substrato de ejecución invalida todo artefacto de benchmark comiteado (`baseline_v0.9.0.json`, `longmemeval_v0.12.0.json`, la línea base de latencia). WS5 (`73c3f12`) acaba de introducir identidad canónica de sistema para que los artefactos sean reproducibles; un cambio de runtime rompería la comparabilidad justo después de haberla conseguido.

### 4.6 Tensión de soberanía

Menor que las anteriores pero no nula: Apache 2.0 es libre y compatible con MIT (requeriría añadirla al allowlist de `deny.toml`), así que no hay problema de licencia. La tensión es de **dependencia de roadmap**: la capa de ejecución de un proyecto cuya tesis es la soberanía tecnológica pasaría a depender de las decisiones de producto de un fabricante de hardware. Es una decisión de José, no técnica.

---

## 5. Lo que sí se adopta: tres patrones, cero dependencia — 🟢 GO

OpenShell es valioso para Tylluan **como especificación de referencia**, no como dependencia. Tres patrones son portables al diseño actual:

### 5.1 Supervisor fuera del workload

`ExecutionGuard` vive en el proceso del kernel. Un guard en el mismo dominio de confianza que el código vigilado es un defecto de diseño, no de implementación. Portar el patrón significaría mover la decisión de política a un proceso separado del que ejecuta los guilds — lo cual encaja con que los guilds **ya son procesos hijo** (`registry/guild_process.rs`), así que el camino existe.

### 5.2 Sustitución de credenciales fuera del workload

Conecta directamente con dos deudas ya documentadas: el fallback de token en query string (`auth.rs::extract_token`, con `warn!` añadido tras la cuarta ronda pero sin retirar) y los guilds que reciben el token del kernel. El patrón: el guild nunca ve la credencial real; el supervisor la sustituye en la petición saliente.

### 5.3 Política como datos verificables, no regex a mano

`_SHELL_METACHAR_PATTERN` es una regex sobre segmentos no citados de una cadena destinada a un shell. Funciona contra el PoC de D25 y es un buen parche, pero es la categoría de defensa que se vuelve a saltar: cualquier construcción de shell no prevista por la regex es un bypass candidato. Un modelo de política declarativa y verificable formalmente (OPA/Rego u equivalente) sustituye "he pensado en los casos malos" por "el motor demuestra qué está permitido".

**Coste/beneficio:** los tres son refactors internos, sin dependencia nueva, sin requisito de contenedor, y compatibles con el perfil `portable`. **Prioridad: por detrás de las acciones 4-8 del informe de la quinta ronda.** Ninguno cierra deuda de medición; §7 explica por qué eso importa.

---

## 6. Receta de despliegue opcional — 🟢 GO (coste ~nulo)

OpenShell inspecciona red **a nivel MCP** y declara que los agentes se integran sin reescribirse. Eso significa que **Tylluan puede correr como workload dentro de un sandbox de OpenShell**, sin ningún cambio en el kernel.

Propuesta: documentarlo en `docs/guides/` como receta opcional para quien ya tenga Docker y quiera aislamiento a nivel de kernel — perfil `server`, despliegues de equipo. Beneficios:

- Da una respuesta concreta a *"¿es seguro ejecutar esto, que ejecuta código real en mi máquina?"*, que es la objeción nº1 de adopción según `STATUS.md`.
- **Cero cambios de arquitectura, cero dependencia nueva, cero impacto en `portable`.** Quien no lo quiera, no lo instala.
- Convierte a OpenShell en complemento en lugar de competidor.

**DoD:** un `docs/guides/` con la receta verificada en vivo una vez (no basta con escribirla), y una línea en `SECURITY.md` que la referencie como opción de endurecimiento. Asignable a Deep o a José, no a un agente sin acceso a disco.

---

## 7. Lectura estratégica — la parte que importa más que la técnica

Que NVIDIA entre en este espacio con un runtime abierto en Apache 2.0 significa que **la capa de sandboxing y permisos para agentes se está comoditizando**. Es la misma dinámica que el análisis de ecosistema de la quinta ronda detectó en local-first con OpenMemory MCP.

**Consecuencia operativa: dejar de invertir en el guard propio como diferenciador.** Que `ExecutionGuard` sea suficiente y honesto sobre sus límites, y nada más. El diferenciador de Tylluan es la **memoria y la identidad persistente del agente**, no el aislamiento de ejecución — y ese es precisamente el terreno donde el proyecto **no tiene hoy una cifra vigente que publicar** (la de cabecera es de v0.12.0 / 2026-07-05, sobre 50 de 500 preguntas, y `LoCoMo` aparece 0 veces en el repo).

Cada hora invertida en reforzar la capa de ejecución compite con cerrar MD-1 y con re-correr LongMemEval sobre v0.17.0. MD-1 es lo que decide si el proyecto puede argumentar su valor ante alguien de fuera; el guard no lo es, y ahora además tiene un competidor gratuito y mejor financiado.

---

## 8. Decisión

1. **NO-GO** como dependencia del kernel, por §4.1 (bloqueante duro: destruye el perfil portable y el requisito ARM64/Pi 4) reforzado por §4.3 (alpha) y §4.4 (no falsable hoy).
2. **GO** para la receta de despliegue opcional de §6 — coste casi nulo, beneficio real de adopción.
3. **GO condicionado** para los tres patrones de §5, **detrás** de las acciones 4-8 de la quinta ronda. No se abren como trabajo hasta que MD-1 esté cerrado.
4. **Revisar este ADR** cuando OpenShell salga de alpha o confirme ARM64 en Linux. Si alguna vez publica un modo sin requisito de contenedor, §4.1 desaparece y la decisión se reevalúa entera.
5. **No** se abre spike de medición ahora. Si se abre en el futuro, mide §5.3 (política declarativa vs regex) contra el PoC de D25, que es el único punto con criterio de éxito ya existente.

---

### Línea para el índice (`ARCHITECTURE_DECISIONS.md`)

- [ADR-016 — NVIDIA OpenShell Runtime](ADR016_nvidia_openshell_runtime.md) — 2026-09-29. NO-GO como dependencia (exige contenedor/MicroVM, rompe perfil `portable` y ARM64/Pi 4); GO para receta de despliegue opcional y 3 patrones de diseño. Lectura estratégica: el sandboxing de agentes se comoditiza — el diferenciador es la memoria, no el aislamiento.

### Adenda para `docs/roadmap/ROADMAP_O3.md` — corrección de la quinta ronda

> **Corrección de la quinta ronda de auditoría externa (2026-09-29).** El propio auditor corrige un falso positivo de su informe: contó `verify_contracts.py` entre los gates huérfanos, pero `scripts/check_contracts.sh:39` ya lo invocaba en `6dd53a7`. El método solo comprobó invocaciones **directas** desde CI y `verify.sh`, no transitivas a través de otro gate. Eran **cuatro** gates huérfanos, no cinco — aunque como `check_contracts.sh` sí estaba huérfano, el efecto reportado (que `verify_contracts.py` nunca corría) era real y la conclusión del hallazgo se sostiene. MD-9 quedó **CERRADO** el 2026-09-28 (`ec56954` cablería + `9343c8d` meta-test de cobertura), con el fix de `check_docs_reality.sh` resuelto por fallback a `tylluan.example.toml` y fallo ruidoso si falta ambos — el cierre correcto, incluido el meta-test que impide la reincidencia silenciosa.
