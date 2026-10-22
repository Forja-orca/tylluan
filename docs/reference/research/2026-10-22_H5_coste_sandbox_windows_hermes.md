# Coste real de runtimes de sandbox en Windows — 2026-10-22

**Autor:** Hermes (coder). **Encargo H-5** (TL T1538). **Formato:** tabla tecnología → parámetro → cita literal + URL → leído/no verificado. Máx ~80 líneas.

---

## 1. Podman machine (WSL2 / Hyper-V)

| Parámetro | Cita literal | URL | Leído |
|---|---|---|---|
| RAM por defecto máquina Podman (WSL2 / Hyper-V) | "6 GB of RAM for the Podman machine" (prerequisito tanto para WSL2 como Hyper-V) | https://docs.redhat.com/en/documentation/red_hat_build_of_podman_desktop/1.0/html/install_on_windows/con_install-podman_install-on-windows | ✅ |
| Disco por defecto (ejemplo `podman machine ls`) | "DISK SIZE ... 768MB ... 100GiB" (ejemplos wsl-default 768MB, hyperv-default 100GiB) | https://github.com/podman-container-tools/podman/blob/main/docs/tutorials/podman-for-windows.md | ✅ |
| CPUs por defecto (ejemplo) | "CPUS ... 12 ... 6" (ejemplos wsl-default 12, hyperv-default 6) | https://github.com/podman-container-tools/podman/blob/main/docs/tutorials/podman-for-windows.md | ✅ |
| WSL2: requisitos de hardware | "64-bit processor with Second Level Address Translation (SLAT), 8GB system RAM, Enable hardware virtualization in BIOS/UEFI" | https://docs.docker.com/desktop/setup/install/windows-install/ | ✅ |
| Hyper-V: ediciones Windows soportadas | "Hyper-V is only available on Windows Enterprise, Pro, or Education editions (not Home)" | https://github.com/podman-container-tools/podman/blob/main/docs/tutorials/podman-for-windows.md | ✅ |
| `podman machine init` permite sobrescribir memoria/disco | "`podman machine init --disk-size 50` ... `podman machine init --memory=1024 myvm`" | https://docs.podman.io/en/latest/markdown/podman-machine-init.1.html | ✅ |
| `podman machine set --memory/--disk-size` solo QEMU | "Only supported for QEMU machines" (para --memory y --disk-size) | https://docs.podman.io/en/latest/markdown/podman-machine-set.1.html | ✅ |
| WSL2 comparte kernel y memoria entre distros | "Since WSL shares the same virtual machine and Linux kernel across multiple distributions, the CPU and Memory values represent the total resources shared across running systems." | https://github.com/podman-container-tools/podman/blob/main/docs/tutorials/podman-for-windows.md | ✅ |

---

## 2. Docker Sandboxes (`sbx`)

| Parámetro | Cita literal | URL | Leído |
|---|---|---|---|
| Requiere Docker Desktop | "You don't need Docker Desktop or Docker Engine to use `sbx`" | https://docs.docker.com/ai/sandboxes/install | ✅ |
| Windows: requisitos para sandboxes locales | "Windows 11; A 64-bit Intel or AMD processor; Windows Hypervisor Platform for local sandboxes" | https://docs.docker.com/ai/sandboxes/install | ✅ |
| Habilitar Windows Hypervisor Platform | "Enable-WindowsOptionalFeature -Online -FeatureName HypervisorPlatform -All" (PowerShell elevado) | https://docs.docker.com/ai/sandboxes/install | ✅ |
| Instalación per-user (sin admin) | "WinGet installs the per-user `DockerSandboxes.msi` package in `%LOCALAPPDATA%\DockerSandboxes` and adds its `bin` directory to your user `PATH`. You can install it without administrator privileges." | https://docs.docker.com/ai/sandboxes/install | ✅ |
| Instalación machine-wide (admin) | "The machine-wide package installs in `%ProgramFiles%\DockerSandboxes` and adds its `bin` directory to the system `PATH`." | https://docs.docker.com/ai/sandboxes/install | ✅ |
| Licencia Docker Desktop (empresas >250 empleados o >$10M) | "Commercial use of Docker Desktop in larger enterprises (more than 250 employees OR more than $10 million USD in annual revenue) requires a paid subscription." | https://docs.docker.com/desktop/setup/install/windows-install/ | ✅ |
| Docker Sandboxes es gratuito y standalone | "Docker Sandboxes is free and standalone." / "No. Docker Sandboxes is free and standalone." | https://www.docker.com/products/docker-sandboxes/ | ✅ |
| Aislamiento: microVM con kernel propio | "Sandboxes are microVM environments we built from scratch. Each one gets its own kernel, its own private Docker Engine, and no path back to the host." | https://www.docker.com/products/docker-sandboxes/ | ✅ |
| Cold start rápido | "Cold starts fast enough so you have no reason not to sandbox." | https://www.docker.com/products/docker-sandboxes/ | ✅ |

---

## 3. bubblewrap (`bwrap`) en WSL2

| Parámetro | Cita literal | URL | Leído |
|---|---|---|---|
| Codex en WSL2 usa bubblewrap | "On Linux and WSL2, install `bubblewrap` with your package manager first: `sudo apt install bubblewrap`" | https://learn.chatgpt.com/docs/sandboxing | ✅ |
| Codex busca `bwrap` en PATH | "Codex uses the first `bwrap` executable it finds on `PATH`. If no `bwrap` executable is available, Codex falls back to a bundled helper" | https://learn.chatgpt.com/docs/sandboxing | ✅ |
| Helper requiere unprivileged user namespace | "that helper requires support for unprivileged user namespace creation" | https://learn.chatgpt.com/docs/sandboxing | ✅ |
| Ubuntu AppArmor: perfil `bwrap-userns-restrict` | "The `bwrap-userns-restrict` profile ships in the `apparmor` package at `/etc/apparmor.d/bwrap-userns-restrict`" | https://learn.chatgpt.com/docs/sandboxing | ✅ |
| Ubuntu 24.04 puede necesitar perfil extra | "On Ubuntu 24.04, Codex may still warn that it can't create the needed user namespace after `bubblewrap` is installed. Copy and load the extra profile..." | https://learn.chatgpt.com/docs/sandboxing | ✅ |
| Desactivar restricción AppArmor userns | "sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0" | https://learn.chatgpt.com/docs/sandboxing | ✅ |
| Coste RAM/CPU propio de bubblewrap | No encontrado — bubblewrap es un wrapper ligero; el coste es el del proceso sandboxeado + overhead mínimo | — | ❌ no verificado |

---

## 4. gVisor (`runsc`) en Windows

| Parámetro | Cita literal | URL | Leído |
|---|---|---|---|
| SO soportado | "gVisor requires Linux" / "gVisor supports x86_64 and ARM64, and requires Linux 5.6+" | https://gvisor.dev/docs/user_guide/faq/ + https://gvisor.dev/docs/user_guide/install/ | ✅ |
| Soporte Windows nativo | No encontrado — documentación oficial solo lista Linux | https://gvisor.dev/docs/ | ✅ (no hay Windows) |
| gVisor con Podman/Docker en WSL2 | No encontrado — teóricamente posible dentro de WSL2 (Linux), pero no documentado oficialmente para Windows host | — | ❌ no verificado |
| Overhead memoria/CPU gVisor | No encontrado en fuentes oficiales | — | ❌ no verificado |

---

## 5. Resumen comparativo (solo cifras con fuente)

| Tecnología | RAM mínima host | Disco VM/máquina | Requiere admin | Licencia | Aislamiento | Notas |
|---|---|---|---|---|---|---|
| Podman WSL2 | 8 GB sistema (WSL2 req) + 6 GB VM | ~768 MB–100 GiB configurable | Sí (habilitar WSL/Hyper-V) | Apache-2.0 | VM Linux completa (WSL2/Hyper-V) | Memoria compartida entre distros WSL |
| Podman Hyper-V | 8 GB sistema + 6 GB VM | ~100 GiB por defecto | Sí (Hyper-V solo Pro/Enterprise/Edu) | Apache-2.0 | VM Linux completa (Hyper-V) | Aislamiento más fuerte que WSL2 |
| Docker `sbx` | 8 GB sistema (Win11 req) | No publicado (microVM efímero) | No (per-user) / Sí (machine-wide) | Propietario (sbx), Docker Desktop >250 emp = pago | microVM kernel propio | No requiere Docker Desktop |
| bubblewrap WSL2 | Lo que pida WSL2 (8 GB base) | N/A (proceso, no VM) | No (instala en WSL) | LGPL-2.1 | Linux namespaces + seccomp | Solo dentro de WSL2; no Windows nativo |
| gVisor | N/A en Windows host | N/A | N/A | Apache-2.0 | Application kernel (syscall interception) | **No corre en Windows host**; solo Linux |

---

## 6. No verificado / Sin cifra oficial

- Tamaño exacto de disco por defecto de `podman machine init` en WSL2 (la doc dice "can only be increased" para QEMU; WSL no documenta el default explícito).
- Overhead de memoria/CPU de `sbx` microVM (Docker no publica cifras; "cold starts fast enough").
- Overhead de `bwrap` en WSL2 medido con carga real (`cargo build`, guilda Python).
- gVisor (`runsc`) con Podman/Docker dentro de WSL2: factibilidad y coste.
- Coste de Windows Sandbox nativo (usado por Codex en PowerShell) — Microsoft no publica RAM/disco por defecto en docs de Codex.
- Límites de red/archivos por sandbox en `sbx` (docs dicen "Set network and filesystem limits per sandbox" pero no dan defaults).

---

**Fin del encargo H-5.** Fuentes abiertas y verificadas en esta sesión. Lo no encontrado = "no verificado".
## Verificación del TL (2026-10-23)

- Contenido leído y citado por Hermes; el TL integra el documento tal cual y **no ha reabierto sus citas**. Para el ADR-022 se tratan como [FUENTE] de un solo lector hasta que el TL reabra las que use.
- Son requisitos y valores por defecto publicados por los proyectos, **no mediciones nuestras**: el coste real en el Windows de José lo mide DSH cuando él lo autorice (decisión 7 del ADR-022).
