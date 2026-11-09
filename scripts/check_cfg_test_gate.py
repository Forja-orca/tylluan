#!/usr/bin/env python3
"""REGLA 13 (TL, 2026-11-09): ningun `#[test]` puede estar envuelto en
`#[cfg(unix)]` / `#[cfg(windows)]` / `#[cfg(target_os = ...)]`.

Por que existe (incidente repetido dos veces):
  `#[cfg(unix)] #[test] fn x()` **solo existe en Linux**. El recuento de tests
  de Windows no lo ve, `check_test_count.sh` compara README/AGENTS contra la
  realidad de la plataforma que corre el gate, y el CI se pone rojo por una
  diferencia que nadie puede reproducir en local. Paso con
  `file_is_written_0600_on_unix` (push 299, arreglado en 849f6a1e).

Regla: **el test se compila en todos los SO**; lo que se gatea es el CUERPO
(el bloque que solo tiene sentido en un SO), no la funcion entera:

    #[test]                        // bien
    fn permisos_0600() {
        #[cfg(unix)]
        { /* asserts POSIX */ }
        #[cfg(windows)]
        { /* o un skip explicito */ }
    }

    #[cfg(unix)]                   // MAL: el test desaparece en Windows
    #[test]
    fn permisos_0600() { ... }

    #[cfg(unix)]                   // MAL tambien: se lleva TODOS los tests del modulo
    #[cfg(test)]
    mod tests { ... }

Uso:
    python scripts/check_cfg_test_gate.py            # arbol actual
    python scripts/check_cfg_test_gate.py --self-test  # mutacion + control

Exit 0 = limpio, 1 = violacion (o self-test roto).
"""
from __future__ import annotations

import re
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

# Atributo cfg que ata el test a un SO (o familia). `cfg(all(test, unix))`
# tambien cuenta: es el mismo efecto por otra puerta.
CFG_OS_RE = re.compile(
    r"#\[\s*cfg\s*\((?P<body>[^\]]*)\)\s*\]",
    re.IGNORECASE,
)
OS_TOKENS = ("unix", "windows", "target_os", "target_family", "target_vendor")

TEST_ATTR_RE = re.compile(r"^#\[\s*(test|tokio::test|rstest|test_case)\b")
MOD_RE = re.compile(r"^(?:pub\s+)?mod\s+(?P<name>\w+)")
CFG_TEST_RE = re.compile(r"#\[\s*cfg\s*\(\s*test\s*\)\s*\]")



def _is_os_cfg(line: str) -> bool:
    match = CFG_OS_RE.search(line)
    if not match:
        return False
    body = match.group("body").lower()
    return any(token in body for token in OS_TOKENS)


def scan_text(text: str, rel: str = "<memoria>") -> list[str]:
    """Devuelve violaciones como 'fichero:linea: ...'."""
    violations: list[str] = []
    lines = text.splitlines()
    for idx, line in enumerate(lines):
        stripped = line.strip()
        if not TEST_ATTR_RE.match(stripped):
            continue
        # Subir por el bloque de atributos/comentarios inmediatamente anterior.
        j = idx - 1
        while j >= 0:
            prev = lines[j].strip()
            if not prev:
                # Una linea en blanco rompe el bloque de atributos.
                break
            if prev.startswith("//"):
                j -= 1
                continue
            if prev.startswith("#["):
                if _is_os_cfg(prev):
                    violations.append(
                        f"{rel}:{j + 1}: `{prev}` gatea el `#[test]` de la linea {idx + 1} "
                        "(REGLA 13: gatea el CUERPO, no la funcion)"
                    )
                j -= 1
                continue
            break

    # Segundo paso: un MODULO de tests gateado por SO es la misma trampa por
    # arriba (se lleva todos sus `#[test]` de una vez).
    for idx, line in enumerate(lines):
        match = MOD_RE.match(line.strip())
        if not match:
            continue
        module_name = match.group("name")
        attributes: list[str] = []
        j = idx - 1
        while j >= 0:
            prev = lines[j].strip()
            if not prev:
                break
            if prev.startswith("//"):
                j -= 1
                continue
            if prev.startswith("#["):
                attributes.append(prev)
                j -= 1
                continue
            break
        if not attributes:
            continue
        os_gated = any(_is_os_cfg(attr) for attr in attributes)
        test_module = any(CFG_TEST_RE.search(attr) for attr in attributes) or (
            module_name == "tests" or module_name.startswith("tests_")
        )
        if os_gated and test_module:
            violations.append(
                f"{rel}:{idx + 1}: modulo `mod {module_name}` de tests gateado por SO "
                "(REGLA 13: gatea el cuerpo de cada test, no el modulo)"
            )
    return violations


def scan_repo(root: Path) -> list[str]:
    violations: list[str] = []
    for path in sorted(root.rglob("*.rs")):
        rel = path.relative_to(root).as_posix()
        if "/target/" in f"/{rel}" or rel.startswith("target/"):
            continue
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        violations.extend(scan_text(text, rel))
    return violations


def self_test() -> int:
    """Mutacion: un fixture que viola tiene que salir ROJO, y el control VERDE."""
    violating = """
#[cfg(test)]
mod tests {
    #[cfg(unix)]
    #[test]
    fn solo_en_linux() {
        assert!(true);
    }
}
"""
    clean = """
#[cfg(test)]
mod tests {
    #[test]
    fn corre_en_todos_los_so() {
        #[cfg(unix)]
        { assert!(true); }
        #[cfg(windows)]
        { assert!(true); }
    }
}
"""
    problems = 0

    module_gate = """
#[cfg(unix)]
#[cfg(test)]
mod tests {
    #[test]
    fn invisible_en_windows() {
        assert!(true);
    }
}
"""
    found = scan_text(violating, "fixture_violador.rs")
    if len(found) != 1:
        print(f"SELF-TEST ROTO: el fixture violador dio {len(found)} hallazgos (esperaba 1)")
        problems += 1

    if len(scan_text(module_gate, "fixture_modulo.rs")) != 1:
        print("SELF-TEST ROTO: un modulo de tests gateado por SO no se detecto")
        problems += 1

    if scan_text(clean, "fixture_limpio.rs"):
        print("SELF-TEST ROTO: el fixture limpio dio hallazgos (falso positivo)")
        problems += 1

    # Y un barrido real sobre un directorio temporal: mismo contrato que el repo.
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        (root / "malo.rs").write_text(violating, encoding="utf-8")
        if not scan_repo(root):
            print("SELF-TEST ROTO: scan_repo no vio el fichero violador")
            problems += 1
        (root / "malo.rs").write_text(clean, encoding="utf-8")
        if scan_repo(root):
            print("SELF-TEST ROTO: scan_repo marco el fichero limpio")
            problems += 1

    if problems:
        return 1
    print("SELF-TEST OK: el violador sale rojo (1 hallazgo) y el control verde")
    return 0


def main(argv: list[str]) -> int:
    if "--self-test" in argv:
        return self_test()

    root = REPO_ROOT
    if "--root" in argv:
        root = Path(argv[argv.index("--root") + 1])

    violations = scan_repo(root)
    if violations:
        print(f"REGLA 13 VIOLADA en {len(violations)} sitio(s):")
        for violation in violations:
            print(f"  {violation}")
        print(
            "\nArreglo: deja el `#[test]` compilando en todos los SO y gatea solo "
            "el cuerpo con `#[cfg(unix)] { ... }` / `#[cfg(windows)] { ... }`."
        )
        return 1

    print("REGLA 13 OK: ningun `#[test]` esta gateado por SO")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
