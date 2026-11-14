#!/usr/bin/env python3
"""Gate del catálogo de descarga de modelos (orden de Jose 2026-11-10, TL T3913).

Lo que vigila: que **no vuelva** un modelo de chat al catálogo de descarga.

Historia (medida, no supuesta): `crates/tylluan-kernel/src/maintenance.rs` tenía
un `vec![...]` con 14 entradas y URL de HuggingFace, tres de ellas pesos de CHAT
(SmolLM2-135M-Instruct, Qwen2.5-1.5B-Instruct, Qwen2.5-0.5B-Instruct) que se
autodescargaban en cuanto `[nexus] allow_hf_download = true`. Tylluan no sirve
chat: el catálogo sale del manifiesto (`<models_dir>/*/model.json`, bloque
`download`) y sólo admite los roles mínimos (embedding, reranker).

Uso:
    python scripts/check_model_catalog.py                  # comprueba el repo
    python scripts/check_model_catalog.py --self-test      # mutacion: el gate TIENE que caer
    python scripts/check_model_catalog.py --root <dir>     # otro arbol

Reglas (una violación = una línea de salida, exit 1):
1. Ningún literal `https://huggingface.co...` en el código del kernel: las URLs
   viven en el manifiesto, no en Rust.
2. Ningún nombre de familia de chat como literal en los ficheros del catálogo
   (`maintenance.rs`, `models.rs`), fuera de comentarios y del módulo de tests.
"""

from __future__ import annotations

import argparse
import re
import sys
import tempfile
from pathlib import Path

# Ficheros donde vive el catálogo de descarga.
CATALOG_FILES = [
    "crates/tylluan-kernel/src/maintenance.rs",
    "crates/tylluan-kernel/src/models.rs",
]

# Zonas donde SÍ se escanea un literal de chat (todo el kernel para URLs).
URL_SCAN_DIR = "crates/tylluan-kernel/src"

CHAT_FAMILIES = (
    "smollm",
    "qwen2.5",
    "qwen3-",
    "llama",
    "mistral",
    "gemma",
    "tinyllama",
    "phi-",
)

HF_URL = re.compile(r"https?://(?:[a-z0-9-]+\.)?huggingface\.co", re.IGNORECASE)
CFG_TEST = re.compile(r"^\s*#\[cfg\(test\)\]")
MOD_TESTS = re.compile(r"^\s*mod\s+tests\b")


def strip_code_noise(line: str) -> str:
    """Quita el comentario de una línea para no marcar prosa como código."""
    stripped = line.lstrip()
    if stripped.startswith("//") or stripped.startswith("/*") or stripped.startswith("*"):
        return ""
    return line


def scan_file(path: Path, check_chat: bool) -> list[tuple[str, int, str]]:
    """Hallazgos como (ruta, línea, motivo) para poder deduplicar sin partir rutas."""
    findings: list[tuple[str, int, str]] = []
    in_tests = False
    pending_cfg_test = False
    # errors="replace": un fuente que no sea UTF-8 no debe tumbar el gate
    # (un gate que revienta no protege nada); se sigue escaneando igual.
    for number, raw in enumerate(
        path.read_text(encoding="utf-8", errors="replace").splitlines(), start=1
    ):
        if CFG_TEST.match(raw):
            pending_cfg_test = True
            continue
        if MOD_TESTS.match(raw):
            if pending_cfg_test:
                in_tests = True
            pending_cfg_test = False
            continue
        if not raw.strip():
            pending_cfg_test = False
            continue
        pending_cfg_test = False
        if in_tests:
            # Los tests pueden nombrar chat a propósito (fixtures de mutación).
            continue
        code = strip_code_noise(raw)
        if not code:
            continue
        if HF_URL.search(code):
            findings.append(
                (
                    str(path),
                    number,
                    "URL de HuggingFace en el código — las URLs van en el manifiesto",
                )
            )
            continue
        if check_chat and '"' in code:
            lowered = code.lower()
            hit = next((fam for fam in CHAT_FAMILIES if fam in lowered), None)
            if hit:
                findings.append(
                    (
                        str(path),
                        number,
                        f"familia de chat '{hit}' en el catálogo de descarga "
                        "(Tylluan no sirve chat)",
                    )
                )
    return findings


def scan(root: Path) -> list[str]:
    raw: list[tuple[str, int, str]] = []
    url_root = root / URL_SCAN_DIR
    if url_root.is_dir():
        for path in sorted(url_root.rglob("*.rs")):
            raw.extend(scan_file(path, check_chat=False))
    for rel in CATALOG_FILES:
        path = root / rel
        if path.is_file():
            raw.extend(scan_file(path, check_chat=True))
    # Cada sitio cuenta una vez: el barrido de URLs y el del catálogo pueden
    # solaparse en el mismo fichero y duplicar el hallazgo.
    seen: set[tuple[str, int]] = set()
    unique: list[str] = []
    for path, line, reason in raw:
        if (path, line) in seen:
            continue
        seen.add((path, line))
        unique.append(f"{path}:{line}: {reason}")
    return unique


def run_self_test() -> int:
    """Mutación: un fichero con chat/URL de HF TIENE que producir hallazgos."""
    failures = 0
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        (root / "crates/tylluan-kernel/src").mkdir(parents=True)
        target = root / CATALOG_FILES[0]
        target.write_text(
            "\n".join(
                [
                    "// Comentario que menciona huggingface.co: NO cuenta.",
                    "pub fn registry() -> Vec<ModelMeta> {",
                    '    let chat = "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct";',
                    '    let nombre = "SmolLM2-135M-Instruct";',
                    "    vec![]",
                    "}",
                    "#[cfg(test)]",
                    "mod tests {",
                    '    const FIXTURE: &str = "https://huggingface.co/fixture/smollm";',
                    "}",
                ]
            ),
            encoding="utf-8",
        )
        findings = scan(root)
        # Una linea con URL gana al chequeo de familia (un hallazgo por linea),
        # asi que 2: la URL de la linea 3 y el nombre de chat de la 4. La URL
        # del fixture de tests (linea 9) NO cuenta.
        if len(findings) != 2:
            print(f"SELF-TEST FALLIDO: esperaba 2 hallazgos, hay {len(findings)}")
            for finding in findings:
                print(f"  {finding}")
            failures += 1
        else:
            print("self-test OK: URL de HF y nombre de chat detectados; fixture de tests ignorado")

        # Control negativo: mismo fichero sin las lineas malas -> 0 hallazgos.
        target.write_text(
            "\n".join(
                [
                    "pub fn registry() -> Vec<ModelMeta> {",
                    "    // las URLs viven en el manifiesto",
                    "    vec![]",
                    "}",
                    "#[cfg(test)]",
                    "mod tests {",
                    '    const FIXTURE: &str = "SmolLM2-135M-Instruct";',
                    "}",
                ]
            ),
            encoding="utf-8",
        )
        if scan(root):
            print("SELF-TEST FALLIDO: el fichero limpio no debe dar hallazgos")
            failures += 1
        else:
            print("self-test OK: fichero limpio -> 0 hallazgos")
    return 0 if failures == 0 else 1


def main() -> int:
    parser = argparse.ArgumentParser(description="Gate del catálogo de descarga de modelos")
    parser.add_argument("--root", default=".", help="raíz del repo (por defecto: .)")
    parser.add_argument("--self-test", action="store_true", help="mutación: el gate tiene que caer")
    args = parser.parse_args()

    if args.self_test:
        return run_self_test()

    root = Path(args.root).resolve()
    findings = scan(root)
    if findings:
        print(f"CATALOGO DE DESCARGA VIOLADO en {len(findings)} sitio(s):")
        for finding in findings:
            print(f"  {finding}")
        print()
        print("Arreglo: la URL y el tamaño van en `<models_dir>/<id>/model.json` (bloque")
        print("`download`), y el rol debe ser `embedding` o `reranker`. Nada de chat.")
        return 1

    print(f"CATALOGO DE DESCARGA OK: sin URLs de HF en Rust y sin familias de chat ({len(CHAT_FAMILIES)} vigilancias)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
