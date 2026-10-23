# Semana de vulnerabilidades (20-23 oct 2026): que nos afecta y que se hizo

Origen: aviso de José (vídeo de midudev: PHP, Node.js, Java, Python/Rust/Go, Electron, pip, Bun). Fuentes primarias abiertas el 2026-10-24: nodejs.org (blog de seguridad), php.net (changelog y portada), oracle.com (CPU octubre), go.dev (release history), learn.microsoft.com (ciclo de vida de Windows 11), PyPI, crates.io, RustSec vía `cargo deny`, `npm audit`. Las cifras de blogs secundarios van marcadas «secundaria».

## Resumen

| Tema | Fuente primaria | ¿Nos afecta? | Estado |
|---|---|---|---|
| PHP RCE (dos críticas, corregidas en 8.5.12, 8.4.27, 8.3.36 y 8.2.35) | php.net | NO: no hay PHP en esta máquina (`php` no existe), ni en Dockerfile, ni en compose, ni en guilds | Nada que parchear |
| Node.js (10 CVE, 4 altas, Brotli RCE; fijadas 22.23.4, 24.22.1, 26.12.1) | nodejs.org | PARCIAL: Node 24.13.0 local (build del dashboard, herramientas); el kernel no usa Node | Pendiente actualizar (ver abajo) |
| Java (Oracle CPU: 55 parches, 49 remotos sin autenticación) | oracle.com | BAJO: Temurin 21.0.8 instalado, pero el kernel y los guilds no lo ejecutan | Pendiente actualizar (winget da 21.0.12) |
| eMudhra: 8 claves de CA intermedias comprometidas (Go 1.27.3, certifi 2026.10.19+, webpki-roots 1.0.10+, pip 26.2.2) | go.dev, PyPI, crates.io | SÍ en Python; NO en Rust | Ver abajo |
| Electron | releases de GitHub | NO: el escritorio usa Tauri (WebView2), no Electron | Nada |
| pip 26.2.2 | PyPI | SÍ: pip global 26.1.2 y venv 25.3 | HECHO |
| Bun | GitHub releases | BAJO: 1.3.14 instalado, última 1.4.4; no verifiqué si 1.4.4 trae el arreglo de eMudhra | Pendiente |
| Windows 11 24H2 Home/Pro fin de soporte | learn.microsoft.com | SÍ, URGENTE: esta máquina es 24H2 build 26100.9711 (última actualización posible); el soporte de 24H2 Pro termina el 2026-10-14 (según la tabla de ciclo de vida) | Pendiente: pasar a 25H2 (decisión de José: requiere reinicio) |
| Windows zero-day de octubre (Shell, evasión de Mark of the Web) | secundaria | Cubierto por KB5127648 (instalado 22/10) | Hecho por Windows Update |

## Hecho hoy (sin reiniciar nada)

1. `Cargo.lock`: `tokio` 1.52.3 -> 1.54.1 (RUSTSEC-2026-0386, `AsyncFd`; el código no usa `AsyncFd`). `cargo deny check advisories` pasó de fallar a `advisories ok`; `cargo check -p tylluan-kernel` exit 0.
2. Las dependencias TLS de Rust ya estaban parcheadas: `rustls-webpki` 0.103.16 y `webpki-roots` 1.0.10 (y la 0.26.11 depende de la 1.0.10) en `Cargo.lock`.
3. Python: pip 26.2.2 y certifi 2026.10.23 en el Python global de usuario, en `.venv` y en el Python empaquetado de los guilds (`guilds-python`, certifi antes 2026.7.22, pip 26.2.1). La salud del kernel de producción siguió respondiendo 200 durante el cambio.
4. `guilds/requirements.txt`: fijado `certifi>=2026.10.19` para que las reconstrucciones de `guilds-python` no vuelvan a un bundle viejo.
5. Dashboard: `dashboard/package.json` con `overrides` de los paquetes `@tiptap/*` a 3.32.0 (tldraw los fijaba en 3.30.1 con una alta de XSS/prototype pollution). `npm audit --omit=dev`: de 29 vulnerabilidades (2 altas) a 0.

## Pendiente: necesita a José (reinicio o ventana de mantenimiento)

- Windows 11: pasar de 24H2 a 25H2 (paquete de habilitación, reinicio). Mientras tanto la máquina no recibirá más parches de seguridad.
- Node 24.13.0 -> 24.22.1, Python 3.14.3 -> 3.14.7, Temurin 21, Git 2.55, Chrome/Edge y Docker Desktop: `winget upgrade`. No los apliqué: hay 9 procesos Node y 52 de Python vivos (Hermes, agentes y guilds de producción) y una actualización en caliente puede romperlos.
- Imagen Docker: reconstruir sobre `debian:bookworm-slim` actualizado para que `ca-certificates` traiga la corrección (secundaria: Debian 12 `20250419~deb12u2`).
- Producción sigue en `955179f`: no recibirá estos cambios hasta el próximo reinicio autorizado.

## No verificado

- CVE-2026-91403 de Java (JSSE, 9.1): solo lo cuentan blogs, no lo encontré en la página de Oracle.
- Que Microsoft haya bloqueado las intermedias de eMudhra en Windows: solo fuentes secundarias; no hay KB propio.
- Que Bun 1.4.4 traiga el arreglo de eMudhra.
- Que Windows Update ofrezca ya 25H2 en esta máquina.
