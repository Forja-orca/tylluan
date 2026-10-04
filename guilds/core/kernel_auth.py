"""Shared kernel auth resolution for guilds (fleet token injection, 2026-10-04).

Before this module, only silva_utils.py and scholars/plugins/memory_bridge.py
sent an Authorization header to the kernel, and both depended on the
TYLLUAN_TOKEN env var being present in the guild process — which the kernel
did not set on spawn (only PYTHONPATH/PYTHONUNBUFFERED) and which is absent
in the default token-in-FILE setup. Result: with dev_mode=false every guild
call to /api/v1/* would 401 (silently, in most callers).

The kernel now injects TYLLUAN_TOKEN into every stdio guild env at spawn
(crates/tylluan-kernel/src/registry/guild_process.rs). This helper is the
single guild-side resolution path, mirroring kernel-side
resolve_self_auth_token (security/coherence_gate.rs):

1. TYLLUAN_TOKEN env (kernel-injected at spawn, or operator-set for manual
   runs),
2. `.tylluan-token` file at the workspace root (the same file the kernel
   reads on startup — config.rs / coherence_gate.rs),
3. None → send no auth header (a dev_mode=true kernel ignores auth anyway).

Never log or print the resolved token. The token file is gitignored and is
already in guilds/core/_security.py SKIP_FILES so guild file operations
never touch it.
"""

import os
from pathlib import Path

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_TOKEN_FILE = _REPO_ROOT / ".tylluan-token"


def resolve_kernel_token():
    """Return the kernel bearer token (env first, then file) or None."""
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
