"""Regression test: run_command() must not let a child process block on
inherited stdin. Found living the real flow (2026-07-26) -- `git status`/
`git --version` timed out via bash_execute while `echo` (never reads stdin)
worked instantly. Root cause: asyncio.create_subprocess_exec() without
stdin=DEVNULL inherits the guild process's stdin, so a child that reads it
(credential prompt, pager) blocks until the timeout kills it.
"""
import sys
import pathlib
import asyncio
import time

import pytest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent.parent))

from guilds.builders.plugins.utils import run_command


@pytest.mark.asyncio
async def test_child_reading_stdin_does_not_hang():
    # Before the fix, sys.stdin.read() blocks until EOF -- which never
    # arrives from an inherited stdin -- and the call only returns once the
    # timeout below kills the process. With stdin=DEVNULL, read() sees EOF
    # immediately and the call completes in well under a second.
    start = time.monotonic()
    returncode, stdout, _stderr = await run_command(
        [sys.executable, "-c", "import sys; data = sys.stdin.read(); print(len(data))"],
        timeout_secs=10,
    )
    elapsed = time.monotonic() - start

    assert returncode == 0
    assert stdout.strip() == "0"
    assert elapsed < 5, f"stdin read blocked the child for {elapsed:.1f}s -- inherited stdin not closed"
