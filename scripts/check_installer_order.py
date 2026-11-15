"""Invariante de orden del instalador (P1c / D-170).

En la ruta por defecto el instalador arranca el kernel; si eso ocurre ANTES de
crear <DataDir>/.venv, el primer arranque de una instalacion limpia deja los
guilds Python sin dependencias. Este assert lo impide.

Uso:  python scripts/check_installer_order.py [--root <dir>]
Salida: una linea por invariante; exit 1 si alguna se rompe.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

# (fichero, marcador del bloque de guilds, marcadores que NO pueden ir antes)
CHECKS = [
    (
        "install.ps1",
        "Python guilds",
        [
            ("instalar el perfil portable", 'install --profile portable'),
            ("arrancar el kernel", "Start-Process -FilePath"),
        ],
    ),
    (
        "install.sh",
        "Python guilds",
        [
            ("instalar el perfil portable", 'install --profile portable'),
        ],
    ),
]


def line_of(text: str, needle: str) -> int | None:
    for number, line in enumerate(text.splitlines(), start=1):
        if needle in line:
            return number
    return None


def main() -> int:
    parser = argparse.ArgumentParser(description="Orden venv-antes-que-kernel en los instaladores")
    parser.add_argument("--root", default=".", help="raiz del repo (por defecto: .)")
    args = parser.parse_args()
    root = Path(args.root).resolve()

    failures = 0
    for name, guilds_marker, later_markers in CHECKS:
        path = root / name
        if not path.is_file():
            print(f"FALTA el instalador: {path}")
            failures += 1
            continue
        text = path.read_text(encoding="utf-8", errors="replace")
        guilds_line = line_of(text, guilds_marker)
        if guilds_line is None:
            print(f"{name}: no encuentro el bloque de guilds ('{guilds_marker}')")
            failures += 1
            continue
        for label, marker in later_markers:
            other = line_of(text, marker)
            if other is None:
                print(f"{name}: no encuentro el paso '{label}' ('{marker}')")
                failures += 1
                continue
            if other < guilds_line:
                print(
                    f"{name}: ORDEN MAL - '{label}' esta en la linea {other} y el venv de "
                    f"guilds en la {guilds_line}. El venv debe crearse ANTES."
                )
                failures += 1
            else:
                print(f"{name}: OK - venv (L{guilds_line}) antes de {label} (L{other})")

    if failures:
        print(f"\n{failures} invariante(s) roto(s)")
        return 1
    print("\norden OK en los dos instaladores")
    return 0


if __name__ == "__main__":
    sys.exit(main())
