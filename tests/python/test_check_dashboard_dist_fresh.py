"""T4028 (2026-11-13): Gate anti-regresión de frescura de `dashboard/dist`.

Verifica el script `scripts/check_dashboard_dist_fresh.sh` frente a:
1. Un `dist/` fresco cuyo `.src_tree` coincide con `HEAD:dashboard:HEAD:packages`
   y cuyo `assets/*.js` contiene el literal del código (`header-health-status`).
2. Mutación 1 (`dist` viejo por `.src_tree` desactualizado): el script FALLA (`rc != 0`).
3. Mutación 2 (`dist` viejo sin `.src_tree`, solo con `.built_from`): el script FALLA (`rc != 0`).
4. Mutación 3 (`dist` viejo con `.src_tree` falsificado al de `HEAD` pero bundle JS
   antiguo sin el literal del código): el script FALLA (`rc != 0`).
5. Mutación 4 (literal inexistente en `dashboard/src`, comprobación vacua): el script FALLA (`rc != 0`).
"""

from __future__ import annotations

import os
import pathlib
import shutil
import subprocess

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
SCRIPT_PATH = REPO_ROOT / "scripts" / "check_dashboard_dist_fresh.sh"


def _find_bash() -> str:
    if os.name == "nt":
        git_bin = shutil.which("git")
        if git_bin:
            git_root = pathlib.Path(git_bin).resolve().parents[1]
            for rel in (("bin", "bash.exe"), ("usr", "bin", "bash.exe")):
                cand = git_root.joinpath(*rel)
                if cand.is_file():
                    return str(cand)
    return shutil.which("bash") or "/bin/bash"


def _expected_src_tree() -> str:
    dash = subprocess.check_output(
        ["git", "-C", str(REPO_ROOT), "rev-parse", "HEAD:dashboard"],
        text=True,
    ).strip()
    pkgs = subprocess.check_output(
        ["git", "-C", str(REPO_ROOT), "rev-parse", "HEAD:packages"],
        text=True,
    ).strip()
    return f"{dash}:{pkgs}"


def _run_gate(dist_dir: pathlib.Path, extra_args: list[str] | None = None) -> subprocess.CompletedProcess[str]:
    cmd = [
        _find_bash(),
        SCRIPT_PATH.as_posix(),
        "--repo-root",
        REPO_ROOT.as_posix(),
        "--dist-dir",
        dist_dir.as_posix(),
    ]
    if extra_args:
        cmd.extend(extra_args)
    return subprocess.run(cmd, capture_output=True, text=True, check=False)


def _create_valid_dist(base_dir: pathlib.Path) -> pathlib.Path:
    dist_dir = base_dir / "dist"
    assets_dir = dist_dir / "assets"
    assets_dir.mkdir(parents=True, exist_ok=True)
    (dist_dir / "index.html").write_text("<!doctype html><html><body></body></html>\n", encoding="utf-8")
    (dist_dir / ".src_tree").write_text(_expected_src_tree() + "\n", encoding="utf-8")
    (assets_dir / "index-mock.js").write_text(
        'const el = {"data-testid": "header-health-status", label: "Solicitar Descarga de Modelo"};\n',
        encoding="utf-8",
    )
    return dist_dir


def test_t4028_fresh_dist_passes(tmp_path: pathlib.Path) -> None:
    """Caso positivo: .src_tree == HEAD:dashboard:HEAD:packages y literal presente en assets."""
    dist_dir = _create_valid_dist(tmp_path)
    res = _run_gate(dist_dir)
    assert res.returncode == 0, f"Un dist fresco debía pasar (rc=0), obtenido rc={res.returncode}:\n{res.stdout}\n{res.stderr}"
    assert "OK    .src_tree" in res.stdout
    assert "OK    literal" in res.stdout


def test_t4028_mutation_stale_src_tree_fails(tmp_path: pathlib.Path) -> None:
    """Mutación 1: .src_tree de un árbol anterior => el script DEBE fallar."""
    dist_dir = _create_valid_dist(tmp_path)
    (dist_dir / ".src_tree").write_text(
        "0000000000000000000000000000000000000000:1111111111111111111111111111111111111111\n",
        encoding="utf-8",
    )
    res = _run_gate(dist_dir)
    assert res.returncode != 0, "Un dist con .src_tree viejo debía fallar"
    assert "FALLO .src_tree" in res.stdout


def test_t4028_mutation_missing_src_tree_fails(tmp_path: pathlib.Path) -> None:
    """Mutación 2: dist sellado solo con .built_from pero sin .src_tree => DEBE fallar."""
    dist_dir = _create_valid_dist(tmp_path)
    (dist_dir / ".src_tree").unlink()
    (dist_dir / ".built_from").write_text("HEAD\n", encoding="utf-8")
    res = _run_gate(dist_dir)
    assert res.returncode != 0, "Un dist sin .src_tree debía fallar aunque tenga .built_from"
    assert "FALLO .src_tree" in res.stdout


def test_t4028_mutation_old_bundle_without_literal_fails(tmp_path: pathlib.Path) -> None:
    """Mutación 3: .src_tree coincide con HEAD pero el JS en assets/ es viejo y no tiene el literal."""
    dist_dir = _create_valid_dist(tmp_path)
    (dist_dir / "assets" / "index-mock.js").write_text(
        'console.log("bundle viejo sin el testid requerido");\n',
        encoding="utf-8",
    )
    res = _run_gate(dist_dir)
    assert res.returncode != 0, "Un dist cuyo bundle JS no contiene el literal del código debía fallar"
    assert "FALLO literal" in res.stdout


def test_t4028_mutation_vacuous_literal_rejected(tmp_path: pathlib.Path) -> None:
    """Mutación 4: pedir un literal que no existe en dashboard/src rechaza la comprobación vacua."""
    dist_dir = _create_valid_dist(tmp_path)
    res = _run_gate(dist_dir, ["--literal", "literal-inexistente-en-src-t4028"])
    assert res.returncode != 0, "Un literal que no existe en dashboard/src debía rechazarse"
    assert "FALLO literal" in res.stdout
