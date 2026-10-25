"""Shared bearer-token resolution for operator scripts.

Resolution order (same as the kernel's own self-call token):

1. ``TYLLUAN_TOKEN`` environment variable.
2. ``.tylluan-token`` at the repository root (gitignored, never tracked).

When no token exists the helpers return ``None`` / headers without
``Authorization``, so a ``dev_mode = true`` kernel keeps working exactly as
before. The token is never logged or persisted by this module.
"""

from __future__ import annotations

import os
from pathlib import Path
from typing import Optional

ENV_VAR = "TYLLUAN_TOKEN"
REPO_ROOT = Path(__file__).resolve().parent.parent
TOKEN_FILE = REPO_ROOT / ".tylluan-token"


def resolve_token(
    env: Optional[dict] = None,
    token_file: Optional[Path] = None,
) -> Optional[str]:
    """Return the kernel bearer token, or ``None`` when there is none.

    ``env`` / ``token_file`` are injectable for unit tests; production callers
    use the process environment and the repo-root token file.
    """
    source_env = os.environ if env is None else env
    token = (source_env.get(ENV_VAR) or "").strip()
    if token:
        return token
    path = TOKEN_FILE if token_file is None else token_file
    try:
        token = path.read_text(encoding="utf-8").strip()
    except OSError:
        return None
    return token or None


def kernel_headers(
    extra: Optional[dict] = None,
    env: Optional[dict] = None,
    token_file: Optional[Path] = None,
) -> dict:
    """HTTP headers for kernel calls: ``extra`` plus ``Authorization`` if a
    token is resolvable. Without a token the result is exactly ``extra`` (no
    auth header), preserving the current ``dev_mode = true`` behaviour."""
    headers = dict(extra or {})
    token = resolve_token(env=env, token_file=token_file)
    if token:
        headers["Authorization"] = "Bearer " + token
    return headers
