# T-DSH-35 — Runtimes de contenedor/sandbox disponibles en Windows (dato para F0 del ADR-022)

- **Agente:** dsh
- **Fecha:** 2026-10-22/23
- **Base:** `E:\tylluan-wt-dsh` en `--detach origin/main` (`a3ae79b`, push 107); limpio antes de empezar.
- **Encargo (turno 1554):** solo lectura + un test nuevo. Medir en mi Windows, **sin instalar nada y sin arrancar contenedores ni descargar imágenes**, qué runtimes de contenedor/sandbox existen (`docker --version`, `podman --version`, `wsl -l -v`, Windows Sandbox vía `Get-WindowsOptionalFeature -Online -FeatureName Containers-DisposableClientVM`, Hyper-V), versión de cada uno y si responde. Tabular runtime × presente × versión × útil para N1/N2 del ADR-022. Informe con `## Impact` y no verificado; diff sin commitear.

## Entorno medido

Windows **10 Pro, 24H2, build 26100.9457, AMD64** (`HKLM:\…\CurrentVersion`; el `ProductName` de registro dice "Windows 10" aunque el build 26100 es 24H2). Todo lo de abajo es de **mi** máquina, no del runner de CI.

## Tabla 1 — runtimes: presente / versión / ¿responde?

| runtime | presente | versión | ¿responde? | evidencia |
|---|---|---|---|---|
| **Docker CLI** | **SÍ** | `Docker version 29.6.1, build 8900f1d` | **NO**: el motor no está levantado | `docker --version` OK; `docker info` → `failed to connect … npipe:////./pipe/dockerDesktopLinuxEngine … the daemon isn't running`; `docker version` → Client 29.6.1 / **Server vacío** |
| **Docker Desktop** (app) | **SÍ** | `4.81.0.232925` | proceso **no** en ejecución; servicio `com.docker.service` **Running**; `AutoStart=false` en `settings-store.json` | versión del `.exe`; `Get-Service`; settings |
| Docker Compose / Desktop CLI | SÍ | `v5.3.0` / `v0.4.1` | client-side (funcionan sin motor) | `docker compose version`, `docker desktop version` |
| **Podman** | **NO** | — | — | `podman --version` → no se reconoce el comando; sin binario en PATH |
| **WSL** | **SÍ** | `WSL 2.6.1.0`, kernel `6.6.87.2-microsoft-standard-WSL2`, WSLg `1.0.66` | **SÍ** (lista distros) | `wsl --version`, `wsl --status` |
| WSL: distros | SÍ | `Ubuntu 24.04.2 LTS` (default, **VERSION 2**, Stopped) + `docker-desktop` (VERSION 2, Stopped) | ninguna arrancada de base | `wsl -l -v` (UTF-16) |
| Dentro de Ubuntu (sondas read-only) | SÍ | `unshare` presente (`/usr/bin/unshare`); **`bwrap` NO**; `podman` NO; `docker` CLI accesible por `/mnt/c/...` | comandos `command -v`/`uname` ejecutados, sin instalar nada | `wsl -d Ubuntu -e sh -c ...` |
| **Windows Sandbox** | **NO habilitado** | — | — | `Win32_OptionalFeature Containers-DisposableClientVM` → **InstallState=2 (Disabled)**; `C:\Windows\System32\WindowsSandbox.exe` y `WindowsSandboxClient.exe` **ausentes** |
| **Hyper-V** | **SÍ (habilitado)** | — | hipervisor en ejecución | `Win32_OptionalFeature Microsoft-Hyper-V-All` → **InstallState=1 (Enabled)**; `HypervisorPresent=True`; `systeminfo` → "Se detectó un hipervisor" |
| nerdctl / bwrap (Windows) | NO | — | — | no presentes |

Notas de método: `Get-WindowsOptionalFeature -Online` **exige elevación** (devolvió "La operación solicitada requiere elevación"), así que el estado de los features se obtuvo de `Win32_OptionalFeature` (CIM, sin admin) + presencia/ausencia de los `.exe`. No se arrancaron contenedores, no se descargaron imágenes y no se instaló nada; la única distro que se arrancó fue Ubuntu (WSL) para ejecutar `command -v`/`uname` de solo lectura.

## Tabla 2 — utilidad para la escalera del ADR-022 (§4.3)

| nivel | mecanismo en Windows (ADR) | qué hay en esta máquina | ¿útil hoy? | coste/nota |
|---|---|---|---|---|
| **N0** | nativo | — | — | es el modo actual |
| **N1** | primitivas del SO vía **WSL2** (`bwrap` + `seccomp`) | WSL2 **SÍ** (Ubuntu 24.04.2, kernel 6.6.87.2), `unshare` SÍ, **`bwrap` NO** | **parcial**: falta `bwrap` (no se instaló: la instrucción era no instalar) | bajo, sin VM nueva: sería `apt install bubblewrap` dentro de Ubuntu; `seccomp` va en el kernel (WSL2 lo trae) |
| **N2** | contenedor sin privilegios (Podman o Docker; en Windows requiere VM) | Docker Desktop **presente pero motor apagado**; Podman **ausente**; Hyper-V **habilitado** (base de VM); WSL2 **presente**; Windows Sandbox **deshabilitado** | **sí, tras arrancar Docker Desktop** (no lo arranqué); Podman exigiría instalarlo | medio; el ADR dice "runtime configurable" — hoy solo hay un camino (Docker Desktop), que el ADR declara **no** dependencia de producto |
| **N3** | microVM (Firecracker, KVM) | no medido (sin KVM/Firecracker en Windows; no se instaló) | **no verificado** | alto |

## Lectura para F0 del ADR-022

1. **N1 es viable por WSL2 con un solo paquete** (`bubblewrap` dentro de Ubuntu; `unshare` ya está). No hay que montar VM nueva: WSL2 ya está. Falta medir el coste real (arranque/memoria) con nuestra carga — **no medido aquí** porque implicaría ejecutar procesos de sandbox (fuera del encargo).
2. **N2 hoy depende de Docker Desktop** en esta máquina (Podman no está; Windows Sandbox deshabilitado; `nerdctl` ausente). El ADR exige "nunca depender de Docker Desktop"; para cumplirlo habría que instalar Podman (o usar `docker` CLI contra otro motor), y luego medir arranque/memoria. Con el motor apagado, `docker` no responde: cualquier flujo N2 exige el humano arrancando Docker Desktop (`AutoStart=false`).
3. **Hyper-V está habilitado** (hipervisor en ejecución), lo que es la base de la VM que necesitaría Podman en Windows — pero no hay Podman, así que hoy no se usa para N2.
4. **Windows Sandbox no es una opción** hoy (feature deshabilitado; además no es el mecanismo del ADR, que apunta a contenedores/microVM).
5. **Dato que falta para la decisión**: el ADR dice que los números de arranque y memoria de cada backend "se miden con nuestra carga real antes de fijar el backend por defecto". Eso **no se ha medido**: ni un contenedor arrancado, ni una imagen descargada (por instrucción expresa). Es el siguiente dato necesario, y requiere que un humano arranque Docker Desktop (o instale Podman).

## Test nuevo (reproducible)

`crates/tylluan-kernel/tests/sandbox_runtimes_detect.rs`: sonda el entorno con `std::process::Command` (`docker --version`, `podman --version`, `wsl --version`, `nerdctl --version`, presencia de `WindowsSandbox*.exe`), decodifica UTF-16LE de `wsl.exe` y **pasa en cualquier host**: los binarios ausentes salen `no` y no hay gates de entorno (la única aserción es de coherencia: si el comando arranca, produce texto). Salida en mi máquina:

```text
runtime                          presente  detalle
docker (CLI)                     SI       Docker version 29.6.1, build 8900f1d
podman                           no       NO PRESENTE
wsl                              SI       Versión de WSL: 2.6.1.0
nerdctl                          no       NO PRESENTE
WindowsSandbox.exe               no       ausente
WindowsSandboxClient.exe         no       ausente
```

## Verificación

| criterio | resultado |
|---|---|
| `cargo test -p tylluan-kernel --test sandbox_runtimes_detect -- --nocapture` | **1 passed / 0 failed** (0,28 s) |
| `cargo clippy -p tylluan-kernel --all-targets -j 3 -- -D warnings` | **exit 0** |
| `git status` | **solo** los dos ficheros nuevos (test + este informe); `src/` intacto |
| `check_test_count.sh` | exit 0 (1264 sin cambios: test de integración) |
| `check_doc_links.py` / `check_docs_reality.sh` | links 0; reality solo el hallazgo preexistente de `ROADMAP_O3.md` (artefacto WSL) |

Sin `cfg(unix)`/`cfg(windows)` en ninguna función.

## Impact

- **Solo lectura + test nuevo**: cero cambios en `src/`, guilds, configuración o corpus; no se arrancó ningún contenedor, no se descargó ninguna imagen, no se instaló nada; no se tocó el kernel `:47004` ni `E:\tylluan`.
- **Para F0/ADR-022**: el dato que faltaba queda tabulado: **hay WSL2 (N1 casi listo, falta `bwrap`) y Docker Desktop (N2 utilizable solo arrancándolo), no hay Podman, no hay Windows Sandbox habilitado y Hyper-V sí está**. La conclusión operativa es que hoy N2 depende de Docker Desktop (lo que el ADR quiere evitar) y que la medición de arranque/memoria con carga real sigue pendiente y necesita un humano que arranque el motor.

## No verificado

- **Estado de features con `Get-WindowsOptionalFeature`**: requiere elevación (falló); usé `Win32_OptionalFeature` (CIM) + presencia de `.exe`. Es evidencia sólida pero no la consulta oficial elevada.
- **Docker "responde"**: medido desde mi shell con el motor apagado; **no arranqué Docker Desktop** (habría sido iniciar un proceso de motor, y el encargo prohíbe arrancar contenedores; arrancar la app podría considerarse fuera de alcance). Un `docker info` tras arrancarlo podría funcionar.
- **N1 dentro de WSL**: ejecuté `command -v`/`uname` (solo lectura) en Ubuntu; **no** probé `bwrap` (no está instalado) ni `unshare` para un sandbox real, ni medí arranque/memoria.
- **N2/N3 reales**: no arranqué contenedores ni microVMs; no hay medición de arranque, memoria, overhead ni de "nuestra carga real" (cargo build + guilda Python) que el ADR exige antes de fijar backend por defecto.
- **Podman en Windows**: no instalado; no verifiqué si `podman machine` funcionaría con el Hyper-V/WSL2 presente.
- **Hyper-V**: `InstallState=1` vía CIM y `HypervisorPresent=True`; no confirmé con DISM elevado ni qué features hijas están activas.
- **CI**: todo es de mi máquina (Windows 10/24H2); no del runner de CI ni de ARM64.
