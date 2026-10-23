"""D-34: `kernel_auth` prefers the guild credential and never falls back.

The kernel now injects `TYLLUAN_GUILD_TOKEN` (agent-class, per process) instead
of the master token. If a guild credential is present, falling back to
`TYLLUAN_TOKEN` or to `.tylluan-token` would silently restore operator
authority, so the fallback must be disabled in that case.
"""

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent))

from guilds.core import kernel_auth  # noqa: E402

GUILD = "tyg_guild-credential-for-tests"
MASTER = "master-token-for-tests"


@pytest.fixture()
def token_file(monkeypatch, tmp_path):
    """Point the file fallback at a temp file instead of the repo root."""
    path = tmp_path / ".tylluan-token"
    monkeypatch.setattr(kernel_auth, "_TOKEN_FILE", path)
    monkeypatch.delenv("TYLLUAN_GUILD_TOKEN", raising=False)
    monkeypatch.delenv("TYLLUAN_TOKEN", raising=False)
    return path


def test_guild_token_wins_and_does_not_fall_back(token_file, monkeypatch):
    token_file.write_text(MASTER, encoding="utf-8")
    monkeypatch.setenv("TYLLUAN_TOKEN", MASTER)
    monkeypatch.setenv("TYLLUAN_GUILD_TOKEN", GUILD)

    assert kernel_auth.resolve_kernel_token() == GUILD


def test_master_env_used_for_manual_runs(token_file, monkeypatch):
    token_file.write_text("file-token", encoding="utf-8")
    monkeypatch.setenv("TYLLUAN_TOKEN", MASTER)

    assert kernel_auth.resolve_kernel_token() == MASTER


def test_token_file_is_the_legacy_fallback(token_file):
    token_file.write_text("file-token", encoding="utf-8")

    assert kernel_auth.resolve_kernel_token() == "file-token"


def test_no_credential_means_no_auth_header(token_file, monkeypatch):
    monkeypatch.setattr(kernel_auth, "_TOKEN_FILE", token_file.parent / "absent")

    assert kernel_auth.resolve_kernel_token() is None
    assert "Authorization" not in kernel_auth.kernel_headers()


def test_kernel_headers_use_the_guild_credential(token_file, monkeypatch):
    monkeypatch.setenv("TYLLUAN_GUILD_TOKEN", GUILD)

    headers = kernel_auth.kernel_headers({"X-Extra": "1"})
    assert headers["Authorization"] == "Bearer " + GUILD
    assert headers["Content-Type"] == "application/json"
    assert headers["X-Extra"] == "1"


def test_blank_values_are_ignored(token_file, monkeypatch):
    token_file.write_text("file-token", encoding="utf-8")
    monkeypatch.setenv("TYLLUAN_GUILD_TOKEN", "   ")
    monkeypatch.setenv("TYLLUAN_TOKEN", "")

    assert kernel_auth.resolve_kernel_token() == "file-token"
