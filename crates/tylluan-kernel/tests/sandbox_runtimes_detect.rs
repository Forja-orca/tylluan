//! T-DSH-35 — sonda reproducible de runtimes de contenedor/sandbox (Windows).
//!
//! Acompaña al informe `docs/internal/IMPL_T-DSH-35_runtimes_sandbox_windows_2026-10-23.md`:
//! reproduce la tabla runtime × presente × versión del entorno donde se corra.
//! NO arranca contenedores, NO descarga imágenes y NO asume que algo esté
//! instalado: si un binario no existe, la fila queda `presente=false` y el test
//! pasa igual (es una sonda, no un gate de entorno). La única aserción es de
//! coherencia: si el comando arranca, produce algún texto.
//!
//! Run:
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test sandbox_runtimes_detect -- --nocapture
//!
//! Sin `cfg(unix)`/`cfg(windows)` en ninguna función y sin tocar `src/`.

use std::process::Command;

/// Resultado de una sonda: ¿existe el binario/fichero? y ¿qué dijo?
struct Probe {
    name: &'static str,
    /// Ruta del ejecutable si se encontró (None = no presente).
    found: Option<String>,
    /// Primera línea de salida (versión o error).
    detail: String,
}

/// `wsl.exe` escribe en UTF-16LE; el resto en UTF-8. Detecta NULs y decodifica.
fn decode(bytes: &[u8]) -> String {
    if bytes.contains(&0) && bytes.len().is_multiple_of(2) {
        let mut units: Vec<u16> = Vec::with_capacity(bytes.len() / 2);
        let mut i = 0;
        while i + 1 < bytes.len() {
            units.push(u16::from_le_bytes([bytes[i], bytes[i + 1]]));
            i += 2;
        }
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).to_string()
    }
}

fn probe_command(name: &'static str, program: &str, args: &[&str]) -> Probe {
    match Command::new(program).args(args).output() {
        Ok(out) => {
            let mut text = decode(&out.stdout).trim().to_string();
            if text.is_empty() {
                text = decode(&out.stderr).trim().to_string();
            }
            let first = text.lines().next().unwrap_or("").trim().to_string();
            Probe {
                name,
                found: Some(program.to_string()),
                detail: if first.is_empty() {
                    format!("exit={:?} sin salida", out.status.code())
                } else {
                    first
                },
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Probe {
            name,
            found: None,
            detail: "NO PRESENTE".to_string(),
        },
        Err(e) => Probe {
            name,
            found: Some(program.to_string()),
            detail: format!("no ejecutable: {e}"),
        },
    }
}

fn probe_file(name: &'static str, path: &str) -> Probe {
    let exists = std::path::Path::new(path).exists();
    Probe {
        name,
        found: if exists { Some(path.to_string()) } else { None },
        detail: if exists { "fichero presente".to_string() } else { "ausente".to_string() },
    }
}

#[test]
fn sandbox_runtimes_probe_is_reproducible() {
    let probes = [
        probe_command("docker (CLI)", "docker", &["--version"]),
        probe_command("podman", "podman", &["--version"]),
        probe_command("wsl", "wsl", &["--version"]),
        probe_command("nerdctl", "nerdctl", &["--version"]),
        probe_file("WindowsSandbox.exe", r"C:\Windows\System32\WindowsSandbox.exe"),
        probe_file(
            "WindowsSandboxClient.exe",
            r"C:\Windows\System32\WindowsSandboxClient.exe",
        ),
    ];

    println!("runtime                          presente  detalle");
    for p in &probes {
        println!(
            "{:<32} {:<8} {}",
            p.name,
            if p.found.is_some() { "SI" } else { "no" },
            p.detail
        );
    }

    // Coherencia: si el binario arrancó, la sonda tiene texto que lo describe.
    for p in &probes {
        assert!(
            !p.detail.is_empty(),
            "la sonda de '{}' no produjo detalle",
            p.name
        );
    }

    // Nota de utilidad (estática, no asertada porque depende del host):
    // - N1 (ADR-022 §4.3) en Windows = WSL2 + bwrap/seccomp dentro de la distro;
    //   esta sonda no arranca la distro para no alterar el entorno.
    // - N2 = contenedor sin privilegios (Podman o Docker), que en Windows exige
    //   VM (WSL2/Hyper-V); la sonda solo mide presencia, no arranque real.
    println!(
        "nota: N1/N2 requieren prueba de arranque real (no se hizo aquí: sin contenedores ni descargas)"
    );
}
