"""Unit tests for the 2 CRITICAL security fixes from the external audit of
v0.13.0: bash.py's command allowlist and filesystem.py's path validation.

No kernel required -- these test the pure functions directly, same pattern
as test_injection_guard.py.
"""
import sys
import pathlib
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent.parent))

from guilds.builders.plugins.bash import _check_allowlist, ALLOWED_COMMANDS
from guilds.builders.plugins.utils import validate_path


# ─── bash.py allowlist ──────────────────────────────────────────────────────

def test_allowed_command_passes():
    assert _check_allowlist("git status") is None


def test_disallowed_command_blocked():
    result = _check_allowlist("rm -rf /")
    assert result is not None
    assert "BLOCKED" in result


def test_disallowed_command_with_wildcard_still_blocked():
    # This is the exact bypass the audit found in the old regex blocklist --
    # `rm -rf /*` slipped past `rm\s+(-rf?|--recursive)\s+/\s*$` because of
    # the wildcard. The allowlist checks the first token only, so it isn't
    # vulnerable to that specific bypass shape at all: `rm` simply isn't
    # in ALLOWED_COMMANDS, regardless of what follows it.
    result = _check_allowlist("rm -rf /*")
    assert result is not None
    assert "rm" not in ALLOWED_COMMANDS


def test_command_injection_via_first_token_blocked():
    result = _check_allowlist("$(which mkfs) /dev/sda1")
    assert result is not None


def test_empty_command_blocked():
    result = _check_allowlist("")
    assert result is not None


# ─── filesystem.py path validation ─────────────────────────────────────────

def test_path_inside_root_is_valid(tmp_path):
    sub = tmp_path / "sub" / "file.txt"
    sub.parent.mkdir(parents=True)
    sub.write_text("x")
    assert validate_path(str(tmp_path), str(sub)) is True


def test_path_outside_root_is_rejected(tmp_path):
    outside = tmp_path.parent / "outside_secret.txt"
    assert validate_path(str(tmp_path), str(outside)) is False


def test_path_traversal_rejected(tmp_path):
    traversal = str(tmp_path / ".." / ".." / "etc" / "shadow")
    assert validate_path(str(tmp_path), traversal) is False


def test_root_itself_is_valid(tmp_path):
    assert validate_path(str(tmp_path), str(tmp_path)) is True
