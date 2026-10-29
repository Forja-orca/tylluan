<p align="center">
  <img src="assets/branding/logo.jpg" alt="Tylluan" width="140">
</p>

<h1 align="center">Tylluan</h1>

<p align="center">
  <strong>Memoria local, un grafo de conocimiento y ejecución de herramientas protegida para agentes de IA.</strong><br>
  Un solo binario de kernel. Funciona en tu máquina. Compatible con clientes MCP.
</p>

<p align="center">
  <a href="https://github.com/forja-orca/tylluan/actions/workflows/ci.yml"><img src="https://github.com/forja-orca/tylluan/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-green.svg" alt="Licencia MIT"></a>
  <img src="https://img.shields.io/badge/version-0.17.2-blue.svg" alt="v0.17.2">
  <img src="https://img.shields.io/badge/MCP-native-purple.svg" alt="MCP nativo">
</p>

<p align="center">
  <a href="README.md">English</a> · <strong>Español</strong> · <a href="README.zh.md">中文</a> <sub>(pendiente de revisión nativa)</sub>
</p>

---

Tylluan es un servidor local al que se conectan tus herramientas de IA por [MCP](https://modelcontextprotocol.io) o por REST. Les da tres cosas que no tienen por sí solas:

- **Memoria que sobrevive a la sesión.** Se guarda en un fichero SQLite de tu disco y se busca por palabras clave y, con un perfil de embeddings, por similitud vectorial y un grafo de conocimiento.
- **Herramientas que se ejecutan de verdad.** Shell, git, ficheros, Docker, búsqueda web, bases de datos y más, a través de [45 guilds](guilds/README.md) («gremios», programas de herramientas independientes). `tylluan_do` tiene un modo de plan (simulación), y las llamadas hechas por las herramientas MCP quedan auditadas.
- **Un lugar compartido para varios agentes.** Cada cliente que conectas lee y escribe la misma memoria y puede hablar en canales compartidos.

Tú pones el modelo y el cliente (Claude Code, Cursor, VS Code, …). Tylluan no trae ningún modelo de lenguaje propio; sus modelos de embeddings (opcionales) se ejecutan en local. No es una aplicación de chat ni un framework de agentes.

> **Software de investigación.** Tylluan ejecuta comandos reales en tu máquina y no ha pasado ninguna auditoría de seguridad independiente de terceros. Lee [Seguridad](#seguridad-qué-protege-y-qué-no) y [DISCLAIMER.md](DISCLAIMER.md) antes de poner algo sensible a su alcance.

<p align="center">
  <img src="docs/assets/how-it-fits.svg" alt="Mapa de procesos: los clientes MCP llegan al transporte HTTP de un solo proceso Rust, tylluan-nexus, en 127.0.0.1 puerto 47004; los clientes REST y A2A entran por el mismo transporte. Cada petición pasa por autenticación, confianza y permisos y después por las seis herramientas soberanas. Estas enrutan al registro de guilds, que habla con 45 procesos Python por stdio JSON-RPC, y leen y escriben en SilvaDB (SQLite con FTS5, grafo y vectores). El modelo juez corre en llama-server a través del guild llama_backend y es opcional. La federación con otros nodos Tylluan aprobados pasa por la malla con Noise." width="100%">
</p>

## Instalación

| Plataforma | Binario de la release |
|------------|-----------------------|
| Linux x86_64 | `tylluan-x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 | `tylluan-aarch64-unknown-linux-gnu.tar.gz` |
| macOS, Apple Silicon | `tylluan-aarch64-apple-darwin.tar.gz` |
| Windows x86_64 | `tylluan-x86_64-pc-windows-msvc.tar.gz` |

No hay binario para macOS Intel. Los binarios están en la [página de releases](https://github.com/forja-orca/tylluan/releases).

```bash
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/Forja-orca/tylluan/main/install.sh | sh

# Windows (PowerShell)
irm https://raw.githubusercontent.com/Forja-orca/tylluan/main/install.ps1 | iex
```

Esto deja `tylluan-nexus` (el kernel) y `tylluan-cli` en `~/.tylluan/bin/` y los añade al `PATH`. Abre una terminal nueva después.

El kernel no necesita Rust, Python ni Node. Los 45 guilds que hay detrás de `tylluan_do` son programas Python y necesitan **Python 3.12 y FastMCP** instalados aparte (`pip install -r guilds/requirements.txt`; el fichero viene en el archivo de la release y en este repositorio); sin ellos esos guilds no arrancan y `/api/v1/doctor` informa `degraded`. La memoria y la búsqueda funcionan sin Python.

Para compilar desde el código fuente, consulta [docs/getting-started/QUICKSTART.md](docs/getting-started/QUICKSTART.md).

## Arrancar y conectar

```bash
tylluan-cli start
curl -s http://127.0.0.1:47004/health
```

Ejecuta antes `tylluan-cli install`: escribe la configuración. Si arrancas sin ella, el primer arranque carga el modelo de embeddings por defecto, que en nuestra prueba tardó 52 s, más de los 30 s que espera el CLI, y entonces el CLI detiene el kernel. El kernel escucha en `127.0.0.1:47004`. La autenticación está activada por defecto: en el primer arranque crea un token en un fichero llamado `.tylluan-token` **en el directorio desde el que se arrancó el kernel**. Mantén ese fichero en privado: quien tenga el token puede ejecutar herramientas en tu máquina.

Conecta un cliente enviando el token en una cabecera `Authorization`. Para Claude Code:

```bash
claude mcp add --transport http tylluan http://127.0.0.1:47004/messages \
  --header "Authorization: Bearer $(cat .tylluan-token)"
```

En Windows PowerShell:

```powershell
claude mcp add --transport http tylluan http://127.0.0.1:47004/messages --header "Authorization: Bearer $(Get-Content .tylluan-token)"
```

Los demás clientes usan la misma URL y cabecera. Para Cursor, VS Code y Claude Desktop mira [integrations/](integrations/) para ver configuraciones de ejemplo y [docs/guides/DEV_MODE_OFF_GUIDE.md](docs/guides/DEV_MODE_OFF_GUIDE.md) para saber dónde toma el token cada cliente. Usa `127.0.0.1`, no `localhost`: en Windows `localhost` puede resolverse primero a IPv6 y no encontrar el kernel.

## Pruébalo

```bash
TOKEN=$(cat .tylluan-token)

# Guardar una memoria
curl -X POST http://127.0.0.1:47004/api/v1/memory/write \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"content": "Tylluan guarda mis notas en un fichero SQLite local."}'

# Recuperarla (el perfil `portable` por defecto busca solo palabras clave y deben aparecer todas)
curl "http://127.0.0.1:47004/api/v1/memory/search?q=notas" \
  -H "Authorization: Bearer $TOKEN"
```

Desde un cliente MCP no llamas a estos endpoints. Le pides a tu agente que recuerde o recupere algo, o que ejecute una tarea («muéstrame los últimos 10 commits de este repositorio»), y usa las herramientas de abajo.

## Las seis herramientas

Con la configuración por defecto de la rama `main` actual, cada cliente conectado ve exactamente estas seis herramientas, por muchos gremios que haya debajo. La release publicada v0.17.1 expone cinco: `tylluan_decide` llegó después de esa etiqueta.

| Herramienta | Qué hace |
|-------------|----------|
| `tylluan_do` | Ejecuta una tarea descrita en lenguaje natural enrutándola a un gremio |
| `tylluan_remember` | Guarda conocimiento |
| `tylluan_recall` | Busca en la memoria a largo plazo (palabras clave; vectores y grafo con un perfil de embeddings) |
| `tylluan_think` | Razona sobre el grafo de conocimiento |
| `tylluan_graph` | Operaciones directas sobre el grafo: tripletas, caminos, PageRank |
| `tylluan_decide` | Responde una pregunta acotada (elección, puntuación o sí/no) o se abstiene. Hoy aplica reglas deterministas y registra cada decisión; por defecto no tiene ningún modelo conectado |

<p align="center">
  <img src="docs/assets/memory-flow.svg" alt="Escritura: tylluan_remember pasa el filtro L1 de patrones de inyección, se vectoriza con mxbai-embed-large (1024 dimensiones, ONNX) y se guarda en SilvaDB; un juez L2 asíncrono clasifica el nodo nuevo como SAFE o SUSPICIOUS y marca la cuarentena. Recuperación: tylluan_recall elige un nivel (rápido, equilibrado o profundo), ejecuta FTS5 BM25 más coseno fusionados con RRF con k igual a 60, y la puerta de coherencia (L1 patrones, L2 procedencia, L3 coseno por debajo de 0,85, L4 juez opcional) filtra los candidatos antes de devolver memorias ordenadas y la cabecera de estadísticas." width="100%">
</p>

## Perfiles

Un binario de kernel, tres configuraciones que cambian calidad de recuperación por consumo.

| Perfil | Embeddings | Descarga extra de modelo | Medido |
|--------|-----------|----------|--------|
| `portable` (perfil de instalación por defecto) | ninguno, solo búsqueda por palabras clave BM25 | 0 MB | tiempo de arranque en Windows y memoria, abajo |
| `clinic` | BGE-Small, 384 dimensiones | unos 67 MB | sin medir |
| `server` | mxbai-embed-large, 1024 dimensiones | sin verificar | sin medir |

`portable` se midió el 2026-10-26 con la release publicada v0.17.1 de Windows x86_64 (una descarga de 15,4 MB), en cuatro arranques en frío con la base de datos vacía: operativo a los 12,8 a 16,8 s, de 38,7 a 40 MB residentes, y ninguna conexión de red saliente ni socket UDP no local en los 10 segundos posteriores a estar operativo. Esa máquina no estaba en reposo (había un kernel de producción y herramientas de desarrollo en marcha), así que lee el tiempo de arranque como un máximo. El script es [scripts/measure_portable_start.ps1](scripts/measure_portable_start.ps1); la máquina y las ejecuciones en bruto están en [portable_start_2026-10-26.json](docs/reference/measurements/portable_start_2026-10-26.json). Linux, macOS, ARM y una Raspberry Pi física **no** se han medido; CI compila la versión ARM64 pero no la arranca.

Para cambiar de perfil, para primero el kernel y ejecuta `tylluan-cli install --profile server --force`. Sobrescribe `tylluan.toml` y arranca un kernel en el puerto por defecto, así que no dejes otro kernel en marcha. `tylluan-cli start --setup` puede elegir un perfil según tu RAM y GPU.

## Por qué Tylluan, y por qué no

**Por qué**

- Tu memoria es un fichero SQLite que puedes abrir con cualquier herramienta SQLite. Sin cuenta, sin clave de API, sin un servicio que mantener vivo.
- Un único binario de kernel compilado, nativo de MCP, compartido por todos los clientes que conectes.
- Local por defecto: los embeddings se calculan dentro del propio proceso con ONNX Runtime, y enrutar una tarea a un gremio usa embeddings locales (puntuación por palabras clave en el perfil `portable`), no un modelo en la nube.

**Por qué no**

- Mem0, Letta, Zep, Cognee y otros llevan años en producción y tienen comunidades más grandes. Tylluan es joven; v0.17.2 es su última release.
- No es un servicio alojado. No hay panel de equipo, ni copias de seguridad gestionadas, ni soporte. Existe un dashboard en React en `dashboard/`, pero los binarios publicados no lo incluyen: compila desde el código con `--features bundled-dashboard` para servirlo desde el kernel.
- No está endurecido para redes hostiles. Mantenlo en `127.0.0.1`.
- Varios gremios necesitan Python 3.12, y algunos (la búsqueda web, por ejemplo) salen a internet cuando se lo pides.

**Calidad de recuperación.** En LongMemEval-S (50 preguntas) con el modelo BGE-M3, Tylluan obtuvo Recall@5 82 %, Recall@10 90 % y una latencia mediana de 12,9 ms en CPU ([resultados en bruto](benchmarks/longmemeval_v0.12.0.json)). Fue en v0.12.0 el 2026-07-05, antes de que el modelo por defecto pasara a `mxbai-embed-large`. No se ha repetido, es un conjunto pequeño y no es una comparación con otros sistemas.

## Seguridad: qué protege y qué no

- **Red.** Por defecto el kernel escucha solo en `127.0.0.1`. No combines nunca `host = "0.0.0.0"` con `dev_mode = true`, que sería una shell remota sin autenticación en tu red; si lo haces, el kernel registra una advertencia crítica y fuerza el host de vuelta a `127.0.0.1`.
- **Autenticación.** Activada por defecto (`dev_mode = false`). Toda llamada a la API y a MCP necesita el token, salvo una lista pública corta: la comprobación de salud, el descubrimiento, los ficheros estáticos del dashboard y cuatro endpoints bajo `/api/` (ping de federación, envío y recepción de sincronización de federación, y el sondeo MCP). La lista exacta es `is_public_bypass_path` en `crates/tylluan-kernel/src/transport/http/auth.rs`.
- **Peticiones desde páginas web.** Una página abierta en tu navegador podría hablar con el kernel en `127.0.0.1`. Desde v0.17.2 las cinco rutas MCP y SSE (`/messages`, `/mcp`, `/api/v1/mcp`, `/sse`, `/sse/download`) y toda ruta bajo `/api/v1/` que no sea GET ni HEAD rechazan con 403 las peticiones con un `Origin` o `Host` no local, y el endpoint MCP exige `Content-Type: application/json`. Si usas v0.17.1, mantén `dev_mode = false`: una página web no tiene tu token, así que no puede llamar a un kernel autenticado.
- **Envenenamiento de memoria.** Una puerta de coherencia filtra lo que devuelve `tylluan_recall`, con comprobaciones deterministas de patrones, procedencia y deriva. Un clasificador opcional basado en un modelo, para los casos ambiguos, está desactivado por defecto; si lo activas, puede poner en cuarentena o rebajar una memoria. El endpoint REST de búsqueda simple todavía no pasa por esa puerta.
- **Herramientas.** Los gremios se ejecutan como procesos normales del sistema operativo con los permisos de tu usuario. Existe un sandbox opcional de Docker (`[security.sandbox]` en `tylluan.toml`, necesita Docker) que puede aislar los gremios bash y code, o todos en su perfil `strict`. Está desactivado por defecto, así que de fábrica un gremio puede hacer todo lo que puedes hacer tú. Las llamadas a herramientas hechas a través de las herramientas MCP se escriben en un registro de auditoría encadenado por hash; el endpoint REST directo para llamar a un gremio todavía no se audita, y una escritura de auditoría que falla se registra como advertencia en lugar de detener la llamada.
- **No cubierto.** No hay auditoría de seguridad independiente de terceros. La federación está pensada para una red local o tu propia VPN, no para internet abierto.

Cómo informar de una vulnerabilidad: [SECURITY.md](SECURITY.md). Modelo de amenazas: [docs/concepts/SECURITY.md](docs/concepts/SECURITY.md).

## Federación

Dos o más instancias de Tylluan pueden sincronizar memorias con pares que apruebas uno a uno: cifrado con Noise, con el origen de cada nodo registrado y con permisos por par y por espacio que deniegan por defecto. Está pensada para una red local o una VPN privada. No se ha probado en internet abierto y ninguna release incluye un nodo de arranque público. Consulta [docs/concepts/FEDERATION.md](docs/concepts/FEDERATION.md).

## Parar, tus datos, desinstalar

- **Parar:** `tylluan-cli stop`. Con la autenticación activada necesita el token: pasa `--token`. Si hay más de un kernel en marcha, pasa también `--port` (o `--data-dir`) para parar el correcto.
- **Tus datos:** la base SQLite es `./data/tylluan.db` relativa al directorio desde el que se arrancó el kernel, con el resto de sus ficheros en `data/` allí. Haz una copia de esa carpeta con el kernel parado.
- **Desinstalar:** borra `~/.tylluan/` y quita la entrada de `PATH` que añadió el instalador. Si pierdes el token, borra `.tylluan-token` y reinicia: se genera uno nuevo y hay que dárselo otra vez a cada cliente.

## Estado

- **Última release:** v0.17.2 (2026-10-27). El trabajo posterior vive en `main`; consulta [CHANGELOG.md](CHANGELOG.md).
- **Tests:** 1512 tests entre el kernel de Rust (lib), `tylluan-link` y `tylluan-fsrs`, que CI ejecuta en cada push junto con clippy, `cargo-deny`, el lint de Python y la suite de auditoría de seguridad.
- **Qué está planificado y qué no:** [ROADMAP.md](ROADMAP.md). Estado actual de cada afirmación: [STATUS.md](STATUS.md).
- **Plataformas:** Linux x86_64 y ARM64, macOS Apple Silicon, Windows x86_64. CI ejecuta los tests en Linux x86_64 y el workflow de release construye los cuatro binarios. La medición de arranque de arriba se tomó en Windows x86_64. Las versiones de macOS y ARM64 no las han ejecutado los mantenedores en hardware (sin verificar).

## Documentación

| | |
|---|---|
| [Quickstart](docs/getting-started/QUICKSTART.md) | Configuración, autenticación, resolución de problemas, compilar desde el código |
| [Arquitectura](docs/concepts/ARCHITECTURE.md) | Cómo encajan las piezas |
| [Guilds](guilds/README.md) | Los 45 programas de herramientas detrás de `tylluan_do` |
| [Decisiones de arquitectura](docs/reference/adr/) | Por qué las cosas son como son |
| [Contribuir](CONTRIBUTING.md) | Cómo ayudar |

## Licencia

[MIT](LICENSE).
