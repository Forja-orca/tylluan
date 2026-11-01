"""B-30 (2026-11-01): the two findings B-24 left open.

B24-H2 — `validate_path` was containment-only, so a name that addresses a Windows
device instead of a file passed the check. `root/NUL` is inside `root`, yet a write
to it is discarded by the device, `CON` blocks on read, and `COM1` writes to a serial
port. Win32 also strips trailing dots/spaces and ignores the extension, so `con.txt`,
`NUL ` and `aux.` are the same device — and `nul:stream` is an NTFS alternate stream
of it.

B24-H3 — `code_stats`, `code_todos` and `code_security_scan` walked a directory
without `validate_path`, so `tylluan_do(code_stats, path="E:/")` counted unrelated
trees and `code_todos`/`code_security_scan` read files outside the project root.

Both are checked through the real guild entry points, same pattern as
`test_b24_guild_path_traversal.py`.
"""
import asyncio
import pathlib
import sys

import pytest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent.parent))

from guilds.builders.plugins.utils import is_windows_device_path, validate_path


ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
SECRET = "B30-CANARY-4c1d92-DO-NOT-READ"


def _roots(tmp_path: pathlib.Path):
    root = tmp_path / "root"
    root.mkdir()
    outside = tmp_path / "outside"
    outside.mkdir()
    (outside / "leak.py").write_text(
        f'API_KEY = "{SECRET}"\n# TODO: canary marker outside the root\n',
        encoding="utf-8",
    )
    return root, outside


# ─── B24-H2: device names are not files ─────────────────────────────────────

@pytest.mark.parametrize(
    "name",
    [
        "NUL",
        "nul",
        "CON",
        "CON.txt",
        "con.txt:stream",
        "AUX.",
        "PRN ",
        "COM1",
        "lpt9.log",
        "sub/NUL",
        "sub\\COM3",
        "CONOUT$",
        "conin$",
    ],
)
def test_windows_device_name_inside_the_root_is_rejected(tmp_path, name):
    root, _ = _roots(tmp_path)
    target = root / name
    assert is_windows_device_path(str(target)) is True, target
    assert validate_path(str(root), str(target)) is False, target


@pytest.mark.parametrize(
    "name",
    [
        "console.py",
        "concatenate.md",
        "nul_utils.py",
        "aux_data.json",
        "COM10",
        "printer.log",
        "copy.txt",
        "records:notes",
    ],
)
def test_names_that_only_look_like_devices_are_allowed(tmp_path, name):
    """Negative control: a deny-list that matches on substrings would pass the test
    above while blocking everyday filenames."""
    root, _ = _roots(tmp_path)
    (root / "sub").mkdir(exist_ok=True)
    target = root / name
    assert is_windows_device_path(str(target)) is False, target
    assert validate_path(str(root), str(target)) is True, target


def test_containment_and_device_checks_are_independent(tmp_path):
    """A device name outside the root stays rejected (containment), and a plain name
    inside stays allowed: neither check may shadow the other."""
    root, outside = _roots(tmp_path)
    assert validate_path(str(root), str(outside / "NUL")) is False
    assert validate_path(str(root), str(outside / "plain.txt")) is False
    assert validate_path(str(root), str(root / "plain.txt")) is True


# ─── B24-H3: the enumerating code_* tools are gated ─────────────────────────

def _code_tools():
    from guilds.builders.plugins.code import (
        code_security_scan,
        code_stats,
        code_todos,
    )

    return (code_stats, code_todos, code_security_scan)


@pytest.mark.parametrize("tool_name", ["code_stats", "code_todos", "code_security_scan"])
def test_directory_tools_block_a_path_outside_the_root(tmp_path, monkeypatch, tool_name):
    root, outside = _roots(tmp_path)
    monkeypatch.setenv("TYLLUAN_ROOT", str(root))
    tool = dict(zip(("code_stats", "code_todos", "code_security_scan"), _code_tools()))[tool_name]

    out = asyncio.run(tool(path=str(outside)))
    assert "BLOCKED" in out, f"{tool_name} enumerated a directory outside the root: {out[:200]}"
    assert SECRET not in out, f"{tool_name} read a file outside the root: {out[:200]}"
    assert "canary marker" not in out, f"{tool_name} read a file outside the root: {out[:200]}"


@pytest.mark.parametrize("tool_name", ["code_stats", "code_todos", "code_security_scan"])
def test_directory_tools_still_work_inside_the_root(tmp_path, monkeypatch, tool_name):
    """Negative control: blocking everything would pass the test above while breaking
    the guild. The file carries a TODO and a hardcoded-key shape on purpose, so each
    tool has something real to report about it."""
    root, _ = _roots(tmp_path)
    (root / "mod.py").write_text(
        'API_KEY = "A1b2C3d4E5f6G7h8I9j0K1l2"\n# TODO: inside the root\ndef f():\n    return 1\n',
        encoding="utf-8",
    )
    monkeypatch.setenv("TYLLUAN_ROOT", str(root))
    tool = dict(zip(("code_stats", "code_todos", "code_security_scan"), _code_tools()))[tool_name]

    out = asyncio.run(tool(path=str(root)))
    assert "BLOCKED" not in out, out
    assert "Scan failed" not in out, out
    assert "mod.py" in out or "Python" in out, out


def test_file_dispatch_still_goes_through_code_parse_guard(tmp_path, monkeypatch):
    """A file outside the root handed to a directory tool takes the code_parse branch;
    that guard (B-24) must still be the one that answers."""
    root, outside = _roots(tmp_path)
    monkeypatch.setenv("TYLLUAN_ROOT", str(root))
    code_stats, _, _ = _code_tools()

    out = asyncio.run(code_stats(path=str(outside / "leak.py")))
    assert "BLOCKED" in out, out
    assert SECRET not in out, out
