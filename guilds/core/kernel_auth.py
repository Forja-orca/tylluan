"""Shared kernel auth resolution for guilds.

Single guild-side resolution path for the Bearer sent to the kernel.

History: before 2026-10-04 the kernel did not inject any credential at spawn, so
with dev_mode=false every guild call to /api/v1/* returned 401 (silently, in
most callers). The kernel then injected the MASTER token as TYLLUAN_TOKEN.

D-34 (2026-10-23) replaced that injection: a kernel-spawned guild now receives
TYLLUAN_GUILD_TOKEN, an ephemeral per-process credential with agent-class
authority only (no admin/maintenance/config/dispatch/approvals), and the kernel
no longer puts the master token in the child environment.

Resolution order:

1. TYLLUAN_GUILD_TOKEN -- kernel-spawned guild. Used, and NO fallback happens:
   falling back to the master token or to `.tylluan-token` would defeat the
   whole split.
2. TYLLUAN_TOKEN -- operator running a guild by hand with the master token.
3. `.tylluan-token` at the workspace root -- legacy/manual runs (same file the
   kernel reads at startup).
4. None -> send no auth header (a dev_mode=true kernel ignores auth anyway).

Never log or print the resolved token. The token file is gitignored and is in
guilds/core/_security.py SKIP_FILES so guild file operations never touch it.
"""

import os
from pathlib import Path

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_TOKEN_FILE = _REPO_ROOT / ".tylluan-token"
_GUILD_TOKEN_ENV = "TYLLUAN_GUILD_TOKEN"


def resolve_kernel_token():
    """Return the kernel bearer token or None.

    Priority: guild credential (no fallback), operator env token, token file.
    """
    guild_token = os.environ.get(_GUILD_TOKEN_ENV, "").strip()
    if guild_token:
        return guild_token
    token = os.environ.get("TYLLUAN_TOKEN", "").strip()
    if token:
        return token
    try:
        token = _TOKEN_FILE.read_text(encoding="utf-8").strip()
    except OSError:
        return None
    return token or None


def kernel_headers(extra=None):
    """Headers for kernel HTTP calls: Content-Type plus Authorization when a
    token is resolvable. `extra` (dict) is merged on top and wins."""
    headers = {"Content-Type": "application/json"}
    token = resolve_kernel_token()
    if token:
        headers["Authorization"] = "Bearer " + token
    if extra:
        headers.update(extra)
    return headers
