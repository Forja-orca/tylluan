"""B-24 (2026-11-01): path traversal / hostile path shapes in the guilds that
touch the filesystem (`filesystem`, `code`, `bash`).

No kernel required — pure functions plus the real guild tool entry points,
same pattern as `test_guild_security_fixes.py`.

Shapes covered per the B-24 order: `..`, absolute paths outside the root,
symlinks that escape, Windows reserved device names, and UNC paths.

Finding fixed alongside this file: `guilds/builders/plugins/code.py::code_parse`
read any path handed to it (`utils.safe_read_file(file_path)`) with NO
`validate_path` call, unlike `filesystem.py`. So `tylluan_do(code_parse,
path="../../.tylluan-token")` — or any absolute path outside the project root —
returned the file's content. The guard below closes the read path; the other
`code.py` tools that enumerate a directory are listed in the B-24 report as
still containment-unchecked.
"""
import asyncio
import os
import pathlib
import sys

import pytest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent.parent))

from guilds.builders.plugins.utils import validate_path


ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
SECRET = "B24-CANARY-8f3a1c-DO-NOT-READ"


def _outside_file(tmp_path: pathlib.Path) -> pathlib.Path:
    """A file that lives OUTSIDE the directory used as the guild root."""
    root = tmp_path / "root"
    root.mkdir()
    outside = tmp_path / "outside" / "secret.txt"
    outside.parent.mkdir()
    outside.write_text(SECRET, encoding="utf-8")
    return root, outside


# ─── validate_path: the containment primitive ───────────────────────────────

def test_dotdot_escape_rejected(tmp_path):
    root, _ = _outside_file(tmp_path)
    assert validate_path(str(root), str(root / ".." / "outside" / "secret.txt")) is False


def test_nested_dotdot_that_stays_inside_is_allowed(tmp_path):
    root, _ = _outside_file(tmp_path)
    (root / "sub").mkdir()
    assert validate_path(str(root), str(root / "sub" / ".." / "file.txt")) is True


def test_absolute_path_outside_rejected(tmp_path):
    root, outside = _outside_file(tmp_path)
    assert validate_path(str(root), str(outside)) is False


def test_sibling_with_shared_name_prefix_rejected(tmp_path):
    """`is_relative_to` compares path components, not string prefixes:
    `/tmp/x/root-evil` must not count as inside `/tmp/x/root`."""
    root = tmp_path / "root"
    evil = tmp_path / "root-evil"
    root.mkdir()
    evil.mkdir()
    (evil / "f.txt").write_text("x")
    assert validate_path(str(root), str(evil / "f.txt")) is False


def test_symlink_escaping_the_root_rejected(tmp_path):
    root, outside = _outside_file(tmp_path)
    link = root / "link.txt"
    try:
        os.symlink(outside, link)
    except (OSError, NotImplementedError):
        pytest.skip("symlink creation not permitted on this host (Windows without developer mode)")
    assert validate_path(str(root), str(link)) is False


@pytest.mark.parametrize(
    "unc",
    [r"\\attacker.example\share\secret.txt", "//attacker.example/share/secret.txt"],
)
def test_unc_paths_rejected(tmp_path, unc):
    root, _ = _outside_file(tmp_path)
    assert validate_path(str(root), unc) is False


def test_windows_reserved_device_name_is_containment_checked_not_name_checked(tmp_path):
    """KNOWN LIMITATION, asserted so it cannot drift silently: the guard is
    containment-only. `CON`, `NUL`, `COM1` inside the root are allowed
    through, and a *write* to them lands on the device instead of a file.
    Fixing that is a separate change (name deny-list), recorded in the B-24
    report rather than smuggled into this one."""
    root, _ = _outside_file(tmp_path)
    assert validate_path(str(root), str(root / "CON")) is True


# ─── filesystem guild: the guard is wired ───────────────────────────────────

def test_filesystem_file_read_blocks_traversal(tmp_path, monkeypatch):
    root, outside = _outside_file(tmp_path)
    monkeypatch.setenv("TYLLUAN_ROOT", str(root))
    monkeypatch.setattr(
        "guilds.builders.plugins.filesystem._FS_ROOT", str(root), raising=False
    )
    from guilds.builders.plugins.filesystem import file_read

    out = asyncio.run(file_read(path=str(outside)))
    assert "BLOCKED" in out, out
    assert SECRET not in out, "traversal read leaked the canary"


# ─── code guild: the guard was missing ─────────────────────────────────────

def test_code_parse_blocks_traversal_read(tmp_path, monkeypatch):
    root, outside = _outside_file(tmp_path)
    monkeypatch.setenv("TYLLUAN_ROOT", str(root))
    from guilds.builders.plugins.code import code_parse

    out = asyncio.run(code_parse(file_path=str(outside)))
    assert SECRET not in out, f"code_parse read a file outside the project root: {out[:200]}"
    assert "BLOCKED" in out, out


def test_code_parse_still_reads_inside_the_root(tmp_path, monkeypatch):
    """Negative control: a guard that blocks everything would pass the test
    above while breaking the guild."""
    root, _ = _outside_file(tmp_path)
    inside = root / "mod.py"
    inside.write_text("def f():\n    return 1\n", encoding="utf-8")
    monkeypatch.setenv("TYLLUAN_ROOT", str(root))
    from guilds.builders.plugins.code import code_parse

    out = asyncio.run(code_parse(file_path=str(inside)))
    assert "BLOCKED" not in out, out
    assert "f" in out, out


# ─── bash guild: cwd containment ────────────────────────────────────────────

def test_bash_execute_blocks_cwd_outside_root(tmp_path, monkeypatch):
    root, _ = _outside_file(tmp_path)
    monkeypatch.setenv("TYLLUAN_ROOT", str(root))
    from guilds.builders.plugins.bash import bash_execute

    out = asyncio.run(bash_execute(command="git status", cwd=str(tmp_path / "outside")))
    assert "BLOCKED" in out, out
